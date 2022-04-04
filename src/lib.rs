#![warn(rust_2018_idioms)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

//! A crate for stuffing things into a pointer.

mod backend;
pub mod strategies;

use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::mem;
use std::ops::Not;

use crate::backend::Backend;
use sptr::Strict;

/// A union of a pointer and some extra data.
pub struct StuffedPtr<T, S, I = usize>(I::Stored, PhantomData<S>)
where
    S: StuffingStrategy<I>,
    I: Backend<T>;

impl<T, S, I> StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    I: Backend<T>,
{
    /// Create a new `StuffedPtr` from a pointer
    pub fn new_ptr(ptr: *mut T) -> Self {
        let addr = Strict::addr(ptr);
        let stuffed = S::stuff_ptr(addr);
        Self(I::set_ptr(ptr, stuffed), PhantomData)
    }

    /// Create a new `StuffPtr` from extra
    pub fn new_extra(extra: S::Extra) -> Self {
        // this doesn't have any provenance, which is ok, since it's never a pointer anyways.
        // if the user calls `set_ptr` it will use the new provenance from that ptr
        let ptr = std::ptr::null_mut();
        let extra = S::stuff_extra(extra);
        Self(I::set_ptr(ptr, extra), PhantomData)
    }

    /// Get the pointer data, or `None` if it contains extra
    pub fn get_ptr(&self) -> Option<*mut T> {
        self.is_extra().not().then(|| {
            // SAFETY: We have done a check that it's not extra
            unsafe { self.get_ptr_unchecked() }
        })
    }

    /// Get the pointer data
    /// # Safety
    /// Must contain pointer data and not extra
    pub unsafe fn get_ptr_unchecked(&self) -> *mut T {
        let (provenance, addr) = I::get_ptr(self.0);
        let addr = S::extract_ptr(addr);
        Strict::with_addr(provenance, addr)
    }

    /// Get owned extra data from this, or `None` if it contains pointer data
    pub fn into_extra(self) -> Option<S::Extra> {
        self.is_extra().then(|| {
            // SAFETY: We checked that it contains an extra above
            unsafe { self.into_extra_unchecked() }
        })
    }

    /// Get owned extra data from this
    /// # Safety
    /// Must contain extra data and not pointer
    pub unsafe fn into_extra_unchecked(self) -> S::Extra {
        // SAFETY: `self` is consumed and forgotten after this call
        let extra = unsafe { self.get_extra_unchecked() };
        mem::forget(self);
        extra
    }

    /// Get extra data from this, or `None` if it contains pointer data
    /// # Safety
    /// The caller must guarantee that only ever on `Extra` exists if `Extra: !Copy`
    pub unsafe fn get_extra(&self) -> Option<S::Extra> {
        self.is_extra().then(|| {
            // SAFETY: We checked that it contains extra above, the caller guarantees the rest
            unsafe { self.get_extra_unchecked() }
        })
    }

    /// Get extra data from this
    /// # Safety
    /// Must contain extra data and not pointer data,
    /// and the caller must guarantee that only ever on `Extra` exists if `Extra: !Copy`
    pub unsafe fn get_extra_unchecked(&self) -> S::Extra {
        let data = self.addr();
        unsafe { S::extract_extra(data) }
    }

    fn addr(&self) -> I {
        I::get_int(self.0)
    }

    fn is_extra(&self) -> bool {
        S::is_extra(self.addr())
    }
}

impl<T, S, I> StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    S::Extra: Copy,
    I: Backend<T>,
{
    /// Get extra data from this, or `None` if it's pointer data
    pub fn copy_extra(&self) -> Option<S::Extra> {
        // SAFETY: `S::Extra: Copy`
        unsafe { self.get_extra() }
    }

    /// Get extra data from this
    /// # Safety
    /// Must contain extra data and not pointer data,
    pub unsafe fn copy_extra_unchecked(&self) -> S::Extra {
        // SAFETY: `S::Extra: Copy`, and the caller guarantees that it's extra
        unsafe { self.get_extra_unchecked() }
    }
}

impl<T, S, I> Debug for StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    S::Extra: Debug,
    I: Backend<T>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // SAFETY:
        // If S::Extra: !Copy, we can't just copy it out and call it a day
        // For example, if it's a Box, not forgetting it here would lead to a double free
        // So we just format it and forget it afterwards
        if let Some(extra) = unsafe { self.get_extra() } {
            f.debug_struct("StuffedPtr::Extra")
                .field("extra", &extra)
                .finish()?;
            mem::forget(extra);
            Ok(())
        } else {
            // SAFETY: Checked above
            let ptr = unsafe { self.get_ptr_unchecked() };
            f.debug_struct("StuffedPtr::Ptr")
                .field("ptr", &ptr)
                .finish()
        }
    }
}

impl<T, S, I> Drop for StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    I: Backend<T>,
{
    fn drop(&mut self) {
        if self.is_extra() {
            // SAFETY: We move it out here and it's never accessed again.
            let extra = unsafe { self.get_extra_unchecked() };
            drop(extra);
        } else {
            // dropping a ptr is a no-op
        }
    }
}

