//! Phase C sixth review: valid code that makes `#[composable]` panic.
//!
//! `Marked::to_tokens` marks a closure argument of a call macro by re-parsing its body as the
//! second statement of a block, `{ ::compose::__private::marked!(…); #body }`. A closure body
//! that starts with a block-like expression and continues with a binary operator (`move |n| if
//! strict { n * 2 } else { n } + base`) is one expression as a closure body, but in statement
//! position the `if` ends the statement and `+ base` doesn't parse: `parse_quote!` panics.
//! (With `-` or `*` instead, the operator parses as a unary one and the closure silently becomes
//! two statements, a type error.) `Regions::wrap_expr` re-parses closure bodies and arm bodies the
//! same way. Parenthesizing the body (`(#body)`) or building the block from a parsed `Expr`
//! keeps it one expression.
//!
//! Expected: compiles. Actual: "custom attribute panicked: expected an expression".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Scaled(scale: impl Fn(u32) -> u32 + 'static) {
    scale(1);
}

#[composable]
fn Screen(strict: bool, base: u32) {
    Scaled!(scale = move |n| if strict { n * 2 } else { n } + base);
}

#[test]
fn compiles() {}
