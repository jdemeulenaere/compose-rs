//! Fifth review round: `MutableInteractionSource` with `clickable_with`, and
//! `clear_and_set_semantics`, against Jetpack Compose. Each test states the input, what Compose
//! does, and what happens instead.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::ClickableOptions;
use compose_ui::{MutableInteractionSource, Offset, PointerEvent, UnitExt};

#[composable]
fn Pressable(
    source: MutableInteractionSource,
    shown: MutableState<bool>,
    enabled: MutableState<bool>,
) {
    if shown.get() {
        Box!(
            modifier = Modifier
                .clickable_with(
                    ClickableOptions {
                        enabled: enabled.get(),
                        indication: compose_ui::Color::UNSPECIFIED.into(),
                        interaction_source: Some(source.clone()),
                        ..ClickableOptions::DEFAULT
                    },
                    || {}
                )
                .size(40.dp()),
        );
    }
}

fn setup() -> (ComposeTestRule, MutableInteractionSource, OwnedState<bool>, OwnedState<bool>) {
    let source = MutableInteractionSource::new();
    let shown = mutable_state_of(true);
    let enabled = mutable_state_of(true);
    let (shown_handle, enabled_handle) = (shown.handle(), enabled.handle());
    let ui = compose_test({
        let source = source.clone();
        move || Pressable(source.clone(), shown_handle, enabled_handle)
    });
    (ui, source, shown, enabled)
}

/// A clickable removed from the composition while pressed (the finger still down): Compose's
/// clickable node, detached, ends its press (`onDetach` → `disposeInteractions`, emitting
/// `PressInteraction.Cancel`), so `collectIsPressedAsState` goes back to false. Here
/// `ClickableNode` has no `on_detach`: the source stays pressed for good (a navigation item's
/// pressed pill left drawn).
#[test]
fn a_clickable_removed_mid_press_releases_its_interaction_source() {
    let (mut ui, source, shown, _enabled) = setup();
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert!(source.is_pressed());
    shown.set(false);
    ui.wait_for_idle();
    assert!(!source.is_pressed(), "the press of a clickable gone");
}

/// A clickable disabled while pressed: Compose's `AbstractClickableNode.update` calls
/// `disposeInteractions()` when `enabled` turns false, emitting `PressInteraction.Cancel`. Here
/// the element update clears the node's own press (`disable_if`) without telling the source
/// (`publish_press` isn't called): it stays pressed.
#[test]
fn a_clickable_disabled_mid_press_releases_its_interaction_source() {
    let (mut ui, source, _shown, enabled) = setup();
    ui.host().send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    ui.wait_for_idle();
    assert!(source.is_pressed());
    enabled.set(false);
    ui.wait_for_idle();
    assert!(!source.is_pressed(), "the press of a disabled clickable");
}

/// Compose's `Text("Hello", Modifier.clearAndSetSemantics { contentDescription = "Greeting" })`
/// reads as "Greeting" only: `collapsedSemantics` applies a node's semantics modifiers from the
/// tail, and a clearing one resets what the modifiers after it (here the text's own semantics
/// node, at the tail of the chain) set. Here the clearing only hides the descendants: the node
/// still has its text.
#[test]
fn clear_and_set_semantics_clears_the_semantics_after_it_on_the_same_node() {
    let mut ui = compose_test(|| {
        BasicText!(
            "Hello",
            modifier = Modifier.test_tag("greeting").clear_and_set_semantics(|config| {
                config.set_content_description("Greeting");
            }),
        );
    });
    let node = ui.on_node_with_tag("greeting").fetch_semantics_node();
    assert_eq!(
        node.config
            .get(&compose_ui::SemanticsProperties::CONTENT_DESCRIPTION)
            .cloned()
            .unwrap_or_default()
            .len(),
        1
    );
    assert!(
        node.config
            .get(&compose_ui::SemanticsProperties::TEXT)
            .cloned()
            .unwrap_or_default()
            .is_empty(),
        "the text is cleared: {:?}",
        node.config.get(&compose_ui::SemanticsProperties::TEXT).cloned().unwrap_or_default()
    );
}
