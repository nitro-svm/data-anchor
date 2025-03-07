use clap::ValueEnum;
use helius::types::{
    GetPriorityFeeEstimateOptions, GetPriorityFeeEstimateRequest, GetPriorityFeeEstimateResponse,
    PriorityLevel,
};
use itertools::Itertools;
use jsonrpsee::{core::client::ClientT, http_client::HttpClient, rpc_params};
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

impl From<Priority> for PriorityLevel {
    fn from(value: Priority) -> Self {
        match value {
            Priority::Min => Self::Min,
            Priority::Low => Self::Low,
            Priority::Medium => Self::Medium,
            Priority::High => Self::High,
            Priority::VeryHigh => Self::VeryHigh,
        }
    }
}

impl From<&Priority> for PriorityLevel {
    fn from(value: &Priority) -> Self {
        (*value).into()
    }
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
            return MicroLamports::MIN;
        }
        let percentile = self.percentile();
        let index = (percentile * (sorted_fees.len() as f32 - 1.0)) as usize;
        MicroLamports(sorted_fees[index.min(sorted_fees.len() - 1)].max(MicroLamports::MIN.0))
    }

    /// Calculates a recommended compute unit price for a transaction based on recent prioritization fees.
    pub async fn get_priority_fee_estimate(
        &self,
        client: &RpcClient,
        mutable_accounts: &[Pubkey],
        use_helius: bool,
    ) -> BloberClientResult<MicroLamports> {
        if use_helius {
            self.get_helius_priority_fee(client, mutable_accounts).await
        } else {
            self.calculate_compute_unit_price(client, mutable_accounts)
                .await
        }
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
            return Ok(MicroLamports::MIN);
        }
        let sorted_fees = recent_prioritization_fees
            .into_iter()
            .map(|f| f.prioritization_fee)
            .sorted()
            .collect::<Vec<_>>();
        Ok(self.calculate_percentile(&sorted_fees))
    }

    /// Calculates a recommended priority fee for a transaction based on recent prioritization fees, using the Helius API
    /// Based on https://docs.helius.dev/solana-apis/priority-fee-api
    pub async fn get_helius_priority_fee(
        &self,
        client: &RpcClient,
        mutable_accounts: &[Pubkey],
    ) -> BloberClientResult<MicroLamports> {
        let client = HttpClient::builder().build(client.url()).unwrap();
        let estimate: GetPriorityFeeEstimateResponse = client
            .request(
                "getPriorityFeeEstimate",
                rpc_params![GetPriorityFeeEstimateRequest {
                    transaction: None,
                    account_keys: Some(mutable_accounts.iter().map(|p| p.to_string()).collect()),
                    options: Some(GetPriorityFeeEstimateOptions {
                        priority_level: Some(self.into()),
                        ..Default::default()
                    })
                }],
            )
            .await
            .unwrap();

        Ok(MicroLamports(
            estimate
                .priority_fee_estimate
                .expect("The request we call should result in presence of this value")
                .ceil() as u64,
        ))
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
