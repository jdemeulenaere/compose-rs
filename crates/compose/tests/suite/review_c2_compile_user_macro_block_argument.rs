//! Phase C second review: valid code that `#[composable]` fails to compile.
//!
//! D64 made the marker a macro so a user's PascalCase macro still accepts it, but only where
//! the user macro takes an `expr`. A block argument is rewritten to
//! `::compose::__private::marked!((n) { … })`, which no longer matches a `$body:block`
//! fragment (nor a `|$x:ident| $body:expr` closure pattern, nor a single `$t:tt`):
//! "no rules expected `::`". (A layout-like user macro named like a composable, taking a
//! block, is the natural case.)
//!
//! Expected: compiles. Actual: "no rules expected `::`".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

macro_rules! Section {
    ($title:expr, $body:block) => {{
        let _ = $title;
        $body
    }};
}

#[composable]
fn Uses(n: u32) {
    Section!("title", {
        let _ = n;
    });
}

#[test]
fn compiles() {}
