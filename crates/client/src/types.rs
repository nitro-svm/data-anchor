use std::fmt::Display;

use solana_rpc_client_api::client_error::Error;
use solana_sdk::clock::Slot;
use thiserror::Error;

use crate::TransactionOutcome;

/// Errors that can occur when interacting with the Blober client.
#[derive(Debug, Error)]
pub enum BloberClientError {
    /// Upload blob errors
    #[error(transparent)]
    UploadBlob(#[from] UploadBlobError),
    /// Indexer errors
    #[error(transparent)]
    Indexer(#[from] IndexerError),
    /// Deployment errors
    #[error(transparent)]
    Deployment(#[from] DeploymentError),
    /// Failed to query Solana RPC: {0}
    #[error("Failed to query Solana RPC: {0}")]
    SolanaRpc(#[from] Error),
}

/// Result returned when interacting with the Blober client.
pub type BloberClientResult<T = ()> = Result<T, BloberClientError>;

/// Transaction outcomes were not successfull.
#[derive(Error, Debug)]
pub enum OutcomeError {
    #[error(
        "Transaction outcomes were not successfull: \n{}",
        .0.iter().filter_map(TransactionOutcome::error).map(|t| format!("- {}: {}", t.data, t.error)).collect::<Vec<_>>().join("\n")
    )]
    Unsuccesful(Vec<TransactionOutcome<TransactionType>>),
}

/// An error that can occur when uploading a blob to a blober account.
#[derive(Error, Debug)]
pub enum UploadBlobError {
    /// Failed to query Solana RPC: {0}
    #[error("Failed to query Solana RPC: {0}")]
    SolanaRpc(#[from] Error),
    /// Failed when sending transactions. Transaction errors:\n{}
    #[error(transparent)]
    TransactionFailure(#[from] OutcomeError),
    /// Fee Strategy conversion failure: {0}
    #[error("Fee Strategy conversion failure: {0}")]
    ConversionError(String),
    /// Failed to declare blob: {0}
    #[error("Failed to declare blob: {0}")]
    DeclareBlob(OutcomeError),
    /// Failed to insert chunks: {0}
    #[error("Failed to insert chunks: {0}")]
    InsertChunks(OutcomeError),
    /// Failed to finalize blob: {0}
    #[error("Failed to finalize blob: {0}")]
    FinalizeBlob(OutcomeError),
    /// Failed to discard blob: {0}
    #[error("Failed to discard blob: {0}")]
    DiscardBlob(OutcomeError),
    /// Failed to initialize blober: {0}
    #[error("Failed to initialize blober: {0}")]
    InitializeBlober(OutcomeError),
    /// Failed to close blober: {0}
    #[error("Failed to close blober: {0}")]
    CloseBlober(OutcomeError),
}

#[derive(Error, Debug)]
pub enum IndexerError {
    /// Failed to read proof for slot {0} via indexer client: {1}
    #[error("Failed to read blobs for slot {0} via indexer client: {1}")]
    Blobs(Slot, String),
    /// Failed to read proof for slot {0} via indexer client: {1}
    #[error("Failed to read proof for slot {0} via indexer client: {1}")]
    Proof(Slot, String),
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
    CloseBlober,
    DeclareBlob,
    DiscardBlob,
    FinalizeBlob,
    InitializeBlober,
    InsertChunk(u16),
}

impl Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionType::CloseBlober => write!(f, "CloseBlober"),
            TransactionType::DeclareBlob => write!(f, "DeclareBlob"),
            TransactionType::DiscardBlob => write!(f, "DiscardBlob"),
            TransactionType::FinalizeBlob => write!(f, "FinalizeBlob"),
            TransactionType::InitializeBlober => write!(f, "InitializeBlober"),
            TransactionType::InsertChunk(i) => write!(f, "InsertChunk {i}"),
        }
    }
}
