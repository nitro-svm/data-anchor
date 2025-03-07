use std::fmt::Display;

use solana_rpc_client_api::client_error::Error;
use solana_sdk::{clock::Slot, commitment_config::ParseCommitmentLevelError};
use thiserror::Error;

use crate::{tx, TransactionOutcome};

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
    /// Invalid commitment: {0}
    #[error("Invalid commitment: {0}")]
    InvalidCommitment(#[from] ParseCommitmentLevelError),
    /// Invalid indexer url: {0}
    #[error("Invalid indexer url: {0}")]
    InvalidIndexerUrl(#[from] jsonrpsee::core::client::Error),
    /// Invalid key or namespace for blober
    #[error("Invalid key or namespace for blober")]
    InvalidKeyOrNamespace,
    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result returned when interacting with the Blober client.
pub type BloberClientResult<T = ()> = Result<T, BloberClientError>;

/// Transaction outcomes were not successfull.
#[derive(Error, Debug)]
pub enum OutcomeError {
    #[error(
        "Transaction outcomes were not successfull: \n{}",
        .0.iter().filter_map(TransactionOutcome::error).map(|t| format!("- {}: {} [{}]", t.data, t.error, t.logs.join("\n"))).collect::<Vec<_>>().join("\n")
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
    ConversionError(&'static str),
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
    /// Failed to compound upload: {0}
    #[error("Failed to compound upload: {0}")]
    CompoundUpload(OutcomeError),
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

/// Transaction types which can be performed by the [`blober::blober`] program.
#[derive(Debug, Clone, Copy)]
pub enum TransactionType {
    CloseBlober,
    Compound,
    CompoundDeclare,
    CompoundFinalize,
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
            TransactionType::Compound => write!(f, "CompoundUpload"),
            TransactionType::CompoundDeclare => write!(f, "CompoundDeclare"),
            TransactionType::CompoundFinalize => write!(f, "CompoundFinalize"),
            TransactionType::DeclareBlob => write!(f, "DeclareBlob"),
            TransactionType::DiscardBlob => write!(f, "DiscardBlob"),
            TransactionType::FinalizeBlob => write!(f, "FinalizeBlob"),
            TransactionType::InitializeBlober => write!(f, "InitializeBlober"),
            TransactionType::InsertChunk(i) => write!(f, "InsertChunk {i}"),
        }
    }
}

impl TransactionType {
    /// Returns the number of signatures required for the transaction type.
    pub(crate) fn num_signatures(&self) -> u16 {
        match self {
            TransactionType::CloseBlober => tx::close_blober::NUM_SIGNATURES,
            TransactionType::Compound => tx::compound::NUM_SIGNATURES,
            TransactionType::CompoundDeclare => tx::compound_declare::NUM_SIGNATURES,
            TransactionType::CompoundFinalize => tx::compound_finalize::NUM_SIGNATURES,
            TransactionType::DeclareBlob => tx::declare_blob::NUM_SIGNATURES,
            TransactionType::DiscardBlob => tx::discard_blob::NUM_SIGNATURES,
            TransactionType::FinalizeBlob => tx::finalize_blob::NUM_SIGNATURES,
            TransactionType::InitializeBlober => tx::initialize_blober::NUM_SIGNATURES,
            TransactionType::InsertChunk(_) => tx::insert_chunk::NUM_SIGNATURES,
        }
    }

    /// Returns the compute unit limit for the transaction type.
    pub(crate) fn compute_unit_limit(&self) -> u32 {
        match self {
            TransactionType::CloseBlober => tx::close_blober::COMPUTE_UNIT_LIMIT,
            TransactionType::Compound => tx::compound::COMPUTE_UNIT_LIMIT,
            TransactionType::CompoundDeclare => tx::compound_declare::COMPUTE_UNIT_LIMIT,
            TransactionType::CompoundFinalize => tx::compound_finalize::COMPUTE_UNIT_LIMIT,
            TransactionType::DeclareBlob => tx::declare_blob::COMPUTE_UNIT_LIMIT,
            TransactionType::DiscardBlob => tx::discard_blob::COMPUTE_UNIT_LIMIT,
            TransactionType::FinalizeBlob => tx::finalize_blob::COMPUTE_UNIT_LIMIT,
            TransactionType::InitializeBlober => tx::initialize_blober::COMPUTE_UNIT_LIMIT,
            TransactionType::InsertChunk(_) => tx::insert_chunk::COMPUTE_UNIT_LIMIT,
        }
    }
}
