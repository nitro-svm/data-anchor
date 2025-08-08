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
    pub slot: u64,
    pub proof: [u8; GROTH16_PROOF_SIZE],
    #[max_len(PROOF_VERIFICATION_KEY_SIZE)]
    pub verification_key: String,
    #[max_len(PROOF_PUBLIC_VALUES_MAX_SIZE)]
    pub public_values: Vec<u8>,
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
        pda_signer_bump: &[u8],
    ) -> Result<()> {
        use crate::{CHECKPOINT_PDA_SIGNER_SEED, CHECKPOINT_SEED, SEED};

        let seeds = &[&[
            SEED,
            CHECKPOINT_SEED,
            CHECKPOINT_PDA_SIGNER_SEED,
            blober.as_ref(),
            pda_signer_bump,
        ][..]];

        let cpi_context = CpiContext::new(data_anchor, account_infos).with_signer(seeds);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_serde() {
        let checkpoint = Checkpoint {
            slot: 123456,
            proof: [1; GROTH16_PROOF_SIZE],
            verification_key: [
                ["0x"].as_slice(),
                ["0"; PROOF_VERIFICATION_KEY_SIZE - 2].as_slice(),
            ]
            .concat()
            .join(""),
            public_values: vec![2; PROOF_PUBLIC_VALUES_MAX_SIZE - 32],
        };

        let serialized = checkpoint.try_to_vec().unwrap();
        let deserialized = Checkpoint::try_from_slice(&serialized).unwrap();
        assert_eq!(checkpoint, deserialized);
        let slot_bytes = [58, 0, 0, 0, 0, 0, 0, 0];
        let groth16_bytes = [
            164, 89, 76, 89, 44, 223, 17, 194, 251, 101, 97, 41, 176, 230, 219, 11, 162, 36, 92,
            57, 173, 127, 79, 160, 29, 151, 174, 198, 52, 107, 226, 56, 172, 240, 32, 218, 15, 158,
            106, 75, 63, 123, 94, 65, 249, 31, 115, 83, 252, 159, 13, 220, 48, 93, 244, 134, 12,
            87, 61, 215, 180, 13, 103, 247, 235, 136, 132, 99, 24, 214, 43, 235, 25, 16, 59, 220,
            201, 75, 88, 109, 240, 33, 14, 71, 60, 153, 181, 225, 16, 197, 255, 58, 185, 142, 168,
            235, 138, 162, 253, 62, 11, 218, 213, 145, 139, 92, 213, 124, 214, 7, 218, 184, 146,
            236, 207, 77, 134, 83, 203, 224, 141, 86, 123, 153, 32, 38, 88, 151, 41, 114, 244, 126,
            46, 75, 209, 182, 185, 186, 89, 203, 201, 147, 126, 200, 232, 224, 187, 81, 229, 26,
            211, 192, 143, 255, 37, 155, 243, 94, 93, 202, 187, 237, 216, 39, 3, 82, 175, 113, 181,
            129, 184, 71, 170, 200, 41, 157, 94, 233, 138, 61, 241, 169, 253, 202, 224, 91, 145,
            99, 5, 187, 189, 140, 205, 41, 112, 6, 12, 14, 86, 45, 63, 35, 84, 28, 99, 230, 188,
            235, 149, 19, 16, 91, 241, 74, 136, 170, 215, 222, 108, 129, 108, 64, 83, 154, 71, 200,
            145, 66, 20, 63, 124, 47, 7, 227, 127, 174, 250, 247, 124, 167, 144, 233, 140, 122,
            233, 253, 244, 30, 139, 185, 240, 133, 144, 197, 144, 88, 74, 237, 166, 119,
        ];
        let verification_key_bytes = [
            66, 0, 0, 0, // size hint
            48, 120, 48, 48, 54, 102, 54, 101, 54, 98, 52, 101, 57, 54, 50, 52, 53, 102, 57, 56,
            101, 48, 52, 55, 98, 49, 55, 99, 99, 50, 98, 100, 52, 98, 101, 55, 56, 102, 98, 48, 50,
            101, 48, 56, 55, 54, 57, 51, 56, 49, 51, 53, 97, 97, 99, 100, 50, 100, 48, 51, 99, 97,
            51, 99, 102, 49,
        ];
        let public_value_bytes = [
            96, 0, 0, 0, // size hint
            3, 145, 232, 95, 237, 197, 86, 36, 133, 7, 130, 192, 44, 20, 165, 56, 142, 241, 131,
            217, 169, 251, 153, 244, 24, 200, 141, 237, 87, 185, 20, 36, 227, 176, 196, 66, 152,
            252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39, 174, 65, 228, 100, 155, 147,
            76, 164, 149, 153, 27, 120, 82, 184, 85, 235, 11, 55, 168, 251, 31, 209, 12, 182, 200,
            154, 42, 5, 29, 196, 222, 85, 16, 54, 24, 5, 250, 103, 79, 41, 124, 74, 196, 185, 94,
            22, 176, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let example = [
            &slot_bytes[..],
            &groth16_bytes[..],
            &verification_key_bytes[..],
            &public_value_bytes[..],
        ]
        .concat();
        let example = &mut example.as_slice();
        let slot = example
            .get(..8)
            .and_then(|s| s.try_into().ok())
            .map(u64::from_le_bytes)
            .unwrap();

        let groth16 = example.get(8..8 + GROTH16_PROOF_SIZE).unwrap();

        let vk_key_size = example
            .get(8 + GROTH16_PROOF_SIZE..8 + GROTH16_PROOF_SIZE + 4)
            .and_then(|s| s.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap() as usize;

        let vk_key_hex = example
            .get(8 + GROTH16_PROOF_SIZE + 4..8 + GROTH16_PROOF_SIZE + 4 + vk_key_size)
            .unwrap();

        let public_values_size = example
            .get(
                8 + GROTH16_PROOF_SIZE + 4 + vk_key_size
                    ..8 + GROTH16_PROOF_SIZE + 4 + vk_key_size + 4,
            )
            .and_then(|s| s.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap() as usize;

        let public_values = example
            .get(
                8 + GROTH16_PROOF_SIZE + 4 + vk_key_size + 4
                    ..8 + GROTH16_PROOF_SIZE + 4 + vk_key_size + 4 + public_values_size,
            )
            .unwrap()
            .to_vec();

        let _checkpoint = Checkpoint {
            proof: groth16.try_into().unwrap(),
            verification_key: String::from_utf8(vk_key_hex.to_vec()).unwrap(),
            public_values,
            slot,
        };
        let _deserialized = Checkpoint::deserialize(example).unwrap();
    }
}
