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
//! use stuff::{StuffedPtr, StuffingStrategy, Either};
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
//!     fn stuff_other(inner: Self::Other) -> u64 {
//!         unsafe { std::mem::transmute(inner) } // both are 64 bit POD's
//!     }
//!
//!     unsafe fn extract(data: u64) -> Either<usize, ManuallyDrop<f64>> {
//!         if (data & QNAN) != QNAN {
//!             Either::Other(ManuallyDrop::new(f64::from_bits(data)))
//!         } else {
//!             Either::Ptr((data & !(SIGN_BIT | QNAN)).try_into().unwrap())
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
extern crate std;

mod backend;
mod strategy;

#[cfg(any())]
mod tag;

use core::{
    fmt::{Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem::ManuallyDrop,
};

use sptr::Strict;

pub use crate::{backend::Backend, either::Either, guard::Guard, strategy::StuffingStrategy};

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
pub struct StuffedPtr<T, S, B = usize>(B::Stored, PhantomData<Either<*mut T, S>>)
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
    pub fn get_ptr(&self) -> Option<*mut T> {
        let (provenance, stored) = B::get_ptr(self.0);
        let addr = unsafe { S::extract(stored).ptr()? };
        Some(Strict::with_addr(provenance.cast::<T>(), addr))
    }

    /// Get owned `other` data from this, or `None` if it contains pointer data
    pub unsafe fn into_other(self) -> Option<S::Other> {
        let this = ManuallyDrop::new(self);
        // SAFETY: `self` is consumed and forgotten after this call
        let other = this.get_other();
        other.map(|md| ManuallyDrop::into_inner(md))
    }

    /// Get `other` data from this, or `None` if it contains pointer data
    pub unsafe fn get_other(&self) -> Option<ManuallyDrop<S::Other>> {
        let data = self.addr();
        unsafe { S::extract(data).other() }
    }

    pub fn into_inner(self) -> Either<*mut T, S::Other> {
        let (provenance, stored) = B::get_ptr(self.0);
        let either = unsafe { S::extract(stored) };
        either
            .map_ptr(|addr| Strict::with_addr(provenance.cast::<T>(), addr))
            .map_other(|other| ManuallyDrop::into_inner(other))
    }

    pub fn get(&self) -> Either<*mut T, Guard<'_, S::Other>> {
        let (provenance, stored) = B::get_ptr(self.0);
        let either = unsafe { S::extract(stored) };
        either
            .map_ptr(|addr| Strict::with_addr(provenance.cast::<T>(), addr))
            .map_other(|other| Guard::new(other))
    }

    fn addr(&self) -> B {
        B::get_int(self.0)
    }
}

/// Extra implementations if the `other` type is `Copy`
impl<T, S, B> StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Copy,
    B: Backend,
{
    /// Get `other` data from this, or `None` if it's pointer data
    pub fn copy_other(&self) -> Option<S::Other> {
        // SAFETY: `S::Other: Copy`
        unsafe { self.get_other().map(|other| *other) }
    }
}

impl<T, S, B> Debug for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Debug,
    B: Backend,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self.get() {
            Either::Ptr(ptr) => f.debug_tuple("Ptr").field(&ptr).finish(),
            Either::Other(other) => f.debug_tuple("Other").field(other.inner()).finish(),
        }
    }
}

impl<T, S, B> Clone for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Clone,
    B: Backend,
{
    fn clone(&self) -> Self {
        match self.get() {
            Either::Ptr(ptr) => StuffedPtr::new_ptr(ptr),
            Either::Other(other) => {
                let cloned_other = other.inner().clone();
                Self::new_other(cloned_other)
            }
        }
    }
}

impl<T, S, B> Copy for StuffedPtr<T, S, B>
where
    S: StuffingStrategy<B>,
    S::Other: Copy,
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
        match (self.get(), other.get()) {
            (Either::Ptr(a), Either::Ptr(b)) => core::ptr::eq(a, b),
            (Either::Other(a), Either::Other(b)) => a.inner() == b.inner(),
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
        match self.get() {
            Either::Ptr(ptr) => {
                ptr.hash(state);
            }
            Either::Other(other) => {
                other.inner().hash(state);
            }
        }
    }
}

mod either {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Either<P, O> {
        Ptr(P),
        Other(O),
    }

    impl<P: Copy, O> Either<P, O> {
        pub fn ptr(&self) -> Option<P> {
            match *self {
                Self::Ptr(ptr) => Some(ptr),
                Self::Other(_) => None,
            }
        }

