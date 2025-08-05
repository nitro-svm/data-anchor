use std::{ops::ControlFlow, option::Option, sync::Arc};

use solana_client::rpc_client::SerializableTransaction;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::transaction::TransactionError;
use solana_transaction_status::{
    TransactionStatus as SolanaTransactionStatus, UiTransactionEncoding,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tracing::{trace, warn};

use crate::batch_client::{
    channels::upgrade_and_send,
    messages::{BlockMessage, ConfirmTransactionMessage, SendTransactionMessage, StatusMessage},
    transaction::TransactionStatus,
};

/// Spawns an independent task that listens for [`ConfirmTransactionMessage`]s and checks their
/// status using the Solana RPC client. Regardless of the transaction's outcome, status updates
/// will be sent as [`StatusMessage`]s with the transaction's current status.
///
/// If the transaction is confirmed at the RPC client's desired commitment level, the transaction
/// will be considered done by the task and won't be checked further.
///
/// If the transaction is not recognized by the RPC server, it will be queued for either
/// re-confirmation or re-sending depending on how many slots have passed since the transaction
/// was initially submitted.
///
/// If the transaction results in an error, it will be queued for re-sending, unless the error
/// is that the transaction has already been processed.
///
/// The task will exit if there are no transaction confirmation senders alive. This will happen when
/// the [transaction sender](`super::transaction_sender::spawn_transaction_sender`) task has exited.
///
/// The task will also exit if the [block watcher](`super::block_watcher::spawn_block_watcher`) task
/// has exited, but this is not expected to happen under normal conditions.
pub fn spawn_transaction_confirmer(
    rpc_client: Arc<RpcClient>,
    mut blockdata_rx: watch::Receiver<BlockMessage>,
    transaction_sender_tx: mpsc::WeakUnboundedSender<SendTransactionMessage>,
    transaction_confirmer_tx: mpsc::WeakUnboundedSender<ConfirmTransactionMessage>,
    mut transaction_confirmer_rx: mpsc::UnboundedReceiver<ConfirmTransactionMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let res = transaction_confirm_loop(
                &mut blockdata_rx,
                &mut transaction_confirmer_rx,
                &rpc_client,
                &transaction_sender_tx,
                &transaction_confirmer_tx,
            )
            .await;

            if res.is_break() {
                warn!(
                    "no receivers for transaction confirmations, shutting down transaction confirmer"
                );
                break;
            }
        }

        warn!("shutting down transaction confirmer");
    })
}

async fn transaction_confirm_loop(
    blockdata_rx: &mut watch::Receiver<BlockMessage>,
    transaction_confirmer_rx: &mut mpsc::UnboundedReceiver<ConfirmTransactionMessage>,
    rpc_client: &Arc<RpcClient>,
    transaction_sender_tx: &mpsc::WeakUnboundedSender<SendTransactionMessage>,
    transaction_confirmer_tx: &mpsc::WeakUnboundedSender<ConfirmTransactionMessage>,
) -> ControlFlow<()> {
    let res = blockdata_rx.changed().await;
    if res.is_err() {
        // No new blockdata will arrive, so the task should exit.
        return ControlFlow::Break(());
    }
    let blockdata = *blockdata_rx.borrow_and_update();
    let batch = get_next_batch_for_confirmation(transaction_confirmer_rx).await?;
    let Some(responses) =
        get_transaction_statuses(rpc_client, transaction_confirmer_tx, batch).await?
    else {
        // If transaction status retrieval fails, don't stop the loop, just keep going
        // and try again later.
        return ControlFlow::Continue(());
    };

    let TransactionResponseCategories {
        status_updates,
        resend,
        mut reconfirm,
    } = categorize_transaction_responses(responses, blockdata.last_valid_block_height);

    for (status, logs, msg) in status_updates {
        let slot = status.slot;
        let status = TransactionStatus::from_solana_status(status, logs, rpc_client.commitment());

        // If the transaction wasn't committed or failed, it has to be checked again.
        if status.should_be_reconfirmed() {
            reconfirm.push(msg.clone());
        }

        // If a response channel is dropped that's fine, that just means the future was dropped
        // (most likely due to timeout) and the transaction is no longer interesting.
        // Ignore the error and continue with other messages regardless.
        let _ = msg.response_tx.send(StatusMessage {
            index: msg.index,
            landed_as: Some((slot, *msg.transaction.get_signature())),
            status,
        });
    }

    upgrade_and_send(transaction_sender_tx, resend)?;
    upgrade_and_send(transaction_confirmer_tx, reconfirm)?;

    ControlFlow::Continue(())
}

