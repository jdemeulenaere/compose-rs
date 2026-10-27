//! Animations through the test rule's main clock: values per frame, springs, interruptions,
//! colors and infinite transitions.

#![allow(non_snake_case)]

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
    let ui = ComposeTestRule::with_manual_clock(move || Animated(handle, spec.clone()));
    (ui, target)
}

#[test]
fn a_tween_moves_with_the_clock_and_ends_at_its_target() {
    let (mut ui, target) = animated(tween(100).easing(LINEAR_EASING).into());
    assert_eq!(seen(), 0.0);
    target.set(100.0);
    ui.wait_for_idle();
    // The frame that recomposes for the new target: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    // The next frame starts the animation; the next ones move it, 16 ms apart.
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
    let (mut ui, target) = animated(snap(50).into());
    target.set(7.0);
    ui.wait_for_idle();
    // The frame that recomposes for the new target: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    // The animation starts at the next frame (16 ms), and snaps 50 ms later.
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
    let middle = Color::from_vector(start.map(|index, a| (a + end[index]) / 2.0));
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

#[test]
fn keyframes_pass_through_their_values_at_their_times() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 160;
        frames.at(80, 50.0f32);
    });
    let (mut ui, target) = animated(spec.into());
    target.set(100.0);
    ui.wait_for_idle();
    // The frame that recomposes for the new target: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(16));
    // 48 ms in: on the way from 0 to 50 (at 80 ms), linearly.
    ui.advance_time_by(Duration::from_millis(48));
    assert!((seen() - 30.0).abs() < 1.0, "{}", seen());
    // 112 ms in: from 50 to the target, 100 at 160 ms.
    ui.advance_time_by(Duration::from_millis(64));
    assert!((seen() - 70.0).abs() < 1.0, "{}", seen());
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(seen(), 100.0);
}

#[test]
fn a_keyframe_eases_the_stretch_it_starts() {
    let eased = keyframes(|frames| {
        frames.duration_millis = 100;
        frames.at(0, 0.0f32).using(FAST_OUT_SLOW_IN_EASING);
    });
    let (mut ui, target) = animated(eased.into());
    target.set(100.0);
    ui.wait_for_idle();
    // The frame that recomposes for the new target: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(16));
    ui.advance_time_by(Duration::from_millis(48));
    // Fast out, slow in: past the linear 48 by then.
    assert!(seen() > 55.0, "{}", seen());
}

#[test]
fn equal_keyframes_are_one_spec() {
    let build = || {
        keyframes(|frames| {
            frames.duration_millis = 300;
            frames.delay_millis = 20;
            frames.at(100, 0.5f32).using(LINEAR_OUT_SLOW_IN_EASING);
        })
    };
    assert_eq!(build(), build());
    assert_ne!(
        build(),
        keyframes(|frames| {
            frames.duration_millis = 300;
            frames.at(100, 0.6f32);
        })
    );
    // Repeated, as `infiniteRepeatable(keyframes { … })`.
    let repeated = infinite_repeatable(build(), RepeatMode::Reverse);
    assert_eq!(repeated.animation, DurationBasedSpec::Keyframes(build()));
}

// Review L1: Compose's `AnimationResult.endState` keeps the velocity the animation ended at (a
// fling stopped by a bound hands it on), though the `Animatable`'s own velocity is zeroed.
#[test]
fn an_animation_result_keeps_the_velocity_it_ended_at() {
    let results = std::rc::Rc::new(RefCell::new(Vec::new()));
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let (free, bounded) = (Animatable::new(0.0f32), Animatable::new(0.0f32));
    bounded.update_bounds(None, Some(50.0));
    for animatable in [free.clone(), bounded.clone()] {
        let log = results.clone();
        scope.launch(async move {
            let result = animatable.animate_to(100.0, tween(100).easing(LINEAR_EASING)).await;
            log.borrow_mut().push((result.end_reason, result.end_velocity));
        });
    }
    ui.advance_time_by(Duration::from_millis(300));
    let results = results.borrow();
    // A linear tween of 100 over 100 ms: 1000 a second.
    assert_eq!(results.len(), 2);
    for (reason, velocity) in results.iter() {
        assert!((velocity - 1000.0).abs() < 1.0, "{reason:?}: {velocity}");
    }
    let reasons: Vec<_> = results.iter().map(|(reason, _)| *reason).collect();
    assert!(reasons.contains(&AnimationEndReason::Finished), "{reasons:?}");
    assert!(reasons.contains(&AnimationEndReason::BoundReached), "{reasons:?}");
    assert_eq!((free.velocity(), bounded.velocity()), (0.0, 0.0));
}
