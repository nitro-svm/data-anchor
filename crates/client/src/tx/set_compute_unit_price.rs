use std::cmp::min;

use itertools::Itertools;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
};

use crate::{
    fees::{FeeStrategy, MicroLamports, Priority},
    BloberClientResult,
};

/// Creates a transaction for setting the compute unit price for a transaction based on recent prioritization fees.
///
/// # Arguments
/// - `client`: The RPC client to use for looking up recent prioritization fees.
/// - `mutable_accounts`: The addresses of the accounts that are mutable in the transaction (and thus need exclusive locks).
/// - `fee_strategy`: The strategy to use for calculating the fees for transactions.
pub async fn set_compute_unit_price(
    client: &RpcClient,
    mutable_accounts: &[Pubkey],
    fee_strategy: FeeStrategy,
) -> BloberClientResult<Instruction> {
    let compute_unit_price = match fee_strategy {
        FeeStrategy::Fixed(fee) => fee.prioritization_fee_rate,
        FeeStrategy::BasedOnRecentFees(priority) => {
            calculate_compute_unit_price(client, mutable_accounts, priority).await?
        }
    };
    let instruction = ComputeBudgetInstruction::set_compute_unit_price(compute_unit_price.0);
    Ok(instruction)
}

/// Calculates a recommended compute unit price for a transaction based on recent prioritization fees.
///
/// # Arguments
/// - `client`: The RPC client to use for looking up recent prioritization fees.
/// - `mutable_accounts`: The addresses of the accounts that are mutable in the transaction (and thus need exclusive locks).
/// - `priority`: The priority of the transaction. Higher priority transactions are more likely to be included in a block.
pub async fn calculate_compute_unit_price(
    client: &RpcClient,
    mutable_accounts: &[Pubkey],
    priority: Priority,
) -> BloberClientResult<MicroLamports> {
    let recent_prioritization_fees = client
        .get_recent_prioritization_fees(mutable_accounts)
        .await?;
    let sorted_fees = recent_prioritization_fees
        .into_iter()
        .map(|f| f.prioritization_fee)
        .sorted()
        .collect::<Vec<_>>();
    if sorted_fees.is_empty() {
        return Ok(MicroLamports::ZERO);
    }
    assert!(!sorted_fees.is_empty());
    let compute_unit_price = calculate_percentile(&sorted_fees, priority);
    Ok(MicroLamports::new(*compute_unit_price))
}

/// Finds the closest value to a given percentile in a sorted list of values.
///
/// # Arguments
/// - `sorted_values`: The list of values to search. Must be sorted in ascending order. Must not be empty.
/// - `priority`: The percentile to find, expressed as a priority.
fn calculate_percentile<T: PartialOrd>(sorted_values: &[T], priority: Priority) -> &T {
    assert!(!sorted_values.is_empty());
    assert!(sorted_values.windows(2).all(|w| w[0] <= w[1]));
    let percentile_index = (sorted_values.len() as f32 * priority.percentile()).round() as usize;
    &sorted_values[min(percentile_index, sorted_values.len() - 1)]
}
