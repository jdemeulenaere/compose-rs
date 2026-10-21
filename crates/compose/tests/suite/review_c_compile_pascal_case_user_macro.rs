//! Phase C review: valid code that `#[composable]` fails to compile.
//!
//! The memoization pass treats every PascalCase macro as a composable's call macro and rewrites
//! the closures in its arguments (and a final block) into `@memoize(…) …` markers that only
//! `call!` understands. A user's own PascalCase macro then gets tokens it can't match:
//! "no rules expected `@`", reported at the `#[composable]` attribute. (Regions already warn
//! that "a user macro may be named like a composable"; before phase C this compiled.)
//!
//! Expected: compiles (only rewrite inside macros known to be call macros, or make the marker
//! something an arbitrary macro still accepts). Actual: "no rules expected `@`".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

macro_rules! Apply {
    ($f:expr, $x:expr) => {
        ($f)($x)
    };
}

#[composable]
fn Uses(n: u32) {
    let doubled = Apply!(|x: u32| x * 2, n);
    let _ = doubled;
}

#[test]
fn compiles() {}
