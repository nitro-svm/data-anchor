use anchor_lang::{
    Discriminator, InstructionData, Space, ToAccountMetas,
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
};
use data_anchor_blober::{
    checkpoint::{Checkpoint, CheckpointConfig},
    find_checkpoint_address, find_checkpoint_config_address,
    instruction::ConfigureCheckpoint,
    state::blober::Blober,
};

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for ConfigureCheckpoint {
    type Input = Pubkey;
    const TX_TYPE: TransactionType = TransactionType::ConfigureCheckpoint;
    const COMPUTE_UNIT_LIMIT: u32 = 34_000;
    const LOADED_ACCOUNT_DATA_SIZE: u32 = (Blober::DISCRIMINATOR.len()
        + Blober::INIT_SPACE
        + Checkpoint::DISCRIMINATOR.len()
        + Checkpoint::INIT_SPACE
        + CheckpointConfig::DISCRIMINATOR.len()
        + CheckpointConfig::INIT_SPACE) as u32;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![
            find_checkpoint_address(args.program_id, args.blober),
            find_checkpoint_config_address(args.program_id, args.blober),
            args.payer,
        ]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::ConfigureCheckpoint {
            checkpoint: find_checkpoint_address(args.program_id, args.blober),
            checkpoint_config: find_checkpoint_config_address(args.program_id, args.blober),
            blober: args.blober,
            payer: args.payer,
            system_program: system_program::id(),
        };

        let data = Self {
            authority: args.input,
        };

        vec![Instruction {
            program_id: args.program_id,
            accounts: accounts.to_account_metas(None),
            data: data.data(),
        }]
    }

    #[cfg(test)]
    fn generate_arbitrary_input(
        _u: &mut arbitrary::Unstructured,
        payer: Pubkey,
        _blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        Ok(payer)
    }
}

#[cfg(test)]
mod tests {
    use data_anchor_blober::instruction::ConfigureCheckpoint;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        ConfigureCheckpoint::test_compute_unit_limit();
    }
}
