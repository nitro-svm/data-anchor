use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
};

use crate::{
    tx::{MessageArguments, SET_PRICE_AND_CU_LIMIT_COST},
    BloberClientResult,
};

pub const COMPUTE_UNIT_LIMIT: u32 = 6_500;

pub const NUM_SIGNATURES: u16 = 1;

pub(super) fn generate_instruction(
    blob: Pubkey,
    blober: Pubkey,
    payer: Pubkey,
    program_id: Pubkey,
    idx: u16,
    data: Vec<u8>,
) -> Instruction {
    let accounts = blober::accounts::InsertChunk {
        blob,
        blober,
        payer,
    };

    let data = blober::instruction::InsertChunk { idx, data };

    Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

/// Inserts a chunk into a blob.
pub async fn insert_chunk(
    args: &MessageArguments,
    blob: Pubkey,
    idx: u16,
    data: Vec<u8>,
) -> BloberClientResult<Message> {
    let instruction =
        generate_instruction(blob, args.blober, args.payer, args.program_id, idx, data);

    let set_price = args
        .fee_strategy
        .set_compute_unit_price(&args.client, &[blob, args.payer], args.use_helius)
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
                let timestamp: u64 = u.arbitrary()?;
                let idx: u16 = u.arbitrary()?;
                let data: Vec<u8> = u.arbitrary()?;
                let namespace: String = u.arbitrary()?;

                let blober = initialize_blober(rpc_client.clone(), program_id, &payer, &namespace)
                    .await
                    .unwrap();

                let blob = find_blob_address(payer.pubkey(), blober, timestamp);

                let instruction = super::generate_instruction(
                    blob,
                    blober,
                    payer.pubkey(),
                    program_id,
                    idx,
                    data,
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
