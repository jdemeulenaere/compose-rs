//! `ArcAnimationSpec` and `keyframes` by `ArcMode`, against Compose's `ArcAnimationTest` and
//! `KeyframeArcAnimationTest` (the curves' plots are unit tests of `arc_spline.rs`).

use compose_animation::*;
use compose_ui::Offset;

const TIME_MILLIS: u32 = 1000;
const TARGET: f32 = 300.0;
const ERROR: f32 = 0.01;

fn close(a: f32, b: f32, tolerance: f32) {
    assert!((a - b).abs() <= tolerance, "{a} isn't {b}");
}

fn arc(mode: ArcMode) -> ArcAnimationSpec {
    ArcAnimationSpec::new(mode).duration(TIME_MILLIS).easing(LINEAR_EASING)
}

fn filled(size: usize, value: f32) -> AnimationVector {
    AnimationVector::filled(size, value)
}

fn play_nanos(percent: f32) -> i64 {
    (TIME_MILLIS as f32 * percent) as i64 * 1_000_000
}

fn value_at(spec: &ArcAnimationSpec, size: usize, percent: f32) -> AnimationVector {
    spec.value_from_nanos(
        play_nanos(percent),
        filled(size, 0.0),
        filled(size, TARGET),
        filled(size, 0.0),
    )
}

fn velocity_at(spec: &ArcAnimationSpec, size: usize, percent: f32) -> AnimationVector {
    spec.velocity_from_nanos(
        play_nanos(percent),
        filled(size, 0.0),
        filled(size, TARGET),
        filled(size, 0.0),
    )
}

fn linear_at(size: usize, percent: f32) -> AnimationVector {
    filled(size, percent * TARGET)
}

