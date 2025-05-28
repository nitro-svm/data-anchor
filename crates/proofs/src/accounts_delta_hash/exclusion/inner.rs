//! Exclusion proof of an account that would be somewhere in the middle of the tree.

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use thiserror::Error;

use crate::accounts_delta_hash::{account_merkle_tree::MERKLE_FANOUT, inclusion::InclusionProof};

/// A proof that a specific account is not present in the accounts_delta_hash.
/// This proof is constructed by proving that the left and right siblings of where
/// the excluded account would have been in the merkle tree are adjacent to each other.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ExclusionInnerProof {
    pub(crate) excluded: Pubkey,
    pub(crate) left: InclusionProof,
    pub(crate) right: InclusionProof,
}

/// Failures that can occur when verifying an [`ExclusionInnerProof`].
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ExclusionInnerProofError {
    #[error("The proofs have different path lengths, or are empty")]
    PathLengthMismatch,
    #[error("The excluded account wouldn't be inbetween the left and right leaves")]
    ExcludedNotBetweenLeftAndRight,
    #[error("The inclusion proof is not for adjacent leaves")]
    NotForAdjacentLeaves,
    #[error("Inner left inclusion proof failed")]
    LeftRootNotIncluded,
    #[error("Inner right inclusion proof failed")]
    RightRootNotIncluded,
}

