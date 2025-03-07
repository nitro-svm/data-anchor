use std::sync::Arc;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

use crate::FeeStrategy;

pub mod close_blober;
pub mod compound;
pub mod declare_blob;
pub mod discard_blob;
pub mod finalize_blob;
pub mod initialize_blober;
pub mod insert_chunk;

pub use close_blober::close_blober;
pub use compound::compound_upload;
pub use declare_blob::declare_blob;
pub use discard_blob::discard_blob;
pub use finalize_blob::finalize_blob;
pub use initialize_blober::initialize_blober;
pub use insert_chunk::insert_chunk;

pub struct MessageArguments {
    // The program ID of the blober program.
    pub program_id: Pubkey,
    // The address of the blober account to insert the chunk into.
    pub blober: Pubkey,
    pub payer: Pubkey,
    pub client: Arc<RpcClient>,
    pub fee_strategy: FeeStrategy,
    pub use_helius: bool,
}

impl MessageArguments {
    pub fn new(
        program_id: Pubkey,
        blober: Pubkey,
        payer: &Keypair,
        client: Arc<RpcClient>,
        fee_strategy: FeeStrategy,
        use_helius: bool,
    ) -> Self {
        Self {
            client,
            blober,
            program_id,
            fee_strategy,
            use_helius,
            payer: payer.pubkey(),
        }
    }
}

/// The constant price of the [`solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price`]
/// and [`solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit`] instructions.
pub const SET_PRICE_AND_CU_LIMIT_COST: u32 = 300;

#[cfg(test)]
mod utils {
    use std::{future::Future, sync::Arc};

    use anchor_lang::{InstructionData, ToAccountMetas};
    use blober::find_blober_address;
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_sdk::{
        commitment_config::CommitmentConfig, instruction::Instruction, pubkey::Pubkey,
        signature::Keypair, signer::Signer, system_program, transaction::Transaction,
    };
    use solana_test_validator::TestValidatorGenesis;

    /// For [`arbtest`] we need to have synchronous code inside the test, so we need to block on the futures.
    pub fn new_tokio<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    /// Initialize a blober PDA account for tests.
    pub async fn initialize_blober(
        rpc_client: Arc<RpcClient>,
        program_id: Pubkey,
        payer: &Keypair,
        namespace: &str,
    ) -> Result<Pubkey, Box<dyn std::error::Error>> {
        let blober = find_blober_address(payer.pubkey(), namespace);

        let accounts = blober::accounts::Initialize {
            blober,
            payer: payer.pubkey(),
            system_program: system_program::id(),
        };

        let data = blober::instruction::Initialize {
            namespace: namespace.to_string(),
            trusted: payer.pubkey(),
        };

        let instruction = Instruction {
            program_id,
            accounts: accounts.to_account_metas(None),
            data: data.data(),
        };

        let recent_blockhash = rpc_client.get_latest_blockhash().await?;

        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        rpc_client.send_and_confirm_transaction(&tx).await?;

        Ok(blober)
    }

    /// Close a blober account for tests.
    pub async fn close_blober(
        rpc_client: Arc<RpcClient>,
        program_id: Pubkey,
        payer: &Keypair,
        namespace: &str,
    ) -> Result<Pubkey, Box<dyn std::error::Error>> {
        let blober = find_blober_address(payer.pubkey(), namespace);

        let accounts = blober::accounts::Close {
            blober,
            payer: payer.pubkey(),
        };

        let data = blober::instruction::Close {};

        let instruction = Instruction {
            program_id,
            accounts: accounts.to_account_metas(None),
            data: data.data(),
        };

        let recent_blockhash = rpc_client.get_latest_blockhash().await?;

        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        rpc_client.send_and_confirm_transaction(&tx).await?;

        Ok(blober)
    }

    /// Setup the environment for integration tests.
    pub async fn setup_environment(program_id: Pubkey) -> (Arc<RpcClient>, Arc<Keypair>) {
        let (test_validator, payer) = TestValidatorGenesis::default()
            .add_program("../../programs/target/deploy/blober", program_id)
            .start_async()
            .await;

        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            test_validator.rpc_url(),
            CommitmentConfig::processed(),
        ));
        let payer = Arc::new(payer);
        // Sending too many transactions at once can cause the test validator to hang. It seems to hit
        // some deadlock with the JsonRPC server shutdown. This is a test, so leak it to keep tests moving.
        std::mem::forget(test_validator);

        (rpc_client, payer)
    }
}
