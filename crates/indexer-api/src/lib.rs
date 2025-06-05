#![doc = include_str!("../README.md")]

use std::collections::HashSet;

use anchor_lang::{AnchorDeserialize, Discriminator};
use chrono::{DateTime, Utc};
use data_anchor_blober::{BLOB_ACCOUNT_INSTRUCTION_IDX, BLOB_BLOBER_INSTRUCTION_IDX};
use data_anchor_proofs::compound::{
    completeness::CompoundCompletenessProof, inclusion::CompoundInclusionProof,
};
use jsonrpsee::{
    core::{RpcResult, SubscriptionResult},
    proc_macros::rpc,
};
use serde::{Deserialize, Serialize};
use solana_sdk::{
    clock::Slot, instruction::CompiledInstruction, pubkey::Pubkey,
    transaction::VersionedTransaction,
};
use solana_transaction_status::InnerInstructions;

/// A compound proof that proves whether a blob has been published in a specific slot.
/// See [`CompoundInclusionProof`] and [`CompoundCompletenessProof`] for more information.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum CompoundProof {
    /// See [`CompoundInclusionProof`].
    Inclusion(CompoundInclusionProof),
    /// See [`CompoundCompletenessProof`].
    Completeness(CompoundCompletenessProof),
}

/// A data structure representing a blober's information, including the blober's pubkey, the
/// payer's pubkey, and the network of the blober.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BloberData {
    pub blober: Pubkey,
    pub payer: Pubkey,
    pub network_id: u64,
}

/// A time range with optional start and end times, used for filtering time.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeRange {
    /// The start time of the range, inclusive.
    pub start: Option<DateTime<Utc>>,
    /// The end time of the range, inclusive.
    pub end: Option<DateTime<Utc>>,
}

/// Request parameters for retrieving blobs by a specific blober's pubkey and a time range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobsByBlober {
    /// The blober's pubkey.
    pub blober: Pubkey,
    /// The time range for which to retrieve blobs.
    #[serde(flatten)]
    pub time_range: TimeRange,
}

/// Request parameters for retrieving blobs by a specific payer's pubkey, network ID, and a time range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobsByPayer {
    /// The payer's pubkey.
    pub payer: Pubkey,
    /// The network name of the blobs.
    pub network_name: String,
    /// The time range for which to retrieve blobs.
    #[serde(flatten)]
    pub time_range: TimeRange,
}

