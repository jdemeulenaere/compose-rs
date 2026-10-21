//! Animations through the test rule's main clock: values per frame, springs, interruptions,
//! colors and infinite transitions.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::{Cell, RefCell};
use std::time::Duration;

use compose_animation::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::Color;

thread_local! {
    static SEEN: Cell<f32> = const { Cell::new(f32::NAN) };
    static FINISHED: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

fn seen() -> f32 {
    SEEN.with(Cell::get)
}

#[composable]
fn Animated(target: MutableState<f32>, spec: AnimationSpec) {
    let value = animate_float_as_state!(
        target.get(),
        animation_spec = spec,
        finished_listener = |value| FINISHED.with(|finished| finished.borrow_mut().push(value)),
    );
    SEEN.with(|seen| seen.set(value.get()));
    BasicText!("{:.1}", value.get());
}

fn animated(spec: AnimationSpec) -> (ComposeTestRule, OwnedState<f32>) {
    FINISHED.with(|finished| finished.borrow_mut().clear());
    let target = mutable_state_of(0.0f32);
    let handle = target.handle();
    let ui = ComposeTestRule::with_manual_clock(move || Animated(handle, spec));
    (ui, target)
}

#[test]
fn a_tween_moves_with_the_clock_and_ends_at_its_target() {
    let (mut ui, target) = animated(tween(100).easing(LINEAR_EASING).into());
    assert_eq!(seen(), 0.0);
    target.set(100.0);
    ui.wait_for_idle();
    // The first frame starts the animation; the next ones move it, 16 ms apart.
    ui.advance_time_by(Duration::from_millis(16));
    let early = seen();
    ui.advance_time_by(Duration::from_millis(48));
    let later = seen();
    assert!(early < later && later < 100.0, "{early} then {later}");
    assert!((later - 48.0).abs() < 1.0, "linear: 48 ms of 100 ms is 48, not {later}");
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(seen(), 100.0);
    assert_eq!(FINISHED.with(|finished| finished.borrow().clone()), [100.0]);
}

#[test]
fn waiting_for_idle_runs_an_animation_to_its_end() {
    let (mut ui, target) = animated(spring().into());
    ui.set_auto_advance(true);
    target.set(1.0);
    ui.wait_for_idle();
    assert_eq!(seen(), 1.0);
    ui.on_node_with_text("1.0").assert_exists();
}

#[test]
fn a_spring_settles_and_a_new_target_continues_from_where_it_is() {
    let (mut ui, target) =
        animated(spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_LOW).into());
    target.set(100.0);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(96));
    let midway = seen();
    assert!(midway > 0.0 && midway < 100.0, "{midway}");
    // Back to 0: from where it is, without a jump.
    target.set(0.0);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(16));
    let after = seen();
    assert!((after - midway).abs() < 15.0, "jumped from {midway} to {after}");
    ui.advance_time_by(Duration::from_secs(3));
    assert_eq!(seen(), 0.0);
    // The first animation was interrupted: only the second finished.
    assert_eq!(FINISHED.with(|finished| finished.borrow().clone()), [0.0]);
}

#[test]
fn a_bouncy_spring_overshoots() {
    let (mut ui, target) =
        animated(spring_with(Spring::DAMPING_RATIO_HIGH_BOUNCY, Spring::STIFFNESS_MEDIUM).into());
    target.set(100.0);
    ui.wait_for_idle();
    let mut highest = 0.0f32;
    for _ in 0..60 {
        ui.advance_time_by_frame();
        highest = highest.max(seen());
    }
    assert!(highest > 110.0, "a bouncy spring goes past its target: {highest}");
    ui.advance_time_by(Duration::from_secs(5));
    assert_eq!(seen(), 100.0);
}

#[test]
fn snap_jumps_after_its_delay() {
    let (mut ui, target) = animated(snap(50));
    target.set(7.0);
    ui.wait_for_idle();
    // The animation starts at the first frame after the change (16 ms), and snaps 50 ms later.
    ui.advance_time_by(Duration::from_millis(48));
    assert_eq!(seen(), 0.0);
    ui.advance_time_by(Duration::from_millis(32));
    assert_eq!(seen(), 7.0);
}

#[test]
fn easings_follow_their_curves() {
    assert_eq!(LINEAR_EASING.transform(0.3), 0.3);
    let half = FAST_OUT_SLOW_IN_EASING.transform(0.5);
    assert!((half - 0.774).abs() < 0.01, "FastOutSlowIn(0.5) = {half}");
    assert_eq!(FAST_OUT_SLOW_IN_EASING.transform(0.0), 0.0);
    assert_eq!(FAST_OUT_SLOW_IN_EASING.transform(1.0), 1.0);
}

#[test]
fn colors_go_through_oklab_and_back() {
    for color in
        [Color::BLACK, Color::WHITE, Color::RED, Color::from_argb(0x80336699), Color::TRANSPARENT]
    {
        assert_eq!(Color::from_vector(color.to_vector()), color);
    }
    // Midway from red to green in Oklab isn't sRGB's muddy (128, 128, 0).
    let start = Color::RED.to_vector();
    let end = Color::GREEN.to_vector();
    let middle: Vec<f32> = start.iter().zip(end).map(|(a, b)| (a + b) / 2.0).collect();
    let middle = Color::from_vector([middle[0], middle[1], middle[2], middle[3]]);
    assert!(middle.red() > 150 && middle.green() > 150, "{middle:?}");
}

#[test]
fn an_animatable_interrupted_by_another_animation_says_so() {
    let animatable = Animatable::new(0.0f32);
    let results = std::rc::Rc::new(RefCell::new(Vec::new()));
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let (first, second) = (animatable.clone(), animatable.clone());
    let (log_first, log_second) = (results.clone(), results.clone());
    scope.launch(async move {
        let result = first.animate_to(10.0, tween(1000)).await;
        log_first.borrow_mut().push(result.end_reason);
    });
    ui.advance_time_by(Duration::from_millis(100));
    assert!(animatable.is_running());
    scope.launch(async move {
        let result = second.animate_to(20.0, tween(100)).await;
        log_second.borrow_mut().push(result.end_reason);
    });
    ui.advance_time_by(Duration::from_millis(300));
    assert_eq!(*results.borrow(), [AnimationEndReason::Interrupted, AnimationEndReason::Finished]);
    assert_eq!(animatable.value(), 20.0);
    assert!(!animatable.is_running());
}

#[test]
fn an_infinite_transition_goes_back_and_forth() {
    #[composable]
    fn Pulse() {
        let transition = remember_infinite_transition!();
        let alpha = transition.animate_float(
            0.0,
            1.0,
            infinite_repeatable(tween(100).easing(LINEAR_EASING), RepeatMode::Reverse),
        );
        SEEN.with(|seen| seen.set(alpha.get()));
    }
    let mut ui = ComposeTestRule::with_manual_clock(Pulse);
    let mut values = Vec::new();
    for _ in 0..30 {
        ui.advance_time_by_frame();
        values.push(seen());
    }
    let highest = values.iter().copied().fold(0.0, f32::max);
    let lowest_after_peak =
        values.iter().skip_while(|value| **value < highest).copied().fold(1.0, f32::min);
    assert!(highest > 0.9 && lowest_after_peak < 0.1, "{values:?}");
}
