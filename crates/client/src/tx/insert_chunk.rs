use anchor_lang::{
    Discriminator, InstructionData, Space, ToAccountMetas, prelude::Pubkey,
    solana_program::instruction::Instruction,
};
use data_anchor_blober::{blob::Blob, instruction::InsertChunk, state::blober::Blober};

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for InsertChunk {
    type Input = (Self, Pubkey);
    const TX_TYPE: TransactionType = TransactionType::InsertChunk(0);
    const COMPUTE_UNIT_LIMIT: u32 = 6_000;
    const LOADED_ACCOUNT_DATA_SIZE: u32 = (Blober::DISCRIMINATOR.len()
        + Blober::INIT_SPACE
        + Blob::DISCRIMINATOR.len()
        + Blob::INIT_SPACE) as u32;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input.1, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::InsertChunk {
            blob: args.input.1,
            blober: args.blober,
            payer: args.payer,
        };

        let data = Self {
            data: args.input.0.data.clone(),
            idx: args.input.0.idx,
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
        let idx: u16 = u.arbitrary()?;
        let data: Vec<u8> = u.arbitrary()?;
        let blob = data_anchor_blober::find_blob_address(
            data_anchor_blober::id(),
            payer,
            blober,
            timestamp,
            data.len(),
        );

        Ok((InsertChunk { data, idx }, blob))
    }
}

#[cfg(test)]
mod tests {
    use data_anchor_blober::instruction::InsertChunk;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        InsertChunk::test_compute_unit_limit();
    }
}
