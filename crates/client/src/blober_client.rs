use std::{sync::Arc, time::Duration};

use anchor_lang::{Discriminator, Space};
use blober::{find_blob_address, state::blober::Blober, CHUNK_SIZE};
use jsonrpsee::ws_client::WsClient;
use nitro_da_indexer_api::{CompoundProof, IndexerRpcClient};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::RpcBlockConfig;
use solana_sdk::{
    hash::Hash, pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction,
};
use solana_transaction_status::{EncodedConfirmedBlock, UiTransactionEncoding};
use tracing::Instrument;

use crate::{
    batch_client::{BatchClient, SuccessfulTransaction},
    fees::{Fee, FeeStrategy, Lamports, Priority},
    helpers::{
        find_finalize_blob_transactions_for_blober, get_unique_timestamp, split_blob_into_chunks,
    },
    tx,
    tx::set_compute_unit_price::calculate_compute_unit_price,
    types::{IndexerError, TransactionType, UploadBlobError},
    Error,
};

pub struct BloberClient {
    pub(crate) payer: Arc<Keypair>,
    pub(crate) rpc_client: Arc<RpcClient>,
    pub(crate) batch_client: BatchClient,
    // Optional for the sake of testing, because in some tests indexer client is not used
    pub(crate) indexer_client: Option<Arc<WsClient>>,
}

impl BloberClient {
    /// Creates a new `BloberClient` with the given payer and RPC client.
    pub fn new(
        payer: Arc<Keypair>,
        rpc_client: Arc<RpcClient>,
        batch_client: BatchClient,
        indexer_client: Arc<WsClient>,
    ) -> Self {
        Self {
            payer,
            rpc_client,
            batch_client,
            indexer_client: Some(indexer_client),
        }
    }

