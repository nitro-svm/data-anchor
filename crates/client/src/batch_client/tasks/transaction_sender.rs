use std::sync::Arc;

use solana_client::{
    client_error::ClientError as Error, rpc_client::SerializableTransaction,
    rpc_config::RpcSendTransactionConfig,
};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentLevel, signature::Signature, signer::keypair::Keypair,
    transaction::Transaction,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::Instant,
};
use tracing::{trace, warn, Instrument, Span};

use super::super::{
    channels::upgrade_and_send,
    messages::{BlockMessage, ConfirmTransactionMessage, SendTransactionMessage},
};
use crate::batch_client::client::SEND_TRANSACTION_INTERVAL;

/// Spawns an independent task that listens for [`SendTransactionMessage`]s and periodically submits
/// transactions using the Solana RPC client, re-signing the transactions when necessary.
///
/// It does *not* check the outcome of the transaction at all other than failing if the transaction
/// submission itself fails. When this happens, the transaction will be queued for re-sending.
///
/// The task will exit if there are no transaction senders alive. This will happen when the
/// [BatchClient](`crate::batch_client::BatchClient`) has been dropped.
#[allow(clippy::too_many_arguments)]
pub fn spawn_transaction_sender(
    rpc_client: Arc<RpcClient>,
    signers: Vec<Arc<Keypair>>,
    blockdata_rx: watch::Receiver<BlockMessage>,
    transaction_confirmer_tx: mpsc::UnboundedSender<ConfirmTransactionMessage>,
    transaction_sender_tx: mpsc::WeakUnboundedSender<SendTransactionMessage>,
    mut transaction_sender_rx: mpsc::UnboundedReceiver<SendTransactionMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_send = Instant::now();

        while let Some(mut msg) = transaction_sender_rx.recv().await {
            if msg.response_tx.is_closed() {
                warn!("no receivers for transaction sender, shutting down transaction sender");
                break;
            }

            // Get the current newest block data but don't wait for a new block, just use
            // the current value.
            let blockdata = *blockdata_rx.borrow();
            let last_valid_block_height =
                sign_transaction_if_necessary(&blockdata, &mut msg, &signers);

            // Space the transaction submissions out by a small delay to avoid rate limits.
            tokio::time::sleep_until(last_send + SEND_TRANSACTION_INTERVAL).await;
            last_send = Instant::now();

            let res = send_transaction(&rpc_client, &msg.transaction)
                .instrument(msg.span.clone())
                .await;

            match res {
                Ok(_) => {
                    let _ = transaction_confirmer_tx.send(ConfirmTransactionMessage {
                        span: msg.span,
                        index: msg.index,
                        transaction: msg.transaction,
                        last_valid_block_height,
                        response_tx: msg.response_tx,
                    });
                }
                Err(e) => {
                    let _enter = msg.span.clone().entered();
                    warn!("failed to send transaction: {e:?}, tx slot: {last_valid_block_height}");

                    let res = upgrade_and_send(
                        &transaction_sender_tx,
                        [SendTransactionMessage {
                            // Force re-sign. Since the transaction couldn't be sent, this should be safe.
                            last_valid_block_height: 0,
                            ..msg
                        }],
                    );

                    if res.is_break() {
                        break;
                    }
                }
            }
        }

        warn!("shutting down transaction sender");
    })
}

/// Signs a transaction if necessary. If the transaction's last valid block height has expired,
/// or if it has been explicitly set to 0, forcing a re-sign.
///
/// If the transaction does not need to be re-signed, it is returned as-is.
///
/// # Returns
/// The last valid block height of the transaction, whether changed or not.
fn sign_transaction_if_necessary(
    blockdata: &BlockMessage,
    msg: &mut SendTransactionMessage,
    signers: &Vec<Arc<Keypair>>,
) -> u64 {
    let _enter = msg.span.clone().entered();
    if blockdata.block_height > msg.last_valid_block_height + 1 {
        let old_sig = *msg.transaction.get_signature();
        msg.transaction.sign(signers, blockdata.blockhash);
        if old_sig != Signature::default() {
            trace!(
                "[{}] re-sending tx {} as {}",
                msg.index,
                old_sig,
                msg.transaction.get_signature()
            );
        }
        blockdata.last_valid_block_height
    } else {
        trace!(
            "[{}] sending tx {}",
            msg.index,
            msg.transaction.get_signature()
        );
        msg.last_valid_block_height
    }
}

