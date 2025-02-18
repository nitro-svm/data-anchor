//! A client that wraps an [`solana_rpc_client::nonblocking::rpc_client::RpcClient`] and optionally
//! a [`solana_client::nonblocking::tpu_client::TpuClient`] and uses them to submit batches of transactions.
//! Providing a [`solana_client::nonblocking::tpu_client::TpuClient`] will enable the client to send transactions
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
