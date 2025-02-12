//! This proof module contains the logic for verifying "completeness" in the sense that there are
//! no blobs in a specific Solana block.

use serde::{Deserialize, Serialize};
use solana_sdk::{clock::Slot, pubkey::Pubkey};
use thiserror::Error;

use crate::{
    accounts_delta_hash::exclusion::{ExclusionProof, ExclusionProofError},
    bank_hash::BankHashProof,
};

/// A proof that there are no blobs in a specific Solana block.
///
/// This proof consists of two parts:
/// 1. An [accounts delta hash proof][`ExclusionProof`] that proves that
///    the accounts_delta_hash does *not* include the [`blober`] account.
/// 2. A [bank hash proof][`BankHashProof`] that proves that the root hash of the accounts_delta_hash
///    is the same as the root in the bank hash.
///
/// The proof can then be verified by supplying the blockhash of the block in which the [`blober`]
/// was invoked.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CompoundCompletenessProof {
    slot: Slot,
    blober_exclusion_proof: ExclusionProof,
    pub bank_hash_proof: BankHashProof,
}

/// Failures that can occur when verifying a [`CompoundCompletenessProof`].
#[derive(Debug, Clone, Error)]
pub enum CompoundCompletenessProofError {
    #[error("The exclusion proof is not for the blober account")]
    ExcludedAccountNotBlober,
    #[error("The proof is for a different blockhash than the one provided, expected {expected:?}, found {found:?}")]
    BlockHashMismatch {
        expected: solana_sdk::hash::Hash,
        found: solana_sdk::hash::Hash,
    },
    #[error(transparent)]
    AccountsDeltaHash(#[from] ExclusionProofError),
}

impl CompoundCompletenessProof {
    /// Creates a completeness proof.
    pub fn new(
        slot: Slot,
        blober_exclusion_proof: ExclusionProof,
        bank_hash_proof: BankHashProof,
    ) -> Self {
        Self {
            slot,
            blober_exclusion_proof,
            bank_hash_proof,
        }
    }

