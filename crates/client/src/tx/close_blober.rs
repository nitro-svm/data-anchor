use anchor_lang::{InstructionData, ToAccountMetas};
use data_anchor_blober::instruction::Close;
use solana_pubkey::Pubkey;
use solana_sdk::instruction::Instruction;

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for Close {
    type Input = ();
    const TX_TYPE: TransactionType = TransactionType::CloseBlober;
    const COMPUTE_UNIT_LIMIT: u32 = 2_400;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::Close {
            blober: args.blober,
            payer: args.payer,
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
        _blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        Ok(())
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