/// The Indexer RPC interface.
#[rpc(server, client)]
pub trait IndexerRpc {
    /// Retrieve a list of blobs for a given slot and blober pubkey. Returns an error if there was a
    /// database or RPC failure, and None if the slot has not been completed yet. If the slot is
    /// completed, an empty list will be returned.
    #[method(name = "get_blobs")]
    async fn get_blobs(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<Vec<Vec<u8>>>>;

    /// Retrieve a list of blobs for a given blober pubkey and time range. Returns an error if there
    /// was a database or RPC failure, and an empty list if no blobs were found.
    #[method(name = "get_blobs_by_blober")]
    async fn get_blobs_by_blober(&self, blober: BlobsByBlober) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a list of blobs for a given payer pubkey, network ID, and time range. Returns an
    /// error if there was a database or RPC failure, and an empty list if no blobs were found.
    #[method(name = "get_blobs_by_payer")]
    async fn get_blobs_by_payer(&self, payer: BlobsByPayer) -> RpcResult<Vec<Vec<u8>>>;

    /// Retrieve a proof for a given slot and blober pubkey. Returns an error if there was a
    /// database or RPC failure, and None if the slot has not been completed yet.
    #[method(name = "get_proof")]
    async fn get_proof(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<CompoundProof>>;

    /// Retrieve a compound proof that covers a particular blob. Returns an error if there was a
    /// database or RPC failure, and None if the blob does not exist.
    #[method(name = "get_proof_for_blob")]
    async fn get_proof_for_blob(&self, blob_address: Pubkey) -> RpcResult<Option<CompoundProof>>;

    /// Add a list of blober PDA addresses to the list of tracked blobers.
    #[method(name = "add_blobers")]
    async fn add_blobers(&self, blobers: HashSet<BloberData>) -> RpcResult<()>;

    /// Remove a list of blober PDA addresses from the list of tracked blobers.
    #[method(name = "remove_blobers")]
    async fn remove_blobers(&self, blobers: HashSet<Pubkey>) -> RpcResult<()>;

    /// Listen to blob finalization events from specified blobers. This will return a stream of
    /// slots and blober PDAs that have finalized blobs. The stream will be closed when the RPC server is
    /// shut down.
    #[subscription(name = "subscribe_blob_finalization" => "listen_subscribe_blob_finalization", unsubscribe = "unsubscribe_blob_finalization", item = (Pubkey, Slot))]
    async fn subscribe_blob_finalization(&self, blobers: HashSet<Pubkey>) -> SubscriptionResult;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedTransactionWithInnerInstructions {
    pub transaction: VersionedTransaction,
    pub inner_instructions: Vec<InnerInstructions>,
}

impl From<VersionedTransaction> for VersionedTransactionWithInnerInstructions {
    fn from(transaction: VersionedTransaction) -> Self {
        Self {
            transaction,
            inner_instructions: Vec::new(),
        }
    }
}

impl From<&VersionedTransaction> for VersionedTransactionWithInnerInstructions {
    fn from(transaction: &VersionedTransaction) -> Self {
        Self {
            transaction: transaction.clone(),
            inner_instructions: Vec::new(),
        }
    }
}

impl VersionedTransactionWithInnerInstructions {
    /// Create an iterator over all instructions in the transaction, including both top-level and
    /// inner instructions.
    pub fn iter_instructions(&self) -> impl Iterator<Item = &CompiledInstruction> {
        self.transaction.message.instructions().iter().chain(
            self.inner_instructions
                .iter()
                .flat_map(|inner| inner.instructions.iter().map(|inner| &inner.instruction)),
        )
    }
}

/// A relevant [`data_anchor_blober`] instruction extracted from a [`VersionedTransaction`].
pub enum RelevantInstruction {
    DeclareBlob(data_anchor_blober::instruction::DeclareBlob),
    InsertChunk(data_anchor_blober::instruction::InsertChunk),
    FinalizeBlob(data_anchor_blober::instruction::FinalizeBlob),
}

impl std::fmt::Debug for RelevantInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelevantInstruction::DeclareBlob(instruction) => f
                .debug_struct("DeclareBlob")
                .field("size", &instruction.blob_size)
                .field("timestamp", &instruction.timestamp)
                .finish(),
            RelevantInstruction::InsertChunk(instruction) => f
                .debug_struct("InsertChunk")
                .field("idx", &instruction.idx)
                .finish(),
            RelevantInstruction::FinalizeBlob(_) => f.debug_struct("FinalizeBlob").finish(),
        }
    }
}

impl RelevantInstruction {
    pub fn try_from_slice(compiled_instruction: &CompiledInstruction) -> Option<Self> {
        use data_anchor_blober::instruction::*;
        let discriminator = compiled_instruction.data.get(..8)?;

        match discriminator {
            DeclareBlob::DISCRIMINATOR => {
                let data = compiled_instruction.data.get(8..).unwrap_or_default();
                DeclareBlob::try_from_slice(data)
                    .map(RelevantInstruction::DeclareBlob)
                    .ok()
            }
            InsertChunk::DISCRIMINATOR => {
                let data = compiled_instruction.data.get(8..).unwrap_or_default();
                InsertChunk::try_from_slice(data)
                    .map(RelevantInstruction::InsertChunk)
                    .ok()
            }
            FinalizeBlob::DISCRIMINATOR => {
                let data = compiled_instruction.data.get(8..).unwrap_or_default();
                FinalizeBlob::try_from_slice(data)
                    .map(RelevantInstruction::FinalizeBlob)
                    .ok()
            }
            // If we don't recognize the discriminator, we ignore the instruction - there might be
            // more instructions packed into the same transaction which might not be relevant to
            // us.
            _ => None,
        }
    }
}

/// A deserialized relevant instruction, containing the blob and blober pubkeys and the instruction.
#[derive(Debug)]
pub struct RelevantInstructionWithAccounts {
    pub blob: Pubkey,
    pub blober: Pubkey,
    pub instruction: RelevantInstruction,
}

