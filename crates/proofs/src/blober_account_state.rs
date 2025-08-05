//! Proof of the state of one or many accounts in a specific Solana slot, without needing
//! to know the public key of the accounts in advance. When combined with a
//! [bank hash proof][`crate::bank_hash::BankHashProof`] (to ensure no updates were left out) it can
//! additionally prove that no account states were censored.

use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use anchor_lang::{
    AnchorDeserialize, Discriminator,
    prelude::Pubkey,
    solana_program::{clock::Slot, hash::HASH_BYTES},
};
use data_anchor_blober::{U32_SIZE_BYTES, hash_blob, merge_hashes, state::blober::Blober};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{compound::ProofBlob, debug::NoPrettyPrint};

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
    #[error("Invalid state data")]
    InvalidStateData,
    #[error("Invalid blob account data: {0:?}")]
    InvalidBlobAccountData(Vec<u8>),
    #[error("Blob size mismatch at index: expected {expected}, found {found}")]
    BlobSizeMismatch { expected: usize, found: usize },
}

pub type BloberAccountStateResult<T = ()> = Result<T, BloberAccountStateError>;

/// An account whose state was hashed using the blober program.
///
/// The bytes should already be sliced to the exact offset and length that the
/// [`data_anchor_blober::instructions::FinalizeBlob`] instruction slices them to.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BlobAccount {
    pub address: Pubkey,
    pub raw_data: Vec<u8>,
}

impl BlobAccount {
    pub fn new(address: Pubkey, raw_data: Vec<u8>) -> Self {
        Self { address, raw_data }
    }

    pub fn hash_blob(&self) -> [u8; HASH_BYTES] {
        hash_blob(&self.address, &self.raw_data)
    }

    pub fn verify(
        &self,
        blob: &ProofBlob<impl AsRef<[u8]>>,
    ) -> BloberAccountStateResult<[u8; HASH_BYTES]> {
        let Some((blob_account_digest_bytes, blob_account_blob_size_bytes)) =
            self.raw_data.split_at_checked(HASH_BYTES)
        else {
            return Err(BloberAccountStateError::InvalidBlobAccountData(
                self.raw_data.clone(),
            ));
        };

        let blob_account_digest: [u8; HASH_BYTES] = blob_account_digest_bytes
            .try_into()
            .map_err(|_| BloberAccountStateError::InvalidBlobAccountData(self.raw_data.clone()))?;
        let blob_account_blob_size_bytes: [u8; U32_SIZE_BYTES as usize] =
            blob_account_blob_size_bytes.try_into().map_err(|_| {
                BloberAccountStateError::InvalidBlobAccountData(self.raw_data.clone())
            })?;

        let blob_account_blob_size = u32::from_le_bytes(blob_account_blob_size_bytes) as usize;

        if let Some(blob_size) = blob.blob_size() {
            if blob_account_blob_size != blob_size {
                return Err(BloberAccountStateError::BlobSizeMismatch {
                    expected: blob_account_blob_size,
                    found: blob_size,
                });
            }
        }

        Ok(blob_account_digest)
    }
}

impl Debug for BlobAccount {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SourceAccount")
            .field(&self.address.to_string())
            .field(&hex::encode(&self.raw_data))
            .finish()
    }
}

/// A proof for the state of one or many accounts in a specific Solana slot.
///
/// To create this proof, the Blober account's [`data_anchor_blober::blober::finalize_blob`] instruction must
/// be invoked for each blob whose state should be proven. The starting offset and length of the
/// "interesting" part of the account data that is to be hashed must also be provided.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BloberAccountStateProof {
    pub initial_hash: [u8; HASH_BYTES],
    pub initial_slot: Slot,
    pub uploads: BTreeMap<Slot, Vec<BlobAccount>>,
}

impl Debug for BloberAccountStateProof {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proof")
            .field("initial_slot", &self.initial_slot)
            .field("initial_hash", &hex::encode(self.initial_hash))
            .field("uploads", &NoPrettyPrint(&self.uploads))
            .finish()
    }
}

impl BloberAccountStateProof {
    pub fn new(
        initial_hash: [u8; HASH_BYTES],
        initial_slot: Slot,
        uploads: BTreeMap<Slot, Vec<BlobAccount>>,
    ) -> Self {
        assert!(
            uploads
                .first_key_value()
                .map(|(slot, _)| *slot > initial_slot)
                .unwrap_or(true),
            "All uploads must be in a slot after the initial slot"
        );
        Self {
            initial_hash,
            initial_slot,
            uploads,
        }
    }

    pub fn blobs(&self) -> impl Iterator<Item = &BlobAccount> {
        self.uploads.values().flat_map(|blobs| blobs.iter())
    }

    pub fn target_slot(&self) -> Slot {
        self.uploads
            .last_key_value()
            .map(|(slot, _)| *slot)
            .unwrap_or(self.initial_slot)
    }

    pub fn calculate_hash(&self) -> [u8; HASH_BYTES] {
        merge_all_hashes(
            std::iter::once(self.initial_hash).chain(self.blobs().map(|blob| blob.hash_blob())),
        )
    }

    pub fn hash_blobs(&self) -> [u8; HASH_BYTES] {
        merge_all_hashes(self.blobs().map(|blob| blob.hash_blob()))
    }

