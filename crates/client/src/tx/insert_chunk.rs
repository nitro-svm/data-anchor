use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    signature::Keypair, signer::Signer,
};

use crate::{fees::FeeStrategy, tx::set_compute_unit_price::set_compute_unit_price, Error};

// TODO: Verify the value
pub const COMPUTE_UNIT_LIMIT: u32 = 7000;

pub const NUM_SIGNATURES: u32 = 1;

pub async fn insert_chunk(
    client: &RpcClient,
    payer: &Keypair,
    blob: Pubkey,
    program_id: Pubkey, // blober
    idx: u16,
    data: Vec<u8>,
    fee_strategy: FeeStrategy,
) -> Result<Message, Error> {
    let accounts = blober::accounts::InsertChunk {
        blob,
        blober: program_id,
        payer: payer.pubkey(),
    };

    let data = blober::instruction::InsertChunk { idx, data };

    let instruction = Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price = set_compute_unit_price(client, &[blob, payer.pubkey()], fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit: Instruction =
        ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT);

    let msg = Message::new(&[set_price, set_limit, instruction], Some(&payer.pubkey()));

    Ok(msg)
}
