use std::{
    cmp::max,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime},
};

use anchor_lang::{solana_program::message::Message, Discriminator};
use blober::{
    instruction::{DeclareBlob, FinalizeBlob, InsertChunk},
    CHUNK_SIZE, COMPOUND_DECLARE_TX_SIZE, COMPOUND_TX_SIZE,
};
use jsonrpsee::ws_client::WsClient;
use solana_sdk::{message::VersionedMessage, pubkey::Pubkey, signer::Signer};
use solana_transaction_status::EncodedTransactionWithStatusMeta;
use tracing::{info_span, Instrument, Span};

use crate::{
    tx::{Compound, CompoundDeclare, CompoundFinalize, MessageArguments, MessageBuilder},
    types::{TransactionType, UploadBlobError},
    BloberClient, BloberClientResult, Fee, FeeStrategy, Lamports, OutcomeError,
    SuccessfulTransaction, TransactionOutcome,
};

pub enum UploadMessages {
    CompoundUpload(Message),
    StaggeredUpload {
        declare_blob: Message,
        insert_chunks: Vec<Message>,
        finalize_blob: Message,
    },
}

impl BloberClient {
    /// Uploads the blob: [`blober::DeclareBlob`], [`blober::InsertChunk`] * N, [`blober::FinalizeBlob`].
    pub(crate) async fn do_upload(
        &self,
        upload_messages: UploadMessages,
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let before = Instant::now();

        match upload_messages {
            UploadMessages::CompoundUpload(tx) => {
                let span = info_span!(parent: Span::current(), "compound_upload");
                Ok(check_outcomes(
                    self.batch_client
                        .send(vec![(TransactionType::Compound, tx)], timeout)
                        .instrument(span)
                        .await,
                )
                .map_err(UploadBlobError::CompoundUpload)?)
            }
            UploadMessages::StaggeredUpload {
                declare_blob,
                insert_chunks,
                finalize_blob,
            } => {
                let span = info_span!(parent: Span::current(), "declare_blob");
                let tx1 = check_outcomes(
                    self.batch_client
                        .send(vec![(TransactionType::DeclareBlob, declare_blob)], timeout)
                        .instrument(span)
                        .await,
                )
                .map_err(UploadBlobError::DeclareBlob)?;

                let span = info_span!(parent: Span::current(), "insert_chunks");
                let timeout =
                    timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
                let tx2 = check_outcomes(
                    self.batch_client
                        .send(
                            insert_chunks
                                .into_iter()
                                .enumerate()
                                .map(|(idx, tx)| (TransactionType::InsertChunk(idx as u16), tx))
                                .collect(),
                            timeout,
                        )
                        .instrument(span)
                        .await,
                )
                .map_err(UploadBlobError::InsertChunks)?;

                let span = info_span!(parent: Span::current(), "finalize_blob");
                let timeout =
                    timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
                let tx3 = check_outcomes(
                    self.batch_client
                        .send(
                            vec![(TransactionType::FinalizeBlob, finalize_blob)],
                            timeout,
                        )
                        .instrument(span)
                        .await,
                )
                .map_err(UploadBlobError::FinalizeBlob)?;

                Ok(tx1
                    .into_iter()
                    .chain(tx2.into_iter())
                    .chain(tx3.into_iter())
                    .collect())
            }
        }
    }

    /// Generates a [`blober::DeclareBlob`], vector of [`blober::InsertChunk`] and a [`blober::FinalizeBlob`] message.
    pub(crate) async fn generate_messages(
        &self,
        blob: Pubkey,
        timestamp: u64,
        blob_data: &[u8],
        fee_strategy: FeeStrategy,
        blober: Pubkey,
    ) -> BloberClientResult<UploadMessages> {
        if blob_data.len() <= COMPOUND_TX_SIZE as usize {
            let fee_strategy_compound = self
                .convert_fee_strategy_to_fixed(
                    fee_strategy,
                    &[blober, blob],
                    TransactionType::Compound,
                )
                .await?;

            let compound = Compound::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_strategy_compound,
                self.helius_fee_estimate,
                Compound::new(blob, timestamp, blob_data.to_vec()),
            ))
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy");

