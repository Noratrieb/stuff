use core::convert::TryInto;

use crate::Backend;

/// A trait that describes how to stuff others and pointers into the pointer sized object.
///
/// This trait is what a user of this crate is expected to implement to use the crate for their own
/// pointer stuffing. It's usually implemented on ZSTs that only serve as stuffing strategies, but
/// it's also completely possible to implement it on the type in [`StuffingStrategy::Other`] directly
/// if possible.
///
/// The generic parameter `B` stands for the [`Backend`](`crate::Backend`) used by the strategy.
///
/// # Safety
///
/// If [`StuffingStrategy::is_other`] returns true for a value, then
/// [`StuffingStrategy::extract_other`] *must* return a valid `Other` for that same value.
///
/// [`StuffingStrategy::stuff_other`] *must* consume `inner` and make sure that it's not dropped
/// if it isn't `Copy`.
///
/// For [`StuffingStrategy::stuff_ptr`] and [`StuffingStrategy::extract_ptr`],
/// `ptr == extract_ptr(stuff_ptr(ptr))` *must* hold true.
pub unsafe trait StuffingStrategy<B> {
    /// The type of the other.
    type Other;

    /// Checks whether the `StufferPtr` data value contains an other value. The result of this
    /// function can be trusted.
    fn is_other(data: B) -> bool;

    /// Stuff other data into a usize that is then put into the pointer. This operation
    /// must be infallible.
    fn stuff_other(inner: Self::Other) -> B;

    /// Extract other data from the data.
    /// # Safety
    /// `data` must contain data created by [`StuffingStrategy::stuff_other`].
    unsafe fn extract_other(data: B) -> Self::Other;

    /// Stuff a pointer address into the pointer sized integer.
    ///
    /// This can be used to optimize away some of the unnecessary parts of the pointer or do other
    /// cursed things with it.
    ///
    /// The default implementation just returns the address directly.
    fn stuff_ptr(addr: usize) -> B;

    /// Extract the pointer address from the data.
    ///
    /// This function expects `inner` to come directly from [`StuffingStrategy::stuff_ptr`].
    fn extract_ptr(inner: B) -> usize;
}

unsafe impl<B> StuffingStrategy<B> for ()
where
    B: Backend<()> + Default + TryInto<usize>,
    usize: TryInto<B>,
{
    type Other = ();

    fn is_other(_data: B) -> bool {
        false
    }

    fn stuff_other(_inner: Self::Other) -> B {
        B::default()
    }

    unsafe fn extract_other(_data: B) -> Self::Other {}

    fn stuff_ptr(addr: usize) -> B {
        addr.try_into()
            .unwrap_or_else(|_| panic!("Address in `stuff_ptr` too big"))
    }

    fn extract_ptr(inner: B) -> usize {
        inner
            .try_into()
            // note: this can't happen 🤔
            .unwrap_or_else(|_| panic!("Pointer value too big for usize"))
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
                type Other = Self;

                fn is_other(data: usize) -> bool {
                    data == usize::MAX
                }

                #[allow(clippy::forget_copy)]
                fn stuff_other(inner: Self::Other) -> usize {
                    core::mem::forget(inner);
                    usize::MAX
                }

                unsafe fn extract_other(_data: usize) -> Self::Other {
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
                type Other = Self;

                fn is_other(data: u64) -> bool {
                    data == u64::MAX
                }

                #[allow(clippy::forget_copy)]
                fn stuff_other(inner: Self::Other) -> u64 {
                    core::mem::forget(inner);
                    u64::MAX
                }

                unsafe fn extract_other(_data: u64) -> Self::Other {
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
                type Other = Self;

                fn is_other(data: u128) -> bool {
                    data == u128::MAX
                }

                #[allow(clippy::forget_copy)]
                fn stuff_other(inner: Self::Other) -> u128 {
                    core::mem::forget(inner);
                    u128::MAX
                }

                unsafe fn extract_other(_data: u128) -> Self::Other {
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
