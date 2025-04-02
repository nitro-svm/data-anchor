use anchor_lang::{InstructionData, ToAccountMetas};
use blober::instruction::DeclareBlob;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, system_program};

use crate::tx::{MessageArguments, MessageBuilder};

impl MessageBuilder for DeclareBlob {
    type Input = (Self, Pubkey);
    const COMPUTE_UNIT_LIMIT: u32 = 44_000;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.input.1, args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = blober::accounts::DeclareBlob {
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
        let blob = blober::find_blob_address(blober::id(), payer, blober, timestamp, blob_size);

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
    use blober::instruction::DeclareBlob;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        DeclareBlob::test_compute_unit_limit();
    }
}
