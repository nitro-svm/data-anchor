use anchor_lang::prelude::*;

use crate::{
    checkpoint::{Checkpoint, CheckpointConfig},
    error::ErrorCode,
    state::blober::Blober,
    CHECKPOINT_CONFIG_SEED, CHECKPOINT_SEED, SEED,
};

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(
        mut,
        close = payer,
        constraint = blober.caller == payer.key(),
    )]
    pub blober: Account<'info, Blober>,

    #[account(
        mut,
        seeds = [
            SEED,
            CHECKPOINT_SEED,
            blober.key().as_ref(),
        ],
        bump
    )]
    pub checkpoint: Option<Account<'info, Checkpoint>>,

    #[account(
        mut,
        seeds = [
            SEED,
            CHECKPOINT_SEED,
            CHECKPOINT_CONFIG_SEED,
            blober.key().as_ref(),
        ],
        bump,
    )]
    pub checkpoint_config: Option<Account<'info, CheckpointConfig>>,

    #[account(mut)]
    pub payer: Signer<'info>,
}

pub fn close_handler(ctx: Context<Close>) -> Result<()> {
    let blober = &mut ctx.accounts.blober;
    let payer = &ctx.accounts.payer;

    let Some(checkpoint) = &ctx.accounts.checkpoint else {
        blober.close(payer.to_account_info())?;
        return Ok(());
    };

    let Some(checkpoint_config) = &ctx.accounts.checkpoint_config else {
        return Err(ErrorCode::CheckpointWithoutConfig.into());
    };

    require!(
        blober.slot == checkpoint.slot && blober.hash == checkpoint.final_hash()?,
        ErrorCode::CheckpointNotUpToDate,
    );

    blober.close(payer.to_account_info())?;
    checkpoint.close(payer.to_account_info())?;
    checkpoint_config.close(payer.to_account_info())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::Close;

    #[test]
    fn test_first_account_is_the_blober() {
        let blober = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let account = Close {
            blober,
            payer,
            checkpoint: None,
            checkpoint_config: None,
        };

        let expected = AccountMeta {
            pubkey: blober,
            is_signer: false,
            is_writable: true,
        };

        let is_signer = None;
        let actual = &account.to_account_metas(is_signer)[0];
        assert_eq!(actual, &expected);
    }
}
