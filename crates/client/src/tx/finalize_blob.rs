use anchor_lang::{InstructionData, ToAccountMetas, prelude::Pubkey};
use blober::instruction::FinalizeBlob;
use solana_sdk::instruction::Instruction;

use crate::tx::{MessageArguments, MessageBuilder};

impl MessageBuilder for FinalizeBlob {
    type Input = Pubkey;
    const COMPUTE_UNIT_LIMIT: u32 = 25_000;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input, args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = blober::accounts::FinalizeBlob {
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
        let blob = blober::find_blob_address(blober::id(), payer, blober, timestamp, blob_size);

        Ok(blob)
    }
}

#[cfg(test)]
mod tests {
    use blober::instruction::FinalizeBlob;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        FinalizeBlob::test_compute_unit_limit();
    }
}
