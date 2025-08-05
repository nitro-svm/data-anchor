use anchor_lang::prelude::Pubkey;
use solana_client::nonblocking::rpc_client::RpcClient;
use tracing::Instrument;

use super::Lamports;
use crate::{ChainError, DataAnchorClientResult, Fee, Priority, TransactionType};

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

impl From<Fee> for FeeStrategy {
    fn from(fee: Fee) -> Self {
        Self::Fixed(fee)
    }
}

impl From<Priority> for FeeStrategy {
    fn from(priority: Priority) -> Self {
        Self::BasedOnRecentFees(priority)
    }
}

impl FeeStrategy {
    /// Converts a [`FeeStrategy`] into a [`Fee`] with the current compute unit price.
    pub(crate) async fn convert_fee_strategy_to_fixed(
        &self,
        rpc_client: &RpcClient,
        mutating_accounts: &[Pubkey],
        tx_type: TransactionType,
    ) -> DataAnchorClientResult<Fee> {
        let priority = match self {
            FeeStrategy::Fixed(fee) => {
                // If the fee strategy is already fixed, return it as is.
                return Ok(*fee);
            }
            FeeStrategy::BasedOnRecentFees(priority) => priority,
        };

        let mut fee_retries = 5;

        while fee_retries > 0 {
            let res = priority
                .get_priority_fee_estimate(rpc_client, mutating_accounts)
                .in_current_span()
                .await;

            match res {
                Ok(fee) => {
                    return Ok(Fee {
                        prioritization_fee_rate: fee,
                        num_signatures: tx_type.num_signatures(),
                        compute_unit_limit: tx_type.compute_unit_limit(),
                        price_per_signature: Lamports(5000),
                        blob_account_size: 0,
                    });
                }
                Err(e) => {
                    fee_retries -= 1;
                    if fee_retries == 0 {
                        return Err(e);
                    }
                }
            }
        }

        Err(ChainError::ConversionError("Fee strategy conversion failed after retries").into())
    }
}
