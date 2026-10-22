//! Phase E review: composition locals (crates/compose-runtime/src/local.rs, D81). Each test
//! states the input, what Compose does, and what happens instead.
//!
//! Checked and sound (no test kept): a reader called again with unchanged arguments sees a new
//! provided value; removing an inner provider of the same local falls back to the outer one;
//! keyed providers in a loop keep their values when reordered, without running again.

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose::runtime::platform::*;
use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// D81: a local is identified by its address, so it must be a `static`. A `const` compiles
/// too (`provides` and `current` take `&'static self`, which constant promotion supplies), but
/// each use promotes its own copy, so the reader never finds the provider and silently reads
/// the default. Compose has no such trap; here it should be rejected at compile time or work.
const CONST_LOCAL: CompositionLocal<&'static str> =
    CompositionLocal::new("ConstLocal", || "default");

#[composable]
fn ConstReader() {
    log(format!("const: {}", CONST_LOCAL.current()));
}

#[test]
fn a_const_local_is_found_by_its_reader() {
    take_log();
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(|| {
        CompositionLocalProvider(CONST_LOCAL.provides("provided"), ConstReader);
    });
    assert_eq!(take_log(), ["const: provided"]);
}
