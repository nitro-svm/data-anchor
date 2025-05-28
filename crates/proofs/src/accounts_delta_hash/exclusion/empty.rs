//! Exclusion proof for when no accounts were changed in the block.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The simplest proof possible, which proves that there are no accounts in the accounts_delta_hash.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ExclusionEmptyProof;

/// Failures that can occur when verifying an [`ExclusionEmptyProof`].c
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ExclusionEmptyProofError {
    #[error("Empty inclusion proof failed")]
    RootMismatch,
}

impl ExclusionEmptyProof {
    /// Verifies that the accounts_delta_hash is for the empty set of accounts.
    pub fn verify(
        &self,
        accounts_delta_hash: solana_sdk::hash::Hash,
    ) -> Result<(), ExclusionEmptyProofError> {
        // If there are no accounts that were updated, Solana defaults to an empty hash.
        if accounts_delta_hash != solana_sdk::hash::Hasher::default().result() {
            return Err(ExclusionEmptyProofError::RootMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use arbtest::arbtest;

    use super::*;
    use crate::accounts_delta_hash::testing::{generate_accounts, TestAccounts};

    #[test]
    fn exclusion_empty_construction() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts {
                accounts,
                accounts_delta_hash,
                tree: levels,
            } = generate_accounts(u, BTreeSet::new(), Vec::new())?;

            let proof = ExclusionEmptyProof;

            dbg!(&levels, &proof);
            if accounts.is_empty() {
                assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
                return Ok(());
            }

            assert_ne!(proof.verify(accounts_delta_hash), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }
}
