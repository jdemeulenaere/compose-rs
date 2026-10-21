//! Regression test from the phase B review: valid code that `#[composable]` used to fail to
//! compile.
//!
//! A `#[cfg]`-ed parameter stays `#[cfg]`-ed in the signature, but the macro uses it
//! unconditionally in the kept tuple, the comparisons and the restart call. When the cfg is off:
//! E0425 "cannot find value `debug_label`" and E0061 (the restart call passes two arguments).
//!
//! Expected: compiles (the parameter, its comparison and its kept slot are cfg'd out together).
//! Actual: E0425 / E0061.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Badge(#[cfg(any())] debug_label: &'static str, count: u32) {
    let _ = count;
}

#[test]
fn compiles() {}
