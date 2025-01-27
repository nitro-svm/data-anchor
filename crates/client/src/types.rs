use std::{fmt::Display, sync::Arc};

use solana_rpc_client_api::client_error::Error;
use thiserror::Error;

use crate::TransactionOutcome;

/// An error that can occur when uploading a blob to a blober account.
#[derive(Error, Debug)]
pub enum UploadBlobError {
    /// Failed to retrieve recent priority fees. Error: {0}
    #[error("Failed to retrieve recent priority fees. Error: {0}")]
    PriorityFees(#[source] Error),
    /// Failed when sending transactions. Transaction errors:\n{}
    #[error(
        "Failed when sending transactions. Transaction errors:\n{}",
        .0.iter().filter_map(TransactionOutcome::error).map(|t| format!("- {}: {}", t.data, t.error)).collect::<Vec<_>>().join("\n")
    )]
    /// Failed to force close the blob. Original error: {0}\n\nClose error: {1}
    Transactions(Vec<TransactionOutcome<TransactionType>>),
    #[error("Failed to force close the blob. Original error: {0}\n\nClose error: {1}")]
    CloseAccount(#[source] Arc<UploadBlobError>, Error),
    /// Fee Strategy conversion failure: {0}
    #[error("Fee Strategy conversion failure: {0}")]
    ConversionError(String),
}

#[derive(Error, Debug)]
pub enum IndexerError {
    /// Failed to read blobs via indexer client: {0}
    #[error("Failed to read blobs via indexer client: {0}")]
    Blobs(String),
    /// Failed to read slot proof via indexer client: {0}
    #[error("Failed to read slot proof via indexer client: {0}")]
    Proof(String),
}

#[derive(Error, Debug)]
pub enum DeploymentError {
    /// Failed to create buffer account: {0}
    #[error("Failed to create buffer account: {0}")]
    Buffer(String),
    /// Failed to deploy program: {0}
    #[error("Failed to deploy program: {0}")]
    Deploy(String),
    /// Failed to get minimum balance for rent exemption: {0}
    #[error("Failed to get minimum balance for rent exemption: {0}")]
    RentBalance(String),
    /// Failed to get recent blockhash
    #[error("Failed to get recent blockhash")]
    BlockHash,
    /// Failed to read program bytecode: {0}
    #[error("Failed to read program bytecode: {0}")]
    Bytecode(String),
}

#[derive(Debug, Clone, Copy)]
pub enum TransactionType {
    DeclareBlob,
    InsertChunk(u16),
    FinalizeBlob,
}

impl Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionType::DeclareBlob => write!(f, "DeclareBlob"),
            TransactionType::InsertChunk(i) => write!(f, "InsertChunk {}", i),
            TransactionType::FinalizeBlob => write!(f, "FinalizeBlob"),
        }
    }
}
