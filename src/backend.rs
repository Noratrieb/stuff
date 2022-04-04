use sptr::Strict;

pub trait Backend<T> {
    type Stored: Copy;

    fn get_ptr(s: Self::Stored) -> (*mut T, Self);

    fn set_ptr(provenance: *mut T, addr: Self) -> Self::Stored;

    fn get_int(s: Self::Stored) -> Self;

    fn set_int(s: Self::Stored, int: Self) -> Self::Stored;
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

    fn set_int(s: Self::Stored, int: Self) -> Self::Stored {
        Strict::with_addr(s, int)
    }
}