/// Deserialize relevant instructions from a transaction, given the indices of the blob and blober
/// accounts in the transaction.
pub fn deserialize_relevant_instructions(
    program_id: &Pubkey,
    tx: &VersionedTransactionWithInnerInstructions,
    blob_pubkey_index: usize,
    blober_pubkey_index: usize,
) -> Vec<RelevantInstructionWithAccounts> {
    tx.iter_instructions()
        .filter_map(|compiled_instruction| {
            let program_id_idx: usize = compiled_instruction.program_id_index.into();
            let relevant_program_id = tx
                .transaction
                .message
                .static_account_keys()
                .get(program_id_idx)?;

            if program_id != relevant_program_id {
                return None; // Skip instructions not related to the specified program ID.
            }

            let blob =
                get_account_at_index(&tx.transaction, compiled_instruction, blob_pubkey_index)?;
            let blober =
                get_account_at_index(&tx.transaction, compiled_instruction, blober_pubkey_index)?;
            let instruction = RelevantInstruction::try_from_slice(compiled_instruction)?;
            let relevant_instruction = RelevantInstructionWithAccounts {
                blob,
                blober,
                instruction,
            };

            Some(relevant_instruction)
        })
        .collect()
}

/// Blober instructions that are relevant to the indexer.
pub enum RelevantBloberInstruction {
    Initialize(data_anchor_blober::instruction::Initialize),
    Close(data_anchor_blober::instruction::Close),
}

impl std::fmt::Debug for RelevantBloberInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelevantBloberInstruction::Initialize(instruction) => f
                .debug_struct("Initialize")
                .field("trusted", &instruction.trusted)
                .finish(),
            RelevantBloberInstruction::Close(_) => f.debug_struct("Close").finish(),
        }
    }
}

impl RelevantBloberInstruction {
    pub fn try_from_slice(compiled_instruction: &CompiledInstruction) -> Option<Self> {
        use data_anchor_blober::instruction::*;
        let discriminator = compiled_instruction.data.get(..8)?;

        match discriminator {
            Initialize::DISCRIMINATOR => {
                let data = compiled_instruction.data.get(8..).unwrap_or_default();
                Initialize::try_from_slice(data)
                    .map(RelevantBloberInstruction::Initialize)
                    .ok()
            }
            Close::DISCRIMINATOR => {
                let data = compiled_instruction.data.get(8..).unwrap_or_default();
                Close::try_from_slice(data)
                    .map(RelevantBloberInstruction::Close)
                    .ok()
            }
            // If we don't recognize the discriminator, we ignore the instruction - there might be
            // more instructions packed into the same transaction which might not be relevant to
            // us.
            _ => None,
        }
    }
}

/// A deserialized relevant blober instruction, containing the blober pubkey and the instruction.
#[derive(Debug)]
pub struct RelevantBloberInstructionWithPubkey {
    pub blober: Pubkey,
    pub instruction: RelevantBloberInstruction,
}

/// Deserialize blober instructions from a transaction, returning a vector of [`RelevantBloberInstructionWithPubkey`].
pub fn deserialize_blober_instructions(
    program_id: &Pubkey,
    tx: &VersionedTransactionWithInnerInstructions,
) -> Vec<RelevantBloberInstructionWithPubkey> {
    tx.iter_instructions()
        .filter_map(|compiled_instruction| {
            let program_id_idx: usize = compiled_instruction.program_id_index.into();

            let relevant_program_id = tx
                .transaction
                .message
                .static_account_keys()
                .get(program_id_idx)?;

            if program_id != relevant_program_id {
                return None; // Skip instructions not related to the specified program ID.
            }

            let blober = get_account_at_index(&tx.transaction, compiled_instruction, 0)?;

            let instruction = RelevantBloberInstruction::try_from_slice(compiled_instruction)?;

            Some(RelevantBloberInstructionWithPubkey {
                blober,
                instruction,
            })
        })
        .collect()
}

/// Extract relevant instructions from a list of transactions.
pub fn extract_relevant_instructions(
    program_id: &Pubkey,
    transactions: &[VersionedTransaction],
) -> Vec<RelevantInstructionWithAccounts> {
    transactions
        .iter()
        .flat_map(|tx| {
            deserialize_relevant_instructions(
                program_id,
                &tx.into(),
                BLOB_ACCOUNT_INSTRUCTION_IDX,
                BLOB_BLOBER_INSTRUCTION_IDX,
            )
        })
        .collect()
}

/// Performs the double-lookup required to find an account at a given account index in an instruction.
/// This is required because the accounts are not stored in the instruction directly, but in a separate
/// account list. It is computed as `payload.account_keys[instruction.accounts[index]]`.
pub fn get_account_at_index(
    tx: &VersionedTransaction,
    instruction: &CompiledInstruction,
    index: usize,
) -> Option<Pubkey> {
    let actual_index = *instruction.accounts.get(index)? as usize;
    tx.message.static_account_keys().get(actual_index).copied()
}
