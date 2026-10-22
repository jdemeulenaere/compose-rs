//! The testing section's samples (docs/guide/kotlin-to-rust.md, "Testing").

use std::time::Duration;

use compose::test::*;
use compose::ui::Role;
use compose::ui::platform::UiHost;
use guide::{effects, navigation, state};

// ANCHOR: rule
#[test]
fn opening_a_message_and_going_back() {
    let mut ui = compose_test(navigation::App); // `createComposeRule()` + `setContent { App() }`
    ui.on_node_with_text("Message 3").perform_click();
    ui.on_node_with_content_description("Star").perform_click().assert_is_on();
    assert_eq!(ui.count_nodes(&has_role(Role::Checkbox)), 1); // a toggle button is a checkbox
    ui.host().on_back_pressed(); // the system back
    ui.wait_for_idle();
    ui.on_node_with_text("Message 3").assert_exists();
    ui.on_node(has_text("Settings").and(has_click_action())).assert_is_enabled();
}
// ANCHOR_END: rule

// ANCHOR: recompositions
#[test]
fn a_click_recomposes_only_the_button_content() {
    let mut ui = compose_test(state::Counter);
    let before = ui.recomposition_counts();
    ui.on_node_with_text("Clicked 0 times").perform_click();
    let ran = ui.recomposition_counts().since(&before);
    // The text reads the count inside the button's content: `Counter` itself skips.
    assert_eq!(ran.scopes_named("Counter"), 0, "{:?}", ran.names());
}
// ANCHOR_END: recompositions

// ANCHOR: clock
#[test]
fn the_countdown_counts_down_on_the_test_clock() {
    // `mainClock.autoAdvance = false`: time moves only when the test says.
    let mut ui = ComposeTestRule::with_manual_clock(|| effects::Countdown(3));
    ui.on_node_with_text("3 s").assert_exists();
    ui.advance_time_by(Duration::from_secs(1));
    ui.on_node_with_text("2 s").assert_exists();
    // `compose_test`'s clock advances by itself: waiting for idle runs through the delays.
    let mut ui = compose_test(|| effects::Countdown(3));
    ui.on_node_with_text("0 s").assert_exists();
}
// ANCHOR_END: clock

// ANCHOR: host
#[test]
fn the_back_stack_survives_recreation() {
    // The headless host the test rule wraps: what an Android view or a desktop window drives.
    let mut ui = compose_test(navigation::App);
    ui.on_node_with_text("Message 3").perform_click();
    // What the Android view saves in its instance state when the activity goes away…
    let saved = ui.host().save_state();
    drop(ui);
    // …and gives a new composition when it's recreated.
    let host = UiHost::headless(400, 800);
    host.restore_state(saved);
    let mut ui = ComposeTestRule::with_host(host, navigation::App);
    ui.on_node_with_content_description("Star").assert_exists(); // still on the message
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_text("Message 3").assert_exists(); // and back goes to the inbox
}
// ANCHOR_END: host

// ANCHOR: script
#[test]
fn the_session_matches_its_transcript() {
    let mut ui = compose_test(navigation::App);
    // Runs tests/navigation.script and compares the transcript with navigation.script.expected
    // (`UPDATE_SCRIPTS=1 cargo test` writes it).
    check_script(&mut ui, concat!(env!("CARGO_MANIFEST_DIR"), "/tests/navigation.script"));
}
// ANCHOR_END: script
