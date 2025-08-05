use anchor_lang::{
    InstructionData, ToAccountMetas,
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
};
use data_anchor_blober::instruction::DeclareBlob;

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for DeclareBlob {
    type Input = (Self, Pubkey);
    const TX_TYPE: TransactionType = TransactionType::DeclareBlob;
    const COMPUTE_UNIT_LIMIT: u32 = 44_000;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input.1, args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::DeclareBlob {
            blob: args.input.1,
            blober: args.blober,
            payer: args.payer,
            system_program: system_program::id(),
        };

        let data = Self {
            timestamp: args.input.0.timestamp,
            blob_size: args.input.0.blob_size,
        };

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

        Ok((
            DeclareBlob {
                timestamp,
                blob_size: blob_size as u32,
            },
            blob,
        ))
    }
}

#[cfg(test)]
mod tests {
    use data_anchor_blober::instruction::DeclareBlob;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        DeclareBlob::test_compute_unit_limit();
    }
}
