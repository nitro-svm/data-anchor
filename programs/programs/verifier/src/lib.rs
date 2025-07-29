#![allow(unexpected_cfgs)]
// Allow unexpected_cfgs because anchor macros add cfgs which are not in the original code

use anchor_lang::prelude::*;
use data_anchor_blober::checkpoint::Checkpoint;
use sp1_solana::{verify_proof, GROTH16_VK_4_0_0_RC3_BYTES};

declare_id!("oGbL1FPtd7Uix2cwjViMiUciz7UJ2U3gqnxZypXsXQi");

#[program]
pub mod data_anchor_verifier {
    use anchor_lang::solana_program::pubkey::PUBKEY_BYTES;

    use super::*;

    pub fn verify(ctx: Context<Verify>, blober: Pubkey) -> Result<()> {
        let public_value_blober =
            bincode::deserialize::<Pubkey>(&ctx.accounts.checkpoint.public_values[..PUBKEY_BYTES])
                .map_err(|_| error!(SP1Error::InvalidPublicValue))?;

        if public_value_blober != blober {
            return Err(error!(SP1Error::BloberMismatch));
        }

        verify_proof(
            &ctx.accounts.checkpoint.proof,
            &ctx.accounts.checkpoint.public_values,
            &ctx.accounts.checkpoint.verification_key,
            GROTH16_VK_4_0_0_RC3_BYTES,
        )
        .map_err(|_| error!(SP1Error::ProofVerificationFailed))?;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(blober: Pubkey)]
pub struct Verify<'info> {
    #[account(
        seeds = [
            data_anchor_blober::SEED,
            data_anchor_blober::CHECKPOINT_SEED,
            blober.as_ref()
        ],
        seeds::program = data_anchor_blober::ID,
        bump
    )]
    pub checkpoint: Account<'info, Checkpoint>,

    #[account(mut)]
    pub payer: Signer<'info>,
}

#[error_code]
pub enum SP1Error {
    #[msg("Proof verification failed.")]
    ProofVerificationFailed,
    #[msg("Invalid public value")]
    InvalidPublicValue,
    #[msg("Blober missmatch in public values")]
    BloberMismatch,
}
