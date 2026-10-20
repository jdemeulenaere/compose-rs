//! Regression test from the second phase B review: valid code that `#[composable]` used to fail to compile.
//!
//! The fix for `#[cfg]` parameters (non-restartable) misses parameters written as patterns:
//! the pattern is renamed to `__compose_argument0` in the signature, where it keeps its
//! `#[cfg]`, but the rebinding `let (width, height) = __compose_argument0;` at the start of the
//! body is emitted unconditionally. With the cfg off: E0425 "cannot find value
//! `__compose_argument0` in this scope", on the `#[composable]` attribute.
//!
//! Expected: compiles (the rebinding is cfg'd out with its parameter).

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Frame(#[cfg(any())] (width, height): (u32, u32), count: u32) {
    let _ = count;
}

#[test]
fn compiles() {}
