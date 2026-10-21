//! Phase C fourth review: valid code that `#[composable]` fails to compile.
//!
//! A nested lambda (D83) is built on a miss from clones of its *captures*. A name bound by a
//! user's statement macro (`Bind!(label = …);`, the doc's own example) is tainted, not a
//! capture: the lambda is keyed as always changed, but the name isn't cloned either. The lazy
//! builder `|| move || … label.clone() …` then moves `label` out of the content block, which
//! becomes `FnOnce`, and the content callback needs `Fn`. The same lambda outside the content
//! block, or `label` bound by a plain `let`, compiles.
//!
//! Expected: compiles (clone tainted names that are used too). Actual: E0525 "expected a closure
//! that implements the `Fn` trait, but this closure only implements `FnOnce`".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

macro_rules! Bind {
    ($name:ident = $value:expr) => {
        let $name = $value;
    };
}

#[composable]
fn Holder(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Clickable(on_click: impl Fn() + 'static) {
    on_click();
}

#[composable]
fn Screen(name: &'static str) {
    Bind!(label = String::from(name));
    Holder!({
        Clickable!(on_click = move || println!("{}", label.clone()));
    });
}

#[test]
fn compiles() {}