    pub fn verify(&self, blober_account_data: &[u8]) -> BloberAccountStateResult {
        if &blober_account_data[..8] != Blober::DISCRIMINATOR {
            return Err(BloberAccountStateError::DiscriminatorMismatch);
        }

        let state = Blober::try_from_slice(&blober_account_data[8..]).map_err(Arc::new)?;

        if let Some((&slot, _)) = self.uploads.last_key_value() {
            if slot != state.slot {
                return Err(BloberAccountStateError::SlotMismatch {
                    expected: slot,
                    found: state.slot,
                });
            }
        } else if state.slot != self.initial_slot {
            return Err(BloberAccountStateError::SlotMismatch {
                expected: self.initial_slot,
                found: state.slot,
            });
        }

        let hash = self.calculate_hash();

        if hash != state.hash {
            return Err(BloberAccountStateError::DigestMismatch {
                expected: hex::encode(hash),
                found: hex::encode(state.hash),
            });
        }

        Ok(())
    }
}

pub fn get_blober_hash(blober_account_data: &[u8]) -> BloberAccountStateResult<[u8; HASH_BYTES]> {
    if &blober_account_data[..8] != Blober::DISCRIMINATOR {
        return Err(BloberAccountStateError::DiscriminatorMismatch);
    }

    let state = Blober::try_from_slice(&blober_account_data[8..]).map_err(Arc::new)?;

    Ok(state.hash)
}

pub fn merge_all_hashes(hashes: impl Iterator<Item = [u8; HASH_BYTES]>) -> [u8; HASH_BYTES] {
    hashes
        .reduce(|acc, hash| merge_hashes(&acc, &hash))
        .expect("account list to not be empty")
}

#[cfg(test)]
mod tests {
    use anchor_lang::AnchorSerialize;
    use arbtest::arbtest;
    use data_anchor_blober::initial_hash;
    use solana_signer::Signer;

    use super::*;
    use crate::testing::ArbKeypair;

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
    fn blobs_before_initial_slot_panics() {
        BloberAccountStateProof::new(
            initial_hash(),
            2,
            BTreeMap::from([(1, vec![BlobAccount::new(Pubkey::default(), vec![0; 10])])]),
        );
    }

    #[test]
    fn single_account() {
        arbtest(|u| {
            let slot = u.arbitrary()?;
            let source_account: (ArbKeypair, Vec<u8>) = u.arbitrary()?;
            let source_accounts = vec![BlobAccount::new(
                source_account.0.pubkey(),
                source_account.1,
            )];

            let proof = BloberAccountStateProof::new(
                initial_hash(),
                slot,
                [(slot + 1, source_accounts.clone())].into_iter().collect(),
            );
            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot: slot + 1,
                    hash: merge_all_hashes(
                        [initial_hash(), source_accounts[0].hash_blob()].into_iter(),
                    ),
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                    namespace: u.arbitrary()?,
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
            let source_accounts = vec![BlobAccount::new(
                source_account.0.pubkey(),
                source_account.1,
            )];

            let proof = BloberAccountStateProof::new(
                initial_hash(),
                slot,
                [(slot + 1, source_accounts.clone())].into_iter().collect(),
            );
            let wrong_data = BlobAccount::new(source_account.0.pubkey(), u.arbitrary::<Vec<u8>>()?);
            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot: slot + 1,
                    hash: merge_all_hashes([initial_hash(), wrong_data.hash_blob()].into_iter()),
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                    namespace: u.arbitrary()?,
                }
                .try_to_vec()
                .unwrap(),
            ]
            .into_iter()
            .flatten()
            .collect();

            if wrong_data.raw_data != source_accounts[0].raw_data {
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
            // At least two accounts are needed for this test to make sense.
            let count = u.int_in_range(2..=1000)?;
            let blob_accounts: Vec<_> = (0..count)
                .map(|_| {
                    let keypair = u.arbitrary::<ArbKeypair>()?;
                    let bytes = u.arbitrary::<Vec<u8>>()?;
                    Ok(BlobAccount::new(keypair.pubkey(), bytes))
                })
                .collect::<Result<_, _>>()?;

            let proof = BloberAccountStateProof::new(
                initial_hash(),
                slot,
                [(slot + 1, blob_accounts.clone())].into_iter().collect(),
            );

            let hash = merge_all_hashes(
                std::iter::once(initial_hash())
                    .chain(blob_accounts.iter().map(|blob| blob.hash_blob())),
            );

            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot: slot + 1,
                    hash,
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                    namespace: u.arbitrary()?,
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
            // At least two accounts are needed for this test to make sense.
            let count = u.int_in_range(2..=1000)?;
            let mut blob_accounts: Vec<_> = (0..count)
                .map(|_| {
                    let keypair = u.arbitrary::<ArbKeypair>()?;
                    let bytes = u.arbitrary::<Vec<u8>>()?;
                    Ok(BlobAccount::new(keypair.pubkey(), bytes))
                })
                .collect::<Result<_, _>>()?;

            let proof = BloberAccountStateProof::new(
                initial_hash(),
                slot,
                [(slot + 1, blob_accounts.clone())].into_iter().collect(),
            );

            let wrong_data = u.arbitrary::<Vec<u8>>()?;
            let wrong_data_index = u.choose_index(blob_accounts.len())?;
            if blob_accounts[wrong_data_index].raw_data == wrong_data {
                // Data wasn't changed, so the test is invalid.
                return Ok(());
            }
            blob_accounts[wrong_data_index].raw_data = wrong_data;

            let hash = merge_all_hashes(
                std::iter::once(initial_hash())
                    .chain(blob_accounts.iter().map(|blob| blob.hash_blob())),
            );

            let blober_account_data: Vec<u8> = [
                Blober::DISCRIMINATOR.to_vec(),
                Blober {
                    slot: slot + 1,
                    hash,
                    caller: u.arbitrary::<ArbKeypair>()?.pubkey().to_bytes().into(),
                    namespace: u.arbitrary()?,
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
