use clap::ValueEnum;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use super::MicroLamports;
use crate::BloberClientResult;

/// The percentile of recent prioritization fees to use as the compute unit price for a transaction.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, ValueEnum,
)]
pub enum Priority {
    /// 0th percentile
    Min,
    /// 25th percentile
    Low,
    /// 50th percentile
    #[default]
    Medium,
    /// 75th percentile
    High,
    /// 95th percentile
    VeryHigh,
}

impl Priority {
    /// Converts the priority enumeration to a percentile value between 0 and 1.
    pub fn percentile(&self) -> f32 {
        match self {
            Self::Min => 0.0,
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.75,
            Self::VeryHigh => 0.95,
        }
    }

    /// Finds the closest value to a given percentile in a sorted list of values.
    ///
    /// # Arguments
    /// - `sorted_values`: The list of values to search. Must be sorted in ascending order. Must not be empty.
    fn calculate_percentile(&self, sorted_fees: &[u64]) -> MicroLamports {
        if sorted_fees.is_empty() {
            return MicroLamports::ZERO;
        }
        let percentile = self.percentile();
        let index = (percentile * (sorted_fees.len() as f32 - 1.0)) as usize;
        MicroLamports(sorted_fees[index.min(sorted_fees.len() - 1)])
    }

    /// Calculates a recommended compute unit price for a transaction based on recent prioritization fees.
    ///
    /// # Arguments
    /// - `client`: The RPC client to use for looking up recent prioritization fees.
    /// - `mutable_accounts`: The addresses of the accounts that are mutable in the transaction (and thus need exclusive locks).
    pub async fn calculate_compute_unit_price(
        &self,
        client: &RpcClient,
        mutable_accounts: &[Pubkey],
    ) -> BloberClientResult<MicroLamports> {
        let recent_prioritization_fees = client
            .get_recent_prioritization_fees(mutable_accounts)
            .await?;
        if recent_prioritization_fees.is_empty() {
            return Ok(MicroLamports::ZERO);
        }
        let sorted_fees = recent_prioritization_fees
            .into_iter()
            .map(|f| f.prioritization_fee)
            .sorted()
            .collect::<Vec<_>>();
        Ok(self.calculate_percentile(&sorted_fees))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_is_default() {
        // This is probably important enough to warrant locking it down with a test.
        let default = Priority::default();
        let medium = Priority::Medium;
        assert_eq!(medium, default);
    }
}
