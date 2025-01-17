use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    signature::Keypair, signer::Signer,
};

use super::set_compute_unit_price;
use crate::{Error, FeeStrategy};

pub const COMPUTE_UNIT_LIMIT: u32 = 10_000;
#[allow(dead_code)]
pub const NUM_SIGNATURES: u32 = 1;

/// Creates a transaction for closing a hasher account.
///
/// # Arguments
/// - `client`: The RPC client to use for sending the transaction.
/// - `payer`: The payer of the transaction. Will receive the rent held by the hasher account.
/// - `hasher`: The address of the hasher account to close. Must have already been initialized.
/// - `fee_strategy`: The strategy to use for calculating the fees for transactions.
pub async fn close_hasher(
    client: &RpcClient,
    payer: &Keypair,
    hasher: Pubkey,
    fee_strategy: FeeStrategy,
) -> Result<Message, Error> {
    let accounts = hasher::accounts::CloseHasher {
        hasher,
        signer: payer.pubkey(),
    };

    let data = hasher::instruction::CloseHasher {};

    let instruction = Instruction {
        program_id: hasher::id(),
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price = set_compute_unit_price(client, &[hasher, payer.pubkey()], fee_strategy).await?;
    // This limit is chosen empirically, should blow up in integration tests if it's set too low.
    let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT);

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

        let msg = close_hasher(&client, &payer, chunker.pubkey(), FeeStrategy::default())
            .await
            .unwrap();
        let recent_blockhash = client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new(&[payer], msg, recent_blockhash);

        assert_eq!(tx.signatures.len() as u32, NUM_SIGNATURES);
    }
}
