use anchor_lang::{prelude::*, solana_program::clock::Slot};

use crate::{
    checkpoint::CheckpointConfig, error::ErrorCode, state::checkpoint::Checkpoint,
    CHECKPOINT_CONFIG_SEED, CHECKPOINT_PDA_SIGNER_SEED, CHECKPOINT_SEED, GROTH16_PROOF_SIZE, SEED,
};

#[derive(Accounts)]
#[instruction(blober: Pubkey)]
pub struct CreateCheckpoint<'info> {
    #[account(
        init_if_needed,
        payer = payer,
        space = Checkpoint::DISCRIMINATOR.len() + Checkpoint::INIT_SPACE,
        seeds = [
            SEED,
            CHECKPOINT_SEED,
            blober.as_ref(),
        ],
        bump
    )]
    pub checkpoint: Account<'info, Checkpoint>,

    #[account(
        seeds = [
            SEED,
            CHECKPOINT_SEED,
            CHECKPOINT_CONFIG_SEED,
            blober.as_ref(),
        ],
        bump,
    )]
    pub checkpoint_config: Account<'info, CheckpointConfig>,

    #[account(
        mut,
        seeds = [
            SEED,
            CHECKPOINT_SEED,
            CHECKPOINT_PDA_SIGNER_SEED,
            blober.as_ref(),
        ],
        seeds::program = checkpoint_config.authority,
        bump,
    )]
    pub pda_signer: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn create_checkpoint_handler(
    ctx: Context<CreateCheckpoint>,
    blober: Pubkey,
    proof: [u8; GROTH16_PROOF_SIZE],
    public_values: Vec<u8>,
    verification_key: String,
    slot: Slot,
) -> Result<()> {
    let new_checkpoint = Checkpoint::new(proof, public_values, verification_key, slot)?;

    let public_value_blober = new_checkpoint.blober()?;

    if public_value_blober != blober {
        return Err(error!(ErrorCode::BloberMismatch));
    }

    if ctx.accounts.checkpoint.slot == 0 {
        return ctx.accounts.checkpoint.store(new_checkpoint);
    }

    if public_value_blober != ctx.accounts.checkpoint.blober()? {
        return Err(error!(ErrorCode::BloberMismatch));
    }

    if ctx.accounts.checkpoint.slot >= slot {
        return Err(error!(ErrorCode::SlotTooLow));
    }

    if new_checkpoint.initial_hash()? != ctx.accounts.checkpoint.final_hash()? {
        return Err(error!(ErrorCode::ProofHashMismatch));
    }

    ctx.accounts.checkpoint.store(new_checkpoint)
}

#[cfg(all(test, feature = "sp1"))]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::CreateCheckpoint;

    #[test]
    fn test_first_account_is_the_checkpoint() {
        let checkpoint = Pubkey::new_unique();
        let checkpoint_config = Pubkey::new_unique();
        let pda_signer = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let system_program = Pubkey::new_unique();

        let account = CreateCheckpoint {
            checkpoint,
            checkpoint_config,
            payer,
            pda_signer,
            system_program,
        };

        let expected = AccountMeta {
            pubkey: checkpoint,
            is_signer: false,
            is_writable: true,
        };

        let is_signer = None;
        let actual = &account.to_account_metas(is_signer)[0];
        assert_eq!(actual, &expected);
    }
}
