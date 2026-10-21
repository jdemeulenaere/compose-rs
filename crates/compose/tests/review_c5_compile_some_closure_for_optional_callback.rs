//! Phase C fifth review: valid code that `#[composable]` fails to compile.
//!
//! An optional callback parameter is declared `Option<impl Fn() + 'static>` (D82), so `Some(…)`
//! of a closure is how plain Rust passes one, and how a caller writes it when the callback is
//! conditional (`on_clear = if editable { Some(move || …) } else { None }`). `call!` makes a
//! bare lambda `Some` memoized callback, but passes anything else as written: the closure
//! inside `Some(…)` reaches a parameter that is really `Option<Callback<dyn Fn()>>`.
//!
//! Expected: compiles, the closure is a memoized `Some` callback, as `on_clear = move || …` is.
//! Actual: E0308 mismatched types, "expected `Callback<dyn Fn()>`, found closure". The caller
//! has to know the rewritten type and write `Some(callback(move || …))`.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn SearchField(value: String, #[default(None)] on_clear: Option<impl Fn() + 'static>) {
    let _ = value;
    if let Some(on_clear) = &on_clear {
        on_clear();
    }
}

#[composable]
fn Screen(query: MutableState<String>, editable: bool) {
    SearchField!(query.get(), on_clear = Some(move || query.set(String::new())));
    SearchField!(
        query.get(),
        on_clear = if editable { Some(move || query.set(String::new())) } else { None }
    );
}

#[test]
fn compiles() {}
