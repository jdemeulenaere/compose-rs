//! A path as an easing curve, against Compose's `PathEasingTest`.

use compose_animation::*;
use compose_ui::{Path, PathOperation, Rect};

fn assert_close(expected: f32, actual: f32, within: f32) {
    assert!((expected - actual).abs() <= within, "{actual}, not {expected}");
}

fn path(build: impl FnOnce(&Path)) -> Path {
    let path = Path::new();
    build(&path);
    path
}

/// Checks the curve only ever goes up, from 0 on.
fn assert_rises(easing: &Easing) {
    let mut previous = -f32::MAX;
    for i in 0..=256 {
        let eased = easing.transform(i as f32 / 256.0);
        assert!(eased >= 0.0, "{eased} at {i}");
        assert!(eased > previous, "{eased} after {previous} at {i}");
        previous = eased;
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_Emphasized_BoundsCheck
#[test]
fn two_curves_ease_as_the_curve_they_make() {
    let easing = Easing::path(&path(|path| {
        path.move_to(0.0, 0.0);
        path.cubic_to(0.05, 0.0, 0.133333, 0.06, 0.166666, 0.4);
        path.cubic_to(0.208333, 0.82, 0.25, 1.0, 1.0, 1.0);
    }));
    assert_eq!(easing.transform(0.0), 0.0);
    assert_eq!(easing.transform(1.0), 1.0);
    assert_close(0.77283, easing.transform(0.25), 1e-4);
    assert_close(0.95061, easing.transform(0.50), 1e-4);
    assert_close(0.99139, easing.transform(0.75), 1e-4);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_EmptyPath_InvalidPath
#[test]
#[should_panic(expected = "must start at 0.0f and end at 1.0f")]
fn an_empty_path_isn_t_a_curve() {
    Easing::path(&Path::new()).transform(0.5);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_DoesNotStartAtZero
#[test]
#[should_panic(expected = "must start at 0.0f and end at 1.0f")]
fn a_curve_starts_at_zero() {
    let easing = Easing::path(&path(|path| {
        path.move_to(0.1, 0.0);
        path.line_to(1.0, 1.0);
    }));
    easing.transform(0.5);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_DoesNotEndAtOne
#[test]
#[should_panic(expected = "must start at 0.0f and end at 1.0f")]
fn a_curve_ends_at_one() {
    Easing::path(&path(|path| path.line_to(0.9, 1.0))).transform(0.5);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_CompareToCubicEasing
#[test]
fn a_cubic_path_is_the_cubic_easing() {
    let cubic = Easing::cubic_bezier(0.4, 0.0, 0.2, 1.0);
    let easing = Easing::path(&path(|path| path.cubic_to(0.4, 0.0, 0.2, 1.0, 1.0, 1.0)));
    for i in 0..=256 {
        let fraction = i as f32 / 256.0;
        assert_close(cubic.transform(fraction), easing.transform(fraction), 1e-6);
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_NonContinuousPath
#[test]
#[should_panic(expected = "continuous on the x axis")]
fn a_gap_along_x_panics_only_when_it_s_reached() {
    let easing = Easing::path(&path(|path| {
        path.move_to(0.00, 0.10);
        path.line_to(0.25, 0.10);
        path.move_to(0.50, 0.40);
        path.line_to(0.75, 0.40);
        path.move_to(0.75, 1.00);
        path.line_to(1.00, 1.00);
    }));
    assert_eq!(easing.transform(0.2), 0.1);
    easing.transform(0.4);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_ClosedPath
#[test]
#[should_panic(expected = "cannot contain a close() command")]
fn a_closed_path_isn_t_a_curve() {
    Easing::path(&path(|path| path.add_oval(Rect::new(0.0, 0.0, 1.0, 1.0)))).transform(0.5);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_Overlapping_Curves
#[test]
fn curves_overlapping_along_x_give_one_of_their_values() {
    let easing = Easing::path(&path(|path| {
        path.move_to(0.00, 0.10);
        path.line_to(0.25, 0.10);
        path.move_to(0.10, 0.30);
        path.line_to(0.60, 0.30);
        path.move_to(0.50, 0.40);
        path.line_to(0.75, 0.40);
        path.move_to(0.75, 1.00);
        path.line_to(1.00, 1.00);
    }));
    let eased = easing.transform(0.2);
    assert!(eased == 0.10 || eased == 0.30, "{eased}");
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_QuadTo
#[test]
fn a_quadratic_path_rises() {
    assert_rises(&Easing::path(&path(|path| path.quadratic_to(1.0, 0.0, 1.0, 1.0))));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_QuadTo_OneToZero
#[test]
fn a_quadratic_path_drawn_backwards_rises() {
    assert_rises(&Easing::path(&path(|path| {
        path.move_to(1.0, 1.0);
        path.quadratic_to(1.0, 0.0, 0.0, 0.0);
    })));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_Overshoots
#[test]
fn a_curve_can_overshoot() {
    let easing = Easing::path(&path(|path| path.cubic_to(0.34, 1.56, 0.64, 1.0, 1.0, 1.0)));
    assert!(easing.transform(0.6) > 1.0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/PathEasingTest.kt, pathEasing_Undershoots
#[test]
fn a_curve_can_undershoot() {
    let easing = Easing::path(&path(|path| path.cubic_to(0.68, -0.6, 0.32, 1.6, 1.0, 1.0)));
    assert!(easing.transform(0.1) < 0.0);
}

/// A curve's range along x is where it goes, not only where it ends: there's no gap in this one.
#[test]
fn a_curve_bulging_along_x_covers_its_bulge() {
    let easing = Easing::path(&path(|path| {
        path.cubic_to(0.9, 0.1, 0.9, 0.3, 0.4, 0.4);
        path.move_to(0.6, 0.6);
        path.line_to(1.0, 1.0);
    }));
    let eased = easing.transform(0.5);
    assert!(eased > 0.0 && eased < 0.4, "{eased}");
}

/// A line's y is between its ends'.
#[test]
fn a_line_eases_along_its_ys() {
    let easing = Easing::path(&path(|path| {
        path.move_to(0.0, 0.2);
        path.line_to(1.0, 0.6);
    }));
    assert_close(0.4, easing.transform(0.5), 1e-6);
    assert_close(0.3, easing.transform(0.25), 1e-6);
}

/// Where only one curve starts, or only one ends, it's that one's.
#[test]
fn a_segment_covers_its_ends() {
    let easing = Easing::path(&path(|path| {
        path.line_to(0.25, 0.1);
        path.move_to(0.5, 0.7);
        path.line_to(1.0, 1.0);
    }));
    assert_close(0.1, easing.transform(0.25), 1e-6);
    assert_close(0.7, easing.transform(0.5), 1e-6);
}

/// Of curves overlapping, the first drawn: Compose's tree says nothing of which, and a list does.
#[test]
fn of_overlapping_curves_the_first_is_taken() {
    let easing = Easing::path(&path(|path| {
        path.line_to(0.5, 0.1);
        path.move_to(0.25, 0.9);
        path.line_to(1.0, 0.9);
    }));
    assert_close(0.08, easing.transform(0.4), 1e-6);
}

/// A result of `Path::op` has no segments here, which only a platform's operator computes: refused
/// where it's written rather than in the middle of an animation.
#[test]
#[should_panic(expected = "result of a path operation")]
fn a_combined_path_is_refused_when_it_s_made_an_easing() {
    let (first, second) =
        (path(|path| path.line_to(1.0, 1.0)), path(|path| path.line_to(1.0, 0.5)));
    Easing::path(&Path::combine(PathOperation::Union, &first, &second));
}
