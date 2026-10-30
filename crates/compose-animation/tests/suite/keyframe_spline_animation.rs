//! `keyframes_with_spline` against Compose's `KeyframeSplineAnimationTest`, evaluated as its
//! vectorized spec.

use compose_animation::*;
use compose_ui::Offset;

const MILLIS_TO_NANOS: i64 = 1_000_000;

fn vector(x: f32, y: f32) -> AnimationVector {
    Offset::new(x, y).to_vector()
}

fn value_at(
    spec: &KeyframesWithSplineSpec,
    millis: i64,
    start: Offset,
    end: Offset,
) -> AnimationVector {
    spec.value_from_millis(millis, start.to_vector(), end.to_vector(), vector(0.0, 0.0))
}

/// From the zero offset to (2, 2), as Compose's helper by default.
fn value(spec: &KeyframesWithSplineSpec, millis: i64) -> AnimationVector {
    value_at(spec, millis, Offset::ZERO, Offset::new(2.0, 2.0))
}

fn velocity_at(
    spec: &KeyframesWithSplineSpec,
    millis: i64,
    start: Offset,
    end: Offset,
    initial_velocity: Offset,
) -> AnimationVector {
    spec.velocity_from_nanos(
        millis * MILLIS_TO_NANOS,
        start.to_vector(),
        end.to_vector(),
        initial_velocity.to_vector(),
    )
}

