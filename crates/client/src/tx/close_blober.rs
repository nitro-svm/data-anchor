use anchor_lang::{InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    pubkey::Pubkey,
};

use crate::{
    tx::{
        set_compute_unit_price::set_compute_unit_price, MessageArguments,
        SET_PRICE_AND_CU_LIMIT_COST,
    },
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 2_400;

#[allow(dead_code, reason = "Might be used for fee calculation later")]
pub const NUM_SIGNATURES: u16 = 1;

fn generate_instruction(blober: Pubkey, payer: Pubkey, program_id: Pubkey) -> Instruction {
    let accounts = blober::accounts::Close { blober, payer };

    let data = blober::instruction::Close {};

    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

/// Closes a blober account.
pub async fn close_blober(args: &MessageArguments) -> BloberClientResult<Message> {
    let instruction = generate_instruction(args.blober, args.payer, args.program_id);

    let set_price =
        set_compute_unit_price(&args.client, &[args.blober, args.payer], args.fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(
        COMPUTE_UNIT_LIMIT + SET_PRICE_AND_CU_LIMIT_COST,
    );

    let msg = Message::new(&[set_price, set_limit, instruction], Some(&args.payer));

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use arbtest::arbtest;
    use solana_sdk::{signer::Signer, transaction::Transaction};

    use crate::tx::utils::{close_blober, initialize_blober, new_tokio, setup_environment};

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        let program_id = blober::id();

        let (rpc_client, payer) = new_tokio(async move { setup_environment(program_id).await });

        arbtest(|u| {
            let rpc_client = rpc_client.clone();
            let payer = payer.clone();

            new_tokio(async move {
                let namespace: String = u.arbitrary()?;

                let blober = initialize_blober(rpc_client.clone(), program_id, &payer, &namespace)
                    .await
                    .unwrap();

                let instruction = super::generate_instruction(blober, payer.pubkey(), program_id);

                let recent_blockhash = rpc_client.get_latest_blockhash().await.unwrap();

                let tx = Transaction::new_signed_with_payer(
                    &[instruction],
                    Some(&payer.pubkey()),
                    &[payer.clone()],
                    recent_blockhash,
                );

                let result = rpc_client.simulate_transaction(&tx).await.unwrap();

                let compute_units = result.value.units_consumed.unwrap() as u32;

                assert!(
                    compute_units <= super::COMPUTE_UNIT_LIMIT,
                    "Used {compute_units}, more than {}",
                    super::COMPUTE_UNIT_LIMIT
                );

                close_blober(rpc_client, program_id, &payer, &namespace)
                    .await
                    .unwrap();

                Ok::<(), arbitrary::Error>(())
            })
        });
    }
}
