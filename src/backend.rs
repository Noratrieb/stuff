use sptr::Strict;
use std::mem;

pub trait Backend<T> {
    type Stored: Copy;

    fn get_ptr(s: Self::Stored) -> (*mut T, Self);

    fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored;

    fn get_int(s: Self::Stored) -> Self;
}

#[allow(clippy::should_assert_eq, dead_code)] // :/
const fn assert_size<B>()
where
    B: Backend<()>,
{
    let has_equal_size = mem::size_of::<B>() == mem::size_of::<B::Stored>();
    assert!(has_equal_size);
}

#[cfg(not(target_pointer_width = "16"))]
const _: () = assert_size::<u128>();
const _: () = assert_size::<u64>();
const _: () = assert_size::<usize>();

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
