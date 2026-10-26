//! The value types' conversions to and from animation vectors, as Compose's `VectorConverter`s:
//! sizes and orders of components, rounding, clamping, colors in Oklab.

use compose_animation::*;
use compose_ui::{Color, Dp, DpOffset, IntOffset, IntSize, Offset, Rect, Size};

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/TypeConverterTest.kt, testFloatToVectorConverter
#[test]
fn a_float_is_a_vector_of_one() {
    assert_eq!(15.0f32.to_vector(), AnimationVector::from(15.0));
    assert_eq!(f32::from_vector(AnimationVector::from(5.0)), 5.0);
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/ConverterTest.kt, testColorConverter
#[test]
fn colors_go_to_oklab_and_back() {
    for color in [Color::RED, Color::GREEN, Color::BLUE] {
        assert_eq!(Color::from_vector(color.to_vector()), color);
    }
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/ConverterTest.kt, testColorConverterClampValuesOutOfRange
#[test]
fn a_color_vector_is_clamped_to_oklab_first() {
    // Alpha over 1 or under 0: clamped.
    assert_eq!(Color::from_vector(AnimationVector::from([1.1, 1.0, 0.0, 0.0])).alpha(), 255);
    assert_eq!(Color::from_vector(AnimationVector::from([-0.1, 1.0, 0.0, 0.0])).alpha(), 0);
    // L, a and b clamped to Oklab's range (0..1, ±0.5) before converting: 3, 3, 3 is the
    // color of 1, 0.5, 0.5, and -3, -3, -3 that of 0, -0.5, -0.5.
    let most = Color::from_vector(AnimationVector::from([1.0, 1.0, 0.5, 0.5]));
    assert_eq!(Color::from_vector(AnimationVector::from([1.0, 3.0, 3.0, 3.0])), most);
    let least = Color::from_vector(AnimationVector::from([1.0, 0.0, -0.5, -0.5]));
    assert_eq!(Color::from_vector(AnimationVector::from([1.0, -3.0, -3.0, -3.0])), least);
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/ConverterTest.kt, testRectConverter
#[test]
fn a_rect_is_a_vector_of_its_sides() {
    assert_eq!(
        Rect::new(1.0, 2.0, 3.0, 4.0).to_vector(),
        AnimationVector::from([1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(
        Rect::from_vector(AnimationVector::from([-400.0, -300.0, -200.0, -100.0])),
        Rect::new(-400.0, -300.0, -200.0, -100.0)
    );
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/ConverterTest.kt, testDpConverter
#[test]
fn a_dp_is_a_vector_of_one() {
    assert_eq!(Dp::from_vector(AnimationVector::from(0.37)), Dp(0.37));
    assert_eq!(Dp(0.61).to_vector(), AnimationVector::from(0.61));
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/ConverterTest.kt, testOffsetConverter
#[test]
fn an_offset_is_a_vector_of_two() {
    assert_eq!(Offset::from_vector(AnimationVector::from([0.3, 0.8])), Offset::new(0.3, 0.8));
    assert_eq!(Offset::new(0.3, 0.8).to_vector(), AnimationVector::from([0.3, 0.8]));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, testIntSize_alwaysWithinValidBounds
#[test]
fn an_int_size_is_never_negative() {
    assert_eq!(IntSize::from_vector(AnimationVector::from([-3.0, -0.4])), IntSize::new(0, 0));
    // Rounded half up, as Kotlin's `roundToInt`.
    assert_eq!(IntSize::from_vector(AnimationVector::from([2.5, 2.49])), IntSize::new(3, 2));
    assert_eq!(IntOffset::from_vector(AnimationVector::from([-2.5, 2.5])), IntOffset::new(-2, 3));
}

#[test]
fn the_other_types_have_their_sizes() {
    assert_eq!(Size::new(1.0, 2.0).to_vector().size(), 2);
    assert_eq!(DpOffset::new(Dp(1.0), Dp(2.0)).to_vector(), AnimationVector::from([1.0, 2.0]));
    assert_eq!(Color::RED.to_vector().size(), 4);
}
