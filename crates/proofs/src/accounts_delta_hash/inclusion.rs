//! Proofs that an account **is** present in the accounts_delta_hash.

use serde::{Deserialize, Serialize};
use solana_sdk::{
    account::{Account, ReadableAccount},
    hash::{Hash, Hasher},
    pubkey::Pubkey,
};

use crate::accounts_delta_hash::account_merkle_tree::hash_account;

/// A single level of the inclusion proof, see [`crate::accounts_delta_hash::InclusionProof`].
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct InclusionProofLevel {
    /// The index of the node in the merkle tree for this level.
    pub(crate) index: usize,
    /// The hashes of all the sibling nodes for this level.
    pub(crate) siblings: Vec<Hash>,
}

/// A proof that a specific account is present in the accounts_delta_hash, and it's exact state.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct InclusionProof {
    /// The account's public key.
    pub(crate) account_pubkey: Pubkey,

    /// The path taken through the merkle tree to get from the leaf to the root.
    /// Each level contains the index of where the node came from on that level,
    /// and the hashes of all the sibling nodes.
    ///
    /// As an example for a binary tree:
    ///               root
    ///                /\
    ///               /  \
    ///              /    \
    ///             a1     a2
    ///             /\    
    ///            /  \   
    ///           /    \  
    ///          /      \   
    ///         /        \  
    ///        b1         b2
    ///       /  \         
    ///      /    \        
    ///     /      \       
    ///    c1      c2     
    ///            / \     
    ///           d1 d2
    ///
    /// Here the path from `d1` to `root` is `[(0, [d2]), (1, [c1]), (0, [b2]), (0, [a2])]`.
    ///
    /// In Solana the merkle fanout is 16, so the tree is significantly wider and each level
    /// will contain 15 siblings instead of one.
    pub(crate) levels: Vec<InclusionProofLevel>,

    pub(crate) account_data: Account,
}

impl std::fmt::Debug for InclusionProof {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InclusionProof")
            .field("account_pubkey", &self.account_pubkey)
            .field("account_data", &self.account_data)
            .field("levels", &self.levels)
            .field("root()", &self.hash())
            .finish()
    }
}

impl InclusionProof {
    /// Creates a full inclusion proof. When the proof is created this way the
    /// hash is computed and thus is automatically correct, but it should not be
    /// trusted since the struct can be modified or created through other means.
    pub fn new(
        account_pubkey: Pubkey,
        account: &impl ReadableAccount,
        levels: Vec<InclusionProofLevel>,
    ) -> Self {
        Self {
            account_pubkey,
            account_data: account.to_account_shared_data().into(),
            levels,
        }
    }

    /// Verifies that an account is present in the given accounts_delta_hash,
    /// and additionally verifies that the account contents match the given account.
    /// This can be used to prove the exact state of an account at a particular slot.
    pub fn verify(&self, accounts_delta_hash: Hash) -> bool {
        let hash = self.hash();
        hash == accounts_delta_hash
    }

    fn hash(&self) -> Hash {
        let mut current_hash = hash_account(&self.account_data, &self.account_pubkey);
        for level in &self.levels {
            let mut hasher = Hasher::default();
            // [0..current]
            for sibling in level.siblings.iter().take(level.index) {
                hasher.hash(sibling.as_ref());
            }

            // [current]
            hasher.hash(current_hash.as_ref());

            // [current+1..]
            for sibling in level.siblings.iter().skip(level.index) {
                hasher.hash(sibling.as_ref());
            }
            current_hash = hasher.result();
        }
        current_hash
    }

    /// Returns the account pubkey.
    pub fn pubkey(&self) -> &Pubkey {
        &self.account_pubkey
    }

    /// Returns the account data.
    pub fn account_data(&self) -> &Account {
        &self.account_data
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use arbtest::arbtest;
    use solana_sdk::account::Account;

    use crate::accounts_delta_hash::testing::{
        TestAccounts, UnwrapOrArbitrary, choose_or_generate, generate_accounts,
    };

    #[test]
    fn inclusion_construction() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts {
                accounts,
                accounts_delta_hash,
                tree,
            } = generate_accounts(u, BTreeSet::new(), Vec::new())?;

            let (mut index, included) = choose_or_generate(u, &accounts)?;
            if index.is_none() {
                // Even if the account is randomly generated by arbitrary, it might also be in the tree.
                index = accounts
                    .iter()
                    .position(|(keypair, _)| keypair.pubkey() == included.0.pubkey());
            }
            let old_included = included.clone();

            let included_account: Account = included.1.into();
            let proof = tree.unchecked_inclusion_proof(
                index.unwrap_or_arbitrary(u)?,
                &included.0.pubkey(),
                &included_account,
            );

            if index.is_some() && included_account.lamports > 0 {
                dbg!(index, included.0.pubkey(), &accounts, &tree, &proof);
                assert!(proof.verify(accounts_delta_hash));
                return Ok(());
            }

            dbg!(
                old_included.0.pubkey(),
                index,
                included.0.pubkey(),
                included_account.lamports,
                &accounts,
                &tree,
                &proof
            );
            assert!(
                !proof.verify(accounts_delta_hash),
                "Proof should not verify"
            );
            Ok(())
        })
        .size_max(100_000_000);
    }
}
