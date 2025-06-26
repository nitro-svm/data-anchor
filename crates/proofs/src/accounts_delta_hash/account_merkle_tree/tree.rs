use std::{
    cmp::min,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    ops::Bound,
};

use itertools::Itertools;
use solana_sdk::{account::Account, pubkey::Pubkey};

use crate::accounts_delta_hash::{
    account_merkle_tree::{
        Leaf, builder::AccountMerkleTreeBuilder, hash_tree, solana_accounts_db::MERKLE_FANOUT,
    },
    exclusion::{
        ExclusionProof, empty::ExclusionEmptyProof, inner::ExclusionInnerProof,
        left::ExclusionLeftProof, right::ExclusionRightProof,
    },
    inclusion::{InclusionProof, InclusionProofLevel},
};

/// Either an inclusion proof or an exclusion proof. See [`InclusionProof`] and [`ExclusionProof`] for more information.
pub enum AccountsDeltaHashProof {
    /// See [`InclusionProof`].
    Inclusion(InclusionProof),
    /// See [`ExclusionProof`].
    Exclusion(ExclusionProof),
    /// The account for which a proof was requested was not marked as important and there is not
    /// enough data to construct the proof.
    AccountNotImportant,
}

/// Represents an immutable merkle tree of Solana accounts changed in a single block.
#[derive(Clone, PartialEq)]
pub struct AccountMerkleTree {
    tree: Vec<Vec<solana_sdk::hash::Hash>>,
    leaves: BTreeMap<Pubkey, Leaf>,
}

impl Debug for AccountMerkleTree {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountMerkleTree")
            .field("root()", &self.root().to_string())
            .field("tree", &self.tree)
            .field("leaves", &self.leaves)
            .finish()
    }
}

impl From<BTreeMap<Pubkey, Leaf>> for AccountMerkleTree {
    fn from(leaves: BTreeMap<Pubkey, Leaf>) -> Self {
        Self {
            // Hash all the accounts individually to get the leaves of the tree.
            tree: hash_tree(
                leaves
                    .iter()
                    .map(|(pubkey, leaf)| leaf.hash(pubkey))
                    .collect(),
            ),
            leaves,
        }
    }
}

impl AccountMerkleTree {
    /// Creates a new builder for constructing an [`AccountMerkleTree`].
    // Mutation testing this just inserts a default value... Which it already is.
    #[cfg_attr(test, mutants::skip)]
    pub fn builder(important_pubkeys: BTreeSet<Pubkey>) -> AccountMerkleTreeBuilder {
        AccountMerkleTreeBuilder::new(important_pubkeys)
    }

    /// Returns the root hash of the merkle tree.
    pub fn root(&self) -> solana_sdk::hash::Hash {
        *self
            .tree
            .last()
            .expect("tree should have at least one level")
            .first()
            .expect("last level should have exactly one hash")
    }

    /// Retrieves the data of a specific account. Returns None if the account is not present in the tree.
    pub fn get_account(&self, pubkey: Pubkey) -> Option<&Account> {
        match self.leaves.get(&pubkey) {
            Some(Leaf::Full(account)) => Some(account),
            _ => None,
        }
    }

    /// Get the rightmost leaf left of the given pubkey.
    fn get_left_neighbour(&self, pubkey: &Pubkey) -> Option<(&Pubkey, &Leaf)> {
        self.leaves
            .range((Bound::Unbounded, Bound::Excluded(*pubkey)))
            .next_back()
    }

    /// Get the leftmost leaf right of the given pubkey.
    fn get_right_neighbour(&self, pubkey: &Pubkey) -> Option<(&Pubkey, &Leaf)> {
        self.leaves
            .range((Bound::Excluded(*pubkey), Bound::Unbounded))
            .next()
    }

    /// Proves a specific account's presence or absence in the tree.
    pub fn prove(&self, account_pubkey: Pubkey) -> AccountsDeltaHashProof {
        if let Some(proof) = self.prove_exclusion(account_pubkey) {
            return AccountsDeltaHashProof::Exclusion(proof);
        }

        if let Some(proof) = self.prove_inclusion(account_pubkey) {
            return AccountsDeltaHashProof::Inclusion(proof);
        }

        AccountsDeltaHashProof::AccountNotImportant
    }

