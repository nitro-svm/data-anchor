use arbitrary::{Arbitrary, Unstructured};

/// Choose an option from a list, or generate a new one.
///
/// # Returns
/// A 2-tuple containing:
/// - The index of the chosen option, or `None` if a new one was generated.
/// - The chosen or generated option.
pub fn choose_or_generate<'a, T: Arbitrary<'a> + Clone + PartialEq>(
    u: &mut Unstructured<'a>,
    options: &[T],
) -> Result<(Option<usize>, T), arbitrary::Error> {
    // 50/50 chance of choosing an existing option, or generating a new one.
    // Matches the behaviour of how `arbitrary` generates lists.
    if u.arbitrary()? {
        let index = u.choose_index(options.len())?;
        Ok((Some(index), options[index].clone()))
    } else {
        let new = u.arbitrary()?;
        // Just in case Arbitrary happens to generate a value that's already in the list.
        let index = options.iter().position(|o| o == &new);
        Ok((index, new))
    }
}
