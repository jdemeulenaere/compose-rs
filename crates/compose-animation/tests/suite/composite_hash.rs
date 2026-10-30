//! Compose's runtime integration `CompositeHashTests`: the composite key hash inside
//! `AnimatedVisibility` stays the same when its modifier changes (bug 338478720). Without
//! `consumeWindowInsets` (not in compose-rs yet), a padding stands for the first modifier.

#![allow(non_snake_case)]

use compose_animation::*;
use compose_foundation::layout::PaddingValues;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Modifier, UnitExt};

#[composable]
fn Content(use_first_modifier: MutableState<bool>, checks: std::rc::Rc<std::cell::Cell<u32>>) {
    let modifier = if use_first_modifier.get() {
        Modifier.padding(PaddingValues::all(0.dp()))
    } else {
        Modifier
    };
    AnimatedVisibility!(true, modifier = modifier, {
        // Recomposes with the change.
        use_first_modifier.get();
        let hash = current_composite_key_hash_code();
        let original = remember(|| hash);
        assert_eq!(original, hash);
        checks.set(checks.get() + 1);
    });
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/CompositeHashTests.kt, testMultipleSetContentCalls
#[test]
fn the_hash_inside_animated_visibility_survives_a_modifier_change() {
    let use_first_modifier = mutable_state_of(true);
    let checks = std::rc::Rc::new(std::cell::Cell::new(0));
    let (handle, c) = (use_first_modifier.handle(), checks.clone());
    let mut ui = compose_test(move || Content(handle, c.clone()));
    ui.wait_for_idle();
    let before = checks.get();
    use_first_modifier.set(false);
    ui.wait_for_idle();
    assert!(checks.get() > before, "the content recomposed");
}
