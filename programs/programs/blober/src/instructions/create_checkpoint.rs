use anchor_lang::{
    prelude::*,
    solana_program::{clock::Slot, pubkey::PUBKEY_BYTES},
    Discriminator,
};

use crate::{
    error::ErrorCode, state::checkpoint::Checkpoint, CHECKPOINT_SEED, GROTH16_PROOF_SIZE,
    PROOF_PUBLIC_VALUES_SIZE, SEED,
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

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn create_checkpoint_handler(
    ctx: Context<CreateCheckpoint>,
    blober: Pubkey,
    proof: [u8; GROTH16_PROOF_SIZE],
    public_values: [u8; PROOF_PUBLIC_VALUES_SIZE],
    verification_key: String,
    slot: Slot,
) -> Result<()> {
    let public_value_blober = bincode::deserialize::<Pubkey>(&public_values[..PUBKEY_BYTES])
        .map_err(|_| error!(ErrorCode::InvalidPublicValue))?;
    if public_value_blober != blober {
        return Err(error!(ErrorCode::InvalidPublicValue));
    }
    ctx.accounts
        .checkpoint
        .store(proof, public_values, verification_key, slot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        ToAccountMetas,
    };

    use crate::accounts::CreateCheckpoint;

    #[test]
    fn test_first_account_is_the_checkpoint() {
        let checkpoint = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let system_program = Pubkey::new_unique();

        let account = CreateCheckpoint {
            checkpoint,
            payer,
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
