#![no_std]
#![warn(rust_2018_idioms)]
#![warn(missing_docs)]
#![deny(clippy::disallowed_methods, clippy::undocumented_unsafe_blocks)]

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
//! use std::mem::ManuallyDrop;
//!
//! use stuff::{StuffedPtr, StuffingStrategy, Unstuffed};
//!
//! // Create a unit struct for our strategy
//! struct NanBoxStrategy;
//!
//! // implementation detail of NaN boxing, a quiet NaN mask
//! const QNAN: u64 = 0x7ffc000000000000;
//! // implementation detail of NaN boxing, the sign bit of an f64
//! const SIGN_BIT: u64 = 0x8000000000000000;
//!
//! impl StuffingStrategy<u64> for NanBoxStrategy {
//!     type Other = f64;
//!
//!     fn stuff_other(inner: Self::Other) -> u64 {
//!         unsafe { std::mem::transmute(inner) } // both are 64 bit POD's
//!     }
//!
//!     fn extract(data: u64) -> Unstuffed<usize, f64> {
//!         if (data & QNAN) != QNAN {
//!             Unstuffed::Other(f64::from_bits(data))
//!         } else {
//!             Unstuffed::Ptr((data & !(SIGN_BIT | QNAN)).try_into().unwrap())
//!         }
//!     }
//!
//!     fn stuff_ptr(addr: usize) -> u64 {
//!         // add the QNAN and SIGN_BIT
//!         SIGN_BIT | QNAN | u64::try_from(addr).unwrap()
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
//! assert_eq!(float.other(), Some(123.5));
//!
//! let object: Object = HashMap::from([("a".to_owned(), 457)]);
//! let boxed = Box::new(object);
//! let ptr: Value = StuffedPtr::new_ptr(Box::into_raw(boxed));
//!
//! let object = unsafe { &*ptr.ptr().unwrap() };
//! assert_eq!(object.get("a"), Some(&457));
//!
//! drop(unsafe { Box::from_raw(ptr.ptr().unwrap()) });
//!
//! // be careful, `ptr` is a dangling pointer now!
//! ```

#[cfg(test)]
extern crate std;

mod backend;
mod strategy;

#[cfg(any())]
mod tag;

use core::{
    fmt::{Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use sptr::Strict;

pub use crate::{backend::Backend, either::Unstuffed, strategy::StuffingStrategy};

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
/// `StuffedPtr` implements most traits like `Hash` or `PartialEq` if the `other` type does.
/// It's also always `Copy`, and therefore requires the other type to be `Copy` as well.
///
/// This type is guaranteed to be `#[repr(transparent)]` to a `B::Stored`.
#[repr(transparent)]
pub struct StuffedPtr<T, S, B = usize>(B::Stored, PhantomData<Unstuffed<*mut T, S>>)
where
    B: Backend;

impl<T, S, B> StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    B: Backend,
{
    /// Create a new `StuffedPtr` from a pointer
    pub fn new_ptr(ptr: *mut T) -> Self {
        let addr = Strict::addr(ptr);
        let stuffed = S::stuff_ptr(addr);
        StuffedPtr(B::set_ptr(ptr.cast::<()>(), stuffed), PhantomData)
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
    pub fn ptr(&self) -> Option<*mut T> {
        let (provenance, stored) = B::get_ptr(self.0);
        let addr = S::extract(stored).ptr()?;
        Some(Strict::with_addr(provenance.cast::<T>(), addr))
    }

    /// Get `other` data from this, or `None` if it contains pointer data
    pub fn other(&self) -> Option<S::Other> {
        let data = self.addr();
        S::extract(data).other()
    }

    /// Get out the unstuffed enum representation
    pub fn unstuff(&self) -> Unstuffed<*mut T, S::Other> {
        let (provenance, stored) = B::get_ptr(self.0);
        let either = S::extract(stored);
        either.map_ptr(|addr| Strict::with_addr(provenance.cast::<T>(), addr))
    }

    fn addr(&self) -> B {
        B::get_int(self.0)
    }
}

impl<T, S, B> Debug for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Debug,
    B: Backend,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self.unstuff() {
            Unstuffed::Ptr(ptr) => f.debug_tuple("Ptr").field(&ptr).finish(),
            Unstuffed::Other(other) => f.debug_tuple("Other").field(&other).finish(),
        }
    }
}

impl<T, S, B> Clone for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    B: Backend,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, S, B> Copy for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    B: Backend,
{
}

impl<T, S, B> PartialEq for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: PartialEq,
    B: Backend,
{
    fn eq(&self, other: &Self) -> bool {
        match (self.unstuff(), other.unstuff()) {
            (Unstuffed::Ptr(a), Unstuffed::Ptr(b)) => core::ptr::eq(a, b),
            (Unstuffed::Other(a), Unstuffed::Other(b)) => a == b,
            _ => false,
        }
    }
}

impl<T, S, B> Eq for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: PartialEq + Eq,
    B: Backend,
{
}

impl<T, S, B> Hash for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Hash,
    B: Backend,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.unstuff() {
            Unstuffed::Ptr(ptr) => {
                ptr.hash(state);
            }
            Unstuffed::Other(other) => {
                other.hash(state);
            }
        }
    }
}

