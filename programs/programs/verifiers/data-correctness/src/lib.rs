#![allow(unexpected_cfgs)]
// Allow unexpected_cfgs because anchor macros add cfgs which are not in the original code

use anchor_lang::prelude::*;
use data_anchor_blober::checkpoint::{Checkpoint, CheckpointConfig};

declare_id!("oGbL1FPtd7Uix2cwjViMiUciz7UJ2U3gqnxZypXsXQi");

#[program]
pub mod data_anchor_data_correctness_verifier {

    use super::*;

    pub fn verify(
        ctx: Context<Verify>,
        blober: Pubkey,
        proof: [u8; data_anchor_blober::GROTH16_PROOF_SIZE],
        public_values: Vec<u8>,
        verification_key: String,
        slot: u64,
    ) -> Result<()> {
        let checkpoint = Checkpoint::new(proof, public_values, verification_key, slot)?;

        checkpoint.verify_zk_proof()?;

        checkpoint.cpi_create_checkpoint(
            blober,
            ctx.accounts.data_anchor.to_account_info(),
            data_anchor_blober::cpi::accounts::CreateCheckpoint {
                checkpoint: ctx.accounts.checkpoint.to_account_info(),
                checkpoint_config: ctx.accounts.checkpoint_config.to_account_info(),
                pda_signer: ctx.accounts.pda_signer.to_account_info(),
                payer: ctx.accounts.payer.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
            },
        )
    }
}

#[derive(Accounts)]
#[instruction(blober: Pubkey)]
pub struct Verify<'info> {
    #[account(
        seeds = [
            data_anchor_blober::SEED,
            data_anchor_blober::CHECKPOINT_SEED,
            blober.as_ref(),
        ],
        seeds::program = data_anchor_blober::ID,
        bump
    )]
    pub checkpoint: Account<'info, Checkpoint>,

    #[account(
        seeds = [
            data_anchor_blober::SEED,
            data_anchor_blober::CHECKPOINT_SEED,
            data_anchor_blober::CHECKPOINT_CONFIG_SEED,
            blober.as_ref(),
        ],
        seeds::program = data_anchor_blober::ID,
        bump
    )]
    pub checkpoint_config: Account<'info, CheckpointConfig>,

    #[account(
        mut,
        seeds = [
            data_anchor_blober::SEED,
            data_anchor_blober::CHECKPOINT_SEED,
            blober.as_ref(),
        ],
        bump
    )]
    pub pda_signer: SystemAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub data_anchor: Program<'info, data_anchor_blober::program::Blober>,

    pub system_program: Program<'info, System>,
}
