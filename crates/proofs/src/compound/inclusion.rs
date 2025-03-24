//! This proof module contains the logic for verifying "inclusion" in the sense that a specific
//! Solana block contains blobs, and that there are no other blobs in the block.

use std::fmt::Debug;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use solana_sdk::{clock::Slot, hash::HASH_BYTES, pubkey::Pubkey};
use thiserror::Error;

use crate::{
    accounts_delta_hash::inclusion::InclusionProof,
    bank_hash::BankHashProof,
    blob::{BlobProof, BlobProofError},
    blober_account_state::{self, BloberAccountStateProof},
};

/// A proof that a specific Solana block contains blobs, and that there are no other blobs in the block.
///
/// This proof consists of four parts:
/// 1. A list of [blob proofs][`BlobProof`] that prove that the blobs uploaded to the [`blober`] program
///    hash to the given blob digest.
/// 2. A [blober account state proof][`BloberAccountStateProof`] that proves that the [`blober`] was
///    invoked exactly as many times as there are blobs.
/// 3. An [accounts delta hash proof][`InclusionProof`] that proves that
///    the accounts_delta_hash *does* include the [`blober`] account.
/// 4. A [bank hash proof][`BankHashProof`] that proves that the root hash of the accounts_delta_hash
///    is the same as the root in the bank hash.
///
/// The proof can then be verified by supplying the blockhash of the block in which the [`blober`] was
/// invoked, as well as the blobs of data which were published.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CompoundInclusionProof {
    slot: Slot,
    blob_proofs: Vec<BlobProof>,
    blober_account_state_proof: BloberAccountStateProof,
    blober_inclusion_proof: InclusionProof,
    pub bank_hash_proof: BankHashProof,
}

/// All data relevant for proving a single blob. If the `chunks` field is `None`, the blob itself will
/// not be checked, but the rest of the proof will still be verified.
pub struct ProofBlob<A: AsRef<[u8]> = Vec<u8>> {
    pub blob: Pubkey,
    pub data: Option<A>,
}

impl ProofBlob<Vec<u8>> {
    pub fn empty(blob: Pubkey) -> Self {
        Self { blob, data: None }
    }
}

impl<A: AsRef<[u8]>> ProofBlob<A> {
    pub fn blob_size(&self) -> Option<usize> {
        let blob = self.data.as_ref()?;
        Some(blob.as_ref().len())
    }
}

impl<A: AsRef<[u8]>> Debug for ProofBlob<A> {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob")
            .field("blob", &self.blob)
            .field("blob_size", &self.blob_size())
            .finish()
    }
}

