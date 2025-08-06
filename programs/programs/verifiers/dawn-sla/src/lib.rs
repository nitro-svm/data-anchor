#![allow(unexpected_cfgs)]
// Allow unexpected_cfgs because anchor macros add cfgs which are not in the original code

use anchor_lang::prelude::*;
use data_anchor_blober::checkpoint::{Checkpoint, CheckpointConfig};

declare_id!("A4Ks3ivLsBVvysaf6BMTNjdcvig1Ti8cSkoMBqYDdsGF");

#[program]
pub mod data_anchor_dawn_sla_verifier {

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

        #[cfg(feature = "sp1")]
        checkpoint.verify_zk_proof()?;

        let sla_bytes = checkpoint
            .non_base_commitments()
            .ok_or_else(|| error!(DawnSlaError::NoSlaCommitmentsFound))?;

        let sla_score: f64 = bincode::deserialize(sla_bytes)
            .map_err(|_| error!(DawnSlaError::InvalidSlaScoreFormat))?;

        require_gte!(sla_score, 0.0, DawnSlaError::InvalidScore);

        checkpoint.cpi_create_checkpoint(
            blober,
            ctx.accounts.data_anchor.to_account_info(),
            ctx.accounts.into(),
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

impl<'info> From<&mut Verify<'info>>
    for data_anchor_blober::cpi::accounts::CreateCheckpoint<'info>
{
    fn from(verify: &mut Verify<'info>) -> Self {
        data_anchor_blober::cpi::accounts::CreateCheckpoint {
            checkpoint: verify.checkpoint.to_account_info(),
            checkpoint_config: verify.checkpoint_config.to_account_info(),
            pda_signer: verify.pda_signer.to_account_info(),
            payer: verify.payer.to_account_info(),
            system_program: verify.system_program.to_account_info(),
        }
    }
}

#[error_code]
pub enum DawnSlaError {
    #[msg("No SLA commitments found in public values")]
    NoSlaCommitmentsFound,
    #[msg("Invalid SLA score format")]
    InvalidSlaScoreFormat,
    #[msg("Invalid SLA score, must be greater than or equal to 0")]
    InvalidScore,
}
