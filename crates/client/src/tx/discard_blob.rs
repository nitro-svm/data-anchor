use anchor_lang::{
    InstructionData, ToAccountMetas, prelude::Pubkey, solana_program::instruction::Instruction,
};
use data_anchor_blober::instruction::DiscardBlob;

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for DiscardBlob {
    type Input = Pubkey;
    const TX_TYPE: TransactionType = TransactionType::DiscardBlob;
    const COMPUTE_UNIT_LIMIT: u32 = 40_000;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::DiscardBlob {
            blob: args.input,
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
        u: &mut arbitrary::Unstructured,
        payer: Pubkey,
        blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        let timestamp: u64 = u.arbitrary()?;
        let blob_size: usize = u.arbitrary()?;
        let blob = data_anchor_blober::find_blob_address(
            data_anchor_blober::id(),
            payer,
            blober,
            timestamp,
            blob_size,
        );

        Ok(blob)
    }
}

#[cfg(test)]
mod tests {
    use data_anchor_blober::instruction::DiscardBlob;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        DiscardBlob::test_compute_unit_limit();
    }
}
