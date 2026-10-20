//! Regression test from the phase B review: valid code that `#[composable]` used to fail to
//! compile.
//!
//! A parameter written as a pattern is renamed (`__compose_argument0`, immutable) and rebound
//! with `let <pattern> = __compose_argument0;` (composable.rs, `rebinds`). The initializer is a
//! place expression, so a `ref mut` binding in the pattern borrows the immutable renamed
//! parameter mutably: E0596. In the original function the parameter is its own place and
//! `ref mut` is fine.
//!
//! Expected: compiles. Actual: E0596 "cannot borrow `__compose_argument0` as mutable".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Counter((ref mut count, step): (u32, u32)) {
    *count += step;
    let _ = count;
}

#[test]
fn compiles() {}
