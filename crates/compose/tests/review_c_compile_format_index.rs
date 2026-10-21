//! Phase C review: valid code that `#[composable]` fails to compile.
//!
//! The capture walk scans string literals in macro tokens for inline format arguments, and
//! takes any alphanumeric name between braces: `"{0}"` yields `0`, which it turns into an
//! identifier with `Ident::new("0", …)`. That panics inside the proc macro, so any explicit
//! positional format index in a memoized closure or content block (`format!("{0} {0}", n)`,
//! `Text!("{0}/{1}", a, b)` inside a `Column! { … }`) fails to compile.
//!
//! Expected: compiles. Actual: proc-macro panic.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Child(show: impl Fn() -> String + 'static) {
    show();
}

#[composable]
fn Parent(n: u32) {
    Child!(move || format!("{0} and {0}", n));
}

#[test]
fn compiles() {}
