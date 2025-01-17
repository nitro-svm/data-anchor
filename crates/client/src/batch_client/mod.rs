//! A client that wraps an [`RpcClient`] and optionally a [`TpuClient`] and uses them to submit
//! batches of transactions. Providing a [`TpuClient`] will enable the client to send transactions
//! directly to the upcoming slot leaders, which is much faster and thus highly recommended.

mod channels;
mod client;
mod messages;
mod tasks;
mod transaction;

pub use client::BatchClient;
pub use transaction::{
    FailedTransaction, SuccessfulTransaction, TransactionOutcome, UnknownTransaction,
};