impl ExclusionInnerProof {
    /// Verifies that an account is not present in the accounts_delta_hash.
    pub fn verify(
        &self,
        accounts_delta_hash: solana_sdk::hash::Hash,
    ) -> Result<(), ExclusionInnerProofError> {
        if self.left.levels.len() != self.right.levels.len() {
            // The paths must have equal length, otherwise they came from different trees.
            return Err(ExclusionInnerProofError::PathLengthMismatch);
        } else if &self.excluded <= self.left.pubkey() || self.right.pubkey() <= &self.excluded {
            // The excluded account must be between the left and right leaves.
            // This also covers the cases where any of [left, excluded, right] are equal to each other.
            return Err(ExclusionInnerProofError::ExcludedNotBetweenLeftAndRight);
        }

        // We use an integer instead of an absolute value to avoid the edge case when
        // left is 0 and right is `MERKLE_FANOUT - 1`, which would result in a positive value.
        const SUBTREE: isize = -((MERKLE_FANOUT - 1) as isize);
        let mut prev_diff = SUBTREE;
        for (left_level, right_level) in self.left.levels.iter().zip(self.right.levels.iter()) {
            let curr_diff = right_level.index as isize - left_level.index as isize;
            match (prev_diff, curr_diff) {
                // There are only 3 valid transitions.
                // - subtree -> subtree: two nodes are adjacent but belong to different subtrees, and their parents are adjacent but belong to different subtrees
                // - subtree -> sibling (1): two nodes are adjacent but belong to different subtrees, and their parents are adjacent siblings
                // - sibling (1) -> same (0): once the nodes are adjacent siblings, then they must have the same parent
                // - same (0) -> same (0): once the paths have converged on the same node, there's no way for them to differ anymore
                (SUBTREE, SUBTREE) | (SUBTREE, 1) | (1, 0) | (0, 0) => prev_diff = curr_diff,
                // The paths in the two proofs diverged, meaning this inclusion proof is not for adjacent leaves.
                _ => return Err(ExclusionInnerProofError::NotForAdjacentLeaves),
            }
        }

        // Sanity checks done, proceed with checking the proofs.
        if !self.left.verify(accounts_delta_hash) {
            return Err(ExclusionInnerProofError::LeftRootNotIncluded);
        } else if !self.right.verify(accounts_delta_hash) {
            return Err(ExclusionInnerProofError::RightRootNotIncluded);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use arbtest::arbtest;
    use solana_accounts_db::{accounts_db::AccountsDb, accounts_hash::AccountsHasher};
    use solana_sdk::{account::Account, pubkey::Pubkey};

    use super::*;
    use crate::accounts_delta_hash::{
        testing::{
            choose_or_generate, generate_accounts, ArbAccount, ArbKeypair, TestAccounts,
            UnwrapOrArbitrary,
        },
        AccountMerkleTree,
    };

    #[test]
    fn exclusion_inner_construction() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts {
                accounts,
                mut accounts_delta_hash,
                tree,
            } = generate_accounts(u, BTreeSet::new(), Vec::new())?;

            let (excluded_index, excluded) = choose_or_generate(u, &accounts)?;
            let excluded = excluded.0.pubkey();
            let (left_index, mut left) = choose_or_generate(u, &accounts)?;
            let (right_index, mut right) = if left_index.is_some()
                && left_index.unwrap() + 1 < accounts.len()
                && u.arbitrary()?
            {
                let right_index = left_index.unwrap() + 1;
                (
                    Some(right_index),
                    accounts.get(right_index).unwrap().clone(),
                )
            } else {
                choose_or_generate(u, &accounts)?
            };

            let mut unmodified = true;
            if u.ratio(1, 10)? {
                left = u.arbitrary()?;
                unmodified = false;
            } else if u.ratio(1, 10)? {
                right = u.arbitrary()?;
                unmodified = false;
            } else if u.ratio(1, 10)? {
                accounts_delta_hash = solana_sdk::hash::Hash::new_from_array(u.arbitrary()?);
                unmodified = false;
            }

            let proof = if u.ratio(1, 10)? {
                let accounts2 = generate_accounts(u, BTreeSet::new(), Vec::new())?;
                let (right_index, right) = choose_or_generate(u, &accounts2.accounts)?;
                unmodified = false;
                ExclusionInnerProof {
                    left: tree.unchecked_inclusion_proof(
                        left_index.unwrap_or_arbitrary(u)?,
                        &left.0.pubkey(),
                        &left.1.clone().into(),
                    ),
                    excluded,
                    right: tree.unchecked_inclusion_proof(
                        right_index.unwrap_or_arbitrary(u)?,
                        &right.0.pubkey(),
                        &right.1.clone().into(),
                    ),
                }
            } else {
                ExclusionInnerProof {
                    left: tree.unchecked_inclusion_proof(
                        left_index.unwrap_or_arbitrary(u)?,
                        &left.0.pubkey(),
                        &left.1.clone().into(),
                    ),
                    excluded,
                    right: tree.unchecked_inclusion_proof(
                        right_index.unwrap_or_arbitrary(u)?,
                        &right.0.pubkey(),
                        &right.1.clone().into(),
                    ),
                }
            };

            if let Some((left_index, right_index)) = left_index.zip(right_index) {
                if unmodified
                    && excluded_index.is_none()
                    && left.0.pubkey() < excluded
                    && excluded < right.0.pubkey()
                    && right_index > 0
                    && left_index == right_index - 1
                {
                    dbg!(
                        left_index,
                        left.0.pubkey(),
                        excluded_index,
                        right_index,
                        right.0.pubkey(),
                        &tree,
                        &proof
                    );
                    assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
                    return Ok(());
                }
            }

            dbg!(&proof);
            assert_ne!(proof.verify(accounts_delta_hash), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }

    /// This test is similar to the one above, but it specifically constructs a tree
    /// where the excluded account is in the middle of the tree. This is the worst case
    /// scenario which requires the most amount of hashes for an inner exclusion proof.
    /// It also verifies that we don't reject the merkle tree path when the left and right
    /// leaves belong to different subtrees.
    #[test]
    fn exclusion_inner_construction_middle_of_the_tree() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let mut accounts: Vec<(ArbKeypair, ArbAccount)> = {
                // The tree must be at least two levels deep (MERKLE_FANOUT * MERKLE_FANOUT),
                // multiply it by two to make it a more exhaustive test case, and add one
                // to account for removing the middle (excluded) account.
                let len = 2 * MERKLE_FANOUT * MERKLE_FANOUT + 1;
                let mut set: HashSet<(ArbKeypair, ArbAccount)> = u.arbitrary()?;
                // We need a set of an exact size, so `u.arbitrary()` is not enough on its own.
                while set.len() < len {
                    if u.is_empty() {
                        return Err(arbitrary::Error::NotEnoughData);
                    }
                    set.insert(u.arbitrary()?);
                }
                set.into_iter().collect()
            };
            accounts.sort_by_key(|(keypair, _)| keypair.pubkey());

            let excluded_index = accounts.len() / 2;
            let excluded = accounts.remove(excluded_index).0.pubkey();
            let left_index = excluded_index - 1;
            let left = accounts[left_index].clone();
            let right_index = excluded_index;
            let right = accounts[right_index].clone();

            let accounts: Vec<(Pubkey, Account)> = accounts
                .into_iter()
                .map(|(keypair, account)| (keypair.pubkey(), account.into()))
                .collect();

            let hashes: Vec<(Pubkey, solana_accounts_db::accounts_hash::AccountHash)> = accounts
                .iter()
                .map(|(pubkey, account)| (*pubkey, AccountsDb::hash_account(account, pubkey)))
                .collect();

            // This is the official root computed by Solana validators, considered a source of truth.
            let accounts_delta_hash = AccountsHasher::accumulate_account_hashes(hashes.clone());

            dbg!(&accounts);

            // This is the complete merkle tree, used to construct the proofs.
            let mut tree_builder = AccountMerkleTree::builder([excluded].into_iter().collect());

            for (pubkey, account) in accounts.into_iter() {
                tree_builder.insert(pubkey, account);
            }
            let tree = tree_builder.build();

            // Sanity check.
            assert_eq!(tree.root(), accounts_delta_hash);

            let proof = ExclusionInnerProof {
                left: tree.unchecked_inclusion_proof(
                    left_index,
                    &left.0.pubkey(),
                    &left.1.clone().into(),
                ),
                excluded,
                right: tree.unchecked_inclusion_proof(
                    right_index,
                    &right.0.pubkey(),
                    &right.1.clone().into(),
                ),
            };

            if left.0.pubkey() != right.0.pubkey() && left.0.pubkey() != excluded {
                dbg!(
                    excluded_index,
                    left_index,
                    left.0.pubkey(),
                    right_index,
                    right.0.pubkey(),
                    &tree,
                    &proof
                );
                assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
                return Ok(());
            }

            dbg!(
                excluded_index,
                left_index,
                left.0.pubkey(),
                right_index,
                right.0.pubkey(),
                &tree,
                &proof
            );
            assert_ne!(proof.verify(accounts_delta_hash), Ok(()));
            Ok(())
        })
        .size_min(100_000_000)
        .size_max(100_000_000);
    }
}
