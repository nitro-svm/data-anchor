use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    ops::Bound,
};

use solana_sdk::{account::Account, pubkey::Pubkey};

use crate::accounts_delta_hash::account_merkle_tree::{
    Leaf, solana_accounts_db::hash_account, tree::AccountMerkleTree,
};

/// Builder for constructing an [`AccountMerkleTree`].
///
/// Insert leaves using [`AccountMerkleTreeBuilder::insert`].
#[derive(Debug, PartialEq, Default, Clone)]
pub struct AccountMerkleTreeBuilder {
    leaves: BTreeMap<Pubkey, Leaf>,
    important_pubkeys: BTreeSet<Pubkey>,
}

impl AccountMerkleTreeBuilder {
    /// Creates a new builder for constructing an [`AccountMerkleTree`].
    pub fn new(important_pubkeys: BTreeSet<Pubkey>) -> Self {
        Self {
            leaves: BTreeMap::new(),
            important_pubkeys,
        }
    }

    #[doc(hidden)]
    #[cfg(test)]
    pub(crate) fn insert_unchecked(&mut self, pubkey: Pubkey, leaf: Leaf) {
        self.leaves.insert(pubkey, leaf);
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

    /// Inserts an account into the merkle tree.
    pub fn insert(&mut self, pubkey: Pubkey, account: Account) {
        if self.would_be_important_or_neighbour_of_important(&pubkey) {
            // Get the previous left and right neighbours of the pubkey to potentially minimize
            // after insert.
            let previous_left = self.get_left_neighbour(&pubkey).map(|(pk, _)| *pk);
            let previous_right = self.get_right_neighbour(&pubkey).map(|(pk, _)| *pk);

            self.leaves.insert(pubkey, Leaf::Full(account));

            previous_left.inspect(|pk| self.replace_with_hash_if_unimportant(pk));
            previous_right.inspect(|pk| self.replace_with_hash_if_unimportant(pk));
        } else {
            self.leaves
                .insert(pubkey, Leaf::Partial(hash_account(&account, &pubkey)));
        }
    }

    /// When account accumulation is complete, build the merkle tree. This makes the tree immutable,
    /// and allows for proof construction.
    pub fn build(self) -> AccountMerkleTree {
        self.leaves.into()
    }

    fn would_be_important_or_neighbour_of_important(&self, new_pubkey: &Pubkey) -> bool {
        if self.important_pubkeys.contains(new_pubkey) {
            return true;
        }

        if let Some(Leaf::Full(_)) = self.leaves.get(new_pubkey) {
            // The pubkey is already present as full, so keep it that way.
            // If this is not checked the logic below fails and downgrades it to a partial every time.
            return true;
        }

        for important in &self.important_pubkeys {
            match new_pubkey.cmp(important) {
                Ordering::Equal => unreachable!("The new pubkey is already checked for equality"),
                Ordering::Less => {
                    if let Some((left_pubkey, _)) = self.get_left_neighbour(important) {
                        if left_pubkey < new_pubkey {
                            // The new pubkey is the new left neighbour of this important pubkey.
                            return true;
                        }
                    } else {
                        // There is no left neighbour, so the new pubkey is the new left neighbour of this important pubkey.
                        return true;
                    }
                }
                Ordering::Greater => {
                    if let Some((right_pubkey, _)) = self.get_right_neighbour(important) {
                        if right_pubkey > new_pubkey {
                            // The new pubkey is the new right neighbour of this important pubkey.
                            return true;
                        }
                    } else {
                        // There is no right neighbour, so the new pubkey is the new right neighbour of this important pubkey.
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Returns true if the pubkey is important or a neighbour of an important pubkey. We keep
    /// neighbours of important pubkeys as full leaves to make proofs more efficient.
    fn is_important_or_neighbour_of_important(&self, pubkey: &Pubkey) -> bool {
        if self.important_pubkeys.contains(pubkey) {
            return true;
        }

        assert!(
            self.leaves.contains_key(pubkey),
            "Account should already by in leaves"
        );

        let left = self.get_left_neighbour(pubkey);

        let is_right_of_important = match left {
            Some((left_pubkey, _)) => {
                // If there is an important pubkey which is not yet in the leaves but is between
                // the rightmost leaf and the pubkey, the pubkey is a neighbour of an important pubkey.
                self.important_pubkeys.contains(left_pubkey)
                    || self
                        .important_pubkeys
                        .iter()
                        .any(|important| left_pubkey < important && important < pubkey)
            }
            // If there are no leaves left of the pubkey, but there is an important pubkey left of
            // the given pubkey, the pubkey is a neighbour of an important pubkey.
            None => self
                .important_pubkeys
                .iter()
                .any(|important| important < pubkey),
        };

        if is_right_of_important {
            return true;
        }

        let right = self.get_right_neighbour(pubkey);

        let is_left_of_important = match right {
            Some((right_pubkey, _)) => {
                // If there is an important pubkey which is not yet in the leaves but is between
                // the leftmost leaf and the bpubkey, the pubkey is a neighbour of an important pubkey.
                self.important_pubkeys.contains(right_pubkey)
                    || self
                        .important_pubkeys
                        .iter()
                        .any(|important| pubkey < important && important < right_pubkey)
            }
            // If there are no leaves right of the pubkey, but there is an important pubkey right
            // of the given pubkey, the pubkey is a neighbour of an important pubkey.
            None => self
                .important_pubkeys
                .iter()
                .any(|important| pubkey < important),
        };

        is_left_of_important
    }

    fn replace_with_hash_if_unimportant(&mut self, pubkey: &Pubkey) {
        if self.is_important_or_neighbour_of_important(pubkey) {
            return;
        }

        self.leaves.entry(*pubkey).and_modify(|leaf| {
            if let Leaf::Full(account) = leaf {
                *leaf = Leaf::Partial(hash_account(&*account, pubkey));
            }
        });
    }
}
