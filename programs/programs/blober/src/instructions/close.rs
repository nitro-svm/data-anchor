use anchor_lang::prelude::*;

use crate::state::blober::Blober;

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(
        mut,
        close = payer,
        constraint = blober.caller == *payer.key,
    )]
    pub blober: Account<'info, Blober>,

    #[account(mut)]
    pub payer: Signer<'info>,
}

pub fn close_handler(_ctx: Context<Close>) -> Result<()> {
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

        let account = Close { blober, payer };

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
