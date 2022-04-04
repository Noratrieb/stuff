use sptr::Strict;

pub trait Backend<T> {
    type Stored: Copy;

    fn get_ptr(s: Self::Stored) -> (*mut T, Self);

    fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored;

    fn get_int(s: Self::Stored) -> Self;
}

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

#[cfg(target_pointer_width = "64")]
impl<T> Backend<T> for u128 {
    // this one keeps the MSB in the pointer address, and the LSB in the integer

    type Stored = (*mut T, u64);

    fn get_ptr(s: Self::Stored) -> (*mut T, Self) {
        (s.0, Self::get_int(s))
    }

    fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored {
        let ptr_addr = (addr >> 64) as u64;
        let int_addr = addr as u64; // truncate it
        (Strict::with_addr(provenance, ptr_addr as usize), int_addr)
    }

    fn get_int(s: Self::Stored) -> Self {
        let ptr_addr = Strict::addr(s.0) as u64;
        (u128::from(ptr_addr) << 64) | u128::from(s.1)
    }
}
