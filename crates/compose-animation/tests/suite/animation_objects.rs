//! Animations as values, against Compose's `Animation`, `TargetBasedAnimation`, `DecayAnimation`
//! and the vectorized specs: values, velocities, durations, finiteness, and an `Animatable`
//! playing them frame by frame.

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Offset};

fn v(components: &[f32]) -> AnimationVector {
    match components {
        [a] => AnimationVector::from([*a]),
        [a, b] => AnimationVector::from([*a, *b]),
        [a, b, c] => AnimationVector::from([*a, *b, *c]),
        [a, b, c, d] => AnimationVector::from([*a, *b, *c, *d]),
        _ => panic!("1 to 4 components"),
    }
}

fn lerp(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationTest.kt, testSnap
#[test]
fn a_snap_holds_the_start_until_its_delay() {
    for (delay, start, end) in [
        (200, v(&[0.0]), v(&[1.0])),
        (2, v(&[0.0, 50.0]), v(&[1.0, -50.0])),
        (0, v(&[0.0, 20.0, -100.0]), v(&[-40.0, 0.0, 200.0])),
        (5, v(&[48.0, 26.0, 88.0, 177.0]), v(&[64.0, 286.0, -999.0, 40.0])),
    ] {
        let snap = snap(delay);
        let at = |time: i64| snap.value_from_millis(time, start, end, start);
        if delay > 0 {
            assert_eq!(at(0), start);
        }
        assert_eq!(at(i64::from(delay) - 1), start);
        assert_eq!(at(i64::from(delay)), end);
        assert_eq!(at(i64::from(delay) + 100), end);
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationTest.kt, testKeyframes
#[test]
fn keyframes_hold_then_ease_to_the_target_after_their_delay() {
    let start = v(&[100.0, 200.0, 300.0]);
    let end = v(&[200.0, 100.0, 0.0]);
    let spec = keyframes::<AnimationVector>(|frames| {
        frames.duration_millis = 200;
        frames.delay_millis = 120;
        frames.at(0, start).using(LINEAR_EASING);
        frames.at(100, start).using(FAST_OUT_LINEAR_IN_EASING);
    });
    let at = |time: i64| spec.value_from_millis(time, start, end, start);
    assert_eq!(at(0), start);
    assert_eq!(at(120), start);
    for time in (0..=200).step_by(50) {
        let fraction = if time <= 100 {
            0.0
        } else {
            FAST_OUT_LINEAR_IN_EASING.transform((time - 100) as f32 / 100.0)
        };
        let expected = start.map(|index, from| lerp(from, end[index], fraction));
        assert_eq!(at(120 + time), expected, "at {time}");
    }
    assert_eq!(at(500), end);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationTest.kt, testTween
#[test]
fn tweens_ease_each_component_after_their_delay() {
    let cases = [
        (tween(DEFAULT_DURATION_MILLIS), v(&[0.0]), v(&[1.0])),
        (tween(200).easing(LINEAR_EASING), v(&[0.0, 50.0]), v(&[1.0, -50.0])),
        (
            tween(DEFAULT_DURATION_MILLIS).delay(10).easing(FAST_OUT_LINEAR_IN_EASING),
            v(&[0.0, 20.0, -100.0]),
            v(&[-40.0, 0.0, 200.0]),
        ),
        (
            tween(DEFAULT_DURATION_MILLIS),
            v(&[48.0, 26.0, 88.0, 177.0]),
            v(&[64.0, 286.0, -999.0, 40.0]),
        ),
    ];
    for (tween, start, end) in cases {
        let at = |time: i64| tween.value_from_millis(time, start, end, start);
        let delay = i64::from(tween.delay_millis);
        assert_eq!(at(0), start);
        assert_eq!(at(delay - 1), start);
        assert_eq!(at(delay), start);
        let fraction = tween.easing.transform(100.0 / tween.duration_millis as f32);
        assert_eq!(at(delay + 100), start.map(|index, from| lerp(from, end[index], fraction)));
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationTest.kt, testSpringAnimation
#[test]
fn a_vector_spring_is_a_float_spring_per_component() {
    let spec = spring_with(Spring::DAMPING_RATIO_HIGH_BOUNCY, Spring::STIFFNESS_MEDIUM);
    let float =
        FloatSpringSpec::new(Spring::DAMPING_RATIO_HIGH_BOUNCY, Spring::STIFFNESS_MEDIUM, 0.01);
    let (start, end, velocity) =
        (v(&[75.0, 1000.0, -256.0]), v(&[1296.0, -357.0, 500.0]), v(&[0.0, 75.0, 5.0]));
    let duration = (0..3)
        .map(|index| float.duration_millis(start[index], end[index], velocity[index]))
        .max()
        .unwrap();
    assert_eq!(spec.duration_millis(start, end, velocity), duration);
    for time in (0..=duration).step_by(100) {
        let expected = start
            .map(|index, from| float.value_from_millis(time, from, end[index], velocity[index]));
        assert_eq!(spec.value_from_millis(time, start, end, velocity), expected, "at {time}");
    }
}

/// A target-based animation of `spec` gives the spec's values, velocities and duration.
fn verify(
    spec: AnimationSpec,
    start: AnimationVector,
    end: AnimationVector,
    velocity: AnimationVector,
) {
    let animation = TargetBasedAnimation::with_velocity_vector(spec, start, end, velocity);
    for time in (0..=animation.duration_millis()).step_by(100) {
        assert_eq!(
            spec.value_from_millis(time, start, end, velocity),
            animation.value_from_millis(time)
        );
        assert_eq!(
            spec.velocity_from_nanos(time * 1_000_000, start, end, velocity),
            animation.velocity_vector_from_millis(time),
            "{spec:?} at {time}"
        );
    }
    assert_eq!(spec.duration_millis(start, end, velocity), animation.duration_millis());
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationTest.kt, testAnimation
#[test]
fn a_target_based_animation_plays_its_spec() {
    let (start, end, velocity) = (
        v(&[78.0, -64.0, 1020.0, 0.0]),
        v(&[438.0, 554.0, -298.0, 18.0]),
        v(&[0.0, 74.0, -20.0, 0.0]),
    );
    verify(
        spring_with(Spring::DAMPING_RATIO_LOW_BOUNCY, Spring::STIFFNESS_LOW)
            .visibility_threshold(v(&[5.0, 4.0, 0.1, 20.0]))
            .into(),
        start,
        end,
        velocity,
    );
    verify(tween(1000).easing(FAST_OUT_LINEAR_IN_EASING).into(), start, end, velocity);
    verify(snap(200).into(), start, end, velocity);
    let keyframes = keyframes::<AnimationVector>(|frames| {
        frames.duration_millis = 800;
        frames.at(200, start);
    });
    verify(keyframes.into(), start, end, velocity);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/IsInfiniteTest.kt, testTweenIsFinite
// (with the snap, keyframes, spring and repeatable cases of the same file)
#[test]
fn only_infinite_repeatables_are_infinite() {
    let keyframes = keyframes::<f32>(|frames| frames.duration_millis = 100);
    for (spec, infinite) in [
        (AnimationSpec::from(tween(DEFAULT_DURATION_MILLIS)), false),
        (snap(0).into(), false),
        (keyframes.into(), false),
        (spring().into(), false),
        (repeatable(10, tween(DEFAULT_DURATION_MILLIS), RepeatMode::Restart).into(), false),
        (infinite_repeatable(tween(DEFAULT_DURATION_MILLIS), RepeatMode::Restart).into(), true),
    ] {
        assert_eq!(spec.is_infinite(), infinite, "{spec:?}");
        assert_eq!(
            TargetBasedAnimation::new(spec, 0.0f32, 0.0).is_infinite(),
            infinite,
            "{spec:?}"
        );
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/IsInfiniteTest.kt, testExponentialDecayAnimationIsFinite
// (with testDecayAnimationIsFinite)
#[test]
fn decays_are_finite() {
    assert!(!DecayAnimation::new(exponential_decay(1.0, 0.1), 0.0f32, 0.0).is_infinite());
    assert!(!float_decay_animation(FloatExponentialDecaySpec::default(), 0.0, 0.0).is_infinite());
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DecayAnimationTest.kt, testExponentialDecay
#[test]
fn an_exponential_decay_slows_until_under_its_threshold() {
    let spec = FloatExponentialDecaySpec::new(1.0, 2.0);
    let (start, velocity) = (200.0f32, -800.0f32);
    let animation = float_decay_animation(spec, start, velocity);
    let finish = animation.value_from_nanos(i64::from(i32::MAX));
    let finish_nanos = animation.duration_millis() * 1_000_000;
    for millis in (0..=4000i64).step_by(200) {
        let nanos = millis * 1_000_000;
        let value = spec.value_from_nanos(nanos, start, velocity);
        let speed = spec.velocity_from_nanos(nanos, start, velocity);
        let finished = nanos >= finish_nanos;
        assert_eq!(finished, animation.is_finished_from_nanos(nanos));
        if !finished {
            assert!(speed.abs() >= 2.0);
            assert!((value - animation.value_from_nanos(nanos)).abs() < 0.001);
            assert!((speed - animation.velocity_vector_from_nanos(nanos)[0]).abs() < 0.001);
        } else {
            assert!(speed.abs() < 2.0);
            assert!((finish - animation.value_from_nanos(nanos)).abs() < 0.001);
        }
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DecayAnimationTest.kt, testDecayThreshold
#[test]
fn a_decay_stops_at_its_threshold_velocity() {
    let threshold = 500.0;
    let (start, velocity) = (2000.0f32, 800.0f32);
    let far = i64::from(i32::MAX);
    let full = float_decay_animation(FloatExponentialDecaySpec::new(1.0, 0.0), start, velocity);
    let stopped =
        float_decay_animation(FloatExponentialDecaySpec::new(1.0, threshold), start, velocity);
    let (stop_value, stop_velocity) =
        (stopped.value_from_nanos(far), stopped.velocity_vector_from_nanos(far)[0]);
    assert!((stop_velocity - threshold).abs() < 0.001, "{stop_velocity}");
    // Going on from there ends where a decay without a threshold does.
    let rest =
        float_decay_animation(FloatExponentialDecaySpec::new(1.0, 0.0), stop_value, stop_velocity);
    assert!((full.value_from_nanos(far) - rest.value_from_nanos(far)).abs() < 2.0);
}

/// Runs `animatable` along `run`, sampling its value and velocity every frame (16 ms): the
/// first sample at play time 0.
fn samples<T: Animate>(
    animatable: &Animatable<T>,
    frames: usize,
    run: impl std::future::Future<Output = AnimationResult<T>> + 'static,
) -> (Vec<(T, T)>, std::rc::Rc<std::cell::RefCell<Option<AnimationResult<T>>>>) {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let kept = result.clone();
    scope.launch(async move {
        *kept.borrow_mut() = Some(run.await);
    });
    let mut values = Vec::new();
    for _ in 0..frames {
        ui.advance_time_by_frame();
        values.push((animatable.value(), animatable.velocity()));
    }
    drop(scope);
    (values, result)
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, animateDecayTest
#[test]
fn an_animatable_decays_frame_by_frame() {
    let expected = float_decay_animation(FloatExponentialDecaySpec::default(), 9.0, 20.0);
    let animatable = Animatable::new(9.0f32);
    let running = animatable.clone();
    let spec = exponential_decay(1.0, 0.1);
    let (values, result) =
        samples(&animatable, 200, async move { running.animate_decay(20.0, &spec).await });
    for (frame, (value, velocity)) in values.iter().enumerate() {
        let play = frame as i64 * 16;
        if play >= expected.duration_millis() {
            break;
        }
        assert!((expected.value_from_millis(play) - value).abs() < 0.001, "at {play}");
        assert!((expected.velocity_from_millis(play) - velocity).abs() < 0.001, "at {play}");
    }
    assert_eq!(animatable.value(), expected.target_value());
    assert!(!animatable.is_running());
    assert_eq!(animatable.velocity(), 0.0);
    assert_eq!(result.borrow().as_ref().unwrap().end_reason, AnimationEndReason::Finished);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, animateToTest
#[test]
fn an_animatable_springs_frame_by_frame() {
    let spec = spring_with(Spring::DAMPING_RATIO_MEDIUM_BOUNCY, Spring::STIFFNESS_MEDIUM);
    let expected = TargetBasedAnimation::new(spec, 0.0f32, 1.0);
    let animatable = Animatable::new(0.0f32);
    let running = animatable.clone();
    let (values, result) =
        samples(&animatable, 100, async move { running.animate_to(1.0, spec).await });
    for (frame, (value, velocity)) in values.iter().enumerate() {
        let play = frame as i64 * 16;
        if play >= expected.duration_millis() {
            break;
        }
        assert!((expected.value_from_millis(play) - value).abs() < 0.001, "at {play}");
        assert!((expected.velocity_from_millis(play) - velocity).abs() < 0.001, "at {play}");
    }
    assert_eq!(
        (animatable.value(), animatable.velocity(), animatable.is_running()),
        (1.0, 0.0, false)
    );
    assert_eq!(result.borrow().as_ref().unwrap().end_reason, AnimationEndReason::Finished);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, animateToGenericTypeTest
#[test]
fn an_offset_animatable_tweens_until_stopped() {
    let (from, to) = (Offset::new(666.0, 321.0), Offset::new(919.0, 864.0));
    let expected = TargetBasedAnimation::new(tween(500), from, to);
    let animatable = Animatable::new(from);
    let running = animatable.clone();
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    scope.launch(async move {
        running.animate_to(to, tween(500)).await;
    });
    for frame in 0..19 {
        ui.advance_time_by_frame();
        let play = frame * 16;
        let value = animatable.value();
        let wanted = expected.value_from_millis(play);
        assert!(
            (value.x - wanted.x).abs() < 0.001 && (value.y - wanted.y).abs() < 0.001,
            "at {play}"
        );
        assert!(animatable.is_running());
    }
    // Stopped mid-way: not running, at rest where it was, the target kept.
    let at = animatable.value();
    animatable.stop();
    assert!(!animatable.is_running());
    assert_eq!(animatable.target_value(), to);
    assert_eq!(animatable.velocity_vector(), AnimationVector::from([0.0, 0.0]));
    ui.advance_time_by_frame();
    assert_eq!(animatable.value(), at);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, animateToWithInterruption
#[test]
fn an_interrupting_spring_starts_at_the_interrupted_frame() {
    let first = TargetBasedAnimation::new(tween(200).easing(LINEAR_EASING), 0.0f32, 200.0);
    let animatable = Animatable::new(0.0f32);
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let running = animatable.clone();
    scope.launch(async move {
        running.animate_to(200.0, tween(200).easing(LINEAR_EASING)).await;
    });
    // Frames at 0, 16, … 96 ms of the tween.
    for frame in 0..7 {
        ui.advance_time_by_frame();
        assert_eq!(animatable.value(), first.value_from_millis(frame * 16));
    }
    let (value, velocity) = (animatable.value(), animatable.velocity());
    let second = TargetBasedAnimation::with_velocity(spring(), value, 300.0, velocity);
    let interrupting = animatable.clone();
    scope.launch(async move {
        interrupting.animate_to(300.0, spring()).await;
    });
    // The spring's play time 0 is the tween's last frame: the next frame is 16 ms into it.
    for frame in 1..100 {
        ui.advance_time_by_frame();
        let play = frame * 16;
        if play >= second.duration_millis() {
            break;
        }
        assert_eq!(animatable.value(), second.value_from_millis(play), "at {play}");
        assert_eq!(animatable.velocity(), second.velocity_from_millis(play), "at {play}");
    }
    ui.advance_time_by(std::time::Duration::from_secs(1));
    assert_eq!(
        (animatable.value(), animatable.target_value(), animatable.velocity()),
        (300.0, 300.0, 0.0)
    );
    assert!(!animatable.is_running());
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, testUpdateBounds
#[test]
fn an_animatable_stays_within_its_bounds() {
    let animatable = Animatable::new(5.0f32);
    // At rest: clamped at once.
    animatable.update_bounds(Some(0.0), Some(4.0));
    assert_eq!(animatable.value(), 4.0);
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let (running, result) = (animatable.clone(), std::rc::Rc::new(std::cell::Cell::new(None)));
    let reason = result.clone();
    scope.launch(async move {
        reason.set(Some(running.animate_to(4.0, tween(100)).await.end_reason));
    });
    ui.advance_time_by_frame();
    // Running: the next frame is clamped, and the animation ends there.
    animatable.update_bounds(Some(-4.0), Some(0.0));
    ui.advance_time_by_frame();
    assert_eq!(animatable.value(), 0.0);
    assert_eq!(result.get(), Some(AnimationEndReason::BoundReached));
    // A snap out of the bounds is clamped.
    animatable.snap_to(animatable.lower_bound().unwrap() - 100.0);
    assert_eq!(Some(animatable.value()), animatable.lower_bound());
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, testIntSize_alwaysWithinValidBounds
#[test]
fn an_int_size_animatable_never_goes_negative() {
    let animatable =
        Animatable::new_with(IntSize::new(10, 10), Some(IntSize::VISIBILITY_THRESHOLD), "Size");
    let running = animatable.clone();
    // Linearly from (-100, -100) to (100, 100) over 100 ms.
    let spec = keyframes::<IntSize>(|frames| {
        frames.duration_millis = 100;
        frames.at(0, IntSize::new(-100, -100)).using(LINEAR_EASING);
    });
    let (values, _) =
        samples(
            &animatable,
            8,
            async move { running.animate_to(IntSize::new(100, 100), spec).await },
        );
    let sizes: Vec<IntSize> = values.into_iter().map(|(value, _)| value).collect();
    // At 0, 16, 32 and 48 ms the size would be negative: zero instead.
    let expected: Vec<IntSize> = [-100.0f32, -68.0, -36.0, -4.0, 28.0, 60.0, 92.0, 100.0]
        .iter()
        .map(|&length| {
            let length = ((length + 0.5).floor() as i32).max(0);
            IntSize::new(length, length)
        })
        .collect();
    assert_eq!(sizes, expected);
}
