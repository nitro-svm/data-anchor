pub use arb::*;

#[cfg(test)]
#[cfg_attr(test, mutants::skip)]
mod arb {
    use solana_sdk::hash::Hash;

    // No point mutation testing the arbitrary implementation.
    pub fn arbitrary_hash(u: &mut arbitrary::Unstructured<'_>) -> Result<Hash, arbitrary::Error> {
        Ok(Hash::new_from_array(u.arbitrary()?))
    }
}
