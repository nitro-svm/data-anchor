use super::Lamports;

/// 10^-6 lamports, only used for prioritization fee calculations.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MicroLamports(pub(crate) u64);

impl MicroLamports {
    /// Zero micro-lamports.
    pub const ZERO: Self = MicroLamports(0);
    /// Minimum recommended fee for a transaction. Based on https://docs.helius.dev/solana-apis/priority-fee-api#helius-priority-fee-api
    pub const MIN: Self = MicroLamports(10_000);

    /// Create an instance of `MicroLamports` from a given value.
    pub fn new(value: u64) -> Self {
        MicroLamports(value)
    }

    /// Extracts the inner value.
    pub fn into_inner(self) -> u64 {
        self.0
    }

    /// Multiplies the inner value by the given value, returning `None` if the result would overflow.
    pub fn checked_mul(&self, rhs: u64) -> Option<Self> {
        self.0.checked_mul(rhs).map(MicroLamports)
    }

    /// Divides the inner value by the given value, returning `None` if `rhs` == 0.
    pub fn checked_div(&self, rhs: u64) -> Option<Self> {
        self.0.checked_div(rhs).map(MicroLamports)
    }

    /// Divides the inner value from the given value, returning `None` if the result would underflow.
    pub fn checked_div_self(&self, rhs: Self) -> Option<u64> {
        self.0.checked_div(rhs.0)
    }

    /// Adds the inner value to the given value, returning `None` if the result would overflow.
    pub fn checked_add(&self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(MicroLamports)
    }

    /// Subtracts the given value from the inner value, returning `None` if the result would underflow.
    pub fn checked_sub(&self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(MicroLamports)
    }
}

impl From<Lamports> for MicroLamports {
    fn from(value: Lamports) -> Self {
        // Can't overflow because MicroLamports is u64 and Lamports is u32.
        MicroLamports(value.0 as u64 * 1_000_000)
    }
}
