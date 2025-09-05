//! This proof module contains the logic for verifying "inclusion" in the sense that a specific
//! Solana block contains blobs, and that there are no other blobs in the block.

use std::fmt::Debug;

use anchor_lang::{
    prelude::Pubkey,
    solana_program::hash::{HASH_BYTES, Hash},
};
use data_anchor_blober::hash_blob;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    blob::{BlobProof, BlobProofError},
    blober_account_state::{
        self, BloberAccountStateError, BloberAccountStateProof, BloberAccountStateResult,
        get_blober_hash, merge_all_hashes,
    },
};

/// A proof that a specific Solana block contains blobs, and that there are no other blobs in the block.
///
/// This proof consists of four parts:
/// 1. A list of [blob proofs][`BlobProof`] that prove that the blobs uploaded to the [`blober`] program
///    hash to the given blob digest.
/// 2. The public key of the [`blober`] PDA that was invoked to commite the blobs to.
/// 3. A [blober account state proof][`BloberAccountStateProof`] that proves that the [`blober`] was
///    invoked exactly as many times as there are blobs.
///
/// The proof can then be verified by supplying the blockhash of the block in which the [`blober`] was
/// invoked, as well as the blobs of data which were published.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CompoundInclusionProof {
    pub blob_proofs: Vec<BlobProof>,
    pub blober_pubkey: Pubkey,
    pub blober_account_state_proof: BloberAccountStateProof,
}

/// All data relevant for proving a single blob. If the `chunks` field is `None`, the blob itself will
/// not be checked, but the rest of the proof will still be verified.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofBlob<A: AsRef<[u8]> = Vec<u8>> {
    pub blob: Pubkey,
    pub data: Option<A>,
}

impl ProofBlob<Vec<u8>> {
    pub fn empty(blob: Pubkey) -> Self {
        Self { blob, data: None }
    }

    pub fn hash_blob(&self) -> [u8; HASH_BYTES] {
        hash_blob(&self.blob, self.data.as_ref().map_or(&[], AsRef::as_ref))
    }
}

impl<A: AsRef<[u8]>> ProofBlob<A> {
    pub fn blob_size(&self) -> Option<usize> {
        let blob = self.data.as_ref()?;
        Some(blob.as_ref().len())
    }
}

impl<A: AsRef<[u8]>> Debug for ProofBlob<A> {
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
    #[error(
        "The proof is for a different blockhash than the one provided, expected {expected:?}, found {found:?}"
    )]
    BlockHashMismatch { expected: Hash, found: Hash },
    #[error(
        "Blob {index} does not match the provided hash, expected {expected:?}, found {found:?}"
    )]
    BlobHashMismatch {
        index: usize,
        expected: Hash,
        found: Hash,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyArgs {
    pub blober: Pubkey,
    pub blober_state: Vec<u8>,
    pub blobs: Vec<ProofBlob<Vec<u8>>>,
}

impl VerifyArgs {
    pub fn hash_blobs(&self) -> [u8; HASH_BYTES] {
        merge_all_hashes(self.blobs.iter().map(ProofBlob::hash_blob))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyArgsCommitment {
    pub blober_hash: [u8; HASH_BYTES],
}

impl TryFrom<VerifyArgs> for VerifyArgsCommitment {
    type Error = BloberAccountStateError;

    fn try_from(args: VerifyArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            blober_hash: get_blober_hash(&args.blober_state)?,
        })
    }
}

impl TryFrom<&VerifyArgs> for VerifyArgsCommitment {
    type Error = BloberAccountStateError;

    fn try_from(args: &VerifyArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            blober_hash: get_blober_hash(&args.blober_state)?,
        })
    }
}

