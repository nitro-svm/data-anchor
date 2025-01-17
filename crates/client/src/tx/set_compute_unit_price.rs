use std::cmp::min;

use anchor_lang::prelude::Pubkey;
use itertools::Itertools;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{compute_budget::ComputeBudgetInstruction, instruction::Instruction};

use crate::{
    fees::{FeeStrategy, MicroLamports, Priority},
    Error,
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
) -> Result<Instruction, Error> {
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
) -> Result<MicroLamports, Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_percentiles_arbitrary_values() {
        let recent_fees = [501, 102, 20003, 404, 305, 306, 207];
        let sorted_fees = recent_fees.into_iter().sorted().collect::<Vec<_>>();
        let percentile_0 = calculate_percentile(&sorted_fees, Priority::Min);
        assert_eq!(*percentile_0, 102);
        let percentile_25 = calculate_percentile(&sorted_fees, Priority::Low);
        assert_eq!(*percentile_25, 305);
        let percentile_50 = calculate_percentile(&sorted_fees, Priority::Medium);
        assert_eq!(*percentile_50, 404);
        let percentile_75 = calculate_percentile(&sorted_fees, Priority::High);
        assert_eq!(*percentile_75, 501);
        let percentile_95 = calculate_percentile(&sorted_fees, Priority::VeryHigh);
        assert_eq!(*percentile_95, 20003);
    }

    #[test]
    fn priority_percentiles_exact_values() {
        let recent_fees = 0..100u64;
        let sorted_fees = recent_fees.into_iter().sorted().collect::<Vec<_>>();
        let percentile_0 = calculate_percentile(&sorted_fees, Priority::Min);
        assert_eq!(*percentile_0, 0);
        let percentile_25 = calculate_percentile(&sorted_fees, Priority::Low);
        assert_eq!(*percentile_25, 25);
        let percentile_50 = calculate_percentile(&sorted_fees, Priority::Medium);
        assert_eq!(*percentile_50, 50);
        let percentile_75 = calculate_percentile(&sorted_fees, Priority::High);
        assert_eq!(*percentile_75, 75);
        let percentile_95 = calculate_percentile(&sorted_fees, Priority::VeryHigh);
        assert_eq!(*percentile_95, 95);
    }

    #[test]
    fn priority_percentiles_single_value() {
        let sorted_fees = vec![1748];
        let percentile_0 = calculate_percentile(&sorted_fees, Priority::Min);
        assert_eq!(*percentile_0, 1748);
        let percentile_25 = calculate_percentile(&sorted_fees, Priority::Low);
        assert_eq!(*percentile_25, 1748);
        let percentile_50 = calculate_percentile(&sorted_fees, Priority::Medium);
        assert_eq!(*percentile_50, 1748);
        let percentile_75 = calculate_percentile(&sorted_fees, Priority::High);
        assert_eq!(*percentile_75, 1748);
        let percentile_95 = calculate_percentile(&sorted_fees, Priority::VeryHigh);
        assert_eq!(*percentile_95, 1748);
    }

    #[test]
    #[should_panic]
    fn percentile_of_unsorted_values_panics() {
        let recent_fees = [501, 102, 20003, 404, 305, 306, 207];
        calculate_percentile(&recent_fees, Priority::Medium);
    }

    #[test]
    #[should_panic]
    fn percentile_of_empty_values_panics() {
        let recent_fees: [u64; 0] = [];
        calculate_percentile(&recent_fees, Priority::Medium);
    }
}
