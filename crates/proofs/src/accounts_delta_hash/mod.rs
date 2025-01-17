//! Proofs related to the accounts_delta_hash, proving whether a specific account was updated or not
//! at a particular slot.
//!
//! # Example
//!
//! ```
//! use solana_sdk::pubkey::Pubkey;
//! use solana_sdk::account::Account;
//!
//! use nitro_da_proofs::accounts_delta_hash::AccountMerkleTree;
//!
//!
//! // These would be fetched using RPC or streaming updates, here we're just creating fake ones.
//! let included_pubkey = Pubkey::new_unique();
//! let included_account = Account::new(100, 0, &Pubkey::new_unique());
//!
//! let excluded_pubkey = Pubkey::new_unique();
//!
//! // Build the merkle tree for the three accounts.
//! let mut tree = AccountMerkleTree::builder([included_pubkey, excluded_pubkey].into_iter().collect());
//! // Insertion requires the account data, but depending on the context only the hash may be kept.
//! tree.insert(included_pubkey, included_account.clone());
//! // Note: The excluded pubkey is never inserted into the tree.
//! let tree = tree.build();
//!
//! // Create the proofs.
//! let inclusion_proof = tree.prove_inclusion(included_pubkey).unwrap();
//! let exclusion_proof = tree.prove_exclusion(excluded_pubkey).unwrap();
//!
//! // This hash should match the value that validators vote on as a part of the bankhash.
//! let accounts_delta_hash = tree.root();
//!
//! // Verifying the full proof also verifies the account data contained within.
//! // It does not make and semantic verifications of the bytes, so it's recommended
//! // to deserialize and verify the account data separately.
//! assert!(inclusion_proof.verify(accounts_delta_hash));
//!
//! // Verifying exclusion proofs doesn't verify the account data.
//! assert_eq!(exclusion_proof.verify(accounts_delta_hash), Ok(()));
//! ```

mod account_merkle_tree;
pub mod exclusion;
pub mod inclusion;

#[doc(hidden)]
#[cfg(test)]
pub(crate) mod testing;

pub use account_merkle_tree::*;
