use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use data_anchor_blober::find_blober_address;
use itertools::Itertools;
use rand::Rng;
use solana_client::{
    client_error::{ClientError as Error, ClientErrorKind as ErrorKind},
    nonblocking::rpc_client::RpcClient,
    rpc_response::{RpcBlockhash, RpcResponseContext},
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
    hash::Hash, native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, signature::Keypair, signer::Signer,
};
use solana_transaction_status::TransactionStatus;
use tokio::time::Instant;

use crate::{
    batch_client, helpers::get_unique_timestamp, BatchClient, DataAnchorClient, FeeStrategy,
    Priority,
};

#[tokio::test]
async fn full_workflow_mock() {
    let client = Arc::new(RpcClient::new_sender(
        MockBlockSender {
            sender: MockSender::new("succeeds".to_string()),
            initial_time: Instant::now(),
        },
        RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
    ));
    full_workflow(client, false).await;
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
    full_workflow(bad_client, false).await;
}

#[tokio::test]
#[ignore = "Running this test requires a local Solana cluster to be running"]
async fn full_workflow_localnet() {
    let client = Arc::new(RpcClient::new_with_commitment(
        "http://127.0.0.1:8899".to_string(),
        CommitmentConfig::confirmed(),
    ));
    full_workflow(client, true).await;
}

async fn full_workflow(blober_rpc_client: Arc<RpcClient>, check_ledger: bool) {
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

    let priority = Priority::default();
    let fee_strategy = FeeStrategy::BasedOnRecentFees(priority);

    let batch_client = BatchClient::new(blober_rpc_client.clone(), vec![payer.clone()])
        .await
        .unwrap();
    let data_anchor_client = DataAnchorClient::builder()
        .payer(payer.clone())
        .program_id(data_anchor_blober::id())
        .rpc_client(blober_rpc_client.clone())
        .batch_client(batch_client)
        .build();

    let namespace = "test".to_owned();
    let blober_pubkey = find_blober_address(data_anchor_blober::id(), payer.pubkey(), &namespace);
    data_anchor_client
        .initialize_blober(fee_strategy, &namespace, Some(Duration::from_secs(5)))
        .await
        .unwrap();

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

    // Useful for spotting the blob data in the transaction ledger.
    let data: Vec<u8> = [0xDE, 0xAD, 0xBE, 0xEF]
        .into_iter()
        .cycle()
        .take(200 * 1024)
        .collect::<Vec<_>>();

    // Retry in case of unreliable client
    let expected_fee = loop {
        let res = data_anchor_client
            .estimate_fees(data.len(), blober_pubkey, priority)
            .await;
        if let Ok(fee) = res {
            break fee;
        }
    };

    let slot_before_upload = data_anchor_client
        .rpc_client
        .get_slot_with_commitment(CommitmentConfig::confirmed())
        .await
        .unwrap();

    let result = data_anchor_client
        .upload_blob(
            &data,
            fee_strategy,
            &namespace,
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
        assert!(
            // The fee is not exact, but should be within 1000 lamports.
            balance_after.abs_diff(balance_before - expected_fee.total_fee().into_inner() as u64)
                < 1000,
        );
    }

    if !check_ledger {
        return;
    }

    let signatures = result.iter().map(|r| r.signature).collect::<Vec<_>>();

    let ledger_data = data_anchor_client
        .get_ledger_blobs_from_signatures(&namespace, None, signatures)
        .await
        .unwrap();

    assert_eq!(data, ledger_data);

    let finalized_slot = result.last().unwrap().slot;

    let all_ledger_blobs = data_anchor_client
        .get_ledger_blobs(
            finalized_slot,
            &namespace,
            None,
            Some(finalized_slot - slot_before_upload + 1),
        )
        .await
        .unwrap();

    assert_eq!(vec![data], all_ledger_blobs);
}

#[tokio::test]
async fn failing_upload_returns_error() {
    let payer = Arc::new(Keypair::new());
    let successful_rpc_client = Arc::new(RpcClient::new_mock("success".to_string()));
    let failing_rpc_client = Arc::new(RpcClient::new_mock("instruction_error".to_string()));

    // Give a failing RPC client to the Batch and TPU clients, so uploads will fail.
    let batch_client =
        batch_client::BatchClient::new(failing_rpc_client.clone(), vec![payer.clone()])
            .await
            .unwrap();
    // Give a successful RPC client to the DataAnchorClient to allow other calls to succeed.
    let data_anchor_client = DataAnchorClient::builder()
        .payer(payer)
        .program_id(Pubkey::new_unique())
        .rpc_client(successful_rpc_client.clone())
        .batch_client(batch_client)
        .build();

    // Useful for spotting the blob data in the transaction ledger.
    let data: Vec<u8> = [0xDE, 0xAD, 0xBE, 0xEF]
        .into_iter()
        .cycle()
        .take(10 * 1024)
        .collect::<Vec<_>>();

    let err = data_anchor_client
        .upload_blob(
            &data,
            FeeStrategy::default(),
            "test",
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
            // Always let airdrops, balance checks and slot queries through, since those
            // are used in the test setup itself.
            RpcRequest::RequestAirdrop | RpcRequest::GetBalance | RpcRequest::GetSlot => 0.0,
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
    let mut threads = Vec::new();
    for _ in 0..100 {
        threads.push(std::thread::spawn(|| {
            let mut timestamps = Vec::new();
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
