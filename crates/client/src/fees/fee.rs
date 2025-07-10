use anchor_lang::prelude::Rent;
use solana_sdk::{compute_budget::ComputeBudgetInstruction, instruction::Instruction};

use super::{Lamports, MicroLamports};

/// The expected fees for a blob upload, broken down by source.
#[derive(Debug, Copy, Clone)]
pub struct Fee {
    /// The number of signatures required, summed across all transactions.
    pub num_signatures: u16,
    /// The price per signature, in lamports. 5000 lamports by default.
    pub price_per_signature: Lamports,
    /// The compute unit limit, summed across all transactions.
    pub compute_unit_limit: u32,
    /// The prioritization fee rate, in micro-lamports.
    pub prioritization_fee_rate: MicroLamports,
    /// The required size of the blober account, in bytes.
    pub blob_account_size: usize,
}

impl Fee {
    pub const ZERO: Fee = Fee {
        num_signatures: 0,
        price_per_signature: Lamports::ZERO,
        compute_unit_limit: 0,
        prioritization_fee_rate: MicroLamports::ZERO,
        blob_account_size: 0,
    };

    /// Calculate the static part of the fee for a blob upload.
    /// It is proportional to the number of signatures.
    pub fn static_fee(&self) -> Lamports {
        self.price_per_signature
            .checked_mul(self.num_signatures as u32)
            .expect("multiplication overflow")
    }

    /// Calculate the recommended prioritization fee for a blob upload at the given priority.
    /// It is proportional to the compute unit limit, *not* the actual consumed compute units.
    /// The value is rounded up to the nearest lamport.
    pub fn prioritization_fee(&self) -> Lamports {
        self.prioritization_fee_rate
            .checked_mul(self.compute_unit_limit as u64)
            .expect("multiplication overflow")
            .try_into()
            .expect("failed to convert from micro-lamports to lamports")
    }

    /// Calculate the total fee for a blob upload, including the static fee and the prioritization fee.
    /// Does not include rent.
    pub fn total_fee(&self) -> Lamports {
        self.static_fee()
            .checked_add(self.prioritization_fee())
            .expect("addition overflow")
    }

    /// Calculate the required rent used as a deposit for the blober account.
    /// Solana programs must hold on to a certain amount of lamports (SOL) in order to exist on-chain.
    /// This rent is paid upfront whenever an account is created or resized, and is proportional to
    /// the size of the account.
    pub fn rent(&self) -> Lamports {
        let minimum_balance = Rent::default().minimum_balance(self.blob_account_size) as u32;
        Lamports::new(minimum_balance)
    }

    /// Creates a transaction for setting the compute unit price for a transaction.
    pub fn set_compute_unit_price(&self) -> Instruction {
        ComputeBudgetInstruction::set_compute_unit_price(self.prioritization_fee_rate.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn less_than_one_lamport_prioritization_fee_is_ok() {
        let fee = Fee {
            num_signatures: 1,
            price_per_signature: Lamports::new(5000),
            compute_unit_limit: 1,
            prioritization_fee_rate: MicroLamports::new(999_999),
            blob_account_size: 100,
        };
        assert_eq!(fee.prioritization_fee(), Lamports::new(1));
    }
}
