//! `keyframes` against Compose's `KeyframesSpec`, evaluated as its vectorized spec from 0 to 1
//! (Compose's `at` test helper): start and end values, easings per stretch, several
//! dimensions, equality, fractions, and times out of range.

use compose_animation::*;

/// The spec's value at `time_millis`, from 0 to 1 at rest, as Compose's `at`.
fn at(spec: &impl VectorizedAnimationSpec, time_millis: i32) -> f32 {
    let (start, end) = (AnimationVector::from(0.0), AnimationVector::from(1.0));
    spec.value_from_millis(i64::from(time_millis), start, end, AnimationVector::from(0.0))[0]
}

fn lerp(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, equalsStartAndEndValues
#[test]
fn keyframes_ending_where_they_start() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 400;
        frames.at(100, 0.0f32);
        frames.at(200, 0.5);
        frames.at(300, 0.8);
        frames.at(400, 0.0);
    });
    assert_eq!(at(&spec, 0), 0.0);
    assert_eq!(at(&spec, 250), 0.65);
    assert_eq!(at(&spec, 400), 0.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, possibleToOverrideStartAndEndValues
#[test]
fn keyframes_set_the_start_and_end_values() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 100;
        frames.at(0, 1.0f32);
        frames.at(100, 0.0);
    });
    assert_eq!(at(&spec, 0), 1.0);
    assert_eq!(at(&spec, 100), 0.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, withEasingOnFullDuration
#[test]
fn a_keyframe_eases_the_whole_duration() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 100;
        frames.at(0, 0.0f32).using(FAST_OUT_SLOW_IN_EASING);
        frames.at(100, 1.0);
    });
    assert_eq!(at(&spec, 31), FAST_OUT_SLOW_IN_EASING.transform(0.31));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, easingOnTheSecondPart
#[test]
fn a_keyframe_eases_the_stretch_after_it() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 200;
        frames.at(100, 1.0f32).using(FAST_OUT_SLOW_IN_EASING);
        frames.at(200, 2.0);
    });
    assert_eq!(at(&spec, 140), 1.0 + FAST_OUT_SLOW_IN_EASING.transform(0.4));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, firstPartIsLinearWithEasingOnTheSecondPart
#[test]
fn the_stretch_before_an_eased_keyframe_is_linear() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 100;
        frames.at(50, 0.5f32).using(FAST_OUT_SLOW_IN_EASING);
        frames.at(100, 1.0);
    });
    assert_eq!(at(&spec, 25), 0.25);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, testMultiDimensKeyframesWithEasing
