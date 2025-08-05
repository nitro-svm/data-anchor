use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime},
};

use data_anchor_api::RelevantInstructionWithAccounts;
use data_anchor_blober::{
    CHUNK_SIZE, COMPOUND_DECLARE_TX_SIZE, COMPOUND_TX_SIZE,
    instruction::{DeclareBlob, FinalizeBlob, InsertChunk},
};
use jsonrpsee::http_client::HttpClient;
use solana_pubkey::Pubkey;
use solana_sdk::{message::Message, signer::Signer};
use tracing::{Instrument, Span, info_span};

use crate::{
    DataAnchorClient, DataAnchorClientResult, FeeStrategy, OutcomeError, SuccessfulTransaction,
    TransactionOutcome,
    client::ChainError,
    tx::{Compound, CompoundDeclare, CompoundFinalize, MessageArguments, MessageBuilder},
    types::TransactionType,
};

pub enum UploadMessages {
    CompoundUpload(Message),
    StaggeredUpload {
        declare_blob: Message,
        insert_chunks: Vec<Message>,
        finalize_blob: Message,
    },
}

impl DataAnchorClient {
    /// Uploads the blob: [`data_anchor_blober::DeclareBlob`], [`data_anchor_blober::InsertChunk`] * N,
    /// [`data_anchor_blober::FinalizeBlob`].
    pub(crate) async fn do_upload(
        &self,
        upload_messages: UploadMessages,
        timeout: Option<Duration>,
    ) -> DataAnchorClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
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
                .map_err(ChainError::CompoundUpload)?)
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
                .map_err(ChainError::DeclareBlob)?;

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
                .map_err(ChainError::InsertChunks)?;

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
                .map_err(ChainError::FinalizeBlob)?;

                Ok(tx1
                    .into_iter()
                    .chain(tx2.into_iter())
                    .chain(tx3.into_iter())
                    .collect())
            }
        }
    }

    /// Generates a [`data_anchor_blober::DeclareBlob`], vector of [`data_anchor_blober::InsertChunk`]
    /// and a [`data_anchor_blober::FinalizeBlob`] message.
    pub(crate) async fn generate_messages(
        &self,
        blob: Pubkey,
        timestamp: u64,
        blob_data: &[u8],
        fee_strategy: FeeStrategy,
        blober: Pubkey,
    ) -> DataAnchorClientResult<UploadMessages> {
        if blob_data.len() <= COMPOUND_TX_SIZE as usize {
            let fee_compound = fee_strategy
                .convert_fee_strategy_to_fixed(
                    &self.rpc_client,
                    &[blober, blob, self.payer.pubkey()],
                    TransactionType::Compound,
                )
                .await?;

            let compound = Compound::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_compound,
                Compound::new(blob, timestamp, blob_data.to_vec()),
            ))
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy");

            return Ok(UploadMessages::CompoundUpload(compound));
        }

        if blob_data.len() <= COMPOUND_DECLARE_TX_SIZE as usize {
            let fee_compound_declare = fee_strategy
                .convert_fee_strategy_to_fixed(
                    &self.rpc_client,
                    &[blober, blob, self.payer.pubkey()],
                    TransactionType::Compound,
                )
                .await?;

            let declare_blob = CompoundDeclare::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_compound_declare,
                CompoundDeclare::new(blob, timestamp, blob_data.to_vec()),
            ))
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy");

            let fee_finalize = fee_strategy
                .convert_fee_strategy_to_fixed(
                    &self.rpc_client,
                    &[blober, blob, self.payer.pubkey()],
                    TransactionType::FinalizeBlob,
                )
                .await?;

            let finalize_blob = FinalizeBlob::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_finalize,
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

        let fee_declare = fee_strategy
            .convert_fee_strategy_to_fixed(
                &self.rpc_client,
                &[blob, self.payer.pubkey()],
                TransactionType::DeclareBlob,
            )
            .await?;

        let declare_blob = DeclareBlob::build_message(MessageArguments::new(
            self.program_id,
            blober,
            &self.payer,
            self.rpc_client.clone(),
            fee_declare,
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

        let fee_insert = fee_strategy
            .convert_fee_strategy_to_fixed(
                &self.rpc_client,
                &[blob, self.payer.pubkey()],
                TransactionType::InsertChunk(0),
            )
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
                    fee_insert,
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

        let finalize_blob = if let Some((chunk_idx, chunk_data)) = last_chunk {
            let fee_compound_finalize = fee_strategy
                .convert_fee_strategy_to_fixed(
                    &self.rpc_client,
                    &[blober, blob, self.payer.pubkey()],
                    TransactionType::CompoundFinalize,
                )
                .await?;

            CompoundFinalize::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_compound_finalize,
                CompoundFinalize::new(*chunk_idx, chunk_data.to_vec(), blob),
            ))
            .await
            .expect("infallible with a fixed fee strategy")
        } else {
            let fee_finalize = fee_strategy
                .convert_fee_strategy_to_fixed(
                    &self.rpc_client,
                    &[blober, blob, self.payer.pubkey()],
                    TransactionType::FinalizeBlob,
                )
                .await?;

            FinalizeBlob::build_message(MessageArguments::new(
                self.program_id,
                blober,
                &self.payer,
                self.rpc_client.clone(),
                fee_finalize,
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

    /// Get a reference to the Indexer RPC client.
    ///
    /// # Panics
    /// If the client is not present. It will be present in real code, but may not be in tests.
    pub(crate) fn indexer(&self) -> &HttpClient {
        self.indexer_client
            .as_ref()
            .expect("indexer client to be present")
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

/// Filters out the relevant instructions for finalized blobs into a [`HashMap`].
pub fn filter_relevant_instructions(
    instructions: Vec<RelevantInstructionWithAccounts>,
    finalized_blobs: &HashSet<Pubkey>,
    acc: &mut HashMap<Pubkey, Vec<RelevantInstructionWithAccounts>>,
) {
    for instruction in instructions {
        if !finalized_blobs.contains(&instruction.blob) {
            continue;
        }
        acc.entry(instruction.blob).or_default().push(instruction);
    }
}
