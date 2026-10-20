//! Review: valid code that `#[composable]` makes fail to compile.
//!
//! syn parses a body's inner attributes (`#![allow(…)]`) into `ItemFn::attrs`; the macro prints
//! all of `attrs` before the `fn`, so the inner attribute lands outside the body: "an inner
//! attribute is not permitted in this context" (composable.rs, `#(#attrs)*`).
//!
//! Expected: compiles, with the lint allowed inside the body. Actual: a parse error.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Quiet(value: i32) {
    #![allow(unused_variables)]
    let unused = value;
}

#[test]
fn compiles() {}
