use std::{ops::ControlFlow, sync::Arc};

use tokio::sync::{mpsc, watch};

use super::messages::{BlockMessage, ConfirmTransactionMessage, SendTransactionMessage};

/// Channels used by the [`super::BatchClient`].
pub struct Channels {
    pub blockdata_tx: watch::Sender<BlockMessage>,
    pub blockdata_rx: watch::Receiver<BlockMessage>,
    pub transaction_confirmer_tx: mpsc::UnboundedSender<ConfirmTransactionMessage>,
    pub transaction_confirmer_rx: mpsc::UnboundedReceiver<ConfirmTransactionMessage>,
    pub transaction_sender_tx: Arc<mpsc::UnboundedSender<SendTransactionMessage>>,
    pub transaction_sender_rx: mpsc::UnboundedReceiver<SendTransactionMessage>,
}

impl Channels {
    /// Creates all the channels used by the [`super::BatchClient`].
    pub fn new() -> Self {
        let (blockdata_tx, mut blockdata_rx) = watch::channel(BlockMessage::default());
        // Mark unchanged to prevent the default value from leaking out.
        blockdata_rx.mark_unchanged();

        let (transaction_confirmer_tx, transaction_confirmer_rx) = mpsc::unbounded_channel();
        let (transaction_sender_tx, transaction_sender_rx) = mpsc::unbounded_channel();
        let transaction_sender_tx = Arc::new(transaction_sender_tx);

        Self {
            blockdata_tx,
            blockdata_rx,
            transaction_confirmer_tx,
            transaction_confirmer_rx,
            transaction_sender_tx,
            transaction_sender_rx,
        }
    }
}

/// Try to send a collection of messages to a [weak sender][`mpsc::WeakUnboundedSender`].
///
/// This has four possible outcomes:
/// - If the channel is closed, no messages are sent and [`ControlFlow::Break`] is returned.
/// - If the channel is still open, the messages will be sent one at a time:
///   - If all messages were sent successfully, [`ControlFlow::Continue`] is returned.
///   - If any of the messages failed to send (which means the channel was closed mid-batch),
///     the rest of the messages will be dropped and [`ControlFlow::Break`] is returned.
pub fn upgrade_and_send<T>(
    sender: &mpsc::WeakUnboundedSender<T>,
    messages: impl IntoIterator<Item = T>,
) -> ControlFlow<()> {
    // First check if there's a receiver on the other end, (even if there are no messages),
    // to ensure shutdown works as it should.
    let Some(sender) = sender.upgrade() else {
        return ControlFlow::Break(());
    };

    for message in messages {
        let res = sender.send(message);
        if res.is_err() {
            return ControlFlow::Break(());
        }
    }
    ControlFlow::Continue(())
}
