// No point mutation testing the arbitrary implementation.
#[cfg(test)]
#[cfg_attr(test, mutants::skip)]
pub fn arbitrary_hash(
    u: &mut arbitrary::Unstructured<'_>,
) -> Result<solana_sdk::hash::Hash, arbitrary::Error> {
    Ok(solana_sdk::hash::Hash::new_from_array(u.arbitrary()?))
}
