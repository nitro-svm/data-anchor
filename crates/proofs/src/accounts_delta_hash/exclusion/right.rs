//! Exclusion proof of an account that would be to the right of the rightmost leaf in the tree.

use serde::{Deserialize, Serialize};
use solana_sdk::{hash::Hash, pubkey::Pubkey};
use thiserror::Error;

use crate::accounts_delta_hash::inclusion::InclusionProof;

/// A proof that a specific account is not present in the accounts_delta_hash.
/// This proof is constructed by proving that the rightmost leaf in the tree
/// would be to the left of the excluded account.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ExclusionRightProof {
    pub(crate) excluded: Pubkey,
    pub(crate) rightmost: InclusionProof,
}

/// Failures that can occur when verifying an [`ExclusionRightProof`].
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ExclusionRightProofError {
    #[error("The excluded account wouldn't be the rightmost leaf")]
    ExcludedNotRightmost,
    #[error("The rightmost leaf must always be the last node in each level of the tree")]
    RightmostNotLastNode,
    #[error("Rightmost inclusion proof failed")]
    RightmostNotIncluded,
}

impl ExclusionRightProof {
    /// Verifies that an account is not present in the accounts_delta_hash.
    pub fn verify(&self, accounts_delta_hash: Hash) -> Result<(), ExclusionRightProofError> {
        if &self.excluded <= self.rightmost.pubkey() {
            // The excluded account must be to the right of the rightmost leaf.
            return Err(ExclusionRightProofError::ExcludedNotRightmost);
        } else if self
            .rightmost
            .levels
            .iter()
            .any(|level| level.index != level.siblings.len())
        {
            // The rightmost leaf must always be the last node in each level of the tree.
            return Err(ExclusionRightProofError::RightmostNotLastNode);
        }

        if !self.rightmost.verify(accounts_delta_hash) {
            return Err(ExclusionRightProofError::RightmostNotIncluded);
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
    fn exclusion_right_construction() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts {
                accounts,
                mut accounts_delta_hash,
                tree,
            } = generate_accounts(u, BTreeSet::new(), Vec::new())?;

            let (excluded_index, excluded) = choose_or_generate(u, &accounts)?;
            let excluded = excluded.0.pubkey();
            let (mut rightmost_index, mut rightmost) = choose_or_generate(u, &accounts)?;

            let mut unmodified = true;
            if u.ratio(1, 10)? {
                rightmost = u.arbitrary()?;
                unmodified = false;
            } else if u.ratio(1, 10)? {
                let prev_index = rightmost_index;
                rightmost_index = Some(u.choose_index(accounts.len())?);
                unmodified = rightmost_index == prev_index;
            } else if u.ratio(1, 10)? {
                accounts_delta_hash = Hash::new_from_array(u.arbitrary()?);
                unmodified = false;
            }

            let proof = ExclusionRightProof {
                rightmost: tree.unchecked_inclusion_proof(
                    rightmost_index.unwrap_or_arbitrary(u)?,
                    &rightmost.0.pubkey(),
                    &rightmost.1.clone().into(),
                ),
                excluded,
            };

            if unmodified
                && excluded_index.is_none()
                && rightmost.0.pubkey() < excluded
                && !accounts.is_empty()
                && rightmost_index == Some(accounts.len() - 1)
            {
                dbg!(
                    rightmost_index,
                    rightmost.0.pubkey(),
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
