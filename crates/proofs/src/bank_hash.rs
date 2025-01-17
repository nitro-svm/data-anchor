//! Proof of the bankhash, proving the state of the Solana bank (including the accounts_delta_hash)
//! at a particular slot.

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

/// The proof for a bankhash is simply its components.
///
/// It's not very interesting on its own, but when combined with [`crate::accounts_delta_hash`]
/// it can be used to prove the state of accounts at a particular slot.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct BankHashProof {
    /// The bankhash of the parent block.
    /// NOT the blockhash.
    pub parent_bankhash: solana_sdk::hash::Hash,

    /// The hash of all modified accounts in the block, see [`crate::accounts_delta_hash`].
    pub(crate) accounts_delta_hash: solana_sdk::hash::Hash,

    /// The number of signatures in the block.
    pub(crate) signature_count: u64,

    /// The Proof-of-History tick after interleaving all the transactions in the block.
    /// NOT related to the bankhash.
    pub blockhash: solana_sdk::hash::Hash,
}

impl Debug for BankHashProof {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Proof")
            .field("parent_bankhash", &self.parent_bankhash)
            .field("accounts_delta_hash", &self.accounts_delta_hash)
            .field("signature_count", &self.signature_count)
            .field("blockhash", &self.blockhash)
            .field("bank_hash()", &self.hash())
            .finish()
    }
}

impl BankHashProof {
    /// Creates a bank hash proof.
    pub fn new(
        parent_bankhash: solana_sdk::hash::Hash,
        accounts_delta_hash: solana_sdk::hash::Hash,
        signature_count: u64,
        blockhash: solana_sdk::hash::Hash,
    ) -> Self {
        Self {
            parent_bankhash,
            accounts_delta_hash,
            signature_count,
            blockhash,
        }
    }

    /// Verifies that the bankhash matches the expected value.
    pub fn verify(&self, bank_hash: solana_sdk::hash::Hash) -> bool {
        self.hash() == bank_hash
    }

    /// Hashes the components to create the bankhash.
    pub fn hash(&self) -> solana_sdk::hash::Hash {
        // https://github.com/anza-xyz/agave/blob/v1.18.22/runtime/src/bank.rs#L6951-L6956
        solana_sdk::hash::hashv(&[
            self.parent_bankhash.as_ref(),
            self.accounts_delta_hash.as_ref(),
            self.signature_count.to_le_bytes().as_ref(),
            self.blockhash.as_ref(),
        ])
    }
}

