mod strategies;

use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::Not;

use sptr::Strict;

pub struct StuffedPtr<T, S>(*mut T, PhantomData<S>)
where
    S: StuffingStrategy;

impl<T, S> StuffedPtr<T, S>
where
    S: StuffingStrategy,
{
    pub fn new_ptr(ptr: *mut T) -> Self {
        Self(map_ptr(ptr, S::stuff_ptr), PhantomData)
    }

    pub fn new_extra(extra: S::Extra) -> Self {
        // this doesn't have any provenance, which is ok, since it's never a pointer anyways.
        // if the user calls `set_ptr` it will use the new provenance from that ptr
        let ptr = std::ptr::null_mut();
        let ptr = Strict::with_addr(ptr, S::stuff_extra(extra));
        Self(ptr, PhantomData)
    }

    pub unsafe fn get_ptr(&self) -> Option<*mut T> {
        self.is_extra().not().then(|| self.get_ptr_unchecked())
    }

    pub unsafe fn get_ptr_unchecked(&self) -> *mut T {
        map_ptr(self.0, S::extract_ptr)
    }

    pub unsafe fn into_extra_unchecked(self) -> S::Extra {
        let data = self.addr();
        S::extract_extra(data)
    }

    pub unsafe fn get_extra_unchecked(&self) -> S::Extra {
        let data = self.addr();
        S::extract_extra(data)
    }

    pub unsafe fn get_extra(&self) -> Option<S::Extra> {
        self.is_extra().then(|| self.get_extra_unchecked())
    }

    fn addr(&self) -> usize {
        Strict::addr(self.0)
    }

    fn is_extra(&self) -> bool {
        S::is_extra(self.addr())
    }
}

impl<T, S> Debug for StuffedPtr<T, S>
where
    S: StuffingStrategy,
    S::Extra: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_extra() {
            // SAFETY: We checked that self contains the extra
            // Note: if S::Extra: !Copy, we can't just copy it out and call it a day
            // For example, if it's a Box, not forgetting it here would lead to a double free
            // So we just format it and forget it afterwards
            let extra = unsafe { self.get_extra_unchecked() };
            f.debug_struct("StuffedPtr::Extra")
                .field("extra", &extra)
                .finish()?;
            std::mem::forget(extra);
            Ok(())
        } else {
            let ptr = map_ptr(self.0, S::extract_ptr);
            f.debug_struct("StuffedPtr::Ptr")
                .field("ptr", &ptr)
                .finish()
        }
    }
}

impl<T, S> Drop for StuffedPtr<T, S>
where
    S: StuffingStrategy,
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

impl<T, S> From<Box<T>> for StuffedPtr<T, S>
where
    S: StuffingStrategy,
{
    fn from(boxed: Box<T>) -> Self {
        Self::new_ptr(Box::into_raw(boxed))
    }
}

pub unsafe trait StuffingStrategy {
    type Extra;

    fn is_extra(data: usize) -> bool;
    fn stuff_extra(inner: Self::Extra) -> usize;
    fn extract_extra(data: usize) -> Self::Extra;

    fn stuff_ptr(inner: usize) -> usize {
        inner
    }
    fn extract_ptr(inner: usize) -> usize {
        inner
    }
}

fn map_ptr<T>(ptr: *mut T, map: impl FnOnce(usize) -> usize) -> *mut T {
    let int = Strict::addr(ptr);
    let result = map(int);
    Strict::with_addr(ptr, result)
}

#[cfg(test)]
mod tests {
    use crate::strategies::test_strategies::{HasDebug, PanicsInDrop};
    use crate::StuffedPtr;

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
    fn debug() {
        let boxed = Box::new(1);
        let stuffed_ptr: StuffedPtr<i32, HasDebug> = boxed.into();
        assert!(format!("{stuffed_ptr:?}").starts_with("StuffedPtr::Ptr {"));

        drop(unsafe { Box::from_raw(stuffed_ptr.get_ptr().unwrap()) });

        let extra = HasDebug;
        let stuffed_ptr: StuffedPtr<i32, HasDebug> = StuffedPtr::new_extra(extra);
        println!("{:?} {:X}", stuffed_ptr.0, usize::MAX);
        assert_eq!(
            format!("{stuffed_ptr:?}"),
            "StuffedPtr::Extra { extra: hello! }"
        );
    }

    #[test]
    #[should_panic]
    fn needs_drop() {
        let extra = PanicsInDrop;
        let stuffed_ptr: StuffedPtr<(), PanicsInDrop> = StuffedPtr::new_extra(extra);
        // the panicking drop needs to be called here!
        drop(stuffed_ptr);
    }
}
