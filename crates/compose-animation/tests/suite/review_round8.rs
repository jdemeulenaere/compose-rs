//! Eighth review round: `AnimatedContent` and `update_transition`, against Jetpack Compose's
//! animation library. Each test states the input, what Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, UnitExt, platform::DrawOp};

fn alphas(ui: &mut ComposeTestRule) -> Vec<f32> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
            _ => None,
        })
        .collect()
}

/// `AnimatedContent(count)` with `fadeIn(tween(1000, easing = LinearEasing)) togetherWith
/// ExitTransition.None` (no size transform), the state changed from 0 to 1, 500 ms in: Compose
/// drives each content's `AnimatedEnterExit` from a child transition of the content's
/// transition, whose current state is the old one, so the new content enters: it's drawn at
/// about half alpha. Here the new state's content is a new `AnimatedVisibility` composed
/// visible from the start (its progress starts shown), so it never runs its enter: it's drawn
/// at full alpha at once; no enter transition of `AnimatedContent` ever plays (the default
/// fade and scale in included).
#[test]
fn animated_content_new_content_runs_its_enter_transition() {
    let count = mutable_state_of(0);
    let handle = count.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        AnimatedContent!(
            handle.get(),
            transition_spec = |_, _| fade_in_with(tween(1000).easing(LINEAR_EASING), 0.0)
                .together_with(ExitTransition::NONE)
                .using(None),
            |value: i32| {
                Box!(
                    modifier = Modifier
                        .size((50.dp(), 20.dp()))
                        .background(Color::BLACK)
                        .test_tag(format!("{value}"))
                );
            }
        );
    });
    count.set(1);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(500));
    ui.on_node_with_tag("1").assert_exists();
    let alphas = alphas(&mut ui);
    assert!(
        alphas.iter().any(|alpha| *alpha > 0.2 && *alpha < 0.8),
        "the new content should be fading in, half way; layer alphas: {alphas:?}"
    );
}

/// `AnimatedContent(screen, Modifier.size(200.dp, 100.dp))` whose content is a `Box` with a
/// background and no size of its own: Compose measures each content (through
/// `AnimatedEnterExit` and the size modifier) with the container's constraints as given, so the
/// box gets a minimum of 200 x 100 and fills the container (`AnimatedContent`'s measure policy
/// calls `measurable.measure(constraints)`). Here `AnimatedContent`'s stack loosens the constraints,
/// so the box collapses to 0 x 0 (a screen-sized `Column` with centered arrangement would
/// likewise sit at the top, wrapping its content).
#[test]
fn animated_content_measures_contents_with_its_constraints() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {
        AnimatedContent!(0, modifier = Modifier.size((200.dp(), 100.dp())), |_value: i32| {
            Box!(modifier = Modifier.test_tag("screen"));
        });
    });
    let bounds = ui.on_node_with_tag("screen").fetch_semantics_node().bounds;
    assert_eq!((bounds.width(), bounds.height()), (200.0, 100.0));
}

/// `AnimatedContent(state)` going 0 -> 1, then back to 0 30 ms later (while 0 is still fading
/// out): in Compose 0's content was never removed from `currentlyVisible`, so it stays in the
/// composition and its remembered state survives (it is created once). Lock-in (passes if the
/// keyed move keeps its group).
#[test]
fn animated_content_returning_state_keeps_its_content() {
    let count = mutable_state_of(0);
    let handle = count.handle();
    let created = Rc::new(Cell::new(0));
    let seen = created.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        AnimatedContent!(handle.get(), move |value: i32| {
            if value == 0 {
                remember(|| seen.set(seen.get() + 1));
            }
            Box!(modifier = Modifier.size((50.dp(), 20.dp())).test_tag(format!("{value}")));
        });
    });
    assert_eq!(created.get(), 1);
    count.set(1);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(30));
    count.set(0);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(1000));
    ui.on_node_with_tag("0").assert_exists();
    ui.on_node_with_tag("1").assert_does_not_exist();
    assert_eq!(created.get(), 1, "0's content was recreated");
}

/// A composition reading only `transition.isRunning` (its animated value read only while
/// drawing), the target changed: in Compose `isRunning` is snapshot state
/// (`startTimeNanos`, set on the animation's first frame), so the reader recomposes and sees
/// `true` while the animation runs, then `false`. Here it is a plain counter cell set in a
/// side effect: nothing invalidates the reader, which keeps the `false` it read in the pass
/// that changed the target, all through the animation.
#[test]
fn transition_is_running_is_observable() {
    let expanded = mutable_state_of(false);
    let handle = expanded.handle();
    let running_seen = Rc::new(Cell::new(false));
    let report = running_seen.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "Expand");
        let alpha =
            transition.animate_float(|_| tween(1000).into(), |&open| if open { 1.0 } else { 0.0 });
        report.set(transition.is_running());
        // The value is read only while drawing.
        Box!(
            modifier = Modifier
                .size((10.dp(), 10.dp()))
                .graphics_layer(move |layer| layer.alpha = alpha.get())
        );
    });
    expanded.set(true);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(300));
    assert!(running_seen.get(), "a reader of is_running should see the animation running");
}

/// An `updateTransition` value animating along `tween(1000, LinearEasing)` from 0 to 100,
/// interrupted at 500 ms (value 50) by the target going back: Compose's
/// `TransitionAnimationState.updateAnimation(isInterrupted = true)` swaps a non-spring spec for
/// its `interruptionSpec`, a default spring (`StiffnessMedium`, no bounce), so the value is
/// back at 0 within ~400 ms. Here the interruption runs the full tween again from 50: at 400
/// ms it's still at about 30.
#[test]
fn transition_interrupted_tween_continues_on_a_spring() {
    let open = mutable_state_of(false);
    let handle = open.handle();
    let value = Rc::new(Cell::new(0.0f32));
    let report = value.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "Open");
        let state = transition.animate_float(
            |_| tween(1000).easing(LINEAR_EASING).into(),
            |&open| if open { 100.0 } else { 0.0 },
        );
        report.set(state.get());
    });
    open.set(true);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(500));
    let midway = value.get();
    assert!(midway > 35.0 && midway < 65.0, "{midway}");
    open.set(false);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(400));
    assert!(value.get() < 2.0, "still at {} 400 ms after the interruption", value.get());
}
