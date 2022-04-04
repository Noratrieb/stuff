//! Several pre-defined strategies to use with `StuffedPtr`.
//!
//! * `()`: An empty strategy, is always the pointer

use crate::StuffingStrategy;

unsafe impl StuffingStrategy<usize> for () {
    type Extra = ();

    fn is_extra(_data: usize) -> bool {
        false
    }

    fn stuff_extra(_inner: Self::Extra) -> usize {
        0
    }

    unsafe fn extract_extra(_data: usize) -> Self::Extra {}

    fn stuff_ptr(addr: usize) -> usize {
        addr
    }

    fn extract_ptr(inner: usize) -> usize {
        inner
    }
}

#[cfg(test)]
pub(crate) mod test_strategies {
    use crate::StuffingStrategy;
    use std::fmt::{Debug, Formatter};

    macro_rules! impl_usize_max_zst {
        ($ty:ident) => {
            // this one lives in usize::MAX
            unsafe impl StuffingStrategy<usize> for $ty {
                type Extra = Self;

                fn is_extra(data: usize) -> bool {
                    data == usize::MAX
                }

                #[allow(clippy::forget_copy)]
                fn stuff_extra(inner: Self::Extra) -> usize {
                    std::mem::forget(inner);
                    usize::MAX
                }

                unsafe fn extract_extra(_data: usize) -> Self::Extra {
                    $ty
                }

                fn stuff_ptr(addr: usize) -> usize {
                    addr
                }

                fn extract_ptr(inner: usize) -> usize {
                    inner
                }
            }
        };
    }

    #[derive(Clone, Copy)]
    pub struct EmptyInMax;

    impl_usize_max_zst!(EmptyInMax);

    pub struct HasDebug;

    impl Debug for HasDebug {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.write_str("hello!")
        }
    }

    impl_usize_max_zst!(HasDebug);

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PanicsInDrop;

    impl Drop for PanicsInDrop {
        fn drop(&mut self) {
            panic!("oh no!!!");
        }
    }

    impl_usize_max_zst!(PanicsInDrop);
}
