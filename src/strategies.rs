use crate::StuffingStrategy;

unsafe impl StuffingStrategy for () {
    type Extra = ();

    fn is_extra(_data: usize) -> bool {
        false
    }

    fn stuff_extra(_inner: Self::Extra) -> usize {
        0
    }

    fn extract_extra(_data: usize) -> Self::Extra {
        ()
    }
}

#[cfg(test)]
pub mod test_strategies {
    use crate::StuffingStrategy;
    use std::fmt::{Debug, Formatter};

    pub struct HasDebug;

    impl Debug for HasDebug {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.write_str("hello!")
        }
    }

    unsafe impl StuffingStrategy for HasDebug {
        type Extra = Self;

        fn is_extra(data: usize) -> bool {
            data == usize::MAX
        }

        fn stuff_extra(_inner: Self::Extra) -> usize {
            usize::MAX
        }

        fn extract_extra(_data: usize) -> Self::Extra {
            Self
        }
    }
}
