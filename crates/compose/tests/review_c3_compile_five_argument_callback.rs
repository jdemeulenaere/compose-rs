//! Phase C third review: valid code that `#[composable]` fails to compile.
//!
//! `#[composable]` accepts an `impl Fn(A, B, C, D, E) + 'static` parameter (any arity) and turns
//! it into `Callback<dyn Fn(A, B, C, D, E)>`, but `IntoCallback` is implemented for closures of
//! at most four arguments and `resolve_N`/`callback_N` stop at four: no closure can be passed,
//! through the call macro (typed or untyped parameters) or `callback(…)`. The definition gives no
//! diagnostic; every call fails with "the trait bound `{closure}: IntoCallback<_>` is not
//! satisfied".
//!
//! Expected: compiles (or the definition is rejected with a message). Actual: E0277 at the call.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Five(on_event: impl Fn(u32, u32, u32, u32, u32) + 'static) {
    on_event(1, 2, 3, 4, 5);
}

#[composable]
fn Uses(n: u32) {
    Five!(move |a: u32, b: u32, c: u32, d: u32, e: u32| {
        let _ = a + b + c + d + e + n;
    });
}

#[test]
fn compiles() {}