    pub async fn upload_blob(
        &self,
        blob_data: &[u8],
        fee_strategy: FeeStrategy,
        blober: Pubkey, // blober program id
        timeout: Option<Duration>,
    ) -> Result<Vec<SuccessfulTransaction<TransactionType>>, UploadBlobError> {
        let timestamp = get_unique_timestamp();

        let blob = find_blob_address(self.payer.pubkey(), timestamp);

        // Convert priority-based fee strategy to a fixed fee by calculating once up-front.
        let fee_strategy = self
            .convert_fee_strategy_to_fixed(fee_strategy, blob)
            .in_current_span()
            .await?;

        let chunks = split_blob_into_chunks(blob_data);

        let (declare_blob_msg, insert_chunks_msgs, finalize_blob_msg) = self
            .generate_messages(blob, timestamp, chunks, fee_strategy, blober)
            .await;

        let res = self
            .do_upload(
                declare_blob_msg,
                insert_chunks_msgs,
                finalize_blob_msg,
                timeout,
            )
            .in_current_span()
            .await;

        if let Err((true, err)) = res {
            // Client errors are not cloneable, and they need to be for the map_err calls to work.
            let err = Arc::new(err);
            // Last attempt to close the blob account.
            let msg = tx::discard_blob(&self.rpc_client, &self.payer, blob, blober, fee_strategy)
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

    pub async fn estimate_fees(&self, blob_size: usize, priority: Priority) -> Result<Fee, Error> {
        // This whole functions is basically a simulation that doesn't run anything. Instead of executing transactions,
        // it just sums the expected fees and number of signatures.

        // The blob account is always newly created, so for estimating compute fees
        // we don't even need the real keypair, any unused pubkey will do.
        let fake_pubkey = Keypair::new().pubkey();
        let prioritization_fee_rate = calculate_compute_unit_price(
            &self.rpc_client,
            &[fake_pubkey, self.payer.pubkey()],
            priority,
        )
        .await?;
        let mut compute_unit_limit = 0u32;
        let mut num_signatures = 0u16;

        compute_unit_limit += tx::declare_blob::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::declare_blob::NUM_SIGNATURES as u16;

        let num_chunks = blob_size.div_ceil(CHUNK_SIZE as usize) as u16;
        compute_unit_limit += num_chunks as u32 * tx::insert_chunk::COMPUTE_UNIT_LIMIT;
        num_signatures += num_chunks * tx::insert_chunk::NUM_SIGNATURES as u16;

        compute_unit_limit += tx::finalize_blob::COMPUTE_UNIT_LIMIT;
        num_signatures += tx::finalize_blob::NUM_SIGNATURES as u16;

        Ok(Fee {
            num_signatures,
            // The base Solana transaction fee = 5000.
            // Reference link: https://solana.com/docs/core/fees#:~:text=While%20transaction%20fees%20are%20paid,of%205k%20lamports%20per%20signature.
            price_per_signature: Lamports::new(5000),
            compute_unit_limit,
            prioritization_fee_rate,
            blob_account_size: Blober::DISCRIMINATOR.len() + Blober::INIT_SPACE,
        })
    }

    /// Fetches all blobs for a given slot.
    pub async fn get_blobs(&self, slot: u64, blober: Pubkey) -> Result<Vec<Vec<u8>>, IndexerError> {
        let blobs = async move {
            loop {
                let blobs = self.indexer().get_blobs(blober, slot).await?;
                if let Some(blobs) = blobs {
                    return Ok(blobs);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        .await
        .map_err(|e: jsonrpsee::core::ClientError| {
            IndexerError::Blobs(format!(
                "Error when retrieving blobs for slot {}: {e:?}",
                slot
            ))
        })?;

        Ok(blobs)
    }

    /// Fetches compound proof for a given slot.
    pub async fn get_slot_proof(&self, slot: u64) -> Result<CompoundProof, IndexerError> {
        let proof = async move {
            loop {
                let proof = self.indexer().get_proof(blober::id(), slot).await?;
                if let Some(proofs) = proof {
                    return Ok(proofs);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        .await
        .map_err(|e: jsonrpsee::core::ClientError| {
            IndexerError::Proof(format!(
                "Error when retrieving proof for slot {}: {e:?}",
                slot
            ))
        })?;

        Ok(proof)
    }

    /// Fetches blob proofs for a given slot
    pub async fn get_blob_hashes(&self, slot: u64, blober: Pubkey) -> Result<Vec<Hash>, Error> {
        let block: EncodedConfirmedBlock = self
            .rpc_client
            .get_block_with_config(
                slot,
                RpcBlockConfig {
                    commitment: Some(self.rpc_client.commitment()),
                    encoding: Some(UiTransactionEncoding::Base58),
                    ..Default::default()
                },
            )
            .await?
            .into();

        // Directly pass the closure returned by the function
        let messages = block
            .transactions
            .iter()
            .filter_map(find_finalize_blob_transactions_for_blober(blober))
            .collect::<Vec<_>>();

        let hashes = messages
            .into_iter()
            .map(|(_blob, message)| message.hash())
            .collect();
        Ok(hashes)
    }
}

#[cfg(test)]
pub mod tests {
    use std::time::SystemTime;

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
    use crate::{batch_client, BloberClient, ErrorKind};

    #[tokio::test]
    async fn full_workflow_mock() {
        let client = Arc::new(RpcClient::new_sender(
            MockBlockSender {
                sender: MockSender::new("succeeds".to_string()),
                initial_time: Instant::now(),
            },
            RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
        ));
        full_workflow(client).await;
    }

    #[tokio::test]
    async fn full_workflow_unreliable_client() {
        // Pass a bad client for blob uploads.
        let bad_client = Arc::new(RpcClient::new_sender(
            UnreliableSender(MockBlockSender {
                sender: MockSender::new("succeeds".to_string()),
                initial_time: Instant::now(),
            }),
            RpcClientConfig::default(),
        ));
        full_workflow(bad_client).await;
    }

    #[tokio::test]
    #[ignore = "Running this test requires a local Solana cluster to be running"]
    async fn full_workflow_localnet() {
        let client = Arc::new(RpcClient::new_with_commitment(
            "http://127.0.0.1:8899".to_string(),
            CommitmentConfig::confirmed(),
        ));
        full_workflow(client).await;
    }

    async fn full_workflow(blober_rpc_client: Arc<RpcClient>) {
        let payer = Arc::new(Keypair::new());
        blober_rpc_client
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
            balance_before = blober_rpc_client
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

        // Better to initialize blober program and return its public key here
        let blober = Keypair::new();
        let blober_pubkey = blober.pubkey();

        let batch_client = BatchClient::new(blober_rpc_client.clone(), None, vec![payer.clone()])
            .await
            .unwrap();
        let blober_client = BloberClient {
            payer: payer.clone(),
            rpc_client: blober_rpc_client.clone(),
            batch_client,
            indexer_client: None,
        };

        // Useful for spotting the blob data in the transaction ledger.
        let data: Vec<u8> = [0xDE, 0xAD, 0xBE, 0xEF]
            .into_iter()
            .cycle()
            .take(200 * 1024)
            .collect::<Vec<_>>();

        // Retry in case of unreliable client
        let expected_fee = loop {
            let res = blober_client.estimate_fees(data.len(), priority).await;
            if let Ok(fee) = res {
                break fee;
            }
        };

        blober_client
            .upload_blob(
                &data,
                fee_strategy,
                blober_pubkey,
                Some(Duration::from_secs(20)),
            )
            .await
            .unwrap();

        // The mock client always reports a balance of 50 lamports, so no meaningful assertions are possible.
        if balance_before != 50 {
            let balance_after = blober_rpc_client
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
        // Give a successful RPC client to the BloberClient to allow other calls to succeed.
        let blober_client = BloberClient {
            payer: payer.clone(),
            rpc_client: successful_rpc_client.clone(),
            batch_client,
            indexer_client: None,
        };

        // Useful for spotting the blob data in the transaction ledger.
        let data: Vec<u8> = [0xDE, 0xAD, 0xBE, 0xEF]
            .into_iter()
            .cycle()
            .take(10 * 1024)
            .collect::<Vec<_>>();

        let err = blober_client
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