    /// Proves that an account is present in the merkle tree, and the exact state of the account data.
    /// Requires that the full account data is present in the tree, otherwise it will return `None`.
    /// Will also return `None` if the account is not present in the tree at all.
    pub fn prove_inclusion(&self, included: Pubkey) -> Option<InclusionProof> {
        let (index, (_, leaf)) = self
            .leaves
            .iter()
            .find_position(|(pubkey, _)| pubkey == &&included)?;

        if let Leaf::Full(account) = leaf {
            let levels = self.calculate_levels_for_inclusion(index);
            Some(InclusionProof::new(included, account, levels))
        } else {
            None
        }
    }

    /// Proves that an account is not present in the merkle tree.
    /// Will return `None` if the account is present in the tree.
    pub fn prove_exclusion(&self, excluded: Pubkey) -> Option<ExclusionProof> {
        if self.leaves.contains_key(&excluded) {
            return None;
        }

        // Exclusion empty
        if self.leaves.is_empty() {
            return Some(ExclusionProof::ExclusionEmpty(ExclusionEmptyProof));
        }

        // Exclusion left
        let (leftmost_pubkey, _) = self
            .leaves
            .first_key_value()
            .expect("leaves to not be empty");
        if &excluded < leftmost_pubkey {
            return Some(ExclusionProof::ExclusionLeft(ExclusionLeftProof {
                excluded,
                leftmost: self.prove_inclusion(*leftmost_pubkey)?,
            }));
        }

        // Exclusion right
        let (rightmost_pubkey, _) = self
            .leaves
            .last_key_value()
            .expect("leaves to not be empty");
        if &excluded > rightmost_pubkey {
            return Some(ExclusionProof::ExclusionRight(ExclusionRightProof {
                rightmost: self.prove_inclusion(*rightmost_pubkey)?,
                excluded,
            }));
        }

        // Exclusion inner
        let (left_pubkey, _) = self
            .get_left_neighbour(&excluded)
            .expect("left leaf pubkey to exist");
        let (right_pubkey, _) = self
            .get_right_neighbour(&excluded)
            .expect("right leaf pubkey to exist");

        Some(ExclusionProof::ExclusionInner(ExclusionInnerProof {
            left: self
                .prove_inclusion(*left_pubkey)
                .expect("left leaf to exist"),
            excluded,
            right: self
                .prove_inclusion(*right_pubkey)
                .expect("right leaf to exist"),
        }))
    }

    #[doc(hidden)]
    #[cfg(test)]
    pub(crate) fn unchecked_inclusion_proof(
        &self,
        index: usize,
        included: &Pubkey,
        account: &Account,
    ) -> InclusionProof {
        InclusionProof::new(
            *included,
            account,
            self.calculate_levels_for_inclusion(index),
        )
    }

    #[doc(hidden)]
    #[cfg(test)]
    pub(crate) fn leaves(&self) -> &BTreeMap<Pubkey, Leaf> {
        &self.leaves
    }

    pub(crate) fn calculate_levels_for_inclusion(
        &self,
        mut index: usize,
    ) -> Vec<InclusionProofLevel> {
        let mut levels = Vec::new();

        // Skip root hash since it is not needed for inclusion proof.
        for level in self.tree.iter().take(self.tree.len() - 1) {
            // Get the starting index of the group of leaves which are hashed at the lowest level
            // to form the first hash of the next level.
            let start_index = (index / MERKLE_FANOUT) * MERKLE_FANOUT;
            // Get all the siblings' hashes of the leaf at the given index. Could be less than
            // MERKLE_FANOUT.
            let siblings = (start_index..min(start_index + MERKLE_FANOUT, level.len()))
                // Skip the leaf itself.
                .filter(|i| i != &index)
                .map(|i| level[i])
                .collect();
            levels.push(InclusionProofLevel {
                index: index - start_index,
                siblings,
            });
            // On the next level we will look at the index divided by the MERKLE_FANOUT.
            index /= MERKLE_FANOUT;
        }

        levels
    }
}
