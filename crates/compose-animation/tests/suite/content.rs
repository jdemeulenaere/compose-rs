//! `AnimatedContent`: the new state's content enters as the old one's exits.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::UnitExt;

#[composable]
fn Counter(count: MutableState<i32>) {
    AnimatedContent!(count.get(), modifier = Modifier.test_tag("container"), |value: i32| {
        Box!(
            modifier =
                Modifier.size_wh((50 + value * 50).dp(), 20.dp()).test_tag(format!("{value}")),
            {
                BasicText!("{}", value);
            }
        );
    });
}

#[test]
fn new_content_enters_as_the_old_one_exits() {
    let count = mutable_state_of(0);
    let handle = count.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Counter(handle));
    ui.on_node_with_tag("0").assert_exists();
    count.set(1);
    ui.advance_time_by(Duration::from_millis(50));
    // Both while the old fades out (90 ms) and the new waits to come in (after 90 ms).
    ui.on_node_with_tag("0").assert_exists();
    ui.on_node_with_tag("1").assert_exists();
    ui.advance_time_by(Duration::from_millis(1000));
    ui.on_node_with_tag("0").assert_does_not_exist();
    ui.on_node_with_tag("1").assert_exists();
    // The container ends at the new content's size.
    let container = ui.on_node_with_tag("container").fetch_semantics_node().bounds;
    assert_eq!(container.width(), 100.0);
}

#[test]
fn a_transform_without_animation_switches_at_once() {
    let count = mutable_state_of(0);
    let handle = count.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        AnimatedContent!(
            handle.get(),
            transition_spec = |_, _| EnterTransition::NONE
                .together_with(ExitTransition::NONE)
                .without_size_transform(),
            |value: i32| BasicText!("value {}", value)
        );
    });
    count.set(1);
    ui.advance_time_by(Duration::from_millis(16));
    ui.on_node_with_text("value 1").assert_exists();
    ui.on_node_with_text("value 0").assert_does_not_exist();
}
