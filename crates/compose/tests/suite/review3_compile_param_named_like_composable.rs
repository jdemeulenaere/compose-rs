//! Regression test from the third phase B review (after 10a535b): valid code that
//! `#[composable]` fails to compile.
//!
//! The restart function calls the composable by its bare name from the top of the body, where
//! the parameters are in scope. A parameter with the composable's own name (natural for a
//! snake_case composable: `fn label(label: String)`, `fn icon(icon: Icon)`; the macro allows
//! any case) shadows the function there: E0618 "expected function, found `String`", and the
//! restart closure captures the parameter, so it no longer coerces to `fn(&dyn Any)` (E0308).
//! 10a535b's inner body block protects the restart function from items in the body, not from
//! the parameters.
//!
//! Expected: compiles.

#![allow(dead_code)]

use compose::runtime::*;

#[composable]
fn label(label: String) {
    let _ = label;
}

#[test]
fn compiles() {}