#[test]
fn keyframes_of_two_dimensions_ease_each() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 400;
        frames.at(200, AnimationVector::from([200.0, 300.0])).using(FAST_OUT_LINEAR_IN_EASING);
    });
    let (start, end) = (AnimationVector::from([0.0, 0.0]), AnimationVector::from([200.0, 400.0]));
    for time in (0..=400).step_by(50) {
        let expected = if time < 200 {
            let fraction = time as f32 / 200.0;
            [lerp(0.0, 200.0, fraction), lerp(0.0, 300.0, fraction)]
        } else {
            let fraction = FAST_OUT_LINEAR_IN_EASING.transform((time - 200) as f32 / 200.0);
            [200.0, lerp(300.0, 400.0, fraction)]
        };
        assert_eq!(
            spec.value_from_millis(time, start, end, AnimationVector::from([0.0, 0.0])),
            AnimationVector::from(expected),
            "at {time}"
        );
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, testNotEquals1
#[test]
fn keyframes_differing_in_duration_easing_or_keyframes_differ() {
    let spec = |duration: u32, first_easing: Easing, second: (i32, f32), third_time: i32| {
        keyframes(move |frames| {
            frames.duration_millis = duration;
            frames.at(100, 0.0f32).using(first_easing.clone());
            frames.at(second.0, second.1).using(if first_easing == LINEAR_EASING {
                FAST_OUT_LINEAR_IN_EASING
            } else {
                LINEAR_EASING
            });
            frames.at(third_time, 0.8);
            frames.at(duration as i32, 1.0);
        })
    };
    let animation = spec(500, LINEAR_EASING, (200, 0.5), 300);
    let altered_duration = spec(700, LINEAR_EASING, (200, 0.5), 300);
    let altered_easing = spec(500, FAST_OUT_SLOW_IN_EASING, (200, 0.5), 300);
    let altered_keyframes = spec(500, LINEAR_EASING, (200, 0.3), 400);
    assert_ne!(animation, altered_duration);
    assert_ne!(animation, altered_easing);
    assert_ne!(animation, altered_keyframes);
    // Compose's `testNotEquals0` has equal keyframes built twice differ (`KeyframesSpec` has
    // no `equals`); here keyframes built alike are equal (D816).
    assert_eq!(animation, spec(500, LINEAR_EASING, (200, 0.5), 300));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, percentageBasedKeyFrames
#[test]
fn keyframes_at_fractions_of_the_duration() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 400;
        frames.at_fraction(0.25, 0.0f32);
        frames.at_fraction(0.5, 0.5);
        frames.at_fraction(0.75, 0.8);
        frames.at_fraction(1.0, 0.0);
    });
    assert_eq!(at(&spec, 0), 0.0);
    assert_eq!(at(&spec, 250), 0.65);
    assert_eq!(at(&spec, 400), 0.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, percentageBasedKeyframesWithEasing
#[test]
fn a_keyframe_at_a_fraction_eases_the_stretch_after_it() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 100;
        frames.at_fraction(0.5, 0.5f32).using(FAST_OUT_SLOW_IN_EASING);
        frames.at_fraction(1.0, 1.0);
    });
    assert_eq!(at(&spec, 25), 0.25);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, outOfRangeValuesOnly
#[test]
fn keyframes_out_of_range_are_ignored() {
    let (duration, delay) = (100, 200);
    let spec = keyframes(|frames| {
        frames.duration_millis = duration as u32;
        frames.delay_millis = delay as u32;
        frames.at(-delay, -1.0f32).using(LINEAR_EASING);
        frames.at(-duration, -2.0).using(LINEAR_EASING);
        frames.at(duration + 50, -3.0).using(LINEAR_EASING);
    });
    // The delay holds the start.
    assert_eq!(at(&spec, 0), 0.0);
    assert_eq!(at(&spec, 100), 0.0);
    assert_eq!(at(&spec, 200), 0.0);
    assert_eq!(at(&spec, delay), 0.0);
    assert_eq!(at(&spec, duration / 2 + delay), 0.5);
    assert_eq!(at(&spec, duration + delay), 1.0);
    // Past the end: the target.
    assert_eq!(at(&spec, delay + duration + 1), 1.0);
    assert_eq!(at(&spec, delay + duration + 50), 1.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeAnimationTest.kt, outOfRangeValues_withForcedInitialAndTarget
#[test]
fn keyframes_out_of_range_are_ignored_with_a_start_and_end_set() {
    let (duration, delay) = (100, 200);
    let spec = keyframes(|frames| {
        frames.duration_millis = duration as u32;
        frames.delay_millis = delay as u32;
        frames.at(-delay, -1.0f32).using(LINEAR_EASING);
        frames.at(-duration, -2.0).using(LINEAR_EASING);
        frames.at(duration + 50, -3.0).using(LINEAR_EASING);
        frames.at(0, 4.0).using(LINEAR_EASING);
        frames.at(duration, 5.0).using(LINEAR_EASING);
    });
    assert_eq!(at(&spec, 0), 4.0);
    assert_eq!(at(&spec, 100), 4.0);
    assert_eq!(at(&spec, 200), 4.0);
    assert_eq!(at(&spec, delay), 4.0);
    assert_eq!(at(&spec, duration / 2 + delay), 4.5);
    assert_eq!(at(&spec, duration + delay), 5.0);
    assert_eq!(at(&spec, delay + duration + 1), 5.0);
    assert_eq!(at(&spec, delay + duration + 50), 5.0);
}
