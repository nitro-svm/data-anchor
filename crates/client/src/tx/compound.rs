use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    pubkey::Pubkey, system_program,
};

use super::{
    declare_blob, finalize_blob, insert_chunk, MessageArguments, SET_PRICE_AND_CU_LIMIT_COST,
};
use crate::BloberClientResult;

pub const COMPUTE_UNIT_LIMIT: u32 = declare_blob::COMPUTE_UNIT_LIMIT
    + insert_chunk::COMPUTE_UNIT_LIMIT
    + finalize_blob::COMPUTE_UNIT_LIMIT;

pub const NUM_SIGNATURES: u16 = 1;

#[allow(clippy::too_many_arguments, reason = "Only used internally")]
pub(super) fn generate_instruction(
    blob: Pubkey,
    blober: Pubkey,
    payer: Pubkey,
    system_program: Pubkey,
    program_id: Pubkey,
    timestamp: u64,
    data: Vec<u8>,
) -> [Instruction; 3] {
    [
        declare_blob::generate_instruction(
            blob,
            blober,
            payer,
            system_program,
            program_id,
            timestamp,
            data.len() as u32,
            1,
        ),
        insert_chunk::generate_instruction(blob, blober, payer, program_id, 0, data),
        finalize_blob::generate_instruction(blob, blober, payer, program_id),
    ]
}

pub async fn compound_upload(
    args: &MessageArguments,
    blob: Pubkey,
    timestamp: u64,
    data: Vec<u8>,
) -> BloberClientResult<Message> {
    let instructions = generate_instruction(
        blob,
        args.blober,
        args.payer,
        system_program::id(),
        args.program_id,
        timestamp,
        data,
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

    let msg = Message::new(
        &[&[set_price, set_limit], instructions.as_ref()].concat(),
        Some(&args.payer),
    );

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use arbtest::arbtest;
    use blober::{find_blob_address, COMPOUND_TX_SIZE};
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
                let data: [u8; COMPOUND_TX_SIZE as usize] = u.arbitrary()?;
                let namespace: String = u.arbitrary()?;

                let blober = initialize_blober(rpc_client.clone(), program_id, &payer, &namespace)
                    .await
                    .unwrap();

                let blob = find_blob_address(payer.pubkey(), blober, timestamp);

                let instructions = super::generate_instruction(
                    blob,
                    blober,
                    payer.pubkey(),
                    system_program,
                    program_id,
                    timestamp,
                    data.to_vec(),
                );

                let recent_blockhash = rpc_client.get_latest_blockhash().await.unwrap();

                let tx = Transaction::new_signed_with_payer(
                    &instructions,
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
