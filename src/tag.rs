use core::marker::PhantomData;

use crate::Backend;

pub struct TaggedPtr<T, S, B = usize>(B::Stored, PhantomData<S>)
where
    B: Backend<T>;

pub trait TaggingStrategy<B> {
    type Tag;

    fn get_tag(data: B) -> Self::Tag;

    fn get_ptr_addr(data: B) -> usize;

    fn set(addr: usize, tag: Self::Tag) -> B;
}
