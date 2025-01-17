use std::{
    cmp::max,
    fmt::Display,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

use anchor_lang::{prelude::Pubkey, Discriminator, Space};
use chunker::{
    find_chunker_address,
    state::chunker::{Chunker, CHUNK_SIZE},
};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
pub use solana_rpc_client_api::client_error::Error;
use solana_sdk::{message::Message, signature::Keypair, signer::Signer, transaction::Transaction};
use thiserror::Error;
use tracing::{info_span, instrument, Instrument, Span};

use crate::{
    fees::{Fee, FeeStrategy, Lamports},
    tx,
    tx::calculate_compute_unit_price,
    BatchClient, Priority, SuccessfulTransaction, TransactionOutcome,
};

/// A client for the Chunker program. This client can be used to upload blobs to chunker accounts.
#[derive(Clone)]
pub struct ChunkerClient {
    payer: Arc<Keypair>,
    rpc_client: Arc<RpcClient>,
    batch_client: BatchClient,
}

/// An error that can occur when uploading a blob to a chunker account.
#[derive(Error, Debug)]
pub enum UploadBlobError {
    #[error("Failed to retrieve recent priority fees. Error: {0}")]
    PriorityFees(#[source] Error),
    #[error(
        "Failed when sending transactions. Transaction errors:\n{}",
        .0.iter().filter_map(TransactionOutcome::error).map(|t| format!("- {}: {}", t.data, t.error)).collect::<Vec<_>>().join("\n")
    )]
    Transactions(Vec<TransactionOutcome<TransactionType>>),
    #[error("Failed to force close the chunker. Original error: {0}\n\nClose error: {1}")]
    CloseAccount(#[source] Arc<UploadBlobError>, Error),
}

impl UploadBlobError {
    pub fn client_errors(&self) -> Vec<&Error> {
        match self {
            UploadBlobError::PriorityFees(e) => vec![e],
            UploadBlobError::Transactions(outcomes) => outcomes
                .iter()
                .filter_map(TransactionOutcome::error)
                .map(|t| &t.error)
                .collect(),
            UploadBlobError::CloseAccount(e1, e2) => {
                e1.client_errors().into_iter().chain([e2]).collect()
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TransactionType {
    CreateChunker,
    InsertChunk(u16),
    CompleteChunker,
}

impl Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionType::CreateChunker => write!(f, "CreateChunker"),
            TransactionType::InsertChunk(i) => write!(f, "InsertChunk {}", i),
            TransactionType::CompleteChunker => write!(f, "CompleteChunker"),
        }
    }
}

impl ChunkerClient {
    /// Creates a new `ChunkerClient` with the given payer and RPC client.
    ///
    /// # Arguments
    /// - `payer`: The payer for all transactions sent by the client.
    /// - `client`: The Solana RPC client to use when sending transactions.
    pub fn new(payer: Arc<Keypair>, rpc_client: Arc<RpcClient>, batch_client: BatchClient) -> Self {
        Self {
            payer,
            rpc_client,
            batch_client,
        }
    }

    /// Estimates the fees for creating a chunker account with the given blob size and priority.
    ///
    /// # Arguments
    /// - `blob_size`: The size of the blob to store in the chunker.
    /// - `priority`: The priority of the transactions. Higher priority transactions are more likely to be included in a block.
    ///
    /// # Returns
    /// The estimated fees for creating the chunker account, broken down by source.
    pub async fn estimate_fees(&self, blob_size: usize, priority: Priority) -> Result<Fee, Error> {
        // This whole functions is basically a simulation that doesn't run anything. Instead of executing transactions,
        // it just sums the expected fees and number of signatures.

        // The chunker account is always newly created, so for estimating compute fees
        // we don't even need the real keypair, any unused pubkey will do.
        let fake_pubkey = Keypair::new().pubkey();
        let prioritization_fee_rate = tx::calculate_compute_unit_price(
            &self.rpc_client,
            &[fake_pubkey, self.payer.pubkey()],
            priority,
        )
        .await?;
        let mut compute_unit_limit = 0u32;
        let mut num_signatures = 0u16;

        compute_unit_limit += tx::create_chunker::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::create_chunker::NUM_SIGNATURES as u16;

        let num_chunks = blob_size.div_ceil(CHUNK_SIZE as usize) as u16;
        compute_unit_limit += num_chunks as u32 * tx::insert_chunk::COMPUTE_UNIT_LIMIT;
        num_signatures += num_chunks * tx::insert_chunk::NUM_SIGNATURES as u16;

        compute_unit_limit += tx::complete_chunker::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::complete_chunker::NUM_SIGNATURES as u16;

        Ok(Fee {
            num_signatures,
            price_per_signature: Lamports::new(5000),
            compute_unit_limit,
            prioritization_fee_rate,
            chunker_account_size: Chunker::DISCRIMINATOR.len() + Chunker::INIT_SPACE,
        })
    }

    /// Uploads a blob to the Solana blockchain.
    ///
    /// The upload process consists of the following steps:
    /// 1. Create a chunker account.
    /// 2. Insert chunks into the chunker account.
    /// 3. Complete the chunker account. This will also trigger the hasher to hash the current
    ///    state of the chunker account.
    ///
    /// If any of the transactions fail, they will be retried repeatedly. If a timeout is provided
    /// and it is reached, the client will still attempt to force close the chunker account once.
    ///
    /// # Arguments
    /// - `data`: The blob to upload.
    /// - `fee_strategy`: The strategy to use for calculating the fees for transactions.
    /// - `hasher_account`: The public key of the hasher account to use for hashing the state of
    ///                     the chunker account.
    /// - `timeout`: The maximum time to wait for the transactions to be confirmed.
    ///              If `None`, the client will wait indefinitely.
    ///
    /// # Returns
    /// A list of the signatures for all the transactions used to upload the blob.
    ///
    /// # Errors
    /// If any transaction fails, the `UploadBlobError` will contain information useful for debugging
    /// and potentially reclaiming the funds from the chunker account.
    #[instrument(skip_all, fields(chunker_pubkey, batch))]
    pub async fn upload_blob(
        &self,
        data: &[u8],
        fee_strategy: FeeStrategy,
        hasher_account: Pubkey,
        timeout: Option<Duration>,
    ) -> Result<Vec<SuccessfulTransaction<TransactionType>>, UploadBlobError> {
        // Get a unique timestamp to use as the seed for the Chunker account, this will result in a
        // new address for every upload; even during parallel uploads.
        let timestamp = get_unique_timestamp();
        let chunker = find_chunker_address(self.payer.pubkey(), timestamp);
        Span::current().record("chunker_pubkey", chunker.to_string());
        let chunks = split_blob_into_chunks(data);

        // Convert priority-based fee strategy to a fixed fee by calculating once up-front.
        let fee_strategy = self
            .convert_fee_strategy_to_fixed(fee_strategy, chunker)
            .in_current_span()
            .await?;

        let (create_msg, insert_msgs, complete_msg) = self
            .generate_messages(chunker, timestamp, chunks, fee_strategy, hasher_account)
            .await;

        let res = self
            .do_upload(create_msg, insert_msgs, complete_msg, timeout)
            .in_current_span()
            .await;
        if let Err((true, err)) = res {
            // Client errors are not cloneable, and they need to be for the map_err calls to work.
            let err = Arc::new(err);
            // Last attempt to close the account.
            let msg = tx::force_close_chunker(&self.rpc_client, &self.payer, chunker, fee_strategy)
                .in_current_span()
                .await
                .expect("infallible with a fixed fee strategy");
            let blockhash = self
                .rpc_client
                .get_latest_blockhash()
                .in_current_span()
                .await
                .map_err(|err2| UploadBlobError::CloseAccount(err.clone(), err2))?;
            let tx = Transaction::new(&[&self.payer], msg, blockhash);
            self.rpc_client
                .send_and_confirm_transaction(&tx)
                .in_current_span()
                .await
                .map_err(|err2| UploadBlobError::CloseAccount(err.clone(), err2))?;
            Err(Arc::into_inner(err).expect("only one handle to the error"))
        } else {
            res.map_err(|(_, err)| err)
        }
    }

    /// Uploads the blob in three steps: Create, Insert * N, Complete. Returns a vec of successful
    /// transactions if everything succeeds, or a tuple of a bool and an UploadBlobError if
    /// something fails. The boolean value indicates whether the Create step succeeded, and thus
    /// whether the chunker account exists.
    async fn do_upload(
        &self,
        create_chunker: (TransactionType, Message),
        inserts: Vec<(TransactionType, Message)>,
        complete_msg: (TransactionType, Message),
        timeout: Option<Duration>,
    ) -> Result<Vec<SuccessfulTransaction<TransactionType>>, (bool, UploadBlobError)> {
        let before = Instant::now();

        let span = info_span!(parent: Span::current(), "create_chunker");
        let tx1 = check_outcomes(
            self.batch_client
                .send(vec![create_chunker], timeout)
                .instrument(span)
                .await,
        )
        .map_err(|err| (false, err))?;

        let span = info_span!(parent: Span::current(), "insert_chunks");
        let timeout = timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
        let tx2 = check_outcomes(
            self.batch_client
                .send(inserts, timeout)
                .instrument(span)
                .await,
        )
        .map_err(|err| (true, err))?;

        let span = info_span!(parent: Span::current(), "complete_chunker");
        let timeout = timeout.map(|timeout| timeout.saturating_sub(Instant::now() - before));
        let tx3 = check_outcomes(
            self.batch_client
                .send(vec![complete_msg], timeout)
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

    async fn generate_messages(
        &self,
        chunker: Pubkey,
        timestamp: u64,
        chunks: Vec<(u16, &[u8])>,
        fee_strategy: FeeStrategy,
        hasher_account: Pubkey,
    ) -> (
        (TransactionType, solana_sdk::message::Message),
        Vec<(TransactionType, solana_sdk::message::Message)>,
        (TransactionType, solana_sdk::message::Message),
    ) {
        let blob_size = chunks.iter().map(|(_, chunk)| chunk.len() as u32).sum();
        let create_msg = (
            TransactionType::CreateChunker,
            tx::create_chunker(
                &self.rpc_client,
                &self.payer,
                chunker,
                timestamp,
                blob_size,
                chunks.len() as u16,
                fee_strategy,
            )
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy"),
        );

        let mut insert_msgs = vec![];
        for (chunk_index, chunk_data) in chunks.iter() {
            let insert_tx = tx::insert_chunk(
                &self.rpc_client,
                &self.payer,
                chunker,
                *chunk_index,
                chunk_data.to_vec(),
                fee_strategy,
            )
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy");
            insert_msgs.push((TransactionType::InsertChunk(*chunk_index), insert_tx));
        }

        let complete_msg = (
            TransactionType::CompleteChunker,
            tx::complete_chunker(
                &self.rpc_client,
                &self.payer,
                chunker,
                hasher_account,
                fee_strategy,
            )
            .in_current_span()
            .await
            .expect("infallible with a fixed fee strategy"),
        );

        (create_msg, insert_msgs, complete_msg)
    }

    async fn convert_fee_strategy_to_fixed(
        &self,
        fee_strategy: FeeStrategy,
        chunker: Pubkey,
    ) -> Result<FeeStrategy, UploadBlobError> {
        Ok(match fee_strategy {
            FeeStrategy::Fixed(_) => fee_strategy,
            FeeStrategy::BasedOnRecentFees(priority) => {
                let mut fee_retries = 5;
                loop {
                    let res = calculate_compute_unit_price(
                        &self.rpc_client,
                        &[chunker, self.payer.pubkey()],
                        priority,
                    )
                    .in_current_span()
                    .await;
                    match res {
                        Ok(fee) => {
                            return Ok(FeeStrategy::Fixed(Fee {
                                prioritization_fee_rate: fee,
                                // The other fields are not used here.
                                num_signatures: 0,
                                price_per_signature: Lamports::ZERO,
                                compute_unit_limit: 0,
                                chunker_account_size: 0,
                            }));
                        }
                        Err(e) => {
                            fee_retries -= 1;
                            if fee_retries == 0 {
                                return Err(UploadBlobError::PriorityFees(e));
                            }
                        }
                    };
                }
            }
        })
    }
}

/// Retrieves a "timestamp" that is guaranteed to be unique. This is accomplished by not *really*
/// using the timestamp, but instead keeping track of the last used timestamp and incrementing it
/// by one if it is equal to the real timestamp. In most scenarios this will mean the returned value
/// will be equal to the system time, but during heavy thread contention the value may be in the
/// future. Regardless, it will always be unique.
///
/// This function should also be safe to use even if the system time is changed while the application
/// is running. If the application is shut down, the system time changed to the past, and then the
/// application is restarted, there will be duplicate timestamps.
///
/// This function will also return unique timestamps across multiple ChunkerClient instances.
fn get_unique_timestamp() -> u64 {
    static LAST_USED_TIMESTAMP: AtomicU64 = AtomicU64::new(0);

    // Load the currently stored value to start off the loop.
    let mut last_used_timestamp = LAST_USED_TIMESTAMP.load(Ordering::Relaxed);
    loop {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time to move forwards")
            .as_secs();
        // If the current time is ahead of the last used time, use that. If not, the next available
        // timestamp will be the last used timestamp + 1.
        let timestamp = max(now, last_used_timestamp + 1);
        // Try to store the new timestamp. It will only succeed if the last used timestamp is still
        // the same as the one that was previously loaded.
        match LAST_USED_TIMESTAMP.compare_exchange_weak(
            last_used_timestamp,
            timestamp,
            // Relaxed is fine as this code only touches a single atomic variable.
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            // The store was successful, so the value is unique.
            Ok(_) => return timestamp,
            // The store failed, another thread must have updated it.
            // Set the new value as the last used timestamp and try again.
            Err(new_timestamp) => last_used_timestamp = new_timestamp,
        }
    }
}

fn check_outcomes(
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

/// Splits a blob of data into chunks of size `[Chunker::CHUNK_SIZE]`.
fn split_blob_into_chunks(data: &[u8]) -> Vec<(u16, &[u8])> {
    data.chunks(CHUNK_SIZE as usize)
        .enumerate()
        .map(|(i, chunk)| (i as u16, chunk))
        .collect::<Vec<_>>()
}

#[cfg(test)]
pub mod tests {

    use async_trait::async_trait;
    use itertools::Itertools;
    use rand::Rng;
    use solana_client::{
        nonblocking::tpu_client::TpuClient,
        rpc_response::{RpcBlockhash, RpcResponseContext},
        tpu_client::TpuClientConfig,
    };
    use solana_rpc_client::{
        mock_sender::MockSender,
        rpc_client::RpcClientConfig,
        rpc_sender::{RpcSender, RpcTransportStats},
    };
    use solana_rpc_client_api::{
        config::RpcRequestAirdropConfig, request::RpcRequest, response::Response,
    };
    use solana_sdk::{
        clock::DEFAULT_MS_PER_SLOT, commitment_config::CommitmentConfig, epoch_info::EpochInfo,
        hash::Hash, native_token::LAMPORTS_PER_SOL,
    };
    use solana_transaction_status::TransactionStatus;
    use tokio::time::Instant;

    use super::*;
    use crate::{batch_client, ErrorKind, HasherClient};

    #[tokio::test]
    async fn full_workflow_mock() {
        let client = Arc::new(RpcClient::new_sender(
            MockBlockSender {
                sender: MockSender::new("succeeds".to_string()),
                initial_time: Instant::now(),
            },
            RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
        ));
        full_workflow(client.clone(), client).await;
    }

    #[tokio::test]
    async fn full_workflow_unreliable_client() {
        // Pass a good client for hasher creation, and a bad client for chunker uploads.
        let good_client = Arc::new(RpcClient::new_mock("succeeds".to_string()));
        let bad_client = Arc::new(RpcClient::new_sender(
            UnreliableSender(MockBlockSender {
                sender: MockSender::new("succeeds".to_string()),
                initial_time: Instant::now(),
            }),
            RpcClientConfig::default(),
        ));
        full_workflow(good_client, bad_client).await;
    }

    #[tokio::test]
    #[ignore = "Running this test requires a local Solana cluster to be running"]
    async fn full_workflow_localnet() {
        let client = Arc::new(RpcClient::new_with_commitment(
            "http://127.0.0.1:8899".to_string(),
            CommitmentConfig::confirmed(),
        ));
        full_workflow(client.clone(), client).await;
    }

    async fn create_test_hasher(client: Arc<RpcClient>) -> Pubkey {
        // Use a different payer to not count the setup fees in the main test.
        let payer = Keypair::new();
        client
            .request_airdrop_with_config(
                &payer.pubkey(),
                10 * LAMPORTS_PER_SOL,
                RpcRequestAirdropConfig {
                    commitment: Some(CommitmentConfig::finalized()),
                    ..RpcRequestAirdropConfig::default()
                },
            )
            .await
            .unwrap();
        print!("Airdropping 10 SOL");
        let mut balance = 0;
        while balance == 0 {
            balance = client.get_balance(&payer.pubkey()).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            print!(".");
        }
        println!();

        let hasher_client = HasherClient::new(Arc::new(payer), client);
        hasher_client
            .create_hasher(None, FeeStrategy::Fixed(Fee::ZERO))
            .await
            .unwrap()
            .pubkey()
    }

    async fn full_workflow(hasher_rpc_client: Arc<RpcClient>, chunker_rpc_client: Arc<RpcClient>) {
        let payer = Arc::new(Keypair::new());
        chunker_rpc_client
            .request_airdrop_with_config(
                &payer.pubkey(),
                10 * LAMPORTS_PER_SOL,
                RpcRequestAirdropConfig {
                    commitment: Some(CommitmentConfig::finalized()),
                    ..RpcRequestAirdropConfig::default()
                },
            )
            .await
            .unwrap();
        print!("Airdropping 10 SOL");
        let mut balance_before = 0;
        while balance_before == 0 {
            balance_before = chunker_rpc_client
                .get_balance(&payer.pubkey())
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            print!(".");
        }
        println!();

        println!(
            "Balance for wallet {}: {} SOL",
            payer.pubkey(),
            balance_before / LAMPORTS_PER_SOL
        );

        let priority = Priority::default();
        let fee_strategy = FeeStrategy::BasedOnRecentFees(priority);

        // Normally, this would be a well-known public key, but for the purposes of this test we
        // have to create it ourselves.
        let hasher_account = create_test_hasher(hasher_rpc_client).await;

        let batch_client = BatchClient::new(chunker_rpc_client.clone(), None, vec![payer.clone()])
            .await
            .unwrap();
        let chunker_client =
            ChunkerClient::new(payer.clone(), chunker_rpc_client.clone(), batch_client);

        // Useful for spotting the blob data in the transaction ledger.
        let data: Vec<u8> = [0xDE, 0xAD, 0xBE, 0xEF]
            .into_iter()
            .cycle()
            .take(200 * 1024)
            .collect::<Vec<_>>();

        // Retry in case of unreliable client
        let expected_fee = loop {
            let res = chunker_client.estimate_fees(data.len(), priority).await;
            if let Ok(fee) = res {
                break fee;
            }
        };

        chunker_client
            .upload_blob(
                &data,
                fee_strategy,
                hasher_account,
                Some(Duration::from_secs(20)),
            )
            .await
            .unwrap();

        // The mock client always reports a balance of 50 lamports, so no meaningful assertions are possible.
        if balance_before != 50 {
            let balance_after = chunker_rpc_client
                .get_balance(&payer.pubkey())
                .await
                .unwrap();
            println!(
                "Balance before: {} lamports, balance after: {} lamports, expected fee was: {}",
                balance_before,
                balance_after,
                expected_fee.total_fee()
            );
            assert_eq!(
                balance_after,
                balance_before - expected_fee.total_fee().into_inner() as u64
            );
        }
    }

    #[tokio::test]
    async fn failing_upload_returns_error() {
        let payer = Arc::new(Keypair::new());
        let successful_rpc_client = Arc::new(RpcClient::new_mock("success".to_string()));
        let failing_rpc_client = Arc::new(RpcClient::new_mock("instruction_error".to_string()));

        // Give a failing RPC client to the Batch and TPU clients, so uploads will fail.
        let tpu_client = Arc::new(
            TpuClient::new(
                "test",
                failing_rpc_client.clone(),
                "",
                TpuClientConfig::default(),
            )
            .await
            .unwrap(),
        );
        let batch_client = batch_client::BatchClient::new(
            failing_rpc_client.clone(),
            Some(tpu_client),
            vec![payer.clone()],
        )
        .await
        .unwrap();
        // Give a successful RPC client to the ChunkerClient to allow other calls to succeed.
        let chunker_client =
            ChunkerClient::new(payer.clone(), successful_rpc_client.clone(), batch_client);

        // Useful for spotting the blob data in the transaction ledger.
        let data: Vec<u8> = [0xDE, 0xAD, 0xBE, 0xEF]
            .into_iter()
            .cycle()
            .take(10 * 1024)
            .collect::<Vec<_>>();

        let err = chunker_client
            .upload_blob(
                &data,
                FeeStrategy::default(),
                Pubkey::new_unique(),
                Some(Duration::from_secs(5)),
            )
            .await
            .unwrap_err();
        println!("{err:#?}");
    }
    // The default MockSender always returns the same value for get_last_blockhash and
    // get_epoch_info, so we wrap that in a bit more logic.
    struct MockBlockSender {
        sender: MockSender,
        initial_time: Instant,
    }

    #[async_trait]
    impl RpcSender for MockBlockSender {
        async fn send(
            &self,
            request: RpcRequest,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, Error> {
            // For this test it's fine to pretend that slots and blocks are the same thing.
            let slot = (Instant::now().duration_since(self.initial_time).as_millis()
                / DEFAULT_MS_PER_SLOT as u128) as u64;
            if let RpcRequest::GetLatestBlockhash = request {
                Ok(serde_json::to_value(Response {
                    context: RpcResponseContext {
                        slot,
                        api_version: None,
                    },
                    value: RpcBlockhash {
                        blockhash: Hash::default().to_string(),
                        last_valid_block_height: slot + 150,
                    },
                })?)
            } else if let RpcRequest::GetEpochInfo = request {
                Ok(serde_json::to_value(EpochInfo {
                    epoch: 0,
                    slot_index: slot,
                    slots_in_epoch: 256,
                    absolute_slot: slot,
                    block_height: slot,
                    transaction_count: Some(123),
                })?)
            } else {
                self.sender.send(request, params).await
            }
        }

        fn get_transport_stats(&self) -> RpcTransportStats {
            self.sender.get_transport_stats()
        }

        fn url(&self) -> String {
            self.sender.url()
        }
    }

    struct UnreliableSender(MockBlockSender);

    #[async_trait]
    impl RpcSender for UnreliableSender {
        async fn send(
            &self,
            request: RpcRequest,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, Error> {
            let failure_rate = match &request {
                // Always let airdrops and balance checks through, since those
                // are used in the test setup itself.
                RpcRequest::RequestAirdrop | RpcRequest::GetBalance => 0.0,
                // This needs special treatment since we want to simulate some of the transactions failing,
                // not the entire request.
                RpcRequest::GetSignatureStatuses => {
                    // Small chance to fail the signature request itself.
                    if rand::thread_rng().gen_bool(0.1) {
                        return Err(Error {
                            request: None,
                            kind: ErrorKind::Custom("failed".to_string()),
                        });
                    }
                    let successful = self.0.send(request, params).await.unwrap();
                    let mut statuses: Response<Vec<Option<TransactionStatus>>> =
                        serde_json::from_value(successful).unwrap();
                    let mut rng = rand::thread_rng();
                    for status in &mut statuses.value {
                        // Even if 50% of transactions fail, the client should still work.
                        // (even higher works too, but the test takes an awfully long time)
                        if rng.gen_bool(0.5) {
                            *status = None;
                        }
                    }
                    return Ok(serde_json::to_value(statuses).unwrap());
                }
                // Any other request can fail rarely.
                _ => 0.1,
            };
            if rand::thread_rng().gen_bool(failure_rate) {
                return Err(Error {
                    request: None,
                    kind: ErrorKind::Custom("failed".to_string()),
                });
            }
            self.0.send(request, params).await
        }

        fn get_transport_stats(&self) -> RpcTransportStats {
            self.0.get_transport_stats()
        }

        fn url(&self) -> String {
            self.0.url()
        }
    }

    #[test]
    fn timestamps_are_unique_under_contention() {
        let mut threads = vec![];
        for _ in 0..100 {
            threads.push(std::thread::spawn(|| {
                let mut timestamps = vec![];
                for _ in 0..1000 {
                    timestamps.push(get_unique_timestamp());
                }
                timestamps
            }));
        }

        let timestamps = threads
            .into_iter()
            .flat_map(|t| t.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(timestamps.len(), timestamps.iter().unique().count());
        let min = timestamps.iter().min().unwrap();
        let max = timestamps.iter().max().unwrap();
        let count = timestamps.len();
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        dbg!(min, max, count, current_time);
    }
}
