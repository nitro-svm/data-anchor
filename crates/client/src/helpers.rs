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
    tx,
    tx::set_compute_unit_price::calculate_compute_unit_price,
    types::{TransactionType, UploadBlobError},
    BloberClient, Fee, FeeStrategy, Lamports, SuccessfulTransaction, TransactionOutcome,
};

impl BloberClient {
    /// Uploads the blob: DeclareBlob, InsertChunks * N, FinalizeBlob.  Returns a vec of successful
    /// transactions if everything succeeds, or tuple of boolean and UploadBlobError where boolean
    /// indicates if blob was declared
    pub(crate) async fn do_upload(
        &self,
        declare_blob: (TransactionType, Message),
        insert_chunks: Vec<(TransactionType, Message)>,
        finalize_blob: (TransactionType, Message),
        timeout: Option<Duration>,
    ) -> Result<Vec<SuccessfulTransaction<TransactionType>>, (bool, UploadBlobError)> {
        let before = Instant::now();

        let span = info_span!(parent: Span::current(), "declare_blob");
        let tx1 = check_outcomes(
            self.batch_client
                .send(vec![declare_blob], timeout)
                .instrument(span)
                .await,
        )
        .map_err(|err| (false, err))?;

        let span = info_span!(parent: Span::current(), "insert_chunks");
        let timeout = timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
        let tx2 = check_outcomes(
            self.batch_client
                .send(insert_chunks, timeout)
                .instrument(span)
                .await,
        )
        .map_err(|err| (true, err))?;

        let span = info_span!(parent: Span::current(), "finalize_blob");
        let timeout = timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
        let tx3 = check_outcomes(
            self.batch_client
                .send(vec![finalize_blob], timeout)
                .instrument(span)
                .await,
        )
        .map_err(|err| (true, err))?;

        Ok(tx1
            .into_iter()
            .chain(tx2.into_iter())
            .chain(tx3.into_iter())
            .collect())
    }

    pub(crate) async fn generate_messages(
        &self,
        blob: Pubkey,
        timestamp: u64,
        chunks: Vec<(u16, &[u8])>,
        fee_strategy: FeeStrategy,
        blober: Pubkey,
    ) -> (
        (TransactionType, Message),
        Vec<(TransactionType, Message)>,
        (TransactionType, Message),
    ) {
        let blob_size = chunks.iter().map(|(_, chunk)| chunk.len() as u32).sum();
        let declare_blob_msg = (
            TransactionType::DeclareBlob,
            tx::declare_blob(
                &self.rpc_client,
                &self.payer,
                blob,
                blober,
                timestamp,
                blob_size,
                chunks.len() as u16,
                fee_strategy,
            )
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy"),
        );

        let insert_chunk_msgs =
            futures::future::join_all(chunks.iter().map(|(chunk_index, chunk_data)| async move {
                let insert_tx = tx::insert_chunk(
                    &self.rpc_client,
                    &self.payer,
                    blob,
                    blober,
                    *chunk_index,
                    chunk_data.to_vec(),
                    fee_strategy,
                )
                .in_current_span()
                .await
                .expect("infallible with a fixed fee strategy");
                (TransactionType::InsertChunk(*chunk_index), insert_tx)
            }))
            .await;

        let complete_msg = (
            TransactionType::FinalizeBlob,
            tx::finalize_blob(&self.rpc_client, &self.payer, blob, blober, fee_strategy)
                .in_current_span()
                .await
                .expect("infallible with a fixed fee strategy"),
        );

        (declare_blob_msg, insert_chunk_msgs, complete_msg)
    }

    pub(crate) async fn convert_fee_strategy_to_fixed(
        &self,
        fee_strategy: FeeStrategy,
        blob: Pubkey,
    ) -> Result<FeeStrategy, UploadBlobError> {
        if let FeeStrategy::Fixed(_) = fee_strategy {
            return Ok(fee_strategy);
        }

        if let FeeStrategy::BasedOnRecentFees(priority) = fee_strategy {
            let mut fee_retries = 5;

            while fee_retries > 0 {
                let res = calculate_compute_unit_price(
                    &self.rpc_client,
                    &[blob, self.payer.pubkey()],
                    priority,
                )
                .in_current_span()
                .await;

                match res {
                    Ok(fee) => {
                        return Ok(FeeStrategy::Fixed(Fee {
                            prioritization_fee_rate: fee,
                            num_signatures: 0,
                            price_per_signature: Lamports::ZERO,
                            compute_unit_limit: 0,
                            blob_account_size: 0,
                        }));
                    }
                    Err(e) => {
                        fee_retries -= 1;
                        if fee_retries == 0 {
                            return Err(UploadBlobError::PriorityFees(e));
                        }
                    }
                }
            }
        }

        Err(UploadBlobError::ConversionError(
            "Fee strategy conversion failed after retries".to_string(),
        ))
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
) -> impl FnMut(&EncodedTransactionWithStatusMeta) -> Option<(Pubkey, VersionedMessage)> {
    move |tx| {
        if let Some(meta) = tx.meta.as_ref() {
            if meta.status.is_err() {
                // Ignore transactions that failed
                return None;
            }
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
            ))
            .next();
        if let Some(blob_address) = blob_address {
            Some((blob_address, versioned_tx.message.clone()))
        } else {
            None
        }
    }
}
fn find_finalize_blob_instruction_for_blober(
    account_keys: &[Pubkey],
    blober: Pubkey,
) -> impl FnMut(&solana_sdk::instruction::CompiledInstruction) -> Option<Pubkey> + '_ {
    move |instruction| {
        let is_blober_instruction = instruction.program_id(account_keys) == &blober::id();
        let discriminator = blober::instruction::FinalizeBlob::DISCRIMINATOR;
        let has_blober_discriminator =
            instruction.data.get(..discriminator.len()) == Some(discriminator);
        let first_account_address = instruction
            .accounts
            .first()
            .and_then(|lookup_account_index| account_keys.get((*lookup_account_index) as usize));
        let second_account_address = instruction
            .accounts
            .get(1)
            .and_then(|lookup_account_index| account_keys.get((*lookup_account_index) as usize));
        if is_blober_instruction
            && has_blober_discriminator
            && second_account_address == Some(&blober)
        {
            first_account_address.copied()
        } else {
            None
        }
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

/// Splits a blob of data into chunks of size `[Blober::CHUNK_SIZE]`.
pub(crate) fn split_blob_into_chunks(data: &[u8]) -> Vec<(u16, &[u8])> {
    data.chunks(CHUNK_SIZE as usize)
        .enumerate()
        .map(|(i, chunk)| (i as u16, chunk))
        .collect::<Vec<_>>()
}

pub(crate) fn check_outcomes(
    outcomes: Vec<TransactionOutcome<TransactionType>>,
) -> Result<Vec<SuccessfulTransaction<TransactionType>>, UploadBlobError> {
    if outcomes.iter().all(|o| o.successful()) {
        let successful_transactions = outcomes
            .into_iter()
            .filter_map(TransactionOutcome::into_successful)
            .collect();
        Ok(successful_transactions)
    } else {
        Err(UploadBlobError::Transactions(outcomes))
    }
}
