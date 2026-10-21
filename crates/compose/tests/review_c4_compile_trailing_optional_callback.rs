//! Phase C fourth review: valid code that `#[composable]` fails to compile.
//!
//! A final block is a trailing lambda only when the last parameter is a callback; an optional
//! callback (D82, `Option<impl Fn()>`) isn't one for `call!` (`Parameter::callback` is `None`
//! for it). So a trailing block for an optional last parameter binds positionally, to the next
//! parameter: here the defaulted `selected: bool`. Kotlin takes a trailing lambda for a
//! nullable function-typed last parameter (Material's `trailingContent`), and the same block
//! given by name (`trailing = { … }`) compiles.
//!
//! Expected: compiles, the block is `Some` content. Actual: E0308 mismatched types, "expected
//! `bool`, found `()`": the block is passed as written, as the value of `selected`, and runs.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Label(#[format] text: String) {
    let _ = text;
}

#[composable]
fn ListItem(
    headline: String,
    #[default(false)] selected: bool,
    #[default(None)] trailing: Option<impl Fn() + 'static>,
) {
    Label(headline);
    let _ = selected;
    if let Some(trailing) = &trailing {
        trailing();
    }
}

#[composable]
fn Screen() {
    ListItem!("headline".to_string(), {
        Label!("trailing");
    });
}

#[test]
fn compiles() {}
