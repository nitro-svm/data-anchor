use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
};

use super::{Fee, Priority};
use crate::BloberClientResult;

/// The strategy to use for calculating the fees for transactions.
#[derive(Debug, Clone, Copy)]
pub enum FeeStrategy {
    /// Use a fixed fee for all transactions.
    Fixed(Fee),
    /// Calculate a reasonable fee based on the recent fees in the network and a given priority.
    BasedOnRecentFees(Priority),
}

impl Default for FeeStrategy {
    fn default() -> Self {
        Self::BasedOnRecentFees(Priority::default())
    }
}

impl FeeStrategy {
    /// Creates a transaction for setting the compute unit price for a transaction based on recent prioritization fees.
    ///
    /// # Arguments
    /// - `client`: The RPC client to use for looking up recent prioritization fees.
    /// - `mutable_accounts`: The addresses of the accounts that are mutable in the transaction (and thus need exclusive locks).
    pub async fn set_compute_unit_price(
        &self,
        client: &RpcClient,
        mutable_accounts: &[Pubkey],
        use_helius: bool,
    ) -> BloberClientResult<Instruction> {
        let compute_unit_price = match self {
            Self::Fixed(fee) => fee.prioritization_fee_rate,
            Self::BasedOnRecentFees(priority) => {
                priority
                    .get_priority_fee_estimate(client, mutable_accounts, use_helius)
                    .await?
            }
        };
        Ok(ComputeBudgetInstruction::set_compute_unit_price(
            compute_unit_price.0,
        ))
    }
}
