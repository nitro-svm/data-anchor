use anchor_lang::prelude::*;

use crate::{
    checkpoint::{Checkpoint, CheckpointConfig},
    error::ErrorCode,
    state::blober::Blober,
    CHECKPOINT_CONFIG_SEED, CHECKPOINT_SEED, SEED,
};

#[derive(Accounts)]
pub struct ConfigureCheckpoint<'info> {
    #[account(
        init_if_needed,
        payer = payer,
        space = Checkpoint::DISCRIMINATOR.len() + Checkpoint::INIT_SPACE,
        seeds = [
            SEED,
            CHECKPOINT_SEED,
            blober.key().as_ref(),
        ],
        bump
    )]
    pub checkpoint: Account<'info, Checkpoint>,

    #[account(
        init_if_needed,
        payer = payer,
        space = CheckpointConfig::DISCRIMINATOR.len() + CheckpointConfig::INIT_SPACE,
        seeds = [
            SEED,
            CHECKPOINT_SEED,
            CHECKPOINT_CONFIG_SEED,
            blober.key().as_ref(),
        ],
        bump,
    )]
    pub checkpoint_config: Account<'info, CheckpointConfig>,

    #[account(
        constraint = blober.caller == payer.key(),
    )]
    pub blober: Account<'info, Blober>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn configure_checkpoint_handler(
    ctx: Context<ConfigureCheckpoint>,
    authority: Pubkey,
) -> Result<()> {
    if ctx.accounts.checkpoint_config.authority != Pubkey::default() {
        require_keys_eq!(
            ctx.accounts.checkpoint_config.authority,
            ctx.accounts.payer.key(),
            ErrorCode::Unauthorized
        );
    }

    ctx.accounts.checkpoint_config.set_inner(CheckpointConfig {
        authority,
        blober: ctx.accounts.blober.key(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::ConfigureCheckpoint;

    #[test]
    fn test_first_account_is_the_checkpoint_config() {
        let checkpoint = Pubkey::new_unique();
        let checkpoint_config = Pubkey::new_unique();
        let blober = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let system_program = Pubkey::new_unique();

        let account = ConfigureCheckpoint {
            checkpoint,
            checkpoint_config,
            blober,
            payer,
            system_program,
        };

        let expected = AccountMeta {
            pubkey: checkpoint_config,
            is_signer: false,
            is_writable: true,
        };

        let is_signer = None;
        let actual = &account.to_account_metas(is_signer)[1];
        assert_eq!(actual, &expected);
    }
}
