use std::fmt::Display;

use data_anchor_api::LedgerDataBlobError;
use data_anchor_blober::instruction::{
    Close, ConfigureCheckpoint, DeclareBlob, DiscardBlob, FinalizeBlob, Initialize, InsertChunk,
};
use solana_commitment_config::ParseCommitmentLevelError;
use solana_rpc_client_api::client_error::Error;
use thiserror::Error;

use crate::{
    TransactionOutcome,
    client::{ChainError, IndexerError},
    tx::{Compound, CompoundDeclare, CompoundFinalize, MessageBuilder},
};

/// Errors that can occur when interacting with the Blober client.
#[derive(Debug, Error)]
pub enum DataAnchorClientError {
    /// Chain interaction errors
    #[error(transparent)]
    ChainErrors(#[from] ChainError),
    /// Indexer errors
    #[error(transparent)]
    Indexer(#[from] IndexerError),
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
    /// Invalid data: {0}
    #[error("Invalid data: {0}")]
    InvalidData(String),
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

/// Transaction types which can be performed by the [`data_anchor_blober::blober`] program.
#[derive(Debug, Clone, Copy)]
pub enum TransactionType {
    CloseBlober,
    Compound,
    CompoundDeclare,
    CompoundFinalize,
    ConfigureCheckpoint,
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
            TransactionType::ConfigureCheckpoint => write!(f, "CreateCheckpoint"),
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
            TransactionType::ConfigureCheckpoint => ConfigureCheckpoint::NUM_SIGNATURES,
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
            TransactionType::ConfigureCheckpoint => ConfigureCheckpoint::COMPUTE_UNIT_LIMIT,
            TransactionType::DeclareBlob => DeclareBlob::COMPUTE_UNIT_LIMIT,
            TransactionType::DiscardBlob => DiscardBlob::COMPUTE_UNIT_LIMIT,
            TransactionType::FinalizeBlob => FinalizeBlob::COMPUTE_UNIT_LIMIT,
            TransactionType::InitializeBlober => Initialize::COMPUTE_UNIT_LIMIT,
            TransactionType::InsertChunk(_) => InsertChunk::COMPUTE_UNIT_LIMIT,
        }
    }
}
