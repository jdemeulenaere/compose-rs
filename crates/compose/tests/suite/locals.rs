//! Composition locals: provided values found from anywhere below, including scopes restarting
//! on their own, and new values recomposing exactly their readers.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};

use compose::runtime::platform::*;
use compose::runtime::*;

static LOCAL_NAME: CompositionLocal<&'static str> =
    CompositionLocal::new("LocalName", || "default");
static LOCAL_OTHER: CompositionLocal<&'static str> =
    CompositionLocal::new("LocalOther", || "other default");

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static MIDDLE_RUNS: Cell<u32> = const { Cell::new(0) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

#[composable]
fn Reader(label: &'static str, tick: MutableState<u32>) {
    let tick = tick.get();
    log(format!("{label}: {} {} ({tick})", LOCAL_NAME.current(), LOCAL_OTHER.current()));
}

#[composable]
fn Middle(tick: MutableState<u32>) {
    MIDDLE_RUNS.with(|runs| runs.set(runs.get() + 1));
    Reader("deep", tick);
}

#[composable]
fn Screen(name: MutableState<&'static str>, tick: MutableState<u32>) {
    Reader("outside", tick);
    CompositionLocalProvider(LOCAL_NAME.provides(name.get()), || {
        Middle(tick);
        CompositionLocalProvider(LOCAL_NAME.provides("inner"), || {
            CompositionLocalProvider(LOCAL_OTHER.provides("other"), || Reader("inner", tick));
        });
    });
}

#[test]
fn providers_reach_their_subtree_and_new_values_recompose_only_readers() {
    take_log();
    let name = mutable_state_of("first");
    let tick = mutable_state_of(0);
    let (name_handle, tick_handle) = (name.handle(), tick.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || Screen(name_handle, tick_handle));
    assert_eq!(
        take_log(),
        [
            "outside: default other default (0)",
            "deep: first other default (0)",
            "inner: inner other (0)"
        ]
    );
    // A reader restarting on its own still finds its provider.
    tick.set(1);
    composition.recompose();
    assert_eq!(
        take_log(),
        [
            "outside: default other default (1)",
            "deep: first other default (1)",
            "inner: inner other (1)"
        ]
    );
    // A new value: the deep reader recomposes; the middle composable between doesn't, nor the
    // inner reader, whose nearer provider didn't change.
    MIDDLE_RUNS.with(|runs| runs.set(0));
    name.set("second");
    composition.recompose();
    assert_eq!(take_log(), ["deep: second other default (1)"]);
    assert_eq!(MIDDLE_RUNS.with(Cell::get), 0);
    // The same value again: nothing.
    name.set("second");
    composition.recompose();
    assert!(take_log().is_empty());
}

#[test]
#[should_panic(expected = "outside a composition pass")]
fn reading_outside_composition_is_a_misuse() {
    let _ = LOCAL_NAME.current();
}
