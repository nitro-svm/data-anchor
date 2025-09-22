use std::sync::Arc;

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, message::Message},
};
use async_trait::async_trait;
use itertools::Itertools;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use tracing::debug;

use crate::{Fee, TransactionType};

pub mod close_blober;
pub mod compound;
pub mod compound_declare;
pub mod compound_finalize;
pub mod configure_checkpoint;
pub mod declare_blob;
pub mod discard_blob;
pub mod finalize_blob;
pub mod initialize_blober;
pub mod insert_chunk;

pub use compound::Compound;
pub use compound_declare::CompoundDeclare;
pub use compound_finalize::CompoundFinalize;

pub struct MessageArguments<Input>
where
    Input: Send,
{
    /// The program ID of the blober program.
    pub program_id: Pubkey,
    /// The address of the blober account to insert the chunk into.
    pub blober: Pubkey,
    pub payer: Pubkey,
    pub client: Arc<RpcClient>,
    pub fee: Fee,
    pub input: Input,
}

impl<Input> MessageArguments<Input>
where
    Input: Send,
{
    pub fn new(
        program_id: Pubkey,
        blober: Pubkey,
        payer: &Keypair,
        client: Arc<RpcClient>,
        fee: Fee,
        input: Input,
    ) -> Self {
        Self {
            client,
            blober,
            program_id,
            fee,
            input,
            payer: payer.pubkey(),
        }
    }

    pub fn to_other<'a, T>(&'a self) -> MessageArguments<T>
    where
        T: From<&'a Input> + Send,
    {
        MessageArguments::<T> {
            program_id: self.program_id,
            blober: self.blober,
            payer: self.payer,
            client: self.client.clone(),
            fee: self.fee,
            input: T::from(&self.input),
        }
    }
}

/// The constant price of the [`solana_compute_budget_interface::ComputeBudgetInstruction::set_compute_unit_price`],
/// [`solana_compute_budget_interface::ComputeBudgetInstruction::set_compute_unit_limit`] and
/// [`solana_compute_budget_interface::ComputeBudgetInstruction::set_loaded_accounts_data_size_limit`] instructions.
pub const SET_PRICE_AND_CU_LIMIT_COST: u32 = 450;

pub const SYSTEM_PROGRAM_DATA_SIZE: u32 = 14;
pub const COMPUTE_BUDGET_PROGRAM_DATA_SIZE: u32 = 22;
pub const ANCHOR_PROGRAM_DATA_SIZE: u32 = 36;

pub const BASE_LOADED_ACCOUNT_DATA_SIZE: u32 =
    SYSTEM_PROGRAM_DATA_SIZE + COMPUTE_BUDGET_PROGRAM_DATA_SIZE + ANCHOR_PROGRAM_DATA_SIZE;

// Per SIMD-0186, all accounts are assigned a base size of 64 bytes to cover
// the storage cost of metadata.
pub const TRANSACTION_ACCOUNT_BASE_SIZE: u32 = 64;

// Per SIMD-0186, resolved address lookup tables are assigned a base size of 8248
// bytes: 8192 bytes for the maximum table size plus 56 bytes for metadata.
pub const ADDRESS_LOOKUP_TABLE_BASE_SIZE: u32 = 8248;

#[async_trait]
pub trait MessageBuilder {
    type Input: Send;
    const TX_TYPE: TransactionType;
    const COMPUTE_UNIT_LIMIT: u32;
    const LOADED_ACCOUNT_DATA_SIZE: u32;
    const NUM_SIGNATURES: u16 = 1;
    #[cfg(test)]
    const INITIALIZE_BLOBER: bool = true;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey>;

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction>;

    async fn build_message(args: MessageArguments<Self::Input>) -> Message {
        let set_price = args.fee.set_compute_unit_price();
        let instructions = Self::generate_instructions(&args);
        let accounts_count = instructions
            .iter()
            .flat_map(|ix| ix.accounts.iter().map(|meta| meta.pubkey))
            .unique()
            .count()
            + 1; // +1 for the `ComputeBudget` program account

        let address_lookup_tables_count = instructions.len().saturating_add(3);

        // This limit is chosen empirically
        let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(
            Self::COMPUTE_UNIT_LIMIT + SET_PRICE_AND_CU_LIMIT_COST,
        );

        debug!(
            "Building message with limits: CU limit {}, loaded account data size limit {}, number of accounts {}, number of address lookup tables {}",
            Self::COMPUTE_UNIT_LIMIT + SET_PRICE_AND_CU_LIMIT_COST,
            Self::LOADED_ACCOUNT_DATA_SIZE
                + BASE_LOADED_ACCOUNT_DATA_SIZE
                + (accounts_count as u32 * TRANSACTION_ACCOUNT_BASE_SIZE)
                + (address_lookup_tables_count as u32 * ADDRESS_LOOKUP_TABLE_BASE_SIZE),
            accounts_count,
            address_lookup_tables_count,
        );
        // This limit can be known based on the instruction
        let set_account_data_size = ComputeBudgetInstruction::set_loaded_accounts_data_size_limit(
            Self::LOADED_ACCOUNT_DATA_SIZE
                + BASE_LOADED_ACCOUNT_DATA_SIZE
                + (accounts_count as u32 * TRANSACTION_ACCOUNT_BASE_SIZE)
                + (address_lookup_tables_count as u32 * ADDRESS_LOOKUP_TABLE_BASE_SIZE),
        );

        let payer = Some(args.payer);

        let mut all_instructions = vec![set_price, set_limit, set_account_data_size];
        all_instructions.extend(Self::generate_instructions(&args));

        Message::new(&all_instructions, payer.as_ref())
    }

