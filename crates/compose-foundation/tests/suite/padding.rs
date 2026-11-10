//! Compose's `PaddingTest`: `padding`'s overloads applied to a child, with too little room and
//! right to left, `absolutePadding` and `PaddingValues.Absolute`, and the negative padding checks,
//! through the `LayoutTest` helpers of [`layout_test`](crate::layout_test). Its intrinsics test
//! is in `intrinsics.rs`; `PaddingValuesTest` at the end. Its inspector tests are n/a.

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_ui::{Alignment, Dp, LOCAL_LAYOUT_DIRECTION, Layout, LayoutDirection, measure_policy};

use crate::layout_test::*;

/// As `PaddingTest.TestBox`: its one child measured with its own constraints.
fn test_box(modifier: Modifier, content: impl Fn() + 'static) {
    let policy = measure_policy(|scope, measurables, constraints| {
        assert_eq!(measurables.len(), 1, "TestBox must have exactly one child");
        let placeable = scope.measure(measurables[0], constraints);
        scope.place_relative(&placeable, 0, 0);
        scope.layout(
            placeable.width.min(constraints.max_width),
            placeable.height.min(constraints.max_height),
        )
    });
    Layout(modifier, policy, callback(content));
}

/// A container in a `TestBox` of `padding`, in a 50 dp `ConstrainedBox` centered in the root:
/// the container's size and position.
fn padded(padding: Modifier) -> Probe {
    let probe = Probe::new(1);
    let p = probe.clone();
    let _ui = show(move || {
        let (p, padding) = (p.clone(), padding.clone());
        Box!(modifier = Modifier.fill_max_size(), |scope| {
            let (p, padding) = (p.clone(), padding.clone());
            let fixed = DpConstraints::fixed(Dp(50.0), Dp(50.0));
            constrained_box(fixed, Modifier.align(scope, Alignment::CENTER), move || {
                let p = p.clone();
                test_box(padding.clone(), move || empty(p.at(0), Spec::default()));
            });
        });
    });
    probe
}

