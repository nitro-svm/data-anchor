//! Proof of the state of the [SlotHashes sysvar](https://docs.solanalabs.com/runtime/sysvars#slothashes)
//! for a given slot. Can be used together with [vote proofs][`crate::vote::single::SingleVoteProof`] to
//! prove that a specific bank hash was voted on.

use std::{fmt::Debug, sync::Arc};

use serde::{Deserialize, Serialize};
use solana_sdk::{clock::Slot, hash::Hash, slot_hashes::SlotHashes};
use thiserror::Error;

use crate::{accounts_delta_hash::inclusion::InclusionProof, debug::NoPrettyPrint};

/// A proof for the state of the SlotHashes sysvar for a given slot.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SlotHashProof {
    /// The slot at which the proven state was captured.
    pub(crate) slot: Slot,
    /// An inclusion proof for the SlotHashes sysvar. It might seem superfluous given that the
    /// sysvar will be present in every slot, but this proof is needed to ensure the state has not
    /// been taken from another valid (but different) slot.
    pub(crate) slot_hashes_inclusion_proof: InclusionProof,
}

impl Debug for SlotHashProof {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Proof");
        s.field("slot", &self.slot).field(
            "slot_hashes_inclusion_proof",
            &self.slot_hashes_inclusion_proof,
        );
        if let Ok(slot_hashes) = self.deserialize_account_data() {
            s.field(
                "slot_hashes[..10]",
                &NoPrettyPrint(slot_hashes.iter().take(10).collect::<Vec<_>>()),
            );
            if let Some(last_slot) = slot_hashes.last() {
                s.field("last_slot", &last_slot.0);
            }
        }
        s.finish()
    }
}

/// Failures that can occur when verifying a [`SlotHashProof`].
#[derive(Debug, Clone, Error)]
pub enum SlotHashError {
    #[error(
        "Slot hash for slot {slot} does not match the expected value, expected {expected}, found {found:?}"
    )]
    SlotHashMismatch {
        slot: Slot,
        expected: Hash,
        found: Option<Hash>,
    },
    #[error("The computed accounts delta hash does not match the provided value")]
    AccountsDeltaHashMismatch,
    #[error("The inclusion proof is not for the SlotHashes sysvar")]
    ProofNotForSlotHashes,
    #[error(transparent)]
    BincodeDeserialize(#[from] Arc<bincode::Error>),
}

impl SlotHashProof {
    /// Creates a new proof for the state of the SlotHashes sysvar for a given slot.
    pub fn new(slot: Slot, slot_hashes_inclusion_proof: InclusionProof) -> Self {
        Self {
            slot,
            slot_hashes_inclusion_proof,
        }
    }

    /// Verifies that the SlotHashes sysvar contains `bank_hash`.
    pub fn verify(
        &self,
        slot: Slot,
        bank_hash: Hash,
        accounts_delta_hash: Hash,
    ) -> Result<(), SlotHashError> {
        if self.slot_hashes_inclusion_proof.account_pubkey != solana_sdk::sysvar::slot_hashes::ID {
            return Err(SlotHashError::ProofNotForSlotHashes);
        }

        if self.hash(slot) != Some(bank_hash) {
            return Err(SlotHashError::SlotHashMismatch {
                slot,
                expected: bank_hash,
                found: self.hash(slot),
            });
        }

        if !self.slot_hashes_inclusion_proof.verify(accounts_delta_hash) {
            return Err(SlotHashError::AccountsDeltaHashMismatch);
        }

        Ok(())
    }

    /// Attempts to deserialize the stored account data into a [`SlotHashes`].
    pub fn deserialize_account_data(&self) -> Result<SlotHashes, Arc<bincode::Error>> {
        bincode::deserialize(&self.slot_hashes_inclusion_proof.account_data.data).map_err(Arc::new)
    }

    /// Attempts to deserialize the stored account data and extracts the bankhash for a specific slot.
    pub fn hash(&self, slot: Slot) -> Option<Hash> {
        self.deserialize_account_data().ok()?.get(&slot).copied()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use arbtest::arbtest;
    use solana_sdk::{account::Account, sysvar, sysvar::SysvarId};

    use super::*;
    use crate::{
        accounts_delta_hash::{
            AccountMerkleTree,
            testing::{ArbAccount, ArbKeypair},
        },
        testing::arbitrary_hash,
    };

    #[test]
    fn slot_hash_construction() {
        arbtest(|u| {
            let mut slot_hashes = u
                .arbitrary_iter::<(u64, [u8; 32])>()?
                .map(|tup| Ok((tup?.0, Hash::new_from_array(tup?.1))))
                .collect::<Result<HashSet<_>, _>>()?
                .into_iter()
                .collect::<Vec<_>>();

            let ((slot, hash), excluded) = if u.ratio(1, 10)? {
                let slot_hash = slot_hashes.remove(u.choose_index(slot_hashes.len())?);
                (slot_hash, true)
            } else {
                let slot_hash = slot_hashes.get(u.choose_index(slot_hashes.len())?).unwrap();
                (*slot_hash, false)
            };

            let slot_hashes = SlotHashes::new(&slot_hashes);

            let mut slot_hashes_account: Account = u.arbitrary::<ArbAccount>()?.into();
            slot_hashes_account.data = bincode::serialize(&slot_hashes).unwrap();

            let other_key = u.arbitrary::<ArbKeypair>()?.pubkey();
            let other_account = u.arbitrary::<ArbAccount>()?;

            let mut tree =
                AccountMerkleTree::builder([sysvar::slot_hashes::ID].into_iter().collect());
            tree.insert(SlotHashes::id(), slot_hashes_account);
            tree.insert(other_key, other_account.into());
            let tree = tree.build();

            let included_id = if u.ratio(1, 10)? {
                other_key
            } else {
                SlotHashes::id()
            };

            let inclusion_proof = tree.prove_inclusion(included_id).unwrap();

            let proof = SlotHashProof::new(slot, inclusion_proof);

            dbg!(&proof, &included_id, &slot_hashes, &tree);
            if excluded || included_id != SlotHashes::id() {
                proof.verify(slot, hash, tree.root()).unwrap_err();
            } else if u.ratio(1, 10)? {
                proof.verify(slot, hash, arbitrary_hash(u)?).unwrap_err();
            } else {
                proof.verify(slot, hash, tree.root()).unwrap();
            }

            Ok(())
        })
        .size_max(100_000_000);
    }
}
