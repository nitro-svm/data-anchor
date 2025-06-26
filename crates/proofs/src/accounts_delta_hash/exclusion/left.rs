//! Exclusion proof of an account that would be to the left of the leftmost leaf in the tree.

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use thiserror::Error;

use crate::accounts_delta_hash::inclusion::InclusionProof;

/// A proof that a specific account is not present in the accounts_delta_hash.
/// This proof is constructed by proving that the leftmost leaf in the tree
/// would be to the right of the excluded account.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ExclusionLeftProof {
    pub(crate) excluded: Pubkey,
    pub(crate) leftmost: InclusionProof,
}

/// Failures that can occur when verifying an [`ExclusionLeftProof`].
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ExclusionLeftProofError {
    #[error("The inclusion proof is not for the leftmost leaf")]
    InclusionProofNotForLeftmost,
    #[error("The excluded account wouldn't be the leftmost leaf")]
    ExcludedNotLeftmost,
    #[error("Leftmost inclusion proof failed")]
    LeftmostNotIncluded,
}

impl ExclusionLeftProof {
    /// Verifies that an account is not present in the accounts_delta_hash.
    pub fn verify(
        &self,
        accounts_delta_hash: solana_sdk::hash::Hash,
    ) -> Result<(), ExclusionLeftProofError> {
        if self.leftmost.levels.iter().any(|level| level.index != 0) {
            // If any of the indices in the path for the leftmost leaf are non-zero, then it's
            // not actually the leftmost leaf.
            return Err(ExclusionLeftProofError::InclusionProofNotForLeftmost);
        } else if self.leftmost.pubkey() <= &self.excluded {
            // The excluded account must be to the left of the leftmost leaf.
            return Err(ExclusionLeftProofError::ExcludedNotLeftmost);
        }

        // Sanity checks done, proceed with checking the proof.
        if !self.leftmost.verify(accounts_delta_hash) {
            return Err(ExclusionLeftProofError::LeftmostNotIncluded);
        }

        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use arbtest::arbtest;

    use super::*;
    use crate::accounts_delta_hash::testing::{
        TestAccounts, UnwrapOrArbitrary, choose_or_generate, generate_accounts,
    };

    #[test]
    fn exclusion_left_construction() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts {
                accounts,
                mut accounts_delta_hash,
                tree,
            } = generate_accounts(u, BTreeSet::new(), Vec::new())?;

            let (leftmost_index, mut leftmost) = choose_or_generate(u, &accounts)?;
            let (excluded_index, excluded) = choose_or_generate(u, &accounts)?;
            let excluded = excluded.0.pubkey();

            let mut unmodified = true;
            if u.ratio(1, 5)? {
                leftmost = u.arbitrary()?;
                unmodified = false;
            } else if u.ratio(1, 10)? {
                accounts_delta_hash = solana_sdk::hash::Hash::new_from_array(u.arbitrary()?);
                unmodified = false;
            }

            let proof = ExclusionLeftProof {
                excluded,
                leftmost: tree.unchecked_inclusion_proof(
                    leftmost_index.unwrap_or_arbitrary(u)?,
                    &leftmost.0.pubkey(),
                    &leftmost.1.clone().into(),
                ),
            };

            if unmodified
                && leftmost_index == Some(0)
                && excluded_index.is_none()
                && excluded < leftmost.0.pubkey()
            {
                dbg!(
                    leftmost_index,
                    leftmost.0.pubkey(),
                    excluded_index,
                    &tree,
                    &proof
                );
                assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
                return Ok(());
            }

            dbg!(&proof);
            assert_ne!(proof.verify(accounts_delta_hash), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }
}