#[derive(Default)]
struct TransactionResponseCategories {
    pub status_updates: Vec<(
        SolanaTransactionStatus,
        Vec<String>,
        ConfirmTransactionMessage,
    )>,
    pub resend: Vec<SendTransactionMessage>,
    pub reconfirm: Vec<ConfirmTransactionMessage>,
}

/// Categorizes the transaction status responses from the RPC client into three different types of outcomes:
/// - Status updates, regardless of good or bad.
/// - Transactions that need to be re-sent due to timeouts or errors.
/// - Transactions that need to be re-confirmed due to still being processed.
///
/// The transaction categories are not mutually exclusive.
fn categorize_transaction_responses(
    responses: impl Iterator<
        Item = (
            (Option<SolanaTransactionStatus>, Vec<String>),
            ConfirmTransactionMessage,
        ),
    >,
    last_valid_block_height: u64,
) -> TransactionResponseCategories {
    let mut categories = TransactionResponseCategories::default();
    for (status, msg) in responses {
        if msg.response_tx.is_closed() {
            // The receiver has been dropped, ignore the transaction and move on to the next.
            continue;
        }
        categorize_transaction_response(&mut categories, status, msg, last_valid_block_height);
    }
    categories
}

/// See [`categorize_transaction_responses`].
fn categorize_transaction_response(
    categories: &mut TransactionResponseCategories,
    status: (Option<SolanaTransactionStatus>, Vec<String>),
    msg: ConfirmTransactionMessage,
    last_valid_block_height: u64,
) {
    let _enter = msg.span.clone().entered();
    let logs = status.1;
    let Some(status) = status.0 else {
        // If there is no status, the transaction was not recognized by the RPC server.
        if msg.last_valid_block_height + 10 < last_valid_block_height {
            // The request was not successful within 10 slots using RPC, try again.
            categories.resend.push(msg.into());
        } else {
            // No status reported, check again later.
            categories.reconfirm.push(msg);
        }
        return;
    };

    match status.err {
        None | Some(TransactionError::AlreadyProcessed) => {
            // Either the transaction succeeded (no error) or it was already processed.
            categories.status_updates.push((status, logs, msg));
        }
        Some(ref err) => {
            // Some instructions are expected to fail, for example inserting too far ahead
            // or closing the Blober when it's not yet full.
            if !matches!(err, TransactionError::InstructionError(_, _)) {
                // Other errors are *not* expected and will be logged, but will not otherwise be
                // handled in any special way.
                warn!("unexpected transaction error: {err:?}");
            }
            // Regardless of the error type, it will be reported, re-signed and re-sent.
            categories.resend.push(SendTransactionMessage {
                span: msg.span.clone(),
                index: msg.index,
                transaction: msg.transaction.clone(),
                // Force re-sign. Since the transaction itself failed, this is safe.
                last_valid_block_height: 0,
                response_tx: msg.response_tx.clone(),
            });
            categories.status_updates.push((status, logs, msg));
        }
    }
}

/// Gets the status of a batch of transactions using the RPC client.
///
/// If the entire status request fails, the transactions will be queued for re-confirmation,
/// unless the transaction confirmer channel is closed.
async fn get_transaction_statuses(
    rpc_client: &Arc<RpcClient>,
    transaction_confirmer_tx: &mpsc::WeakUnboundedSender<ConfirmTransactionMessage>,
    batch: Vec<ConfirmTransactionMessage>,
    // The use of [`ControlFlow`] here might seem superfluous at first since the function only ever
    // returns [`ControlFlow::Continue`], but [`upgrade_and_send`] *can* return [`ControlFlow::Break`]
    // when the transaction confirmer sender could not be upgraded, breaking out of the loop.
) -> ControlFlow<
    (),
    Option<
        impl Iterator<
            Item = (
                (Option<SolanaTransactionStatus>, Vec<String>),
                ConfirmTransactionMessage,
            ),
        >,
    >,
> {
    let signatures: Vec<_> = batch
        .iter()
        .map(|msg| *msg.transaction.get_signature())
        .collect();
    let response = match rpc_client.get_signature_statuses(&signatures[..]).await {
        Ok(response) => response,
        Err(e) => {
            warn!("failed to get signatures: {e:?}");
            upgrade_and_send(transaction_confirmer_tx, batch)?;
            // The transactions were re-queued, keep the loop going.
            return ControlFlow::Continue(None);
        }
    };

    trace!(
        "got status for {} signatures",
        response.value.iter().flatten().count()
    );

    let mut all_logs = Vec::with_capacity(response.value.len());

    for (status, signature) in response.value.iter().zip(signatures.into_iter()) {
        let Some(status) = status else {
            // The RPC server didn't recognize the transaction, so it will be re-queued.
            all_logs.push(Vec::new());
            continue;
        };

        if status.err.is_none() {
            // The transaction was recognized and processed, so it will be reported.
            all_logs.push(Vec::new());
            continue;
        }

        let tx = match rpc_client
            .get_transaction(&signature, UiTransactionEncoding::Json)
            .await
        {
            Ok(tx) => tx,
            Err(e) => {
                warn!("failed to get failed transaction: {e:?}");
                all_logs.push(Vec::new());
                continue;
            }
        };

        let Some(logs) = tx.transaction.meta.map(|meta| meta.log_messages) else {
            // The transaction was recognized but not processed, so it will be re-queued.
            all_logs.push(Vec::new());
            continue;
        };

        all_logs.push(logs.unwrap_or(Vec::new()));
    }

    let responses = response
        .value
        .into_iter()
        .zip(all_logs.into_iter())
        .zip(batch.into_iter());

    ControlFlow::Continue(Some(responses))
}

