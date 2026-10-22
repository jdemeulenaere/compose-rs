//! An app calling the library's composables. Exits with status 1, listing the composables that
//! lost their remembered state, if any did.

#![allow(non_snake_case)]

use compose::runtime::*;
use compose::runtime::platform::*;
use widgets::*;

#[composable]
fn Screen(tick: MutableState<u32>, outer: MutableState<u32>) {
    let _ = outer.get();
    Inlined("inlined", tick);
    InlinedAlways("always", tick);
    Generic(9u8, tick);
    Generic(1i64, tick);
    Plain("plain", tick);
    Section(tick);
    // Defaults resolved in `widgets`, by an imported and a path call macro.
    Clicker!();
    widgets::Clicker!(label = "path");
}

fn main() {
    let tick = mutable_state_of(0u32);
    let outer = mutable_state_of(0u32);
    let (tick_handle, outer_handle) = (tick.handle(), outer.handle());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || Screen(tick_handle, outer_handle));
    LOG.with(|log| log.borrow_mut().clear());
    for round in 1..=2 {
        // Each composable restarts on its own (its restart function calls it)...
        tick.set(round);
        composition.recompose();
        // ...then is called again by its recomposing parent.
        outer.set(round);
        composition.recompose();
    }
    let log = LOG.with(|log| log.borrow().clone());
    let lost: Vec<&String> = log.iter().filter(|entry| entry.starts_with("create")).collect();
    if lost.is_empty() {
        println!("ok");
    } else {
        println!("state recreated: {lost:?}");
        std::process::exit(1);
    }
}
