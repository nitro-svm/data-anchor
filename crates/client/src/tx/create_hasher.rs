use anchor_lang::{InstructionData, ToAccountMetas};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    signature::Keypair, signer::Signer, system_program,
};

use super::set_compute_unit_price;
use crate::{Error, FeeStrategy};

pub const COMPUTE_UNIT_LIMIT: u32 = 8_000;
#[allow(dead_code)]
pub const NUM_SIGNATURES: u32 = 2;

/// Creates a transaction for creating a hasher account.
///
/// # Arguments
/// - `client`: The RPC client to use for sending the transaction.
/// - `payer`: The payer of the transaction. Will be used to pay the rent held by the copy account, as well as transaction fees.
/// - `hasher`: The address of the hasher to create. Must not already exist.
/// - `fee_strategy`: The strategy to use for calculating the fees for transactions.
pub async fn create_hasher(
    client: &RpcClient,
    payer: &Keypair,
    hasher: &Keypair,
    fee_strategy: FeeStrategy,
) -> Result<Message, Error> {
    let accounts = hasher::accounts::CreateHasher {
        hasher: hasher.pubkey(),
        signer: payer.pubkey(),
        system_program: system_program::ID,
    };

    let data = hasher::instruction::CreateHasher {
        trusted_caller: chunker::id(),
    };

    let instruction = Instruction {
        program_id: hasher::id(),
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    };

    let set_price =
        set_compute_unit_price(client, &[hasher.pubkey(), payer.pubkey()], fee_strategy).await?;
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
        let hasher = Keypair::new();
        let client = RpcClient::new_mock("succeeds".to_string());

        let msg = create_hasher(&client, &payer, &hasher, FeeStrategy::default())
            .await
            .unwrap();
        let recent_blockhash = client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new(&[payer, hasher], msg, recent_blockhash);

        assert_eq!(tx.signatures.len() as u32, NUM_SIGNATURES);
    }
}
