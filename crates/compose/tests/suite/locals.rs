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

#[test]
#[should_panic(expected = "outside a composition pass")]
fn reading_after_a_pass_is_a_misuse() {
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(|| {
        CompositionLocalProvider(LOCAL_NAME.provides("provided"), || {
            let _ = LOCAL_NAME.current();
        });
    });
    let _ = LOCAL_NAME.current();
}

// ---- Providers found through provider groups only (D310) ----------------------------------

#[composable]
fn Wrapper(tick: MutableState<u32>) {
    Reader("deepest", tick);
}

#[composable]
fn Nested(inner: MutableState<&'static str>, tick: MutableState<u32>) {
    CompositionLocalProvider(LOCAL_NAME.provides("outer"), || {
        Reader("before", tick);
        CompositionLocalProvider(LOCAL_NAME.provides(inner.get()), || {
            Reader("nested", tick);
            CompositionLocalProvider(LOCAL_OTHER.provides("other"), || Wrapper(tick));
        });
        Reader("after", tick);
    });
}

#[test]
fn the_nearest_of_nested_providers_wins_and_siblings_after_see_the_outer_one() {
    take_log();
    let inner = mutable_state_of("inner");
    let tick = mutable_state_of(0);
    let (inner_handle, tick_handle) = (inner.handle(), tick.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || Nested(inner_handle, tick_handle));
    let expected = |tick: u32, inner: &str| {
        [
            format!("before: outer other default ({tick})"),
            format!("nested: {inner} other default ({tick})"),
            format!("deepest: {inner} other ({tick})"),
            format!("after: outer other default ({tick})"),
        ]
    };
    assert_eq!(take_log(), expected(0, "inner"));
    // Each reader restarts on its own, walked to through the providers.
    tick.set(1);
    composition.recompose();
    assert_eq!(take_log(), expected(1, "inner"));
    // The middle provider's new value: its readers only.
    inner.set("changed");
    composition.recompose();
    assert_eq!(take_log(), ["nested: changed other default (1)", "deepest: changed other (1)"]);
    tick.set(2);
    composition.recompose();
    assert_eq!(take_log(), expected(2, "changed"));
}

#[composable]
fn Toggled(provided: MutableState<bool>, tick: MutableState<u32>) {
    CompositionLocalProvider(LOCAL_NAME.provides("outer"), || {
        if provided.get() {
            CompositionLocalProvider(LOCAL_NAME.provides("inner"), || Reader("reader", tick));
        } else {
            Reader("reader", tick);
        }
        // After the toggled provider, in groups that may reuse its freed ones.
        Reader("sibling", tick);
    });
}

#[test]
fn a_removed_provider_leaves_its_readers_with_the_one_around_it() {
    take_log();
    let provided = mutable_state_of(true);
    let tick = mutable_state_of(0);
    let (provided_handle, tick_handle) = (provided.handle(), tick.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || Toggled(provided_handle, tick_handle));
    assert_eq!(take_log(), ["reader: inner other default (0)", "sibling: outer other default (0)"]);
    for round in 1..4 {
        provided.set(false);
        composition.recompose();
        tick.set(2 * round - 1);
        composition.recompose();
        assert_eq!(
            take_log(),
            [
                format!("reader: outer other default ({})", 2 * round - 2),
                format!("reader: outer other default ({})", 2 * round - 1),
                format!("sibling: outer other default ({})", 2 * round - 1),
            ]
        );
        provided.set(true);
        composition.recompose();
        tick.set(2 * round);
        composition.recompose();
        assert_eq!(
            take_log(),
            [
                format!("reader: inner other default ({})", 2 * round - 1),
                format!("reader: inner other default ({})", 2 * round),
                format!("sibling: outer other default ({})", 2 * round),
            ]
        );
    }
}

#[composable]
fn Keyed(order: MutableState<Vec<&'static str>>, tick: MutableState<u32>) {
    for name in order.get() {
        key(name, move || {
            CompositionLocalProvider(LOCAL_NAME.provides(name), move || Reader(name, tick))
        });
    }
}

#[test]
fn keyed_providers_moved_among_their_siblings_keep_their_readers() {
    take_log();
    let order = mutable_state_of(vec!["a", "b", "c"]);
    let tick = mutable_state_of(0);
    let (order_handle, tick_handle) = (order.handle(), tick.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || Keyed(order_handle, tick_handle));
    assert_eq!(
        take_log(),
        ["a: a other default (0)", "b: b other default (0)", "c: c other default (0)"]
    );
    order.set(vec!["c", "a", "b"]);
    composition.recompose();
    assert!(take_log().is_empty());
    tick.set(1);
    composition.recompose();
    assert_eq!(
        take_log(),
        ["c: c other default (1)", "a: a other default (1)", "b: b other default (1)"]
    );
}
