//! Proof of the contents of a blob uploaded to the blober program.

use std::{cmp::min, fmt::Debug};

use blober::{compute_blob_digest, CHUNK_SIZE};
use serde::{Deserialize, Serialize};
use solana_sdk::hash::HASH_BYTES;
use thiserror::Error;

/// A proof that a specific blob has been uploaded to the blober program. The proof consists of two
/// parts: The digest of the blob, and the order in which its chunks arrived. The digest is computed
/// incrementally by hashing the current hash (starting from the default hash) with the chunk index
/// and data, see [`compute_blob_digest`] for the exact implementation.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BlobProof {
    /// The SHA-256 hash of the blob.
    pub digest: [u8; 32],
    pub chunk_order: Vec<u16>,
}

impl Debug for BlobProof {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proof")
            .field(
                "digest",
                &solana_sdk::hash::Hash::new_from_array(self.digest),
            )
            .field("chunk_order", &self.chunk_order)
            .finish()
    }
}

/// Failures that can occur when verifying a [`BlobProof`].
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum BlobProofError {
    #[error("Invalid structure when checking blob against stored chunks.")]
    InvalidStructure,
    #[error("Digest does not match the expected value. Expected {expected:?}, found {found:?}")]
    DigestMismatch {
        expected: [u8; HASH_BYTES],
        found: [u8; HASH_BYTES],
    },
}

impl BlobProof {
    /// Creates a new proof for the given blob. The blob must be at least one byte in size.
    pub fn new<A: AsRef<[u8]>>(chunks: &[(u16, A)]) -> Self {
        let digest = compute_blob_digest(chunks);
        let chunk_order = chunks.iter().map(|(i, _)| *i).collect();
        Self {
            digest,
            chunk_order,
        }
    }

    /// Verifies that the given blob matches the proof.
    pub fn verify(&self, blob: &[u8]) -> Result<(), BlobProofError> {
        let chunk_size = CHUNK_SIZE as usize;
        let chunks = self
            .chunk_order
            .iter()
            .map(|&i| {
                let start_offset = i as usize * chunk_size;
                let end_offset = min(start_offset + chunk_size, blob.len());

                match blob.get(start_offset..end_offset) {
                    Some(chunk) => Ok((i, chunk)),
                    None => Err(BlobProofError::InvalidStructure),
                }
            })
            .collect::<Result<Vec<_>, BlobProofError>>()?;

        let digest = compute_blob_digest(&chunks);

        if self.digest == digest {
            Ok(())
        } else {
            Err(BlobProofError::DigestMismatch {
                expected: self.digest,
                found: digest,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use arbtest::arbtest;
    use blober::CHUNK_SIZE;

    use super::*;

    #[test]
    fn empty_blob() {
        BlobProof::new::<&[u8]>(&[]).verify(&[]).unwrap();
    }

    #[test]
    fn proof() {
        arbtest(|u| {
            let data = u.arbitrary::<Vec<u8>>()?;
            if data.is_empty() {
                // Empty blob, invalid test.
                return Ok(());
            }
            let mut chunks = data
                .chunks(CHUNK_SIZE as usize)
                .enumerate()
                .map(|(i, c)| (i as u16, c))
                .collect::<Vec<_>>();
            for _ in 0..u.arbitrary_len::<usize>()? {
                let a = u.choose_index(chunks.len())?;
                let b = u.choose_index(chunks.len())?;
                chunks.swap(a, b);
            }
            let proof = BlobProof::new(&chunks);
            proof.verify(&data).unwrap();
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn false_proof() {
        arbtest(|u| {
            let mut data = u.arbitrary::<Vec<u8>>()?;
            if data.is_empty() {
                // Empty blob, invalid test.
                return Ok(());
            }
            let chunks = data
                .chunks(CHUNK_SIZE as usize)
                .enumerate()
                .map(|(i, c)| (i as u16, c))
                .collect::<Vec<_>>();

            let proof = BlobProof::new(&chunks);
            // Swap the 0th byte with some other byte, which should change the digest.
            let other = 1 + u.choose_index(data.len() - 1)?;
            data.swap(0, other);
            proof.verify(&data).unwrap_err();
            Ok(())
        })
        .size_max(100_000_000);
    }
}
