//! Interpolation (`Interpolatable` and `Brush::lerp`) and non-pixel shader tests.

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use compose_ui::interpolatable::{Interpolant, Interpolatable, Lerped, lerp};
use compose_ui::platform::Gradient;
use compose_ui::{
    BlendMode, Brush, Color, CustomShaderBrush, ImageBitmap, Matrix, Offset, Shader, ShaderBrush,
    Size, TileMode,
};

fn lerped_brush(a: Option<&Brush>, b: Option<&Brush>, t: f32) -> Brush {
    Brush::lerp(a, b, t).expect("brush lerp result")
}

// ---- `InterpolatableTests` (11 tests) ---------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
struct A(f32);

#[derive(Clone, Copy, PartialEq, Debug)]
struct B(f32);

#[derive(Clone, Copy, PartialEq, Debug)]
struct X(f32);

#[derive(Clone, Copy, PartialEq, Debug)]
struct Y(f32);

impl Interpolatable for A {
    fn lerp(&self, other: Option<&dyn Any>, t: f32) -> Option<Box<dyn Any>> {
        let target = match other {
            None => 0.0,
            Some(other) => {
                if let Some(y) = other.downcast_ref::<Y>() {
                    y.0
                } else if let Some(a) = other.downcast_ref::<A>() {
                    a.0
                } else {
                    other.downcast_ref::<B>()?.0
                }
            }
        };
        let value = match other {
            None => (1.0 - t) * 0.0 + t * self.0,
            Some(_) => self.0 + (target - self.0) * t,
        };
        Some(Box::new(A(value)))
    }
}

impl Interpolatable for B {
    fn lerp(&self, other: Option<&dyn Any>, t: f32) -> Option<Box<dyn Any>> {
        match other {
            None => Some(Box::new(B(self.0 * t))),
            Some(_) => None,
        }
    }
}

fn unwrap_a(result: Lerped) -> f32 {
    match result {
        Lerped::Value(v) => v.downcast_ref::<A>().expect("A").0,
        other => panic!("expected Value(A), got {other:?}"),
    }
}

