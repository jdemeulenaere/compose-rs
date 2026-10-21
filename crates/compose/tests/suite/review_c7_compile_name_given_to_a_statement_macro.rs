//! Phase C seventh review: valid code that `#[composable]` fails to compile.
//!
//! A statement macro outside the allow-list may bind a local the walk can't see, so every new
//! identifier in its tokens is bound *tainted* for the rest of the block (D106), and a lambda
//! naming one counts as always changed. `token_idents` takes *every* identifier of the tokens
//! that isn't a field name: constants, types, enum variants, keywords (`as`, `true`, `mut`),
//! not only the lowercase names a macro could bind. A later lambda naming one then captures it:
//! - a constant (`check!(count.get() <= MAX)`, then a handler clamping to `MAX`): since D115 the
//!   builder rebinds it, `let MAX = Clone::clone(&MAX);`, a refutable pattern. Before D115 a
//!   top-level lambda only took a key, `ArgProbe(&MAX)`, which compiled;
//! - a keyword in the lambda's macro tokens (`done as f32` in a `format!`/`println!`, after a
//!   statement macro that also cast with `as`): the marker becomes `marked!(…, as, …)`, which
//!   `call!` can't parse; the generated `let as = …` couldn't compile either.
//!
//! The statement macro here is a user's own logging or assertion macro, the most common kind of
//! macro outside the allow-list.
//!
//! Expected: compiles. Actual: E0005 "refutable pattern in local binding" at `MAX` in the
//! counter's handler; "expected identifier, found keyword `as`" at the progress handler.

#![allow(non_snake_case, dead_code, unused_macros)]

use compose::runtime::*;

#[composable]
fn Button(on_click: impl Fn() + 'static) {
    let _ = on_click;
}

/// A user's assertion or logging macro.
macro_rules! check {
    ($($tokens:tt)*) => {};
}

const MAX: u32 = 10;

#[composable]
fn Counter(count: MutableState<u32>) {
    check!(count.get() <= MAX);
    Button!(on_click = move || count.set((count.get() + 1).min(MAX)));
}

#[composable]
fn Progress(done: u32, total: u32) {
    check!(done as f32 <= total as f32);
    Button!(on_click = move || println!("{:.0}%", 100.0 * done as f32 / total as f32));
}

#[test]
fn compiles() {}
