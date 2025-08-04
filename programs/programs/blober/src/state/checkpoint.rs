use anchor_lang::{
    prelude::*,
    solana_program::{clock::Slot, hash::HASH_BYTES, pubkey::PUBKEY_BYTES},
};

use crate::{
    error::ErrorCode, GROTH16_PROOF_SIZE, PROOF_PUBLIC_VALUES_MAX_SIZE, PROOF_VERIFICATION_KEY_SIZE,
};

#[account]
#[derive(Debug, InitSpace, PartialEq, Eq, PartialOrd, Ord)]
pub struct Checkpoint {
    pub proof: [u8; GROTH16_PROOF_SIZE],
    #[max_len(PROOF_PUBLIC_VALUES_MAX_SIZE)]
    pub public_values: Vec<u8>,
    #[max_len(PROOF_VERIFICATION_KEY_SIZE)]
    pub verification_key: String,
    pub slot: u64,
}

impl Checkpoint {
    pub fn new(
        proof: [u8; GROTH16_PROOF_SIZE],
        public_values: Vec<u8>,
        verification_key: String,
        slot: Slot,
    ) -> Result<Self> {
        if public_values.len() > PROOF_PUBLIC_VALUES_MAX_SIZE {
            return Err(error!(ErrorCode::PublicValuesExceedMaxSize));
        }
        if verification_key.len() != PROOF_VERIFICATION_KEY_SIZE {
            return Err(error!(ErrorCode::InvalidVerificationKeySize));
        }

        Ok(Self {
            proof,
            public_values,
            verification_key,
            slot,
        })
    }

    pub fn store(&mut self, new: Self) -> Result<()> {
        if new.public_values.len() > PROOF_PUBLIC_VALUES_MAX_SIZE {
            return Err(error!(ErrorCode::PublicValuesExceedMaxSize));
        }
        if new.verification_key.len() != PROOF_VERIFICATION_KEY_SIZE {
            return Err(error!(ErrorCode::InvalidVerificationKeySize));
        }

        self.proof = new.proof;
        self.public_values = new.public_values;
        self.verification_key = new.verification_key;
        self.slot = new.slot;
        Ok(())
    }

    #[cfg(feature = "sp1")]
    pub fn verify_zk_proof(&self) -> Result<()> {
        sp1_solana::verify_proof(
            &self.proof,
            &self.public_values,
            &self.verification_key,
            sp1_solana::GROTH16_VK_5_0_0_BYTES,
        )
        .map_err(|_| error!(ErrorCode::ProofVerificationFailed))
    }

    pub fn blober(&self) -> Result<Pubkey> {
        bincode::deserialize::<Pubkey>(
            self.public_values
                .get(..PUBKEY_BYTES)
                .ok_or_else(|| error!(ErrorCode::InvalidPublicValue))?,
        )
        .map_err(|_| error!(ErrorCode::InvalidPublicValue))
    }

    pub fn initial_hash(&self) -> Result<[u8; 32]> {
        bincode::deserialize::<[u8; 32]>(
            self.public_values
                .get(PUBKEY_BYTES..PUBKEY_BYTES + HASH_BYTES)
                .ok_or_else(|| error!(ErrorCode::InvalidPublicValue))?,
        )
        .map_err(|_| error!(ErrorCode::InvalidPublicValue))
    }

    pub fn final_hash(&self) -> Result<[u8; 32]> {
        bincode::deserialize::<[u8; 32]>(
            self.public_values
                .get(PUBKEY_BYTES + HASH_BYTES..)
                .ok_or_else(|| error!(ErrorCode::InvalidPublicValue))?,
        )
        .map_err(|_| error!(ErrorCode::InvalidPublicValue))
    }

    pub fn non_base_commitments(&self) -> Option<&[u8]> {
        self.public_values.get(PUBKEY_BYTES + HASH_BYTES * 2..)
    }

    #[cfg(feature = "cpi")]
    pub fn cpi_create_checkpoint<'info>(
        &self,
        blober: Pubkey,
        data_anchor: AccountInfo<'info>,
        account_infos: crate::cpi::accounts::CreateCheckpoint<'info>,
    ) -> Result<()> {
        use crate::{CHECKPOINT_SEED, SEED};

        let seeds = &[&[SEED, CHECKPOINT_SEED, blober.as_ref()][..]];

        let cpi_context = CpiContext::new_with_signer(data_anchor, account_infos, seeds);

        crate::cpi::create_checkpoint(
            cpi_context,
            blober,
            self.proof,
            self.public_values.clone(),
            self.verification_key.clone(),
            self.slot,
        )
    }
}

#[account]
#[derive(Debug, InitSpace, PartialEq, Eq, PartialOrd, Ord)]
pub struct CheckpointConfig {
    pub blober: Pubkey,
    pub authority: Pubkey,
}