            return Ok(UploadMessages::CompoundUpload(compound));
        }

        if blob_data.len() <= COMPOUND_DECLARE_TX_SIZE as usize {
            let fee_strategy_compound = self
                .convert_fee_strategy_to_fixed(
                    fee_strategy,
                    &[blober, blob],
                    TransactionType::Compound,
                )
                .await?;

            let declare_blob = CompoundDeclare::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_strategy_compound,
                self.helius_fee_estimate,
                CompoundDeclare::new(blob, timestamp, blob_data.to_vec()),
            ))
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy");

            let fee_strategy_finalize = self
                .convert_fee_strategy_to_fixed(
                    fee_strategy,
                    &[blober, blob],
                    TransactionType::FinalizeBlob,
                )
                .await?;

            let finalize_blob = FinalizeBlob::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_strategy_finalize,
                self.helius_fee_estimate,
                blob,
            ))
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy");

            return Ok(UploadMessages::StaggeredUpload {
                declare_blob,
                insert_chunks: Vec::new(),
                finalize_blob,
            });
        }

        let chunks = split_blob_into_chunks(blob_data);

        let fee_strategy_declare = self
            .convert_fee_strategy_to_fixed(fee_strategy, &[blob], TransactionType::DeclareBlob)
            .await?;

        let declare_blob = DeclareBlob::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee_strategy_declare,
            self.helius_fee_estimate,
            (
                DeclareBlob {
                    blob_size: blob_data.len() as u32,
                    timestamp,
                },
                blob,
            ),
        ))
        .in_current_span()
        .await
        .expect("infallible with a fixed fee strategy");

        let fee_strategy_insert = self
            .convert_fee_strategy_to_fixed(fee_strategy, &[blob], TransactionType::InsertChunk(0))
            .await?;

        let mut chunk_iterator = chunks.iter();
        let last_chunk = chunk_iterator.next_back();

        let insert_chunks =
            futures::future::join_all(chunk_iterator.map(|(chunk_index, chunk_data)| async move {
                InsertChunk::build_message(MessageArguments::new(
                    self.program_id,
                    blober,
                    &self.payer,
                    self.rpc_client.clone(),
                    fee_strategy_insert,
                    self.helius_fee_estimate,
                    (
                        InsertChunk {
                            idx: *chunk_index,
                            data: chunk_data.to_vec(),
                        },
                        blob,
                    ),
                ))
                .in_current_span()
                .await
                .expect("infallible with a fixed fee strategy")
            }))
            .await;

        let fee_strategy_finalize = self
            .convert_fee_strategy_to_fixed(
                fee_strategy,
                &[blober, blob],
                TransactionType::FinalizeBlob,
            )
            .await?;

        let finalize_blob = if let Some((chunk_idx, chunk_data)) = last_chunk {
            CompoundFinalize::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_strategy_finalize,
                self.helius_fee_estimate,
                CompoundFinalize::new(*chunk_idx, chunk_data.to_vec(), blob),
            ))
            .await
            .expect("infallible with a fixed fee strategy")
        } else {
            FinalizeBlob::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_strategy_finalize,
                self.helius_fee_estimate,
                blob,
            ))
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy")
        };

        Ok(UploadMessages::StaggeredUpload {
            declare_blob,
            insert_chunks,
            finalize_blob,
        })
    }

    /// Converts a [`FeeStrategy`] into a [`FeeStrategy::Fixed`] with the current compute unit price.
    pub(crate) async fn convert_fee_strategy_to_fixed(
        &self,
        fee_strategy: FeeStrategy,
        mutating_accounts: &[Pubkey],
        tx_type: TransactionType,
    ) -> BloberClientResult<FeeStrategy> {
        let FeeStrategy::BasedOnRecentFees(priority) = fee_strategy else {
            return Ok(fee_strategy);
        };

        let mut fee_retries = 5;

        let mutating_accounts = [mutating_accounts, &[self.payer.pubkey()]].concat();

        while fee_retries > 0 {
            let res = priority
                .get_priority_fee_estimate(
                    &self.rpc_client,
                    &mutating_accounts,
                    self.helius_fee_estimate,
                )
                .in_current_span()
                .await;

            match res {
                Ok(fee) => {
                    return Ok(FeeStrategy::Fixed(Fee {
                        prioritization_fee_rate: fee,
                        num_signatures: tx_type.num_signatures(),
                        compute_unit_limit: tx_type.compute_unit_limit(),
                        price_per_signature: Lamports(5000),
                        blob_account_size: 0,
                    }));
                }
                Err(e) => {
                    fee_retries -= 1;
                    if fee_retries == 0 {
                        return Err(e);
                    }
                }
            }
        }

        Err(UploadBlobError::ConversionError("Fee strategy conversion failed after retries").into())
    }

    /// Get a reference to the Indexer RPC client.
    ///
    /// # Panics
    /// If the client is not present. It will be present in real code, but may not be in tests.
    pub(crate) fn indexer(&self) -> &WsClient {
        self.indexer_client
            .as_ref()
            .expect("indexer client to be present")
    }
}