/// Failures that can occur when verifying a [`CompoundInclusionProof`].
#[derive(Debug, Clone, Error)]
pub enum CompoundInclusionProofError {
    #[error("The number of blobs does not match the number of proofs")]
    InvalidNumberOfBlobs,
    #[error(
        "The number of blob accounts does not match the number of proofs, some blobs are missing"
    )]
    MissingBlobs,
    #[error("The inclusion proof is not for the blober account")]
    IncludedAccountNotBlober,
    #[error("The proof is for a different blockhash than the one provided, expected {expected:?}, found {found:?}")]
    BlockHashMismatch {
        expected: solana_sdk::hash::Hash,
        found: solana_sdk::hash::Hash,
    },
    #[error(
        "Blob {index} does not match the provided hash, expected {expected:?}, found {found:?}"
    )]
    BlobHashMismatch {
        index: usize,
        expected: solana_sdk::hash::Hash,
        found: solana_sdk::hash::Hash,
    },
    #[error(
        "Blob {index} does not match the provided blob size, expected {expected}, found {found}"
    )]
    BlobSizeMismatch {
        index: usize,
        expected: usize,
        found: usize,
    },
    #[error("Blob {index} has invalid blob account data: 0x{}", hex::encode(.bytes))]
    InvalidBlobAccountData { index: usize, bytes: Vec<u8> },
    #[error("The computed accounts delta hash does not match the provided value")]
    AccountsDeltaHashMismatch,
    #[error(transparent)]
    BloberAccountState(#[from] blober_account_state::BloberAccountStateError),
    #[error(transparent)]
    Blob(#[from] BlobProofError),
}

impl CompoundInclusionProof {
    /// Creates an inclusion proof.
    pub fn new(
        slot: Slot,
        blob_proofs: Vec<BlobProof>,
        blober_account_state_proof: BloberAccountStateProof,
        blober_inclusion_proof: InclusionProof,
        bank_hash_proof: BankHashProof,
    ) -> Self {
        Self {
            slot,
            blob_proofs,
            blober_account_state_proof,
            blober_inclusion_proof,
            bank_hash_proof,
        }
    }

    /// Verifies that a specific Solana block contains the provided blobs, and that no blobs have been excluded.
    #[tracing::instrument(skip_all, err(Debug), fields(slot = %self.slot, blober = %blober, blockhash = %blockhash))]
    pub fn verify(
        &self,
        blober: Pubkey,
        blockhash: solana_sdk::hash::Hash,
        blobs: &[ProofBlob<impl AsRef<[u8]>>],
    ) -> Result<(), CompoundInclusionProofError> {
        if blobs.len() != self.blob_proofs.len() {
            return Err(CompoundInclusionProofError::InvalidNumberOfBlobs);
        }
        if self.blober_account_state_proof.blob_accounts.len() != self.blob_proofs.len() {
            return Err(CompoundInclusionProofError::MissingBlobs);
        }
        if self.blober_inclusion_proof.account_pubkey != blober {
            return Err(CompoundInclusionProofError::IncludedAccountNotBlober);
        }

        if self.bank_hash_proof.blockhash != blockhash {
            return Err(CompoundInclusionProofError::BlockHashMismatch {
                expected: blockhash,
                found: self.bank_hash_proof.blockhash,
            });
        }

        let blob_accounts = &self.blober_account_state_proof.blob_accounts;

        for (index, ((blob, blob_proof), blob_account)) in blobs
            .iter()
            .zip_eq(&self.blob_proofs)
            .zip_eq(blob_accounts)
            .enumerate()
        {
            let (blob_account_digest, blob_account_blob_size) = blob_account.1.split_at(HASH_BYTES);
            let blob_account_digest: [u8; 32] = blob_account_digest.try_into().map_err(|_| {
                CompoundInclusionProofError::InvalidBlobAccountData {
                    index,
                    bytes: blob_account.1.clone(),
                }
            })?;
            let blob_account_blob_size: [u8; 4] =
                blob_account_blob_size.try_into().map_err(|_| {
                    CompoundInclusionProofError::InvalidBlobAccountData {
                        index,
                        bytes: blob_account.1.clone(),
                    }
                })?;
            let blob_account_blob_size = u32::from_le_bytes(blob_account_blob_size) as usize;

            if let Some(blob_size) = blob.blob_size() {
                if blob_account_blob_size != blob_size {
                    return Err(CompoundInclusionProofError::BlobSizeMismatch {
                        index,
                        expected: blob_account_blob_size,
                        found: blob_size,
                    });
                }
            }

            if blob_account_digest != blob_proof.digest {
                return Err(CompoundInclusionProofError::BlobHashMismatch {
                    index,
                    expected: solana_sdk::hash::Hash::new_from_array(blob_proof.digest),
                    found: solana_sdk::hash::Hash::new_from_array(blob_account_digest),
                });
            }

            if let Some(data) = &blob.data {
                blob_proof.verify(data.as_ref())?;
            }
        }

        self.blober_account_state_proof
            .verify(&self.blober_inclusion_proof.account_data.data)?;

        if !self
            .blober_inclusion_proof
            .verify(self.bank_hash_proof.accounts_delta_hash)
        {
            return Err(CompoundInclusionProofError::AccountsDeltaHashMismatch);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use anchor_lang::{AnchorSerialize, Discriminator};
    use arbtest::arbtest;
    use blober::{
        hash_blob, initial_hash,
        state::{blob::Blob, blober::Blober},
        BLOB_DATA_END, BLOB_DATA_START, CHUNK_SIZE,
    };
    use blober_account_state::BlobAccount;
    use solana_sdk::{
        account::Account, native_token::LAMPORTS_PER_SOL, slot_hashes::SlotHashes, system_program,
        sysvar, sysvar::SysvarId,
    };

    use super::*;
    use crate::{
        accounts_delta_hash::{
            testing::{ArbAccount, ArbKeypair},
            AccountMerkleTree,
        },
        bank_hash::BankHashProof,
        testing::arbitrary_hash,
    };

    #[test]
    fn inclusion_construction_single_blob() {
        arbtest(|u| {
            // ------------------------- Blob -------------------------
            let blob: &[u8] = u.arbitrary()?;
            if blob.is_empty() {
                // Empty blob, invalid test.
                return Ok(());
            } else if blob.len() > u16::MAX as usize {
                // Blob too large, invalid test.
                return Ok(());
            }
            let mut chunks = blob
                .chunks(CHUNK_SIZE as usize)
                .enumerate()
                .map(|(i, chunk)| (i as u16, chunk))
                .collect::<Vec<_>>();
            // Swap a few chunks around to simulate out-of-order submission.
            for _ in 0..10 {
                let a = u.choose_index(chunks.len())?;
                let b = u.choose_index(chunks.len())?;
                chunks.swap(a, b);
            }

            let blober = u.arbitrary::<ArbKeypair>()?.pubkey();

            let mut unmodified = true;

            let mut blob_account: (ArbKeypair, ArbAccount) = u.arbitrary()?;

            blob_account.1.data = if u.ratio(1, 10)? {
                unmodified = false;
                u.arbitrary::<[u8; BLOB_DATA_END]>()?.to_vec()
            } else {
                let mut blob_pda = Blob::new(0, 0, blob.len() as u32, 0);
                for (chunk_index, chunk_data) in &chunks {
                    blob_pda.insert(0, *chunk_index, chunk_data);
                }
                [Blob::DISCRIMINATOR.to_vec(), blob_pda.try_to_vec().unwrap()]
                    .into_iter()
                    .flatten()
                    .collect()
            };

            let blob_proof = BlobProof::new(&chunks);

            // ------------------------- Blober account state -------------------------
            let mut slot = u.arbitrary()?;
            if slot == 0 {
                // Slot 0 doesn't work for the contract and will never happen outside of tests.
                slot = 1;
            }
            let mut source_accounts: Vec<_> = vec![BlobAccount(
                blob_account.0.pubkey(),
                blob_account.1.data[BLOB_DATA_START..BLOB_DATA_END].to_vec(),
            )];

            if u.ratio(1, 10)? {
                // Add an extra source account that hasn't actually called the blober, I.E. false proof.
                source_accounts.push(BlobAccount(
                    u.arbitrary::<ArbKeypair>()?.pubkey(),
                    u.arbitrary()?,
                ));
                unmodified = false;
            }

            let blober_account_state_proof =
                blober_account_state::BloberAccountStateProof::new(slot, source_accounts.clone());

            // Accounts delta hash, starting with unrelated accounts.
            let other_accounts: Vec<(ArbKeypair, ArbAccount)> = u.arbitrary()?;

            let mut tree = AccountMerkleTree::builder(
                [blober, sysvar::slot_hashes::ID]
                    .into_iter()
                    .chain(other_accounts.iter().map(|(kp, _)| kp.pubkey()))
                    .collect(),
            );
            for (pubkey, account) in other_accounts.iter() {
                tree.insert(pubkey.pubkey(), account.clone().into());
            }
            // Always include the blober account.
            let mut blober_data = Blober {
                caller: blober::ID,
                hash: initial_hash(),
                slot: 0,
            };
            if u.ratio(1, 10)? {
                let new_slot = u.arbitrary()?;
                if new_slot != 0 {
                    unmodified = new_slot == slot;
                    slot = new_slot;
                }
            }

            if u.ratio(9, 10)? {
                blober_data.store_hash(
                    &hash_blob(
                        &blob_account.0.pubkey().to_bytes().into(),
                        &blob_account.1.data[BLOB_DATA_START..BLOB_DATA_END],
                    ),
                    slot,
                );
            } else {
                // The blober account was not invoked.
                unmodified = false;
            }
            let blober_account = Account {
                lamports: LAMPORTS_PER_SOL,
                data: [
                    Blober::DISCRIMINATOR.to_vec(),
                    blober_data.try_to_vec().unwrap(),
                ]
                .into_iter()
                .flatten()
                .collect(),
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            };

            let (tree, accounts_delta_hash_proof) =
                if !other_accounts.is_empty() && u.ratio(1, 10)? {
                    // The blober is never inserted into the tree.
                    let tree = tree.build();
                    let false_accounts_delta_hash_proof = tree.unchecked_inclusion_proof(
                        u.choose_index(other_accounts.len())?,
                        &blober,
                        &blober_account,
                    );
                    unmodified = false;
                    (tree, false_accounts_delta_hash_proof)
                } else if !other_accounts.is_empty() && u.ratio(1, 10)? {
                    // Valid inclusion proof but for the wrong account.
                    let keypair = &u.choose(&other_accounts)?.0;
                    let tree = tree.build();
                    let accounts_delta_hash_proof = tree.prove_inclusion(keypair.pubkey()).unwrap();
                    unmodified = keypair.pubkey() == blober;
                    (tree, accounts_delta_hash_proof)
                } else {
                    tree.insert(blober, blober_account);
                    let tree = tree.build();
                    let accounts_delta_hash_proof = tree.prove_inclusion(blober).unwrap();
                    (tree, accounts_delta_hash_proof)
                };

            // ----------------------- Payer proof -----------------------------------------
            let writable_blob_account = blob_account.0.pubkey();
            let read_only_blober_account = blober::ID.to_bytes().into();

            // ------------------------- Bank hash -------------------------
            let parent_bankhash = arbitrary_hash(u)?;
            let root = tree.root();
            let signature_count = u.arbitrary()?;
            let blockhash = arbitrary_hash(u)?;

            let mut bank_hash_proof =
                BankHashProof::new(parent_bankhash, root, signature_count, blockhash);

            if u.ratio(1, 10)? {
                // Not testing exhaustively here, just that anything is wrong with the bank hash proof.
                let new_root = arbitrary_hash(u)?;
                unmodified = new_root == root;
                bank_hash_proof.accounts_delta_hash = new_root;
            }

            // ------------------------- Multi vote proof -------------------------
            let mut trusted_vote_authorities: Vec<ArbKeypair> = vec![
                arbitrary::Arbitrary::arbitrary(u)?,
                arbitrary::Arbitrary::arbitrary(u)?,
            ];
            trusted_vote_authorities.sort_by_key(|pk| pk.pubkey());

            let required_votes = 1 + u.choose_index(trusted_vote_authorities.len())?;

            unmodified = unmodified
                && required_votes <= trusted_vote_authorities.len()
                && required_votes > 0;

            let proven_slot = u.arbitrary()?;
            let proven_hash = bank_hash_proof.hash();

            let slot_hashes = u
                .arbitrary_iter::<(u64, [u8; 32])>()?
                .map(|tup| Ok((tup?.0, solana_sdk::hash::Hash::new_from_array(tup?.1))))
                // Include the hash that's being proven.
                .chain([Ok((proven_slot, proven_hash))].into_iter())
                .collect::<Result<HashSet<_>, _>>()?
                .into_iter()
                .collect::<Vec<_>>();
            if slot_hashes.is_empty() {
                return Ok(());
            }

            let slot_hashes = SlotHashes::new(&slot_hashes);

            let mut slot_hashes_account: Account = u.arbitrary::<ArbAccount>()?.into();
            slot_hashes_account.data = bincode::serialize(&slot_hashes).unwrap();

            let mut slot_hashes_tree =
                AccountMerkleTree::builder([read_only_blober_account].into_iter().collect());
            slot_hashes_tree.insert(SlotHashes::id(), slot_hashes_account);

            // ------------------------- Compound proof -------------------------
            let blob_proofs = if u.ratio(1, 10)? {
                // Missing blob proof.
                unmodified = false;
                Vec::new()
            } else if u.ratio(1, 10)? {
                // Extra blob proof.
                unmodified = false;
                vec![blob_proof.clone(), blob_proof]
            } else {
                vec![blob_proof]
            };

            let compound_inclusion_proof = CompoundInclusionProof::new(
                proven_slot,
                blob_proofs,
                blober_account_state_proof,
                accounts_delta_hash_proof,
                bank_hash_proof,
            );

            let blobs = if u.ratio(1, 10)? {
                // No blobs.
                unmodified = false;
                Vec::new()
            } else if u.ratio(1, 10)? {
                // An extra blob.
                unmodified = false;
                vec![blob.to_vec(), blob.to_vec()]
            } else if u.ratio(1, 10)? {
                // A single blob, the right size, but the wrong contents.
                let mut new_blob = Vec::new();
                while new_blob.len() < blob.len() {
                    new_blob.push(u.arbitrary()?);
                }
                unmodified = unmodified && new_blob == blob;
                vec![new_blob]
            } else if u.ratio(1, 10)? {
                // A single blob, but the wrong size.
                let mut new_blob = Vec::new();
                while new_blob.len() == blob.len() {
                    new_blob = u.arbitrary()?;
                }
                unmodified = unmodified && new_blob == blob;
                vec![new_blob]
            } else {
                vec![blob.to_vec()]
            };

            let blobs = blobs
                .into_iter()
                .map(|data| ProofBlob {
                    blob: writable_blob_account,
                    data: Some(data),
                })
                .collect::<Vec<_>>();

            if unmodified {
                dbg!(&compound_inclusion_proof);
                compound_inclusion_proof
                    .verify(
                        blober,
                        // In real code this value wouldn't come from the proof itself,
                        // instead it would be sourced from a third-party Solana node.
                        bank_hash_proof.blockhash,
                        &blobs,
                    )
                    .unwrap();
                // It should also be possible to verify the proof without the blob data.
                let empty_blobs: Vec<_> = blobs
                    .into_iter()
                    .map(|b| ProofBlob::empty(b.blob))
                    .collect();
                compound_inclusion_proof
                    .verify(blober, bank_hash_proof.blockhash, &empty_blobs)
                    .unwrap();
                roundtrip_serialization(compound_inclusion_proof);
            } else {
                compound_inclusion_proof
                    .verify(blober, bank_hash_proof.blockhash, &blobs)
                    .unwrap_err();
                roundtrip_serialization(compound_inclusion_proof);
            }

            Ok(())
        })
        .size_max(100_000_000);
    }

    fn roundtrip_serialization(proof: CompoundInclusionProof) {
        let serialized_json = serde_json::to_string(&proof).unwrap();
        let deserialized_json: CompoundInclusionProof =
            serde_json::from_str(&serialized_json).unwrap();
        assert_eq!(proof, deserialized_json);

        let serialized_bincode = bincode::serialize(&proof).unwrap();
        let deserialized_bincode: CompoundInclusionProof =
            bincode::deserialize(&serialized_bincode).unwrap();
        assert_eq!(proof, deserialized_bincode);

        let serialized_risc0_zkvm = risc0_zkvm::serde::to_vec(&proof).unwrap();
        let deserialized_risc0_zkvm: CompoundInclusionProof =
            risc0_zkvm::serde::from_slice(&serialized_risc0_zkvm).unwrap();
        assert_eq!(proof, deserialized_risc0_zkvm);
    }
}
