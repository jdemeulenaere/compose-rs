//! Phase C fourth review: valid code that `#[composable]` fails to compile.
//!
//! The right operand of `&&` is a branch: when it is the `let` of a let chain making a likely
//! call, its scrutinee is wrapped, `let Some(first) = { region; Words(…).0.first() }`. A
//! temporary of a let chain's scrutinee lives through the `then` block, but one in a block's
//! tail expression is dropped at the end of the block, so a scrutinee borrowing a temporary
//! no longer compiles. A PascalCase call is any tuple struct or enum variant constructor, so the
//! trigger needn't be a composable at all. The same function without `#[composable]` (below)
//! compiles.
//!
//! Expected: compiles. Actual: E0716 "temporary value dropped while borrowed".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

struct Words(Vec<String>);

#[composable]
fn Label(#[format] text: String) {
    let _ = text;
}

#[composable]
fn FirstWord(text: Option<String>) {
    if let Some(text) = &text
        && let Some(first) = Words(vec![text.clone()]).0.first()
    {
        Label!("{first}");
    }
}

fn first_word(text: Option<String>) {
    if let Some(text) = &text
        && let Some(first) = Words(vec![text.clone()]).0.first()
    {
        println!("{first}");
    }
}

#[test]
fn compiles() {}
