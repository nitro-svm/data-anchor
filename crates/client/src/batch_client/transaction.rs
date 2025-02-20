use solana_sdk::{
    clock::Slot, commitment_config::CommitmentConfig, signature::Signature,
    transaction::TransactionError,
};
use solana_transaction_status::TransactionStatus as SolanaTransactionStatus;
use tracing::info;

use crate::Error;

/// The final outcome of a transaction after the [`BatchClient`] is done, either successfully
/// or due to reaching the timeout.
#[derive(Debug)]
pub enum TransactionOutcome<T> {
    /// The transaction was successfully confirmed by the network at the desired commitment level.
    Success(SuccessfulTransaction<T>),
    /// Either the transaction was not submitted to the network, or it was submitted but not confirmed.
    Unknown(UnknownTransaction<T>),
    /// The transaction latest status contained an error.
    Failure(FailedTransaction<T>),
}

/// A transaction that was successfully confirmed by the network at the desired commitment level.
#[derive(Debug)]
pub struct SuccessfulTransaction<T> {
    pub data: T,
    pub slot: Slot,
    pub signature: Signature,
}

/// A transaction that either was not submitted to the network, or it was submitted but not confirmed.
#[derive(Debug)]
pub struct UnknownTransaction<T> {
    pub data: T,
}

/// A transaction that resulted in an error.
#[derive(Debug)]
pub struct FailedTransaction<T> {
    pub data: T,
    pub error: Error,
}

impl<T> TransactionOutcome<T> {
    /// Returns `true` if the outcome was successful.
    pub fn successful(&self) -> bool {
        match self {
            TransactionOutcome::Success(_) => true,
            TransactionOutcome::Unknown(_) | TransactionOutcome::Failure(_) => false,
        }
    }

    /// Returns [`Option::Some`] if the outcome was successful, or [`Option::None`] otherwise.
    pub fn into_successful(self) -> Option<SuccessfulTransaction<T>> {
        match self {
            TransactionOutcome::Success(s) => Some(s),
            TransactionOutcome::Unknown(_) | TransactionOutcome::Failure(_) => None,
        }
    }

    /// Returns a reference to the inner [`FailedTransaction`] if the outcome was a failure, or [`None`] otherwise.
    pub fn error(&self) -> Option<&FailedTransaction<T>> {
        match self {
            TransactionOutcome::Success(_) => None,
            TransactionOutcome::Unknown(_) => None,
            TransactionOutcome::Failure(f) => Some(f),
        }
    }
}

/// Tracks the progress of a transaction, and holds on to its associated data.
pub struct TransactionProgress<T> {
    pub data: T,
    pub landed_as: Option<(Slot, Signature)>,
    pub status: TransactionStatus,
}

impl<T> TransactionProgress<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            landed_as: None,
            status: TransactionStatus::Pending,
        }
    }
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
        info!("Transaction status: {status:?}");
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

impl<T> From<TransactionProgress<T>> for TransactionOutcome<T> {
    fn from(progress: TransactionProgress<T>) -> Self {
        match progress.status {
            TransactionStatus::Pending | TransactionStatus::Processing => {
                TransactionOutcome::Unknown(UnknownTransaction {
                    data: progress.data,
                })
            }
            TransactionStatus::Failed(err) => TransactionOutcome::Failure(FailedTransaction {
                data: progress.data,
                error: err.into(),
            }),
            TransactionStatus::Committed => TransactionOutcome::Success(SuccessfulTransaction {
                data: progress.data,
                slot: progress.landed_as.unwrap().0,
                signature: progress.landed_as.unwrap().1,
            }),
        }
    }
}