fn unwrap_b(result: Lerped) -> f32 {
    match result {
        Lerped::Value(v) => v.downcast_ref::<B>().expect("B").0,
        other => panic!("expected Value(B), got {other:?}"),
    }
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testBothInterpolatableSameType
#[test]
fn interpolatable_both_same_type() {
    let a1 = A(100.0);
    let a2 = A(200.0);
    let result =
        lerp(Some(Interpolant::Interpolatable(&a1)), Some(Interpolant::Interpolatable(&a2)), 0.5);
    assert_eq!(unwrap_a(result), 150.0);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testLeftInterpolatableRightNot
#[test]
fn interpolatable_left_interpolatable_right_not() {
    let a = A(100.0);
    let x = X(200.0);
    assert!(matches!(
        lerp(Some(Interpolant::Other(&x)), Some(Interpolant::Interpolatable(&a)), 0.51),
        Lerped::Stop
    ));
    assert!(matches!(
        lerp(Some(Interpolant::Other(&x)), Some(Interpolant::Interpolatable(&a)), 0.49),
        Lerped::Start
    ));
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testLeftInterpolatableRightNotButConverted
#[test]
fn interpolatable_left_interpolatable_right_not_but_converted() {
    let a = A(100.0);
    let y = Y(200.0);
    let result = lerp(Some(Interpolant::Interpolatable(&a)), Some(Interpolant::Other(&y)), 0.5);
    assert_eq!(unwrap_a(result), 150.0);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testRightInterpolatableLeftNot
#[test]
fn interpolatable_right_interpolatable_left_not() {
    let x = X(100.0);
    let a = A(200.0);
    assert!(matches!(
        lerp(Some(Interpolant::Other(&x)), Some(Interpolant::Interpolatable(&a)), 0.51),
        Lerped::Stop
    ));
    assert!(matches!(
        lerp(Some(Interpolant::Other(&x)), Some(Interpolant::Interpolatable(&a)), 0.49),
        Lerped::Start
    ));
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testRightInterpolatableLeftNotButConverted
#[test]
fn interpolatable_right_interpolatable_left_not_but_converted() {
    let y = Y(100.0);
    let a = A(200.0);
    let result = lerp(Some(Interpolant::Other(&y)), Some(Interpolant::Interpolatable(&a)), 0.5);
    assert_eq!(unwrap_a(result), 150.0);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testLeftNull
#[test]
fn interpolatable_left_null() {
    let a = A(100.0);
    let result = lerp(None, Some(Interpolant::Interpolatable(&a)), 0.5);
    assert_eq!(unwrap_a(result), 50.0);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testRightNull
#[test]
fn interpolatable_right_null() {
    let a = A(100.0);
    let result = lerp(Some(Interpolant::Interpolatable(&a)), None, 0.5);
    assert_eq!(unwrap_a(result), 50.0);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testDifferentTypes
#[test]
fn interpolatable_different_types() {
    let a = A(100.0);
    let b = B(200.0);
    let result =
        lerp(Some(Interpolant::Interpolatable(&a)), Some(Interpolant::Interpolatable(&b)), 0.5);
    assert_eq!(unwrap_a(result), 150.0);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testBothInterpolatableOnlyOneConverts
#[test]
fn interpolatable_both_only_one_converts() {
    let a = A(100.0);
    let b = B(200.0);
    assert_eq!(
        unwrap_a(lerp(
            Some(Interpolant::Interpolatable(&a)),
            Some(Interpolant::Interpolatable(&b)),
            0.5
        )),
        150.0
    );
    assert_eq!(
        unwrap_a(lerp(
            Some(Interpolant::Interpolatable(&b)),
            Some(Interpolant::Interpolatable(&a)),
            0.5
        )),
        150.0
    );
    let only_b = lerp(Some(Interpolant::Interpolatable(&b)), None, 0.5);
    assert_eq!(unwrap_b(only_b), 100.0);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testNeitherInterpolatable
#[test]
fn interpolatable_neither() {
    let a = X(100.0);
    let b = X(200.0);
    assert!(matches!(
        lerp(Some(Interpolant::Other(&a)), Some(Interpolant::Other(&b)), 0.4),
        Lerped::Start
    ));
    assert!(matches!(
        lerp(Some(Interpolant::Other(&a)), Some(Interpolant::Other(&b)), 0.6),
        Lerped::Stop
    ));
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/InterpolatableTests.kt, testSameObject
#[test]
fn interpolatable_same_object() {
    let a = A(100.0);
    assert!(matches!(
        lerp(Some(Interpolant::Interpolatable(&a)), Some(Interpolant::Interpolatable(&a)), 0.4),
        Lerped::Start
    ));
    assert!(matches!(
        lerp(Some(Interpolant::Interpolatable(&a)), Some(Interpolant::Interpolatable(&a)), 0.6),
        Lerped::Stop
    ));
}

// ---- `BrushInterpolationTests` (31 tests) -----------------------------------------------------

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSolidToSolid
#[test]
fn brush_lerp_solid_to_solid() {
    let a = Brush::solid(Color::RED);
    let b = Brush::solid(Color::BLUE);
    assert_eq!(
        lerped_brush(Some(&a), Some(&b), 0.5),
        Brush::solid(Color::lerp(Color::RED, Color::BLUE, 0.5))
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSolidToNull
#[test]
fn brush_lerp_solid_to_null() {
    let a = Brush::solid(Color::RED);
    assert_eq!(
        lerped_brush(Some(&a), None, 0.5),
        Brush::solid(Color::lerp(Color::RED, Color::TRANSPARENT, 0.5))
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testNullToSolid
#[test]
fn brush_lerp_null_to_solid() {
    let a = Brush::solid(Color::RED);
    assert_eq!(
        lerped_brush(None, Some(&a), 0.5),
        Brush::solid(Color::lerp(Color::TRANSPARENT, Color::RED, 0.5))
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToLinear
#[test]
fn brush_lerp_linear_to_linear() {
    let a = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::linear_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::new(1.0, 1.0),
        Offset::ZERO,
        TileMode::Clamp,
    );
    let result = lerped_brush(Some(&a), Some(&b), 0.5);
    let expected = Brush::linear_gradient_with(
        [Color::lerp(Color::RED, Color::GREEN, 0.5), Color::lerp(Color::BLUE, Color::YELLOW, 0.5)],
        Offset::new(0.5, 0.5),
        Offset::new(0.5, 0.5),
        TileMode::Clamp,
    );
    assert_eq!(result, expected);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testVerticalToHorizontalLinearGradient
#[test]
fn brush_lerp_vertical_to_horizontal_linear_gradient() {
    let a = Brush::vertical_gradient_with([Color::RED, Color::BLUE], 0.0, 100.0, TileMode::Clamp);
    let b =
        Brush::horizontal_gradient_with([Color::GREEN, Color::YELLOW], 0.0, 100.0, TileMode::Clamp);
    let result = lerped_brush(Some(&a), Some(&b), 0.5);
    let expected = Brush::linear_gradient_with(
        [Color::lerp(Color::RED, Color::GREEN, 0.5), Color::lerp(Color::BLUE, Color::YELLOW, 0.5)],
        Offset::ZERO,
        Offset::new(50.0, 50.0),
        TileMode::Clamp,
    );
    assert_eq!(result, expected);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testVerticalToHorizontalWithDefaultValues
#[test]
fn brush_lerp_vertical_to_horizontal_with_default_values() {
    let a = Brush::vertical_gradient([Color::RED, Color::BLUE]);
    let b = Brush::horizontal_gradient([Color::GREEN, Color::YELLOW]);
    assert_eq!(
        lerped_brush(Some(&a), Some(&b), 0.4),
        Brush::linear_gradient_with(
            [
                Color::lerp(Color::RED, Color::GREEN, 0.4),
                Color::lerp(Color::BLUE, Color::YELLOW, 0.4)
            ],
            Offset::ZERO,
            Offset::new(0.0, f32::INFINITY),
            TileMode::Clamp,
        )
    );
    assert_eq!(
        lerped_brush(Some(&a), Some(&b), 0.6),
        Brush::linear_gradient_with(
            [
                Color::lerp(Color::RED, Color::GREEN, 0.6),
                Color::lerp(Color::BLUE, Color::YELLOW, 0.6)
            ],
            Offset::ZERO,
            Offset::new(f32::INFINITY, 0.0),
            TileMode::Clamp,
        )
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToNull
#[test]
fn brush_lerp_linear_to_null() {
    let a = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let expected = Brush::linear_gradient_with(
        [
            Color::lerp(Color::RED, Color::TRANSPARENT, 0.5),
            Color::lerp(Color::BLUE, Color::TRANSPARENT, 0.5),
        ],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    assert_eq!(lerped_brush(Some(&a), None, 0.5), expected);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testNullToLinear
#[test]
fn brush_lerp_null_to_linear() {
    let a = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let expected = Brush::linear_gradient_with(
        [
            Color::lerp(Color::TRANSPARENT, Color::RED, 0.5),
            Color::lerp(Color::TRANSPARENT, Color::BLUE, 0.5),
        ],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    assert_eq!(lerped_brush(None, Some(&a), 0.5), expected);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToLinearWithStops
#[test]
fn brush_lerp_linear_to_linear_with_stops() {
    let a = Brush::linear_gradient_with(
        [(0.0, Color::RED), (1.0, Color::BLUE)],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::linear_gradient_with(
        [(0.2, Color::GREEN), (0.8, Color::YELLOW)],
        Offset::new(1.0, 1.0),
        Offset::ZERO,
        TileMode::Clamp,
    );
    let expected = Brush::linear_gradient_with(
        [
            (0.1, Color::lerp(Color::RED, Color::GREEN, 0.5)),
            (0.9, Color::lerp(Color::BLUE, Color::YELLOW, 0.5)),
        ],
        Offset::new(0.5, 0.5),
        Offset::new(0.5, 0.5),
        TileMode::Clamp,
    );
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.5), expected);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToLinearWithMismatchedStops
#[test]
fn brush_lerp_linear_to_linear_with_mismatched_stops() {
    let a = Brush::linear_gradient_with(
        [(0.0, Color::RED), (1.0, Color::BLUE)],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::linear_gradient_with(
        [(0.2, Color::GREEN), (0.8, Color::YELLOW), (0.9, Color::CYAN)],
        Offset::new(1.0, 1.0),
        Offset::ZERO,
        TileMode::Clamp,
    );
    let expected = Brush::linear_gradient_with(
        [
            (0.1, Color::lerp(Color::RED, Color::GREEN, 0.5)),
            (0.9, Color::lerp(Color::BLUE, Color::YELLOW, 0.5)),
            (0.95, Color::lerp(Color::BLUE, Color::CYAN, 0.5)),
        ],
        Offset::new(0.5, 0.5),
        Offset::new(0.5, 0.5),
        TileMode::Clamp,
    );
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.5), expected);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToLinearWithNullStops
#[test]
fn brush_lerp_linear_to_linear_with_null_stops() {
    let a = Brush::linear_gradient_with(
        [(0.0, Color::RED), (1.0, Color::BLUE)],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::linear_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::new(1.0, 1.0),
        Offset::ZERO,
        TileMode::Clamp,
    );
    let Brush::Linear { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected linear gradient");
    };
    assert_eq!(gradient.stops, None);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToLinearWithTileMode
#[test]
fn brush_lerp_linear_to_linear_with_tile_mode() {
    let a = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::linear_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::new(1.0, 1.0),
        Offset::ZERO,
        TileMode::Repeated,
    );
    let Brush::Linear { gradient: g1, .. } = lerped_brush(Some(&a), Some(&b), 0.4) else {
        panic!("expected linear");
    };
    assert_eq!(g1.tile_mode, TileMode::Clamp);
    let Brush::Linear { gradient: g2, .. } = lerped_brush(Some(&a), Some(&b), 0.6) else {
        panic!("expected linear");
    };
    assert_eq!(g2.tile_mode, TileMode::Repeated);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSolidToLinear
#[test]
fn brush_lerp_solid_to_linear() {
    let a = Brush::solid(Color::RED);
    let b = Brush::linear_gradient_with(
        [Color::BLUE, Color::GREEN],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let Brush::Linear { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected linear");
    };
    assert_eq!(
        &*gradient.colors,
        &[Color::lerp(Color::RED, Color::BLUE, 0.5), Color::lerp(Color::RED, Color::GREEN, 0.5)]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToSolid
#[test]
fn brush_lerp_linear_to_solid() {
    let a = Brush::linear_gradient_with(
        [Color::BLUE, Color::GREEN],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::solid(Color::RED);
    let Brush::Linear { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected linear");
    };
    assert_eq!(
        &*gradient.colors,
        &[Color::lerp(Color::BLUE, Color::RED, 0.5), Color::lerp(Color::GREEN, Color::RED, 0.5)]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testRadialToRadial
#[test]
fn brush_lerp_radial_to_radial() {
    let a =
        Brush::radial_gradient_with([Color::RED, Color::BLUE], Offset::ZERO, 1.0, TileMode::Clamp);
    let b = Brush::radial_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::new(1.0, 1.0),
        2.0,
        TileMode::Clamp,
    );
    assert_eq!(
        lerped_brush(Some(&a), Some(&b), 0.5),
        Brush::radial_gradient_with(
            [
                Color::lerp(Color::RED, Color::GREEN, 0.5),
                Color::lerp(Color::BLUE, Color::YELLOW, 0.5)
            ],
            Offset::new(0.5, 0.5),
            1.5,
            TileMode::Clamp,
        )
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testRadialToNull
#[test]
fn brush_lerp_radial_to_null() {
    let a =
        Brush::radial_gradient_with([Color::RED, Color::BLUE], Offset::ZERO, 1.0, TileMode::Clamp);
    assert_eq!(
        lerped_brush(Some(&a), None, 0.5),
        Brush::radial_gradient_with(
            [
                Color::lerp(Color::RED, Color::TRANSPARENT, 0.5),
                Color::lerp(Color::BLUE, Color::TRANSPARENT, 0.5)
            ],
            Offset::ZERO,
            1.0,
            TileMode::Clamp,
        )
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testNullToRadial
#[test]
fn brush_lerp_null_to_radial() {
    let a =
        Brush::radial_gradient_with([Color::RED, Color::BLUE], Offset::ZERO, 1.0, TileMode::Clamp);
    assert_eq!(
        lerped_brush(None, Some(&a), 0.5),
        Brush::radial_gradient_with(
            [
                Color::lerp(Color::TRANSPARENT, Color::RED, 0.5),
                Color::lerp(Color::TRANSPARENT, Color::BLUE, 0.5)
            ],
            Offset::ZERO,
            1.0,
            TileMode::Clamp,
        )
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testRadialToRadialWithTileMode
#[test]
fn brush_lerp_radial_to_radial_with_tile_mode() {
    let a =
        Brush::radial_gradient_with([Color::RED, Color::BLUE], Offset::ZERO, 1.0, TileMode::Mirror);
    let b = Brush::radial_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::new(1.0, 1.0),
        2.0,
        TileMode::Decal,
    );
    let Brush::Radial { gradient: g1, .. } = lerped_brush(Some(&a), Some(&b), 0.4) else {
        panic!("expected radial");
    };
    assert_eq!(g1.tile_mode, TileMode::Mirror);
    let Brush::Radial { gradient: g2, .. } = lerped_brush(Some(&a), Some(&b), 0.6) else {
        panic!("expected radial");
    };
    assert_eq!(g2.tile_mode, TileMode::Decal);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testRadialToRadialWithStops
#[test]
fn brush_lerp_radial_to_radial_with_stops() {
    let a = Brush::radial_gradient_with(
        [(0.0, Color::RED), (1.0, Color::BLUE)],
        Offset::ZERO,
        1.0,
        TileMode::Clamp,
    );
    let b = Brush::radial_gradient_with(
        [(0.2, Color::GREEN), (0.8, Color::YELLOW)],
        Offset::new(1.0, 1.0),
        2.0,
        TileMode::Clamp,
    );
    assert_eq!(
        lerped_brush(Some(&a), Some(&b), 0.5),
        Brush::radial_gradient_with(
            [
                (0.1, Color::lerp(Color::RED, Color::GREEN, 0.5)),
                (0.9, Color::lerp(Color::BLUE, Color::YELLOW, 0.5)),
            ],
            Offset::new(0.5, 0.5),
            1.5,
            TileMode::Clamp,
        )
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testRadialToRadialWithMismatchedStops
#[test]
fn brush_lerp_radial_to_radial_with_mismatched_stops() {
    let a = Brush::radial_gradient_with(
        [(0.0, Color::RED), (1.0, Color::BLUE)],
        Offset::ZERO,
        1.0,
        TileMode::Clamp,
    );
    let b = Brush::radial_gradient_with(
        [(0.2, Color::GREEN), (0.8, Color::YELLOW), (0.9, Color::CYAN)],
        Offset::new(1.0, 1.0),
        2.0,
        TileMode::Clamp,
    );
    assert_eq!(
        lerped_brush(Some(&a), Some(&b), 0.5),
        Brush::radial_gradient_with(
            [
                (0.1, Color::lerp(Color::RED, Color::GREEN, 0.5)),
                (0.9, Color::lerp(Color::BLUE, Color::YELLOW, 0.5)),
                (0.95, Color::lerp(Color::BLUE, Color::CYAN, 0.5)),
            ],
            Offset::new(0.5, 0.5),
            1.5,
            TileMode::Clamp,
        )
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testRadialToRadialWithNullStops
#[test]
fn brush_lerp_radial_to_radial_with_null_stops() {
    let a = Brush::radial_gradient_with(
        [(0.0, Color::RED), (1.0, Color::BLUE)],
        Offset::ZERO,
        1.0,
        TileMode::Clamp,
    );
    let b = Brush::radial_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::new(1.0, 1.0),
        2.0,
        TileMode::Clamp,
    );
    let Brush::Radial { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected radial");
    };
    assert_eq!(gradient.stops, None);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSolidToRadial
#[test]
fn brush_lerp_solid_to_radial() {
    let a = Brush::solid(Color::RED);
    let b = Brush::radial_gradient_with(
        [Color::BLUE, Color::GREEN],
        Offset::ZERO,
        1.0,
        TileMode::Clamp,
    );
    let Brush::Radial { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected radial");
    };
    assert_eq!(
        &*gradient.colors,
        &[Color::lerp(Color::RED, Color::BLUE, 0.5), Color::lerp(Color::RED, Color::GREEN, 0.5)]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testRadialToSolid
#[test]
fn brush_lerp_radial_to_solid() {
    let a = Brush::radial_gradient_with(
        [Color::BLUE, Color::GREEN],
        Offset::ZERO,
        1.0,
        TileMode::Clamp,
    );
    let b = Brush::solid(Color::RED);
    let Brush::Radial { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected radial");
    };
    assert_eq!(
        &*gradient.colors,
        &[Color::lerp(Color::BLUE, Color::RED, 0.5), Color::lerp(Color::GREEN, Color::RED, 0.5)]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSolidToSweep
#[test]
fn brush_lerp_solid_to_sweep() {
    let a = Brush::solid(Color::RED);
    let b = Brush::sweep_gradient_with([Color::BLUE, Color::GREEN], Offset::ZERO);
    let Brush::Sweep { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected sweep");
    };
    assert_eq!(
        &*gradient.colors,
        &[Color::lerp(Color::RED, Color::BLUE, 0.5), Color::lerp(Color::RED, Color::GREEN, 0.5)]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSweepToSolid
#[test]
fn brush_lerp_sweep_to_solid() {
    let a = Brush::sweep_gradient_with([Color::BLUE, Color::GREEN], Offset::ZERO);
    let b = Brush::solid(Color::RED);
    let Brush::Sweep { gradient, .. } = lerped_brush(Some(&a), Some(&b), 0.5) else {
        panic!("expected sweep");
    };
    assert_eq!(
        &*gradient.colors,
        &[Color::lerp(Color::BLUE, Color::RED, 0.5), Color::lerp(Color::GREEN, Color::RED, 0.5)]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSweepToNull
#[test]
fn brush_lerp_sweep_to_null() {
    let a = Brush::sweep_gradient_with([Color::RED, Color::BLUE], Offset::ZERO);
    let Brush::Sweep { gradient, .. } = lerped_brush(Some(&a), None, 0.5) else {
        panic!("expected sweep");
    };
    assert_eq!(
        &*gradient.colors,
        &[
            Color::lerp(Color::RED, Color::TRANSPARENT, 0.5),
            Color::lerp(Color::BLUE, Color::TRANSPARENT, 0.5)
        ]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testNullToSweep
#[test]
fn brush_lerp_null_to_sweep() {
    let a = Brush::sweep_gradient_with([Color::RED, Color::BLUE], Offset::ZERO);
    let Brush::Sweep { gradient, .. } = lerped_brush(None, Some(&a), 0.5) else {
        panic!("expected sweep");
    };
    assert_eq!(
        &*gradient.colors,
        &[
            Color::lerp(Color::TRANSPARENT, Color::RED, 0.5),
            Color::lerp(Color::TRANSPARENT, Color::BLUE, 0.5)
        ]
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSweepToSweep
#[test]
fn brush_lerp_sweep_to_sweep() {
    let a = Brush::sweep_gradient_with([Color::RED, Color::BLUE], Offset::ZERO);
    let b = Brush::sweep_gradient_with([Color::GREEN, Color::YELLOW], Offset::new(1.0, 1.0));
    assert_eq!(
        lerped_brush(Some(&a), Some(&b), 0.5),
        Brush::sweep_gradient_with(
            [
                Color::lerp(Color::RED, Color::GREEN, 0.5),
                Color::lerp(Color::BLUE, Color::YELLOW, 0.5)
            ],
            Offset::new(0.5, 0.5),
        )
    );
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToSweep
#[test]
fn brush_lerp_linear_to_sweep() {
    let a = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::sweep_gradient_with([Color::GREEN, Color::YELLOW], Offset::ZERO);
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.4), a);
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.6), b);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testSweepToRadial
#[test]
fn brush_lerp_sweep_to_radial() {
    let a = Brush::sweep_gradient_with([Color::RED, Color::BLUE], Offset::ZERO);
    let b = Brush::radial_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::ZERO,
        1.0,
        TileMode::Clamp,
    );
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.4), a);
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.6), b);
}

// Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/BrushInterpolationTests.kt, testLinearToRadial
#[test]
fn brush_lerp_linear_to_radial() {
    let a = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::ZERO,
        Offset::new(1.0, 1.0),
        TileMode::Clamp,
    );
    let b = Brush::radial_gradient_with(
        [Color::GREEN, Color::YELLOW],
        Offset::ZERO,
        1.0,
        TileMode::Clamp,
    );
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.4), a);
    assert_eq!(lerped_brush(Some(&a), Some(&b), 0.6), b);
}

// ---- `ShaderTest` non-pixel unit tests (11 tests) ---------------------------------------------

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testLinearGradientIntrinsicSize
#[test]
fn linear_gradient_intrinsic_size() {
    let brush = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::new(200.0, 100.0),
        Offset::new(300.0, 300.0),
        TileMode::Clamp,
    );
    assert_eq!(brush.intrinsic_size(), Size::new(100.0, 200.0));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testLinearGradientNegativePosition
#[test]
fn linear_gradient_negative_position() {
    let brush = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::new(200.0, 100.0),
        Offset::new(100.0, -100.0),
        TileMode::Clamp,
    );
    assert_eq!(brush.intrinsic_size(), Size::new(100.0, 200.0));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testLinearGradientInfiniteWidth
#[test]
fn linear_gradient_infinite_width() {
    let brush = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::new(f32::INFINITY, 100.0),
        Offset::new(f32::INFINITY, 300.0),
        TileMode::Clamp,
    );
    let size = brush.intrinsic_size();
    assert!(size.width.is_nan() && size.height == 200.0, "{size:?}");
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testLinearGradientInfiniteHeight
#[test]
fn linear_gradient_infinite_height() {
    let brush = Brush::linear_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::new(100.0, 0.0),
        Offset::new(200.0, f32::INFINITY),
        TileMode::Clamp,
    );
    let size = brush.intrinsic_size();
    assert!(size.width == 100.0 && size.height.is_nan(), "{size:?}");
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testSweepGradientIntrinsicSize
#[test]
fn sweep_gradient_intrinsic_size() {
    assert!(Brush::sweep_gradient([Color::RED, Color::BLUE]).intrinsic_size().is_unspecified());
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testRadialGradientIntrinsicSize
#[test]
fn radial_gradient_intrinsic_size() {
    let brush = Brush::radial_gradient_with(
        [Color::RED, Color::BLUE],
        Offset::UNSPECIFIED,
        50.0,
        TileMode::Clamp,
    );
    assert_eq!(brush.intrinsic_size(), Size::new(100.0, 100.0));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testRadialGradientInfiniteSize
#[test]
fn radial_gradient_infinite_size() {
    assert!(Brush::radial_gradient([Color::RED, Color::BLUE]).intrinsic_size().is_unspecified());
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testInvalidWidthBrush, testInvalidHeightBrush, testValidToInvalidWidthBrush, testValidToInvalidHeightBrush
#[test]
fn radial_gradient_empty_size_drops_shader() {
    let brush = Brush::radial_gradient([Color::RED, Color::BLUE]);
    assert!(brush.shader(Size::new(0.0, 10.0), Offset::ZERO, 1.0).is_none());
    assert!(brush.shader(Size::new(10.0, 0.0), Offset::ZERO, 1.0).is_none());
    assert!(brush.shader(Size::new(10.0, 10.0), Offset::ZERO, 1.0).is_some());
    assert!(brush.shader(Size::new(0.0, 10.0), Offset::ZERO, 1.0).is_none());
    assert!(brush.shader(Size::new(10.0, 0.0), Offset::ZERO, 1.0).is_none());
}

#[derive(Clone)]
struct CountingBrush(Arc<AtomicUsize>, Size);

impl CustomShaderBrush for CountingBrush {
    fn create_shader(&self, _size: Size) -> Shader {
        self.0.fetch_add(1, Ordering::Relaxed);
        Shader::linear_gradient(
            Offset::ZERO,
            Offset::new(10.0, 0.0),
            [Color::RED, Color::BLUE],
            TileMode::Clamp,
        )
    }

    fn intrinsic_size(&self) -> Size {
        self.1
    }

    fn same(&self, other: &dyn CustomShaderBrush) -> bool {
        other.downcast_ref::<CountingBrush>().is_some_and(|other| Arc::ptr_eq(&self.0, &other.0))
    }
}

#[test]
fn shader_brush_caches_by_size_and_supports_transform_and_composite() {
    let calls = Arc::new(AtomicUsize::new(0));
    let brush = ShaderBrush::custom(CountingBrush(calls.clone(), Size::new(12.0, 34.0)));
    assert_eq!(brush.intrinsic_size(), Size::new(12.0, 34.0));
    let composite =
        Brush::composite(brush.clone().into(), Brush::solid(Color::GREEN), BlendMode::SrcIn);
    assert!(composite.intrinsic_size().is_unspecified());
    assert!(composite.shader(Size::new(10.0, 10.0), Offset::ZERO, 1.0).is_some());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    // Same size reuses cached shader inside `paint_shader`:
    let _ = composite.shader(Size::new(10.0, 10.0), Offset::ZERO, 1.0);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    // Different size creates anew:
    let _ = composite.shader(Size::new(20.0, 10.0), Offset::ZERO, 1.0);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    // Local matrix is carried onto PaintShader:
    let img = ImageBitmap::from_rgba_premultiplied(1, 1, vec![255, 0, 0, 255]);
    let img_brush = ShaderBrush::new(Shader::image(&img, TileMode::Repeated, TileMode::Mirror));
    img_brush.set_transform(Some(Matrix::translation(3.0, 4.0)));
    let painted =
        Brush::from(img_brush).shader(Size::new(10.0, 10.0), Offset::new(1.0, 2.0), 1.0).unwrap();
    assert_eq!(
        Gradient {
            colors: Arc::from([Color::RED, Color::BLUE]),
            stops: None,
            tile_mode: TileMode::Clamp
        }
        .colors
        .len(),
        2
    );
    assert_eq!(painted.transform.as_deref(), Some(&Matrix::translation(4.0, 6.0)));
}
