//! Phase D review: dropping a host whose composition was poisoned by a panicking composable
//! releases every node exactly once (the applier's "release of a still-attached node").

#![allow(non_snake_case)]

use crate::review_d_support;
use crate::support;

use std::cell::Cell;

use compose_runtime::*;
use compose_ui::*;
use review_d_support::*;
use support::{Column, Leaf};

#[composable]
fn Bomb(armed: MutableState<bool>) {
    if armed.get() {
        panic!("review: composable failed");
    }
    Leaf(Modifier.size(10.dp()).then_custom(FragileElement));
}

#[test]
fn a_poisoned_composition_releases_each_node_once() {
    let armed = mutable_state_of(false);
    let handle = armed.handle();
    let mut host = support::host(move || {
        Column!({
            Leaf(Modifier.size(10.dp()).then_custom(FragileElement));
            Column!({
                Leaf(Modifier.size(10.dp()).then_custom(FragileElement));
                Bomb(handle);
            });
        });
    });
    DETACHED.with(|detached| detached.set(0));
    armed.set(true);
    assert!(quiet(|| host.frame()).is_err());
    assert!(quiet(move || drop(host)).is_ok(), "dropping the poisoned host panicked");
    assert_eq!(DETACHED.with(Cell::get), 3);
}
