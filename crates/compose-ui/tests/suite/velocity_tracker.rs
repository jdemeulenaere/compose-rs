//! Velocity tracking, as Compose's: `VelocityTracker` (an Lsq2 tracker per axis),
//! `VelocityTracker1D` (Lsq2 or impulse, positions or changes), the 40 ms "pointer stopped"
//! cut-off, 20 samples within 100 ms, and the least-squares fit behind them.

// Compose's recorded samples, with their digits.
#![allow(clippy::excessive_precision)]

use compose_ui::{
    Offset, Velocity, VelocityTracker, VelocityTracker1D, VelocityTrackingStrategy,
    poly_fit_least_squares,
};

use crate::velocity_data::{
    EXPECTED_2D_VELOCITIES, INTERRUPTED_VELOCITY_EVENT_DATA, VELOCITY_EVENT_DATA,
};

/// How close a 2D velocity must be, as Compose's `Lsq2VelocityTrackerTest`.
const TOLERANCE_2D: f32 = 0.2;

fn check_velocity(actual: Velocity, expected_x: f32, expected_y: f32) {
    assert!((actual.x - expected_x).abs() <= TOLERANCE_2D, "x {} for {expected_x}", actual.x);
    assert!((actual.y - expected_y).abs() <= TOLERANCE_2D, "y {} for {expected_y}", actual.y);
}