impl VerifyArgs {
    pub fn into_commitment(&self) -> BloberAccountStateResult<VerifyArgsCommitment> {
        VerifyArgsCommitment::try_from(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompoundInclusionProofCommitment {
    pub blober_initial_hash: [u8; HASH_BYTES],
}

impl From<CompoundInclusionProof> for CompoundInclusionProofCommitment {
    fn from(proof: CompoundInclusionProof) -> Self {
        Self {
            blober_initial_hash: proof.blober_account_state_proof.initial_hash,
        }
    }
}

impl From<&CompoundInclusionProof> for CompoundInclusionProofCommitment {
    fn from(proof: &CompoundInclusionProof) -> Self {
        Self {
            blober_initial_hash: proof.blober_account_state_proof.initial_hash,
        }
    }
}

impl CompoundInclusionProof {
    /// Creates an inclusion proof.
    pub fn new(
        blob_proofs: Vec<BlobProof>,
        blober_pubkey: Pubkey,
        blober_account_state_proof: BloberAccountStateProof,
    ) -> Self {
        Self {
            blob_proofs,
            blober_pubkey,
            blober_account_state_proof,
        }
    }

    pub fn into_commitment(&self) -> CompoundInclusionProofCommitment {
        CompoundInclusionProofCommitment::from(self)
    }

    pub fn target_slot(&self) -> u64 {
        self.blober_account_state_proof.target_slot()
    }

    pub fn hash_proofs(&self) -> [u8; HASH_BYTES] {
        merge_all_hashes(self.blob_proofs.iter().map(BlobProof::hash_proof))
    }

    /// Verifies that a specific Solana block contains the provided blobs, and that no blobs have been excluded.
    #[tracing::instrument(skip_all, err(Debug), fields(blober = %blober))]
    pub fn verify(
        &self,
        blober: Pubkey,
        blober_state: &[u8],
        blobs: &[ProofBlob<impl AsRef<[u8]>>],
    ) -> Result<(), CompoundInclusionProofError> {
        if blobs.len() != self.blob_proofs.len() {
            return Err(CompoundInclusionProofError::InvalidNumberOfBlobs);
        }
        let blob_count = self.blober_account_state_proof.blobs().count();
        if blob_count != self.blob_proofs.len() {
            return Err(CompoundInclusionProofError::MissingBlobs);
        }
        if self.blober_pubkey != blober {
            return Err(CompoundInclusionProofError::IncludedAccountNotBlober);
        }

        let blob_accounts = self.blober_account_state_proof.blobs().collect::<Vec<_>>();

        for (index, ((blob, blob_proof), blob_account)) in blobs
            .iter()
            .zip_eq(&self.blob_proofs)
            .zip_eq(blob_accounts)
            .enumerate()
        {
            let digest = blob_account.verify(blob)?;

            if digest != blob_proof.digest {
                return Err(CompoundInclusionProofError::BlobHashMismatch {
                    index,
                    expected: Hash::new_from_array(blob_proof.digest),
                    found: Hash::new_from_array(digest),
                });
            }

            if let Some(data) = &blob.data {
                blob_proof.verify(data.as_ref())?;
            }
        }

        self.blober_account_state_proof.verify(blober_state)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::collections::BTreeMap;

    use anchor_lang::{AnchorSerialize, Discriminator, solana_program::clock::Slot};
    use arbtest::arbtest;
    use blober_account_state::{BlobAccount, merge_all_hashes};
    use data_anchor_blober::{
        BLOB_DATA_END, BLOB_DATA_START, CHUNK_SIZE, initial_hash,
        state::{blob::Blob, blober::Blober},
    };
    use solana_signer::Signer;

    use super::*;
    use crate::testing::{ArbAccount, ArbKeypair};

    fn roundtrip_serialization(proof: CompoundInclusionProof) {
        let serialized_json = serde_json::to_string(&proof).unwrap();
        let deserialized_json: CompoundInclusionProof =
            serde_json::from_str(&serialized_json).unwrap();
        assert_eq!(proof, deserialized_json);

        let serialized_bincode = bincode::serialize(&proof).unwrap();
        let deserialized_bincode: CompoundInclusionProof =
            bincode::deserialize(&serialized_bincode).unwrap();
        assert_eq!(proof, deserialized_bincode);
    }

    #[test]
    fn inclusion_construction_no_changes() {
        let slot = 1;
        let blober = Pubkey::new_unique();
        let blober_account_state_proof =
            BloberAccountStateProof::new(initial_hash(), slot, Default::default());
        let compound_inclusion_proof =
            CompoundInclusionProof::new(Vec::new(), blober, blober_account_state_proof);
        let blober_state = Blober {
            caller: Pubkey::new_unique(),
            namespace: "test".to_string(),
            hash: initial_hash(),
            slot: 1,
        };
        let state_bytes = [
            Blober::DISCRIMINATOR,
            blober_state.try_to_vec().unwrap().as_ref(),
        ]
        .concat();
        let uploads: Vec<ProofBlob<Vec<u8>>> = Vec::new();
        let verification = compound_inclusion_proof.verify(blober, &state_bytes, &uploads);
        assert!(
            verification.is_ok(),
            "Expected verification to succeed, but it failed: {verification:?}",
        );
    }

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

            // 10% chance that there's invalid data, 90% chance that it's the original
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
            let mut source_accounts: Vec<_> = vec![BlobAccount::new(
                blob_account.0.pubkey(),
                blob_account.1.data[BLOB_DATA_START..BLOB_DATA_END].to_vec(),
            )];

            if u.ratio(1, 10)? {
                // Add an extra source account that hasn't actually called the blober, I.E. false proof.
                source_accounts.push(BlobAccount::new(
                    u.arbitrary::<ArbKeypair>()?.pubkey(),
                    u.arbitrary()?,
                ));
                unmodified = false;
            }

            let blober_account_state_proof = BloberAccountStateProof::new(
                initial_hash(),
                slot,
                [(slot + 1, source_accounts.clone())].into_iter().collect(),
            );

            // Always include the blober account.
            let mut blober_data = Blober {
                caller: data_anchor_blober::id(),
                hash: initial_hash(),
                slot: 0,
                namespace: "".to_string(),
            };
            if u.ratio(1, 10)? {
                let new_slot = u.arbitrary()?;
                if new_slot >= slot && new_slot != 0 {
                    unmodified = new_slot == slot;
                    slot = new_slot;
                }
            }

            if u.ratio(9, 10)? {
                blober_data.store_hash(&source_accounts[0].hash_blob(), slot + 1);
            } else {
                // The blober account was not invoked.
                unmodified = false;
            }

            // ----------------------- Payer proof -----------------------------------------
            let writable_blob_account = blob_account.0.pubkey();

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

            let compound_inclusion_proof =
                CompoundInclusionProof::new(blob_proofs, blober, blober_account_state_proof);

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

            dbg!(&compound_inclusion_proof);
            let blober_state = [
                Blober::DISCRIMINATOR,
                blober_data.try_to_vec().unwrap().as_ref(),
            ]
            .concat();
            if unmodified {
                compound_inclusion_proof
                    .verify(blober, &blober_state, &blobs)
                    .unwrap();
                // It should also be possible to verify the proof without the blob data.
                let empty_blobs: Vec<_> = blobs
                    .into_iter()
                    .map(|b| ProofBlob::empty(b.blob))
                    .collect();
                compound_inclusion_proof
                    .verify(blober, &blober_state, &empty_blobs)
                    .unwrap();
                roundtrip_serialization(compound_inclusion_proof);
            } else {
                compound_inclusion_proof
                    .verify(blober, &blober_state, &blobs)
                    .unwrap_err();
                roundtrip_serialization(compound_inclusion_proof);
            }

            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn inclusion_construction_multiple_slots_multiple_blobs() {
        arbtest(|u| {
            let slots: u64 = u.int_in_range(1..=20)?;

            let mut blobs =
                BTreeMap::<Slot, Vec<(ProofBlob<Vec<u8>>, BlobProof, BlobAccount)>>::new();

            let mut unmodified = true;

            for slot in 1..=slots {
                let blob_count: u64 = u.int_in_range(0..=5)?;
                let mut slot_blobs = Vec::with_capacity(blob_count as usize);

                for _ in 0..blob_count {
                    let mut blob = vec![0u8; u.int_in_range(0..=u16::MAX)? as usize];
                    u.fill_buffer(&mut blob)?;

                    if blob.is_empty() {
                        // Empty blob, skip.
                        continue;
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

                    let blob_address = u.arbitrary::<ArbKeypair>()?.pubkey();
                    let mut blob_state = Blob::new(slot, 0, blob.len() as u32, 0);
                    for (chunk_index, chunk_data) in &chunks {
                        blob_state.insert(slot, *chunk_index, chunk_data);
                    }

                    let proof_blob = if u.ratio(1, 10)? {
                        let modified_blob = u.arbitrary::<Vec<u8>>()?;
                        if modified_blob != blob {
                            unmodified = false;
                        }
                        ProofBlob {
                            blob: blob_address,
                            data: Some(modified_blob),
                        }
                    } else {
                        ProofBlob {
                            blob: blob_address,
                            data: Some(blob.clone()),
                        }
                    };

                    let blob_proof = if u.ratio(1, 10)? {
                        let mut new_chunks = chunks.clone();
                        for _ in 0..10 {
                            let a = u.choose_index(chunks.len())?;
                            let b = u.choose_index(chunks.len())?;
                            new_chunks.swap(a, b);
                        }
                        if new_chunks != chunks {
                            unmodified = false;
                        }

                        BlobProof::new(&new_chunks)
                    } else {
                        BlobProof::new(&chunks)
                    };

                    let blob_account_state = [
                        Blob::DISCRIMINATOR.to_vec(),
                        blob_state.try_to_vec().unwrap(),
                    ]
                    .concat()[BLOB_DATA_START..BLOB_DATA_END]
                        .to_vec();
                    let blob_account = if u.ratio(1, 10)? {
                        let new_key = u.arbitrary::<ArbKeypair>()?.pubkey();
                        let new_blob_account_state = u.arbitrary::<Vec<u8>>()?;

                        if new_key != blob_address || new_blob_account_state != blob_account_state {
                            unmodified = false;
                        }

                        BlobAccount::new(new_key, new_blob_account_state)
                    } else {
                        BlobAccount::new(blob_address, blob_account_state)
                    };

                    slot_blobs.push((proof_blob, blob_proof, blob_account));
                }

                // We want to start insertions at slot 2
                blobs.insert(slot + 1, slot_blobs);
            }

            let blober_pubkey = u.arbitrary::<ArbKeypair>()?.pubkey();

            let mut blob_accounts = if u.ratio(1, 10)? {
                // Add an extra blob account that hasn't actually called the blober, I.E. false proof.
                let mut blob_accounts_map = BTreeMap::new();
                for (slot, blob_data) in blobs.iter() {
                    if u.ratio(1, 10)? && !blob_data.is_empty() {
                        // Skip this slot.
                        unmodified = false;
                        continue;
                    }

                    let mut slot_blob_accounts = Vec::new();

                    for (_, _, account) in blob_data {
                        if u.ratio(1, 10)? {
                            // Skip this account.
                            unmodified = false;
                            continue;
                        } else {
                            slot_blob_accounts.push(account.clone());
                        }
                    }

                    if u.ratio(1, 10)? {
                        // Add an extra account that hasn't called the blober.
                        unmodified = false;
                        let insert_index = u.choose_index(slot_blob_accounts.len())?;
                        slot_blob_accounts.insert(
                            insert_index,
                            BlobAccount::new(u.arbitrary::<ArbKeypair>()?.pubkey(), u.arbitrary()?),
                        );
                    }

                    if !slot_blob_accounts.is_empty() {
                        blob_accounts_map.insert(*slot, slot_blob_accounts);
                    }
                }

                blob_accounts_map
            } else {
                blobs
                    .iter()
                    .map(|(slot, accounts)| {
                        (
                            *slot,
                            accounts
                                .iter()
                                .map(|(_, _, account)| account.clone())
                                .collect(),
                        )
                    })
                    .collect()
            };

            blob_accounts.retain(|_, accounts| !accounts.is_empty());

            let blober_account_state_proof =
                BloberAccountStateProof::new(initial_hash(), 1, blob_accounts);

            let blob_proofs = if u.ratio(1, 10)? {
                let mut blob_proofs = Vec::new();
                for slot_blobs in blobs.values() {
                    for (_, proof, _) in slot_blobs {
                        if u.ratio(1, 10)? {
                            // Skip this proof.
                            unmodified = false;
                            continue;
                        }
                        blob_proofs.push(proof.clone());
                    }
                }
                blob_proofs
            } else {
                blobs
                    .values()
                    .flat_map(|blobs| {
                        blobs
                            .iter()
                            .map(|(_, proof, _)| proof.clone())
                            .collect_vec()
                    })
                    .collect_vec()
            };

            let compound_inclusion_proof =
                CompoundInclusionProof::new(blob_proofs, blober_pubkey, blober_account_state_proof);

            let caller = u.arbitrary::<ArbKeypair>()?.pubkey();
            let namespace = u.arbitrary::<String>()?;

            let hash = if u.ratio(1, 10)? {
                let mut hashes = vec![initial_hash()];
                for slot_blobs in blobs.values() {
                    for (_, _, account) in slot_blobs {
                        if u.ratio(1, 10)? {
                            // Skip this account.
                            unmodified = false;
                            continue;
                        }
                        hashes.push(account.hash_blob());
                    }
                }
                merge_all_hashes(hashes.into_iter())
            } else {
                merge_all_hashes(
                    std::iter::once(initial_hash()).chain(blobs.values().flat_map(|slot_blobs| {
                        slot_blobs.iter().map(|(_, _, account)| account.hash_blob())
                    })),
                )
            };

            let expected_slot = blobs
                .iter()
                .filter_map(|(slot, blobs)| (!blobs.is_empty()).then_some(slot))
                .max()
                .cloned()
                .unwrap_or(1);
            let slot = if u.ratio(1, 10)? {
                let new_slot = u.arbitrary::<Slot>()?;

                if new_slot != expected_slot {
                    unmodified = false;
                }

                new_slot
            } else {
                expected_slot
            };

            let blober = Blober {
                caller,
                namespace,
                hash,
                slot,
            };

            let blober_state =
                [Blober::DISCRIMINATOR, blober.try_to_vec().unwrap().as_ref()].concat();
            let blobs = blobs
                .values()
                .flat_map(|blobs| blobs.iter().map(|(blob, _, _)| blob.clone()).collect_vec())
                .collect_vec();

            dbg!(&compound_inclusion_proof);
            dbg!(&blober_pubkey);
            dbg!(&blober.slot);
            dbg!(&blobs);

            let verification_result =
                compound_inclusion_proof.verify(blober_pubkey, &blober_state, &blobs);

            if unmodified {
                verification_result.unwrap();
            } else {
                verification_result.unwrap_err();
            }

            roundtrip_serialization(compound_inclusion_proof);

            Ok(())
        })
        .size_max(100_000_000);
    }
}
