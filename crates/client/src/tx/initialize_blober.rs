use anchor_lang::{
    Discriminator, InstructionData, Space, ToAccountMetas,
    prelude::Pubkey,
    solana_program::{instruction::Instruction, rent::ACCOUNT_STORAGE_OVERHEAD, system_program},
};
use data_anchor_blober::{instruction::Initialize, state::blober::Blober};

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for Initialize {
    type Input = (String, Pubkey);
    const TX_TYPE: TransactionType = TransactionType::InitializeBlober;
    const COMPUTE_UNIT_LIMIT: u32 = 26_000;
    const LOADED_ACCOUNT_DATA_SIZE: u32 = (Blober::DISCRIMINATOR.len()
        + Blober::INIT_SPACE
        + ACCOUNT_STORAGE_OVERHEAD as usize) as u32;
    #[cfg(test)]
    const INITIALIZE_BLOBER: bool = false;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::Initialize {
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
        let blober =
            data_anchor_blober::find_blober_address(data_anchor_blober::id(), payer, &namespace);

        Ok((namespace, blober))
    }
}

#[cfg(test)]
mod tests {
    use data_anchor_blober::instruction::Initialize;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        Initialize::test_compute_unit_limit();
    }
}
