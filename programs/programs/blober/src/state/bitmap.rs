use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize, InitSpace};

use crate::{constants::CHUNKS_BITMAP_SIZE, error::ErrorCode};

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, InitSpace, AnchorSerialize, AnchorDeserialize,
)]
pub struct Bitmap {
    pub num_chunks: u16,
    pub map: [u8; CHUNKS_BITMAP_SIZE as usize],
}

fn byte_containing_idx(idx: u16) -> usize {
    (idx / 8) as usize
}

fn bit_offset_for_idx(idx: u16) -> usize {
    (idx % 8) as usize
}

impl Bitmap {
    pub fn new(number_of_chunks: u16) -> Self {
        Self {
            num_chunks: number_of_chunks,
            map: [0; CHUNKS_BITMAP_SIZE as usize],
        }
    }

    /// Mark the bit corresponding to the given index and return whether it was already set, panicking if it is out of bounds
    ///
    /// Panicking is the "correct" behavior since it will cause the Solana tx to revert
    pub fn test_and_set(&mut self, idx: u16) -> std::result::Result<(), ErrorCode> {
        if idx >= self.num_chunks || byte_containing_idx(idx) > self.map.len() {
            panic!("chunk {idx} out of bounds");
        }

        let byte_offset = byte_containing_idx(idx);
        let byte = self.map[byte_offset];

        let bit_offset = bit_offset_for_idx(idx);
        let bit_mask = 1 << bit_offset;

        if byte & bit_mask != 0 {
            return Err(ErrorCode::DuplicateChunk);
        }

        self.map[byte_offset] = byte | bit_mask;

        Ok(())
    }

    /// Check if all bits are set to 1.
    pub fn is_complete(&self) -> bool {
        let limit = byte_containing_idx(self.num_chunks);
        // Every byte except the last.
        for i in 0..limit {
            if self.map[i] != 0b11111111 {
                return false;
            }
        }
        // Then check the last one separately, which is not a full byte of ones.
        self.map[limit] == (1 << bit_offset_for_idx(self.num_chunks) as u8) - 1
    }
}
