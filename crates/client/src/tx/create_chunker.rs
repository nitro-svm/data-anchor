use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    signature::Keypair, signer::Signer, system_program,
};

use super::set_compute_unit_price;
use crate::{fees::FeeStrategy, Error};

pub const COMPUTE_UNIT_LIMIT: u32 = 30_000;
pub const NUM_SIGNATURES: u32 = 1;

/// Creates a transaction for creating a chunker account.
///
/// # Arguments
/// - `client`: The RPC client to use for sending the transaction.
/// - `payer`: The payer of the transaction. Will be used to pay the rent held by the chunker account, as well as transaction fees.
/// - `chunker`: The address of the chunker account to create. Must not already exist.
/// - `blob_digest`: The expected digest of the blob to be stored in the chunker.
/// - `num_chunks`: The number of chunks to store in the chunker.
/// - `fee_strategy`: The strategy to use for calculating the fees for transactions.
pub async fn create_chunker(
    client: &RpcClient,
    payer: &Keypair,
    chunker: Pubkey,
    timestamp: u64,
    blob_size: u32,
    num_chunks: u16,
    fee_strategy: FeeStrategy,
) -> Result<Message, Error> {
    let accounts = chunker::accounts::CreateChunker {
        chunker,
        signer: payer.pubkey(),
        system_program: system_program::ID,
    };

    let data = chunker::instruction::CreateChunker {
        timestamp,
        blob_size,
        num_chunks,
    };

    let instruction = Instruction {
        program_id: chunker::id(),
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price =
        set_compute_unit_price(client, &[chunker, payer.pubkey()], fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT);

    let msg = Message::new(&[set_price, set_limit, instruction], Some(&payer.pubkey()));

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use chunker::{find_chunker_address, state::chunker::CHUNK_SIZE};
    use solana_sdk::transaction::Transaction;

    use super::*;

    #[tokio::test]
    async fn verify() {
        let payer = Keypair::new();
        let chunker = find_chunker_address(payer.pubkey(), 0);
        let client = RpcClient::new_mock("succeeds".to_string());

        let msg = create_chunker(
            &client,
            &payer,
            chunker,
            0,
            100 * CHUNK_SIZE as u32,
            100,
            FeeStrategy::default(),
        )
        .await
        .unwrap();
        let recent_blockhash = client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new(&[payer], msg, recent_blockhash);

        assert_eq!(tx.signatures.len() as u32, NUM_SIGNATURES);
    }
}
