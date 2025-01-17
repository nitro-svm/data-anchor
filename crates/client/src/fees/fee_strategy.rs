use super::{Fee, Priority};

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
