# stuff

[![crates.io](https://img.shields.io/crates/v/stuff.svg)](https://crates.io/crates/stuff)
[![docs.rs](https://img.shields.io/docsrs/stuff)](https://docs.rs/stuff)
![Build Status](https://img.shields.io/endpoint.svg?url=https%3A%2F%2Factions-badge.atrox.dev%2FNilstrieb%2Fstuff%2Fbadge%3Fref%3Dmain&style=flat)

A crate for stuffing things into a pointer.

This crate is tested using miri (with `-Zmiri-tag-raw-pointers`).

`stuff` helps you to

- Stuff arbitrary data into pointers
- Stuff pointers or arbitrary data into fixed size storage (u64, u128)

in a **portable and provenance friendly** way.
 
It does by providing an abstraction around it, completely abstracting away the provenance and pointers from
the user, allowing the user to do their bit stuffing only on integers (pointer addresses) themselves.

`StuffedPtr` is the main type of this crate. It's a type whose size depends on the
choice of `Backend` (defaults to `usize`, `u64` and `u128` are also possible). It can store a
pointer or some extra data.

You can choose any arbitrary bitstuffing depending on the `StuffingStrategy`, an unsafe trait that governs 
how the extra data (or the pointer itself) will be packed into the backend. While this trait is still unsafe,
it's a lot safer than doing everything by hand.

# Example: NaN-Boxing
Pointers are hidden in the NaN values of floats. NaN boxing often involves also hiding booleans
or null in there, but we stay with floats and pointers (pointers to a `HashMap` that servers
as our "object" type).

See [crafting interpreters](https://craftinginterpreters.com/optimization.html#nan-boxing)
for more details.

```rust
use std::collections::HashMap;
use stuff::{StuffedPtr, StuffingStrategy};

// Create a unit struct for our strategy
struct NanBoxStrategy;

const QNAN: u64 = 0x7ffc000000000000; // implementation detail of NaN boxing, a quiet NaN mask

const SIGN_BIT: u64 = 0x8000000000000000; // implementation detail of NaN boxing, the sign bit of an f64

unsafe impl StuffingStrategy<u64> for NanBoxStrategy {
    type Extra = f64;
    
    fn is_extra(data: u64) -> bool {
        (data & QNAN) != QNAN
    }
    
    fn stuff_extra(inner: Self::Extra) -> u64 {
        unsafe { std::mem::transmute(inner) } // both are 64 bit POD's
    }
    
    unsafe fn extract_extra(data: u64) -> Self::Extra {
        std::mem::transmute(data) // both are 64 bit POD's
    }
    
    fn stuff_ptr(addr: usize) -> u64 {
        // add the QNAN and SIGN_BIT
        SIGN_BIT | QNAN | u64::try_from(addr).unwrap()
    }
    
    fn extract_ptr(inner: u64) -> usize {
        // keep everything except for QNAN and SIGN_BIT
        (inner & !(SIGN_BIT | QNAN)).try_into().unwrap()
    }
}

type Object = HashMap<String, u32>; // a very, very crude representation of an object

type Value = StuffedPtr<Object, NanBoxStrategy, u64>; // our value type

fn main() {
    let float: Value = StuffedPtr::new_extra(123.5);
    assert_eq!(float.copy_extra(), Some(123.5));
    
    let object: Object = HashMap::from([("a".to_owned(), 457)]);
    
    let boxed = Box::new(object);
    let ptr: Value = StuffedPtr::new_ptr(Box::into_raw(boxed));
    
    let object = unsafe { &*ptr.get_ptr().unwrap() };
    assert_eq!(object.get("a"), Some(&457));
    
    drop(unsafe { Box::from_raw(ptr.get_ptr().unwrap()) });
    // `ptr` is a dangling pointer now!
}
```

# MSRV-Policy
`stuff`s current MSRV is `1.31.1`, and this version will not get raised unless absolutely necessary.