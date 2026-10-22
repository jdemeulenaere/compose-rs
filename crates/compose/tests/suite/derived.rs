//! `derived_state_of`, as Compose's `derivedStateOf`: readers recompose when the derived value
//! changes, not on every write to what it reads.

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

fn composition() -> Composition {
    take_log();
    Composition::new(NodeTree::<()>::new())
}

#[composable]
fn Status(items: MutableState<Vec<u32>>) {
    let empty = remember(move || derived_state_of(move || items.with(Vec::is_empty)));
    log(format!("empty: {}", empty.get()));
}

#[test]
fn readers_recompose_only_when_the_derived_value_changes() {
    let composition = composition();
    let items = mutable_state_list_of::<u32>();
    let handle = items.handle();
    composition.set_content(move || Status(handle));
    assert_eq!(take_log(), ["empty: true"]);
    items.edit(|items| items.push(1));
    composition.recompose();
    assert_eq!(take_log(), ["empty: false"]);
    // Still not empty: the screen doesn't recompose.
    items.edit(|items| items.push(2));
    assert!(!composition.has_invalidations());
    composition.recompose();
    assert_eq!(take_log(), Vec::<String>::new());
    items.edit(Vec::clear);
    composition.recompose();
    assert_eq!(take_log(), ["empty: true"]);
}

#[test]
fn a_derived_state_reads_fresh_after_a_write() {
    let count = mutable_state_of(2);
    let handle = count.handle();
    let doubled = derived_state_of(move || handle.get() * 2);
    assert_eq!(doubled.get(), 4);
    count.set(5);
    assert_eq!(doubled.get(), 10);
    assert_eq!(doubled.as_state().get(), 10);
}

#[test]
fn derived_states_chain() {
    let count = mutable_state_of(1);
    let handle = count.handle();
    let doubled = derived_state_of(move || handle.get() * 2);
    let doubled_state = doubled.as_state();
    let described = derived_state_of(move || format!("{} items", doubled_state.get()));
    assert_eq!(described.get(), "2 items");
    count.set(3);
    assert_eq!(described.get(), "6 items");
}

#[composable]
fn Total(prices: MutableState<Vec<u32>>, tax: MutableState<u32>) {
    // Reads in the calculation don't make this scope a reader of `prices` or `tax`.
    let total = remember(move || {
        derived_state_of(move || prices.with(|prices| prices.iter().sum::<u32>()) + tax.get())
    });
    log(format!("total: {}", total.get()));
}

#[test]
fn a_change_that_keeps_the_value_recomposes_nothing() {
    let composition = composition();
    let prices = mutable_state_of(vec![1, 2]);
    let tax = mutable_state_of(1);
    let (prices_handle, tax_handle) = (prices.handle(), tax.handle());
    composition.set_content(move || Total(prices_handle, tax_handle));
    assert_eq!(take_log(), ["total: 4"]);
    // 1 + 2 + 1 = 4 = 3 + 0 + 1: the same total.
    prices.set(vec![3, 0]);
    assert!(!composition.has_invalidations(), "the total didn't change");
    tax.set(2);
    composition.recompose();
    assert_eq!(take_log(), ["total: 5"]);
}

#[test]
fn a_dropped_derived_state_stops_recomputing() {
    let count = mutable_state_of(1);
    let handle = count.handle();
    let doubled = derived_state_of(move || {
        log("calculated");
        handle.get() * 2
    });
    assert_eq!(take_log(), ["calculated"]);
    drop(doubled);
    count.set(2);
    assert_eq!(take_log(), Vec::<String>::new());
}

#[test]
fn a_write_recomputes_once() {
    let count = mutable_state_of(1);
    let handle = count.handle();
    let doubled = derived_state_of(move || {
        log("calculated");
        handle.get() * 2
    });
    take_log();
    count.set(2);
    assert_eq!(doubled.get(), 4);
    assert_eq!(take_log(), ["calculated"]);
}