    /// Verifies that there are no blobs in a specific Solana block.
    #[tracing::instrument(skip_all, err(Debug), fields(slot = %self.slot, blober = %blober, blockhash = %blockhash))]
    pub fn verify(
        &self,
        blober: Pubkey,
        blockhash: solana_sdk::hash::Hash,
    ) -> Result<(), CompoundCompletenessProofError> {
        if let Some(excluded) = self.blober_exclusion_proof.excluded() {
            // If the exclusion proof is for a specific account, it should be for the blober account.
            if excluded != &blober {
                return Err(CompoundCompletenessProofError::ExcludedAccountNotBlober);
            }
        } // If it's for the empty case (no accounts updated), there's nothing to check.

        if self.bank_hash_proof.blockhash != blockhash {
            return Err(CompoundCompletenessProofError::BlockHashMismatch {
                expected: blockhash,
                found: self.bank_hash_proof.blockhash,
            });
        }

        self.blober_exclusion_proof
            .verify(self.bank_hash_proof.accounts_delta_hash)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use arbtest::arbtest;
    use solana_sdk::{account::Account, slot_hashes::SlotHashes, sysvar, sysvar::SysvarId};

    use super::*;
    use crate::{
        accounts_delta_hash::{
            exclusion::{left::ExclusionLeftProof, ExclusionProof},
            testing::{choose_or_generate, ArbAccount, ArbKeypair, UnwrapOrArbitrary},
            AccountMerkleTree,
        },
        testing::arbitrary_hash,
    };

    #[test]
    fn completeness_construction() {
        arbtest(|u| {
            let accounts: Vec<(ArbKeypair, ArbAccount)> = u.arbitrary()?;
            let (leftmost_index, leftmost) = choose_or_generate(u, &accounts)?;

            let blober = u.arbitrary::<ArbKeypair>()?.pubkey();

            let mut solana_accounts = accounts
                .into_iter()
                .map(|(keypair, account)| (keypair.pubkey(), account.into()))
                .collect::<Vec<_>>();
            let is_excluded = if u.ratio(1, 2)? {
                solana_accounts.push((
                    blober,
                    Account {
                        // The actual contents of the blober doesn't matter for this proof - if it's
                        // not excluded, the proof is invalid.
                        ..Default::default()
                    },
                ));
                false
            } else {
                true
            };
            solana_accounts.sort_by_key(|(pubkey, _)| *pubkey);

            // Used later in the test, but must be marked as an important pubkey in advance for that to work.
            let not_blober = u.arbitrary::<ArbKeypair>()?.pubkey();
            let mut tree = AccountMerkleTree::builder([blober, not_blober].into_iter().collect());
            for (pubkey, account) in solana_accounts.iter() {
                tree.insert(*pubkey, account.clone());
            }
            let tree = tree.build();

            let parent_bankhash = arbitrary_hash(u)?;
            let signature_count = u.arbitrary()?;
            let blockhash = arbitrary_hash(u)?;
            let root = tree.root();
            let bank_hash_proof =
                BankHashProof::new(parent_bankhash, root, signature_count, blockhash);

            let mut trusted_vote_authorities: Vec<ArbKeypair> = vec![
                arbitrary::Arbitrary::arbitrary(u)?,
                arbitrary::Arbitrary::arbitrary(u)?,
            ];
            trusted_vote_authorities.sort_by_key(|pk| pk.pubkey());

            let required_votes = 1 + u.choose_index(trusted_vote_authorities.len())?;

            let votes_valid =
                required_votes <= trusted_vote_authorities.len() && required_votes > 0;

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
                AccountMerkleTree::builder([sysvar::slot_hashes::ID].into_iter().collect());
            slot_hashes_tree.insert(SlotHashes::id(), slot_hashes_account);

            if is_excluded {
                let exclusion_proof = tree.prove_exclusion(blober).unwrap();
                let proof = CompoundCompletenessProof::new(
                    proven_slot,
                    exclusion_proof.clone(),
                    bank_hash_proof,
                );
                if u.ratio(1, 5)? {
                    // Wrong accounts_delta_hash, but account *is* actually excluded.
                    let accounts_delta_hash = arbitrary_hash(u)?;
                    let bank_hash_proof = BankHashProof::new(
                        parent_bankhash,
                        accounts_delta_hash,
                        signature_count,
                        blockhash,
                    );
                    let proof = CompoundCompletenessProof::new(
                        proven_slot,
                        exclusion_proof,
                        bank_hash_proof,
                    );
                    proof.verify(blober, bank_hash_proof.blockhash).unwrap_err();
                    roundtrip_serialization(proof);
                } else if !solana_accounts.is_empty() && u.ratio(1, 5)? {
                    // The excluded account is not the blober account.
                    if not_blober != blober {
                        dbg!(&tree, &not_blober.to_string());
                        if let Some(exclusion_proof) = tree.prove_exclusion(not_blober) {
                            let proof = CompoundCompletenessProof::new(
                                proven_slot,
                                exclusion_proof,
                                bank_hash_proof,
                            );
                            proof.verify(blober, bank_hash_proof.blockhash).unwrap_err();
                            roundtrip_serialization(proof);
                        }
                    }
                } else if !votes_valid {
                    // Something is wrong with the multi vote proof.
                    proof.verify(blober, bank_hash_proof.blockhash).unwrap_err();
                    roundtrip_serialization(proof);
                } else {
                    dbg!(&proof);
                    proof
                        .verify(
                            blober,
                            // In real code this value wouldn't come from the proof itself,
                            // instead it would be sourced from a third-party Solana node.
                            bank_hash_proof.blockhash,
                        )
                        .unwrap();
                    roundtrip_serialization(proof);
                };
            } else {
                // It doesn't really matter which false exclusion proof is used here, it could be
                // exhaustive but it's not worth the readability of the test.
                let false_exclusion_proof = ExclusionProof::ExclusionLeft(ExclusionLeftProof {
                    excluded: blober,
                    leftmost: tree.unchecked_inclusion_proof(
                        leftmost_index.unwrap_or_arbitrary(u)?,
                        &leftmost.0.pubkey(),
                        &leftmost.1.clone().into(),
                    ),
                });
                let proof = CompoundCompletenessProof::new(
                    proven_slot,
                    false_exclusion_proof,
                    bank_hash_proof,
                );
                dbg!(&solana_accounts, &proof);
                proof.verify(blober, bank_hash_proof.blockhash).unwrap_err();
                roundtrip_serialization(proof);
            }

            Ok(())
        })
        .size_max(100_000_000);
    }

    fn roundtrip_serialization(proof: CompoundCompletenessProof) {
        let serialized_json = serde_json::to_string(&proof).unwrap();
        let deserialized_json: CompoundCompletenessProof =
            serde_json::from_str(&serialized_json).unwrap();
        assert_eq!(proof, deserialized_json);

        let serialized_bincode = bincode::serialize(&proof).unwrap();
        let deserialized_bincode: CompoundCompletenessProof =
            bincode::deserialize(&serialized_bincode).unwrap();
        assert_eq!(proof, deserialized_bincode);

        let serialized_risc0_zkvm = risc0_zkvm::serde::to_vec(&proof).unwrap();
        let deserialized_risc0_zkvm: CompoundCompletenessProof =
            risc0_zkvm::serde::from_slice(&serialized_risc0_zkvm).unwrap();
        assert_eq!(proof, deserialized_risc0_zkvm);
    }
}
