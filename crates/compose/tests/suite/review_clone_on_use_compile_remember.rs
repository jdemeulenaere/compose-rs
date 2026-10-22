//! Clone-on-use review: rule (a) skips `remember`'s calculation.
//!
//! docs/03 lists `remember_…(list)` among the calls whose bare capture arguments are cloned, but
//! the `remember` arm of the pass never visits its arguments, so a capture given by itself to a
//! function inside the calculation (`remember(|| mutable_state_of(initial))`, Kotlin's
//! `remember { mutableStateOf(initial) }` in a content lambda) still moves out of the `Fn`
//! lambda. Not a regression; a gap in the rule where Compose code meets it most.
//!
//! Expected: compiles, as `make(name)` does outside `remember`. Actual: E0507 "cannot move out of
//! value, a captured variable in an `Fn` closure".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

#[composable]
fn Section(content: impl Fn() + 'static) {
    content();
}

fn make(text: String) -> usize {
    text.len()
}

#[composable]
fn Remembers(name: String) {
    Section!({
        let direct = make(name);
        let remembered = remember(|| make(name));
        let _ = (direct, remembered);
    });
}

#[test]
fn compiles() {}
