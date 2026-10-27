//! Adversarial review of Material components' semantics (what a screen reader is told), against
//! Jetpack Compose's Material 3.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{Modifier, Role, UnitExt};

/// Compose's `Slider` is a semantics node of its own (`progressSemantics` is
/// `semantics(mergeDescendants = true)`): inside a clickable card, it stays a node a screen
/// reader can reach and adjust; it isn't merged into the card (which would make the card
/// "disabled" with a disabled slider, and take its value).
#[test]
fn a_slider_in_a_clickable_card_is_its_own_node() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Card!(on_click = || {}, modifier = Modifier.test_tag("card"), {
                Text!("Volume");
                Slider!(
                    0.5,
                    |_| {},
                    modifier = Modifier.width(200.dp()).test_tag("slider"),
                    enabled = false
                );
            });
        });
    });
    let card = ui.on_node_with_tag("card").fetch_semantics_node();
    assert!(
        !card.config.contains(&compose_ui::SemanticsProperties::DISABLED),
        "the disabled slider made the card disabled: {:?}",
        card.config
    );
    assert_eq!(
        card.config.get(&compose_ui::SemanticsProperties::STATE_DESCRIPTION).cloned(),
        None,
        "the card took the slider's value"
    );
    ui.on_node_with_tag("slider").assert_exists();
}

/// A `stateDescription` set on the slider's `modifier` (outside its own semantics) is what
/// Compose reads: outer semantics modifiers win, the documented way to describe a slider's value
/// in words.
#[test]
fn a_sliders_own_state_description_is_kept() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Slider!(
                0.5,
                |_| {},
                modifier = Modifier
                    .width(200.dp())
                    .test_tag("slider")
                    .semantics(|config| config.set_state_description("Loud")),
            );
        });
    });
    let node = ui.on_node_with_tag("slider").fetch_semantics_node();
    assert_eq!(
        node.config
            .get(&compose_ui::SemanticsProperties::STATE_DESCRIPTION)
            .cloned()
            .as_ref()
            .map(|s| s.as_str()),
        Some("Loud")
    );
}

/// A clickable row (no role) with a described `Icon` has no role in Compose (`Role` merges as
/// `parentValue`), not the icon's `Role.Image`.
#[test]
fn a_clickable_row_with_an_icon_is_not_an_image() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Row!(modifier = Modifier.clickable(|| {}).test_tag("row"), {
                Icon!(icons::filled::add(), Some("Add"));
                Text!("New item");
            });
        });
    });
    let node = ui.on_node_with_tag("row").fetch_semantics_node();
    assert_ne!(
        node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
        Some(Role::Image),
        "{:?}",
        node.config
    );
}

/// Lock-in: a disabled `Button` is disabled and offers no click; a `Switch` is a toggle with the
/// switch role; a `TextField` is one editable node with its label's text.
#[test]
fn passes_buttons_switches_and_text_fields_describe_themselves() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                Button!(
                    on_click = || {},
                    modifier = Modifier.test_tag("button"),
                    enabled = false,
                    {
                        Text!("Save");
                    }
                );
                Switch!(true, Some(|_| {}), modifier = Modifier.test_tag("switch"));
                TextField!(
                    "Ada".to_string(),
                    |_| {},
                    modifier = Modifier.test_tag("field"),
                    label = Some(|| Text!("Name"))
                );
            });
        });
    });
    let button = ui.on_node_with_tag("button").fetch_semantics_node();
    assert!(button.config.contains(&compose_ui::SemanticsProperties::DISABLED));
    assert_eq!(
        button.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
        Some(Role::Button)
    );
    let button_id = button.id;
    assert!(
        !ui.host().perform_semantics_action(button_id, &compose_ui::SemanticsActions::ON_CLICK)
    );
    let switch = ui.on_node_with_tag("switch").fetch_semantics_node();
    assert_eq!(
        switch.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
        Some(Role::Switch)
    );
    assert_eq!(
        switch.config.get(&compose_ui::SemanticsProperties::TOGGLEABLE_STATE).copied(),
        Some(compose_ui::ToggleableState::On)
    );
    let field = ui.on_node_with_tag("field").fetch_semantics_node();
    assert!(field.config.contains(&compose_ui::SemanticsProperties::EDITABLE_TEXT));
    assert_eq!(
        field
            .config
            .get(&compose_ui::SemanticsProperties::EDITABLE_TEXT)
            .cloned()
            .as_ref()
            .map(|s| s.as_str()),
        Some("Ada")
    );
    assert_eq!(
        field
            .config
            .get(&compose_ui::SemanticsProperties::TEXT)
            .into_iter()
            .flatten()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        ["Name"]
    );
}
