use std::sync::Arc;

use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, message::Message,
    signature::Keypair, signer::Signer,
};

use crate::{DataAnchorClientResult, Fee, TransactionType};

pub mod close_blober;
pub mod compound;
pub mod compound_declare;
pub mod compound_finalize;
pub mod create_checkpoint;
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

/// The constant price of the [`solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price`]
/// and [`solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit`] instructions.
pub const SET_PRICE_AND_CU_LIMIT_COST: u32 = 300;

#[async_trait]
pub trait MessageBuilder {
    type Input: Send;
    const TX_TYPE: TransactionType;
    const COMPUTE_UNIT_LIMIT: u32;
    const NUM_SIGNATURES: u16 = 1;
    #[cfg(test)]
    const INITIALIZE_BLOBER: bool = true;

    fn mutable_accounts(args: &MessageArguments<Self::Input>) -> Vec<Pubkey>;

    fn generate_instructions(args: &MessageArguments<Self::Input>) -> Vec<Instruction>;

    async fn build_message(args: MessageArguments<Self::Input>) -> DataAnchorClientResult<Message> {
        let set_price = args.fee.set_compute_unit_price();

        // This limit is chosen empirically
        let set_limit = ComputeBudgetInstruction::set_compute_unit_limit(
            Self::COMPUTE_UNIT_LIMIT + SET_PRICE_AND_CU_LIMIT_COST,
        );

        let payer = Some(args.payer);

        let mut all_instructions = vec![set_price, set_limit];
        all_instructions.extend(Self::generate_instructions(&args));

        Ok(Message::new(&all_instructions, payer.as_ref()))
    }

    #[cfg(test)]
    fn generate_arbitrary_input(
        u: &mut arbitrary::Unstructured,
        payer: Pubkey,
        blober: Pubkey,
    ) -> arbitrary::Result<Self::Input>;

    #[cfg(test)]
    fn test_compute_unit_limit() {
        use solana_sdk::transaction::Transaction;
        use utils::{close_blober, initialize_blober, new_tokio, setup_environment};

        use crate::FeeStrategy;

        let program_id = data_anchor_blober::id();

        let (rpc_client, payer) = new_tokio(async move { setup_environment(program_id).await });

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
                    &[payer.clone()],
                    recent_blockhash,
                );

                let result = args.client.simulate_transaction(&tx).await.unwrap();

                let compute_units = result.value.units_consumed.unwrap() as u32;

                assert!(
                    compute_units <= Self::COMPUTE_UNIT_LIMIT,
                    "Used {compute_units}, more than {}",
                    Self::COMPUTE_UNIT_LIMIT
                );

                if Self::INITIALIZE_BLOBER {
                    close_blober(args.client, args.program_id, &payer, &namespace)
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

    use anchor_lang::{InstructionData, ToAccountMetas};
    use data_anchor_blober::find_blober_address;
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_pubkey::Pubkey;
    use solana_sdk::{
        commitment_config::CommitmentConfig, instruction::Instruction, signature::Keypair,
        signer::Signer, system_program, transaction::Transaction,
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
    pub async fn setup_environment(program_id: Pubkey) -> (Arc<RpcClient>, Arc<Keypair>) {
        let (test_validator, payer) = TestValidatorGenesis::default()
            .add_program(
                "../../programs/target/deploy/data_anchor_blober",
                program_id,
            )
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