        pub fn other(self) -> Option<O> {
            match self {
                Self::Ptr(_) => None,
                Self::Other(other) => Some(other),
            }
        }

        pub fn map_ptr<U>(self, f: impl FnOnce(P) -> U) -> Either<U, O> {
            match self {
                Self::Ptr(ptr) => Either::Ptr(f(ptr)),
                Self::Other(other) => Either::Other(other),
            }
        }

        pub fn map_other<U>(self, f: impl FnOnce(O) -> U) -> Either<P, U> {
            match self {
                Self::Ptr(ptr) => Either::Ptr(ptr),
                Self::Other(other) => Either::Other(f(other)),
            }
        }
    }
}

mod guard {
    use core::{fmt::Debug, marker::PhantomData, mem::ManuallyDrop, ops::Deref};

    use self::no_alias::AllowAlias;

    mod no_alias {
        use core::{fmt::Debug, hash::Hash, mem::MaybeUninit, ops::Deref};

        /// A `T` except with aliasing problems removed
        ///
        /// this is very sketchy and actually kinda equivalent to whatever ralf wants to add to ManuallyDrop
        /// so idk but whatever this is a great type
        pub struct AllowAlias<T>(MaybeUninit<T>);

        impl<T> AllowAlias<T> {
            pub fn new(value: T) -> Self {
                Self(MaybeUninit::new(value))
            }

            pub fn into_inner(self) -> T {
                unsafe { self.0.assume_init() }
            }
        }

        impl<T: Debug> Debug for AllowAlias<T> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple("NoAlias").field(&self.0).finish()
            }
        }

        impl<T: PartialEq> PartialEq for AllowAlias<T> {
            fn eq(&self, other: &Self) -> bool {
                &*self == &*other
            }
        }

        impl<T: Eq> Eq for AllowAlias<T> {}

        impl<T: PartialOrd> PartialOrd for AllowAlias<T> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                self.deref().partial_cmp(&*other)
            }
        }

        impl<T: Ord> Ord for AllowAlias<T> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.deref().cmp(&*other)
            }
        }

        impl<T: Hash> Hash for AllowAlias<T> {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                self.deref().hash(state);
            }
        }

        impl<T> Deref for AllowAlias<T> {
            type Target = T;

            fn deref(&self) -> &Self::Target {
                unsafe { self.0.assume_init_ref() }
            }
        }
    }

    /// Acts like a `&T` but has to carry around the value.
    pub struct Guard<'a, T> {
        inner: AllowAlias<T>,
        _boo: PhantomData<&'a ()>,
    }

    impl<'a, T> Guard<'a, T> {
        pub fn new(value: ManuallyDrop<T>) -> Self {
            Self {
                inner: AllowAlias::new(ManuallyDrop::into_inner(value)),
                _boo: PhantomData,
            }
        }

        /// # Safety
        /// Make sure to not violate aliasing with this
        pub unsafe fn into_inner(self) -> ManuallyDrop<T> {
            ManuallyDrop::new(self.inner.into_inner())
        }

        pub fn inner(&self) -> &T {
            &*self
        }
    }

    impl<T> Deref for Guard<'_, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.inner
        }
    }

    impl<T: Debug> Debug for Guard<'_, T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("NeverDrop")
                .field("inner", &*self.inner)
                .finish()
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use core::mem;
    use std::{boxed::Box, format, println};

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
                        let ptr = stuffed_ptr.get_ptr().unwrap();
                        let boxed = Box::from_raw(ptr);
                        assert_eq!(*boxed, 1);
                    }
                }


                #[test]
                fn [<get_other__ $backend>]() {
                    let stuffed_ptr: StuffedPtr<(), EmptyInMax, $backend> = StuffedPtr::new_other(EmptyInMax);
                    assert!(unsafe { stuffed_ptr.get_other() }.is_some());
                    assert!(matches!(stuffed_ptr.copy_other(), Some(EmptyInMax)));
                }

                #[test]
                fn [<debug__ $backend>]() {
                    let boxed = Box::new(1);
                    let stuffed_ptr: StuffedPtr<i32, HasDebug, $backend> = from_box(boxed);
                    println!("{stuffed_ptr:?}");
                    assert!(format!("{stuffed_ptr:?}").starts_with("Ptr("));

                    drop(unsafe { Box::from_raw(stuffed_ptr.get_ptr().unwrap()) });

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
