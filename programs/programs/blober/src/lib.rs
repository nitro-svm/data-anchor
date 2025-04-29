#![doc = include_str!("../../../README.md")]
#![allow(unexpected_cfgs)]
// Allow unexpected_cfgs because anchor macros add cfgs which are not in the original code

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;
#[cfg(test)]
mod tests;

use anchor_lang::{prelude::*, solana_program::hash};
pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("8xAuVgAygVN2sPXJzycT7AU7c9ZUJkG357HonxdFXjyc");

#[program]
pub mod blober {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, namespace: String, trusted: Pubkey) -> Result<()> {
        initialize_handler(ctx, namespace, trusted)
    }

    pub fn declare_blob(ctx: Context<DeclareBlob>, timestamp: u64, blob_size: u32) -> Result<()> {
        declare_blob_handler(ctx, timestamp, blob_size)
    }

    pub fn insert_chunk(ctx: Context<InsertChunk>, idx: u16, data: Vec<u8>) -> Result<()> {
        insert_chunk_handler(ctx, idx, data)
    }

    pub fn finalize_blob(ctx: Context<FinalizeBlob>) -> Result<()> {
        finalize_blob_handler(ctx)
    }

    pub fn discard_blob(ctx: Context<DiscardBlob>) -> Result<()> {
        discard_blob_handler(ctx)
    }

    pub fn close(ctx: Context<Close>) -> Result<()> {
        close_handler(ctx)
    }
}

/// Hashes a single chunk on top of the previous hash.
pub fn hash_leaf(previous_hash: [u8; 32], chunk_index: u16, chunk_data: &[u8]) -> [u8; 32] {
    hash::hashv(&[&previous_hash, &chunk_index.to_le_bytes(), chunk_data]).to_bytes()
}

/// Computes a blob digest of all the chunks of a blob.
pub fn compute_blob_digest<A: AsRef<[u8]>>(chunks: &[(u16, A)]) -> [u8; 32] {
    chunks
        .iter()
        .fold(initial_hash(), |hash, (chunk_index, chunk_data)| {
            hash_leaf(hash, *chunk_index, chunk_data.as_ref())
        })
}

/// Retrieves the PDA address of a blob account to store chunks and digest the data.
pub fn find_blob_address(
    program_id: Pubkey,
    payer: Pubkey,
    blober: Pubkey,
    timestamp: u64,
    blob_size: usize,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            SEED,
            payer.as_ref(),
            blober.as_ref(),
            timestamp.to_le_bytes().as_ref(),
            (blob_size as u32).to_le_bytes().as_ref(),
        ],
        &program_id,
    )
    .0
}

/// Retrieves the PDA address of a blober account to store digests and finalize blobs.
pub fn find_blober_address(program_id: Pubkey, payer: Pubkey, namespace: &str) -> Pubkey {
    Pubkey::find_program_address(&[SEED, payer.as_ref(), namespace.as_bytes()], &program_id).0
}

/// Computes the hashed state of a blob account.
pub fn hash_blob(key: &Pubkey, data: &[u8]) -> [u8; 32] {
    hash::hashv(&[key.as_ref(), data]).to_bytes()
}

/// Merges two hashes into a single one. Used when there are multiple blobs to finalize in the same
/// slot.
pub fn merge_hashes(current: &[u8; 32], new: &[u8; 32]) -> [u8; 32] {
    hash::hashv(&[current, new]).to_bytes()
}
