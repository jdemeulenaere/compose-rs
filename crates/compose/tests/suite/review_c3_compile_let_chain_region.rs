//! Phase C third review: valid code that `#[composable]` fails to compile.
//!
//! The right operand of `&&` is a branch, so `Regions` wraps it in a block with a region guard
//! when it contains a likely composable call. In a let chain (edition 2024) that operand can be
//! a `let`: `{ let __compose_region = …; let Some(y) = find(Id(x)) }` isn't an expression, and
//! `wrap_expr`'s `parse_quote!` panics. Any PascalCase call (a tuple struct, an enum variant)
//! in a later `let` of a chain triggers it.
//!
//! Expected: compiles. Actual: "custom attribute panicked … expected `;`".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[derive(Clone, Copy)]
struct Id(u32);

fn find(id: Id) -> Option<u32> {
    Some(id.0)
}

#[composable]
fn Uses(a: Option<u32>) {
    if let Some(x) = a
        && let Some(y) = find(Id(x))
    {
        let _ = y;
    }
}

#[test]
fn compiles() {}
