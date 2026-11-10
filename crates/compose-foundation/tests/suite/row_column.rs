//! Compose's `RowColumnTest`: `Row` and `Column` sizes and positions, weights, roundings,
//! alignments, arrangements, constraints and right-to-left layouts, through the `LayoutTest`
//! helpers of [`layout_test`](crate::layout_test) (a phone's density, a root of 1000 by 1600).
//! Its intrinsics tests are in `intrinsics.rs`, its `alignBy` ones in `alignment_line.rs`.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::callback;
use compose_ui::{
    Alignment, Constraints, Dp, DpSize, HorizontalAlignment, IntSize, IntrinsicSize, Layout,
    LayoutCoordinates, Offset, VerticalAlignment, measure_policy,
};

use crate::layout_test::*;

/// `content` in a container aligning it top-start, as most of these tests.
fn top_start(content: impl Fn() + 'static) {
    container(Modifier, Spec::aligned(Alignment::TOP_START), content);
}

fn width_into(width: &Rc<Cell<f32>>) -> Modifier {
    let width = width.clone();
    Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
        width.set(coordinates.size().width as f32)
    })
}

fn height_into(height: &Rc<Cell<f32>>) -> Modifier {
    let height = height.clone();
    Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
        height.set(coordinates.size().height as f32)
    })
}

// ---- Size and position --------------------------------------------------------------------

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow
#[test]
fn row() {
    let size_dp = px(50.0);
    let size = round(size_dp);
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        top_start(move || {
            let p = p.clone();
            Row!(|_| {
                empty(p.at(0), Spec::sized(size_dp, size_dp));
                empty(p.at(1), Spec::sized(size_dp * 2.0, size_dp * 2.0));
            });
        });
    });
    let double = (to_px(size_dp) * 2.0).round() as i32;
    assert_eq!(probe.size(0), self::size(size, size));
    assert_eq!(probe.size(1), self::size(double, double));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(size, 0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn
#[test]
fn column() {
    let size_dp = px(50.0);
    let size = round(size_dp);
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        top_start(move || {
            let p = p.clone();
            Column!(|_| {
                empty(p.at(0), Spec::sized(size_dp, size_dp));
                empty(p.at(1), Spec::sized(size_dp * 2.0, size_dp * 2.0));
            });
        });
    });
    let double = (to_px(size_dp) * 2.0).round() as i32;
    assert_eq!(probe.size(0), self::size(size, size));
    assert_eq!(probe.size(1), self::size(double, double));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(0, size));
}

