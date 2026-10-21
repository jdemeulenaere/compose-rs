//! Phase C third review: code the docs say compiles, but doesn't.
//!
//! docs/03 ("Lambdas", point 3, and D80): a memoized closure resolves through `resolve_N`, "so
//! an untyped closure parameter (`|text| …`) takes its type from the callback parameter". That
//! holds for call macros (review_c2_compile_untyped_callback_argument.rs), where the probe is
//! the argument itself. In plain code the memoized closure stays wrapped in `callback(…)`:
//! `callback(resolve_1(lambda_probe(…), move |text| …))`, and `callback`'s generic `F` doesn't
//! reach the closure, so `text.len()` is "type annotations needed" (E0282). The same for a
//! modifier closure (`Modifier.on_key_event(move |event| event.key == …)`, D70), which
//! compose-ui's own tests annotate (`|event: KeyEvent|`).
//!
//! Expected: compiles. Actual: E0282 on `text` and on `event`.

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;
use compose::ui::{Key, Modifier};

#[composable]
fn Field(on_change: impl Fn(String) + 'static) {
    on_change(String::new());
}

#[composable]
fn Uses(n: usize) {
    Field(callback(move |text| {
        let _ = text.len() + n;
    }));
    // A modifier's closure goes to an `impl IntoCallback` parameter, which doesn't give its
    // parameters a type: they're annotated, as in plain Rust (D87).
    let _ =
        Modifier.on_key_event(move |event: compose::ui::KeyEvent| event.key == Key::Enter && n > 0);
}

#[test]
fn compiles() {}