/// Gets a batch of up to 256 transactions to be confirmed from the channel.
///
/// The 256 limit comes from how many transactions the [`RpcClient::get_signature_statuses`] will
/// allow for a single request, so there is no point in retrieving more than that.
pub async fn get_next_batch_for_confirmation(
    transaction_confirmer_rx: &mut mpsc::UnboundedReceiver<ConfirmTransactionMessage>,
) -> ControlFlow<(), Vec<ConfirmTransactionMessage>> {
    let mut batch = Vec::new();
    let received_messages = transaction_confirmer_rx.recv_many(&mut batch, 256).await;
    if received_messages == 0 {
        // If this is ever zero, that means the channel was closed.
        // No more transactions will ever be received, so the task should exit.
        return ControlFlow::Break(());
    }
    ControlFlow::Continue(batch)
}

#[cfg(test)]
mod tests {
    use std::{mem, sync::Mutex};

    use anchor_lang::prelude::Pubkey;
    use async_trait::async_trait;
    use solana_client::{
        client_error::ClientError,
        rpc_client::RpcClientConfig,
        rpc_request::RpcRequest,
        rpc_response::{Response, RpcResponseContext},
        rpc_sender::{RpcSender, RpcTransportStats},
    };
    use solana_keypair::Keypair;
    use solana_rpc_client::mock_sender::MockSender;
    use solana_rpc_client_api::client_error::Result as SolanaResult;
    use solana_sdk::{
        commitment_config::CommitmentConfig, hash::Hash, instruction::InstructionError,
        signer::Signer, transaction::Transaction,
    };
    use solana_transaction_status::{
        TransactionConfirmationStatus, TransactionStatus as SolanaTransactionStatus,
    };
    use tracing::{Level, Span};

    use super::*;
    use crate::batch_client::transaction::TransactionStatus;

    #[tokio::test]
    async fn test_categorize_transaction_response() {
        // Status is present as processed, no error.
        let categories = categorize_helper(
            Some(Ok(())),
            None,
            Some(TransactionConfirmationStatus::Processed),
            0,
        );
        // Should result in a status update.
        assert_eq!(categories.status_updates.len(), 1);
        assert_eq!(categories.resend.len(), 0);
        assert_eq!(categories.reconfirm.len(), 0);

        // Status is present as confirmed, no error.
        let categories = categorize_helper(
            Some(Ok(())),
            None,
            Some(TransactionConfirmationStatus::Confirmed),
            0,
        );
        // Should result in a status update.
        assert_eq!(categories.status_updates.len(), 1);
        assert_eq!(categories.resend.len(), 0);
        assert_eq!(categories.reconfirm.len(), 0);

        // Status is absent, last valid block height is equal to the transaction's last valid block height.
        let categories = categorize_helper(
            None,
            None,
            Some(TransactionConfirmationStatus::Confirmed),
            0,
        );
        // Should result in a reconfirm.
        assert_eq!(categories.status_updates.len(), 0);
        assert_eq!(categories.resend.len(), 0);
        assert_eq!(categories.reconfirm.len(), 1);

        // Status is absent, last valid block height is >100 above the transaction's last valid block height,
        // and the TPU client is enabled.
        let categories = categorize_helper(
            None,
            None,
            Some(TransactionConfirmationStatus::Confirmed),
            101,
        );
        // Should result in a resend.
        assert_eq!(categories.status_updates.len(), 0);
        assert_eq!(categories.resend.len(), 1);
        assert_eq!(categories.reconfirm.len(), 0);

        // Status is absent, last valid block height is >10 above the transaction's last valid block height,
        // and the TPU client is disabled.
        let categories = categorize_helper(
            None,
            None,
            Some(TransactionConfirmationStatus::Confirmed),
            11,
        );
        // Should result in a resend.
        assert_eq!(categories.status_updates.len(), 0);
        assert_eq!(categories.resend.len(), 1);
        assert_eq!(categories.reconfirm.len(), 0);

        // Status is present as an AlreadyProcessed error.
        let categories = categorize_helper(
            Some(Ok(())),
            Some(TransactionError::AlreadyProcessed),
            Some(TransactionConfirmationStatus::Confirmed),
            0,
        );
        // Should result in a status update.
        assert_eq!(categories.status_updates.len(), 1);
        assert_eq!(categories.resend.len(), 0);
        assert_eq!(categories.reconfirm.len(), 0);

        // Status is present as some other error.
        let categories = categorize_helper(
            Some(Ok(())),
            Some(TransactionError::AccountInUse),
            Some(TransactionConfirmationStatus::Confirmed),
            0,
        );
        // Should result in a status update and a re-send.
        assert_eq!(categories.status_updates.len(), 1);
        assert_eq!(categories.resend.len(), 1);
        assert_eq!(categories.reconfirm.len(), 0);
    }

