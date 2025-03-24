use anchor_lang::{InstructionData, ToAccountMetas};
use blober::instruction::Initialize;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, system_program};

use crate::tx::{MessageArguments, MessageBuilder};

impl MessageBuilder for Initialize {
    type Input = (String, Pubkey);
    const COMPUTE_UNIT_LIMIT: u32 = 28_000;
    #[cfg(test)]
    const INITIALIZE_BLOBER: bool = false;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = blober::accounts::Initialize {
            blober: args.input.1,
            payer: args.payer,
            system_program: system_program::id(),
        };

        let data = Self {
            namespace: args.input.0.clone(),
            trusted: args.payer,
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
        _blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        let namespace: String = u.arbitrary()?;
        let blober = blober::find_blober_address(payer, &namespace);

        Ok((namespace, blober))
    }
}

#[cfg(test)]
mod tests {
    use blober::instruction::Initialize;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        Initialize::test_compute_unit_limit();
    }
}