/// As `testPaddingWithDifferentInsetsImplementation`: the child of `padding` inset by `left`,
/// `top`, `right` and `bottom`.
fn assert_insets(padding: Modifier, left: Dp, top: Dp, right: Dp, bottom: Dp) {
    let probe = padded(padding);
    let size = round(Dp(50.0));
    let (left, top, right, bottom) = (round(left), round(top), round(right), round(bottom));
    assert_eq!(probe.size(0), self::size(size - left - right, size - top - bottom));
    let x = ((ROOT.width - size) as f32 / 2.0).round() as i32 + left;
    let y = ((ROOT.height - size) as f32 / 2.0).round() as i32 + top;
    assert_eq!(probe.position(0), offset(x, y));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, negativeStartPadding_throws
#[test]
#[should_panic(expected = "Padding must be non-negative")]
fn negative_start_padding_panics() {
    let _ = Modifier.padding(PaddingValues { start: Dp(-1.0), ..PaddingValues::ZERO });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, negativeTopPadding_throws
#[test]
#[should_panic(expected = "Padding must be non-negative")]
fn negative_top_padding_panics() {
    let _ = Modifier.padding(PaddingValues { top: Dp(-1.0), ..PaddingValues::ZERO });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, negativeEndPadding_throws
#[test]
#[should_panic(expected = "Padding must be non-negative")]
fn negative_end_padding_panics() {
    let _ = Modifier.padding(PaddingValues { end: Dp(-1.0), ..PaddingValues::ZERO });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, negativeBottomPadding_throws
#[test]
#[should_panic(expected = "Padding must be non-negative")]
fn negative_bottom_padding_panics() {
    let _ = Modifier.padding(PaddingValues { bottom: Dp(-1.0), ..PaddingValues::ZERO });
}

/// Compose's constructors check too (`PaddingValuesImpl`, `PaddingValues.Absolute`,
/// `absolutePadding`'s element); an unspecified side is no padding, as `padding(start, …)`'s.
#[test]
fn every_padding_constructor_refuses_a_negative_side() {
    let negative = Dp(-1.0);
    let panics = |make: fn(Dp)| std::panic::catch_unwind(move || make(negative)).is_err();
    assert!(panics(|side| {
        let _ = PaddingValues::new(side, Dp(0.0), Dp(0.0), Dp(0.0));
    }));
    assert!(panics(|side| {
        let _ = PaddingValues::all(side);
    }));
    assert!(panics(|side| {
        let _ = PaddingValues::absolute(Dp(0.0), Dp(0.0), side, Dp(0.0));
    }));
    assert!(panics(|side| drop(Modifier.absolute_padding(side, Dp(0.0), Dp(0.0), Dp(0.0)))));
    assert!(panics(|side| {
        drop(Modifier.padding(PaddingValues { left: side, ..PaddingValues::ZERO }))
    }));
    let unspecified = PaddingValues { start: Dp::UNSPECIFIED, ..PaddingValues::ZERO };
    let _ = Modifier.padding(unspecified);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, allEqualToAbsoluteWithExplicitSides
#[test]
fn all_equal_to_absolute_with_explicit_sides() {
    let ten = Dp(10.0);
    assert!(Modifier.padding((ten, ten, ten, ten)).same(&Modifier.padding(ten)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, symmetricEqualToAbsoluteWithExplicitSides
#[test]
fn symmetric_equal_to_absolute_with_explicit_sides() {
    let (ten, twenty) = (Dp(10.0), Dp(20.0));
    assert!(Modifier.padding((ten, twenty, ten, twenty)).same(&Modifier.padding((ten, twenty))));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, paddingAllAppliedToChild
#[test]
fn padding_all_applied_to_child() {
    let padding = Dp(10.0);
    assert_insets(Modifier.padding(padding), padding, padding, padding, padding);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, paddingPaddingValuesAppliedToChild
#[test]
fn padding_padding_values_applied_to_child() {
    let (start, top, end, bottom) = (Dp(1.0), Dp(3.0), Dp(6.0), Dp(10.0));
    let padding = PaddingValues { start, top, end, bottom, ..PaddingValues::ZERO };
    assert_insets(Modifier.padding(padding), start, top, end, bottom);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, absolutePaddingAppliedToChild
#[test]
fn absolute_padding_applied_to_child() {
    let (left, top, right, bottom) = (Dp(10.0), Dp(15.0), Dp(20.0), Dp(30.0));
    let padding = Modifier.absolute_padding(left, top, right, bottom);
    assert_insets(padding, left, top, right, bottom);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, paddingAbsolutePaddingValuesAppliedToChild
#[test]
fn padding_absolute_padding_values_applied_to_child() {
    let padding = PaddingValues::absolute(Dp(1.0), Dp(3.0), Dp(6.0), Dp(10.0));
    assert_insets(Modifier.padding(padding), Dp(1.0), Dp(3.0), Dp(6.0), Dp(10.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, insufficientSpaceAvailable
#[test]
fn insufficient_space_available() {
    let padding = Dp(30.0);
    let probe = padded(Modifier.padding(padding));
    let (size, padding) = (round(Dp(50.0)), round(padding));
    assert_eq!(probe.size(0), self::size(0, 0));
    let x = ((ROOT.width - size) as f32 / 2.0).round() as i32 + padding;
    let y = ((ROOT.height - size) as f32 / 2.0).round() as i32 + padding;
    assert_eq!(probe.position(0), offset(x, y));
}

fn rtl(content: impl Fn() + 'static) {
    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), content);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, testPadding_rtl
#[test]
fn padding_rtl() {
    let size_dp = px(100.0);
    let size = round(size_dp);
    let (padding1_dp, padding2_dp, padding3_dp) = (Dp(5.0), Dp(10.0), Dp(15.0));
    let (padding1, padding2, padding3) =
        (round(padding1_dp), round(padding2_dp), round(padding3_dp));
    let probe = Probe::new(3);
    let p = probe.clone();
    // Left to right: P1 S P2 | S P3 | P1 S; right to left: S P1 | P3 S | P2 S P1.
    let _ui = show(move || {
        let p = p.clone();
        rtl(move || {
            let p = p.clone();
            Row!(modifier = Modifier.fill_max_size(), |_| {
                let zero = PaddingValues::ZERO;
                let paddings = [
                    PaddingValues { start: padding1_dp, end: padding2_dp, ..zero },
                    PaddingValues { end: padding3_dp, ..zero },
                    PaddingValues { start: padding1_dp, ..zero },
                ];
                for (index, padding) in paddings.into_iter().enumerate() {
                    let modifier = Modifier.padding(padding).size(size_dp).then(p.at(index));
                    Box!(modifier = modifier);
                }
            });
        });
    });
    let width = ROOT.width;
    assert_eq!(probe.position(0), offset(width - padding1 - size, 0));
    assert_eq!(probe.position(1), offset(width - padding1 - padding2 - size * 2, 0));
    let third = width - size * 3 - padding1 * 2 - padding2 - padding3;
    assert_eq!(probe.position(2), offset(third, 0));
    for index in 0..3 {
        assert_eq!(probe.size(index), self::size(size, size));
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, testAbsolutePadding_rtl
#[test]
fn absolute_padding_rtl() {
    let size_dp = px(100.0);
    let size = round(size_dp);
    let (padding1_dp, padding2_dp, padding3_dp) = (Dp(5.0), Dp(10.0), Dp(15.0));
    let (padding1, padding2, padding3) =
        (round(padding1_dp), round(padding2_dp), round(padding3_dp));
    let probe = Probe::new(2);
    let p = probe.clone();
    // Left to right: P1 S P2 | S P3; right to left: S P3 | P1 S P2.
    let _ui = show(move || {
        let p = p.clone();
        rtl(move || {
            let p = p.clone();
            Row!(modifier = Modifier.fill_max_size(), |_| {
                let zero = Dp(0.0);
                let first = Modifier.absolute_padding(padding1_dp, zero, padding2_dp, zero);
                Box!(modifier = first.size(size_dp).then(p.at(0)));
                let second = Modifier.absolute_padding(zero, zero, padding3_dp, zero);
                Box!(modifier = second.size(size_dp).then(p.at(1)));
            });
        });
    });
    let width = ROOT.width;
    assert_eq!(probe.position(0), offset(width - padding2 - size, 0));
    let second = width - size * 2 - padding1 - padding2 - padding3;
    assert_eq!(probe.position(1), offset(second, 0));
}

/// `PaddingValues.Absolute` given to `padding` right to left keeps its left on the left, too
/// little room or not, as Compose's `PaddingValuesModifier` (`place`, not `placeRelative`).
#[test]
fn absolute_padding_values_keep_their_sides_right_to_left() {
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        rtl(move || {
            let p = p.clone();
            Row!(modifier = Modifier.fill_max_size(), |_| {
                let padding = PaddingValues::absolute(px(5.0), px(0.0), px(10.0), px(0.0));
                Box!(modifier = Modifier.padding(padding).size(px(100.0)).then(p.at(0)));
                // 30 px of padding in 20: the content still starts 25 px from the left.
                let tight = PaddingValues::absolute(px(25.0), px(0.0), px(5.0), px(0.0));
                let modifier = Modifier.required_width(px(20.0)).padding(tight).then(p.at(1));
                Box!(modifier = modifier);
            });
        });
    });
    let width = ROOT.width;
    assert_eq!(probe.position(0), offset(width - 10 - 100, 0));
    assert_eq!(probe.size(1), size(0, 0));
    let tight_left = width - 115 - 20;
    assert_eq!(probe.position(1), offset(tight_left + 25, 0));
}

/// An absolute and a relative padding added keep both kinds of sides, as Compose's `plus`
/// reads each in the layout's direction (D2320).
#[test]
fn absolute_and_relative_paddings_add_side_by_side() {
    let absolute = PaddingValues::absolute(Dp(1.0), Dp(2.0), Dp(3.0), Dp(4.0));
    let relative = PaddingValues::new(Dp(10.0), Dp(20.0), Dp(30.0), Dp(40.0));
    let sum = absolute + relative;
    assert_eq!(sides(sum), [Dp(11.0), Dp(22.0), Dp(33.0), Dp(44.0)]);
    assert_eq!(sum.calculate_left_padding(LayoutDirection::Rtl), Dp(31.0));
    assert_eq!(sum.calculate_right_padding(LayoutDirection::Rtl), Dp(13.0));
    assert_eq!(sum.calculate_start_padding(LayoutDirection::Rtl), Dp(13.0));
    assert_eq!(sum.calculate_end_padding(LayoutDirection::Rtl), Dp(31.0));
    let resolved = sum.resolved(LayoutDirection::Rtl);
    assert_eq!((resolved.start, resolved.end), (Dp(13.0), Dp(31.0)));
    assert_eq!((resolved.left, resolved.right), (Dp(0.0), Dp(0.0)));
    assert_ne!(absolute, PaddingValues::new(Dp(1.0), Dp(2.0), Dp(3.0), Dp(4.0)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingTest.kt, testPaddingValuesRtl
#[test]
fn padding_values_rtl() {
    let (box_size, start, end) = (10.0, 1.0, 2.0);
    let probe = Probe::new(1);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        rtl(move || {
            let padding = PaddingValues { start: px(start), end: px(end), ..PaddingValues::ZERO };
            Box!(modifier = Modifier.size(px(box_size)).padding(padding).then(p.at(0)));
        });
    });
    assert_eq!(probe.size(0).width, (box_size - start - end) as i32);
}

// ---- PaddingValuesTest --------------------------------------------------------------------

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingValuesTest.kt, zeroPaddingValues
#[test]
fn zero_padding_values() {
    let probe = Probe::new(1);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        Box!(modifier = Modifier.fill_max_size(), |_| {
            let modifier = Modifier.size(px(100.0)).padding(PaddingValues::ZERO).then(p.at(0));
            Box!(modifier = modifier);
        });
    });
    assert_eq!(probe.size(0), size(100, 100));
}

/// Each side of `padding`, left to right: left, top, right, bottom.
fn sides(padding: PaddingValues) -> [Dp; 4] {
    [
        padding.calculate_left_padding(LayoutDirection::Ltr),
        padding.calculate_top_padding(),
        padding.calculate_right_padding(LayoutDirection::Ltr),
        padding.calculate_bottom_padding(),
    ]
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingValuesTest.kt, plusPaddingValues
#[test]
fn plus_padding_values() {
    let (inner, content) = (PaddingValues::all(Dp(24.0)), PaddingValues::all(Dp(8.0)));
    assert_eq!(sides(inner + content), [Dp(32.0); 4]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingValuesTest.kt, minusPaddingValues
#[test]
fn minus_padding_values() {
    let (inner, content) = (PaddingValues::all(Dp(24.0)), PaddingValues::all(Dp(8.0)));
    assert_eq!(sides(inner - content), [Dp(16.0); 4]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/PaddingValuesTest.kt, minusPaddingValues_mustBeNonNegative
#[test]
fn minus_padding_values_must_be_non_negative() {
    let (inner, content) = (PaddingValues::all(Dp(24.0)), PaddingValues::all(Dp(8.0)));
    assert_eq!(sides(content - inner), [Dp(0.0); 4]);
}

/// D1930: the sides follow the layout direction, as Compose's `calculate*Padding`.
#[test]
fn padding_values_sides_follow_the_layout_direction() {
    let padding = PaddingValues::new(Dp(1.0), Dp(2.0), Dp(3.0), Dp(4.0));
    assert_eq!(padding.calculate_left_padding(LayoutDirection::Rtl), Dp(3.0));
    assert_eq!(padding.calculate_right_padding(LayoutDirection::Rtl), Dp(1.0));
    let sum = padding + PaddingValues::new(Dp(10.0), Dp(20.0), Dp(30.0), Dp(40.0));
    assert_eq!(sides(sum), [Dp(11.0), Dp(22.0), Dp(33.0), Dp(44.0)]);
}
