use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    signature::Keypair, signer::Signer,
};

use crate::{fees::FeeStrategy, tx::set_compute_unit_price::set_compute_unit_price, Error};

// TODO: Verify the value
const COMPUTE_UNIT_LIMIT: u32 = 40_000;

pub async fn discard_blob(
    client: &RpcClient,
    payer: &Keypair,
    blob: Pubkey,
    program_id: Pubkey, // blober
    fee_strategy: FeeStrategy,
) -> Result<Message, Error> {
    let accounts = blober::accounts::DiscardBlob {
        blob,
        payer: payer.pubkey(),
    };

    let data = blober::instruction::DiscardBlob {};

    let instruction = Instruction {
        program_id,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price = set_compute_unit_price(client, &[blob, payer.pubkey()], fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT);

    let msg = Message::new(&[set_price, set_limit, instruction], Some(&payer.pubkey()));

    Ok(msg)
}