/// Feeds each recorded gesture (its down samples) to a tracker and calls `check` with the
/// tracker and the gesture's number at its up.
fn for_each_gesture(
    data: &[(i64, f32, f32, bool)],
    mut check: impl FnMut(&mut VelocityTracker, usize),
) {
    let mut tracker = VelocityTracker::new();
    let mut gesture = 0;
    for &(uptime, x, y, down) in data {
        if down {
            tracker.add_position(uptime, Offset::new(x, y));
        } else {
            check(&mut tracker, gesture);
            tracker.reset_tracking();
            gesture += 1;
        }
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_returnsExpectedValues
#[test]
fn recorded_gestures_give_their_expected_velocities() {
    for_each_gesture(VELOCITY_EVENT_DATA, |tracker, gesture| {
        let (x, y) = EXPECTED_2D_VELOCITIES[gesture];
        check_velocity(tracker.calculate_velocity(), x, y);
    });
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_returnsExpectedValues_withMaximumVelocity
#[test]
fn recorded_gestures_give_their_velocities_within_a_maximum() {
    let maximum = Velocity::new(200.0, 200.0);
    for_each_gesture(VELOCITY_EVENT_DATA, |tracker, gesture| {
        let (x, y) = EXPECTED_2D_VELOCITIES[gesture];
        check_velocity(
            tracker.calculate_velocity_with(maximum),
            x.abs().min(maximum.x) * x.signum(),
            y.abs().min(maximum.y) * y.signum(),
        );
    });
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_gapOf40MillisecondsInPositions_positionsAfterGapIgnored
#[test]
fn samples_before_a_gap_of_more_than_40_milliseconds_are_ignored() {
    for_each_gesture(INTERRUPTED_VELOCITY_EVENT_DATA, |tracker, _| {
        check_velocity(tracker.calculate_velocity(), 649.489_3, 3_890.305);
    });
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_noData_returnsZero
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_onePosition_returnsZero
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, resetTracking_resetsTracking
#[test]
fn no_data_one_position_or_a_reset_gives_no_velocity() {
    let mut tracker = VelocityTracker::new();
    assert_eq!(tracker.calculate_velocity(), Velocity::ZERO);
    let (uptime, x, y, _) = VELOCITY_EVENT_DATA[0];
    tracker.add_position(uptime, Offset::new(x, y));
    assert_eq!(tracker.calculate_velocity(), Velocity::ZERO);
    tracker.add_position(uptime + 10, Offset::new(x + 10.0, y));
    tracker.reset_tracking();
    assert_eq!(tracker.calculate_velocity(), Velocity::ZERO);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_twoPoints_sameTimeStamp_returnsZero
#[test]
fn points_at_the_same_time_give_no_velocity() {
    let mut tracker = VelocityTracker::new();
    for x in [100.0, 200.0, 300.0] {
        tracker.add_position(1000, Offset::new(x, 0.0));
    }
    assert_eq!(tracker.calculate_velocity(), Velocity::ZERO);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_whenOlderSamplesDiscardedDueToTimeGap_computesVelocityWithRemainingTwoSamples
#[test]
fn the_two_samples_after_a_gap_give_the_velocity() {
    let mut tracker = VelocityTracker::new();
    tracker.add_position(0, Offset::new(0.0, 0.0));
    tracker.add_position(20, Offset::new(100.0, 0.0));
    // A gap of more than 40 ms: the samples before it don't count.
    tracker.add_position(120, Offset::new(200.0, 0.0));
    tracker.add_position(130, Offset::new(300.0, 0.0));
    check_velocity(tracker.calculate_velocity(), 10_000.0, 0.0);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocity_exactlyTwoSamples_returnsLinearVelocity
#[test]
fn two_samples_give_a_linear_velocity() {
    let mut tracker = VelocityTracker::new();
    tracker.add_position(0, Offset::new(0.0, 0.0));
    tracker.add_position(10, Offset::new(10.0, 0.0));
    check_velocity(tracker.calculate_velocity(), 1000.0, 0.0);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTrackerTest.kt, calculateVelocityWithMaxValue_valueShouldBeGreaterThanZero
#[test]
#[should_panic(expected = "should be positive")]
fn a_negative_maximum_velocity_panics() {
    VelocityTracker::new().calculate_velocity_with(Velocity::new(-1.0, 1.0));
}

/// A release more than 40 ms after the last move starts over: no fling, as Compose's
/// `Lsq2VelocityTracker.addPointerInputChange` (b/238654963).
#[test]
fn a_release_after_a_pause_gives_no_velocity() {
    let mut tracker = VelocityTracker::new();
    tracker.add_pointer_input_change(0, Offset::new(0.0, 0.0), true, false);
    for step in 1..=5 {
        tracker.add_pointer_input_change(
            step * 10,
            Offset::new(0.0, step as f32 * 10.0),
            false,
            false,
        );
    }
    assert!(tracker.calculate_velocity().y > 900.0);
    tracker.add_pointer_input_change(100, Offset::new(0.0, 50.0), false, true);
    assert_eq!(tracker.calculate_velocity(), Velocity::ZERO);
}

// ---- VelocityTracker1D ---------------------------------------------------------------------

/// How close a 1D velocity must be: within this fraction of it, as Compose's
/// `VelocityTracker1DTest`.
const TOLERANCE_1D: f32 = 0.2;

/// Checks the velocity each strategy gives `points` (positions, or changes if `differential`),
/// within `maximum`, as Compose's `checkTestCase`.
fn check_test_case(
    differential: bool,
    points: &[(i64, f32)],
    expected: &[(VelocityTrackingStrategy, f32)],
    maximum: f32,
) {
    for &(strategy, velocity) in expected {
        let mut tracker = VelocityTracker1D::with_strategy(differential, strategy);
        for &(time, point) in points {
            tracker.add_data_point(time, point);
        }
        let clamped = if velocity == 0.0 { 0.0 } else { velocity.clamp(-maximum, maximum) };
        let actual = tracker.calculate_velocity_with(maximum);
        assert!(
            (actual - clamped).abs() <= clamped.abs() * TOLERANCE_1D,
            "{strategy:?}: {actual} for {clamped}, points {points:?}"
        );
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, lsq2_differentialValues_unsupported
#[test]
#[should_panic(expected = "Lsq2 not (yet) supported")]
fn lsq2_takes_no_differential_data() {
    VelocityTracker1D::with_strategy(true, VelocityTrackingStrategy::Lsq2);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, resetTracking_defaultConstructor
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, resetTracking_differentialValues_impulse
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, resetTracking_nonDifferentialValues_impulse
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, resetTracking_nonDifferentialValues_lsq2
#[test]
fn a_reset_1d_tracker_gives_no_velocity() {
    let trackers = [
        VelocityTracker1D::default(),
        VelocityTracker1D::with_strategy(true, VelocityTrackingStrategy::Impulse),
        VelocityTracker1D::with_strategy(false, VelocityTrackingStrategy::Impulse),
        VelocityTracker1D::with_strategy(false, VelocityTrackingStrategy::Lsq2),
    ];
    for mut tracker in trackers {
        for (time, point) in [(0, 0.0), (10, 5.0), (20, 10.0), (30, 15.0), (40, 30.0)] {
            tracker.add_data_point(time, point);
        }
        tracker.reset_tracking();
        assert_eq!(tracker.calculate_velocity(), 0.0);
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, calculateVelocityWithMaxValue_valueShouldBeGreaterThanZero
#[test]
#[should_panic(expected = "should be a positive value")]
fn a_negative_1d_maximum_velocity_panics() {
    VelocityTracker1D::default().calculate_velocity_with(-1.0);
}

/// Checks each recorded gesture along each axis with `strategy` against `expected`.
fn check_recorded_gestures(
    strategy: VelocityTrackingStrategy,
    expected: &[(f32, f32)],
    maximum: f32,
) {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    let mut gesture = 0;
    for &(uptime, x, y, down) in VELOCITY_EVENT_DATA {
        if down {
            xs.push((uptime, x));
            ys.push((uptime, y));
        } else {
            check_test_case(false, &xs, &[(strategy, expected[gesture].0)], maximum);
            check_test_case(false, &ys, &[(strategy, expected[gesture].1)], maximum);
            xs.clear();
            ys.clear();
            gesture += 1;
        }
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, testsFromThe2DVelocityTrackerTest_noClamping_lsq
#[test]
fn the_recorded_gestures_along_each_axis_with_lsq2() {
    check_recorded_gestures(VelocityTrackingStrategy::Lsq2, EXPECTED_2D_VELOCITIES, f32::MAX);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, testsFromThe2DVelocityTrackerTest_noClamping_impulse
#[test]
fn the_recorded_gestures_along_each_axis_with_the_impulse_strategy() {
    let expected = [
        (118.8, 799.0),
        (214.5, 855.8),
        (-50.5, 182.2),
        (628.5, -2127.9),
        (120.72147, -2970.8),
        (537.3, 2236.1),
        (450.44498, 1786.9),
        (430.9, -2648.1),
        (248.4, -2723.7),
        (285.7, -2929.3),
        (322.6, 2369.8),
        (1024.0, 4477.2),
        (629.8, 3802.5),
    ];
    check_recorded_gestures(VelocityTrackingStrategy::Impulse, &expected, f32::MAX);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, testsFromThe2DVelocityTrackerTest_withVelocityClamping
#[test]
fn the_recorded_gestures_along_each_axis_within_a_maximum() {
    check_recorded_gestures(VelocityTrackingStrategy::Lsq2, EXPECTED_2D_VELOCITIES, 500.0);
}

// ---- The least-squares fit ------------------------------------------------------------------

fn assert_fit(x: &[f32], y: &[f32], degree: usize, expected: &[f32]) {
    let actual = poly_fit_least_squares(x, y, degree).expect("a fit");
    for (index, value) in expected.iter().enumerate() {
        assert!(
            (actual[index] - value).abs() <= 0.00001,
            "coefficient {index}: {} for {value} ({actual:?})",
            actual[index]
        );
    }
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear2PointsSlopeOf0Intercept1_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear2PointsSlopeOf1Intercept0_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear2PointsSlopeOf1000000_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear2PointsSlopeOfNegative5_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear2PointsRandom1_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear2PointsRandom2_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear2PointsRandom3_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_linear4PointsImperfect_isCorrect
#[test]
fn a_line_fits_two_or_more_points() {
    assert_fit(&[0.0, 1.0], &[1.0, 1.0], 1, &[1.0, 0.0]);
    assert_fit(&[0.0, 1.0], &[0.0, 1.0], 1, &[0.0, 1.0]);
    assert_fit(&[0.0, 1.0], &[0.0, 1_000_000.0], 1, &[0.0, 1_000_000.0]);
    assert_fit(&[0.0, 1.0], &[0.0, -5.0], 1, &[0.0, -5.0]);
    assert_fit(&[-8.0, -2.0], &[7.0, 5.0], 1, &[4.33333, -0.33333]);
    assert_fit(&[-7.0, 4.0], &[4.0, -2.0], 1, &[0.181818, -0.545454]);
    assert_fit(&[4.0, -6.0], &[-6.0, 0.0], 1, &[-3.6, -0.6]);
    assert_fit(&[0.0, 2.0, 0.0, 2.0], &[0.0, 1.0, 2.0, 3.0], 1, &[1.0, 0.5]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_quadratic3PointsActuallyLinear_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_quadratic3Points_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_quadratic5Points_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_quadratic4PointsImperfect_isCorrect
#[test]
fn a_parabola_fits_three_or_more_points() {
    assert_fit(&[0.0, 1.0, 2.0], &[0.0, 1.0, 2.0], 2, &[0.0, 1.0, 0.0]);
    assert_fit(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0], 2, &[0.0, 2.0, -1.0]);
    assert_fit(&[0.0, 1.0, 2.0, 3.0, 4.0], &[0.0, 1.0, 4.0, 9.0, 16.0], 2, &[0.0, 0.0, 1.0]);
    assert_fit(&[0.0, 1.0, 2.0, 1.0], &[0.0, -1.0, 0.0, -2.0], 2, &[0.0, -3.0, 1.5]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_cubic4PointsActuallyLinear_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_cubic4Points_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_cubic6PointsImperfect_isCorrect
#[test]
fn a_cubic_fits_four_or_more_points() {
    assert_fit(&[0.0, 1.0, 2.0, 3.0], &[0.0, 1.0, 2.0, 3.0], 3, &[0.0, 1.0, 0.0, 0.0]);
    assert_fit(&[-1.0, 0.0, 1.0, 2.0], &[1.0, 0.0, 1.0, 0.0], 3, &[0.0, 0.66666, 1.0, -0.66666]);
    assert_fit(
        &[-1.0, 0.0, 1.0, 2.0, 0.0, 1.0],
        &[1.0, 0.0, 1.0, 0.0, 1.0, 0.0],
        3,
        &[0.5, -0.083333, 0.25, -0.16666],
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_1Point_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_degreeLargerThanData_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_3Points2IdenticalDegree1_isCorrect
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_3Points2IdenticalDegree2
#[test]
fn too_few_or_repeated_points_lower_the_degree() {
    assert_fit(&[0.0], &[13.0], 1, &[13.0, 0.0]);
    assert_fit(&[0.0, 1.0], &[0.0, 1.0], 2, &[0.0, 1.0, 0.0]);
    assert_fit(&[0.0, 0.0, 1.0], &[0.0, 0.0, 1.0], 1, &[0.0, 1.0]);
    assert_fit(&[0.0, 0.0, 1.0], &[0.0, 0.0, 1.0], 2, &[0.0, 0.0]);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_degreeIsNegative_throwsIllegalArgumentException
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/gesture/scrollorientationlocking/PolyFitLeastSquaresTest.kt, polyFitLeastSquares_noData_throwsIllegalArgumentException
#[test]
fn no_degree_or_no_data_has_no_fit() {
    assert_eq!(poly_fit_least_squares(&[0.0, 1.0], &[0.0, 1.0], 0), None);
    assert_eq!(poly_fit_least_squares(&[], &[], 1), None);
}

// The data-driven cases of Compose's `VelocityTracker1DTest`, one test each.

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, twoPoints_nonDifferentialValues
#[test]
fn two_positions_give_their_slope() {
    check_test_case(
        false,
        &[(1, 5.0), (2, 15.0)],
        &[(VelocityTrackingStrategy::Impulse, 10000.0), (VelocityTrackingStrategy::Lsq2, 10000.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, twoPoints_differentialValues
#[test]
fn two_changes_give_their_impulse() {
    check_test_case(
        true,
        &[(1, 5.0), (2, 10.0)],
        &[(VelocityTrackingStrategy::Impulse, 10000.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, threePoints_pointerStoppedMoving_nonDifferentialValues
#[test]
fn positions_after_a_pause_give_no_velocity() {
    check_test_case(
        false,
        &[(25, 25.0), (50, 50.0), (100, 100.0)],
        &[(VelocityTrackingStrategy::Lsq2, 0.0), (VelocityTrackingStrategy::Impulse, 0.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, threePoints_pointerStoppedMoving_differentialValues
#[test]
fn changes_after_a_pause_give_no_velocity() {
    check_test_case(
        true,
        &[(25, 25.0), (50, 50.0), (100, 100.0)],
        &[(VelocityTrackingStrategy::Impulse, 0.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, threePoints_zeroVelocity_nonDifferentialValues
#[test]
fn a_position_held_gives_no_velocity() {
    check_test_case(
        false,
        &[(0, 273.0), (1, 273.0), (2, 273.0)],
        &[(VelocityTrackingStrategy::Impulse, 0.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, threePoints_zeroVelocity_differentialValues
#[test]
fn changes_of_nothing_give_no_velocity() {
    check_test_case(
        true,
        &[(0, 0.0), (1, 0.0), (2, 0.0)],
        &[(VelocityTrackingStrategy::Impulse, 0.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_positiveVelocity_positiveDataPoints_nonDifferentialValues
#[test]
fn rising_positions_above_zero_give_their_rate() {
    check_test_case(
        false,
        &[(0, 0.0), (10, 5.0), (20, 10.0), (30, 15.0)],
        &[(VelocityTrackingStrategy::Impulse, 500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_positiveVelocity_positiveDataPoints_differentialValues
#[test]
fn equal_positive_changes_give_their_rate() {
    check_test_case(
        true,
        &[(10, 5.0), (20, 5.0), (30, 5.0)],
        &[(VelocityTrackingStrategy::Impulse, 500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_positiveVelocity_negativeDataPoints_nonDifferentialValues
#[test]
fn rising_positions_below_zero_give_their_rate() {
    check_test_case(
        false,
        &[(0, -20.0), (10, -15.0), (20, -10.0)],
        &[(VelocityTrackingStrategy::Impulse, 500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_positiveVelocity_mixedSignDataPoints_nonDifferentialValues
#[test]
fn rising_positions_through_zero_give_their_rate() {
    check_test_case(
        false,
        &[(0, -5.0), (10, 0.0), (20, 5.0)],
        &[(VelocityTrackingStrategy::Impulse, 500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_negativeVelocity_negativeDataPoints_nonDifferentialValues
#[test]
fn falling_positions_below_zero_give_their_rate() {
    check_test_case(
        false,
        &[(0, 0.0), (10, -5.0), (20, -10.0)],
        &[(VelocityTrackingStrategy::Impulse, -500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_negativeVelocity_negativeDataPoints_differentialValues
#[test]
fn equal_negative_changes_give_their_rate() {
    check_test_case(
        true,
        &[(0, 5.0), (10, -5.0), (20, -5.0)],
        &[(VelocityTrackingStrategy::Impulse, -500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_negativeVelocity_postiveDataPoints_nonDifferentialValues
#[test]
fn falling_positions_above_zero_give_their_rate() {
    check_test_case(
        false,
        &[(0, 20.0), (10, 15.0), (20, 10.0)],
        &[(VelocityTrackingStrategy::Impulse, -500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotion_negativeVelocity_mixedSignDataPoints_nonDifferentialValues
#[test]
fn falling_positions_through_zero_give_their_rate() {
    check_test_case(
        false,
        &[(0, 5.0), (10, 0.0), (20, -5.0)],
        &[(VelocityTrackingStrategy::Impulse, -500.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearHalfMotion
#[test]
fn a_motion_from_rest_half_way_gives_the_rate_it_reached() {
    check_test_case(
        false,
        &[
            (0, 0.0),
            (10, 0.0),
            (20, 0.0),
            (30, 0.0),
            (40, 0.0),
            (50, 0.0),
            (60, 20.0),
            (70, 40.0),
            (80, 60.0),
            (90, 80.0),
            (100, 100.0),
        ],
        &[(VelocityTrackingStrategy::Impulse, 2000.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearHalfMotionSampled
#[test]
fn the_same_motion_sampled_unevenly_gives_nearly_its_rate() {
    check_test_case(
        false,
        &[(0, 0.0), (30, 0.0), (40, 0.0), (70, 40.0), (100, 100.0)],
        &[(VelocityTrackingStrategy::Impulse, 2018.2)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotionFollowedByFlatLine
#[test]
fn a_motion_that_stopped_and_stayed_gives_no_velocity() {
    check_test_case(
        false,
        &[
            (0, 0.0),
            (10, 10.0),
            (20, 20.0),
            (30, 30.0),
            (40, 40.0),
            (50, 50.0),
            (60, 50.0),
            (70, 50.0),
            (80, 50.0),
            (90, 50.0),
            (100, 50.0),
        ],
        &[(VelocityTrackingStrategy::Impulse, 0.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, linearMotionFollowedByFlatLineWithoutIntermediatePoints
#[test]
fn a_motion_with_only_its_last_position_repeated_gives_no_velocity() {
    check_test_case(
        false,
        &[(0, 0.0), (50, 50.0), (100, 50.0)],
        &[(VelocityTrackingStrategy::Impulse, 0.0)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, pixelWatch_rsb_scrollDown_thenUp_thenDown
#[test]
fn a_watch_crowns_reversals_end_with_their_last_direction() {
    check_test_case(
        true,
        &[
            (2580534, -0.0334),
            (2580549, -0.1336),
            (2580566, -0.2505),
            (2580581, -0.1837),
            (2580597, -0.2672),
            (2580613, -0.5511),
            (2580635, -0.6012),
            (2580661, -0.4509),
            (2580683, -0.4175),
            (2580705, -0.1503),
            (2580722, -0.0167),
            (2580786, 0.0501),
            (2580801, 0.1503),
            (2580822, 0.3006),
            (2580838, 0.3006),
            (2580854, 0.4008),
            (2580869, 0.5177),
            (2580886, 0.501),
            (2580905, 0.3507),
            (2580921, 0.3507),
            (2580937, 0.0668),
            (2580974, 0.0167),
            (2581034, -0.0668),
            (2581049, -0.1169),
            (2581070, -0.3173),
            (2581086, -0.2004),
            (2581101, -0.2338),
            (2581118, -0.4175),
            (2581134, -0.4175),
            (2581150, -0.3674),
            (2581166, -0.2672),
            (2581181, -0.1503),
            (2581199, -0.0668),
        ],
        &[(VelocityTrackingStrategy::Impulse, -9.73)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, pixelWatch_rsb_scrollDown
#[test]
fn a_watch_crowns_turn_down_gives_its_velocity() {
    check_test_case(
        true,
        &[
            (224598, -0.0501),
            (224621, -0.1336),
            (224645, -0.5511),
            (224669, -0.8016),
            (224687, -1.0354),
            (224706, -0.4843),
            (224738, -0.334),
            (224754, -0.0835),
        ],
        &[(VelocityTrackingStrategy::Impulse, -27.86)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, pxielWatch_rsb_scrollUp
#[test]
fn a_watch_crowns_turn_up_gives_its_velocity() {
    check_test_case(
        true,
        &[
            (269606, 0.0501),
            (269626, 0.2171),
            (269641, 0.2672),
            (269658, 0.2672),
            (269674, 0.2672),
            (269690, 0.3674),
            (269706, 0.5511),
            (269722, 0.501),
        ],
        &[(VelocityTrackingStrategy::Impulse, 31.92)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, swordfishFlingDown_xValues
#[test]
fn a_recorded_fling_downs_sideways_drift() {
    check_test_case(
        false,
        &[
            (0, 271.0),
            (16, 269.786346),
            (35, 267.983063),
            (52, 262.638397),
            (68, 266.138824),
            (85, 274.79245),
            (96, 274.79245),
        ],
        &[(VelocityTrackingStrategy::Impulse, 623.57)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, swordfishFlingDown_yValues
#[test]
fn a_recorded_fling_downs_motion() {
    check_test_case(
        false,
        &[
            (0, 96.0),
            (16, 106.922775),
            (35, 156.660034),
            (52, 220.339081),
            (68, 331.581116),
            (85, 428.113159),
            (96, 428.113159),
        ],
        &[(VelocityTrackingStrategy::Impulse, 5970.73)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, sailfishFlingUpSlow_xValues
#[test]
fn a_recorded_slow_fling_ups_sideways_drift() {
    check_test_case(
        false,
        &[
            (235089067, 528.0),
            (235089084, 527.0),
            (235089093, 527.0),
            (235089095, 527.0),
            (235089101, 527.0),
            (235089110, 528.0),
            (235089112, 528.25),
            (235089118, 531.0),
            (235089126, 535.0),
            (235089129, 536.33),
            (235089135, 540.0),
            (235089144, 546.0),
            (235089146, 547.21),
            (235089152, 553.0),
            (235089160, 559.0),
            (235089162, 560.66),
        ],
        &[(VelocityTrackingStrategy::Impulse, 764.34), (VelocityTrackingStrategy::Lsq2, 951.69)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, sailfishFlingUpSlow_yValues
#[test]
fn a_recorded_slow_fling_ups_motion() {
    check_test_case(
        false,
        &[
            (235089067, 983.0),
            (235089084, 981.0),
            (235089093, 977.0),
            (235089095, 975.93),
            (235089101, 970.0),
            (235089110, 960.0),
            (235089112, 957.51),
            (235089118, 946.0),
            (235089126, 931.0),
            (235089129, 926.02),
            (235089135, 914.0),
            (235089144, 896.0),
            (235089146, 892.36),
            (235089152, 877.0),
            (235089160, 851.0),
            (235089162, 843.82),
        ],
        &[
            (VelocityTrackingStrategy::Impulse, -3604.82),
            (VelocityTrackingStrategy::Lsq2, -3044.96),
        ],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, sailfishFlingUpFast_xValues
#[test]
fn a_recorded_fast_fling_ups_sideways_drift() {
    check_test_case(
        false,
        &[
            (920922, 561.0),
            (920930, 559.0),
            (920938, 559.0),
            (920947, 562.91),
            (920955, 577.0),
            (920963, 596.87),
            (920972, 631.0),
            (920980, 671.31),
            (920989, 715.0),
        ],
        &[(VelocityTrackingStrategy::Impulse, 5670.32), (VelocityTrackingStrategy::Lsq2, 5991.87)],
        f32::MAX,
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker1DTest.kt, sailfishFlingUpFast_yValues
#[test]
fn a_recorded_fast_fling_ups_motion() {
    check_test_case(
        false,
        &[
            (920922, 1412.0),
            (920930, 1377.0),
            (920938, 1371.0),
            (920947, 1342.68),
            (920955, 1272.0),
            (920963, 1190.54),
            (920972, 1093.0),
            (920980, 994.68),
            (920989, 903.0),
        ],
        &[
            (VelocityTrackingStrategy::Impulse, -13021.1),
            (VelocityTrackingStrategy::Lsq2, -15094.0),
        ],
        f32::MAX,
    );
}
