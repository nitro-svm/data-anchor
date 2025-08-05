//! This crate contains proofs related to the Solana blockchain.
//!
//! The proofs can prove the state of accounts on the chain and whether or not they were updated,
//! but it makes no semantic assumptions about the account data, it's just considered raw bytes.
//! The account data must first be deserialized and verified that it matches the expected state.

pub mod blob;
pub mod blober_account_state;
pub mod compound;
mod debug;

#[doc(hidden)]
#[cfg(test)]
pub(crate) mod testing {
    use std::{cmp::max, hash::Hash, ops::Deref};

    use anchor_lang::solana_program::clock::Epoch;
    use arbitrary::{Arbitrary, Unstructured};
    use solana_account::Account;
    use solana_keypair::Keypair;
    use solana_seed_derivable::SeedDerivable;
    use solana_signer::Signer;

    /// An arbitrary keypair, since we can't implement [`arbitrary::Arbitrary`] for
    /// [`solana_keypair::Keypair`] or [`anchor_lang::Pubkey`].
    ///
    /// Mainly used to generate valid pubkeys from arbitrary seeds.
    #[derive(Debug, PartialEq)]
    pub struct ArbKeypair(Keypair);

    impl Deref for ArbKeypair {
        type Target = Keypair;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    // A bunch of these impls aren't good security-wise, but this is all just for testing.

    impl Clone for ArbKeypair {
        fn clone(&self) -> Self {
            ArbKeypair(self.0.insecure_clone())
        }
    }

    impl Eq for ArbKeypair {}

    impl Hash for ArbKeypair {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.to_bytes().hash(state);
        }
    }

    impl<'a> Arbitrary<'a> for ArbKeypair {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            // The seed needs to be at least 32 bytes long.
            let len: usize = 32 + u.arbitrary_len::<ArbKeypair>()?;
            let mut seed = Vec::with_capacity(len);
            for _ in 0..len {
                seed.push(u.arbitrary()?);
            }
            let keypair = Keypair::from_seed(&seed).map_err(|_| arbitrary::Error::NotEnoughData)?;

            Ok(ArbKeypair(keypair))
        }

        fn size_hint(_depth: usize) -> (usize, Option<usize>) {
            (32, None)
        }
    }

    /// An arbitrary account, since we can't implement [`arbitrary::Arbitrary`] for [`solana_account::Account`].
    #[derive(Debug, Arbitrary, Clone, PartialEq, Eq, Hash)]
    pub struct ArbAccount {
        pub lamports: u64,
        pub data: Vec<u8>,
        pub owner: ArbKeypair,
        pub executable: bool,
        pub rent_epoch: Epoch,
    }

    impl From<ArbAccount> for Account {
        fn from(val: ArbAccount) -> Self {
            Account {
                lamports: max(1, val.lamports),
                data: val.data,
                owner: val.owner.pubkey(),
                executable: val.executable,
                rent_epoch: val.rent_epoch,
            }
        }
    }
}
