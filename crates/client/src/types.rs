use std::fmt::Display;

use data_anchor_api::LedgerDataBlobError;
use data_anchor_blober::instruction::{
    Close, DeclareBlob, DiscardBlob, FinalizeBlob, Initialize, InsertChunk,
};
use solana_rpc_client_api::client_error::Error;
use solana_sdk::{clock::Slot, commitment_config::ParseCommitmentLevelError};
use thiserror::Error;

use crate::{
    TransactionOutcome,
    tx::{Compound, CompoundDeclare, CompoundFinalize, MessageBuilder},
};

/// Errors that can occur when interacting with the Blober client.
#[derive(Debug, Error)]
pub enum DataAnchorClientError {
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
    /// Invalid indexer API token: {0}
    #[error("Invalid indexer API token: {0}")]
    InvalidIndexerApiToken(String),
    /// Invalid key or namespace for blober
    #[error("Invalid key or namespace for blober")]
    InvalidKeyOrNamespace,
    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Ledger data blob error: {0}
    #[error("Ledger data blob error: {0}")]
    LedgerDataBlob(#[from] LedgerDataBlobError),
}

/// Result returned when interacting with the Blober client.
pub type DataAnchorClientResult<T = ()> = Result<T, DataAnchorClientError>;

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
    /// Failed to read blobs for slot {0} via indexer client: {1}
    #[error("Failed to read blobs for slot {0} via indexer client: {1}")]
    Blobs(Slot, String),
    /// Failed to read proof for slot {0} via indexer client: {1}
    #[error("Failed to read proof for slot {0} via indexer client: {1}")]
    Proof(Slot, String),
    /// Failed to read blobs for blober {0} via indexer client: {1}
    #[error("Failed to read blobs for blober {0} via indexer client: {1}")]
    BlobsForBlober(String, String),
    /// Failed to read blobs for payer {0} via indexer client: {1}
    #[error("Failed to read blobs for payer {0} via indexer client: {1}")]
    BlobsForPayer(String, String),
    /// Failed to read blobs for network {0} via indexer client: {1}
    #[error("Failed to read blobs for network {0} via indexer client: {1}")]
    BlobsForNetwork(String, String),
    /// Failed to read blobs for namespace {0} via indexer client: {1}
    #[error("Failed to read blobs for namespace {0} via indexer client: {1}")]
    BlobsForNamespace(String, String),
    /// Failed to read proof for blob {0} via indexer client: {1}
    #[error("Failed to read proof for blob {0} via indexer client: {1}")]
    ProofForBlob(String, String),
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

/// Transaction types which can be performed by the [`data_anchor_blober::blober`] program.
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
            TransactionType::CloseBlober => Close::NUM_SIGNATURES,
            TransactionType::Compound => Compound::NUM_SIGNATURES,
            TransactionType::CompoundDeclare => CompoundDeclare::NUM_SIGNATURES,
            TransactionType::CompoundFinalize => CompoundFinalize::NUM_SIGNATURES,
            TransactionType::DeclareBlob => DeclareBlob::NUM_SIGNATURES,
            TransactionType::DiscardBlob => DiscardBlob::NUM_SIGNATURES,
            TransactionType::FinalizeBlob => FinalizeBlob::NUM_SIGNATURES,
            TransactionType::InitializeBlober => Initialize::NUM_SIGNATURES,
            TransactionType::InsertChunk(_) => InsertChunk::NUM_SIGNATURES,
        }
    }

    /// Returns the compute unit limit for the transaction type.
    pub(crate) fn compute_unit_limit(&self) -> u32 {
        match self {
            TransactionType::CloseBlober => Close::COMPUTE_UNIT_LIMIT,
            TransactionType::Compound => Compound::COMPUTE_UNIT_LIMIT,
            TransactionType::CompoundDeclare => CompoundDeclare::COMPUTE_UNIT_LIMIT,
            TransactionType::CompoundFinalize => CompoundFinalize::COMPUTE_UNIT_LIMIT,
            TransactionType::DeclareBlob => DeclareBlob::COMPUTE_UNIT_LIMIT,
            TransactionType::DiscardBlob => DiscardBlob::COMPUTE_UNIT_LIMIT,
            TransactionType::FinalizeBlob => FinalizeBlob::COMPUTE_UNIT_LIMIT,
            TransactionType::InitializeBlober => Initialize::COMPUTE_UNIT_LIMIT,
            TransactionType::InsertChunk(_) => InsertChunk::COMPUTE_UNIT_LIMIT,
        }
    }
}
