#![no_std]
#![warn(rust_2018_idioms)]
#![warn(missing_docs)]

//! A crate for stuffing things into a pointer.
//!
//! `stuff` helps you to
//!
//! - Stuff arbitrary data into pointers
//! - Stuff pointers or arbitrary data into fixed size storage (u64, u128)
//!
//! in a **portable and provenance friendly** way.
//!
//! [`StuffedPtr`] is the main type of this crate. You it's a type whose size depends on the
//! choice of [`Backend`] (defaults to `usize`, `u64` and `u128` are also possible). It can store a
//! pointer or some `other` data.
//!
//! You can choose any arbitrary bitstuffing depending on the [`StuffingStrategy`], an unsafe trait that governs
//! how the `other` data (or the pointer itself) will be packed into the backend. While this trait is still unsafe,
//! it's a lot safer than doing everything by hand.
//!
//! # Example: NaN-Boxing
//! Pointers are hidden in the NaN values of floats. NaN boxing often involves also hiding booleans
//! or null in there, but we stay with floats and pointers (pointers to a `HashMap` that servers
//! as our "object" type).
//!
//! See [crafting interpreters](https://craftinginterpreters.com/optimization.html#nan-boxing)
//! for more details.
//! ```
//! use std::collections::HashMap;
//! # use std::convert::{TryFrom, TryInto};
//!
//! use stuff::{StuffedPtr, StuffingStrategy};
//!
//! // Create a unit struct for our strategy
//! struct NanBoxStrategy;
//!
//! // implementation detail of NaN boxing, a quiet NaN mask
//! const QNAN: u64 = 0x7ffc000000000000;
//! // implementation detail of NaN boxing, the sign bit of an f64
//! const SIGN_BIT: u64 = 0x8000000000000000;
//!
//! unsafe impl StuffingStrategy<u64> for NanBoxStrategy {
//!     type Other = f64;
//!
//!     fn is_other(data: u64) -> bool {
//!         (data & QNAN) != QNAN
//!     }
//!
//!     fn stuff_other(inner: Self::Other) -> u64 {
//!         unsafe { std::mem::transmute(inner) } // both are 64 bit POD's
//!     }
//!
//!     unsafe fn extract_other(data: u64) -> Self::Other {
//!         std::mem::transmute(data) // both are 64 bit POD's
//!     }
//!
//!     fn stuff_ptr(addr: usize) -> u64 {
//!         // add the QNAN and SIGN_BIT
//!         SIGN_BIT | QNAN | u64::try_from(addr).unwrap()
//!     }
//!
//!     fn extract_ptr(inner: u64) -> usize {
//!         // keep everything except for QNAN and SIGN_BIT
//!         (inner & !(SIGN_BIT | QNAN)).try_into().unwrap()
//!     }
//! }
//!
//! // a very, very crude representation of an object
//! type Object = HashMap<String, u32>;
//!
//! // our value type
//! type Value = StuffedPtr<Object, NanBoxStrategy, u64>;
//!
//! let float: Value = StuffedPtr::new_other(123.5);
//! assert_eq!(float.copy_other(), Some(123.5));
//!
//! let object: Object = HashMap::from([("a".to_owned(), 457)]);
//! let boxed = Box::new(object);
//! let ptr: Value = StuffedPtr::new_ptr(Box::into_raw(boxed));
//!
//! let object = unsafe { &*ptr.get_ptr().unwrap() };
//! assert_eq!(object.get("a"), Some(&457));
//!
//! drop(unsafe { Box::from_raw(ptr.get_ptr().unwrap()) });
//!
//! // be careful, `ptr` is a dangling pointer now!
//! ```

#[cfg(test)]
extern crate alloc; // we want that for tests so we can use `Box`

mod backend;
mod strategy;
mod tag;

