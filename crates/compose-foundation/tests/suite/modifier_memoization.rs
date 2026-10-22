//! Every `Modifier` method taking a callback memoizes a closure argument on its captures, so a
//! child given the modifier skips when its parent recomposes (compose-macros'
//! `CALLBACK_MODIFIERS`).

#![allow(non_snake_case)]

use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{ClickableOptions, Role, UnitExt};

#[composable]
fn Leaf(modifier: Modifier) {
    Box!(modifier = modifier.size(10.dp()));
}

const KINDS: [&str; 12] = [
    "clickable",
    "clickable_with",
    "toggleable",
    "toggleable_with",
    "selectable",
    "selectable_with",
    "combined_clickable",
    "combined_clickable_with",
    "on_focus_changed",
    "on_key_event",
    "on_size_changed",
    "on_globally_positioned",
];

#[composable]
fn Parent(kind: usize, counter: MutableState<i32>) {
    // The parent recomposes on each write.
    let _ = counter.get();
    let options = ClickableOptions { role: Some(Role::Checkbox), ..ClickableOptions::DEFAULT };
    match kind {
        0 => Leaf(Modifier.clickable(move || counter.set(0))),
        1 => Leaf(Modifier.clickable_with(options, move || counter.set(0))),
        2 => Leaf(Modifier.toggleable(true, move |_| counter.set(0))),
        3 => Leaf(Modifier.toggleable_with(true, options, move |_| counter.set(0))),
        4 => Leaf(Modifier.selectable(true, move || counter.set(0))),
        5 => Leaf(Modifier.selectable_with(true, options, move || counter.set(0))),
        6 => Leaf(Modifier.combined_clickable(None, None, move || counter.set(0))),
        7 => Leaf(Modifier.combined_clickable_with(options, None, None, move || counter.set(0))),
        8 => Leaf(Modifier.on_focus_changed(move |_| counter.set(0))),
        9 => Leaf(Modifier.on_key_event(move |_| {
            counter.set(0);
            false
        })),
        10 => Leaf(Modifier.on_size_changed(move |_| counter.set(0))),
        _ => Leaf(Modifier.on_globally_positioned(move |_| counter.set(0))),
    }
}

#[test]
fn closures_given_to_callback_modifiers_are_memoized() {
    let mut recomposed = Vec::new();
    for (kind, name) in KINDS.iter().enumerate() {
        let owned = Rc::new(mutable_state_of(0));
        let counter = owned.handle();
        let mut ui = compose_test(move || Parent(kind, counter));
        let before = ui.recomposition_counts();
        counter.set(1);
        ui.wait_for_idle();
        let after = ui.recomposition_counts().since(&before);
        assert_eq!(after.scopes_named("Parent"), 1, "{name}: the parent recomposes");
        if after.scopes_named("Leaf") != 0 {
            recomposed.push(*name);
        }
    }
    assert!(recomposed.is_empty(), "children recomposed with these modifiers: {recomposed:?}");
}
