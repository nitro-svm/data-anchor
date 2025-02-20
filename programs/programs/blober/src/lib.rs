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

declare_id!("CdczmTavZ6HQwSvEgKJtyrQzKYV4MyU6EZ4Gz5KsULoP");

#[program]
pub mod blober {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, caller: Pubkey) -> Result<()> {
        initialize_handler(ctx, caller)
    }

    pub fn declare_blob(
        ctx: Context<DeclareBlob>,
        timestamp: u64,
        blob_size: u32,
        num_chunks: u16,
    ) -> Result<()> {
        declare_blob_handler(ctx, timestamp, blob_size, num_chunks)
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

pub fn hash_leaf(previous_hash: [u8; 32], chunk_index: u16, chunk_data: &[u8]) -> [u8; 32] {
    hash::hashv(&[&previous_hash, &chunk_index.to_le_bytes(), chunk_data]).to_bytes()
}

pub fn compute_blob_digest<A: AsRef<[u8]>>(chunks: &[(u16, A)]) -> [u8; 32] {
    chunks
        .iter()
        .fold(initial_hash(), |hash, (chunk_index, chunk_data)| {
            hash_leaf(hash, *chunk_index, chunk_data.as_ref())
        })
}

pub fn find_blob_address(payer: Pubkey, timestamp: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[SEED, payer.as_ref(), timestamp.to_le_bytes().as_ref()],
        &id(),
    )
    .0
}

pub fn hash_blob(key: &Pubkey, data: &[u8]) -> [u8; 32] {
    hash::hashv(&[key.as_ref(), data]).to_bytes()
}

pub fn merge_hashes(current: &[u8; 32], new: &[u8; 32]) -> [u8; 32] {
    hash::hashv(&[current, new]).to_bytes()
}
