//! Phase C third review: valid code that `#[composable]` fails to compile.
//!
//! A PascalCase macro invoked by path in statement position is rewritten to
//! `{ use path::Name; Name!(…) }` (D65), taking it for a composable's call macro. A user's own
//! exported macro that expands to statements (here a `let`) then no longer works: inside the
//! block it sits in expression position ("expected expression, found `let` statement"), and a
//! binding or an item it declares wouldn't be visible after the block anyway.
//!
//! Expected: compiles. Actual: "expected expression, found `let` statement" and "cannot find
//! value `doubled`".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

mod macros {
    #[macro_export]
    macro_rules! Bind {
        ($name:ident = $value:expr) => {
            let $name = $value;
        };
    }
}

#[composable]
fn Label(text: String) {
    let _ = text;
}

#[composable]
fn Uses(n: u32) {
    crate::Bind!(doubled = n * 2);
    Label(doubled.to_string());
}

#[test]
fn compiles() {}
