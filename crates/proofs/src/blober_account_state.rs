//! Proof of the state of one or many accounts in a specific Solana slot, without needing
//! to know the public key of the accounts in advance. When combined with a
//! [bank hash proof][`crate::bank_hash::BankHashProof`] (to ensure no updates were left out) it can
//! additionally prove that no account states were censored.

use std::{fmt::Debug, sync::Arc};

use anchor_lang::{AnchorDeserialize, Discriminator};
use blober::{hash_blob, merge_hashes, state::blober::Blober};
use serde::{Deserialize, Serialize};
use solana_sdk::{clock::Slot, pubkey::Pubkey};
use thiserror::Error;

use crate::debug::NoPrettyPrint;

/// An account whose state was hashed using the blober program.
///
/// The bytes should already be sliced to the exact offset and length that the
/// [`blober::instructions::FinalizeBlob`] instruction slices them to.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BlobAccount(pub Pubkey, pub Vec<u8>);

impl Debug for BlobAccount {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SourceAccount")
            .field(&self.0.to_string())
            .field(&hex::encode(&self.1))
            .finish()
    }
}

/// A proof for the state of one or many accounts in a specific Solana slot.
///
/// To create this proof, the Blober account's [`blober::blober::finalize_blob`] instruction must
/// be invoked for each blob whose state should be proven. The starting offset and length of the
/// "interesting" part of the account data that is to be hashed must also be provided.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BloberAccountStateProof {
    /// The slot that the accounts were updated in.
    slot: Slot,
    /// The bytes should already be sliced to the exact offset and length that the
    /// [`blober::instructions::FinalizeBlob`] instruction slices them to.
    pub(crate) blob_accounts: Vec<BlobAccount>,
}

impl Debug for BloberAccountStateProof {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proof")
            .field("slot", &self.slot)
            .field("source_accounts", &NoPrettyPrint(&self.blob_accounts))
            .finish()
    }
}

/// Failures that can occur when verifying a [`BloberAccountStateProof`].
#[derive(Debug, Clone, Error)]
pub enum BloberAccountStateError {
    #[error("Discriminator mismatch, wrong account type")]
    DiscriminatorMismatch,
    #[error(transparent)]
    BorshDeserialize(#[from] Arc<std::io::Error>),
    #[error("Proof is not for the correct slot")]
    SlotMismatch { expected: Slot, found: Slot },
    #[error("Digest does not match the expected value")]
    DigestMismatch { expected: String, found: String },
}

impl BloberAccountStateProof {
    /// Creates a proof for the state of the provided blob accounts at the given slot.
    pub fn new(slot: Slot, blob_accounts: Vec<BlobAccount>) -> Self {
        assert!(
            !blob_accounts.is_empty(),
            "If there are no blob accounts, there is nothing to prove"
        );
        Self {
            slot,
            blob_accounts,
        }
    }

    /// Verifies that the provided blober account data matches the expected state.
    pub fn verify(&self, blober_account_data: &[u8]) -> Result<(), BloberAccountStateError> {
        if &blober_account_data[..8] != Blober::DISCRIMINATOR {
            return Err(BloberAccountStateError::DiscriminatorMismatch);
        }

        let state = Blober::try_from_slice(&blober_account_data[8..]).map_err(Arc::new)?;

        if self.slot != state.slot {
            return Err(BloberAccountStateError::SlotMismatch {
                expected: self.slot,
                found: state.slot,
            });
        }

        let hash = merge_all_hashes(
            self.blob_accounts
                .iter()
                .map(|BlobAccount(pubkey, data)| hash_blob(&pubkey.to_bytes().into(), data)),
        );

        if hash != state.hash {
            return Err(BloberAccountStateError::DigestMismatch {
                expected: hex::encode(hash),
                found: hex::encode(state.hash),
            });
        }

        Ok(())
    }
}

fn merge_all_hashes(hashes: impl Iterator<Item = [u8; 32]>) -> [u8; 32] {
    // We recursively merge all the remaining values.
    hashes
        .reduce(|acc, hash| merge_hashes(&acc, &hash))
        .expect("account list to not be empty")
}

#[cfg(test)]
mod tests {
    use anchor_lang::AnchorSerialize;
    use arbtest::arbtest;

    use super::*;
    use crate::accounts_delta_hash::testing::ArbKeypair;

    #[test]
    fn test_merge_all_hashes() {
        arbtest(|u| {
            let hashes = [u.arbitrary()?, u.arbitrary()?, u.arbitrary()?];

            let expected = merge_hashes(&merge_hashes(&hashes[0], &hashes[1]), &hashes[2]);

            assert_eq!(merge_all_hashes(hashes.iter().cloned()), expected);

            Ok(())
        });
    }

