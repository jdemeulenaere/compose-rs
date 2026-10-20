//! Review: valid code that `#[composable]` makes fail to compile.
//!
//! The region pass descends into inline `const { … }` blocks. A branch there with a PascalCase
//! call (a tuple-struct constructor, `Dp(…)`, which is `const`) gets a `region(…)` call, which
//! isn't `const fn`: E0015 "cannot call non-const function in constants".
//!
//! Expected: compiles. Actual: E0015.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Dp(f32);

const COMPACT: bool = true;

#[composable]
fn Padded(level: u32) {
    let padding = const { if COMPACT { Dp(4.0) } else { Dp(8.0) } };
    let _ = (level, padding);
}

#[test]
fn compiles() {}
