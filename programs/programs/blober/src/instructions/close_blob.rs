use anchor_lang::prelude::*;

use crate::{blob::Blob, state::blober::Blober, SEED};

#[derive(Accounts)]
pub struct DiscardBlob<'info> {
    #[account(
        mut,
        close = payer,
        seeds = [
            SEED,
            payer.key().as_ref(),
            blob.timestamp.to_le_bytes().as_ref()
        ],
        bump = blob.bump,
    )]
    pub blob: Account<'info, Blob>,

    #[account(
        constraint = blober.caller == *payer.key,
    )]
    pub blober: Account<'info, Blober>,

    #[account(mut)]
    pub payer: Signer<'info>,
}

pub fn discard_blob_handler(_ctx: Context<DiscardBlob>) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::DiscardBlob;

    #[test]
    fn test_first_account_is_the_blob() {
        let blob = Pubkey::new_unique();
        let blober = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let account = DiscardBlob {
            blob,
            blober,
            payer,
        };

        let expected = AccountMeta {
            pubkey: blob,
            is_signer: false,
            is_writable: true,
        };

        let is_signer = None;
        let actual = &account.to_account_metas(is_signer)[0];
        assert_eq!(actual, &expected);
    }
}
