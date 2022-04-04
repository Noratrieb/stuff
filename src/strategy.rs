/// A trait that describes how to stuff extras and pointers into the pointer sized object.
///
/// This trait is what a user of this crate is expected to implement to use the crate for their own
/// pointer stuffing. It's usually implemented on ZSTs that only serve as stuffing strategies, but
/// it's also completely possible to implement it on the type in [`StuffingStrategy::Extra`] directly
/// if possible.
///
/// # Safety
///
/// If [`StuffingStrategy::is_extra`] returns true for a value, then
/// [`StuffingStrategy::extract_extra`] *must* return a valid `Extra` for that same value.
///
/// [`StuffingStrategy::stuff_extra`] *must* consume `inner` and make sure that it's not dropped
/// if it isn't `Copy`.
///
/// For [`StuffingStrategy::stuff_ptr`] and [`StuffingStrategy::extract_ptr`],
/// `ptr == extract_ptr(stuff_ptr(ptr))` *must* hold true.
pub unsafe trait StuffingStrategy<I> {
    /// The type of the extra.
    type Extra;

    /// Checks whether the `StufferPtr` data value contains an extra value. The result of this
    /// function can be trusted.
    fn is_extra(data: I) -> bool;

    /// Stuff extra data into a usize that is then put into the pointer. This operation
    /// must be infallible.
    fn stuff_extra(inner: Self::Extra) -> I;

    /// Extract extra data from the data.
    /// # Safety
    /// `data` must contain data created by [`StuffingStrategy::stuff_extra`].
    unsafe fn extract_extra(data: I) -> Self::Extra;

    /// Stuff a pointer address into the pointer sized integer.
    ///
    /// This can be used to optimize away some of the unnecessary parts of the pointer or do other
    /// cursed things with it.
    ///
    /// The default implementation just returns the address directly.
    fn stuff_ptr(addr: usize) -> I;

    /// Extract the pointer address from the data.
    ///
    /// This function expects `inner` to come directly from [`StuffingStrategy::stuff_ptr`].
    fn extract_ptr(inner: I) -> usize;
}

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

unsafe impl StuffingStrategy<u64> for () {
    type Extra = ();

    fn is_extra(_data: u64) -> bool {
        false
    }

    fn stuff_extra(_inner: Self::Extra) -> u64 {
        0
    }

    unsafe fn extract_extra(_data: u64) -> Self::Extra {}

    fn stuff_ptr(addr: usize) -> u64 {
        addr as u64
    }

    fn extract_ptr(inner: u64) -> usize {
        inner as usize
    }
}

unsafe impl StuffingStrategy<u128> for () {
    type Extra = ();

    fn is_extra(_data: u128) -> bool {
        false
    }

    fn stuff_extra(_inner: Self::Extra) -> u128 {
        0
    }

    unsafe fn extract_extra(_data: u128) -> Self::Extra {}

    fn stuff_ptr(addr: usize) -> u128 {
        addr as u128
    }

    fn extract_ptr(inner: u128) -> usize {
        inner as usize
    }
}

#[cfg(test)]
pub(crate) mod test_strategies {
    use core::fmt::{Debug, Formatter};

    use super::StuffingStrategy;

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
                    core::mem::forget(inner);
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
            unsafe impl StuffingStrategy<u64> for $ty {
                type Extra = Self;

                fn is_extra(data: u64) -> bool {
                    data == u64::MAX
                }

                #[allow(clippy::forget_copy)]
                fn stuff_extra(inner: Self::Extra) -> u64 {
                    core::mem::forget(inner);
                    u64::MAX
                }

                unsafe fn extract_extra(_data: u64) -> Self::Extra {
                    $ty
                }

                fn stuff_ptr(addr: usize) -> u64 {
                    addr as u64
                }

                fn extract_ptr(inner: u64) -> usize {
                    inner as usize
                }
            }

            unsafe impl StuffingStrategy<u128> for $ty {
                type Extra = Self;

                fn is_extra(data: u128) -> bool {
                    data == u128::MAX
                }

                #[allow(clippy::forget_copy)]
                fn stuff_extra(inner: Self::Extra) -> u128 {
                    core::mem::forget(inner);
                    u128::MAX
                }

                unsafe fn extract_extra(_data: u128) -> Self::Extra {
                    $ty
                }

                fn stuff_ptr(addr: usize) -> u128 {
                    addr as u128
                }

                fn extract_ptr(inner: u128) -> usize {
                    inner as usize
                }
            }
        };
    }

    #[derive(Clone, Copy)]
    pub struct EmptyInMax;

    impl_usize_max_zst!(EmptyInMax);

    pub struct HasDebug;

    impl Debug for HasDebug {
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
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
