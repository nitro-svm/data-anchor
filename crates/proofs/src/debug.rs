use std::{
    fmt,
    fmt::{Debug, Formatter},
};

pub struct NoPrettyPrint<T: Debug>(pub T);

impl<T: Debug> Debug for NoPrettyPrint<T> {
    #[cfg_attr(test, mutants::skip)]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // {:#?} never used here even if dbg!() specifies it
        write!(f, "{:?}", self.0)
    }
}
