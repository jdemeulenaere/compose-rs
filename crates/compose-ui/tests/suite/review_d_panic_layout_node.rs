//! Phase D review: a layout modifier node panicking once breaks its node for good.
//!
//! `measure_layer` swaps the running layout modifier node out of its chain for `Taken`; the
//! unwind drops the real node (without `on_detach`) and leaves `Taken` in the chain, so the
//! next measurement of the node panics with "compose-ui: a layout node". The unwind also drops
//! the ancestors' children lists (review_d_panic_measure.rs), which today hides the node from
//! later frames, so this test first fails at "removing the node panics" (`UiTree::remove` past
//! the root's empty child list); with that fixed, the `Taken` left in the chain shows. One test
//! per binary: the corrupt tree panics again on drop, which aborts at thread exit.

#![allow(non_snake_case)]

use crate::review_d_support;
use crate::support;

use std::cell::Cell;

use compose_runtime::*;
use compose_ui::*;
use review_d_support::*;

#[test]
fn a_panicking_layout_modifier_node_recovers_and_is_detached_once() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut host = support::host(move || {
        if handle.get() {
            support::Leaf(Modifier.size(10.dp()).then_custom(FragileElement));
        }
    });
    DETACHED.with(|detached| detached.set(0));
    set_fail(true);
    host.resize(IntSize::new(300, 300));
    assert!(quiet(|| host.frame()).is_err());
    set_fail(false);
    host.resize(IntSize::new(200, 100));
    let again = quiet(|| host.frame());
    let message = again.as_ref().err().and_then(|panic| panic.downcast_ref::<String>().cloned());
    shown.set(false);
    let removed = quiet(|| host.frame()).is_ok();
    let dropped = quiet(move || drop(host)).is_ok();
    assert!(again.is_ok(), "a frame after the panic panics again: {message:?}");
    assert!(removed, "removing the node panics");
    assert_eq!(DETACHED.with(Cell::get), 1, "the node was detached exactly once");
    assert!(dropped, "dropping the host panicked");
}
