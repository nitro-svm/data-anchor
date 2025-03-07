use anchor_lang::{InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    pubkey::Pubkey, system_program,
};

use crate::{
    tx::{MessageArguments, SET_PRICE_AND_CU_LIMIT_COST},
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 28_000;

pub const NUM_SIGNATURES: u16 = 1;

fn generate_instruction(
    blober: Pubkey,
    payer: Pubkey,
    program_id: Pubkey,
    namespace: String,
) -> Instruction {
    let accounts = blober::accounts::Initialize {
        blober,
        payer,
        system_program: system_program::id(),
    };

    let data = blober::instruction::Initialize {
        namespace,
        trusted: payer,
    };

    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

/// Initializes the blober account with a given trusted caller set as the payer of this transaction.
pub async fn initialize_blober(
    args: &MessageArguments,
    namespace: String,
) -> BloberClientResult<Message> {
    let instruction = generate_instruction(args.blober, args.payer, args.program_id, namespace);

    let set_price = args
        .fee_strategy
        .set_compute_unit_price(&args.client, &[args.blober, args.payer], args.use_helius)
        .await?;
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
    use blober::find_blober_address;
    use solana_sdk::{signer::Signer, transaction::Transaction};

    use crate::tx::utils::{new_tokio, setup_environment};

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
                let blober = find_blober_address(payer.pubkey(), &namespace);

                let instruction =
                    super::generate_instruction(blober, payer.pubkey(), program_id, namespace);

                let recent_blockhash = rpc_client.get_latest_blockhash().await.unwrap();

                let tx = Transaction::new_signed_with_payer(
                    &[instruction],
                    Some(&payer.pubkey()),
                    &[payer],
                    recent_blockhash,
                );

                let result = rpc_client.simulate_transaction(&tx).await.unwrap();

                let compute_units = result.value.units_consumed.unwrap() as u32;

                assert!(
                    compute_units <= super::COMPUTE_UNIT_LIMIT,
                    "Used {compute_units}, more than {}",
                    super::COMPUTE_UNIT_LIMIT
                );

                Ok::<(), arbitrary::Error>(())
            })
        });
    }
}
