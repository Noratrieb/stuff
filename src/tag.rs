#![allow(dead_code)]

use core::marker::PhantomData;

use sptr::Strict;

use crate::Backend;

pub struct TaggedPtr<T, S, B = usize>(B::Stored, PhantomData<S>)
where
    B: Backend<T>;

impl<T, S, B> TaggedPtr<T, S, B>
where
    S: TaggingStrategy<B>,
    B: Backend<T>,
{
    pub fn new(ptr: *mut T, tag: S::Tag) -> Self {
        let addr = Strict::addr(ptr);
        let tagged = S::set(addr, tag);
        let stored = B::set_ptr(ptr, tagged);
        TaggedPtr(stored, PhantomData)
    }

    pub fn get_ptr(&self) -> *mut T {
        let (provenance, stored) = B::get_ptr(self.0);
        let addr = S::get_ptr_addr(stored);
        Strict::with_addr(provenance, addr)
    }

    pub fn get_tag(&self) -> S::Tag {
        let stored = B::get_int(self.0);
        S::get_tag(stored)
    }

    pub fn set_tag(&self, tag: S::Tag) -> Self {
        let (provenance, stored) = B::get_ptr(self.0);
        let ptr_addr = S::get_ptr_addr(stored);
        let addr = S::set(ptr_addr, tag);
        let stored = B::set_ptr(provenance, addr);
        TaggedPtr(stored, PhantomData)
    }
}

impl<T, S, B> Clone for TaggedPtr<T, S, B>
where
    B: Backend<T>,
{
    fn clone(&self) -> Self {
        TaggedPtr(self.0, self.1)
    }
}

impl<T, S, B> Copy for TaggedPtr<T, S, B> where B: Backend<T> {}

pub trait TaggingStrategy<B> {
    type Tag: Copy;

    fn get_tag(data: B) -> Self::Tag;

    fn get_ptr_addr(data: B) -> usize;

    fn set(addr: usize, tag: Self::Tag) -> B;
}

#[cfg(test)]
mod tests {}