use core::{
    fmt::{Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem,
    ops::Not,
};

use sptr::Strict;

pub use crate::{backend::Backend, strategy::StuffingStrategy};

/// A union of a pointer or some `other` data, bitpacked into a value with the size depending on
/// `B`. It defaults to `usize`, meaning pointer sized, but `u64` and `u128` are also provided
/// by this crate. You can also provide your own [`Backend`] implementation
///
/// The stuffing strategy is supplied as the second generic parameter `S`.
///
/// The first generic parameter `T` is the type that the pointer is pointing to.
///
/// For a usage example, view the crate level documentation.
///
/// This pointer does *not* drop `other` data, [`StuffedPtr::into_other`] can be used if that is required.
///
/// `StuffedPtr` implements most traits like `Clone`, `PartialEq` or `Copy` if the `other` type does.
///
/// This type is guaranteed to be `#[repr(transparent)]` to a `B::Stored`.
#[repr(transparent)]
pub struct StuffedPtr<T, S, B = usize>(B::Stored, PhantomData<S>)
where
    S: StuffingStrategy<B>,
    B: Backend<T>;

impl<T, S, B> StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    B: Backend<T>,
{
    /// Create a new `StuffedPtr` from a pointer
    pub fn new_ptr(ptr: *mut T) -> Self {
        let addr = Strict::addr(ptr);
        let stuffed = S::stuff_ptr(addr);
        StuffedPtr(B::set_ptr(ptr, stuffed), PhantomData)
    }

    /// Create a new `StuffPtr` from `other` data
    pub fn new_other(other: S::Other) -> Self {
        // this doesn't have any provenance, which is ok, since it's never a pointer anyways.
        // if the user calls `set_ptr` it will use the new provenance from that ptr
        let ptr = core::ptr::null_mut();
        let other = S::stuff_other(other);
        StuffedPtr(B::set_ptr(ptr, other), PhantomData)
    }

    /// Get the pointer data, or `None` if it contains `other` data
    pub fn get_ptr(&self) -> Option<*mut T> {
        match self.is_other().not() {
            true => {
                // SAFETY: We have done a check that it's not other
                unsafe { Some(self.get_ptr_unchecked()) }
            }
            false => None,
        }
    }

    /// Get the unstuffed pointer data from the stuffed pointer, assuming that the `StuffedPtr`
    /// contains pointer data.
    ///
    /// # Safety
    /// `StuffedPtr` must contain pointer data and not `other` data
    pub unsafe fn get_ptr_unchecked(&self) -> *mut T {
        let (provenance, addr) = B::get_ptr(self.0);
        let addr = S::extract_ptr(addr);
        Strict::with_addr(provenance, addr)
    }

    /// Get owned `other` data from this, or `None` if it contains pointer data
    pub fn into_other(self) -> Option<S::Other> {
        match self.is_other() {
            true => {
                // SAFETY: We checked that it contains an other above
                unsafe { Some(self.into_other_unchecked()) }
            }
            false => None,
        }
    }

    /// Turn this pointer into `other` data.
    /// # Safety
    /// `StuffedPtr` must contain `other` data and not pointer
    pub unsafe fn into_other_unchecked(self) -> S::Other {
        // SAFETY: `self` is consumed and forgotten after this call
        let other = self.get_other_unchecked();
        mem::forget(self);
        other
    }

    /// Get `other` data from this, or `None` if it contains pointer data
    /// # Safety
    /// The caller must guarantee that only ever on `Other` exists if `Other: !Copy`
    pub unsafe fn get_other(&self) -> Option<S::Other> {
        match self.is_other() {
            true => {
                // SAFETY: We checked that it contains other above, the caller guarantees the rest
                Some(self.get_other_unchecked())
            }
            false => None,
        }
    }

    /// Get `other` data from this
    /// # Safety
    /// Must contain `other` data and not pointer data,
    /// and the caller must guarantee that only ever on `Other` exists if `Other: !Copy`
    pub unsafe fn get_other_unchecked(&self) -> S::Other {
        let data = self.addr();
        S::extract_other(data)
    }

    fn addr(&self) -> B {
        B::get_int(self.0)
    }

    fn is_other(&self) -> bool {
        S::is_other(self.addr())
    }
}

/// Extra implementations if the `other` type is `Copy`
impl<T, S, B> StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Copy,
    B: Backend<T>,
{
    /// Get `other` data from this, or `None` if it's pointer data
    pub fn copy_other(&self) -> Option<S::Other> {
        // SAFETY: `S::Other: Copy`
        unsafe { self.get_other() }
    }

    /// Get `other` data from this
    /// # Safety
    /// Must contain `other` data and not pointer data,
    pub unsafe fn copy_other_unchecked(&self) -> S::Other {
        // SAFETY: `S::Other: Copy`, and the caller guarantees that it's other
        self.get_other_unchecked()
    }
}

impl<T, S, B> Debug for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Debug,
    B: Backend<T>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        // SAFETY:
        // If S::Other: !Copy, we can't just copy it out and call it a day
        // For example, if it's a Box, not forgetting it here would lead to a double free
        // So we just format it and forget it afterwards
        if let Some(other) = unsafe { self.get_other() } {
            f.debug_struct("StuffedPtr::Other")
                .field("other", &other)
                .finish()?;
            mem::forget(other);
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

impl<T, S, B> Clone for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Clone,
    B: Backend<T>,
{
    fn clone(&self) -> Self {
        // SAFETY: We forget that `other` ever existed after taking the reference and cloning it
        if let Some(other) = unsafe { self.get_other() } {
            let cloned_other = other.clone();
            mem::forget(other);
            Self::new_other(cloned_other)
        } else {
            // just copy the pointer
            StuffedPtr(self.0, PhantomData)
        }
    }
}

impl<T, S, B> Copy for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Copy,
    B: Backend<T>,
{
}

impl<T, S, B> PartialEq for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: PartialEq,
    B: Backend<T>,
{
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: We forget them after
        let others = unsafe { (self.get_other(), other.get_other()) };

        let eq = match &others {
            (Some(other1), Some(other2)) => other1.eq(other2),
            (None, None) => {
                // SAFETY: `get_other` returned `None`, so it must be a ptr
                unsafe {
                    let ptr1 = self.get_ptr_unchecked();
                    let ptr2 = self.get_ptr_unchecked();
                    core::ptr::eq(ptr1, ptr2)
                }
            }
            _ => false,
        };

        mem::forget(others);

        eq
    }
}

impl<T, S, B> Eq for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: PartialEq + Eq,
    B: Backend<T>,
{
}

impl<T, S, B> Hash for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Hash,
    B: Backend<T>,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        // SAFETY: We forget that `other` ever existed after taking the reference and cloning it
        if let Some(other) = unsafe { self.get_other() } {
            other.hash(state);
            mem::forget(other);
        } else {
            // SAFETY: Checked above
            let ptr = unsafe { self.get_ptr_unchecked() };
            ptr.hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use alloc::{boxed::Box, format};
    use core::mem;

    use paste::paste;

    use crate::{
        strategy::test_strategies::{EmptyInMax, HasDebug, PanicsInDrop},
        Backend, StuffedPtr, StuffingStrategy,
    };

    // note: the tests mostly use the `PanicsInDrop` type and strategy, to make sure that no
    // `other` is ever dropped accidentally.

    fn from_box<T, S, B>(boxed: Box<T>) -> StuffedPtr<T, S, B>
    where
        S: StuffingStrategy<B>,
        B: Backend<T>,
    {
        StuffedPtr::new_ptr(Box::into_raw(boxed))
    }

    macro_rules! make_tests {
        ($backend:ident) => {
            paste! {
                #[test]
                fn [<set_get_ptr_no_other__ $backend>]() {
                     unsafe {
                        let boxed = Box::new(1);
                        let stuffed_ptr: StuffedPtr<i32, (), $backend> = from_box(boxed);
                        let ptr = stuffed_ptr.get_ptr_unchecked();
                        let boxed = Box::from_raw(ptr);
                        assert_eq!(*boxed, 1);
                    }
                }


                #[test]
                fn [<get_other__ $backend>]() {
                    let stuffed_ptr: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_other(EmptyInMax);
                    assert!(stuffed_ptr.is_other());
                    assert!(matches!(stuffed_ptr.copy_other(), Some(EmptyInMax)));
                }

                #[test]
                fn [<debug__ $backend>]() {
                    let boxed = Box::new(1);
                    let stuffed_ptr: StuffedPtr<i32, HasDebug, $backend> = from_box(boxed);
                    assert!(format!("{stuffed_ptr:?}").starts_with("StuffedPtr::Ptr {"));

                    drop(unsafe { Box::from_raw(stuffed_ptr.get_ptr().unwrap()) });

                    let other = HasDebug;
                    let stuffed_ptr: StuffedPtr<i32, HasDebug, $backend> = StuffedPtr::new_other(other);
                    assert_eq!(
                        format!("{stuffed_ptr:?}"),
                        "StuffedPtr::Other { other: hello! }"
                    );
                }

                #[test]
                #[allow(clippy::redundant_clone)]
                fn [<clone__ $backend>]() {
                    let mut unit = ();
                    let stuffed_ptr1: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_ptr(&mut unit);
                    let _ = stuffed_ptr1.clone();

                    let stuffed_ptr1: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_other(PanicsInDrop);
                    let stuffed_ptr2 = stuffed_ptr1.clone();

                    mem::forget((stuffed_ptr1, stuffed_ptr2));
                }


                #[test]
                fn [<eq__ $backend>]() {
                    // two pointers
                    let mut unit = ();
                    let stuffed_ptr1: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_ptr(&mut unit);
                    let stuffed_ptr2: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_ptr(&mut unit);

                    assert_eq!(stuffed_ptr1, stuffed_ptr2);

                    let stuffed_ptr1: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_ptr(&mut unit);
                    let stuffed_ptr2: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_other(PanicsInDrop);

                    assert_ne!(stuffed_ptr1, stuffed_ptr2);
                    mem::forget(stuffed_ptr2);
                }


                #[test]
                fn [<dont_drop_other_when_pointer__ $backend>]() {
                    let mut unit = ();
                    let stuffed_ptr: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_ptr(&mut unit);
                    // the panicking drop needs not to be called here!
                    drop(stuffed_ptr);
                }


                #[test]
                fn [<some_traits_dont_drop__ $backend>]() {
                    // make sure that other is never dropped twice

                    let stuffed_ptr1: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_other(PanicsInDrop);
                    let stuffed_ptr2: StuffedPtr<(), PanicsInDrop, $backend> = StuffedPtr::new_other(PanicsInDrop);

                    // PartialEq
                    assert_eq!(stuffed_ptr1, stuffed_ptr2);
                    // Debug
                    let _ = format!("{stuffed_ptr1:?}");

                    mem::forget((stuffed_ptr1, stuffed_ptr2));
                }
            }
        };
    }

    make_tests!(u128);
    make_tests!(u64);
    make_tests!(usize);
}
