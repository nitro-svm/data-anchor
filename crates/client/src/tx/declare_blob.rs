use anchor_lang::{InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    pubkey::Pubkey, system_program,
};

use crate::{
    tx::{MessageArguments, SET_PRICE_AND_CU_LIMIT_COST},
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 44_000;

pub const NUM_SIGNATURES: u16 = 1;

#[allow(clippy::too_many_arguments, reason = "Only used internally")]
pub(super) fn generate_instruction(
    blob: Pubkey,
    blober: Pubkey,
    payer: Pubkey,
    system_program: Pubkey,
    program_id: Pubkey,
    timestamp: u64,
    blob_size: usize,
) -> Instruction {
    let accounts = blober::accounts::DeclareBlob {
        blob,
        blober,
        payer,
        system_program,
    };

    let data = blober::instruction::DeclareBlob {
        timestamp,
        blob_size: blob_size as u32,
    };

    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

/// Declares a blob with the given blober.
pub async fn declare_blob(
    args: &MessageArguments,
    blob: Pubkey,
    timestamp: u64,
    blob_size: usize,
) -> BloberClientResult<Message> {
    let instruction = generate_instruction(
        blob,
        args.blober,
        args.payer,
        system_program::id(),
        args.program_id,
        timestamp,
        blob_size,
    );

    let set_price = args
        .fee_strategy
        .set_compute_unit_price(
            &args.client,
            &[blob, args.blober, args.payer],
            args.use_helius,
        )
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
    use blober::find_blob_address;
    use solana_sdk::{signer::Signer, system_program, transaction::Transaction};

    use crate::tx::utils::{close_blober, initialize_blober, new_tokio, setup_environment};

    #[test]
    #[ignore]
    fn test_compute_unit_limit() {
        let program_id = blober::id();
        let system_program = system_program::id();

        let (rpc_client, payer) = new_tokio(async move { setup_environment(program_id).await });

        arbtest(|u| {
            let rpc_client = rpc_client.clone();
            let payer = payer.clone();

            new_tokio(async move {
                let timestamp: u64 = u.arbitrary()?;
                let blob_size: usize = u.arbitrary()?;
                let namespace: String = u.arbitrary()?;

                let blober = initialize_blober(rpc_client.clone(), program_id, &payer, &namespace)
                    .await
                    .unwrap();

                let blob = find_blob_address(payer.pubkey(), blober, timestamp, blob_size);

                let instruction = super::generate_instruction(
                    blob,
                    blober,
                    payer.pubkey(),
                    system_program,
                    program_id,
                    timestamp,
                    blob_size,
                );

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
