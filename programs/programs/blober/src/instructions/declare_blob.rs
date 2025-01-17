use anchor_lang::{prelude::*, Discriminator};

use crate::{blob::Blob, SEED};

#[derive(Accounts)]
#[instruction(timestamp: u64)]
pub struct DeclareBlob<'info> {
    #[account(
        init,
        payer = payer,
        space = Blob::DISCRIMINATOR.len() + Blob::INIT_SPACE,
        seeds = [
            SEED,
            payer.key().as_ref(),
            timestamp.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub blob: Account<'info, Blob>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn declare_blob_handler(
    ctx: Context<DeclareBlob>,
    timestamp: u64,
    blob_size: u32,
    num_chunks: u16,
) -> Result<()> {
    ctx.accounts.blob.set_inner(Blob::new(
        Clock::get()?.slot,
        timestamp,
        blob_size,
        num_chunks,
        ctx.bumps.blob,
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::DeclareBlob;

    #[test]
    fn test_first_account_is_the_blob() {
        let blob = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let system_program = Pubkey::new_unique();

        let account = DeclareBlob {
            blob,
            payer,
            system_program,
        };

        let expected = AccountMeta {
            pubkey: blob,
            is_signer: false,
            is_writable: true,
        };

        let is_signer = None;
        let account_metas = account.to_account_metas(is_signer);
        let actual = &account_metas[0];
        assert_eq!(actual, &expected);
    }
}
