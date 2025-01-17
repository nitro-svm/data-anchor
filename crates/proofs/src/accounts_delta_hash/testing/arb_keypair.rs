use std::hash::Hash;

use arbitrary::{Arbitrary, Unstructured};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::{SeedDerivable, Signer, SignerError},
};

/// An arbitrary keypair, since we can't implement [`arbitrary::Arbitrary`] for
/// [`solana_sdk::signature::Keypair`] or [`solana_sdk::pubkey::Pubkey`].
///
/// Mainly used to generate valid pubkeys from arbitrary seeds.
#[derive(Debug, PartialEq)]
pub struct ArbKeypair(Keypair);

impl ArbKeypair {
    pub fn pubkey(&self) -> Pubkey {
        self.0.pubkey()
    }

    pub fn sign_message(&self, message: &[u8]) -> Signature {
        self.0.sign_message(message)
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

impl Signer for ArbKeypair {
    fn try_pubkey(&self) -> Result<Pubkey, SignerError> {
        self.0.try_pubkey()
    }

    fn try_sign_message(&self, message: &[u8]) -> Result<Signature, SignerError> {
        self.0.try_sign_message(message)
    }

    fn is_interactive(&self) -> bool {
        self.0.is_interactive()
    }
}
