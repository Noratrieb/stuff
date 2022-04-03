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

    macro_rules! impl_usize_max_zst {
        ($ty:ident) => {
            // this one lives in usize::MAX
            unsafe impl StuffingStrategy for $ty {
                type Extra = Self;

                fn is_extra(data: usize) -> bool {
                    data == usize::MAX
                }

                fn stuff_extra(_inner: Self::Extra) -> usize {
                    usize::MAX
                }

                fn extract_extra(_data: usize) -> Self::Extra {
                    $ty
                }
            }
        };
    }

    pub struct HasDebug;

    impl Debug for HasDebug {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.write_str("hello!")
        }
    }

    impl_usize_max_zst!(HasDebug);

    pub struct PanicsInDrop;

    impl Drop for PanicsInDrop {
        fn drop(&mut self) {
            panic!("oh no!!!");
        }
    }

    impl_usize_max_zst!(PanicsInDrop);
}
