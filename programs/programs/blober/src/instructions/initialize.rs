use anchor_lang::{prelude::*, Discriminator};

use crate::{state::blober::Blober, SEED};

#[derive(Accounts)]
#[instruction(namespace: String)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = Blober::DISCRIMINATOR.len() + Blober::INIT_SPACE,
        seeds = [
            SEED,
            payer.key().as_ref(),
            namespace.as_bytes()
        ],
        bump
    )]
    pub blober: Account<'info, Blober>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_handler(
    ctx: Context<Initialize>,
    _namespace: String,
    trusted: Pubkey,
) -> Result<()> {
    ctx.accounts.blober.caller = trusted;
    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::Initialize;

    #[test]
    fn test_first_account_is_the_blober() {
        let blober = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let system_program = Pubkey::new_unique();

        let account = Initialize {
            blober,
            payer,
            system_program,
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