    #[cfg(test)]
    fn generate_arbitrary_input(
        u: &mut arbitrary::Unstructured,
        payer: Pubkey,
        blober: Pubkey,
    ) -> arbitrary::Result<Self::Input>;

    #[allow(unused_variables, reason = "`updated` is used in asserts")]
    #[cfg(test)]
    fn test_compute_unit_limit()
    where
        Self: std::marker::Send,
    {
        use solana_transaction::Transaction;
        use utils::{close_blober, initialize_blober, new_tokio, setup_environment};
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();

        use crate::FeeStrategy;

        let program_id = data_anchor_blober::id();

        let (rpc_client, payer) = new_tokio(async move { setup_environment().await });

        arbtest::arbtest(|u| {
            let rpc_client = rpc_client.clone();
            let payer = payer.clone();

            new_tokio(async move {
                let namespace: String = u.arbitrary()?;

                let blober = if Self::INITIALIZE_BLOBER {
                    initialize_blober(rpc_client.clone(), program_id, &payer, &namespace)
                        .await
                        .unwrap()
                } else {
                    data_anchor_blober::find_blober_address(
                        data_anchor_blober::id(),
                        payer.pubkey(),
                        &namespace,
                    )
                };

                let input = Self::generate_arbitrary_input(u, payer.pubkey(), blober).unwrap();

                let fee = FeeStrategy::default()
                    .convert_fee_strategy_to_fixed(
                        &rpc_client,
                        &[blober, payer.pubkey()],
                        Self::TX_TYPE,
                    )
                    .await
                    .unwrap();

                let args = MessageArguments::new(
                    program_id,
                    blober,
                    &payer,
                    rpc_client.clone(),
                    fee,
                    input,
                );

                let recent_blockhash = args.client.get_latest_blockhash().await.unwrap();

                let tx = Transaction::new_signed_with_payer(
                    &Self::generate_instructions(&args),
                    Some(&args.payer),
                    std::slice::from_ref(&payer),
                    recent_blockhash,
                );

                let accounts_count = tx.message.account_keys.len();
                let address_lookup_tables_count = tx.message.instructions.len();
                let accounts = tx.message.account_keys.clone();

                let loaded_account_data_size_limit =
                    Self::LOADED_ACCOUNT_DATA_SIZE + BASE_LOADED_ACCOUNT_DATA_SIZE;

                let result = rpc_client.simulate_transaction(&tx).await.unwrap();

                let compute_units = result.value.units_consumed.unwrap() as u32;
                let loaded_account_data_size = result.value.loaded_accounts_data_size.unwrap_or(0);

                assert!(
                    compute_units * 11 / 10 <= Self::COMPUTE_UNIT_LIMIT,
                    "Used {compute_units} compute units, limit is only {}",
                    Self::COMPUTE_UNIT_LIMIT
                );
                assert!(
                    loaded_account_data_size <= loaded_account_data_size_limit,
                    "Used {loaded_account_data_size} bytes of loaded account data, limit is {loaded_account_data_size_limit}"
                );

                if Self::INITIALIZE_BLOBER {
                    close_blober(rpc_client, program_id, &payer, &namespace)
                        .await
                        .unwrap();
                }

                Ok::<(), arbitrary::Error>(())
            })
        });
    }
}

#[cfg(test)]
mod utils {
    use std::{future::Future, sync::Arc};

    use anchor_lang::{
        InstructionData, ToAccountMetas,
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
    };
    use data_anchor_blober::find_blober_address;
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_commitment_config::CommitmentConfig;
    use solana_keypair::Keypair;
    use solana_native_token::LAMPORTS_PER_SOL;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

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
        let blober = find_blober_address(data_anchor_blober::id(), payer.pubkey(), namespace);

        let accounts = data_anchor_blober::accounts::Initialize {
            blober,
            payer: payer.pubkey(),
            system_program: system_program::id(),
        };

        let data = data_anchor_blober::instruction::Initialize {
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
        let blober = find_blober_address(data_anchor_blober::id(), payer.pubkey(), namespace);

        let accounts = data_anchor_blober::accounts::Close {
            blober,
            payer: payer.pubkey(),
            checkpoint: None,
            checkpoint_config: None,
        };

        let data = data_anchor_blober::instruction::Close {};

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
    pub async fn setup_environment() -> (Arc<RpcClient>, Arc<Keypair>) {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            "http://localhost:8899".to_string(),
            CommitmentConfig::processed(),
        ));
        let payer = Arc::new(Keypair::new());

        let lamports = 100 * LAMPORTS_PER_SOL;
        let target_balance = rpc_client
            .get_minimum_balance_for_rent_exemption(0)
            .await
            .unwrap()
            + lamports;

        rpc_client
            .request_airdrop(&payer.pubkey(), target_balance)
            .await
            .unwrap();

        // Wait for airdrop confirmation
        loop {
            let balance = rpc_client
                .get_balance_with_commitment(&payer.pubkey(), CommitmentConfig::processed())
                .await
                .unwrap()
                .value;
            if balance >= lamports {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        (rpc_client, payer)
    }
}
