//! Regression test from the second phase B review: valid code that `#[composable]` used to fail to compile.
//!
//! In a loop with a `continue`, each statement containing a likely composable call gets its own
//! region group, and a macro statement is rewritten as `{ region; mac!(…); }`. A statement
//! macro that declares bindings for the code after it (`let`-declaring macros such as
//! `pin_mut!`, `tokio::pin!` or the user's own `bind!`) then declares them inside the new block,
//! out of scope for the rest of the loop body: E0425 "cannot find value `height`".
//!
//! Expected: compiles (the region guard is added without scoping the macro's statements, e.g.
//! `let __compose_region = region(…); mac!(…);`, or macro statements aren't wrapped).

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

macro_rules! bind {
    ($name:ident = $value:expr) => {
        let $name = $value;
    };
}

#[composable]
fn Measure(id: u32) -> u32 {
    id * 10
}

#[composable]
fn Row(height: u32) {
    let _ = height;
}

#[composable]
fn List(count: u32) {
    for id in 0..count {
        if id == 0 {
            continue;
        }
        bind!(height = Measure(id));
        Row(height);
    }
}

#[test]
fn compiles() {}