/// Finds finalize blob transactions for the supplied [`blober`] Pubkey
pub(crate) fn find_finalize_blob_transactions_for_blober(
    blober: Pubkey,
    program_id: Pubkey,
) -> impl FnMut(&EncodedTransactionWithStatusMeta) -> Option<(Pubkey, VersionedMessage)> {
    move |tx| {
        // Ignore transactions that failed
        if matches!(tx.meta.as_ref(), Some(meta) if meta.status.is_err()) {
            return None;
        }

        let versioned_tx = tx.transaction.decode()?;
        let account_keys = versioned_tx.message.static_account_keys();
        let blob_address = versioned_tx
            .message
            .instructions()
            .iter()
            .filter_map(find_finalize_blob_instruction_for_blober(
                account_keys,
                blober,
                program_id,
            ))
            .next();

        blob_address.map(|blob_address| (blob_address, versioned_tx.message))
    }
}

/// Filters [`blober::instruction::FinalizeBlob`] instructions for the supplied `blober` Pubkey on the given `program_id`.
fn find_finalize_blob_instruction_for_blober(
    account_keys: &[Pubkey],
    blober: Pubkey,
    program_id: Pubkey,
) -> impl FnMut(&solana_sdk::instruction::CompiledInstruction) -> Option<Pubkey> + '_ {
    move |instruction| {
        if instruction.program_id(account_keys) != &program_id {
            return None;
        }

        let discriminator = blober::instruction::FinalizeBlob::DISCRIMINATOR;

        if instruction.data.get(..discriminator.len()) != Some(discriminator) {
            return None;
        }

        if instruction
            .accounts
            .get(1)
            .and_then(|lookup_account_index| account_keys.get((*lookup_account_index) as usize))
            != Some(&blober)
        {
            return None;
        }

        instruction
            .accounts
            .first()
            .and_then(|lookup_account_index| account_keys.get((*lookup_account_index) as usize))
            .copied()
    }
}

/// Returns a unique timestamp in seconds since the UNIX epoch.
/// If multiple threads or instances use this function, timestamps are incremented to ensure uniqueness.
pub(crate) fn get_unique_timestamp() -> u64 {
    static LAST_USED_TIMESTAMP: AtomicU64 = AtomicU64::new(0);

    let mut last_used_timestamp = LAST_USED_TIMESTAMP.load(Ordering::Relaxed);
    loop {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("System time must move forward")
            .as_secs();

        // Use the current time or the next available timestamp.
        let timestamp = max(now, last_used_timestamp + 1);

        // Update the last used timestamp if no other thread has changed it.
        match LAST_USED_TIMESTAMP.compare_exchange_weak(
            last_used_timestamp,
            timestamp,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return timestamp, // Success, return the unique timestamp.
            Err(new_timestamp) => last_used_timestamp = new_timestamp, // Retry with updated value.
        }
    }
}

/// Splits a blob of data into chunks of size [`CHUNK_SIZE`].
pub(crate) fn split_blob_into_chunks(data: &[u8]) -> Vec<(u16, &[u8])> {
    data.chunks(CHUNK_SIZE as usize)
        .enumerate()
        .map(|(i, chunk)| (i as u16, chunk))
        .collect::<Vec<_>>()
}

pub(crate) fn check_outcomes(
    outcomes: Vec<TransactionOutcome<TransactionType>>,
) -> Result<Vec<SuccessfulTransaction<TransactionType>>, OutcomeError> {
    if outcomes.iter().all(|o| o.successful()) {
        let successful_transactions = outcomes
            .into_iter()
            .filter_map(TransactionOutcome::into_successful)
            .collect();
        Ok(successful_transactions)
    } else {
        Err(OutcomeError::Unsuccesful(outcomes))
    }
}