mod either {
    /// The enum representation of a `StuffedPtr`
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Unstuffed<P, O> {
        /// The pointer or pointer address
        Ptr(P),
        /// The other type
        Other(O),
    }

    impl<P: Copy, O> Unstuffed<P, O> {
        /// Get the pointer, or `None` if it's the other
        pub fn ptr(&self) -> Option<P> {
            match *self {
                Self::Ptr(ptr) => Some(ptr),
                Self::Other(_) => None,
            }
        }

        /// Get the other type, or `None` if it's the pointer
        pub fn other(self) -> Option<O> {
            match self {
                Self::Ptr(_) => None,
                Self::Other(other) => Some(other),
            }
        }

        /// Maps the pointer type if it's a pointer, or does nothing if it's other
        pub fn map_ptr<U>(self, f: impl FnOnce(P) -> U) -> Unstuffed<U, O> {
            match self {
                Self::Ptr(ptr) => Unstuffed::Ptr(f(ptr)),
                Self::Other(other) => Unstuffed::Other(other),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use std::{boxed::Box, format, println};

    use paste::paste;

    use crate::{
        strategy::test_strategies::{EmptyInMax, HasDebug},
        Backend, StuffedPtr, StuffingStrategy,
    };

    fn from_box<T, S, B>(boxed: Box<T>) -> StuffedPtr<T, S, B>
    where
        S: StuffingStrategy<B>,
        B: Backend,
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
                        let ptr = stuffed_ptr.ptr().unwrap();
                        let boxed = Box::from_raw(ptr);
                        assert_eq!(*boxed, 1);
                    }
                }


                #[test]
                fn [<get_other__ $backend>]() {
                    let stuffed_ptr: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_other(EmptyInMax);
                    assert!(stuffed_ptr.other().is_some());
                    assert!(matches!(stuffed_ptr.other(), Some(EmptyInMax)));
                }

                #[test]
                fn [<debug__ $backend>]() {
                    let boxed = Box::new(1);
                    let stuffed_ptr: StuffedPtr<i32, HasDebug, $backend> = from_box(boxed);
                    println!("{stuffed_ptr:?}");
                    assert!(format!("{stuffed_ptr:?}").starts_with("Ptr("));

                    drop(unsafe { Box::from_raw(stuffed_ptr.ptr().unwrap()) });

                    let other = HasDebug;
                    let stuffed_ptr: StuffedPtr<i32, HasDebug, $backend> = StuffedPtr::new_other(other);
                    assert_eq!(
                        format!("{stuffed_ptr:?}"),
                        "Other(hello!)"
                    );
                }

                #[test]
                #[allow(clippy::redundant_clone)]
                fn [<clone__ $backend>]() {
                    let mut unit = ();
                    let stuffed_ptr1: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_ptr(&mut unit);
                    let _ = stuffed_ptr1.clone();

                    let stuffed_ptr1: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_other(EmptyInMax);
                    let _ = stuffed_ptr1.clone();
                }


                #[test]
                fn [<eq__ $backend>]() {
                    // two pointers
                    let mut unit = ();
                    let stuffed_ptr1: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_ptr(&mut unit);
                    let stuffed_ptr2: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_ptr(&mut unit);

                    assert_eq!(stuffed_ptr1, stuffed_ptr2);

                    let stuffed_ptr1: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_ptr(&mut unit);
                    let stuffed_ptr2: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_other(EmptyInMax);

                    assert_ne!(stuffed_ptr1, stuffed_ptr2);
                }

            }
        };
    }

    make_tests!(u128);
    make_tests!(u64);
    make_tests!(usize);
}
