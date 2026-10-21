//! Phase C sixth review: valid code that `#[composable]` fails to compile.
//!
//! `Some(lambda)` for an optional callback loses its marker when the lambda makes a likely
//! composable call: `Regions` puts the closure's region guard in front of the marker statement,
//! so `call!` treats the closure as unmarked (review_c6.rs,
//! `some_content_lambda_for_an_optional_callback_is_memoized`). Inside a content block, an
//! unmarked closure isn't the nested, clone-built probe of D83: it is the user's `move` closure,
//! which moves `title` out of the enclosing content closure, an `Fn`. Without the composable call
//! (`Some(move || { let _ = title.clone(); })`) the same code compiles, and so does `label = move
//! || Label(title.clone())` without `Some`.
//!
//! Expected: compiles, the label a nested memoized lambda. Actual: E0507 "cannot move out of
//! `title`, a captured variable in an `Fn` closure".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Section(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Label(text: String) {
    let _ = text;
}

#[composable]
fn Field(#[default(None)] label: Option<impl Fn() + 'static>) {
    if let Some(label) = &label {
        label();
    }
}

#[composable]
fn Form(title: String) {
    Section!({
        Field!(label = Some(move || Label(title.clone())));
    });
}

#[test]
fn compiles() {}