    #[test]
    #[should_panic]
    fn empty_accounts_panics() {
        BloberAccountStateProof::new(0, Vec::new());
    }

    #[test]
    fn single_account() {
        arbtest(|u| {
            let slot = u.arbitrary()?;
            let source_account: (ArbKeypair, Vec<u8>) = u.arbitrary()?;
            let source_accounts = vec![BlobAccount(source_account.0.pubkey(), source_account.1)];

            let proof = BloberAccountStateProof::new(slot, source_accounts.clone());
            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot,
                    hash: solana_sdk::hash::hashv(&[
                        source_accounts[0].0.as_ref(),
                        &source_accounts[0].1,
                    ])
                    .to_bytes(),
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                }
                .try_to_vec()
                .unwrap(),
            ]
            .concat();

            proof.verify(&blober_account_data).unwrap();

            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn single_account_wrong_data() {
        arbtest(|u| {
            let slot = u.arbitrary()?;
            let source_account: (ArbKeypair, Vec<u8>) = u.arbitrary()?;
            let source_accounts = vec![BlobAccount(source_account.0.pubkey(), source_account.1)];

            let proof = BloberAccountStateProof::new(slot, source_accounts.clone());
            let wrong_data = u.arbitrary::<Vec<u8>>()?;
            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot,
                    hash: solana_sdk::hash::hashv(&[source_accounts[0].0.as_ref(), &wrong_data])
                        .to_bytes(),
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                }
                .try_to_vec()
                .unwrap(),
            ]
            .into_iter()
            .flatten()
            .collect();

            if wrong_data != source_accounts[0].1 {
                proof.verify(&blober_account_data).unwrap_err();
            } else {
                proof.verify(&blober_account_data).unwrap();
            }

            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn multiple_accounts() {
        arbtest(|u| {
            let slot = u.arbitrary()?;
            let blob_accounts: Vec<(ArbKeypair, Vec<u8>)> = u.arbitrary()?;
            let mut blob_accounts: Vec<_> = blob_accounts
                .into_iter()
                .map(|(keypair, bytes)| BlobAccount(keypair.pubkey(), bytes))
                .collect();

            // At least two accounts are needed for this test to make sense.
            while blob_accounts.len() < 2 {
                blob_accounts.push(BlobAccount(
                    u.arbitrary::<ArbKeypair>()?.pubkey(),
                    u.arbitrary()?,
                ));
            }
            let proof = BloberAccountStateProof::new(slot, blob_accounts.clone());

            let hash = blob_accounts
                .iter()
                .map(|BlobAccount(pubkey, bytes)| {
                    solana_sdk::hash::hashv(&[pubkey.as_ref(), bytes])
                })
                .reduce(|lhs, rhs| solana_sdk::hash::hashv(&[lhs.as_ref(), rhs.as_ref()]))
                .unwrap()
                .to_bytes();

            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot,
                    hash,
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                }
                .try_to_vec()
                .unwrap(),
            ]
            .into_iter()
            .flatten()
            .collect();

            proof.verify(&blober_account_data).unwrap();

            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn multiple_accounts_wrong_data() {
        arbtest(|u| {
            let slot = u.arbitrary()?;
            let blob_accounts: Vec<(ArbKeypair, Vec<u8>)> = u.arbitrary()?;
            let mut blob_accounts: Vec<_> = blob_accounts
                .into_iter()
                .map(|(keypair, bytes)| BlobAccount(keypair.pubkey(), bytes))
                .collect();

            // At least two accounts are needed for this test to make sense.
            while blob_accounts.len() < 2 {
                blob_accounts.push(BlobAccount(
                    u.arbitrary::<ArbKeypair>()?.pubkey(),
                    u.arbitrary()?,
                ));
            }
            let proof = BloberAccountStateProof::new(slot, blob_accounts.clone());

            let wrong_data = u.arbitrary::<Vec<u8>>()?;
            let wrong_data_index = u.choose_index(blob_accounts.len())?;
            if blob_accounts[wrong_data_index].1 == wrong_data {
                // Data wasn't changed, so the test is invalid.
                return Ok(());
            }
            blob_accounts[wrong_data_index].1 = wrong_data;

            let hash = blob_accounts
                .iter()
                .map(|BlobAccount(pubkey, bytes)| {
                    solana_sdk::hash::hashv(&[pubkey.as_ref(), bytes])
                })
                .reduce(|lhs, rhs| solana_sdk::hash::hashv(&[lhs.as_ref(), rhs.as_ref()]))
                .unwrap()
                .to_bytes();

            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot,
                    hash,
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                }
                .try_to_vec()
                .unwrap(),
            ]
            .into_iter()
            .flatten()
            .collect();

            proof.verify(&blober_account_data).unwrap_err();

            Ok(())
        })
        .size_max(100_000_000);
    }
}