/// X ahead of the line and Y behind it (above), or the other way (below), and the complements.
fn assert_above(spec: &ArcAnimationSpec, size: usize) {
    for percent in [0.25, 0.5, 0.75] {
        let (value, linear) = (value_at(spec, size, percent), linear_at(size, percent));
        assert!(value[0] > linear[0], "{percent}: {value:?}");
        assert!(value[1] < linear[1], "{percent}: {value:?}");
    }
    close(TARGET - value_at(spec, size, 0.25)[0], value_at(spec, size, 0.75)[1], ERROR);
    let middle = velocity_at(spec, size, 0.5);
    close(middle[0], middle[1], ERROR);
    let start = velocity_at(spec, size, 0.0);
    close(0.0, start[1], ERROR);
    assert!(start[0] > ERROR);
    let end = velocity_at(spec, size, 1.0);
    close(0.0, end[0], ERROR);
    assert!(end[1] > ERROR);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, test2DInterpolation_withArcAbove
#[test]
fn an_arc_above_leads_in_x_and_lags_in_y() {
    assert_above(&arc(ArcMode::ArcAbove), 2);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, test2DInterpolation_withArcBelow
#[test]
fn an_arc_below_lags_in_x_and_leads_in_y() {
    let spec = arc(ArcMode::ArcBelow);
    for percent in [0.25, 0.5, 0.75] {
        let (value, linear) = (value_at(&spec, 2, percent), linear_at(2, percent));
        assert!(value[0] < linear[0], "{percent}: {value:?}");
        assert!(value[1] > linear[1], "{percent}: {value:?}");
    }
    close(TARGET - value_at(&spec, 2, 0.25)[1], value_at(&spec, 2, 0.75)[0], ERROR);
    let middle = velocity_at(&spec, 2, 0.5);
    close(middle[0], middle[1], ERROR);
    let start = velocity_at(&spec, 2, 0.0);
    close(0.0, start[0], ERROR);
    assert!(start[1] > ERROR);
    let end = velocity_at(&spec, 2, 1.0);
    close(0.0, end[1], ERROR);
    assert!(end[0] > ERROR);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, test2DInterpolation_withLinearArc
#[test]
fn a_linear_arc_is_the_line() {
    let spec = arc(ArcMode::ArcLinear);
    for percent in [0.25, 0.5, 0.75] {
        assert_eq!(value_at(&spec, 2, percent), linear_at(2, percent));
        let velocity = velocity_at(&spec, 2, percent);
        close(0.0, velocity[0] - velocity[1], ERROR);
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, test2DInterpolation_withEasing
#[test]
fn an_easing_moves_along_the_arc() {
    let (plain, eased) = (
        arc(ArcMode::ArcLinear),
        ArcAnimationSpec::new(ArcMode::ArcLinear)
            .duration(TIME_MILLIS)
            .easing(FAST_OUT_SLOW_IN_EASING),
    );
    // At 15 % it lags, at 26 % they're about the same, at 50 % it leads.
    let (a, b) = (value_at(&plain, 2, 0.15), value_at(&eased, 2, 0.15));
    assert!(a[0] > b[0] && a[1] > b[1]);
    let (a, b) = (value_at(&plain, 2, 0.26), value_at(&eased, 2, 0.26));
    close(a[0], b[0], 1.0);
    close(a[1], b[1], 1.0);
    let (a, b) = (value_at(&plain, 2, 0.5), value_at(&eased, 2, 0.5));
    assert!(a[0] < b[0] && a[1] < b[1]);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, test1DInterpolation_isAlwaysLinear
#[test]
fn one_dimension_is_always_the_line() {
    for mode in [ArcMode::ArcAbove, ArcMode::ArcBelow, ArcMode::ArcLinear] {
        let spec = arc(mode);
        for percent in [0.25, 0.5, 0.75] {
            assert_eq!(value_at(&spec, 1, percent), linear_at(1, percent), "{mode:?}");
        }
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, test3DInterpolation_firstPairAsArc
#[test]
fn of_three_dimensions_the_first_two_arc() {
    assert_above(&arc(ArcMode::ArcAbove), 3);
    // The third goes along the line (Compose's test leaves it out).
    let value = value_at(&arc(ArcMode::ArcAbove), 3, 0.5);
    assert_eq!(value[2], linear_at(3, 0.5)[2]);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, test4DInterpolation_twoPairsAsArcs
#[test]
fn four_dimensions_are_two_arcs() {
    let spec = arc(ArcMode::ArcAbove);
    for percent in [0.25, 0.5, 0.75] {
        let (value, linear) = (value_at(&spec, 4, percent), linear_at(4, percent));
        assert!(value[0] > linear[0] && value[1] < linear[1], "{percent}: {value:?}");
        assert!(value[2] > linear[2] && value[3] < linear[3], "{percent}: {value:?}");
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, testEquals
#[test]
fn arcs_of_the_same_parameters_are_equal() {
    assert_eq!(ArcAnimationSpec::new(ArcMode::ArcAbove), ArcAnimationSpec::new(ArcMode::ArcAbove));
    let custom =
        |easing| ArcAnimationSpec::new(ArcMode::ArcBelow).duration(13).delay(17).easing(easing);
    assert_eq!(custom(EASE_IN_OUT), custom(Easing::cubic_bezier(0.42, 0.0, 0.58, 1.0)));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, testNotEquals
#[test]
fn arcs_of_other_parameters_differ() {
    let linear = || ArcAnimationSpec::new(ArcMode::ArcLinear);
    assert_ne!(ArcAnimationSpec::new(ArcMode::ArcAbove), ArcAnimationSpec::new(ArcMode::ArcBelow));
    assert_ne!(linear().duration(5), linear().duration(7));
    assert_ne!(linear().delay(9), linear().delay(11));
    assert_ne!(linear().easing(EASE_IN_OUT), linear().easing(FAST_OUT_SLOW_IN_EASING));
}

/// As a spec of what plays specs, and repeated: `DurationBasedAnimationSpec`.
#[test]
fn an_arc_plays_as_a_duration_based_spec() {
    let spec = arc(ArcMode::ArcAbove).delay(100);
    let (start, end, zero) = (filled(2, 0.0), filled(2, TARGET), filled(2, 0.0));
    let animation: AnimationSpec = spec.clone().into();
    assert_eq!(animation.duration_nanos(start, end, zero), 1100 * 1_000_000);
    let during = animation.value_from_nanos(600 * 1_000_000, start, end, zero);
    assert_eq!(during, spec.value_from_nanos(600 * 1_000_000, start, end, zero));
    assert!(during[0] > during[1], "an arc: {during:?}");
    let duration: DurationBasedSpec = spec.into();
    assert_eq!((duration.duration_millis(), duration.delay_millis()), (1000, 100));
    // Its delay holds the start.
    assert_eq!(animation.value_from_nanos(50 * 1_000_000, start, end, zero), start);
}

/// A spec's kept arcs follow the values it's asked for.
#[test]
fn other_start_and_end_values_give_other_arcs() {
    let spec = arc(ArcMode::ArcBelow);
    let middle = |start: f32, end: f32| {
        spec.value_from_nanos(500 * 1_000_000, filled(2, start), filled(2, end), filled(2, 0.0))
    };
    let first = middle(0.0, 300.0);
    assert_eq!(middle(0.0, 300.0), first);
    let doubled = middle(0.0, 600.0);
    close(doubled[0], first[0] * 2.0, 0.01);
    assert_eq!(middle(0.0, 300.0), first);
}

const KEYFRAME_MILLIS: u32 = 3000;
const KEYFRAME_TARGET: f32 = 600.0;

fn keyframe_value(spec: &KeyframesSpec, millis: i64) -> AnimationVector {
    spec.value_from_nanos(
        millis * 1_000_000,
        filled(2, 0.0),
        filled(2, KEYFRAME_TARGET),
        filled(2, 0.0),
    )
}

fn keyframe_linear(percent: f32) -> f32 {
    percent * KEYFRAME_TARGET
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeArcAnimationTest.kt, test2DArcKeyFrame_interpolatedValues
#[test]
fn each_stretch_of_keyframes_takes_its_first_keyframe_s_arc() {
    let spec = keyframes(|frames| {
        frames.duration_millis = KEYFRAME_MILLIS;
        frames.at(0, Offset::ZERO).using(LINEAR_EASING).using_arc(ArcMode::ArcAbove);
        frames
            .at(1000, Offset::new(200.0, 200.0))
            .using(LINEAR_EASING)
            .using_arc(ArcMode::ArcBelow);
        frames
            .at_fraction(2.0 / 3.0, Offset::new(400.0, 400.0))
            .using(LINEAR_EASING)
            .using_arc(ArcMode::ArcLinear);
    });
    let value = keyframe_value(&spec, 500);
    let linear = keyframe_linear(1.0 / 6.0);
    assert!(value[0] > linear && value[1] < linear, "above: {value:?}");
    let value = keyframe_value(&spec, 1500);
    let linear = keyframe_linear(3.0 / 6.0);
    assert!(value[0] < linear && value[1] > linear, "below: {value:?}");
    let value = keyframe_value(&spec, 2500);
    let linear = keyframe_linear(5.0 / 6.0);
    close(value[0], linear, 0.0001);
    close(value[1], linear, 0.0001);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/KeyframeArcAnimationTest.kt, test2DArcKeyFrame_multipleEasing
#[test]
fn linear_arcs_keep_each_stretch_s_easing() {
    let spec = keyframes(|frames| {
        frames.duration_millis = KEYFRAME_MILLIS;
        frames.at(0, Offset::ZERO).using(EASE_IN_CUBIC).using_arc(ArcMode::ArcLinear);
        frames
            .at(1000, Offset::new(200.0, 200.0))
            .using(LINEAR_EASING)
            .using_arc(ArcMode::ArcLinear);
        frames
            .at_fraction(2.0 / 3.0, Offset::new(400.0, 400.0))
            .using(EASE_OUT_CUBIC)
            .using_arc(ArcMode::ArcLinear);
    });
    let value = keyframe_value(&spec, 500);
    assert!(value[0] < keyframe_linear(1.0 / 6.0) && value[1] < keyframe_linear(1.0 / 6.0));
    let value = keyframe_value(&spec, 1500);
    close(value[0], keyframe_linear(3.0 / 6.0), 0.0001);
    close(value[1], keyframe_linear(3.0 / 6.0), 0.0001);
    let value = keyframe_value(&spec, 2500);
    assert!(value[0] > keyframe_linear(5.0 / 6.0) && value[1] > keyframe_linear(5.0 / 6.0));
}

/// Keyframes with no arc are as they were; an arc mode tells two keyframes apart.
#[test]
fn an_arc_mode_is_part_of_what_tells_keyframes_apart() {
    let spec = |mode: ArcMode| {
        keyframes(|frames: &mut KeyframesBuilder<Offset>| {
            frames.duration_millis = 100;
            frames.at(0, Offset::ZERO).using_arc(mode);
        })
    };
    assert_eq!(spec(ArcMode::ArcAbove), spec(ArcMode::ArcAbove));
    assert_ne!(spec(ArcMode::ArcAbove), spec(ArcMode::ArcBelow));
}
