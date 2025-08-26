use anchor_lang::{prelude::*, solana_program::hash};

use crate::{merge_hashes, MAX_NAMESPACE_LENGTH};

#[account]
#[derive(Debug, InitSpace, PartialEq, Eq, PartialOrd, Ord)]
pub struct Blober {
    pub hash: [u8; hash::HASH_BYTES],
    pub slot: u64,
    pub caller: Pubkey,
    #[max_len(MAX_NAMESPACE_LENGTH)]
    pub namespace: String,
}

impl Blober {
    pub fn store_hash(&mut self, hash: &[u8; hash::HASH_BYTES], slot_num: u64) {
        assert!(slot_num > 0);
        assert!(slot_num >= self.slot);

        self.slot = slot_num;
        self.hash = merge_hashes(&self.hash, hash);
    }
}