/// Submits a transaction using the [`TpuClient`] if one is provided, otherwise using the
/// [`RpcClient`].
///
/// Returns an error if the transaction submission itself fails - the outcome of the transaction
/// is not checked.
async fn send_transaction(
    rpc_client: &Arc<RpcClient>,
    transaction: &Transaction,
) -> Result<(), Error> {
    let rpc_client = rpc_client.clone();
    let transaction = transaction.clone();
    let span = Span::current();
    tokio::spawn(async move {
        let res = rpc_client
            .send_transaction_with_config(
                &transaction,
                RpcSendTransactionConfig {
                    max_retries: Some(0),
                    skip_preflight: true,
                    preflight_commitment: Some(CommitmentLevel::Processed),
                    ..Default::default()
                },
            )
            .instrument(span.clone())
            .await;
        // Log errors but don't act on them, they will be caught later and retried regardless.
        if let Err(e) = res {
            warn!(parent: &span, "Error sending transaction: {:?}", e);
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::prelude::Pubkey;
    use solana_sdk::{hash::Hash, signer::Signer};
    use tokio::time::{sleep_until, Duration, Instant};
    use tracing::{Level, Span};

    use super::*;

    /// This is essentially an integration test of the full lifecycle of the transaction sender.
    #[tokio::test(start_paused = true)]
    async fn test_transaction_sender() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .try_init();

        // Use the paused current time as a reference point so the rest of the test doesn't depend
        // on the current time.
        let initial_time = Instant::now();

        let rpc_client = Arc::new(RpcClient::new_mock("succeeds".to_string()));

        // This connection manager and its constituent parts are implemented below.
        let payer = Arc::new(Keypair::new());

        let initial_block = BlockMessage {
            blockhash: Hash::new_from_array(Pubkey::new_unique().to_bytes()),
            last_valid_block_height: 1150,
            block_height: 1000,
        };
        let (blockdata_tx, blockdata_rx) = watch::channel(initial_block);
        let (transaction_confirmer_tx, mut transaction_confirmer_rx) =
            mpsc::unbounded_channel::<ConfirmTransactionMessage>();
        let (transaction_sender_tx, transaction_sender_rx) =
            mpsc::unbounded_channel::<SendTransactionMessage>();

        let handle = spawn_transaction_sender(
            rpc_client,
            vec![payer.clone()],
            blockdata_rx,
            transaction_confirmer_tx,
            transaction_sender_tx.downgrade(),
            transaction_sender_rx,
        );

        // No transactions should be queued for confirmation yet.
        transaction_confirmer_rx.try_recv().unwrap_err();

        // Send a transaction.
        let transaction = Transaction::new_signed_with_payer(
            &[solana_sdk::system_instruction::transfer(
                &payer.pubkey(),
                &solana_sdk::system_program::id(),
                1,
            )],
            Some(&payer.pubkey()),
            &[&payer],
            solana_sdk::hash::Hash::default(),
        );
        let (response_tx, mut response_rx) = mpsc::unbounded_channel();
        transaction_sender_tx
            .send(SendTransactionMessage {
                span: Span::current(),
                index: 0,
                transaction: transaction.clone(),
                last_valid_block_height: initial_block.last_valid_block_height,
                response_tx: response_tx.clone(),
            })
            .unwrap();
        sleep_until(initial_time + SEND_TRANSACTION_INTERVAL + Duration::from_millis(1)).await;

        // There should be one message in the confirmation queue.
        let confirmation = transaction_confirmer_rx.try_recv().unwrap();
        transaction_confirmer_rx.try_recv().unwrap_err();
        assert_eq!(confirmation.index, 0);
        assert_eq!(confirmation.transaction, transaction);
        assert_eq!(
            confirmation.last_valid_block_height,
            initial_block.last_valid_block_height
        );

        // Send the transaction again, but with a different last_valid_block_height.
        // This should cause the transaction to be re-signed.

        // Set a new blockhash to make the signature different.
        let new_block = BlockMessage {
            blockhash: Hash::new_from_array(Pubkey::new_unique().to_bytes()),
            last_valid_block_height: 1151,
            block_height: 1001,
        };
        blockdata_tx.send(new_block).unwrap();
        transaction_sender_tx
            .send(SendTransactionMessage {
                span: Span::current(),
                index: 1,
                transaction: transaction.clone(),
                last_valid_block_height: 0,
                response_tx: response_tx.clone(),
            })
            .unwrap();
        sleep_until(initial_time + 2 * SEND_TRANSACTION_INTERVAL + Duration::from_millis(1)).await;

        // There should be one message in the confirmation queue.
        let confirmation = transaction_confirmer_rx.try_recv().unwrap();
        transaction_confirmer_rx.try_recv().unwrap_err();
        assert_eq!(confirmation.index, 1);
        assert_eq!(
            confirmation.last_valid_block_height,
            new_block.last_valid_block_height
        );

        // No confirmations should have been sent to the response channel by this task.
        response_rx.try_recv().unwrap_err();

        // Drop the transaction sender and response receiver to trigger the watcher to exit.
        drop(transaction_sender_tx);
        drop(response_rx);
        handle.await.unwrap();
    }
}
