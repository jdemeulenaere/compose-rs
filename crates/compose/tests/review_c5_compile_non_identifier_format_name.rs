//! Phase C fifth review: valid code that `#[composable]` fails to compile.
//!
//! The capture walk reads every string literal in a lambda as a possible format string and
//! makes an identifier of each `{name}` it finds, checking the name with
//! `char::is_alphabetic`/`is_alphanumeric`. Those accept characters an identifier can't hold
//! (superscripts and fractions are numeric: `²`, `³`, `½`), and `Ident::new` panics on them. A
//! plain (non-format) string such as a unit template `"{m²}"` or a formula `"{x²} + {y²}"` in a
//! lambda then makes the whole composable fail with a proc-macro panic, not an error naming
//! anything.
//!
//! Expected: compiles (the literal isn't a format string; at most an over-counted capture).
//! Actual: "custom attribute panicked … `"m²"` is not a valid identifier".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Field(label: &'static str, unit: impl Fn() -> String + 'static) {
    let _ = (label, unit);
}

fn localize(template: &str, value: f32) -> String {
    template.replace("{m²}", &format!("{value} m²"))
}

#[composable]
fn AreaField(area: MutableState<f32>) {
    Field!("area", move || localize("Area: {m²}", area.get()));
}

#[test]
fn compiles() {}
