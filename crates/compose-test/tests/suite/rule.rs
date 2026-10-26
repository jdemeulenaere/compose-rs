//! The test rule over a small counter: finders, actions, assertions and an event script.

#![allow(non_snake_case)]

use compose_runtime::*;
use compose_test::*;
use compose_ui::*;
use crate::support::*;

#[composable]
fn Counter() {
    let count = remember(|| mutable_state_of(0));
    Column!(modifier = Modifier.padding(16.dp()), {
        Row!(
            modifier =
                Modifier.clickable(move || count.update(|count| count + 1)).test_tag("increment"),
            {
                Text!("Count: {}", count.get());
            }
        );
        Row!(
            modifier = Modifier
                .clickable_with(
                    ClickableOptions {
                        enabled: count.get() > 0,
                        role: Some(Role::Button),
                        ..ClickableOptions::DEFAULT
                    },
                    move || count.set(0)
                )
                .test_tag("reset"),
            {
                Text!("Reset");
            }
        );
    });
}

#[test]
fn finders_actions_and_assertions() {
    let mut ui = compose_test(Counter);
    ui.on_node_with_text("Reset").assert_is_not_enabled();
    for _ in 0..3 {
        ui.on_node(has_text_containing("Count")).perform_click();
    }
    ui.on_node_with_text("Count: 3").assert_exists().assert_is_enabled();
    ui.on_node_with_text("Count: 0").assert_does_not_exist();
    ui.on_node_with_tag("reset").assert_is_enabled().assert_text_equals(&["Reset"]);
    ui.on_node_with_tag("reset").perform_click();
    ui.on_node_with_text("Count: 0").assert_exists();
    assert_eq!(ui.count_nodes(&has_click_action()), 2);
    // Keyboard: Tab to the first clickable, Enter clicks it.
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("increment").assert_is_focused();
    ui.press_key(Key::Enter);
    ui.on_node_with_text("Count: 1").assert_exists();
}

#[test]
#[should_panic(expected = "expected exactly 1 node with text \"Missing\", found 0")]
fn a_missing_node_fails_with_the_tree() {
    let mut ui = compose_test(Counter);
    ui.on_node_with_text("Missing").perform_click();
}

/// The rules' hosts use the recommended set unless a test says otherwise, so tests exercise what
/// ships (D610); `use_optimizations` sets another for the rules made after it.
#[test]
fn the_rules_default_to_the_recommended_set() {
    use compose_ui::platform::Optimizations;
    let recommended = Optimizations::recommended();
    assert_eq!(compose_test(Counter).host().optimizations(), recommended);
    assert_eq!(ComposeTestRule::with_manual_clock(Counter).host().optimizations(), recommended);
    assert_eq!(
        ComposeTestRule::sized(IntSize::new(100, 100), Counter).host().optimizations(),
        recommended
    );
    use_optimizations(Optimizations::NONE);
    assert_eq!(compose_test(Counter).host().optimizations(), Optimizations::NONE);
    assert_eq!(
        ComposeTestRule::with_manual_clock(Counter).host().optimizations(),
        Optimizations::NONE
    );
}

#[test]
fn an_event_script_records_its_session() {
    let mut ui = compose_test(Counter);
    check_script(&mut ui, concat!(env!("CARGO_MANIFEST_DIR"), "/tests/scripts/counter.script"));
}
