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
    AnimatedContent!(count.get(), modifier = Modifier.test_tag("container"), |_, value: i32| {
        Box!(
            modifier =
                Modifier.size(((50 + value * 50).dp(), 20.dp())).test_tag(format!("{value}")),
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
            transition_spec =
                |_| EnterTransition::NONE.together_with(ExitTransition::NONE).using(None),
            |_, value: i32| BasicText!("value {}", value)
        );
    });
    count.set(1);
    // A frame recomposes for the change (Compose recomposes only on frames, D900), the next
    // ends the transition, which has nothing to animate.
    ui.advance_time_by(Duration::from_millis(32));
    ui.on_node_with_text("value 1").assert_exists();
    ui.on_node_with_text("value 0").assert_does_not_exist();
}

#[test]
fn a_transition_animates_its_values_together_then_reaches_its_target() {
    use std::cell::Cell;
    use std::rc::Rc;
    let expanded = mutable_state_of(false);
    let handle = expanded.handle();
    let seen: Rc<Cell<(f32, bool, bool)>> = Rc::default();
    let report = seen.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "Expand");
        let width = transition
            .animate_dp(|_| tween(100).into(), |&open| if open { 200.dp() } else { 100.dp() });
        let alpha =
            transition.animate_float(|_| tween(200).into(), |&open| if open { 1.0 } else { 0.5 });
        report.set((width.get().0, transition.current_state(), transition.is_running()));
        Box!(modifier = Modifier.size((width.get(), 10.dp())).alpha(alpha.get()).test_tag("box"));
    });
    assert_eq!(seen.get(), (100.0, false, false));
    expanded.set(true);
    ui.advance_time_by(Duration::from_millis(48));
    let (width, current, running) = seen.get();
    assert!(width > 100.0 && width < 200.0, "{width}");
    assert!(!current && running);
    // The width done after 100 ms (it starts a frame or two in), the alpha after 200: only
    // then at its target.
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(seen.get().0, 200.0);
    assert!(!seen.get().1, "the alpha still animates");
    ui.advance_time_by(Duration::from_millis(150));
    assert_eq!(seen.get(), (200.0, true, false));
}
