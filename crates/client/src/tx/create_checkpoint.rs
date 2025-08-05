use anchor_lang::{InstructionData, ToAccountMetas};
use data_anchor_blober::instruction::ConfigureCheckpoint;
use solana_pubkey::Pubkey;
use solana_sdk::{instruction::Instruction, system_program};

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for ConfigureCheckpoint {
    type Input = (Self, Pubkey);
    const TX_TYPE: TransactionType = TransactionType::ConfigureCheckpoint;
    const COMPUTE_UNIT_LIMIT: u32 = 15_500;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::ConfigureCheckpoint {
            checkpoint_config: args.input.1,
            blober: args.blober,
            payer: args.payer,
            system_program: system_program::id(),
        };

        let data = Self {
            authority: args.input.0.authority,
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
        blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        let checkpoint_config =
            data_anchor_blober::find_checkpoint_config_address(data_anchor_blober::id(), blober);
        let config = Self { authority: payer };

        Ok((config, checkpoint_config))
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
