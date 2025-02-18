use std::{option::Option, sync::Arc};

use solana_client::{
    nonblocking::tpu_client::TpuClient, rpc_client::SerializableTransaction,
    rpc_config::RpcSendTransactionConfig,
};
use solana_connection_cache::connection_cache::{
    BaseClientConnection, ConnectionManager, ConnectionPool, NewConnectionConfig,
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
use crate::{batch_client::client::SEND_TRANSACTION_INTERVAL, Error, ErrorKind};

/// Spawns an independent task that listens for [`SendTransactionMessage`]s and periodically submits
/// transactions using the Solana RPC client, re-signing the transactions when necessary.
///
/// It does *not* check the outcome of the transaction at all other than failing if the transaction
/// submission itself fails. When this happens, the transaction will be queued for re-sending.
///
/// The task will exit if there are no transaction senders alive. This will happen when the
/// [BatchClient](`crate::batch_client::BatchClient`) has been dropped.
#[allow(clippy::too_many_arguments)]
pub fn spawn_transaction_sender<P, M, C>(
    rpc_client: Arc<RpcClient>,
    tpu_client: Option<Arc<TpuClient<P, M, C>>>,
    signers: Vec<Arc<Keypair>>,
    blockdata_rx: watch::Receiver<BlockMessage>,
    transaction_confirmer_tx: mpsc::UnboundedSender<ConfirmTransactionMessage>,
    transaction_sender_tx: mpsc::WeakUnboundedSender<SendTransactionMessage>,
    mut transaction_sender_rx: mpsc::UnboundedReceiver<SendTransactionMessage>,
) -> JoinHandle<()>
where
    P: ConnectionPool<NewConnectionConfig = C>,
    M: ConnectionManager<ConnectionPool = P, NewConnectionConfig = C>,
    C: NewConnectionConfig,
    <P::BaseClientConnection as BaseClientConnection>::NonblockingClientConnection: Send + Sync,
{
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

            let res = send_transaction(&rpc_client, &tpu_client, &msg.transaction)
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
async fn send_transaction<P, M, C>(
    rpc_client: &Arc<RpcClient>,
    tpu_client: &Option<Arc<TpuClient<P, M, C>>>,
    transaction: &Transaction,
) -> Result<(), Error>
where
    P: ConnectionPool<NewConnectionConfig = C>,
    M: ConnectionManager<ConnectionPool = P, NewConnectionConfig = C>,
    C: NewConnectionConfig,
{
    if let Some(tpu_client) = tpu_client {
        tpu_client
            .try_send_transaction(transaction)
            .in_current_span()
            .await
            .map_err(|e| Error {
                // Wrap the error to keep the return type consistent.
                request: None,
                kind: ErrorKind::Custom(e.to_string()),
            })
    } else {
        let rpc_client = rpc_client.clone();
        let transaction = transaction.clone();
        let span = Span::current();
        tokio::spawn(async move {
            let res = rpc_client
                .send_transaction_with_config(
                    &transaction,
                    RpcSendTransactionConfig {
                        max_retries: None,
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
}

#[cfg(test)]
mod tests {
    use std::{
        mem,
        net::SocketAddr,
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
    };

    use anchor_lang::prelude::Pubkey;
    use async_trait::async_trait;
    use solana_client::{connection_cache::Protocol, tpu_client::TpuClientConfig};
    use solana_connection_cache::{
        client_connection::ClientConnection as BlockingClientConnection,
        connection_cache::{
            BaseClientConnection, ConnectionCache, ConnectionManager, ConnectionPool,
            NewConnectionConfig,
        },
        nonblocking::client_connection::ClientConnection as NonblockingClientConnection,
    };
    use solana_sdk::{hash::Hash, signer::Signer, transport::Result as TransportResult};
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
        let manager = MockConnectionManager::default();
        let connection_cache = Arc::new(ConnectionCache::new("mock", manager.clone(), 3).unwrap());
        let tpu_client = Arc::new(
            TpuClient::new_with_connection_cache(
                rpc_client.clone(),
                "",
                TpuClientConfig::default(),
                connection_cache,
            )
            .await
            .unwrap(),
        );
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
            Some(tpu_client),
            vec![payer.clone()],
            blockdata_rx,
            transaction_confirmer_tx,
            transaction_sender_tx.downgrade(),
            transaction_sender_rx,
        );

        // No transactions should be sent yet.
        let sent_transactions = manager.get_and_clear_sent_transactions();
        assert_eq!(sent_transactions, vec![]);
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

        let sent_transactions = manager.get_and_clear_sent_transactions();
        assert_eq!(sent_transactions, vec![transaction.clone()]);
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

        let sent_transactions = manager.get_and_clear_sent_transactions();
        assert_eq!(sent_transactions.len(), 1);
        let resigned_transaction = sent_transactions[0].clone();
        assert_eq!(
            resigned_transaction.message.header,
            transaction.message.header
        );
        assert_eq!(
            resigned_transaction.message.account_keys,
            transaction.message.account_keys
        );
        assert_eq!(
            resigned_transaction.message.instructions,
            transaction.message.instructions
        );
        assert_ne!(
            resigned_transaction.message.recent_blockhash,
            transaction.message.recent_blockhash
        );
        assert_ne!(resigned_transaction.signatures, transaction.signatures);

        // There should be one message in the confirmation queue.
        let confirmation = transaction_confirmer_rx.try_recv().unwrap();
        transaction_confirmer_rx.try_recv().unwrap_err();
        assert_eq!(confirmation.index, 1);
        assert_eq!(confirmation.transaction, resigned_transaction);
        assert_eq!(
            confirmation.last_valid_block_height,
            new_block.last_valid_block_height
        );

        // Make all transaction sending fail from this point on.
        manager.set_fail_sends(true);

        transaction_sender_tx
            .send(SendTransactionMessage {
                span: Span::current(),
                index: 2,
                transaction: resigned_transaction.clone(),
                last_valid_block_height: new_block.last_valid_block_height,
                response_tx: response_tx.clone(),
            })
            .unwrap();
        sleep_until(initial_time + 3 * SEND_TRANSACTION_INTERVAL + Duration::from_millis(1)).await;
        // This time it should not be in the confirmation queue.
        transaction_confirmer_rx.try_recv().unwrap_err();

        // Turn off the fail_sends flag.
        manager.set_fail_sends(false);
        // Wait a bit more, and the transaction should be sent again.
        sleep_until(initial_time + 4 * SEND_TRANSACTION_INTERVAL + Duration::from_millis(1)).await;
        let sent_transactions = manager.get_and_clear_sent_transactions();
        assert_eq!(sent_transactions, vec![resigned_transaction.clone()]);
        // There should be one message in the confirmation queue.
        let confirmation = transaction_confirmer_rx.try_recv().unwrap();
        transaction_confirmer_rx.try_recv().unwrap_err();
        assert_eq!(confirmation.index, 2);
        assert_eq!(confirmation.transaction, resigned_transaction);
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

    #[derive(Default, Clone)]
    struct MockConnectionManager {
        pools: Arc<Mutex<Vec<MockConnectionPool>>>,
        fail_sends: Arc<AtomicBool>,
    }

    impl MockConnectionManager {
        fn get_and_clear_sent_transactions(&self) -> Vec<Transaction> {
            let mut transactions = vec![];
            let pools = self.pools.lock().unwrap();
            for pool in pools.iter() {
                let base_connections = pool.base_connections.lock().unwrap();
                for base_connection in base_connections.iter() {
                    let connections = base_connection.connections.lock().unwrap();
                    for connection in connections.iter() {
                        let sent_data = mem::take(&mut *connection.sent_data.lock().unwrap());
                        for data in sent_data {
                            // Only interested in transaction data, so if the deserialization fails that's fine.
                            if let Ok(transaction) = bincode::deserialize(data.as_slice()) {
                                transactions.push(transaction);
                            }
                        }
                    }
                }
            }

            transactions
        }

        fn set_fail_sends(&self, fail_sends: bool) {
            self.fail_sends.store(fail_sends, Ordering::SeqCst);
        }
    }

    impl ConnectionManager for MockConnectionManager {
        type ConnectionPool = MockConnectionPool;
        type NewConnectionConfig = MockConnectionConfig;
        const PROTOCOL: Protocol = Protocol::QUIC;

        fn new_connection_pool(&self) -> Self::ConnectionPool {
            let pool = MockConnectionPool {
                base_connections: Default::default(),
                fail_sends: self.fail_sends.clone(),
            };
            let mut pools = self.pools.lock().unwrap();
            pools.push(pool.clone());
            pool
        }

        fn new_connection_config(&self) -> Self::NewConnectionConfig {
            MockConnectionConfig
        }

        fn update_key(&self, _key: &Keypair) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct MockConnectionPool {
        base_connections: Arc<Mutex<Vec<Arc<MockBaseClientConnection>>>>,
        fail_sends: Arc<AtomicBool>,
    }
    impl ConnectionPool for MockConnectionPool {
        type NewConnectionConfig = MockConnectionConfig;
        type BaseClientConnection = MockBaseClientConnection;

        fn add_connection(
            &mut self,
            config: &Self::NewConnectionConfig,
            addr: &std::net::SocketAddr,
        ) -> usize {
            let mut connections = self.base_connections.lock().unwrap();
            let index = connections.len();
            let connection = self.create_pool_entry(config, addr);
            connections.push(connection);
            index
        }

        fn num_connections(&self) -> usize {
            self.base_connections.lock().unwrap().len()
        }

        fn get(
            &self,
            index: usize,
        ) -> Result<
            Arc<Self::BaseClientConnection>,
            solana_connection_cache::connection_cache::ConnectionPoolError,
        > {
            Ok(Arc::clone(
                self.base_connections.lock().unwrap().get(index).unwrap(),
            ))
        }

        fn create_pool_entry(
            &self,
            _config: &Self::NewConnectionConfig,
            _addr: &std::net::SocketAddr,
        ) -> Arc<Self::BaseClientConnection> {
            Arc::new(MockBaseClientConnection {
                fail_sends: self.fail_sends.clone(),
                connections: Default::default(),
            })
        }
    }

    struct MockConnectionConfig;
    impl NewConnectionConfig for MockConnectionConfig {
        fn new() -> Result<Self, solana_connection_cache::connection_cache::ClientError> {
            Ok(Self)
        }
    }

    struct MockBaseClientConnection {
        fail_sends: Arc<AtomicBool>,
        connections: Arc<Mutex<Vec<Arc<MockClientConnection>>>>,
    }
    impl BaseClientConnection for MockBaseClientConnection {
        type BlockingClientConnection = MockClientConnection;
        type NonblockingClientConnection = MockClientConnection;

        fn new_blocking_connection(
            &self,
            addr: std::net::SocketAddr,
            _stats: Arc<solana_connection_cache::connection_cache_stats::ConnectionCacheStats>,
        ) -> Arc<Self::BlockingClientConnection> {
            let connection = Arc::new(MockClientConnection::new(addr, self.fail_sends.clone()));
            let mut connections = self.connections.lock().unwrap();
            connections.push(connection.clone());
            connection
        }

        fn new_nonblocking_connection(
            &self,
            addr: std::net::SocketAddr,
            _stats: Arc<solana_connection_cache::connection_cache_stats::ConnectionCacheStats>,
        ) -> Arc<Self::NonblockingClientConnection> {
            let connection = Arc::new(MockClientConnection::new(addr, self.fail_sends.clone()));
            let mut connections = self.connections.lock().unwrap();
            connections.push(connection.clone());
            connection
        }
    }

    struct MockClientConnection {
        fail_sends: Arc<AtomicBool>,
        server_addr: std::net::SocketAddr,
        sent_data: Mutex<Vec<Vec<u8>>>,
    }

    impl MockClientConnection {
        fn new(server_addr: std::net::SocketAddr, fail_sends: Arc<AtomicBool>) -> Self {
            Self {
                fail_sends,
                server_addr,
                sent_data: Mutex::new(vec![]),
            }
        }
    }

    impl BlockingClientConnection for MockClientConnection {
        fn server_addr(&self) -> &std::net::SocketAddr {
            &self.server_addr
        }

        fn send_data(&self, buffer: &[u8]) -> solana_sdk::transport::Result<()> {
            if self.fail_sends.load(Ordering::SeqCst) && !buffer.is_empty() {
                Err(solana_sdk::transport::TransportError::IoError(
                    std::io::Error::new(std::io::ErrorKind::Other, "fail_sends"),
                ))
            } else {
                let mut sent_data = self.sent_data.lock().unwrap();
                sent_data.push(buffer.to_vec());
                Ok(())
            }
        }

        fn send_data_async(&self, _buffer: Vec<u8>) -> solana_sdk::transport::Result<()> {
            unimplemented!("not used in test")
        }

        fn send_data_batch(&self, _buffers: &[Vec<u8>]) -> solana_sdk::transport::Result<()> {
            unimplemented!("not used in test")
        }

        fn send_data_batch_async(
            &self,
            _buffers: Vec<Vec<u8>>,
        ) -> solana_sdk::transport::Result<()> {
            unimplemented!("not used in test")
        }
    }

    #[async_trait]
    impl NonblockingClientConnection for MockClientConnection {
        fn server_addr(&self) -> &SocketAddr {
            &self.server_addr
        }

        async fn send_data(&self, buffer: &[u8]) -> TransportResult<()> {
            if self.fail_sends.load(Ordering::SeqCst) && !buffer.is_empty() {
                Err(solana_sdk::transport::TransportError::IoError(
                    std::io::Error::new(std::io::ErrorKind::Other, "fail_sends"),
                ))
            } else {
                let mut sent_data = self.sent_data.lock().unwrap();
                sent_data.push(buffer.to_vec());
                Ok(())
            }
        }

        async fn send_data_batch(&self, _buffers: &[Vec<u8>]) -> TransportResult<()> {
            unimplemented!("not used in test")
        }
    }
}
