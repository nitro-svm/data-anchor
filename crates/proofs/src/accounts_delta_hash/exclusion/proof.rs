use serde::{Deserialize, Serialize};
use solana_sdk::{hash::Hash, pubkey::Pubkey};
use thiserror::Error;

use crate::accounts_delta_hash::exclusion::{
    empty::{ExclusionEmptyProof, ExclusionEmptyProofError},
    inner::{ExclusionInnerProof, ExclusionInnerProofError},
    left::{ExclusionLeftProof, ExclusionLeftProofError},
    right::{ExclusionRightProof, ExclusionRightProofError},
};

/// Represents any kind of exclusion proof, regardless of the specifics of the proof.
/// Useful to handle the different types of exclusion proofs in a generic way.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ExclusionProof {
    ExclusionLeft(ExclusionLeftProof),
    ExclusionInner(ExclusionInnerProof),
    ExclusionRight(ExclusionRightProof),
    ExclusionEmpty(ExclusionEmptyProof),
}

/// Failures that can occur when verifying an [`ExclusionProof`].
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ExclusionProofError {
    #[error(transparent)]
    Left(#[from] ExclusionLeftProofError),
    #[error(transparent)]
    Inner(#[from] ExclusionInnerProofError),
    #[error(transparent)]
    Right(#[from] ExclusionRightProofError),
    #[error(transparent)]
    Empty(#[from] ExclusionEmptyProofError),
}

impl ExclusionProof {
    /// Verifies that an account is not present in the accounts_delta_hash.
    pub fn verify(&self, accounts_delta_hash: Hash) -> Result<(), ExclusionProofError> {
        use ExclusionProof::*;
        // Delegate to the specific proof type implementations.
        match self {
            ExclusionLeft(proof) => Ok(proof.verify(accounts_delta_hash)?),
            ExclusionInner(proof) => Ok(proof.verify(accounts_delta_hash)?),
            ExclusionRight(proof) => Ok(proof.verify(accounts_delta_hash)?),
            ExclusionEmpty(proof) => Ok(proof.verify(accounts_delta_hash)?),
        }
    }

    /// Returns the excluded account pubkey, if any. Returns None for [`ExclusionProof::ExclusionEmpty`].
    pub fn excluded(&self) -> Option<&Pubkey> {
        use ExclusionProof::*;
        match self {
            ExclusionLeft(proof) => Some(&proof.excluded),
            ExclusionInner(proof) => Some(&proof.excluded),
            ExclusionRight(proof) => Some(&proof.excluded),
            _ => None,
        }
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
    fn any_exclusion_construction() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts {
                accounts,
                accounts_delta_hash,
                tree,
            } = generate_accounts(u, BTreeSet::new(), Vec::new())?;

            let (proof, is_valid) = match u.int_in_range(0..=3)? {
                0 => {
                    let (leftmost_index, leftmost) = choose_or_generate(u, &accounts)?;
                    let (excluded_index, excluded) = choose_or_generate(u, &accounts)?;
                    let excluded = excluded.0.pubkey();

                    let proof = ExclusionLeftProof {
                        excluded,
                        leftmost: tree.unchecked_inclusion_proof(
                            leftmost_index.unwrap_or_arbitrary(u)?,
                            &leftmost.0.pubkey(),
                            &leftmost.1.clone().into(),
                        ),
                    };

                    let is_valid = leftmost_index == Some(0)
                        && excluded_index.is_none()
                        && excluded < leftmost.0.pubkey();

                    (ExclusionProof::ExclusionLeft(proof), is_valid)
                }
                1 => {
                    let (excluded_index, excluded) = choose_or_generate(u, &accounts)?;
                    let excluded = excluded.0.pubkey();
                    let (left_index, left) = choose_or_generate(u, &accounts)?;
                    let (right_index, right) = choose_or_generate(u, &accounts)?;

                    dbg!(&tree, &left_index, &left, &right_index, &right);

                    let proof = ExclusionInnerProof {
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
                    };

                    let is_valid =
                        if let Some((left_index, right_index)) = left_index.zip(right_index) {
                            excluded_index.is_none()
                                && left.0.pubkey() < excluded
                                && excluded < right.0.pubkey()
                                && right_index > 0
                                && left_index == right_index - 1
                        } else {
                            false
                        };

                    (ExclusionProof::ExclusionInner(proof), is_valid)
                }
                2 => {
                    let (excluded_index, excluded) = choose_or_generate(u, &accounts)?;
                    let excluded = excluded.0.pubkey();
                    let (rightmost_index, rightmost) = choose_or_generate(u, &accounts)?;

                    let proof = ExclusionRightProof {
                        rightmost: tree.unchecked_inclusion_proof(
                            rightmost_index.unwrap_or_arbitrary(u)?,
                            &rightmost.0.pubkey(),
                            &rightmost.1.clone().into(),
                        ),
                        excluded,
                    };

                    let is_valid = excluded_index.is_none()
                        && rightmost.0.pubkey() < excluded
                        && !accounts.is_empty()
                        && rightmost_index == Some(accounts.len() - 1);

                    (ExclusionProof::ExclusionRight(proof), is_valid)
                }
                3 => {
                    let proof = ExclusionEmptyProof;

                    let is_valid = accounts.is_empty();

                    (ExclusionProof::ExclusionEmpty(proof), is_valid)
                }
                _ => unimplemented!(),
            };

            dbg!(&proof);
            if is_valid {
                assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
                return Ok(());
            }

            assert_ne!(proof.verify(accounts_delta_hash), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }
}
