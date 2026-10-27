//! `animate`, `animate_decay` and `AnimationState` against Compose's `SuspendAnimationTest`:
//! the block's values each frame against the animation they play. Compose's test clock gives
//! frames 50 ms apart; the test rule's are 16 ms apart.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::Offset;

const FRAME: i64 = 16;

/// Runs `task` in a rule's clock, a frame at a time, until it ends.
fn run(ui: &mut ComposeTestRule, task: impl std::future::Future<Output = ()> + 'static) {
    let done = Rc::new(Cell::new(false));
    let finished = done.clone();
    let scope = CoroutineScope::new();
    scope.launch(async move {
        task.await;
        finished.set(true);
    });
    let mut frames = 0;
    while !done.get() {
        ui.advance_time_by_frame();
        frames += 1;
        assert!(frames < 1_000, "the animation never ended");
    }
}

fn close(a: f32, b: f32, at: i64) {
    assert!((a - b).abs() <= 0.001, "at {at} ms: {a}, Compose's {b}");
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SuspendAnimationTest.kt, animateFloatVariantTest
#[test]
fn a_float_animates_frame_by_frame() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let spec = spring_with(Spring::DAMPING_RATIO_MEDIUM_BOUNCY, Spring::STIFFNESS_MEDIUM);
    let expected = TargetBasedAnimation::new(spec, 0.0f32, 1.0);
    let frames = Rc::new(Cell::new(0));
    let seen = frames.clone();
    run(&mut ui, async move {
        let mut play = 0;
        animate(0.0, 1.0, 0.0, spec, move |value, velocity| {
            close(value, expected.value_from_millis(play), play);
            close(velocity, expected.velocity_from_millis(play), play);
            play += FRAME;
            seen.set(seen.get() + 1);
        })
        .await;
    });
    assert!(frames.get() > 10);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SuspendAnimationTest.kt, animateGenericsVariantTest
#[test]
fn any_type_animates_frame_by_frame() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let (from, to) = (Offset::new(666.0, 321.0), Offset::new(919.0, 864.0));
    let expected = TargetBasedAnimation::new(tween(500), from, to);
    run(&mut ui, async move {
        let mut play = 0;
        animate_value(from, to, None, tween(500), move |value: Offset, _| {
            let at = expected.value_from_millis(play);
            close(value.x, at.x, play);
            close(value.y, at.y, play);
            play += FRAME;
        })
        .await;
    });
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SuspendAnimationTest.kt, animateDecayTest
#[test]
fn a_float_decays_frame_by_frame() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let (from, velocity) = (666.0f32, 999.0f32);
    let expected = float_decay_animation(FloatExponentialDecaySpec::default(), from, velocity);
    run(&mut ui, async move {
        let mut play = 0;
        animate_decay(from, velocity, FloatExponentialDecaySpec::default(), move |value, speed| {
            close(value, expected.value_from_millis(play), play);
            close(speed, expected.velocity_from_millis(play), play);
            play += FRAME;
        })
        .await;
    });
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SuspendAnimationTest.kt, animateToTest
#[test]
fn an_animation_state_animates_on_from_its_last_frame_until_cancelled() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let (from, to) = (Offset::new(666.0, 321.0), Offset::new(919.0, 864.0));
    let expected = TargetBasedAnimation::new(tween(500), from, to);
    // Its last frame now; the animation starts 96 ms (six frames) later.
    let start = compose_runtime::platform::now_nanos() as i64;
    let state =
        AnimationState::with(from, AnimationVector::from([0.0, 0.0]), start, i64::MIN, false);
    for _ in 0..6 {
        ui.advance_time_by_frame();
    }
    let plays = Rc::new(RefCell::new(Vec::new()));
    let (seen, animated) = (plays.clone(), state.clone());
    run(&mut ui, async move {
        let watched = animated.clone();
        animated
            .animate_to(to, tween(500), true, move |scope| {
                assert!(watched.is_running());
                assert!(scope.is_running());
                let play = (scope.last_frame_time_nanos() - start) / 1_000_000;
                let at = expected.value_from_millis(play);
                close(scope.value().x, at.x, play);
                close(scope.value().y, at.y, play);
                seen.borrow_mut().push(play);
                if play >= 300 {
                    // Cancelled early: the state stops too.
                    scope.cancel_animation();
                    assert!(!watched.is_running());
                    assert!(!scope.is_running());
                }
            })
            .await;
    });
    // The first block call at play time 0 at once (from the last frame), then the frames;
    // none after the cancel.
    let plays = plays.borrow();
    assert_eq!(plays[0], 0);
    assert_eq!(plays[1], 96 + 16);
    assert!(*plays.last().unwrap() >= 300 && plays[plays.len() - 2] < 300, "{plays:?}");
    assert!(!state.is_running());
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SuspendAnimationTest.kt, animateDecayOnAnimationStateTest
#[test]
fn an_animation_state_decays_frame_by_frame() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let expected = float_decay_animation(FloatExponentialDecaySpec::default(), 9.0, 20.0);
    run(&mut ui, async move {
        let state = AnimationState::with_velocity(9.0f32, 20.0);
        let spec = generate_decay_animation_spec(FloatExponentialDecaySpec::default());
        let watched = state.clone();
        let mut play = 0;
        state
            .animate_decay(&spec, false, move |scope| {
                close(scope.value(), expected.value_from_millis(play), play);
                close(scope.velocity(), expected.velocity_from_millis(play), play);
                play += FRAME;
                assert!((scope.value() - watched.value()).abs() <= 0.0001);
                assert!((scope.velocity() - watched.velocity()).abs() <= 0.0001);
            })
            .await;
    });
}

/// An `Animatable`'s block hears each frame's value, as `animateTo(…, block)`, the last one
/// the target.
#[test]
fn an_animatables_block_hears_each_frame() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let expected = TargetBasedAnimation::new(tween(100), 0.0f32, 10.0);
    let heard = Rc::new(RefCell::new(Vec::new()));
    let seen = heard.clone();
    run(&mut ui, async move {
        let animatable = Animatable::new(0.0f32);
        let result = animatable
            .animate_to_with(10.0, tween(100), 0.0, move |animatable| {
                seen.borrow_mut().push(animatable.value())
            })
            .await;
        assert_eq!(result.end_reason, AnimationEndReason::Finished);
    });
    let heard = heard.borrow();
    let wanted: Vec<f32> =
        (0..heard.len() as i64).map(|frame| expected.value_from_millis(frame * FRAME)).collect();
    assert_eq!(*heard, wanted);
    assert_eq!(*heard.last().unwrap(), 10.0);
}
