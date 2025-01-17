use std::time::{Duration, SystemTime};

use anchor_lang::{prelude::*, solana_program::hash};

use super::bitmap::Bitmap;
use crate::{
    constants::{BLOB_SLOT_INCREMENTAL_DELAY_LIMIT, BLOB_SLOT_TOTAL_DELAY_LIMIT, CHUNK_SIZE},
    error::ErrorCode,
    hash_leaf, initial_hash,
};

#[cfg(test)]
mod tests;

#[account]
#[derive(InitSpace)]
pub struct Blob {
    digest: [u8; hash::HASH_BYTES],
    size: u32,
    bitmap: Bitmap,
    pub(crate) timestamp: u64,
    pub(crate) created_at: u64,
    pub(crate) last_updated_at: u64,
    pub(crate) bump: u8,
}

impl std::fmt::Debug for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Accumulator")
            .field("digest", &hex::encode(self.digest))
            .field(
                "timestamp",
                &(SystemTime::UNIX_EPOCH + Duration::from_secs(self.timestamp)),
            )
            .field("last_updated", &self.last_updated_at)
            .field("num_chunks", &self.bitmap.num_chunks)
            .finish()
    }
}

impl Blob {
    pub fn new(slot: u64, timestamp: u64, blob_size: u32, num_chunks: u16, bump: u8) -> Self {
        if blob_size != 0 && num_chunks != 0 {
            let size_min = (num_chunks as u32 - 1) * CHUNK_SIZE as u32;
            let size_max = size_min + CHUNK_SIZE as u32;
            assert!(
                blob_size >= size_min && blob_size <= size_max,
                "blob size must be between {size_min} and {size_max}, got {blob_size}"
            );
        }

        Self {
            digest: initial_hash(),
            timestamp,
            size: blob_size,
            created_at: slot,
            last_updated_at: slot,
            bitmap: Bitmap::new(num_chunks),
            bump,
        }
    }

    pub fn blob_digest(&self) -> &[u8; hash::HASH_BYTES] {
        &self.digest
    }

    pub fn is_complete(&self) -> bool {
        self.bitmap.is_complete()
    }

    pub fn insert(&mut self, slot: u64, chunk_index: u16, chunk_data: &[u8]) {
        if self.check_preconditions(slot, chunk_index).is_err() {
            return;
        }
        self.digest = hash_leaf(self.digest, chunk_index, chunk_data);
    }

    fn check_preconditions(
        &mut self,
        slot: u64,
        chunk_index: u16,
    ) -> std::result::Result<(), ErrorCode> {
        if chunk_index >= self.bitmap.num_chunks {
            panic!("chunk {chunk_index} out of bounds");
        }
        self.check_time_limits(slot);

        self.bitmap.test_and_set(chunk_index)
    }

    fn check_time_limits(&mut self, slot: u64) {
        if slot.abs_diff(self.created_at) > BLOB_SLOT_TOTAL_DELAY_LIMIT {
            panic!("blob created at {} is too far in the past", self.created_at);
        }
        if slot.abs_diff(self.last_updated_at) > BLOB_SLOT_INCREMENTAL_DELAY_LIMIT {
            panic!(
                "blob last updated at {} is too far in the past",
                self.last_updated_at
            );
        }
        self.last_updated_at = slot;
    }
}