    fn categorize_helper(
        signature_status: Option<Result<(), TransactionError>>,
        signature_err: Option<TransactionError>,
        confirmation_status: Option<TransactionConfirmationStatus>,
        last_valid_block_height: u64,
    ) -> TransactionResponseCategories {
        let (response_tx, _) = mpsc::unbounded_channel();
        let mut categories = TransactionResponseCategories::default();
        let status = signature_status.map(|status| SolanaTransactionStatus {
            slot: 0,
            confirmations: None,
            status,
            err: signature_err,
            confirmation_status,
        });
        let msg = ConfirmTransactionMessage {
            span: Span::current(),
            index: 0,
            transaction: Transaction::default(),
            last_valid_block_height: 0,
            response_tx: response_tx.clone(),
        };
        categorize_transaction_response(
            &mut categories,
            (status, Vec::new()),
            msg,
            last_valid_block_height,
        );
        categories
    }

    #[tokio::test]
    async fn test_get_transaction_statuses_success() {
        let payer = Arc::new(Keypair::new());
        let status = SolanaTransactionStatus {
            status: Ok(()),
            slot: 0,
            confirmations: None,
            err: None,
            confirmation_status: Some(TransactionConfirmationStatus::Confirmed),
        };

        let rpc_client = Arc::new(RpcClient::new_sender(
            MockSender::new_with_mocks(
                "succeeds",
                [(
                    RpcRequest::GetSignatureStatuses,
                    serde_json::to_value(Response {
                        context: RpcResponseContext {
                            slot: 1,
                            api_version: None,
                        },
                        // One status value, one None.
                        value: vec![Some(status), None],
                    })
                    .unwrap(),
                )]
                .into_iter()
                .collect(),
            ),
            RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
        ));

        let (transaction_confirmer_tx, mut transaction_confirmer_rx) = mpsc::unbounded_channel();
        let (response_tx, mut response_rx) = mpsc::unbounded_channel();

        let transaction = Transaction::new_signed_with_payer(
            &[solana_sdk::system_instruction::transfer(
                &payer.pubkey(),
                &solana_sdk::system_program::id(),
                1,
            )],
            Some(&payer.pubkey()),
            &[&payer],
            Hash::default(),
        );
        let confirmer_tx = transaction_confirmer_tx.downgrade();
        let ControlFlow::Continue(Some(messages)) = get_transaction_statuses(
            &rpc_client,
            &confirmer_tx,
            vec![
                ConfirmTransactionMessage {
                    span: Span::current(),
                    index: 0,
                    transaction: transaction.clone(),
                    last_valid_block_height: 0,
                    response_tx: response_tx.clone(),
                },
                ConfirmTransactionMessage {
                    span: Span::current(),
                    index: 1,
                    transaction,
                    last_valid_block_height: 0,
                    response_tx: response_tx.clone(),
                },
            ],
        )
        .await
        else {
            panic!("transaction statuses should be continue");
        };

        // Both messages should be returned.
        let messages: Vec<_> = messages.collect();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].1.index, 0);
        assert_eq!(
            messages[0].0.0.as_ref().unwrap().confirmation_status,
            Some(TransactionConfirmationStatus::Confirmed)
        );
        assert_eq!(messages[1].1.index, 1);
        assert_eq!(messages[1].0.0, None);

        // Nothing should have been queued for re-confirmation.
        transaction_confirmer_rx.try_recv().unwrap_err();
        // And no status updates should have been sent.
        response_rx.try_recv().unwrap_err();
    }

    #[tokio::test]
    async fn test_get_transaction_statuses_rpc_fails() {
        let payer = Arc::new(Keypair::new());
        let rpc_client = Arc::new(RpcClient::new_mock("fails".to_string()));

        let (transaction_confirmer_tx, mut transaction_confirmer_rx) = mpsc::unbounded_channel();
        let (response_tx, mut response_rx) = mpsc::unbounded_channel();

        let transaction = Transaction::new_signed_with_payer(
            &[solana_sdk::system_instruction::transfer(
                &payer.pubkey(),
                &solana_sdk::system_program::id(),
                1,
            )],
            Some(&payer.pubkey()),
            &[&payer],
            Hash::default(),
        );
        let ControlFlow::Continue(None) = get_transaction_statuses(
            &rpc_client,
            &transaction_confirmer_tx.downgrade(),
            vec![
                ConfirmTransactionMessage {
                    span: Span::current(),
                    index: 0,
                    transaction: transaction.clone(),
                    last_valid_block_height: 0,
                    response_tx: response_tx.clone(),
                },
                ConfirmTransactionMessage {
                    span: Span::current(),
                    index: 1,
                    transaction,
                    last_valid_block_height: 0,
                    response_tx: response_tx.clone(),
                },
            ],
        )
        .await
        else {
            panic!("transaction statuses should be continue");
        };

        // The messages should have been queued for re-confirmation.
        let msg_0 = transaction_confirmer_rx.recv().await.unwrap();
        let msg_1 = transaction_confirmer_rx.recv().await.unwrap();
        assert_eq!(msg_0.index, 0);
        assert_eq!(msg_1.index, 1);
        // But no status updates should have been sent.
        response_rx.try_recv().unwrap_err();
    }

    #[tokio::test]
    async fn test_get_next_batch_for_confirmation() {
        let (transaction_confirmer_tx, mut transaction_confirmer_rx) =
            mpsc::unbounded_channel::<ConfirmTransactionMessage>();
        let (response_tx, mut response_rx) = mpsc::unbounded_channel();

        // Queue 5 transactions.
        for message in generate_confirm_messages(5, &response_tx) {
            transaction_confirmer_tx.send(message.clone()).unwrap();
        }
        let ControlFlow::Continue(batch) =
            get_next_batch_for_confirmation(&mut transaction_confirmer_rx).await
        else {
            panic!("batch should be continue");
        };
        assert_eq!(batch.len(), 5);

        // Queue 300 transactions.
        for message in generate_confirm_messages(300, &response_tx) {
            transaction_confirmer_tx.send(message.clone()).unwrap();
        }
        // The first batch should contain the first 256 transactions.
        let ControlFlow::Continue(batch) =
            get_next_batch_for_confirmation(&mut transaction_confirmer_rx).await
        else {
            panic!("batch should be continue");
        };
        assert_eq!(batch.len(), 256);
        // The next batch should contain the remaining 44 transactions.
        let ControlFlow::Continue(batch) =
            get_next_batch_for_confirmation(&mut transaction_confirmer_rx).await
        else {
            panic!("batch should be continue");
        };
        assert_eq!(batch.len(), 44);

        // Drop the sender.
        drop(transaction_confirmer_tx);

        // The next batch should fail out with a ControlFlow::Break.
        let control_flow = get_next_batch_for_confirmation(&mut transaction_confirmer_rx).await;
        assert!(control_flow.is_break());

        // The response channel shouldn't have been touched throughout all the above.
        response_rx.try_recv().unwrap_err();
    }

    fn generate_confirm_messages(
        amount: usize,
        response_tx: &mpsc::UnboundedSender<StatusMessage>,
    ) -> Vec<ConfirmTransactionMessage> {
        (0..amount)
            .map(|index| ConfirmTransactionMessage {
                span: Span::current(),
                index,
                transaction: Transaction::default(),
                last_valid_block_height: 0,
                response_tx: response_tx.clone(),
            })
            .collect()
    }

    /// This is essentially an integration test of the full lifecycle of the transaction confirmer.
    #[tokio::test(start_paused = true)]
    async fn test_transaction_confirmer() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .try_init();

        let mock_sender = TrackingMockSender::new(MockSender::new("succeeds".to_string()));
        let rpc_client = Arc::new(RpcClient::new_sender(
            mock_sender.clone(),
            RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
        ));
        let payer = Arc::new(Keypair::new());

        let initial_block = BlockMessage {
            blockhash: Hash::new_from_array(Pubkey::new_unique().to_bytes()),
            last_valid_block_height: 300,
            block_height: 150,
        };
        let (blockdata_tx, blockdata_rx) = watch::channel(initial_block);
        let (transaction_confirmer_tx, transaction_confirmer_rx) =
            mpsc::unbounded_channel::<ConfirmTransactionMessage>();
        let (transaction_sender_tx, mut transaction_sender_rx) =
            mpsc::unbounded_channel::<SendTransactionMessage>();

        let handle = spawn_transaction_confirmer(
            rpc_client,
            blockdata_rx,
            transaction_sender_tx.downgrade(),
            transaction_confirmer_tx.downgrade(),
            transaction_confirmer_rx,
        );

        // No requests should be sent yet.
        let sent_requests = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests, Vec::new());

        // Queue a transaction for confirmation.
        let transaction = Transaction::new_signed_with_payer(
            &[solana_sdk::system_instruction::transfer(
                &payer.pubkey(),
                &solana_sdk::system_program::id(),
                1,
            )],
            Some(&payer.pubkey()),
            &[&payer],
            Hash::default(),
        );
        let (response_tx, mut response_rx) = mpsc::unbounded_channel();
        transaction_confirmer_tx
            .send(ConfirmTransactionMessage {
                span: Span::current(),
                index: 0,
                transaction: transaction.clone(),
                last_valid_block_height: initial_block.last_valid_block_height,
                response_tx: response_tx.clone(),
            })
            .unwrap();

        // Trigger a "new" blockhash to make the confirmation loop iterate.
        blockdata_tx.send_modify(|_| {});

        // Wait for the loop to finish. There should be exactly one response sent.
        let status = response_rx.recv().await.unwrap();
        response_rx.try_recv().unwrap_err();
        assert_eq!(status.index, 0);
        assert_eq!(status.landed_as, Some((0, transaction.signatures[0])));
        assert_eq!(status.status, TransactionStatus::Committed);
        // The signatures should have been checked with the RPC client.
        let sent_requests = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 1);
        assert_eq!(sent_requests[0].request, RpcRequest::GetSignatureStatuses);
        assert_eq!(
            sent_requests[0].params,
            serde_json::json!([[transaction.signatures[0].to_string()]])
        );
        // But nothing should have been sent to the transaction sender for re-sending.
        transaction_sender_rx.try_recv().unwrap_err();

        // Prepare a nine-transaction batch.
        // Since the mock sender is set up to confirm three transactions at a time,
        // it's convenient to split the transactions into three batches.
        let transactions = (0..9)
            .map(|index| {
                let transaction = Transaction::new_signed_with_payer(
                    &[solana_sdk::system_instruction::transfer(
                        &payer.pubkey(),
                        &solana_sdk::system_program::id(),
                        1 + index as u64,
                    )],
                    Some(&payer.pubkey()),
                    &[&payer],
                    Hash::default(),
                );
                (
                    transaction.clone(),
                    ConfirmTransactionMessage {
                        span: Span::current(),
                        index,
                        transaction,
                        last_valid_block_height: initial_block.last_valid_block_height
                            + index as u64,
                        response_tx: response_tx.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();
        let transaction_signatures = transactions
            .iter()
            .map(|(_, msg)| msg.transaction.signatures[0].to_string())
            .collect::<Vec<_>>();
        for (_tx, msg) in transactions.clone() {
            transaction_confirmer_tx.send(msg).unwrap();
        }

        // Trigger a "new" blockhash to make the confirmation loop iterate again.
        blockdata_tx.send_modify(|_| {});

        let mut responses = Vec::new();
        response_rx.recv_many(&mut responses, 100).await;
        assert_eq!(responses.len(), 5);
        // The first three in this batch should be committed, and the next two should be processing.
        // The other 5 shouldn't have been responded to yet.
        assert!(
            responses[..3]
                .iter()
                .all(|r| r.status == TransactionStatus::Committed)
        );
        assert!(
            responses[3..]
                .iter()
                .all(|r| r.status == TransactionStatus::Processing)
        );
        // It is the first five transactions that should have been responded to.
        assert_eq!(
            responses.iter().map(|r| r.index).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );

        // All 10 transactions should have been checked.
        let sent_requests: Vec<TrackedRequest> = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 1);
        let sent_signatures: Vec<Vec<String>> =
            serde_json::from_value(sent_requests[0].params.clone()).unwrap();
        assert_eq!(sent_signatures, vec![transaction_signatures.clone()]);

        // Iterate again.
        blockdata_tx.send_modify(|_| {});
        let mut responses = Vec::new();
        response_rx.recv_many(&mut responses, 100).await;
        assert_eq!(responses.len(), 5);
        // Again, the first three in this batch should be committed, and the next two should be processing.
        assert!(
            responses[..3]
                .iter()
                .all(|r| r.status == TransactionStatus::Committed)
        );
        assert!(
            responses[3..]
                .iter()
                .all(|r| r.status == TransactionStatus::Processing)
        );
        // Even if transactions were queued for re-confirming, they should be checked *after* the
        // initial transactions.
        assert_eq!(
            responses.iter().map(|r| r.index).collect::<Vec<_>>(),
            // Note the 3 at the end, which was queued for re-confirming from the previous batch.
            [5, 6, 7, 8, 3]
        );

        // This time, transactions 5 6 7 8 (new) and 3 4 (re-confirmed) should have been checked.
        let sent_requests: Vec<TrackedRequest> = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 1);
        let sent_signatures: Vec<Vec<String>> =
            serde_json::from_value(sent_requests[0].params.clone()).unwrap();
        assert_eq!(
            sent_signatures,
            vec![vec![
                transaction_signatures[5].clone(),
                transaction_signatures[6].clone(),
                transaction_signatures[7].clone(),
                transaction_signatures[8].clone(),
                transaction_signatures[3].clone(),
                transaction_signatures[4].clone()
            ]]
        );

        // One more round.
        blockdata_tx.send_modify(|_| {});
        let mut responses = Vec::new();
        response_rx.recv_many(&mut responses, 100).await;
        // This time there should only be three committed transactions, and nothing else.
        assert_eq!(responses.len(), 3);
        assert!(
            responses
                .iter()
                .all(|r| r.status == TransactionStatus::Committed)
        );
        assert_eq!(
            responses.iter().map(|r| r.index).collect::<Vec<_>>(),
            // 4 from the first batch, and 8 and 3 (again!) from the second batch.
            [4, 8, 3]
        );

        // This time, transactions 4 8 3 (re-confirmed) should have been checked.
        let sent_requests: Vec<TrackedRequest> = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 1);
        let sent_signatures: Vec<Vec<String>> =
            serde_json::from_value(sent_requests[0].params.clone()).unwrap();
        assert_eq!(
            sent_signatures,
            vec![vec![
                transaction_signatures[4].clone(),
                transaction_signatures[8].clone(),
                transaction_signatures[3].clone(),
            ]]
        );

        // Throughout all the above, *no* transactions should have been re-sent.
        transaction_sender_rx.try_recv().unwrap_err();

        // In TPU mode, trying to confirm a transaction that's more than 100 slots behind
        // should trigger a re-send.
        transaction_confirmer_tx
            .send(ConfirmTransactionMessage {
                span: Span::current(),
                index: 10,
                transaction: transaction.clone(),
                last_valid_block_height: 0,
                response_tx: response_tx.clone(),
            })
            .unwrap();
        // Set the sender to always reply with None for all statuses.
        // If the transaction was successful, it wouldn't trigger the TPU re-send.
        *mock_sender.mode.lock().unwrap() = TrackingMockSenderMode::AllNone;
        blockdata_tx.send_modify(|_| {});

        let message = transaction_sender_rx.recv().await.unwrap();
        assert_eq!(message.index, 10);
        let sent_requests: Vec<TrackedRequest> = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 1);

        // If transactions fail (and return an error), they should be re-sent.
        *mock_sender.mode.lock().unwrap() = TrackingMockSenderMode::AllInstructionError;
        transaction_confirmer_tx
            .send(ConfirmTransactionMessage {
                span: Span::current(),
                index: 11,
                transaction: transaction.clone(),
                last_valid_block_height: initial_block.last_valid_block_height,
                response_tx: response_tx.clone(),
            })
            .unwrap();
        blockdata_tx.send_modify(|_| {});

        let message = transaction_sender_rx.recv().await.unwrap();
        assert_eq!(message.index, 11);
        // Status should be sent as an error.
        let status = response_rx.recv().await.unwrap();
        response_rx.try_recv().unwrap_err();
        assert_eq!(status.index, 11);
        assert!(matches!(status.status, TransactionStatus::Failed(..)));
        let sent_requests: Vec<TrackedRequest> = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 2);

        // If getting signatures itself fails, the transactions should be re-confirmed.
        *mock_sender.mode.lock().unwrap() = TrackingMockSenderMode::RpcError;
        transaction_confirmer_tx
            .send(ConfirmTransactionMessage {
                span: Span::current(),
                index: 12,
                transaction: transaction.clone(),
                last_valid_block_height: initial_block.last_valid_block_height,
                response_tx: response_tx.clone(),
            })
            .unwrap();
        blockdata_tx.send_modify(|_| {});
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let sent_requests: Vec<TrackedRequest> = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 1);
        // Reset the mode to have the confirmation go through this time.
        *mock_sender.mode.lock().unwrap() =
            TrackingMockSenderMode::ThreeConfirmedTwoProcessingRestNone;
        blockdata_tx.send_modify(|_| {});

        let status = response_rx.recv().await.unwrap();
        response_rx.try_recv().unwrap_err();
        assert_eq!(status.index, 12);
        assert_eq!(status.status, TransactionStatus::Committed);
        let sent_requests: Vec<TrackedRequest> = mock_sender.get_and_clear_sent_requests();
        assert_eq!(sent_requests.len(), 1);

        drop(transaction_confirmer_tx);
        drop(blockdata_tx);
        handle.await.unwrap();
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TrackedRequest {
        request: RpcRequest,
        params: serde_json::Value,
        response: Option<serde_json::Value>,
    }

    #[derive(Clone)]
    struct TrackingMockSender {
        sender: Arc<MockSender>,
        log: Arc<Mutex<Vec<TrackedRequest>>>,
        mode: Arc<Mutex<TrackingMockSenderMode>>,
    }

    #[derive(Clone, Copy)]
    enum TrackingMockSenderMode {
        ThreeConfirmedTwoProcessingRestNone,
        AllNone,
        AllInstructionError,
        RpcError,
    }

    impl TrackingMockSender {
        fn new(sender: MockSender) -> Self {
            Self {
                sender: Arc::new(sender),
                log: Arc::new(Mutex::new(Vec::new())),
                mode: Arc::new(Mutex::new(
                    TrackingMockSenderMode::ThreeConfirmedTwoProcessingRestNone,
                )),
            }
        }

        fn get_and_clear_sent_requests(&self) -> Vec<TrackedRequest> {
            mem::take(&mut *self.log.lock().unwrap())
        }
    }

    #[async_trait]
    impl RpcSender for TrackingMockSender {
        async fn send(
            &self,
            request: RpcRequest,
            params: serde_json::Value,
        ) -> SolanaResult<serde_json::Value> {
            let response = match request {
                RpcRequest::GetSignatureStatuses => {
                    let request_signatures: Vec<Vec<String>> =
                        serde_json::from_value(params.clone()).unwrap();
                    let statuses = match *self.mode.lock().unwrap() {
                        TrackingMockSenderMode::ThreeConfirmedTwoProcessingRestNone => {
                            generate_three_two_rest_response(request_signatures)
                        }
                        TrackingMockSenderMode::AllNone => {
                            vec![None; request_signatures[0].len()]
                        }
                        TrackingMockSenderMode::AllInstructionError => {
                            let status = SolanaTransactionStatus {
                                slot: 0,
                                confirmations: None,
                                status: Ok(()),
                                err: Some(TransactionError::InstructionError(
                                    0,
                                    InstructionError::ProgramFailedToComplete,
                                )),
                                confirmation_status: Some(TransactionConfirmationStatus::Processed),
                            };
                            vec![Some(status); request_signatures[0].len()]
                        }
                        TrackingMockSenderMode::RpcError => {
                            self.log.lock().unwrap().push(TrackedRequest {
                                request,
                                params,
                                response: None,
                            });
                            return Err(ClientError {
                                request: Some(request),
                                kind: solana_client::client_error::ClientErrorKind::Custom(
                                    "fail".to_string(),
                                ),
                            });
                        }
                    };
                    Ok(serde_json::to_value(Response {
                        context: RpcResponseContext {
                            slot: 0,
                            api_version: None,
                        },
                        value: statuses,
                    })?)
                }
                _ => self.sender.send(request, params.clone()).await,
            };
            self.log.lock().unwrap().push(TrackedRequest {
                request,
                params,
                response: response.as_ref().ok().cloned(),
            });
            response
        }

        fn get_transport_stats(&self) -> RpcTransportStats {
            self.sender.get_transport_stats()
        }

        fn url(&self) -> String {
            self.sender.url()
        }
    }

    /// In order to simulate not all messages landing at once:
    /// Confirm 3, set 2 to processing, and the rest to None.
    fn generate_three_two_rest_response(
        request_signatures: Vec<Vec<String>>,
    ) -> Vec<Option<SolanaTransactionStatus>> {
        let mut it = request_signatures.concat().into_iter();
        let confirmed = it
            .by_ref()
            .take(3)
            .map(|_| {
                Some(SolanaTransactionStatus {
                    slot: 0,
                    confirmations: None,
                    status: Ok(()),
                    err: None,
                    confirmation_status: Some(TransactionConfirmationStatus::Confirmed),
                })
            })
            .collect::<Vec<_>>();
        let processing = it
            .by_ref()
            .take(2)
            .map(|_| {
                Some(SolanaTransactionStatus {
                    slot: 0,
                    confirmations: None,
                    status: Ok(()),
                    err: None,
                    confirmation_status: Some(TransactionConfirmationStatus::Processed),
                })
            })
            .collect::<Vec<_>>();
        let rest = it.map(|_| None).collect::<Vec<_>>();
        [confirmed, processing, rest].concat()
    }
}
