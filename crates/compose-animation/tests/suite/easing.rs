//! Easing curves against Compose's `CubicBezierEasing` and `EasingFunctions`: their ends,
//! overshoot, equality, tiny and near-1 fractions, and the curve they draw; tweens along them.

use compose_animation::*;

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt, cubicBezierStartsAt0
#[test]
fn a_cubic_bezier_starts_at_0() {
    assert_eq!(FAST_OUT_SLOW_IN_EASING.transform(0.0), 0.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt, cubicBezierEndsAt1
#[test]
fn a_cubic_bezier_ends_at_1() {
    assert_eq!(FAST_OUT_LINEAR_IN_EASING.transform(1.0), 1.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt, cubicBezierDoesntExceed1
#[test]
fn a_cubic_bezier_within_bounds_doesnt_exceed_1() {
    assert!(Easing::CubicBezier(0.0, 0.0, 0.15, 1.0).transform(0.999_999) <= 1.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt, cubicBezierDoesExceed1
#[test]
fn an_overshooting_cubic_bezier_exceeds_1() {
    assert!(Easing::CubicBezier(0.34, 1.56, 0.64, 1.0).transform(0.6) > 1.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt, cubicBezierEquals
#[test]
fn cubic_beziers_are_equal_by_their_points() {
    let curve = Easing::CubicBezier(1.0, 2.0, 3.0, 4.0);
    assert_eq!(curve, Easing::CubicBezier(1.0, 2.0, 3.0, 4.0));
    for other in [
        Easing::CubicBezier(0.0, 2.0, 3.0, 4.0),
        Easing::CubicBezier(1.0, 0.0, 3.0, 4.0),
        Easing::CubicBezier(1.0, 2.0, 0.0, 4.0),
        Easing::CubicBezier(1.0, 2.0, 3.0, 0.0),
        Easing::CubicBezier(4.0, 3.0, 2.0, 1.0),
    ] {
        assert_ne!(curve, other);
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/EasingUnitTest.kt, canSolveCubicForSmallFractions
#[test]
fn a_cubic_bezier_solves_tiny_fractions() {
    let curve = Easing::CubicBezier(0.3, 0.0, 0.7, 1.0);
    for bits in [
        0x3e80_0000u32,
        0x3e00_0000,
        0x3d80_0000,
        0x3a80_0000,
        0x3600_0000,
        0x3480_0000,
        // Below the solver's epsilon.
        0x3400_0000,
        0x3421_0fb0,
        0x3380_0000,
        0x3300_0000,
        0x0000_0000,
    ] {
        let eased = curve.transform(f32::from_bits(bits));
        assert!((0.0..=1.0).contains(&eased), "{bits:#x}: {eased}");
    }
}

/// The curves defined within 0..1, as Compose's test lists them.
const IN_RANGE: [Easing; 26] = [
    Easing::CubicBezier(0.4, 0.0, 0.2, 1.0),
    EASE,
    EASE_IN,
    EASE_IN_BACK,
    EASE_IN_CIRC,
    EASE_IN_CUBIC,
    EASE_IN_EXPO,
    EASE_IN_OUT,
    EASE_IN_OUT_CIRC,
    EASE_IN_OUT_CUBIC,
    EASE_IN_OUT_EXPO,
    EASE_IN_OUT_QUAD,
    EASE_IN_OUT_QUART,
    EASE_IN_OUT_QUINT,
    EASE_IN_OUT_SINE,
    EASE_IN_SINE,
    EASE_OUT,
    EASE_OUT_CIRC,
    EASE_OUT_CUBIC,
    EASE_OUT_EXPO,
    EASE_OUT_QUAD,
    EASE_OUT_QUART,
    EASE_OUT_QUINT,
    EASE_OUT_SINE,
    EASE_IN_QUAD,
    EASE_IN_QUART,
];

// Compose: compose/animation/animation-core/src/androidHostTest/kotlin/androidx/compose/animation/core/EasingTest.android.kt, canSolveCubicForFractionsCloseToZeroOrOne
#[test]
fn curves_solve_fractions_close_to_0_and_1() {
    let ulp = f32::EPSILON;
    let (low, high) = (-ulp * 2.0, 1.0 + ulp * 2.0);
    for curve in IN_RANGE {
        let check = |bits: u32| {
            let fraction = f32::from_bits(bits);
            let eased = curve.transform(fraction);
            assert!((low..=high).contains(&eased), "f({fraction}) = {eased} for {curve:?}");
        };
        (0x3f7f_9d99..=0x3f7f_ffff).for_each(check);
        (0x0..=0x6266).for_each(check);
        check(0x3421_b161);
    }
}

// Compose: compose/animation/animation-core/src/androidHostTest/kotlin/androidx/compose/animation/core/EasingTest.android.kt, canSolveOvershootingCurve
#[test]
fn an_overshooting_curve_solves_fractions_close_to_1() {
    let highest = (0x3f7f_9d99..=0x3f7f_ffffu32)
        .map(|bits| EASE_OUT_BACK.transform(f32::from_bits(bits)))
        .fold(f32::MIN_POSITIVE, f32::max);
    assert!(highest > f32::MIN_POSITIVE);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/EasingTest.kt, easing
// Compose draws the curve as a path and its evaluation as lines, and compares the bitmaps; this
// compares the evaluation with the curve sampled densely by its parameter.
#[test]
fn a_cubic_bezier_follows_its_curve() {
    for [a, b, c, d] in [
        [0.40f32, 0.00, 0.20, 1.0],
        [0.00, 0.00, 0.20, 1.0],
        [0.40, 0.00, 1.00, 1.0],
        [0.34, 1.56, 0.64, 1.0],
        [0.00, 1.00, 0.00, 1.0],
    ] {
        let easing = Easing::CubicBezier(a, b, c, d);
        let point = |t: f64| {
            let u = 1.0 - t;
            let curve = |p1: f32, p2: f32| {
                3.0 * u * u * t * f64::from(p1) + 3.0 * u * t * t * f64::from(p2) + t * t * t
            };
            (curve(a, c), curve(b, d))
        };
        let samples: Vec<(f64, f64)> =
            (0..=20_000).map(|i| point(f64::from(i) / 20_000.0)).collect();
        for step in 1..128 {
            let x = step as f32 / 128.0;
            // The curve's height where its x is closest.
            let (_, y) = samples
                .iter()
                .min_by(|p, q| (p.0 - f64::from(x)).abs().total_cmp(&(q.0 - f64::from(x)).abs()))
                .unwrap();
            let eased = f64::from(easing.transform(x));
            assert!((eased - y).abs() < 2e-3, "({a}, {b}, {c}, {d}) at {x}: {eased} against {y}");
        }
    }
}

fn accelerate(fraction: f32) -> f32 {
    fraction * 2.0
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TweenAnimationTest.kt, delayCorrectness
#[test]
fn a_tween_waits_out_its_delay() {
    let spec = tween(200).delay(100).easing(LINEAR_EASING);
    let at = |time: i64| {
        spec.value_from_millis(
            time,
            AnimationVector::from(0.0),
            AnimationVector::from(1000.0),
            0.0.into(),
        )[0]
    };
    assert_eq!(at(0), 0.0);
    assert_eq!(at(50), 0.0);
    assert_eq!(at(100), 0.0);
    assert_ne!(at(101), 0.0);
}

/// A 1D spec's value from 0 to 1 at `time`, as Compose's test helper `at`.
fn at(spec: impl VectorizedAnimationSpec, time: i64) -> f32 {
    spec.value_from_millis(time, AnimationVector::from(0.0), AnimationVector::from(1.0), 0.0.into())
        [0]
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TweenAnimationTest.kt, easingIsApplied
#[test]
fn a_tween_follows_its_easing() {
    let spec = tween(300).easing(Easing::Function(accelerate));
    assert_eq!(at(spec, (300.0 * 0.3f32) as i64), accelerate(0.3));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TweenAnimationTest.kt, endValueCalculatedForPlaytimeOverDuration
#[test]
fn a_tween_past_its_duration_is_at_its_end() {
    assert_eq!(at(tween(200), 210), 1.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/SnapAnimationTest.kt, isFinishedAtTheBeginning
#[test]
fn a_snap_is_over_at_once() {
    assert_eq!(at(snap(0), 0), 1.0);
    assert!(TargetBasedAnimation::new(snap(0), 0.0f32, 0.0).is_finished_from_millis(0));
}

/// The elastic and bounce curves start at 0 and end at 1, and the bounce stays within them.
#[test]
fn the_function_curves_start_at_0_and_end_at_1() {
    for curve in [
        EASE_IN_ELASTIC,
        EASE_OUT_ELASTIC,
        EASE_IN_OUT_ELASTIC,
        EASE_IN_BOUNCE,
        EASE_OUT_BOUNCE,
        EASE_IN_OUT_BOUNCE,
    ] {
        assert!(curve.transform(0.0).abs() < 1e-6, "{curve:?}");
        assert!((curve.transform(1.0) - 1.0).abs() < 1e-6, "{curve:?}");
    }
    for step in 0..=100 {
        let bounced = EASE_OUT_BOUNCE.transform(step as f32 / 100.0);
        assert!((0.0..=1.0 + 1e-6).contains(&bounced), "{bounced}");
    }
}
