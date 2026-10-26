//! `repeatable` and `infinite_repeatable` against Compose's `RepeatableSpec` and
//! `InfiniteRepeatableSpec`: values across repetitions, durations, reversal, start offsets, and
//! the specs that can't repeat forever.

use compose_animation::*;

const DELAY: u32 = 13;
const DURATION: u32 = 50;
const MILLIS: i64 = 1_000_000;

/// The spec's value at `time_millis`, from 0 to 1 at rest, as Compose's `at`.
fn at(spec: impl VectorizedAnimationSpec, time_millis: u32) -> f32 {
    let (start, end) = (AnimationVector::from(0.0), AnimationVector::from(1.0));
    spec.value_from_millis(i64::from(time_millis), start, end, AnimationVector::from(0.0))[0]
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, twoRepeatsValuesCalculation
#[test]
fn two_repetitions_start_over() {
    let repeat = repeatable(2, tween(DURATION), RepeatMode::Restart);
    let animation = TargetBasedAnimation::new(repeat, 0.0f32, 0.0);
    assert_eq!(at(repeat, 0), 0.0);
    assert!(at(repeat, DURATION - 1) > 0.9);
    assert!(at(repeat, DURATION + 1) < 0.1);
    assert!(at(repeat, DURATION * 2 - 1) > 0.9);
    assert_eq!(at(repeat, DURATION * 2), 1.0);
    assert!(!animation.is_finished_from_millis(i64::from(DURATION) * 2 - 1));
    assert!(animation.is_finished_from_millis(i64::from(DURATION) * 2));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testRepeatedAnimationDuration
#[test]
fn a_repeatables_duration_is_its_repetitions_with_their_delays() {
    let repeat = repeatable(5, tween(DURATION).delay(DELAY), RepeatMode::Restart);
    let zero = AnimationVector::from(0.0);
    assert_eq!(repeat.duration_nanos(zero, zero, zero), i64::from((DELAY + DURATION) * 5) * MILLIS);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testRepeatModeReverse
#[test]
fn reversed_repetitions_go_back_and_forth() {
    let repeat = repeatable(9, tween(100).easing(LINEAR_EASING), RepeatMode::Reverse);
    let animation = TargetBasedAnimation::new(repeat, 0.0f32, 100.0);
    for time in 0..=100 {
        assert!((animation.value_from_millis(time) - time as f32).abs() <= 0.01, "at {time}");
    }
    for time in 100..=200 {
        let value = animation.value_from_millis(time);
        assert!((value - (200.0 - time as f32)).abs() <= 0.01, "at {time}");
    }
    assert_eq!(animation.value_from_millis(100), 100.0);
    assert_eq!(animation.value_from_millis(101), 99.0);
    assert_eq!(animation.value_from_millis(200), 0.0);
    assert_eq!(animation.value_from_millis(300), 100.0);
    assert_eq!(animation.value_from_millis(880), 80.0);
    assert_eq!(animation.value_from_millis(900), 100.0);
    assert_eq!(animation.value_from_millis(901), 100.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testInfiniteRepeat
#[test]
fn an_infinite_repeatable_never_ends() {
    let short = infinite_repeatable(tween(100).easing(LINEAR_EASING), RepeatMode::Reverse);
    let extra_long = 1_000_000_000u32;
    let long = infinite_repeatable(tween(extra_long).easing(LINEAR_EASING), RepeatMode::Restart);
    let (zero, hundred) = (AnimationVector::from(0.0), AnimationVector::from(100.0));
    assert_eq!(short.duration_nanos(zero, hundred, zero), i64::MAX);
    assert_eq!(long.duration_nanos(zero, hundred, zero), i64::MAX);
    let repeat_short = TargetBasedAnimation::new(short, 0.0f32, 100.0);
    let repeat_long = TargetBasedAnimation::new(long, 0.0f32, extra_long as f32);
    assert_eq!(repeat_short.duration_nanos(), i64::MAX);
    assert_eq!(repeat_long.duration_nanos(), i64::MAX);
    assert!(!repeat_short.is_finished_from_nanos(100_000_000_000_000_000));
    // Reversed.
    assert_eq!(repeat_short.value_from_nanos(31 * MILLIS), 31.0);
    assert_eq!(repeat_short.value_from_nanos(133 * MILLIS), 67.0);
    // Restarted.
    assert!((repeat_long.value_from_nanos(31 * MILLIS) - 31.0).abs() <= 0.1);
    let later = (i64::from(extra_long) + 31) * MILLIS;
    assert!((repeat_long.value_from_nanos(later) - 31.0).abs() <= 0.1);
}

/// A repeatable from 0 to 1000 and the same delayed and fast-forwarded by `offset`: the
/// delayed one is the plain one `offset` later, holding its start meanwhile; the fast-forwarded
/// one `offset` earlier.
fn check_start_offsets(make: impl Fn(StartOffset) -> RepeatableSpec, duration: i64, offset: u32) {
    let plain = TargetBasedAnimation::new(make(StartOffset::default()), 0.0f32, 1000.0);
    let delayed = TargetBasedAnimation::new(make(StartOffset::new(offset)), 0.0f32, 1000.0);
    let fast_forwarded = TargetBasedAnimation::new(
        make(StartOffset::with_type(offset, StartOffsetType::FastForward)),
        0.0f32,
        1000.0,
    );
    let offset = i64::from(offset);
    for time in (0..=duration * 3).step_by(17) {
        assert_eq!(
            plain.value_from_nanos(time * MILLIS),
            delayed.value_from_nanos((time + offset) * MILLIS),
            "delayed at {time}"
        );
        assert_eq!(
            plain.value_from_nanos(time * MILLIS),
            fast_forwarded.value_from_nanos((time - offset) * MILLIS),
            "fast-forwarded at {time}"
        );
    }
    for time in (0..=offset).step_by(10) {
        assert_eq!(plain.value_from_nanos(0), delayed.value_from_nanos(time * MILLIS));
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testStartOffsetRepeatable
#[test]
fn a_repeatables_start_offset_delays_or_fast_forwards_it() {
    let (duration, offset) = (600, 300);
    let make = |start| {
        repeatable_with(5, tween(duration).easing(LINEAR_EASING), RepeatMode::Restart, start)
    };
    let plain = TargetBasedAnimation::new(make(StartOffset::default()), 0.0f32, 1000.0);
    let delayed = TargetBasedAnimation::new(make(StartOffset::new(offset)), 0.0f32, 1000.0);
    let fast_forwarded = TargetBasedAnimation::new(
        make(StartOffset::with_type(offset, StartOffsetType::FastForward)),
        0.0f32,
        1000.0,
    );
    let offset_nanos = i64::from(offset) * MILLIS;
    assert_eq!(plain.duration_nanos(), delayed.duration_nanos() - offset_nanos);
    assert_eq!(plain.duration_nanos(), fast_forwarded.duration_nanos() + offset_nanos);
    check_start_offsets(make, i64::from(duration), offset);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testStartOffsetInfiniteRepeatable
#[test]
fn an_infinite_repeatables_start_offset_delays_or_fast_forwards_it() {
    let (duration, offset) = (600, 31);
    let make = |start| infinite_repeatable_with(tween(duration), RepeatMode::Restart, start);
    let plain = TargetBasedAnimation::new(make(StartOffset::default()), 0.0f32, 1000.0);
    let delayed = TargetBasedAnimation::new(make(StartOffset::new(offset)), 0.0f32, 1000.0);
    let fast_forwarded = TargetBasedAnimation::new(
        make(StartOffset::with_type(offset, StartOffsetType::FastForward)),
        0.0f32,
        1000.0,
    );
    assert_eq!(plain.duration_nanos(), delayed.duration_nanos());
    assert_eq!(plain.duration_nanos(), fast_forwarded.duration_nanos());
    check_start_offsets(make, i64::from(duration), offset);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testInfiniteRepeatableZeroDuration
#[test]
#[should_panic(expected = "0 duration")]
fn a_tween_of_no_time_cant_repeat_forever() {
    infinite_repeatable(tween(0), RepeatMode::Restart);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testInfiniteRepeatableZeroDurationSnap
#[test]
#[should_panic(expected = "0 duration")]
fn a_snap_cant_repeat_forever() {
    infinite_repeatable(snap(0), RepeatMode::Restart);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/RepeatableAnimationTest.kt, testInfiniteRepeatableZeroDurationKeyframes
#[test]
#[should_panic(expected = "0 duration")]
fn keyframes_of_no_time_cant_repeat_forever() {
    infinite_repeatable(
        keyframes(|frames| {
            frames.duration_millis = 0;
            frames.delay_millis = 0;
            frames.at(0, 0.0f32);
        }),
        RepeatMode::Restart,
    );
}