fn velocity(spec: &KeyframesWithSplineSpec, millis: i64) -> AnimationVector {
    velocity_at(spec, millis, Offset::ZERO, Offset::new(2.0, 2.0), Offset::ZERO)
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, interpolatedValues
#[test]
fn a_spline_passes_through_its_keyframes_following_the_end_values() {
    let spec = keyframes_with_spline(|frames| {
        frames.duration_millis = 500;
        // At half the time, the final Y.
        frames.at(250, Offset::new(1.0, 2.0));
    });
    assert_eq!(value(&spec, 125), vector(0.5, 1.25));
    assert_eq!(velocity(&spec, 125), vector(4.0, 10.0));
    // X is linear, its speed constant; Y doesn't change any more.
    assert_eq!(value(&spec, 375), vector(1.5, 2.0));
    assert_eq!(velocity(&spec, 375), vector(4.0, 0.0));
    // Other start and end values: the value and velocity follow.
    let (start, end) = (Offset::new(2.0, 2.0), Offset::ZERO);
    assert_eq!(value_at(&spec, 375, start, end), vector(0.5, 1.25));
    assert_eq!(velocity_at(&spec, 375, start, end, Offset::ZERO), vector(-4.0, -10.0));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, interpolatedValues_periodic
#[test]
fn a_periodic_bias_gives_both_ends_one_velocity() {
    let frames = |frames: &mut KeyframesWithSplineBuilder<f32>| {
        frames.duration_millis = 1000;
        frames.at(500, 1.0).using(LINEAR_EASING);
    };
    let velocity_at_nanos = |spec: &KeyframesWithSplineSpec, nanos: i64| {
        let zero = AnimationVector::from(0.0);
        spec.velocity_from_nanos(nanos, zero, zero, zero)[0]
    };
    // Not at exactly 0, which would always be the initial velocity.
    let start = |spec: &KeyframesWithSplineSpec| velocity_at_nanos(spec, 1);
    let end = |spec: &KeyframesWithSplineSpec| velocity_at_nanos(spec, 1000 * MILLIS_TO_NANOS);
    let close = |a: f32, b: f32, tolerance: f32| {
        assert!((a - b).abs() <= tolerance, "{a} isn't {b}");
    };
    let regular = keyframes_with_spline(frames);
    let balanced = periodic_keyframes_with_spline(0.5, frames);
    let start_bias = periodic_keyframes_with_spline(0.0, frames);
    let end_bias = periodic_keyframes_with_spline(1.0, frames);
    close(start(&balanced), end(&balanced), 0.0001);
    close(start(&start_bias), end(&start_bias), 0.0001);
    close(start(&end_bias), end(&end_bias), 0.001);
    // Balanced: the average of the regular spline's.
    let average = (start(&regular) + end(&regular)) / 2.0;
    assert_eq!(average, start(&balanced));
    assert_eq!(average, end(&balanced));
    // Biased to the start, the end stays; to the end, the start does.
    close(end(&start_bias), end(&regular), 0.00001);
    close(start(&end_bias), start(&regular), 0.00001);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, testMultipleEasing
#[test]
fn each_stretch_takes_the_easing_of_its_first_keyframe() {
    let spec = keyframes_with_spline(|frames| {
        frames.duration_millis = 300;
        frames.at(0, Offset::new(0.0, 0.0)).using(EASE_IN_CUBIC);
        frames.at(100, Offset::new(1.0, 1.0)).using(EASE_OUT_CUBIC);
        frames.at(200, Offset::new(2.0, 2.0)).using(LINEAR_EASING);
        // Never applied: it's at the end.
        frames.at(300, Offset::new(3.0, 3.0)).using(LINEAR_OUT_SLOW_IN_EASING);
    });
    // The start and end don't matter: they're overwritten.
    let eased_in = value(&spec, 50);
    assert!(eased_in[0] < 0.5 && eased_in[1] < 0.5);
    let eased_out = value(&spec, 150);
    assert!(eased_out[0] > 1.5 && eased_out[1] > 1.5);
    let linear = value(&spec, 250);
    assert!((linear[0] - 2.5).abs() <= 0.001 && (linear[1] - 2.5).abs() <= 0.001);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, possibleToOverrideStartAndEndValues
#[test]
fn keyframes_at_the_ends_override_the_values_asked_for() {
    let spec = keyframes_with_spline(|frames| {
        frames.duration_millis = 500;
        frames.at(0, Offset::new(0.0, 0.0));
        frames.at(250, Offset::new(1.0, 1.0));
        frames.at(500, Offset::new(2.0, 2.0));
    });
    let asked = Offset::new(-1.0, -1.0);
    assert_eq!(value_at(&spec, 0, asked, asked), vector(0.0, 0.0));
    assert_eq!(value_at(&spec, 500, asked, asked), vector(2.0, 2.0));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, initialVelocityIgnored
#[test]
fn the_initial_velocity_is_ignored() {
    let spec = keyframes_with_spline(|frames| {
        frames.duration_millis = 500;
        frames.at(250, Offset::new(1.0, 1.0));
    });
    // Constant: linear from (0, 0) to (2, 2).
    let expected = vector(4.0, 4.0);
    let (start, end) = (Offset::ZERO, Offset::new(2.0, 2.0));
    for time in [0, 10] {
        for initial in [Offset::ZERO, Offset::new(100.0, 100.0)] {
            assert_eq!(velocity_at(&spec, time, start, end, initial), expected);
        }
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, testDelay
#[test]
fn a_delay_holds_the_initial_value() {
    let spec = keyframes_with_spline(|frames| {
        frames.duration_millis = 500;
        frames.delay_millis = 100;
        frames.at(250, Offset::new(1.0, 1.0));
    });
    let end = Offset::new(2.0, 2.0);
    assert_eq!(value(&spec, 0), vector(0.0, 0.0));
    assert_eq!(value_at(&spec, 50, Offset::new(3.0, 3.0), end), vector(3.0, 3.0));
    assert_eq!(value_at(&spec, 100, Offset::new(5.0, 5.0), end), vector(5.0, 5.0));
    assert_eq!(value(&spec, 225), vector(0.5, 0.5));
    assert_eq!(value(&spec, 350), vector(1.0, 1.0));
    assert_eq!(value(&spec, 600), vector(2.0, 2.0));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, testNotEquals0
#[test]
fn splines_built_alike_are_equal() {
    let spec = || {
        keyframes_with_spline(|frames| {
            frames.duration_millis = 100;
            frames.at(30, Offset::new(1.0, 1.0));
            frames.at(50, Offset::new(2.0, 2.0));
        })
    };
    // Compose's has no `equals`, so these differ there; built alike, they're equal here, as
    // `keyframes` (D816), not to restart what plays them on every pass.
    assert_eq!(spec(), spec());
    let fractions = keyframes_with_spline(|frames| {
        frames.duration_millis = 100;
        frames.at_fraction(0.3, Offset::new(1.0, 1.0));
        frames.at_fraction(0.5, Offset::new(2.0, 2.0));
    });
    assert_eq!(spec(), fractions);
    let reversed = keyframes_with_spline(|frames| {
        frames.duration_millis = 100;
        frames.at_fraction(0.5, Offset::new(2.0, 2.0));
        frames.at_fraction(0.3, Offset::new(1.0, 1.0));
    });
    assert_eq!(spec(), reversed);
    // With another bias, or none: not.
    let periodic = |bias| {
        periodic_keyframes_with_spline(bias, |frames: &mut KeyframesWithSplineBuilder<Offset>| {
            frames.duration_millis = 100;
            frames.at(30, Offset::new(1.0, 1.0));
            frames.at(50, Offset::new(2.0, 2.0));
        })
    };
    assert_ne!(spec(), periodic(0.5));
    assert_ne!(periodic(0.0), periodic(0.5));
    assert_eq!(periodic(0.5), periodic(0.5));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeSplineAnimationTest.kt, testNotEquals1
#[test]
fn splines_differing_in_duration_values_times_or_count_differ() {
    let spec = |duration: u32, value: Offset, time: i32, extra: Option<i32>| {
        keyframes_with_spline(|frames| {
            frames.duration_millis = duration;
            frames.at(time, value);
            if let Some(time) = extra {
                frames.at(time, value);
            }
        })
    };
    let one = Offset::new(1.0, 1.0);
    assert_ne!(spec(400, one, 200, None), spec(401, one, 200, None));
    assert_ne!(
        spec(400, Offset::new(1.0, -1.0), 200, None),
        spec(400, Offset::new(-1.0, 1.0), 200, None)
    );
    assert_ne!(spec(400, one, 200, None), spec(400, one, 201, None));
    assert_ne!(spec(400, one, 200, None), spec(400, one, 200, Some(201)));
}

/// Where it plays: as a spec of `Animatable` and in a repeatable, a duration-based spec.
#[test]
fn a_spline_plays_as_any_duration_based_spec() {
    let spec = keyframes_with_spline(|frames: &mut KeyframesWithSplineBuilder<f32>| {
        frames.duration_millis = 100;
        frames.delay_millis = 20;
        frames.at(50, 0.25);
    });
    let duration: DurationBasedSpec = spec.clone().into();
    assert_eq!(duration.duration_millis(), 100);
    assert_eq!(duration.delay_millis(), 20);
    let animation: AnimationSpec = spec.into();
    let zero = AnimationVector::from(0.0);
    assert_eq!(animation.duration_nanos(zero, zero, zero), 120 * MILLIS_TO_NANOS);
    let repeated = repeatable(2, duration, RepeatMode::Restart);
    let value = |time| repeated.value_from_millis(time, zero, AnimationVector::from(1.0), zero)[0];
    assert_eq!(value(70), 0.25);
    assert_eq!(value(190), 0.25, "the second time");
}

/// A shared spec's splines are kept by start and end values: another end, with the same start,
/// isn't the last one's (D846).
#[test]
fn a_spec_read_with_another_end_value_follows_it() {
    let spec = keyframes_with_spline(|frames: &mut KeyframesWithSplineBuilder<Offset>| {
        frames.duration_millis = 500;
        frames.at(250, Offset::new(1.0, 1.0));
    });
    let (start, near, far) = (Offset::ZERO, Offset::new(2.0, 2.0), Offset::new(10.0, 10.0));
    let at_near = value_at(&spec, 375, start, near);
    let at_far = value_at(&spec, 375, start, far);
    assert_ne!(at_near, at_far);
    // And back, with more in between than it keeps.
    for end in [3.0, 4.0, 5.0, 6.0, 7.0] {
        value_at(&spec, 375, start, Offset::new(end, end));
    }
    assert_eq!(value_at(&spec, 375, start, near), at_near);
    assert_eq!(value_at(&spec, 375, start, far), at_far);
}

/// The same with another start value and the same end.
#[test]
fn a_spec_read_with_another_start_value_follows_it() {
    let spec = keyframes_with_spline(|frames: &mut KeyframesWithSplineBuilder<Offset>| {
        frames.duration_millis = 500;
        frames.at(250, Offset::new(1.0, 1.0));
    });
    let end = Offset::new(2.0, 2.0);
    let first = value_at(&spec, 125, Offset::ZERO, end);
    let second = value_at(&spec, 125, Offset::new(-4.0, -4.0), end);
    assert_ne!(first, second);
    assert_eq!(value_at(&spec, 125, Offset::ZERO, end), first);
}
