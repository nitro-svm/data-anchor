use std::sync::Arc;

use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::clock::DEFAULT_MS_PER_SLOT;
use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{sleep, Duration},
};
use tracing::warn;

use super::super::messages::BlockMessage;

/// Spawns an independent task that periodically checks the latest blockhash and epoch info using
/// the Solana RPC client, and broadcasts it as a [`BlockMessage`] on the given channel.
///
/// The task will exit if there are no receivers alive. This will happen when the
/// [transaction confirmer](`super::transaction_confirmer::spawn_transaction_confirmer`) and
/// [transaction sender](`super::transaction_sender::spawn_transaction_sender`) tasks have both exited.
pub fn spawn_block_watcher(
    blockdata_tx: watch::Sender<BlockMessage>,
    rpc_client: Arc<RpcClient>,
) -> JoinHandle<()> {
    tokio::spawn({
        async move {
            // This will never equal the new slot, so the first update is always broadcast.
            let mut last_update = BlockMessage::default();
            loop {
                sleep(Duration::from_millis(DEFAULT_MS_PER_SLOT)).await;
                let Ok((blockhash, last_valid_block_height)) = rpc_client
                    .get_latest_blockhash_with_commitment(rpc_client.commitment())
                    .await
                else {
                    warn!("failed to get latest blockhash");
                    continue;
                };
                let Ok(epoch_info) = rpc_client
                    .get_epoch_info_with_commitment(rpc_client.commitment())
                    .await
                else {
                    warn!("failed to get epoch info");
                    continue;
                };
                let block_height = epoch_info.block_height;
                let new_update = BlockMessage {
                    blockhash,
                    last_valid_block_height,
                    block_height,
                };
                if new_update != last_update {
                    last_update = new_update;
                    let res = blockdata_tx.send(new_update);
                    if res.is_err() {
                        // If there's no receiver alive, the watcher should shut down.
                        break;
                    }
                } else if blockdata_tx.is_closed() {
                    // Additionally check if the channel is closed even if the new update is unchanged,
                    // mostly to prevent getting stuck in tests.
                    break;
                }
            }
            warn!("shutting down block watcher");
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cmp::min;

    use async_trait::async_trait;
    use solana_client::{
        rpc_client::RpcClientConfig,
        rpc_request::RpcRequest,
        rpc_response::{Response, RpcBlockhash, RpcResponseContext},
        rpc_sender::{RpcSender, RpcTransportStats},
    };
    use solana_rpc_client::mock_sender::MockSender;
    use solana_rpc_client_api::client_error::Result as SolanaResult;
    use solana_sdk::{epoch_info::EpochInfo, hash::Hash};
    use tokio::time::Instant;
    use tracing::Level;

    use super::*;

    /// This is essentially an integration test of the full lifecycle of the block watcher.
    #[tokio::test(start_paused = true)]
    async fn test_block_watcher() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .try_init();

        // Use the paused current time as a reference point so the rest of the test doesn't depend
        // on the current time.
        let initial_time = Instant::now();

        // Dummy initial value to distinguish it from what the MockBlockSender returns.
        let initial_value = BlockMessage {
            blockhash: Hash::default(),
            last_valid_block_height: 1234,
            block_height: 5678,
        };
        let (tx, mut rx) = watch::channel(initial_value);
        let client = Arc::new(RpcClient::new_sender(
            // This sender is implemented below.
            MockBlockSender {
                sender: MockSender::new("succeeds"),
                initial_time,
                max_slot: 3,
            },
            RpcClientConfig::default(),
        ));
        let handle = spawn_block_watcher(tx, client);

        // Checking the value straight away should return the initial value.
        assert_eq!(*rx.borrow_and_update(), initial_value);

        // Checking the value half a slot later should give the same value.
        tokio::time::sleep_until(initial_time + Duration::from_millis(DEFAULT_MS_PER_SLOT / 2))
            .await;
        assert_eq!(*rx.borrow_and_update(), initial_value);

        // Checking the value one slot later (and a bit) should give a new value.
        tokio::time::sleep_until(initial_time + Duration::from_millis(DEFAULT_MS_PER_SLOT + 1))
            .await;
        assert_eq!(
            *rx.borrow_and_update(),
            BlockMessage {
                blockhash: Hash::default(),
                last_valid_block_height: 151,
                block_height: 1
            }
        );

        // Checking the value two slots later should skip the intermediate value.
        tokio::time::sleep_until(initial_time + Duration::from_millis(3 * DEFAULT_MS_PER_SLOT + 1))
            .await;
        assert_eq!(
            *rx.borrow_and_update(),
            BlockMessage {
                blockhash: Hash::default(),
                // Note: Not 152 and 2.
                last_valid_block_height: 153,
                block_height: 3
            }
        );

        // The sender is set up to keep returning slot 3 forever, never slot 4, so the watcher
        // shouldn't send any updates after this point and this should time out.
        tokio::time::timeout_at(
            initial_time + Duration::from_millis(6 * DEFAULT_MS_PER_SLOT + 1),
            rx.changed(),
        )
        .await
        // The err being unwrapped is the timeout error.
        .unwrap_err();

        // Drop the receiver to trigger the watcher to exit.
        drop(rx);
        handle.await.unwrap();
    }
    struct MockBlockSender {
        sender: MockSender,
        initial_time: Instant,
        max_slot: u64,
    }

    #[async_trait]
    impl RpcSender for MockBlockSender {
        async fn send(
            &self,
            request: RpcRequest,
            params: serde_json::Value,
        ) -> SolanaResult<serde_json::Value> {
            // For this test it's fine to pretend that slots and blocks are the same thing.
            let slot = (Instant::now().duration_since(self.initial_time).as_millis()
                / DEFAULT_MS_PER_SLOT as u128) as u64;
            let slot = min(slot, self.max_slot);
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
}
