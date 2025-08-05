use std::sync::Arc;

use itertools::Itertools;
use solana_client::{client_error::ClientError as Error, nonblocking::rpc_client::RpcClient};
use solana_keypair::Keypair;
use solana_sdk::{message::Message, transaction::Transaction};
use tokio::{
    sync::mpsc,
    time::{Duration, Instant, sleep, timeout_at},
};
use tracing::{Span, info, warn};

use super::{
    channels::Channels,
    messages::{self, SendTransactionMessage, StatusMessage},
    tasks::{
        block_watcher::spawn_block_watcher, transaction_confirmer::spawn_transaction_confirmer,
        transaction_sender::spawn_transaction_sender,
    },
    transaction::{TransactionOutcome, TransactionProgress, TransactionStatus},
};

/// Send at ~333 TPS
pub const SEND_TRANSACTION_INTERVAL: Duration = Duration::from_millis(1);

/// A client that wraps an [`RpcClient`] and uses it to submit batches of transactions.
pub struct BatchClient {
    transaction_sender_tx: Arc<mpsc::UnboundedSender<SendTransactionMessage>>,
}

// Clone can't be derived because of the phantom references to the TPU implementation details.
impl Clone for BatchClient {
    fn clone(&self) -> Self {
        Self {
            transaction_sender_tx: self.transaction_sender_tx.clone(),
        }
    }
}

impl BatchClient {
    /// Creates a new [`BatchClient`], and spawns the associated background tasks. The background
    /// tasks will run until the [`BatchClient`] is dropped.
    pub async fn new(
        rpc_client: Arc<RpcClient>,
        signers: Vec<Arc<Keypair>>,
    ) -> Result<Self, Error> {
        let Channels {
            blockdata_tx,
            mut blockdata_rx,
            transaction_confirmer_tx,
            transaction_confirmer_rx,
            transaction_sender_tx,
            transaction_sender_rx,
        } = Channels::new();

        spawn_block_watcher(blockdata_tx, rpc_client.clone());
        // Wait for the first update so the default value is never visible.
        let _ = blockdata_rx.changed().await;

        spawn_transaction_confirmer(
            rpc_client.clone(),
            blockdata_rx.clone(),
            transaction_sender_tx.downgrade(),
            transaction_confirmer_tx.downgrade(),
            transaction_confirmer_rx,
        );

        spawn_transaction_sender(
            rpc_client.clone(),
            signers.clone(),
            blockdata_rx.clone(),
            transaction_confirmer_tx.clone(),
            transaction_sender_tx.downgrade(),
            transaction_sender_rx,
        );

        Ok(Self {
            transaction_sender_tx,
        })
    }

    /// Queue a batch of transactions to be sent to the network. An attempt will be made to submit
    /// the transactions in the provided order, they can be reordered, especially in case of
    /// re-submissions. The client will re-submit the transactions until they are successfully
    /// confirmed or the timeout is reached, if one is provided.
    ///
    /// Cancel safety: Dropping the future returned by this method will stop any further
    /// re-submissions of the provided transactions, but makes no guarantees about the number of
    /// transactions that have already been submitted or confirmed.
    pub async fn send<T>(
        &self,
        messages: Vec<(T, Message)>,
        timeout: Option<std::time::Duration>,
    ) -> Vec<TransactionOutcome<T>> {
        let (data, messages): (Vec<_>, Vec<_>) = messages.into_iter().unzip();
        let response_rx = self.queue_messages(messages);
        wait_for_responses(data, response_rx, timeout, log_progress_bar).await
    }

    fn queue_messages(&self, messages: Vec<Message>) -> mpsc::UnboundedReceiver<StatusMessage> {
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        for (index, message) in messages.into_iter().enumerate() {
            let transaction = Transaction::new_unsigned(message);
            let res = self
                .transaction_sender_tx
                .send(messages::SendTransactionMessage {
                    span: Span::current(),
                    index,
                    transaction,
                    // This will trigger a "re"-sign, keeping signing logic in one place.
                    last_valid_block_height: 0,
                    response_tx: response_tx.clone(),
                });
            if res.is_err() {
                warn!("transaction_sender_rx dropped, can't queue new messages");
                break;
            }
        }

        response_rx
    }
}

/// Wait for the submitted transactions to be confirmed, or for a timeout to be reached.
/// This function will also report the progress of the transactions using the provided closure.
///
/// Progress will be checked every second, and any updates in that time will be merged together.
pub async fn wait_for_responses<T>(
    data: Vec<T>,
    mut response_rx: mpsc::UnboundedReceiver<StatusMessage>,
    timeout: Option<Duration>,
    report: impl Fn(&[TransactionProgress<T>]),
) -> Vec<TransactionOutcome<T>> {
    let num_messages = data.len();
    // Start with all messages as pending.
    let mut progress: Vec<_> = data.into_iter().map(TransactionProgress::new).collect();
    let deadline = optional_timeout_to_deadline(timeout);

    loop {
        sleep(Duration::from_millis(100)).await;

        // The deadline has to be checked separately because the response_rx could be receiving
        // messages faster than they're being processed, which means recv_many returns instantly
        // and never triggers the timeout.
        if deadline < Instant::now() {
            break;
        }

        let mut buffer = Vec::new();
        match timeout_at(deadline, response_rx.recv_many(&mut buffer, num_messages)).await {
            Ok(0) => {
                // If this is ever zero, that means the channel was closed.
                // This will return the received transactions even if not all of them landed.
                break;
            }
            Err(_) => {
                // Timeout reached, break out and return what has already been received.
                break;
            }
            _ => {}
        }

        let mut changed = false;
        for msg in buffer {
            if progress[msg.index].landed_as != msg.landed_as {
                progress[msg.index].landed_as = msg.landed_as;
                changed = true;
            }
            if progress[msg.index].status != msg.status {
                progress[msg.index].status = msg.status;
                changed = true;
            }
        }
        if changed {
            report(&progress);
        }
    }

    progress.into_iter().map(Into::into).collect()
}

/// Converts an optional timeout to a conditionless deadline.
/// If the timeout is not set, the deadline will be set 30 years in the future.
fn optional_timeout_to_deadline(timeout: Option<Duration>) -> Instant {
    timeout
        .map(|timeout| Instant::now() + timeout)
        // 30 years in the future is far ahead to be effectively infinite,
        // but low enough to not overflow on some OSes.
        .unwrap_or(Instant::now() + Duration::from_secs(60 * 24 * 365 * 30))
}

fn log_progress_bar<T>(progress: &[TransactionProgress<T>]) {
    let dots: String = progress
        .iter()
        .map(|progress| match progress.status {
            TransactionStatus::Pending => ' ',
            TransactionStatus::Processing => '.',
            TransactionStatus::Committed => 'x',
            TransactionStatus::Failed(..) => '!',
        })
        .join("");
    info!("[{dots}]");
}