/// Two children of `width` by `height` (the second `second_height`) in a row or column, with
/// `weight`s and `fill`.
fn weighted(
    vertical: bool,
    weights: [f32; 2],
    fill: bool,
    width: f32,
    height: f32,
    second_height: f32,
) -> Probe {
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        top_start(move || {
            let p = p.clone();
            let child = move |index: usize, modifier: Modifier| {
                let height = if index == 0 { height } else { second_height };
                empty(modifier.then(p.at(index)), Spec::sized(px(width), px(height)));
            };
            if vertical {
                Column!(|scope| {
                    child(0, Modifier.weight_with(scope, weights[0], fill));
                    child(1, Modifier.weight_with(scope, weights[1], fill));
                });
            } else {
                Row!(|scope| {
                    child(0, Modifier.weight_with(scope, weights[0], fill));
                    child(1, Modifier.weight_with(scope, weights[1], fill));
                });
            }
        });
    });
    probe
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withChildrenWithWeight
#[test]
fn row_with_children_with_weight() {
    let probe = weighted(false, [1.0, 2.0], true, 50.0, 80.0, 80.0);
    let (root, height) = (ROOT.width as f32, round(px(80.0)));
    assert_eq!(probe.size(0), size((root / 3.0).round() as i32, height));
    assert_eq!(probe.size(1), size((root * 2.0 / 3.0).round() as i32, height));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset((root / 3.0).round() as i32, 0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withChildrenWithWeightNonFilling
#[test]
fn row_with_children_with_weight_non_filling() {
    let probe = weighted(false, [1.0, 2.0], false, 50.0, 80.0, 160.0);
    let (width, height) = (round(px(50.0)), round(px(80.0)));
    assert_eq!(probe.size(0), size(width, height));
    assert_eq!(probe.size(1), size(width, round(px(80.0) * 2.0)));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(width, 0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withChildrenWithMaxValueWeight
#[test]
fn row_with_children_with_max_value_weight() {
    let probe = weighted(false, [f32::MAX, 1.0], true, 50.0, 80.0, 80.0);
    let height = round(px(80.0));
    assert_eq!(probe.size(0), size(ROOT.width, height));
    assert_eq!(probe.size(1), size(0, height));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(ROOT.width, 0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withChildrenWithPositiveInfinityWeight
#[test]
fn row_with_children_with_positive_infinity_weight() {
    let probe = weighted(false, [f32::INFINITY, 1.0], true, 50.0, 80.0, 80.0);
    let height = round(px(80.0));
    assert_eq!(probe.size(0), size(ROOT.width, height));
    assert_eq!(probe.size(1), size(0, height));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(ROOT.width, 0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withChildrenWithWeight
#[test]
fn column_with_children_with_weight() {
    let probe = weighted(true, [1.0, 2.0], true, 80.0, 50.0, 50.0);
    let (root, width) = (ROOT.height as f32, round(px(80.0)));
    assert_eq!(probe.size(0), size(width, (root / 3.0).round() as i32));
    assert_eq!(probe.size(1), size(width, (root * 2.0 / 3.0).round() as i32));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(0, (root / 3.0).round() as i32));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withChildrenWithWeightNonFilling
#[test]
fn column_with_children_with_weight_non_filling() {
    let probe = weighted(true, [1.0, 2.0], false, 80.0, 50.0, 50.0);
    let (width, height) = (round(px(80.0)), round(px(50.0)));
    assert_eq!(probe.size(0), size(width, height));
    assert_eq!(probe.size(1), size(width, height));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(0, height));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withChildrenWithMaxValueWeight
#[test]
fn column_with_children_with_max_value_weight() {
    let probe = weighted(true, [f32::MAX, 1.0], true, 80.0, 50.0, 50.0);
    let width = round(px(80.0));
    assert_eq!(probe.size(0), size(width, ROOT.height));
    assert_eq!(probe.size(1), size(width, 0));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(0, ROOT.height));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withChildrenWithPositiveInfinityWeight
#[test]
fn column_with_children_with_positive_infinity_weight() {
    let probe = weighted(true, [f32::INFINITY, 1.0], true, 80.0, 50.0, 50.0);
    let width = round(px(80.0));
    assert_eq!(probe.size(0), size(width, ROOT.height));
    assert_eq!(probe.size(1), size(width, 0));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(0, ROOT.height));
}

fn panics(f: impl FnOnce() -> Modifier) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_invalidWeight
#[test]
fn row_invalid_weight() {
    let scope = RowScope::__new();
    assert!(panics(|| Modifier.weight(scope, -1.0)));
    assert!(panics(|| Modifier.weight(scope, f32::NAN)));
    assert!(panics(|| Modifier.weight(scope, f32::NEG_INFINITY)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_invalidWeight
#[test]
fn column_invalid_weight() {
    let scope = ColumnScope::__new();
    assert!(panics(|| Modifier.weight(scope, -1.0)));
    assert!(panics(|| Modifier.weight(scope, f32::NAN)));
    assert!(panics(|| Modifier.weight(scope, f32::NEG_INFINITY)));
}

// ---- Roundings ----------------------------------------------------------------------------

/// A row (or column) wrapping its content, after `padding` pixels at its start (top), at most
/// `max` pixels long, of children with `weights`: its length, and their lengths and positions.
fn rounded(
    vertical: bool,
    max: f32,
    padding: f32,
    weights: &'static [f32],
) -> (f32, Vec<(f32, f32)>) {
    let length = Rc::new(Cell::new(0.0));
    let children: Vec<_> = weights.iter().map(|_| Rc::new(Cell::new((0.0, 0.0)))).collect();
    let (l, c) = (length.clone(), children.clone());
    let _ui = show(move || {
        let c = c.clone();
        let child = move |index: usize, modifier: Modifier| {
            let seen = c[index].clone();
            let modifier =
                modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
                    let (size, at) = (coordinates.size(), coordinates.position_in_root());
                    seen.set(if vertical {
                        (size.height as f32, at.y)
                    } else {
                        (size.width as f32, at.x)
                    });
                });
            empty(modifier, Spec::default());
        };
        let wrap = Modifier.wrap_content_size_with(Alignment::TOP_START, false);
        if vertical {
            let modifier = wrap
                .padding(PaddingValues { top: px(padding), ..PaddingValues::ZERO })
                .height_in(Dp(0.0), px(max))
                .then(height_into(&l));
            Column!(modifier = modifier, |scope| {
                for (index, &weight) in weights.iter().enumerate() {
                    child(index, Modifier.weight(scope, weight));
                }
            });
        } else {
            let modifier = wrap
                .padding(PaddingValues { start: px(padding), ..PaddingValues::ZERO })
                .width_in(Dp(0.0), px(max))
                .then(width_into(&l));
            Row!(modifier = modifier, |scope| {
                for (index, &weight) in weights.iter().enumerate() {
                    child(index, Modifier.weight(scope, weight));
                }
            });
        }
    });
    (length.get(), children.iter().map(|child| child.get()).collect())
}

fn assert_rounded(vertical: bool, max: f32, weights: &'static [f32]) {
    let (length, children) = rounded(vertical, max, 1.0, weights);
    assert_eq!(length, max);
    let mut at = 1.0;
    for &(child, position) in &children {
        assert_eq!(position, at);
        at += child;
    }
    assert_eq!(children.iter().map(|&(child, _)| child).sum::<f32>(), length);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_doesNotPlaceChildrenOutOfBounds_becauseOfRoundings
#[test]
fn row_does_not_place_children_out_of_bounds_because_of_roundings() {
    assert_rounded(false, 11.0, &[1.0, 1.0]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_isNotLargerThanItsChildren_becauseOfRoundings
#[test]
fn row_is_not_larger_than_its_children_because_of_roundings() {
    assert_rounded(false, 8.0, &[2.0, 2.0, 3.0]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_isNotLargetThanItsChildren_becauseOfRoundings
#[test]
fn column_is_not_larger_than_its_children_because_of_roundings() {
    assert_rounded(true, 8.0, &[1.0, 1.0, 1.0]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_doesNotPlaceChildrenOutOfBounds_becauseOfRoundings
#[test]
fn column_does_not_place_children_out_of_bounds_because_of_roundings() {
    assert_rounded(true, 11.0, &[1.0, 1.0]);
}

// ---- Cross-axis alignment -----------------------------------------------------------------

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withColumnAlignModifier_usesCorrectCrossAxisSize
#[test]
fn row_with_column_align_modifier_uses_correct_cross_axis_size() {
    let (child_width, child_height, row_height) = (px(20.0), px(50.0), px(100.0));
    let probe = Probe::new(1);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        Column!(|column| {
            let column_modifier = Modifier.align(column, Alignment::END);
            let p = p.clone();
            Row!(modifier = Modifier.height(row_height), |_| {
                Box!(
                    modifier = column_modifier
                        .clone()
                        .size(DpSize::new(child_width, child_height))
                        .then(p.at(0))
                );
            });
        });
    });
    let expected_y = (to_px(row_height) - to_px(child_height)).round() as i32;
    assert_eq!(probe.position(0), offset(0, expected_y));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withRowAlignModifier_usesCorrectCrossAxisSize
#[test]
fn column_with_row_align_modifier_uses_correct_cross_axis_size() {
    let (child_width, child_height, column_width) = (px(50.0), px(20.0), px(100.0));
    let probe = Probe::new(1);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        Row!(|row| {
            let row_modifier = Modifier.align(row, Alignment::BOTTOM);
            let p = p.clone();
            Column!(modifier = Modifier.width(column_width), |_| {
                Box!(
                    modifier = row_modifier
                        .clone()
                        .size(DpSize::new(child_width, child_height))
                        .then(p.at(0))
                );
            });
        });
    });
    let expected_x = (to_px(column_width) - to_px(child_width)).round() as i32;
    assert_eq!(probe.position(0), offset(expected_x, 0));
}

/// D1927: in a right-to-left column, a row's alignment isn't mirrored (Compose's
/// `Alignment.Vertical` takes no direction: `TOP` is on the left), the column's own horizontal
/// one is (its start on the right).
#[test]
fn a_row_alignment_in_a_right_to_left_column_is_not_mirrored() {
    let (child, column_width) = (px(20.0), px(100.0));
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl);
        compose_runtime::CompositionLocalProvider(rtl, move || {
            let p = p.clone();
            Row!(|row| {
                let top = Modifier.align(row, Alignment::TOP);
                let p = p.clone();
                Column!(modifier = Modifier.width(column_width), |_| {
                    Box!(modifier = top.clone().size(child).then(p.at(0)));
                    Box!(modifier = Modifier.size(child).then(p.at(1)));
                });
            });
        });
    });
    let end = round(column_width) - round(child);
    // The row wraps the column, at the root's origin.
    let column_x = 0;
    assert_eq!(probe.position(0).x as i32, column_x);
    assert_eq!(probe.position(1).x as i32, column_x + end);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withStretchCrossAxisAlignment
#[test]
fn row_with_stretch_cross_axis_alignment() {
    let size_dp = px(50.0);
    let size = round(size_dp);
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        Row!(|_| {
            empty(Modifier.fill_max_height().then(p.at(0)), Spec::sized(size_dp, size_dp));
            empty(
                Modifier.fill_max_height().then(p.at(1)),
                Spec::sized(size_dp * 2.0, size_dp * 2.0),
            );
        });
    });
    assert_eq!(probe.size(0), self::size(size, ROOT.height));
    assert_eq!(probe.size(1), self::size((to_px(size_dp) * 2.0).round() as i32, ROOT.height));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(size, 0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withStretchCrossAxisAlignment
#[test]
fn column_with_stretch_cross_axis_alignment() {
    let size_dp = px(50.0);
    let size = round(size_dp);
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        Column!(|_| {
            empty(Modifier.fill_max_width().then(p.at(0)), Spec::sized(size_dp, size_dp));
            empty(
                Modifier.fill_max_width().then(p.at(1)),
                Spec::sized(size_dp * 2.0, size_dp * 2.0),
            );
        });
    });
    assert_eq!(probe.size(0), self::size(ROOT.width, size));
    assert_eq!(probe.size(1), self::size(ROOT.width, round(size_dp * 2.0)));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(0, size));
}

/// Three children of 50 px in a row filling the root's height (`center` its alignment, else
/// top), aligned top, by the row (or centered), bottom.
fn row_gravities(centered_by_row: bool) {
    let size_dp = px(50.0);
    let size = round(size_dp);
    let probe = Probe::new(3);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        let alignment = if centered_by_row { Alignment::CENTER_VERTICALLY } else { Alignment::TOP };
        Row!(modifier = Modifier.fill_max_height(), vertical_alignment = alignment, |row| {
            let spec = Spec::sized(size_dp, size_dp);
            empty(Modifier.align(row, Alignment::TOP).then(p.at(0)), spec);
            let middle = if centered_by_row {
                Modifier
            } else {
                Modifier.align(row, Alignment::CENTER_VERTICALLY)
            };
            empty(middle.then(p.at(1)), spec);
            empty(Modifier.align(row, Alignment::BOTTOM).then(p.at(2)), spec);
        });
    });
    let root = ROOT.height as f32;
    for index in 0..3 {
        assert_eq!(probe.size(index), self::size(size, size));
    }
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(size, ((root - size as f32) / 2.0).round() as i32));
    assert_eq!(probe.position(2), offset(size * 2, ROOT.height - size));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withGravityModifier_andGravityParameter
#[test]
fn row_with_gravity_modifier_and_gravity_parameter() {
    row_gravities(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withGravityModifier
#[test]
fn row_with_gravity_modifier() {
    row_gravities(false);
}

/// As [`row_gravities`], in a column filling the root's width.
fn column_gravities(centered_by_column: bool) {
    let size_dp = px(50.0);
    let size = round(size_dp);
    let probe = Probe::new(3);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        let alignment =
            if centered_by_column { Alignment::CENTER_HORIZONTALLY } else { Alignment::START };
        Column!(modifier = Modifier.fill_max_width(), horizontal_alignment = alignment, |column| {
            let spec = Spec::sized(size_dp, size_dp);
            empty(Modifier.align(column, Alignment::START).then(p.at(0)), spec);
            let middle = if centered_by_column {
                Modifier
            } else {
                Modifier.align(column, Alignment::CENTER_HORIZONTALLY)
            };
            empty(middle.then(p.at(1)), spec);
            empty(Modifier.align(column, Alignment::END).then(p.at(2)), spec);
        });
    });
    let root = ROOT.width as f32;
    for index in 0..3 {
        assert_eq!(probe.size(index), self::size(size, size));
    }
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.position(1), offset(((root - size as f32) / 2.0).round() as i32, size));
    assert_eq!(probe.position(2), offset(ROOT.width - size, size * 2));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withGravityModifier
#[test]
fn column_with_gravity_modifier() {
    column_gravities(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withCustomVertical_alignment
#[test]
fn row_with_custom_vertical_alignment() {
    let row_height = Dp(200.0);
    let box_size = Dp(20.0);
    let offset = Rc::new(Cell::new(0.0f32));
    let captured_space = Rc::new(Cell::new(0));
    let captured_size = Rc::new(Cell::new(0));
    let (o, space_out, size_out) = (offset.clone(), captured_space.clone(), captured_size.clone());
    let _ui = show(move || {
        let (o, space_out, size_out) = (o.clone(), space_out.clone(), size_out.clone());
        let alignment = VerticalAlignment::custom(move |size, space| {
            space_out.set(space);
            size_out.set(size);
            ((space - size) / 2).clamp(0, space - size)
        });
        Row!(
            modifier = Modifier.size((Dp(200.0), row_height)),
            vertical_alignment = alignment,
            |_| {
                let o = o.clone();
                Box!(
                    modifier = Modifier.size(box_size).on_globally_positioned(
                        move |c: LayoutCoordinates| o.set(c.position_in_root().y)
                    )
                );
            }
        );
    });
    let expected = ((round(row_height) - round(box_size)) / 2) as f32;
    assert_eq!(offset.get(), expected);
    assert_eq!(captured_size.get(), round(box_size));
    assert_eq!(captured_space.get(), round(row_height));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withCustomHorizontal_alignment
#[test]
fn column_with_custom_horizontal_alignment() {
    let column_width = Dp(300.0);
    let box_size = Dp(40.0);
    let offset = Rc::new(Cell::new(0.0f32));
    let captured_space = Rc::new(Cell::new(0));
    let captured_size = Rc::new(Cell::new(0));
    let (o, space_out, size_out) = (offset.clone(), captured_space.clone(), captured_size.clone());
    let _ui = show(move || {
        let (o, space_out, size_out) = (o.clone(), space_out.clone(), size_out.clone());
        let alignment = HorizontalAlignment::custom(move |size, space, _| {
            space_out.set(space);
            size_out.set(size);
            ((space - size) / 2).clamp(0, space - size)
        });
        Column!(
            modifier = Modifier.size((column_width, Dp(200.0))),
            horizontal_alignment = alignment,
            |_| {
                let o = o.clone();
                Box!(
                    modifier = Modifier.size(box_size).on_globally_positioned(
                        move |c: LayoutCoordinates| o.set(c.position_in_root().x)
                    )
                );
            }
        );
    });
    let expected = ((round(column_width) - round(box_size)) / 2) as f32;
    assert_eq!(offset.get(), expected);
    assert_eq!(captured_size.get(), round(box_size));
    assert_eq!(captured_space.get(), round(column_width));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withCustomHorizontalAlignModifier
#[test]
fn column_with_custom_horizontal_align_modifier() {
    let column_width = Dp(300.0);
    let box_size = Dp(40.0);
    let offset = Rc::new(Cell::new(0.0f32));
    let captured_space = Rc::new(Cell::new(0));
    let captured_size = Rc::new(Cell::new(0));
    let (o, space_out, size_out) = (offset.clone(), captured_space.clone(), captured_size.clone());
    let _ui = show(move || {
        let (o, space_out, size_out) = (o.clone(), space_out.clone(), size_out.clone());
        Column!(modifier = Modifier.size((column_width, Dp(200.0))), |column| {
            let (o, space_out, size_out) = (o.clone(), space_out.clone(), size_out.clone());
            let alignment = HorizontalAlignment::custom(move |size, space, _| {
                space_out.set(space);
                size_out.set(size);
                ((space - size) / 2).clamp(0, space - size)
            });
            Box!(
                modifier = Modifier.size(box_size).align(column, alignment).on_globally_positioned(
                    move |c: LayoutCoordinates| o.set(c.position_in_root().x)
                )
            );
        });
    });
    let expected = ((round(column_width) - round(box_size)) / 2) as f32;
    assert_eq!(offset.get(), expected);
    assert_eq!(captured_size.get(), round(box_size));
    assert_eq!(captured_space.get(), round(column_width));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withCustomVerticalAlignModifier
#[test]
fn row_with_custom_vertical_align_modifier() {
    let row_height = Dp(200.0);
    let box_size = Dp(20.0);
    let offset = Rc::new(Cell::new(0.0f32));
    let captured_space = Rc::new(Cell::new(0));
    let captured_size = Rc::new(Cell::new(0));
    let (o, space_out, size_out) = (offset.clone(), captured_space.clone(), captured_size.clone());
    let _ui = show(move || {
        let (o, space_out, size_out) = (o.clone(), space_out.clone(), size_out.clone());
        Row!(modifier = Modifier.size((Dp(200.0), row_height)), |row| {
            let (o, space_out, size_out) = (o.clone(), space_out.clone(), size_out.clone());
            let alignment = VerticalAlignment::custom(move |size, space| {
                space_out.set(space);
                size_out.set(size);
                ((space - size) / 2).clamp(0, space - size)
            });
            Box!(
                modifier = Modifier.size(box_size).align(row, alignment).on_globally_positioned(
                    move |c: LayoutCoordinates| o.set(c.position_in_root().y)
                )
            );
        });
    });
    let expected = ((round(row_height) - round(box_size)) / 2) as f32;
    assert_eq!(offset.get(), expected);
    assert_eq!(captured_size.get(), round(box_size));
    assert_eq!(captured_space.get(), round(row_height));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withGravityModifier_andGravityParameter
#[test]
fn column_with_gravity_modifier_and_gravity_parameter() {
    column_gravities(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withAlignByModifier_andWeight
#[test]
fn column_with_align_by_modifier_and_weight() {
    let (baseline_dp, size_dp) = (px(30.0), px(40.0));
    let (baseline, size) = (round(baseline_dp), round(size_dp));
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        Column!(modifier = Modifier.fill_max_width(), |column| {
            // As `BaselineTestLayout`: its test line at `baseline`.
            let line = compose_ui::VerticalAlignmentLine::new(i32::min);
            let policy = measure_policy(move |scope, _, constraints| {
                scope.provide_alignment_lines([(line, baseline)]);
                scope.layout(size.max(constraints.min_width), size.max(constraints.min_height))
            });
            Layout(Modifier.align_by(column, line).then(p.at(0)), policy, callback(|| {}));
            empty(
                Modifier
                    .align_by_block(column, |measured| measured.measured_width() / 2)
                    .weight(column, 1.0)
                    .then(p.at(1)),
                Spec { width: Some(size_dp), ..Spec::default() },
            );
        });
    });
    assert_eq!(probe.size(0), self::size(size, size));
    assert_eq!(probe.position(0), offset(0, 0));
    assert_eq!(probe.size(1).width, size);
    assert_eq!(probe.position(1), offset(baseline - size / 2, size));
}

// ---- Sizes --------------------------------------------------------------------------------

/// The size of the layout `content` puts its modifier on.
fn measured(content: impl Fn(Modifier) + 'static) -> IntSize {
    let seen = Rc::new(Cell::new(IntSize::new(-1, -1)));
    let s = seen.clone();
    let _ui = show(move || {
        let s = s.clone();
        content(Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
            s.set(coordinates.size())
        }));
    });
    seen.get()
}

/// The two spacers of most size tests: 50 px and 100 px squares.
fn two_spacers() {
    let size_dp = px(50.0);
    Spacer(Modifier.size(DpSize::new(size_dp, size_dp)));
    Spacer(Modifier.size(DpSize::new(size_dp * 2.0, size_dp * 2.0)));
}

/// A row (or column) of `modifier` with [`two_spacers`], centered in the root.
fn centered(vertical: bool, modifier: Modifier) {
    center(move || {
        if vertical {
            Column!(modifier = modifier.clone(), |_| two_spacers());
        } else {
            Row!(modifier = modifier.clone(), |_| two_spacers());
        }
    });
}

/// As [`centered`], with a weighted 50 px container and a 100 px one.
fn centered_weighted(vertical: bool, modifier: Modifier) {
    center(move || {
        let size_dp = px(50.0);
        let children = move |weight: Modifier| {
            empty(weight, Spec::sized(size_dp, size_dp));
            empty(Modifier, Spec::sized(size_dp * 2.0, size_dp * 2.0));
        };
        if vertical {
            Column!(modifier = modifier.clone(), |scope| children(Modifier.weight(scope, 1.0)));
        } else {
            Row!(modifier = modifier.clone(), |scope| children(Modifier.weight(scope, 1.0)));
        }
    });
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_expandedWidth_withExpandedModifier
#[test]
fn row_expanded_width_with_expanded_modifier() {
    let size = measured(|seen| centered(false, Modifier.fill_max_width().then(seen)));
    assert_eq!(size.width, ROOT.width);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_wrappedWidth_withNoWeightChildren
#[test]
fn row_wrapped_width_with_no_weight_children() {
    let size = measured(|seen| centered(false, seen));
    assert_eq!(size.width, round(px(50.0) * 3.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_expandedWidth_withWeightChildren
#[test]
fn row_expanded_width_with_weight_children() {
    let size = measured(|seen| centered_weighted(false, seen));
    assert_eq!(size.width, ROOT.width);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withMaxCrossAxisSize
#[test]
fn row_with_max_cross_axis_size() {
    let size = measured(|seen| centered(false, Modifier.fill_max_height().then(seen)));
    assert_eq!(size.height, ROOT.height);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withMinCrossAxisSize
#[test]
fn row_with_min_cross_axis_size() {
    let size = measured(|seen| centered(false, seen));
    assert_eq!(size.height, round(px(50.0) * 2.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_expandedHeight_withExpandedModifier
#[test]
fn column_expanded_height_with_expanded_modifier() {
    let size = measured(|seen| centered(true, Modifier.fill_max_height().then(seen)));
    assert_eq!(size.height, ROOT.height);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_wrappedHeight_withNoChildrenWithWeight
#[test]
fn column_wrapped_height_with_no_children_with_weight() {
    let size = measured(|seen| centered(true, seen));
    assert_eq!(size.height, round(px(50.0) * 3.0));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_expandedHeight_withWeightChildren
#[test]
fn column_expanded_height_with_weight_children() {
    let size = measured(|seen| centered_weighted(true, seen));
    assert_eq!(size.height, ROOT.height);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withMaxCrossAxisSize
#[test]
fn column_with_max_cross_axis_size() {
    let size = measured(|seen| centered(true, Modifier.fill_max_width().then(seen)));
    assert_eq!(size.width, ROOT.width);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withMinCrossAxisSize
#[test]
fn column_with_min_cross_axis_size() {
    let size = measured(|seen| centered(true, seen));
    assert_eq!(size.width, round(px(50.0) * 2.0));
}

/// A row (or column) of `modifier` with [`two_spacers`] (or a weighted container and a
/// fixed one), in a `ConstrainedBox` of `constraints`, centered in the root.
fn constrained(vertical: bool, weighted: bool, constraints: DpConstraints, modifier: Modifier) {
    center(move || {
        let modifier = modifier.clone();
        constrained_box(constraints, Modifier, move || {
            if weighted {
                let size_dp = px(50.0);
                let children = move |weight: Modifier| {
                    empty(weight, Spec::sized(size_dp, size_dp));
                    empty(Modifier, Spec::sized(size_dp * 2.0, size_dp * 2.0));
                };
                if vertical {
                    Column!(modifier = modifier.clone(), |scope| children(
                        Modifier.weight(scope, 1.0)
                    ));
                } else {
                    Row!(modifier = modifier.clone(), |scope| children(
                        Modifier.weight(scope, 1.0)
                    ));
                }
            } else if vertical {
                Column!(modifier = modifier.clone(), |_| two_spacers());
            } else {
                Row!(modifier = modifier.clone(), |_| two_spacers());
            }
        });
    });
}

fn max_width(width: Dp) -> DpConstraints {
    DpConstraints { max_width: width, ..DpConstraints::default() }
}

fn min_width(width: Dp) -> DpConstraints {
    DpConstraints { min_width: width, ..DpConstraints::default() }
}

fn max_height(height: Dp) -> DpConstraints {
    DpConstraints { max_height: height, ..DpConstraints::default() }
}

fn min_height(height: Dp) -> DpConstraints {
    DpConstraints { min_height: height, ..DpConstraints::default() }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withExpandedModifier_respectsMaxWidthConstraint
#[test]
fn row_with_expanded_modifier_respects_max_width_constraint() {
    let width = px(250.0);
    let size = measured(move |seen| {
        constrained(false, false, max_width(width), Modifier.fill_max_width().then(seen))
    });
    assert_eq!(size.width, ROOT.width.min(round(width)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withChildrenWithWeight_respectsMaxWidthConstraint
#[test]
fn row_with_children_with_weight_respects_max_width_constraint() {
    let width = px(250.0);
    let size = measured(move |seen| constrained(false, true, max_width(width), seen));
    assert_eq!(size.width, ROOT.width.min(round(width)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withNoWeightChildren_respectsMinWidthConstraint
#[test]
fn row_with_no_weight_children_respects_min_width_constraint() {
    let width = px(250.0);
    let size = measured(move |seen| constrained(false, false, min_width(width), seen));
    assert_eq!(size.width, round(width));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withMaxCrossAxisSize_respectsMaxHeightConstraint
#[test]
fn row_with_max_cross_axis_size_respects_max_height_constraint() {
    let height = px(250.0);
    let size = measured(move |seen| {
        constrained(false, false, max_height(height), Modifier.fill_max_height().then(seen))
    });
    assert_eq!(size.height, ROOT.height.min(round(height)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withMinCrossAxisSize_respectsMinHeightConstraint
#[test]
fn row_with_min_cross_axis_size_respects_min_height_constraint() {
    let height = px(150.0);
    let size = measured(move |seen| constrained(false, false, min_height(height), seen));
    assert_eq!(size.height, round(height));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withExpandedModifier_respectsMaxHeightConstraint
#[test]
fn column_with_expanded_modifier_respects_max_height_constraint() {
    let height = px(250.0);
    let size = measured(move |seen| {
        constrained(true, false, max_height(height), Modifier.fill_max_height().then(seen))
    });
    assert_eq!(size.height, ROOT.height.min(round(height)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withWeightChildren_respectsMaxHeightConstraint
#[test]
fn column_with_weight_children_respects_max_height_constraint() {
    let height = px(250.0);
    let size = measured(move |seen| constrained(true, true, max_height(height), seen));
    assert_eq!(size.height, ROOT.height.min(round(height)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withChildren_respectsMinHeightConstraint
#[test]
fn column_with_children_respects_min_height_constraint() {
    let height = px(250.0);
    let size = measured(move |seen| constrained(true, false, min_height(height), seen));
    assert_eq!(size.height, round(height));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withMaxCrossAxisSize_respectsMaxWidthConstraint
#[test]
fn column_with_max_cross_axis_size_respects_max_width_constraint() {
    let width = px(250.0);
    let size = measured(move |seen| {
        constrained(true, false, max_width(width), Modifier.fill_max_width().then(seen))
    });
    assert_eq!(size.width, ROOT.width.min(round(width)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withMinCrossAxisSize_respectsMinWidthConstraint
#[test]
fn column_with_min_cross_axis_size_respects_min_width_constraint() {
    let width = px(150.0);
    let size = measured(move |seen| constrained(true, false, min_width(width), seen));
    assert_eq!(size.width, round(width));
}

/// A row (column) of `IntrinsicSize::Max` width (height) in a `ConstrainedBox` at least 250 px
/// long, of one weighted 50 px container: the row's and the child's sizes.
fn min_main_axis(vertical: bool) -> (IntSize, IntSize) {
    let length = px(250.0);
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        center(move || {
            let p = p.clone();
            let constraints = if vertical { min_height(length) } else { min_width(length) };
            constrained_box(constraints, Modifier, move || {
                let p = p.clone();
                let size_dp = px(50.0);
                if vertical {
                    let modifier = Modifier.height(IntrinsicSize::Max).then(p.at(0));
                    Column!(modifier = modifier, |scope| {
                        empty(
                            Modifier.weight(scope, 1.0).then(p.at(1)),
                            Spec::sized(size_dp, size_dp),
                        );
                    });
                } else {
                    let modifier = Modifier.width(IntrinsicSize::Max).then(p.at(0));
                    Row!(modifier = modifier, |scope| {
                        empty(
                            Modifier.weight(scope, 1.0).then(p.at(1)),
                            Spec::sized(size_dp, size_dp),
                        );
                    });
                }
            });
        });
    });
    (probe.size(0), probe.size(1))
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withMinMainAxisSize
#[test]
fn row_with_min_main_axis_size() {
    let (row, child) = min_main_axis(false);
    let expected = size(round(px(250.0)), round(px(50.0)));
    assert_eq!(row, expected);
    assert_eq!(child, expected);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withMinMainAxisSize
#[test]
fn column_with_min_main_axis_size() {
    let (column, child) = min_main_axis(true);
    let expected = size(round(px(50.0)), round(px(250.0)));
    assert_eq!(column, expected);
    assert_eq!(child, expected);
}

/// Saves the constraints of what it's measured with into `into`, as a leaf of no size.
fn constraints_into(modifier: Modifier, into: &Rc<Cell<Option<Constraints>>>) {
    let into = into.clone();
    let policy = measure_policy(move |scope, _, constraints| {
        into.set(Some(constraints));
        scope.layout(0, 0)
    });
    Layout(modifier, policy, callback(|| {}));
}

/// A `BoxWithConstraints` saving its constraints into `into`, of a `FixedSizeLayout` of `width`
/// by `height`.
fn box_with_constraints_into(into: &Rc<Cell<Option<Constraints>>>, width: i32, height: i32) {
    let into = into.clone();
    BoxWithConstraints!(|scope: BoxWithConstraintsScope| {
        into.set(Some(scope.constraints));
        fixed_size_layout(width, height, Vec::new());
    });
}

fn seen() -> Rc<Cell<Option<Constraints>>> {
    Rc::new(Cell::new(None))
}

/// In infinite constraints, a `ConstrainedBox` at least 100 px long holding a row (column) of
/// two 30 px children and a weighted one: their constraints.
fn infinite(vertical: bool) -> [Option<Constraints>; 3] {
    let seen = [seen(), seen(), seen()];
    let s = seen.clone();
    let _ui = show(move || {
        let s = s.clone();
        with_infinite_constraints(move || {
            let s = s.clone();
            let (minimum, child) = (px(100.0), round(px(30.0)));
            let constraints = if vertical { min_height(minimum) } else { min_width(minimum) };
            constrained_box(constraints, Modifier, move || {
                let s = s.clone();
                if vertical {
                    Column!(|scope| {
                        box_with_constraints_into(&s[0], 0, child);
                        box_with_constraints_into(&s[1], 0, child);
                        constraints_into(Modifier.weight(scope, 1.0), &s[2]);
                    });
                } else {
                    Row!(|scope| {
                        box_with_constraints_into(&s[0], child, 0);
                        box_with_constraints_into(&s[1], child, 0);
                        constraints_into(Modifier.weight(scope, 1.0), &s[2]);
                    });
                }
            });
        });
    });
    [seen[0].get(), seen[1].get(), seen[2].get()]
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_measuresChildrenCorrectly_whenMeasuredWithInfiniteWidth
#[test]
fn row_measures_children_correctly_when_measured_with_infinite_width() {
    let [first, second, weighted] = infinite(false);
    assert_eq!(first, Some(Constraints::unbounded()));
    assert_eq!(second, Some(Constraints::unbounded()));
    let left = round(px(100.0)) - round(px(30.0)) * 2;
    let weighted = weighted.unwrap();
    assert_eq!((weighted.min_width, weighted.max_width), (left, left));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_measuresChildrenCorrectly_whenMeasuredWithInfiniteHeight
#[test]
fn column_measures_children_correctly_when_measured_with_infinite_height() {
    let [first, second, weighted] = infinite(true);
    assert_eq!(first, Some(Constraints::unbounded()));
    assert_eq!(second, Some(Constraints::unbounded()));
    let left = round(px(100.0)) - round(px(30.0)) * 2;
    let weighted = weighted.unwrap();
    assert_eq!((weighted.min_height, weighted.max_height), (left, left));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_protectsAgainstOverflow
#[test]
fn row_protects_against_overflow() {
    let placed = Rc::new(Cell::new(0));
    let first = seen();
    let (p, f) = (placed.clone(), first.clone());
    let _ui = show(move || {
        let (p, f) = (p.clone(), f.clone());
        with_infinite_constraints(move || {
            let (p, f) = (p.clone(), f.clone());
            constrained_box(min_width(px(0.0)), Modifier, move || {
                let (p, f) = (p.clone(), f.clone());
                Row!(horizontal_arrangement = Arrangement::spaced_by(Dp(0.5)), |scope| {
                    let (counted, f) = (p.clone(), f.clone());
                    let policy = measure_policy(move |scope, _, constraints| {
                        f.set(Some(constraints));
                        counted.set(counted.get() + 1);
                        scope.layout(1 << 23, 100)
                    });
                    Layout(Modifier, policy, callback(|| {}));
                    let counted = p.clone();
                    Box!(modifier = Modifier.weight_with(scope, 1.0, true), |_| {
                        counted.set(counted.get() + 1)
                    });
                    let counted = p.clone();
                    Box!(modifier = Modifier.weight_with(scope, 0.000_000_01, true), |_| {
                        counted.set(counted.get() + 1)
                    });
                });
            });
        });
    });
    assert_eq!(first.get(), Some(Constraints::unbounded()));
    assert!(placed.get() >= 3, "each child measured or composed");
}

/// In a box, a `ConstrainedBox` of exactly 100 by 200 px holding a row (column) of two
/// `BoxWithConstraints` of 50 by 100 px: their constraints.
fn no_weight_children(vertical: bool) -> [Option<Constraints>; 2] {
    let seen = [seen(), seen()];
    let s = seen.clone();
    let _ui = show(move || {
        let s = s.clone();
        Box!(|_| {
            let s = s.clone();
            let (width, height) = (px(100.0), px(200.0));
            constrained_box(DpConstraints::fixed(width, height), Modifier, move || {
                let s = s.clone();
                let (child_width, child_height) = (round(px(50.0)), round(px(100.0)));
                if vertical {
                    Column!(|_| {
                        box_with_constraints_into(&s[0], child_width, child_height);
                        box_with_constraints_into(&s[1], child_width, child_height);
                    });
                } else {
                    Row!(|_| {
                        box_with_constraints_into(&s[0], child_width, child_height);
                        box_with_constraints_into(&s[1], child_width, child_height);
                    });
                }
            });
        });
    });
    [seen[0].get(), seen[1].get()]
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_measuresNoWeightChildrenCorrectly
#[test]
fn row_measures_no_weight_children_correctly() {
    let [first, second] = no_weight_children(false);
    let (width, height, child) = (round(px(100.0)), round(px(200.0)), round(px(50.0)));
    assert_eq!(first, Some(Constraints::new(0, width, 0, height)));
    assert_eq!(second, Some(Constraints::new(0, width - child, 0, height)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_measuresNoWeightChildrenCorrectly
#[test]
fn column_measures_no_weight_children_correctly() {
    let [first, second] = no_weight_children(true);
    let (width, height, child) = (round(px(100.0)), round(px(200.0)), round(px(100.0)));
    assert_eq!(first, Some(Constraints::new(0, width, 0, height)));
    assert_eq!(second, Some(Constraints::new(0, width, 0, height - child)));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_doesNotExpand_whenWeightChildrenDoNotFill
#[test]
fn row_does_not_expand_when_weight_children_do_not_fill() {
    let size = measured(|seen| {
        Row!(modifier = seen, |scope| {
            Box!(modifier = Modifier.weight_with(scope, 1.0, false).size(px(10.0)));
        });
    });
    assert_eq!(size.width, 10);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_doesNotExpand_whenWeightChildrenDoNotFill
#[test]
fn column_does_not_expand_when_weight_children_do_not_fill() {
    let size = measured(|seen| {
        Column!(modifier = seen, |scope| {
            Box!(modifier = Modifier.weight_with(scope, 1.0, false).size(px(10.0)));
        });
    });
    assert_eq!(size.height, 10);
}

/// A row 40 px at most (a column of 40 px) spaced by 8 px, of two weighted boxes: their
/// lengths and positions.
fn spaced_weights(vertical: bool) {
    let (length, space) = (40, 8);
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        let spacing = Arrangement::spaced_by(px(space as f32));
        if vertical {
            Column!(
                modifier = Modifier.height(px(length as f32)),
                vertical_arrangement = spacing,
                |scope| {
                    Box!(modifier = Modifier.weight(scope, 1.0).then(p.at(0)));
                    Box!(modifier = Modifier.weight(scope, 1.0).then(p.at(1)));
                }
            );
        } else {
            Row!(
                modifier = Modifier.width_in(Dp(0.0), px(length as f32)),
                horizontal_arrangement = spacing,
                |scope| {
                    Box!(modifier = Modifier.weight(scope, 1.0).then(p.at(0)));
                    Box!(modifier = Modifier.weight(scope, 1.0).then(p.at(1)));
                }
            );
        }
    });
    let each = (length - space) / 2;
    let main = |index: usize| {
        let (size, at) = (probe.size(index), probe.position(index));
        if vertical { (size.height, at.y as i32) } else { (size.width, at.x as i32) }
    };
    assert_eq!(main(0), (each, 0));
    assert_eq!(main(1), (each, each + space));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_includesSpacing_withWeightChildren
#[test]
fn row_includes_spacing_with_weight_children() {
    spaced_weights(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_includesSpacing_withWeightChildren
#[test]
fn column_includes_spacing_with_weight_children() {
    spaced_weights(true);
}

// ---- Main-axis arrangements ---------------------------------------------------------------

/// `content` right to left, as `DeviceConfigurationOverride.LayoutDirection(Rtl)`.
fn rtl(content: impl Fn() + 'static) {
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl);
    compose_runtime::CompositionLocalProvider(rtl, content);
}

/// Where `count` containers of `size` pixels go in their parent, a row filling the root's width
/// with `arrangement` (right to left with `rtl`; centered in the root with `center`).
fn row_arranged(
    arrangement: HorizontalArrangement,
    count: usize,
    size: f32,
    is_rtl: bool,
) -> Vec<Offset> {
    let probe = Probe::new(count + 1);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        let arrangement = arrangement.clone();
        let row = move || {
            let p = p.clone();
            let arrangement = arrangement.clone();
            let modifier = Modifier.fill_max_width().then(p.at(count));
            Row!(modifier = modifier, horizontal_arrangement = arrangement.clone(), |_| {
                for index in 0..count {
                    empty(p.at(index), Spec::sized(px(size), px(size)));
                }
            });
        };
        if is_rtl { rtl(row) } else { center(row) }
    });
    let parent = probe.position(count);
    (0..count).map(|index| probe.position(index) - parent).collect()
}

/// As [`row_arranged`], in a column filling the root's height.
fn column_arranged(arrangement: VerticalArrangement, count: usize, size: f32) -> Vec<Offset> {
    let probe = Probe::new(count + 1);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        let arrangement = arrangement.clone();
        center(move || {
            let p = p.clone();
            let modifier = Modifier.fill_max_height().then(p.at(count));
            Column!(modifier = modifier, vertical_arrangement = arrangement.clone(), |_| {
                for index in 0..count {
                    empty(p.at(index), Spec::sized(px(size), px(size)));
                }
            });
        });
    });
    let parent = probe.position(count);
    (0..count).map(|index| probe.position(index) - parent).collect()
}

fn x(at: f32) -> Offset {
    Offset::new(at.round(), 0.0)
}

fn y(at: f32) -> Offset {
    Offset::new(0.0, at.round())
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withStartArrangement
#[test]
fn row_with_start_arrangement() {
    let size = round(px(50.0)) as f32;
    assert_eq!(row_arranged(Arrangement::START, 3, 50.0, false), [x(0.0), x(size), x(size * 2.0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withEndArrangement
#[test]
fn row_with_end_arrangement() {
    let (root, size) = (ROOT.width as f32, round(px(50.0)) as f32);
    assert_eq!(
        row_arranged(Arrangement::END, 3, 50.0, false),
        [x(root - size * 3.0), x(root - size * 2.0), x(root - size)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withCenterArrangement
#[test]
fn row_with_center_arrangement() {
    let size = round(px(50.0)) as f32;
    let extra = ROOT.width as f32 - size * 3.0;
    assert_eq!(
        row_arranged(Arrangement::CENTER.into(), 3, 50.0, false),
        [x(extra / 2.0), x(extra / 2.0 + size), x(extra / 2.0 + size * 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpaceEvenlyArrangement
#[test]
fn row_with_space_evenly_arrangement() {
    let size = round(px(50.0)) as f32;
    let gap = (ROOT.width as f32 - size * 3.0) / 4.0;
    assert_eq!(
        row_arranged(Arrangement::SPACE_EVENLY.into(), 3, 50.0, false),
        [x(gap), x(size + gap * 2.0), x(size * 2.0 + gap * 3.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpaceBetweenArrangement_singleItem
#[test]
fn row_with_space_between_arrangement_single_item() {
    assert_eq!(row_arranged(Arrangement::SPACE_BETWEEN.into(), 1, 50.0, false), [x(0.0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpaceBetweenArrangement_multipleItems
#[test]
fn row_with_space_between_arrangement_multiple_items() {
    let size = round(px(50.0)) as f32;
    let gap = (ROOT.width as f32 - size * 3.0) / 2.0;
    assert_eq!(
        row_arranged(Arrangement::SPACE_BETWEEN.into(), 3, 50.0, false),
        [x(0.0), x(gap + size), x(gap * 2.0 + size * 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpaceAroundArrangement
#[test]
fn row_with_space_around_arrangement() {
    let size = round(px(50.0)) as f32;
    let gap = (ROOT.width as f32 - size * 3.0) / 3.0;
    assert_eq!(
        row_arranged(Arrangement::SPACE_AROUND.into(), 3, 50.0, false),
        [x(gap / 2.0), x(gap * 3.0 / 2.0 + size), x(gap * 5.0 / 2.0 + size * 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withTopArrangement
#[test]
fn column_with_top_arrangement() {
    let size = round(px(50.0)) as f32;
    assert_eq!(column_arranged(Arrangement::TOP, 3, 50.0), [y(0.0), y(size), y(size * 2.0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withBottomArrangement
#[test]
fn column_with_bottom_arrangement() {
    let (root, size) = (ROOT.height as f32, round(px(50.0)) as f32);
    assert_eq!(
        column_arranged(Arrangement::BOTTOM, 3, 50.0),
        [y(root - size * 3.0), y(root - size * 2.0), y(root - size)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withCenterArrangement
#[test]
fn column_with_center_arrangement() {
    let size = round(px(50.0)) as f32;
    let extra = ROOT.height as f32 - size * 3.0;
    assert_eq!(
        column_arranged(Arrangement::CENTER.into(), 3, 50.0),
        [y(extra / 2.0), y(extra / 2.0 + size), y(extra / 2.0 + size * 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withSpaceEvenlyArrangement
#[test]
fn column_with_space_evenly_arrangement() {
    let size = round(px(50.0)) as f32;
    let gap = (ROOT.height as f32 - size * 3.0) / 4.0;
    assert_eq!(
        column_arranged(Arrangement::SPACE_EVENLY.into(), 3, 50.0),
        [y(gap), y(size + gap * 2.0), y(size * 2.0 + gap * 3.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withSpaceBetweenArrangement
#[test]
fn column_with_space_between_arrangement() {
    let size = round(px(50.0)) as f32;
    let gap = (ROOT.height as f32 - size * 3.0) / 2.0;
    assert_eq!(
        column_arranged(Arrangement::SPACE_BETWEEN.into(), 3, 50.0),
        [y(0.0), y(gap + size), y(gap * 2.0 + size * 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withSpaceAroundArrangement
#[test]
fn column_with_space_around_arrangement() {
    let size = round(px(50.0)) as f32;
    let gap = (ROOT.height as f32 - size * 3.0) / 3.0;
    assert_eq!(
        column_arranged(Arrangement::SPACE_AROUND.into(), 3, 50.0),
        [y(gap / 2.0), y(gap * 3.0 / 2.0 + size), y(gap * 5.0 / 2.0 + size * 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_arrangementCenter
#[test]
fn row_rtl_arrangement_center() {
    let size = 100.0;
    let extra = ROOT.width as f32 - size * 3.0;
    assert_eq!(
        row_arranged(Arrangement::CENTER.into(), 3, 100.0, true),
        [x(extra / 2.0 + size * 2.0), x(extra / 2.0 + size), x(extra / 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_arrangementSpaceEvenly
#[test]
fn row_rtl_arrangement_space_evenly() {
    let size = 100.0;
    let gap = (ROOT.width as f32 - size * 3.0) / 4.0;
    assert_eq!(
        row_arranged(Arrangement::SPACE_EVENLY.into(), 3, 100.0, true),
        [x(size * 2.0 + gap * 3.0), x(size + gap * 2.0), x(gap)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_arrangementSpaceBetween_singleItem
#[test]
fn row_rtl_arrangement_space_between_single_item() {
    let gap = ROOT.width as f32 - 100.0;
    assert_eq!(row_arranged(Arrangement::SPACE_BETWEEN.into(), 1, 100.0, true), [x(gap)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_arrangementSpaceBetween_multipleItems
#[test]
fn row_rtl_arrangement_space_between_multiple_items() {
    let size = 100.0;
    let gap = (ROOT.width as f32 - size * 3.0) / 2.0;
    assert_eq!(
        row_arranged(Arrangement::SPACE_BETWEEN.into(), 3, 100.0, true),
        [x(gap * 2.0 + size * 2.0), x(gap + size), x(0.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_arrangementSpaceAround
#[test]
fn row_rtl_arrangement_space_around() {
    let size = 100.0;
    let gap = (ROOT.width as f32 - size * 3.0) / 3.0;
    assert_eq!(
        row_arranged(Arrangement::SPACE_AROUND.into(), 3, 100.0, true),
        [x(gap * 5.0 / 2.0 + size * 2.0), x(gap * 3.0 / 2.0 + size), x(gap / 2.0)]
    );
}

/// In a right-to-left root, a row (column) filling the root's width with `arrangement` (with
/// `aligned`, each child aligned so), of a 35 px and a 70 px container: their positions in the
/// root.
fn rtl_pair(vertical: bool, arrangement: HorizontalArrangement, aligned: bool) -> [Offset; 2] {
    let probe = Probe::new(2);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        let arrangement = arrangement.clone();
        rtl(move || {
            let p = p.clone();
            let size = px(35.0);
            if vertical {
                Column!(modifier = Modifier.fill_max_width(), |column| {
                    let end = |modifier: Modifier| {
                        if aligned { modifier.align(column, Alignment::END) } else { modifier }
                    };
                    container(end(Modifier.size(size)).then(p.at(0)), Spec::default(), || {});
                    container(end(Modifier.size(size * 2.0)).then(p.at(1)), Spec::default(), || {});
                });
            } else {
                let arrangement = arrangement.clone();
                Row!(
                    modifier = Modifier.fill_max_width(),
                    horizontal_arrangement = arrangement,
                    |_| {
                        container(Modifier.size(size).then(p.at(0)), Spec::default(), || {});
                        container(Modifier.size(size * 2.0).then(p.at(1)), Spec::default(), || {});
                    }
                );
            }
        });
    });
    [probe.position(0), probe.position(1)]
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_arrangementStart
#[test]
fn row_rtl_arrangement_start() {
    let (root, size_dp) = (ROOT.width as f32, px(35.0));
    let size = round(size_dp) as f32;
    assert_eq!(
        rtl_pair(false, Arrangement::START, false),
        [x(root - size), x(root - (to_px(size_dp) * 3.0).round())]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_arrangementEnd
#[test]
fn row_rtl_arrangement_end() {
    let size_dp = px(35.0);
    assert_eq!(rtl_pair(false, Arrangement::END, false), [x(to_px(size_dp) * 2.0), x(0.0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_Rtl_gravityStart
#[test]
fn column_rtl_gravity_start() {
    let (root, size_dp) = (ROOT.width as f32, px(35.0));
    let size = round(size_dp) as f32;
    assert_eq!(
        rtl_pair(true, Arrangement::START, false),
        [x(root - size), Offset::new((root - to_px(size_dp * 2.0)).round(), size)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_Rtl_gravityEnd
#[test]
fn column_rtl_gravity_end() {
    // Compose's test uses 50 px children; the positions don't depend on it.
    let size = round(px(35.0)) as f32;
    assert_eq!(rtl_pair(true, Arrangement::START, true), [x(0.0), y(size)]);
}

/// Children of `child` pixels (`required` sizes, else `size`) in a row (column) of `length`
/// pixels (`required`, or none: its children's) arranged by `arrangement`, in a column (row),
/// right to left with `is_rtl`: the parent's length, and each child's position in it and length.
struct Spaced {
    vertical: bool,
    is_rtl: bool,
    length: Option<f32>,
    child: f32,
    required: bool,
    count: usize,
}

impl Spaced {
    fn row(length: Option<f32>, child: f32, required: bool, count: usize) -> Spaced {
        Spaced { vertical: false, is_rtl: false, length, child, required, count }
    }

    fn column(length: Option<f32>, child: f32, required: bool, count: usize) -> Spaced {
        Spaced { vertical: true, ..Spaced::row(length, child, required, count) }
    }

    fn rtl(self) -> Spaced {
        Spaced { is_rtl: true, ..self }
    }

    fn run_row(self, arrangement: HorizontalArrangement) -> (i32, Vec<(f32, i32)>) {
        self.run(Some(arrangement), None)
    }

    fn run_column(self, arrangement: VerticalArrangement) -> (i32, Vec<(f32, i32)>) {
        self.run(None, Some(arrangement))
    }

    fn run(
        self,
        horizontal: Option<HorizontalArrangement>,
        vertical: Option<VerticalArrangement>,
    ) -> (i32, Vec<(f32, i32)>) {
        let Spaced { vertical: is_column, is_rtl, length, child, required, count } = self;
        let parent = Rc::new(Cell::new(IntSize::new(-1, -1)));
        let children: Vec<_> = (0..count).map(|_| Rc::new(Cell::new((0.0, 0)))).collect();
        let (pa, ch) = (parent.clone(), children.clone());
        let _ui = show(move || {
            let (pa, ch) = (pa.clone(), ch.clone());
            let horizontal = horizontal.clone();
            let vertical = vertical.clone();
            let content = move || {
                let pa = pa.clone();
                let mut modifier =
                    Modifier.on_globally_positioned(move |c: LayoutCoordinates| pa.set(c.size()));
                if let Some(length) = length {
                    modifier = Modifier.required_size(px(length)).then(modifier);
                } else if is_column && is_rtl {
                    // As `testColumn_withSpacedByArrangement_rtl`'s column.
                    modifier = Modifier.fill_max_height().then(modifier);
                }
                let ch = ch.clone();
                let children = Rc::new(move || {
                    for seen in ch.iter() {
                        let seen = seen.clone();
                        let size = if required {
                            Modifier.required_size(px(child))
                        } else {
                            Modifier.size(px(child))
                        };
                        let modifier = size.on_globally_positioned(move |c: LayoutCoordinates| {
                            let (size, at) = (c.size(), c.position_in_parent());
                            seen.set(if is_column {
                                (at.y, size.height)
                            } else {
                                (at.x, size.width)
                            });
                        });
                        Box!(modifier = modifier);
                    }
                });
                if is_column {
                    let arrangement = vertical.clone().unwrap();
                    Row!(|_| {
                        let children = children.clone();
                        Column!(
                            modifier = modifier.clone(),
                            vertical_arrangement = arrangement.clone(),
                            |_| { children() }
                        );
                    });
                } else {
                    let arrangement = horizontal.clone().unwrap();
                    Column!(|_| {
                        let children = children.clone();
                        Row!(
                            modifier = modifier.clone(),
                            horizontal_arrangement = arrangement.clone(),
                            |_| { children() }
                        );
                    });
                }
            };
            if is_rtl { rtl(content) } else { content() }
        });
        let parent = parent.get();
        let length = if is_column { parent.height } else { parent.width };
        (length, children.iter().map(|child| child.get()).collect())
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpacedByArrangement
#[test]
fn row_with_spaced_by_arrangement() {
    let (length, children) =
        Spaced::row(None, 20.0, true, 2).run_row(Arrangement::spaced_by(px(10.0)).into());
    assert_eq!(length, 50);
    assert_eq!(children[0].0, 0.0);
    assert_eq!(children[1].0, 30.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withSpacedByArrangement
#[test]
fn column_with_spaced_by_arrangement() {
    let (length, children) =
        Spaced::column(None, 20.0, true, 2).run_column(Arrangement::spaced_by(px(10.0)).into());
    assert_eq!(length, 50);
    assert_eq!(children[0].0, 0.0);
    assert_eq!(children[1].0, 30.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpacedByAlignedArrangement
#[test]
fn row_with_spaced_by_aligned_arrangement() {
    let (length, children) = Spaced::row(Some(50.0), 20.0, true, 2)
        .run_row(Arrangement::spaced_by((px(10.0), Alignment::END)));
    assert_eq!(length, 50);
    assert_eq!(children[0].0, 50.0 - 10.0 - 20.0 * 2.0);
    assert_eq!(children[1].0, 50.0 - 20.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withSpacedByAlignedArrangement
#[test]
fn column_with_spaced_by_aligned_arrangement() {
    let (length, children) = Spaced::column(Some(50.0), 20.0, true, 2)
        .run_column(Arrangement::spaced_by((px(10.0), Alignment::BOTTOM)));
    assert_eq!(length, 50);
    assert_eq!(children[0].0, 50.0 - 10.0 - 20.0 * 2.0);
    assert_eq!(children[1].0, 50.0 - 20.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpacedByArrangement_insufficientSpace
#[test]
fn row_with_spaced_by_arrangement_insufficient_space() {
    let (length, children) =
        Spaced::row(Some(50.0), 20.0, false, 3).run_row(Arrangement::spaced_by(px(15.0)).into());
    assert_eq!(length, 50);
    assert_eq!(children, [(0.0, 20), (35.0, 15), (50.0, 0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withSpacedByArrangement_insufficientSpace
#[test]
fn column_with_spaced_by_arrangement_insufficient_space() {
    let (length, children) = Spaced::column(Some(50.0), 20.0, false, 3)
        .run_column(Arrangement::spaced_by(px(15.0)).into());
    assert_eq!(length, 50);
    assert_eq!(children, [(0.0, 20), (35.0, 15), (50.0, 0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withAlignedArrangement
#[test]
fn row_with_aligned_arrangement() {
    let (length, children) =
        Spaced::row(Some(50.0), 20.0, false, 2).run_row(Arrangement::aligned(Alignment::END));
    assert_eq!(length, 50);
    assert_eq!(children, [(10.0, 20), (30.0, 20)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withAlignedArrangement
#[test]
fn column_with_aligned_arrangement() {
    let (length, children) = Spaced::column(Some(50.0), 20.0, true, 2)
        .run_column(Arrangement::aligned(Alignment::BOTTOM));
    assert_eq!(length, 50);
    assert_eq!(children[0].0, 10.0);
    assert_eq!(children[1].0, 30.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpacedByArrangement_insufficientSpace_rtl
#[test]
fn row_with_spaced_by_arrangement_insufficient_space_rtl() {
    let (length, children) = Spaced::row(Some(50.0), 20.0, false, 3)
        .rtl()
        .run_row(Arrangement::spaced_by(px(15.0)).into());
    assert_eq!(length, 50);
    assert_eq!(children, [(30.0, 20), (0.0, 15), (0.0, 0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_withSpacedByAlignedArrangement
#[test]
fn row_rtl_with_spaced_by_aligned_arrangement() {
    let (length, children) = Spaced::row(Some(50.0), 20.0, true, 2)
        .rtl()
        .run_row(Arrangement::spaced_by((px(10.0), Alignment::END)));
    assert_eq!(length, 50);
    assert_eq!(children[0].0, 30.0);
    assert_eq!(children[1].0, 0.0);
}

/// D1928: a spaced group centered in a right-to-left row is centered, its last space not
/// counted as free space.
#[test]
fn a_spaced_group_centered_right_to_left_is_centered() {
    let (length, children) = Spaced::row(Some(100.0), 20.0, true, 2)
        .rtl()
        .run_row(Arrangement::spaced_by((px(10.0), Alignment::CENTER_HORIZONTALLY)));
    assert_eq!(length, 100);
    assert_eq!(children[0].0, 55.0);
    assert_eq!(children[1].0, 25.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_withSpacedByArrangement_rtl
#[test]
fn column_with_spaced_by_arrangement_rtl() {
    let (_, children) = Spaced::column(None, 20.0, true, 2)
        .rtl()
        .run_column(Arrangement::spaced_by(px(10.0)).into());
    assert_eq!(children[0].0, 0.0);
    assert_eq!(children[1].0, 30.0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_withSpacedByArrangement_rtl
#[test]
fn row_with_spaced_by_arrangement_rtl() {
    // A row of exactly two children, the space and a buffer of 15 px.
    let probe = Probe::new(3);
    let p = probe.clone();
    let _ui = show(move || {
        let p = p.clone();
        rtl(move || {
            let p = p.clone();
            Column!(|_| {
                let (size, space) = (px(20.0), px(10.0));
                let p = p.clone();
                Row!(
                    modifier = Modifier.required_width(size * 2.0 + space + px(15.0)).then(p.at(2)),
                    horizontal_arrangement = Arrangement::spaced_by(space),
                    |_| {
                        Box!(modifier = Modifier.required_size(size).then(p.at(0)));
                        Box!(modifier = Modifier.required_size(size).then(p.at(1)));
                    }
                );
            });
        });
    });
    let row = probe.position(2).x;
    assert_eq!(probe.position(0).x - row, 20.0 + 10.0 + 15.0);
    assert_eq!(probe.position(1).x - row, 15.0);
}

// ---- Absolute arrangements ---------------------------------------------------------------

fn assert_absolute_left(is_rtl: bool) {
    let size = round(px(100.0)) as f32;
    assert_eq!(
        row_arranged(AbsoluteArrangement::LEFT, 3, 100.0, is_rtl),
        [x(0.0), x(size), x(size * 2.0)]
    );
}

fn assert_absolute_right(is_rtl: bool) {
    let (root, size) = (ROOT.width as f32, round(px(100.0)) as f32);
    assert_eq!(
        row_arranged(AbsoluteArrangement::RIGHT, 3, 100.0, is_rtl),
        [x(root - size * 3.0), x(root - size * 2.0), x(root - size)]
    );
}

fn assert_absolute_center(is_rtl: bool) {
    let size = round(px(100.0)) as f32;
    let extra = ROOT.width as f32 - size * 3.0;
    assert_eq!(
        row_arranged(AbsoluteArrangement::CENTER, 3, 100.0, is_rtl),
        [x(extra / 2.0), x(extra / 2.0 + size), x(extra / 2.0 + size * 2.0)]
    );
}

fn assert_absolute_space_evenly(is_rtl: bool) {
    let size = round(px(100.0)) as f32;
    let gap = (ROOT.width as f32 - size * 3.0) / 4.0;
    assert_eq!(
        row_arranged(AbsoluteArrangement::SPACE_EVENLY, 3, 100.0, is_rtl),
        [x(gap), x(gap * 2.0 + size), x(gap * 3.0 + size * 2.0)]
    );
}

fn assert_absolute_space_between(is_rtl: bool) {
    let size = round(px(100.0)) as f32;
    let gap = (ROOT.width as f32 - size * 3.0) / 2.0;
    assert_eq!(
        row_arranged(AbsoluteArrangement::SPACE_BETWEEN, 3, 100.0, is_rtl),
        [x(0.0), x(gap + size), x(gap * 2.0 + size * 2.0)]
    );
}

fn assert_absolute_space_around(is_rtl: bool) {
    let size = round(px(100.0)) as f32;
    let gap = (ROOT.width as f32 - size * 3.0) / 3.0;
    assert_eq!(
        row_arranged(AbsoluteArrangement::SPACE_AROUND, 3, 100.0, is_rtl),
        [x(gap / 2.0), x(gap * 3.0 / 2.0 + size), x(gap * 5.0 / 2.0 + size * 2.0)]
    );
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_absoluteArrangementLeft
#[test]
fn row_absolute_arrangement_left() {
    assert_absolute_left(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_absoluteArrangementLeft
#[test]
fn row_rtl_absolute_arrangement_left() {
    assert_absolute_left(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_absoluteArrangementRight
#[test]
fn row_absolute_arrangement_right() {
    assert_absolute_right(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_absoluteArrangementRight
#[test]
fn row_rtl_absolute_arrangement_right() {
    assert_absolute_right(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_absoluteArrangementCenter
#[test]
fn row_absolute_arrangement_center() {
    assert_absolute_center(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_absoluteArrangementCenter
#[test]
fn row_rtl_absolute_arrangement_center() {
    assert_absolute_center(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_absoluteArrangementSpaceEvenly
#[test]
fn row_absolute_arrangement_space_evenly() {
    assert_absolute_space_evenly(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Row_absoluteArrangementSpaceEvenly
#[test]
fn row_rtl_absolute_arrangement_space_evenly() {
    assert_absolute_space_evenly(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_absoluteArrangementSpaceBetween
#[test]
fn row_absolute_arrangement_space_between() {
    assert_absolute_space_between(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Row_absoluteArrangementSpaceBetween
#[test]
fn row_rtl_absolute_arrangement_space_between() {
    assert_absolute_space_between(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_absoluteArrangementSpaceAround
#[test]
fn row_absolute_arrangement_space_around() {
    assert_absolute_space_around(false);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_absoluteArrangementSpaceAround
#[test]
fn row_rtl_absolute_arrangement_space_around() {
    assert_absolute_space_around(true);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_Rtl_withSpacedByAlignedAbsoluteArrangement
#[test]
fn row_rtl_with_spaced_by_aligned_absolute_arrangement() {
    let (length, children) = Spaced::row(Some(50.0), 20.0, true, 2)
        .rtl()
        .run_row(AbsoluteArrangement::spaced_by((px(10.0), Alignment::END)));
    assert_eq!(length, 50);
    assert_eq!(children[0].0, 0.0);
    assert_eq!(children[1].0, 30.0);
}

/// With free space in RTL, `AbsoluteArrangement::spaced_by((space, alignment))` keeps items
/// ordered left-to-right while aligning the group in the layout's direction (`Alignment::START`
/// goes to the right, `Alignment::END` to the left), as Compose's `SpacedAligned(rtlMirror = false)`.
#[test]
fn row_rtl_absolute_spaced_by_with_free_space_aligns_group() {
    let (_, children) = Spaced::row(Some(100.0), 20.0, true, 2)
        .rtl()
        .run_row(AbsoluteArrangement::spaced_by((px(10.0), Alignment::START)));
    assert_eq!(children[0].0, 50.0);
    assert_eq!(children[1].0, 80.0);

    let (_, children) = Spaced::row(Some(100.0), 20.0, true, 2)
        .rtl()
        .run_row(AbsoluteArrangement::aligned(Alignment::START));
    assert_eq!(children[0].0, 60.0);
    assert_eq!(children[1].0, 80.0);
}

/// A row (column) in a `ConstrainedBox` of exactly 50 px, centered in the root, of a 30 px
/// spacer: the spacer's size.
fn min_constraints(vertical: bool) -> IntSize {
    measured(move |seen| {
        center(move || {
            let seen = seen.clone();
            constrained_box(DpConstraints::fixed(px(50.0), px(50.0)), Modifier, move || {
                let spacer = Modifier.size(px(30.0)).then(seen.clone());
                if vertical {
                    Column!(|_| Spacer(spacer.clone()));
                } else {
                    Row!(|_| Spacer(spacer.clone()));
                }
            });
        });
    })
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRow_doesNotUseMinConstraintsOnChildren
#[test]
fn row_does_not_use_min_constraints_on_children() {
    let child = round(px(30.0));
    assert_eq!(min_constraints(false), size(child, child));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testColumn_doesNotUseMinConstraintsOnChildren
#[test]
fn column_does_not_use_min_constraints_on_children() {
    let child = round(px(30.0));
    assert_eq!(min_constraints(true), size(child, child));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, scenarioShouldNotCrash
#[test]
fn scenario_should_not_crash() {
    let sized = Rc::new(Cell::new(false));
    let s = sized.clone();
    let _ui = show(move || {
        let s = s.clone();
        Column!(modifier = Modifier.width(IntrinsicSize::Max), |_| {
            let s = s.clone();
            Row!(modifier = Modifier.width(Dp(200.0)), |row| {
                Box!(modifier = Modifier.weight(row, 0.8));
                let s = s.clone();
                Box!(modifier = Modifier.weight(row, 0.2).on_size_changed(move |_| s.set(true)));
            });
        });
    });
    assert!(sized.get());
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnTest.kt, testRowColumnModifiersChain_leftMostWins
#[test]
fn row_column_modifiers_chain_left_most_wins() {
    let height = Rc::new(Cell::new(-1));
    let h = height.clone();
    let _ui = show(move || {
        let h = h.clone();
        Box!(|_| {
            let h = h.clone();
            Column!(modifier = Modifier.height(px(24.0)), |column| {
                let h = h.clone();
                let seen = Modifier
                    .on_globally_positioned(move |c: LayoutCoordinates| h.set(c.size().height));
                empty(Modifier.weight(column, 2.0).weight(column, 1.0).then(seen), Spec::default());
                empty(Modifier.weight(column, 1.0), Spec::default());
            });
        });
    });
    assert_eq!(height.get(), 24 * 2 / 3);
}
