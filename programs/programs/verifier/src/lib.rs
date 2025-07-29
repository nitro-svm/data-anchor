#![allow(unexpected_cfgs)]
// Allow unexpected_cfgs because anchor macros add cfgs which are not in the original code

use anchor_lang::prelude::*;
use sp1_solana::{verify_proof, GROTH16_VK_4_0_0_RC3_BYTES};

declare_id!("oGbL1FPtd7Uix2cwjViMiUciz7UJ2U3gqnxZypXsXQi");

#[program]
pub mod sp1_anchor_verifier {
    use super::*;

    pub fn verify(_ctx: Context<Verify>, instruction_data: Vec<u8>) -> Result<()> {
        // Deserialize the InstructionData
        let idata = InstructionData::try_from_slice(&instruction_data)
            .map_err(|_| error!(SP1Error::InvalidInstructionData))?;

        let vk = GROTH16_VK_4_0_0_RC3_BYTES;

        verify_proof(
            &idata.groth16_proof.proof,
            &idata.groth16_proof.sp1_public_inputs,
            &idata.vkey_hash,
            vk,
        )
        .map_err(|_| error!(SP1Error::ProofVerificationFailed))?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Verify<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SP1Groth16Proof {
    pub proof: Vec<u8>,
    pub sp1_public_inputs: Vec<u8>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InstructionData {
    pub groth16_proof: SP1Groth16Proof,
    pub vkey_hash: String,
}

#[error_code]
pub enum SP1Error {
    #[msg("Invalid instruction data.")]
    InvalidInstructionData,
    #[msg("Proof verification failed.")]
    ProofVerificationFailed,
}
