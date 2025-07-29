use anchor_lang::{InstructionData, ToAccountMetas};
use data_anchor_blober::{
    GROTH16_PROOF_SIZE, PROOF_PUBLIC_VALUES_SIZE, instruction::CreateCheckpoint,
};
use solana_sdk::{clock::Slot, instruction::Instruction, pubkey::Pubkey, system_program};

use crate::{
    TransactionType,
    tx::{MessageArguments, MessageBuilder},
};

impl MessageBuilder for CreateCheckpoint {
    type Input = (
        Pubkey,
        [u8; GROTH16_PROOF_SIZE],
        [u8; PROOF_PUBLIC_VALUES_SIZE],
        String,
        Slot,
    );
    const TX_TYPE: TransactionType = TransactionType::CreateCheckpoint;
    const COMPUTE_UNIT_LIMIT: u32 = 15_500;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey> {
        vec![args.blober, args.payer]
    }

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction> {
        let accounts = data_anchor_blober::accounts::CreateCheckpoint {
            checkpoint: args.input.0,
            payer: args.payer,
            system_program: system_program::id(),
        };

        let data = Self {
            blober: args.input.0,
            proof: args.input.1,
            public_values: args.input.2,
            verification_key: args.input.3.clone(),
            slot: args.input.4,
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
        _payer: Pubkey,
        blober: Pubkey,
    ) -> arbitrary::Result<Self::Input> {
        use std::fmt::Write;

        use data_anchor_blober::PROOF_VERIFICATION_KEY_SIZE;
        use solana_sdk::pubkey::PUBKEY_BYTES;

        let proof: [u8; GROTH16_PROOF_SIZE] = u.arbitrary()?;
        let public_values = [
            blober.as_ref(),
            u.arbitrary::<[u8; PROOF_PUBLIC_VALUES_SIZE - PUBKEY_BYTES]>()?
                .as_ref(),
        ]
        .concat()
        .as_slice()
        .try_into()
        .expect("Failed to create public values array");
        let verification_key: [u8; PROOF_VERIFICATION_KEY_SIZE] = u.arbitrary()?;
        let verification_key = verification_key
            .iter()
            .fold(String::from("0x"), |mut acc, b| {
                write!(acc, "{:02x}", b).expect("Failed to write hex string");
                acc
            });
        let slot: Slot = u.arbitrary()?;

        Ok((blober, proof, public_values, verification_key, slot))
    }
}

#[cfg(test)]
mod tests {
    use data_anchor_blober::instruction::CreateCheckpoint;

    use crate::tx::MessageBuilder;

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        CreateCheckpoint::test_compute_unit_limit();
    }
}