#[cfg(test)]
impl<'a> arbitrary::Arbitrary<'a> for BankHashProof {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> Result<Self, arbitrary::Error> {
        use crate::testing::arbitrary_hash;
        Ok(Self {
            parent_bankhash: arbitrary_hash(u)?,
            accounts_delta_hash: arbitrary_hash(u)?,
            signature_count: u.arbitrary()?,
            blockhash: arbitrary_hash(u)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use arbitrary::Arbitrary;
    use arbtest::arbtest;

    use super::*;
    use crate::testing::arbitrary_hash;

    #[test]
    fn known_values_from_solana() {
        // These hardcoded values were taken from various tests in the Solana implementation.
        let test_cases = vec![
            (
                "11111111111111111111111111111111",
                "AAH4XpMn5FrdDoCwaTXKY8Cz3hmeQKbeZFt8S44XYuYi",
                1,
                "J4UmrMsC4pE4GKEgrbyegswSfMopxs38zg1xb7abVnfa",
                "acuaeWZsRuYjSq8Z3t7BbRB2TnF538yrMZENofNQ3A9",
            ),
            (
                "11111111111111111111111111111111",
                "3e1WeddAcM6joht3K69aUzvYj4peMbWepYatDKYt9sNC",
                0,
                "2B7nfm7qU7z2n1LsXhYNsy1SgUrQytK2aW7HTRiKTSur",
                "ARQDUtGsw7DPFmiw7AWvHgpxib1E3V15jp91U25wkFD",
            ),
            (
                "11111111111111111111111111111111",
                "9KMQVuq5rUG2ji36GfvoxZCTHrJEvuH9YJyfFjFZ6DDx",
                0,
                "2B7nfm7qU7z2n1LsXhYNsy1SgUrQytK2aW7HTRiKTSur",
                "8uqjLNiXSkyg99dxRXMTJPN2Xz9xn6KvKkkNwMpPTLt4",
            ),
            (
                "8uqjLNiXSkyg99dxRXMTJPN2Xz9xn6KvKkkNwMpPTLt4",
                "EtDCV1MSqhvL8sfmwhxuVB3eC53KGJVT4NDVU5CBhaPD",
                0,
                "2B7nfm7qU7z2n1LsXhYNsy1SgUrQytK2aW7HTRiKTSur",
                "2Pj5sRN85mVTqk4hiri9SgPQ7VZ7t8ki4AiXciKsdqzi",
            ),
            (
                "8uqjLNiXSkyg99dxRXMTJPN2Xz9xn6KvKkkNwMpPTLt4",
                "Fpi4tGcNqSmJxVc1F2A63v3gqTmahkjmF9D7qLxiKQAn",
                1,
                "2B7nfm7qU7z2n1LsXhYNsy1SgUrQytK2aW7HTRiKTSur",
                "2WxpWuxFHLMKjVQDQSBznBbJNjZrxMiAEdPB4s6hTG2G",
            ),
            (
                "11111111111111111111111111111111",
                "3d8fqmw2aGfek4c5ZgTDnojF8UjQKxfmCS1sdRbUutZb",
                0,
                "GBXGZS579k5B7eqrGELiDkiP7vvQyB4bZ93u8cgaYFeJ",
                "6ustQtaSzYzaMZbAVsNTdtj8RMgMeE83PDy1DdVV6N6U",
            ),
            (
                "11111111111111111111111111111111",
                "cLUFixMKDgFbNcWxLab3sJZsEaYPs41ZGTEoJbUPfX9",
                2,
                "GBXGZS579k5B7eqrGELiDkiP7vvQyB4bZ93u8cgaYFeJ",
                "Chz6wPtQvcehwJemAvhP39xTPYB2BWPaa4F458Sohhjw",
            ),
            (
                "FKv2WQG68Fj3MU5MVPTNzj4SaUq6heCgReMdMnvQW4Sy",
                "CbYAUEJFYsCSF4ySAeUB71WzpTFW5bdu6Jp4zu3TLfMi",
                2,
                "JAu73Nm8wtCVEihHFitDMVQ2W3jRGuA19ZeshTp4gYvj",
                "81CMWBfTbpRuX2zQbRyzgbfGBJoX7dMTUfMFkK6gs75v",
            ),
        ];

        use solana_sdk::hash::Hash;

        for (parent_bankhash, accounts_delta_hash, signature_count, blockhash, expected) in
            test_cases
        {
            let proof = BankHashProof {
                parent_bankhash: Hash::from_str(parent_bankhash).unwrap(),
                accounts_delta_hash: Hash::from_str(accounts_delta_hash).unwrap(),
                signature_count,
                blockhash: Hash::from_str(blockhash).unwrap(),
            };
            assert!(proof.verify(Hash::from_str(expected).unwrap()));
        }
    }

    #[test]
    fn bank_hash_construction() {
        arbtest(move |u| {
            let mut proof = BankHashProof::arbitrary(u)?;
            let hash_before = proof.hash();

            let mut unmodified = true;
            if u.ratio(1, 10)? {
                let new_parent_bankhash = arbitrary_hash(u)?;
                unmodified = new_parent_bankhash == proof.parent_bankhash;
                proof.parent_bankhash = new_parent_bankhash;
            } else if u.ratio(1, 10)? {
                let new_accounts_delta_hash = arbitrary_hash(u)?;
                unmodified = new_accounts_delta_hash == proof.accounts_delta_hash;
                proof.accounts_delta_hash = new_accounts_delta_hash;
            } else if u.ratio(1, 10)? {
                let new_signature_count = u.arbitrary()?;
                unmodified = new_signature_count == proof.signature_count;
                proof.signature_count = new_signature_count;
            } else if u.ratio(1, 10)? {
                let new_blockhash = arbitrary_hash(u)?;
                unmodified = new_blockhash == proof.blockhash;
                proof.blockhash = new_blockhash;
            }

            if unmodified {
                assert!(proof.verify(hash_before));
            } else {
                assert!(!proof.verify(hash_before));
            }

            Ok(())
        });
    }
}
