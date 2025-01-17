use solana_sdk::{
    clock::Slot,
    commitment_config::CommitmentConfig,
    signature::Signature,
    transaction::{Transaction, TransactionError},
};
use solana_transaction_status::TransactionStatus as SolanaTransactionStatus;
use tokio::sync::mpsc;
use tracing::Span;

/// Info about the current height of the blockchain.
#[derive(Clone, Debug, Copy, PartialEq, Default)]
pub struct BlockMessage {
    pub blockhash: solana_sdk::hash::Hash,
    pub last_valid_block_height: u64,
    pub block_height: u64,
}

/// A transaction that should be sent to the network.
#[derive(Clone, Debug)]
pub struct SendTransactionMessage {
    pub span: Span,
    pub index: usize,
    pub transaction: Transaction,
    pub last_valid_block_height: u64,
    pub response_tx: mpsc::UnboundedSender<StatusMessage>,
}

/// A transaction that has been submitted to the network, and is awaiting confirmation.
#[derive(Clone, Debug)]
pub struct ConfirmTransactionMessage {
    pub span: Span,
    pub index: usize,
    pub transaction: Transaction,
    pub last_valid_block_height: u64,
    pub response_tx: mpsc::UnboundedSender<StatusMessage>,
}

impl From<ConfirmTransactionMessage> for SendTransactionMessage {
    fn from(msg: ConfirmTransactionMessage) -> Self {
        Self {
            span: msg.span,
            index: msg.index,
            transaction: msg.transaction,
            last_valid_block_height: msg.last_valid_block_height,
            response_tx: msg.response_tx,
        }
    }
}

/// A status update for a transaction that has been submitted to the network, good or bad.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusMessage {
    pub index: usize,
    pub landed_as: Option<(Slot, Signature)>,
    pub status: TransactionStatus,
}

/// Describes the current status of a transaction, whether it has been submitted or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Processing,
    Committed,
    Failed(TransactionError),
}

impl TransactionStatus {
    /// Translates from a [`SolanaTransactionStatus`] and a [commitment level](`CommitmentConfig`)
    /// to a [`TransactionStatus`].
    pub fn from_solana_status(
        status: SolanaTransactionStatus,
        commitment: CommitmentConfig,
    ) -> Self {
        if let Some(TransactionError::AlreadyProcessed) = status.err {
            TransactionStatus::Committed
        } else if let Some(err) = status.err {
            TransactionStatus::Failed(err)
        } else if status.satisfies_commitment(commitment) {
            TransactionStatus::Committed
        } else {
            TransactionStatus::Processing
        }
    }

    /// Checks whether a transaction should be re-confirmed based on its status.
    ///
    /// These should be re-confirmed:
    /// - [`TransactionStatus::Pending`]
    /// - [`TransactionStatus::Processing`]
    ///
    /// These should *not* be re-confirmed:
    /// - [`TransactionStatus::Committed`]
    /// - [`TransactionStatus::Failed`]
    pub fn should_be_reconfirmed(&self) -> bool {
        match self {
            TransactionStatus::Pending => true,
            TransactionStatus::Processing => true,
            TransactionStatus::Committed => false,
            TransactionStatus::Failed(_) => false,
        }
    }
}
