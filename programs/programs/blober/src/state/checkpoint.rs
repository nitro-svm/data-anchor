use anchor_lang::{prelude::*, solana_program::clock::Slot};

use crate::{GROTH16_PROOF_SIZE, PROOF_PUBLIC_VALUES_SIZE, PROOF_VERIFICATION_KEY_SIZE};

#[account]
#[derive(Debug, InitSpace, PartialEq, Eq, PartialOrd, Ord)]
pub struct Checkpoint {
    pub proof: [u8; GROTH16_PROOF_SIZE],
    pub public_values: [u8; PROOF_PUBLIC_VALUES_SIZE],
    #[max_len(PROOF_VERIFICATION_KEY_SIZE)]
    pub verification_key: String,
    pub slot: u64,
}

impl Checkpoint {
    pub fn store(
        &mut self,
        proof: [u8; GROTH16_PROOF_SIZE],
        public_values: [u8; PROOF_PUBLIC_VALUES_SIZE],
        verification_key: String,
        slot: Slot,
    ) {
        self.proof = proof;
        self.public_values = public_values;
        self.verification_key = verification_key;
        self.slot = slot;
    }
}
