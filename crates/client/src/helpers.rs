use std::{
    cmp::max,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime},
};

use anchor_lang::{solana_program::message::Message, Discriminator};
use blober::CHUNK_SIZE;
use jsonrpsee::ws_client::WsClient;
use solana_sdk::{message::VersionedMessage, pubkey::Pubkey, signer::Signer};
use solana_transaction_status::EncodedTransactionWithStatusMeta;
use tracing::{info_span, Instrument, Span};

use crate::{
    tx::{self, MessageArguments},
    types::{TransactionType, UploadBlobError},
    BloberClient, BloberClientResult, Fee, FeeStrategy, Lamports, OutcomeError,
    SuccessfulTransaction, TransactionOutcome,
};

impl BloberClient {
    /// Uploads the blob: [`blober::DeclareBlob`], [`blober::InsertChunk`] * N, [`blober::FinalizeBlob`].
    pub(crate) async fn do_upload(
        &self,
        declare_blob: (TransactionType, Message),
        insert_chunks: Vec<(TransactionType, Message)>,
        finalize_blob: (TransactionType, Message),
        timeout: Option<Duration>,
    ) -> BloberClientResult<Vec<SuccessfulTransaction<TransactionType>>> {
        let before = Instant::now();

        let span = info_span!(parent: Span::current(), "declare_blob");
        let tx1 = check_outcomes(
            self.batch_client
                .send(vec![declare_blob], timeout)
                .instrument(span)
                .await,
        )
        .map_err(UploadBlobError::DeclareBlob)?;

        let span = info_span!(parent: Span::current(), "insert_chunks");
        let timeout = timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
        let tx2 = check_outcomes(
            self.batch_client
                .send(insert_chunks, timeout)
                .instrument(span)
                .await,
        )
        .map_err(UploadBlobError::InsertChunks)?;

        let span = info_span!(parent: Span::current(), "finalize_blob");
        let timeout = timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
        let tx3 = check_outcomes(
            self.batch_client
                .send(vec![finalize_blob], timeout)
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

    /// Generates a [`blober::DeclareBlob`], vector of [`blober::InsertChunk`] and a [`blober::FinalizeBlob`] message.
    pub(crate) async fn generate_messages(
        &self,
        blob: Pubkey,
        blob_size: u32,
        timestamp: u64,
        chunks: Vec<(u16, &[u8])>,
        fee_strategy: FeeStrategy,
        blober: Pubkey,
    ) -> BloberClientResult<(
        (TransactionType, Message),
        Vec<(TransactionType, Message)>,
        (TransactionType, Message),
    )> {
        let fee_strategy_declare = self
            .convert_fee_strategy_to_fixed(fee_strategy, &[blob], TransactionType::DeclareBlob)
            .await?;

        let declare_blob_msg = (
            TransactionType::DeclareBlob,
            tx::declare_blob(
                &MessageArguments::new(
                    self.program_id,
                    blober,
                    &self.payer,
                    self.rpc_client.clone(),
                    fee_strategy_declare,
                    self.helius_fee_estimate,
                ),
                blob,
                timestamp,
                blob_size,
                chunks.len() as u16,
            )
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy"),
        );

        let fee_strategy_insert = self
            .convert_fee_strategy_to_fixed(fee_strategy, &[blob], TransactionType::InsertChunk(0))
            .await?;

        let insert_chunk_msgs =
            futures::future::join_all(chunks.iter().map(|(chunk_index, chunk_data)| async move {
                let insert_tx = tx::insert_chunk(
                    &MessageArguments::new(
                        self.program_id,
                        blober,
                        &self.payer,
                        self.rpc_client.clone(),
                        fee_strategy_insert,
                        self.helius_fee_estimate,
                    ),
                    blob,
                    *chunk_index,
                    chunk_data.to_vec(),
                )
                .in_current_span()
                .await
                .expect("infallible with a fixed fee strategy");
                (TransactionType::InsertChunk(*chunk_index), insert_tx)
            }))
            .await;

        let fee_strategy_finalize = self
            .convert_fee_strategy_to_fixed(
                fee_strategy,
                &[blober, blob],
                TransactionType::FinalizeBlob,
            )
            .await?;

        let complete_msg = (
            TransactionType::FinalizeBlob,
            tx::finalize_blob(
                &MessageArguments::new(
                    self.program_id,
                    blober,
                    &self.payer,
                    self.rpc_client.clone(),
                    fee_strategy_finalize,
                    self.helius_fee_estimate,
                ),
                blob,
            )
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy"),
        );

        Ok((declare_blob_msg, insert_chunk_msgs, complete_msg))
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