impl<T, S, I> Clone for StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    S::Extra: Clone,
    I: Backend<T>,
{
    fn clone(&self) -> Self {
        // SAFETY: We forget that `extra` ever existed after taking the reference and cloning it
        if let Some(extra) = unsafe { self.get_extra() } {
            let cloned_extra = extra.clone();
            mem::forget(extra);
            Self::new_extra(cloned_extra)
        } else {
            // just copy the pointer
            Self(self.0, PhantomData)
        }
    }
}

impl<T, S, I> PartialEq for StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    S::Extra: PartialEq,
    I: Backend<T>,
{
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: We forget them after
        let extras = unsafe { (self.get_extra(), other.get_extra()) };

        let eq = match &extras {
            (Some(extra1), Some(extra2)) => extra1.eq(extra2),
            (None, None) => {
                // SAFETY: `get_extra` returned `None`, so it must be a ptr
                unsafe {
                    let ptr1 = self.get_ptr_unchecked();
                    let ptr2 = self.get_ptr_unchecked();
                    std::ptr::eq(ptr1, ptr2)
                }
            }
            _ => false,
        };

        mem::forget(extras);

        eq
    }
}

impl<T, S, I> Eq for StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    S::Extra: PartialEq + Eq,
    I: Backend<T>,
{
}

impl<T, S, I> From<Box<T>> for StuffedPtr<T, S, I>
where
    S: StuffingStrategy<I>,
    I: Backend<T>,
{
    fn from(boxed: Box<T>) -> Self {
        Self::new_ptr(Box::into_raw(boxed))
    }
}

/// A trait that describes how to stuff extras and pointers into the pointer sized object.
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

#[cfg(test)]
mod tests {
    use crate::strategies::test_strategies::{EmptyInMax, HasDebug, PanicsInDrop};
    use crate::StuffedPtr;
    use std::mem;

    // note: the tests mostly use the `PanicsInDrop` type and strategy, to make sure that no
    // extra is ever dropped accidentally.

    #[test]
    fn set_get_ptr_no_extra() {
        unsafe {
            let boxed = Box::new(1);
            let stuffed_ptr: StuffedPtr<i32, ()> = boxed.into();
            let ptr = stuffed_ptr.get_ptr_unchecked();
            let boxed = Box::from_raw(ptr);
            assert_eq!(*boxed, 1);
        }
    }

    #[test]
    fn get_extra() {
        let stuffed_ptr: StuffedPtr<(), EmptyInMax> = StuffedPtr::new_extra(EmptyInMax);
        assert!(stuffed_ptr.is_extra());
        assert!(matches!(stuffed_ptr.copy_extra(), Some(EmptyInMax)));
    }

    #[test]
    fn debug() {
        let boxed = Box::new(1);
        let stuffed_ptr: StuffedPtr<i32, HasDebug> = boxed.into();
        assert!(format!("{stuffed_ptr:?}").starts_with("StuffedPtr::Ptr {"));

        drop(unsafe { Box::from_raw(stuffed_ptr.get_ptr().unwrap()) });

        let extra = HasDebug;
        let stuffed_ptr: StuffedPtr<i32, HasDebug> = StuffedPtr::new_extra(extra);
        assert_eq!(
            format!("{stuffed_ptr:?}"),
            "StuffedPtr::Extra { extra: hello! }"
        );
    }

    #[test]
    #[should_panic]
    fn drop_extra_when_extra() {
        let stuffed_ptr: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_extra(PanicsInDrop);
        // the panicking drop needs to be called here!
        drop(stuffed_ptr);
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn clone() {
        let mut unit = ();
        let stuffed_ptr1: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_ptr(&mut unit);
        let _ = stuffed_ptr1.clone();

        let stuffed_ptr1: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_extra(PanicsInDrop);
        let stuffed_ptr2 = stuffed_ptr1.clone();

        mem::forget((stuffed_ptr1, stuffed_ptr2));
    }

    #[test]
    fn eq() {
        // two pointers
        let mut unit = ();
        let stuffed_ptr1: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_ptr(&mut unit);
        let stuffed_ptr2: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_ptr(&mut unit);

        assert_eq!(stuffed_ptr1, stuffed_ptr2);

        let stuffed_ptr1: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_ptr(&mut unit);
        let stuffed_ptr2: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_extra(PanicsInDrop);

        assert_ne!(stuffed_ptr1, stuffed_ptr2);
        mem::forget(stuffed_ptr2);
    }

    #[test]
    fn dont_drop_extra_when_pointer() {
        let mut unit = ();
        let stuffed_ptr: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_ptr(&mut unit);
        // the panicking drop needs not to be called here!
        drop(stuffed_ptr);
    }

    #[test]
    fn some_traits_dont_drop() {
        // make sure that extra is never dropped twice

        let stuffed_ptr1: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_extra(PanicsInDrop);
        let stuffed_ptr2: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_extra(PanicsInDrop);

        // PartialEq
        assert_eq!(stuffed_ptr1, stuffed_ptr2);
        // Debug
        let _ = format!("{stuffed_ptr1:?}");

        mem::forget((stuffed_ptr1, stuffed_ptr2));
    }
}
