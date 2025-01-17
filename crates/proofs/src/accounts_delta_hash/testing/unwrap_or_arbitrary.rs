use arbitrary::{Arbitrary, Unstructured};

pub trait UnwrapOrArbitrary<'a, T: Arbitrary<'a>> {
    fn unwrap_or_arbitrary(self, u: &mut Unstructured<'a>) -> Result<T, arbitrary::Error>;
}

impl<'a, T: Arbitrary<'a>> UnwrapOrArbitrary<'a, T> for Option<T> {
    fn unwrap_or_arbitrary(self, u: &mut Unstructured<'a>) -> Result<T, arbitrary::Error> {
        if let Some(value) = self {
            Ok(value)
        } else {
            u.arbitrary()
        }
    }
}
