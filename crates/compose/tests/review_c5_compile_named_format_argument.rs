//! Phase C fifth review: valid code that `#[composable]` fails to compile.
//!
//! A `#[format]` call macro takes "format arguments" (`Text!("Count: {}", n)`), but only
//! positional ones: `call!` stops collecting them at the first `name = value`, which it then
//! binds as a named argument of the composable. A named format argument, the Rust way to put an
//! expression behind a name in the string (`"{count} items", count = list.len()`, since inline
//! arguments must be plain identifiers), is rejected as an unknown parameter. (And one named like
//! a parameter, `"{color}", color = …`, would silently bind the parameter instead.)
//!
//! Expected: compiles, as `format!("{count} items", count = items.len())` does. Actual:
//! "`Label` has no parameter `count`; its parameters are: text, bold".

#![allow(non_snake_case, dead_code)]

use std::rc::Rc;

use compose::runtime::*;

#[composable]
fn Label(#[format] text: String, #[default(false)] bold: bool) {
    let _ = (text, bold);
}

#[composable]
fn Summary(items: Rc<Vec<String>>) {
    Label!("{count} items", count = items.len());
    Label!("{first} and {} more", items.len() - 1, first = items[0], bold = true);
}

#[test]
fn compiles() {}
