use std::{fmt::Display, num::TryFromIntError};

use thiserror::Error;

use super::MicroLamports;

/// The smallest fraction of the native Solana token, SOL. 1 lamport = 0.000000001 SOL.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lamports(pub(crate) u32);

impl Lamports {
    /// Zero lamports.
    pub const ZERO: Self = Lamports(0);

    /// Create an instance of `Lamports` from a given value.
    pub fn new(value: u32) -> Self {
        Lamports(value)
    }

    /// Extracts the inner value.
    pub fn into_inner(self) -> u32 {
        self.0
    }

    /// Multiplies the inner value by the given value, returning `None` if the result would overflow.
    pub fn checked_mul(&self, rhs: u32) -> Option<Self> {
        self.0.checked_mul(rhs).map(Lamports)
    }

    /// Divides the inner value by the given value, returning `None` if `rhs` == 0.
    pub fn checked_div(&self, rhs: u32) -> Option<Self> {
        self.0.checked_div(rhs).map(Lamports)
    }

    /// Adds the inner value to the given value, returning `None` if the result would overflow.
    pub fn checked_add(&self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Lamports)
    }

    /// Subtracts the inner value from the given value, returning `None` if the result would underflow.
    pub fn checked_sub(&self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Lamports)
    }
}

impl Display for Lamports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} lamports", self.0)
    }
}

impl TryFrom<MicroLamports> for Lamports {
    type Error = LamportsFromMicroLamportsError;

    fn try_from(value: MicroLamports) -> Result<Self, Self::Error> {
        Ok(Lamports(value.0.div_ceil(1_000_000).try_into().map_err(
            |e| LamportsFromMicroLamportsError::Overflow(value.0, e),
        )?))
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum LamportsFromMicroLamportsError {
    #[error("Microlamports value is too large ({0} / 1 000 000 > 2^32-1), it would overflow ({1})")]
    Overflow(u64, #[source] TryFromIntError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micro_lamports_to_lamports_rounds_up() {
        assert_eq!(Lamports::try_from(MicroLamports(0)), Ok(Lamports(0)));
        assert_eq!(Lamports::try_from(MicroLamports(500_000)), Ok(Lamports(1)));
        assert_eq!(
            Lamports::try_from(MicroLamports(1_000_000)),
            Ok(Lamports(1))
        );
        assert_eq!(
            Lamports::try_from(MicroLamports(1_000_001)),
            Ok(Lamports(2))
        );
    }

    #[test]
    fn more_than_max_lamports_errors() {
        let too_large_value = (u32::MAX as u64 + 1) * 1_000_000;
        let err = Lamports::try_from(MicroLamports(too_large_value)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Microlamports value is too large (4294967296000000 / 1 000 000 > 2^32-1), it would overflow (out of range integral type conversion attempted)",
        );
    }
}
