use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    signature::Keypair, signer::Signer,
};

use super::set_compute_unit_price;
use crate::{fees::FeeStrategy, Error};

pub const COMPUTE_UNIT_LIMIT: u32 = 7000;
pub const NUM_SIGNATURES: u32 = 1;

/// Creates a transaction for uploading a single chunk to a chunker account.
///
/// # Arguments
/// - `client`: The RPC client to use for sending the transaction.
/// - `payer`: The payer of the transaction. Will be used to pay the rent held by the chunker account, as well as transaction fees.
/// - `chunker`: The address of the chunker account to store the chunk in. Must have already been initialized.
/// - `chunk_index`: The index of the chunk to store. Must be less than the number of chunks in the chunker.
/// - `chunk_data`: The binary data to store in the chunk. The data must fit in a single Solana transaction.
/// - `fee_strategy`: The strategy to use for calculating the fees for transactions.
pub async fn insert_chunk(
    client: &RpcClient,
    payer: &Keypair,
    chunker: Pubkey,
    chunk_index: u16,
    chunk_data: Vec<u8>,
    fee_strategy: FeeStrategy,
) -> Result<Message, Error> {
    let accounts = chunker::accounts::InsertChunk {
        chunker,
        signer: payer.pubkey(),
    };

    let data = chunker::instruction::InsertChunk {
        chunk_index,
        chunk_data,
    };

    let instruction = Instruction {
        program_id: chunker::id(),
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price =
        set_compute_unit_price(client, &[chunker, payer.pubkey()], fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit: Instruction =
        ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT);

    let msg = Message::new(&[set_price, set_limit, instruction], Some(&payer.pubkey()));

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use solana_sdk::transaction::Transaction;

    use super::*;

    #[tokio::test]
    async fn verify() {
        let payer = Keypair::new();
        let chunker = Keypair::new();
        let client = RpcClient::new_mock("succeeds".to_string());

        let msg = insert_chunk(
            &client,
            &payer,
            chunker.pubkey(),
            0,
            vec![0; 768],
            FeeStrategy::default(),
        )
        .await
        .unwrap();
        let recent_blockhash = client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new(&[payer], msg, recent_blockhash);

        assert_eq!(tx.signatures.len() as u32, NUM_SIGNATURES);
    }
}
