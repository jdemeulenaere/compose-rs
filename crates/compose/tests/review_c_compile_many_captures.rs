//! Phase C review: valid code that `#[composable]` fails to compile.
//!
//! The lambda keys are a tuple of one key per capture, and `LambdaKeys` is implemented for
//! tuples of at most 12. A screen's content block easily refers to more than 12 locals (and
//! the walk over-counts: every identifier in macro tokens and format strings naming a local
//! counts), and then fails with E0277: the key tuple is not `LambdaKeys`.
//!
//! Expected: compiles (nest the tuples, or fall back to `Unkeyed`). Actual: E0277.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Column(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Label(#[format] text: String) {
    let _ = text;
}

#[composable]
fn Form(a: u32, b: u32, c: u32, d: u32, e: u32, f: u32, g: u32) {
    let h = a + 1;
    let i = b + 1;
    let j = c + 1;
    let k = d + 1;
    let l = e + 1;
    let m = f + 1;
    Column!({
        Label!("{a} {b} {c} {d} {e} {f} {g}");
        Label!("{h} {i} {j} {k} {l} {m}");
    });
}

#[test]
fn compiles() {}
