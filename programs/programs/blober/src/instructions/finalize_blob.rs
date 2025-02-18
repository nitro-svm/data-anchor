use anchor_lang::prelude::*;

use crate::{blob::Blob, hash_blob, state::blober::Blober, BLOB_DATA_END, BLOB_DATA_START, SEED};

#[derive(Accounts)]
pub struct FinalizeBlob<'info> {
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
        mut,
        constraint = blober.caller == *payer.key,
    )]
    pub blober: Account<'info, Blober>,

    #[account(mut)]
    pub payer: Signer<'info>,
}

pub fn finalize_blob_handler(ctx: Context<FinalizeBlob>) -> Result<()> {
    if !ctx.accounts.blob.is_complete() {
        panic!("blob is missing chunks, can't be completed in this state")
    }

    let blob_info = ctx.accounts.blob.to_account_info();

    let blob_digest_and_size = &blob_info.data.borrow()[BLOB_DATA_START..BLOB_DATA_END];

    let blob_hash = hash_blob(blob_info.key, blob_digest_and_size);
    let current_slot_num = Clock::get()?.slot;

    let blober_account = &mut ctx.accounts.blober;
    blober_account.store_hash(&blob_hash, current_slot_num);

    let new_hash = blober_account.hash;

    msg!(
        &format!("slot: {current_slot_num:?}, triggering account hash: {blob_hash:?}, accumulated hash: {new_hash:?}")
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::FinalizeBlob;

    #[test]
    fn test_first_account_is_the_blob() {
        let blober = Pubkey::new_unique();
        let blob = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let account = FinalizeBlob {
            blober,
            blob,
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
