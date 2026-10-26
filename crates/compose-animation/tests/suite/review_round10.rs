//! Tenth review round: `update_transition` interrupted mid-way, against Jetpack Compose's
//! `Transition`, and `AnimatedContent`'s exits. Each test states the input, what Compose does,
//! and what happens instead.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, UnitExt, platform::DrawOp};

/// An `updateTransition(target)` with one `animateFloat` along `tween(1000)`, its target
/// changed from 0 to 1 and, 500 ms in, back to 0. Compose's `Transition.updateTarget` sees the
/// change interrupt a running transition (`currentState != targetState`) and moves
/// `currentState` on to the state it was heading for (`transitionState.initialState =
/// this.targetState`): while the value goes back, `currentState` is 1 and `targetState` 0, so
/// `currentState == targetState` (how callers tell a transition has settled, as
/// `AnimatedVisibility`'s `isIdle`) is false until it's back. Here `current_state` stays 0, the
/// state before the interruption: it equals the target at once while the value is still
/// mid-way.
#[test]
fn an_interrupted_transition_is_at_the_state_it_was_heading_for() {
    let target = mutable_state_of(0);
    let handle = target.handle();
    let seen = Rc::new(RefCell::new((0, 0, false)));
    let report = seen.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "Interrupted");
        let _value = transition.animate_float(
            |_, _| tween(1000).easing(LINEAR_EASING).into(),
            |&state| state as f32 * 100.0,
        );
        *report.borrow_mut() =
            (transition.current_state(), transition.target_state(), transition.is_running());
    });
    target.set(1);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(500));
    target.set(0);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    let (current, target_state, running) = *seen.borrow();
    assert!(running, "the value is still going back");
    assert_eq!(target_state, 0);
    assert_eq!(current, 1, "current_state while going back from the interrupted 0 -> 1");
}

/// The same transition, 0 to 1 interrupted at 500 ms by 2, its `transitionSpec` logging the
/// segment it's asked for. Compose asks for the spec of `Segment(1, 2)`: the interrupted
/// target is the new segment's initial state (`segment = SegmentImpl(this.targetState,
/// targetState)`), so `1 isTransitioningTo 2` picks its spec. Here the spec is asked for
/// `(0, 2)`, a change the transition never had.
#[test]
fn an_interrupting_change_asks_for_the_spec_from_the_interrupted_target() {
    let target = mutable_state_of(0);
    let handle = target.handle();
    let segments: Rc<RefCell<Vec<(i32, i32)>>> = Rc::default();
    let log = segments.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "Segments");
        let log = log.clone();
        let _value = transition.animate_float(
            move |&initial, &target| {
                log.borrow_mut().push((initial, target));
                tween(1000).easing(LINEAR_EASING).into()
            },
            |&state| state as f32 * 100.0,
        );
    });
    target.set(1);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(500));
    target.set(2);
    ui.wait_for_idle();
    let segments = segments.borrow().clone();
    assert_eq!(segments.last(), Some(&(1, 2)), "segments asked for: {segments:?}");
}

// ---- AnimatedContent -----------------------------------------------------------------------

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

/// `AnimatedContent(count)` whose spec for 0 -> 1 fades the old content out
/// (`fadeOut(tween(1000, LinearEasing))`) and for 1 -> 2 has no exit (`ExitTransition.None`),
/// no enters, no size transform; the count goes 0 -> 1, and 500 ms in (content 0 at half alpha)
/// on to 2. Compose computes each content's exit once, when it stops being the target
/// (`remember(segment.targetState == stateForContent) { transitionSpec().initialContentExit }`),
/// so content 0 goes on fading out from half alpha. Here every content takes the newest
/// transform: content 0, mid-way through its fade, is given 1 -> 2's exit (none), so its fade
/// is dropped and it pops back to full alpha until its old animation ends.
#[test]
fn a_content_keeps_its_exit_when_the_state_changes_again() {
    let count = mutable_state_of(0);
    let handle = count.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        AnimatedContent!(
            handle.get(),
            transition_spec = |_, target| {
                let exit = if target == 1 {
                    fade_out_with(tween(1000).easing(LINEAR_EASING), 0.0)
                } else {
                    ExitTransition::NONE
                };
                EnterTransition::NONE.together_with(exit).without_size_transform()
            },
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
    let before = alphas(&mut ui);
    assert!(before.iter().any(|alpha| *alpha > 0.3 && *alpha < 0.7), "{before:?}");
    count.set(2);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.on_node_with_tag("0").assert_exists();
    let after = alphas(&mut ui);
    assert!(
        after.iter().any(|alpha| *alpha > 0.3 && *alpha < 0.7),
        "content 0 should still be fading out (about half alpha); the layers' alphas: {after:?}"
    );
}
