use anchor_lang::{prelude::*, solana_program::hash};

use crate::merge_hashes;

#[account]
#[derive(InitSpace)]
pub struct Blober {
    pub hash: [u8; hash::HASH_BYTES],
    pub slot: u64,
    pub caller: Pubkey,
}

impl Blober {
    pub fn store_hash(&mut self, hash: &[u8; hash::HASH_BYTES], slot_num: u64) {
        assert!(slot_num > 0);
        assert!(slot_num >= self.slot);

        if slot_num > self.slot {
            self.hash = *hash;
            self.slot = slot_num;
        } else {
            self.hash = merge_hashes(&self.hash, hash);
        }
    }
}
