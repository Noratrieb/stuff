use std::mem;

use sptr::Strict;

/// A backend where the stuffed pointer is stored. Must be bigger or equal to the pointer size.
pub trait Backend<T> {
    /// The underlying type where the data is stored. Often a tuple of a pointer (for the provenance)
    /// and some integers to fill up the bytes.
    type Stored: Copy;

    /// Get the pointer from the backed. Since the [`crate::StuffingStrategy`] is able to use the full
    /// bytes to pack in the pointer address, the full address is returned in the second tuple field,
    /// as the integer. The provenance of the pointer is returned as the first tuple field, but its
    /// address should be ignored and may be invalid.
    fn get_ptr(s: Self::Stored) -> (*mut T, Self);

    /// Set a new pointer address. The provenance of the new pointer is transferred in the first argument,
    /// and the address in the second. See [`Backend::get_ptr`] for more details on the separation.
    fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored;

    /// Get the integer value from the backend. Note that this *must not* be used to create a pointer,
    /// for that use [`Backend::get_ptr`] to keep the provenance.
    fn get_int(s: Self::Stored) -> Self;
}

#[allow(dead_code)] // :/
const fn assert_same_size<A, B>() {
    let has_equal_size = mem::size_of::<A>() == mem::size_of::<B>();
    assert!(has_equal_size);
}

#[cfg(not(target_pointer_width = "16"))]
const _: () = assert_same_size::<u128, <u128 as Backend<()>>::Stored>();
const _: () = assert_same_size::<u64, <u64 as Backend<()>>::Stored>();
const _: () = assert_same_size::<usize, <usize as Backend<()>>::Stored>();

impl<T> Backend<T> for usize {
    type Stored = *mut T;

    fn get_ptr(s: Self::Stored) -> (*mut T, Self) {
        (s, Strict::addr(s))
    }

    fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored {
        Strict::with_addr(provenance, addr)
    }

    fn get_int(s: Self::Stored) -> Self {
        Strict::addr(s)
    }
}

#[cfg(target_pointer_width = "64")]
/// on 64 bit, we can just treat u64/usize interchangeably, because uintptr_t == size_t in Rust
impl<T> Backend<T> for u64 {
    type Stored = *mut T;

    fn get_ptr(s: Self::Stored) -> (*mut T, Self) {
        (s, Strict::addr(s) as u64)
    }

    fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored {
        Strict::with_addr(provenance, addr as usize)
    }

    fn get_int(s: Self::Stored) -> Self {
        Strict::addr(s) as u64
    }
}

macro_rules! impl_backend_2_tuple {
    (impl for $ty:ty { (*mut T, $int:ident), $num:literal }) => {
        impl<T> Backend<T> for $ty {
            // this one keeps the MSB in the pointer address, and the LSB in the integer

            type Stored = (*mut T, $int);

            fn get_ptr(s: Self::Stored) -> (*mut T, Self) {
                (s.0, Self::get_int(s))
            }

            fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored {
                let ptr_addr = (addr >> $num) as usize;
                let int_addr = addr as $int; // truncate it
                (Strict::with_addr(provenance, ptr_addr), int_addr)
            }

            fn get_int(s: Self::Stored) -> Self {
                let ptr_addr = Strict::addr(s.0) as $int;
                (<$ty>::from(ptr_addr) << $num) | <$ty>::from(s.1)
            }
        }
    };
}

/// num1 is ptr-sized, num2 is 2*ptr sized
#[cfg_attr(target_pointer_width = "64", allow(unused))] // not required on 64 bit
macro_rules! impl_backend_3_tuple {
    (impl for $ty:ty { (*mut T, $int1:ident, $int2:ident), $num1:literal, $num2:literal }) => {
        impl<T> Backend<T> for $ty {
            // this one keeps the MSB in the pointer address, ISB in int1 and the LSB in the int2

            type Stored = (*mut T, $int1, $int2);

            fn get_ptr(s: Self::Stored) -> (*mut T, Self) {
                (s.0, Self::get_int(s))
            }

            fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored {
                let ptr_addr = (addr >> ($num1 + $num2)) as usize;
                let num1_addr = (addr >> $num2) as $int1; // truncate it
                let num2_addr = addr as $int2; // truncate it
                (
                    Strict::with_addr(provenance, ptr_addr),
                    num1_addr,
                    num2_addr,
                )
            }

            fn get_int(s: Self::Stored) -> Self {
                let ptr_addr = Strict::addr(s.0) as $ty;
                let num1_addr = self.1 as $ty;
                let num2_addr = self.2 as $ty;
                (ptr_addr << ($num1 + $num2)) | (num1_addr << ($num2)) | num2_addr
            }
        }
    };
}

#[cfg(target_pointer_width = "64")]
impl_backend_2_tuple!(impl for u128 { (*mut T, u64), 64 });

#[cfg(target_pointer_width = "32")]
impl_backend_2_tuple!(impl for u64 { (*mut T, u32), 32 });

#[cfg(target_pointer_width = "32")]
impl_backend_3_tuple!(impl 128 u64 { (*mut T, u32, u64), 32, 64 });

#[cfg(target_pointer_width = "16")]
impl_backend_3_tuple!(impl for u64 { (*mut T, u16, u32), 16, 32 });

// no 128 on 16 bit for now
