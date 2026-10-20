//! Review: valid code that `#[derive(ComposeArg)]` makes fail to compile.
//!
//! The derive compares `&self.field` with `&other.field`; on a `#[repr(packed)]` struct those
//! are unaligned references: E0793. `#[derive(PartialEq)]` handles this by copying the fields
//! (the struct must be `Copy`, as it is here).
//!
//! Expected: compiles. Actual: E0793 "reference to field of packed struct is unaligned".

#![allow(dead_code)]

use compose::runtime::*;

#[derive(Clone, Copy, ComposeArg)]
#[repr(C, packed)]
struct Rgb565 {
    tag: u8,
    value: u32,
}

#[test]
fn compiles() {}
