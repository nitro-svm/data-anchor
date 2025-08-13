use anchor_lang::{
    InstructionData, ToAccountMetas, prelude::Pubkey, solana_program::instruction::Instruction,
};
use data_anchor_blober::instruction::Close;

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for Close {
    type Input = Option<(Pubkey, Pubkey)>;
    const TX_TYPE: TransactionType = TransactionType::CloseBlober;
    const COMPUTE_UNIT_LIMIT: u32 = 10_000;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        let mut certain = vec![args.blober, args.payer];

        if let Some((checkpoint, checkpoint_config)) = &args.input {
            certain.extend_from_slice(&[*checkpoint, *checkpoint_config]);
        }

        certain
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::Close {
            blober: args.blober,
            payer: args.payer,
            checkpoint: args.input.map(|(checkpoint, _)| checkpoint),
            checkpoint_config: args.input.map(|(_, checkpoint_config)| checkpoint_config),
        };

        let data = Self {};

        vec![Instruction {
            program_id: args.program_id,
            accounts: accounts.to_account_metas(None),
            data: data.data(),
        }]
    }

    #[cfg(test)]
    fn generate_arbitrary_input(
        _u: &mut arbitrary::Unstructured,
        _payer: Pubkey,
        blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        let checkoint =
            data_anchor_blober::find_checkpoint_address(data_anchor_blober::id(), blober);
        let checkpoint_config =
            data_anchor_blober::find_checkpoint_config_address(data_anchor_blober::id(), blober);
        Ok(Some((checkoint, checkpoint_config)))
    }
}

#[cfg(test)]
mod tests {
    use data_anchor_blober::instruction::Close;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        Close::test_compute_unit_limit();
    }
}
