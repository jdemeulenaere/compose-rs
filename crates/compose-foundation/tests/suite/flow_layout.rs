//! `FlowRow` and `FlowColumn` (D1350–D1353): wrapping, maximum items and lines, weights,
//! filling a line across, alignments, arrangements, right to left, and their intrinsics.
//! Adapted from Compose's `FlowRowColumnTest` (density 1: a dp is a pixel). Its tests of the
//! deprecated `overflow` parameter (`…Visible`, `…SeeMore…`, `…Collapse…`) aren't ported.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, IntSize, IntrinsicSize, LayoutCoordinates, LayoutDirection, Rect, UnitExt,
};

fn size_cell() -> Rc<Cell<IntSize>> {
    Rc::new(Cell::new(IntSize::new(-1, -1)))
}

/// Records the size of what follows it, as the tests' `onSizeChanged`.
fn size_into(size: &Rc<Cell<IntSize>>) -> Modifier {
    let size = size.clone();
    Modifier.on_size_changed(move |measured: IntSize| size.set(measured))
}

/// Records each layout's bounds in the root, by index.
type Recorded = Rc<RefCell<Vec<Option<Rect>>>>;

fn recorded(count: usize) -> Recorded {
    Rc::new(RefCell::new(vec![None; count]))
}

fn bounds_into(recorded: &Recorded, index: usize) -> Modifier {
    let recorded = recorded.clone();
    Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
        // Where and how large, as Compose's tests read: `boundsInRoot()` of no height is zeros.
        let (at, size) = (coordinates.position_in_root(), coordinates.size());
        let bounds = Rect::new(at.x, at.y, at.x + size.width as f32, at.y + size.height as f32);
        recorded.borrow_mut()[index] = Some(bounds);
    })
}

fn rect(recorded: &Recorded, index: usize) -> Rect {
    recorded.borrow()[index].unwrap_or_else(|| panic!("item {index} wasn't placed"))
}

/// A rule of `content` in a root of `size` px, at density 1.
fn rule(size: i32, content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(size, size), content)
}

/// A flow row (or column) of `count` boxes of `item` px in a box of `container` px: its size.
fn flow_size(
    row: bool,
    container: i32,
    count: usize,
    item: i32,
    max_items: u32,
    weighted: bool,
) -> IntSize {
    let size = size_cell();
    let recorded = size.clone();
    let _ui = rule(800, move || {
        let recorded = recorded.clone();
        Box!(modifier = Modifier.size((container as f32).dp()), {
            let recorded = recorded.clone();
            if row {
                FlowRow!(
                    modifier = size_into(&recorded),
                    max_items_in_each_row = max_items,
                    |scope| {
                        for _ in 0..count {
                            let modifier = Modifier.size((item as f32).dp());
                            let modifier =
                                if weighted { modifier.weight(scope, 1.0) } else { modifier };
                            Box!(modifier = modifier);
                        }
                    }
                );
            } else {
                FlowColumn!(
                    modifier = size_into(&recorded),
                    max_items_in_each_column = max_items,
                    |scope| {
                        for _ in 0..count {
                            let modifier = Modifier.size((item as f32).dp());
                            let modifier =
                                if weighted { modifier.weight(scope, 1.0) } else { modifier };
                            Box!(modifier = modifier);
                        }
                    }
                );
            }
        });
    });
    size.get()
}

fn across(row: bool, size: IntSize) -> i32 {
    if row { size.height } else { size.width }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_wrapsToTheNextLine
// and testFlowColumn_wrapsToTheNextLine
#[test]
fn a_flow_wraps_onto_the_next_line() {
    for row in [true, false] {
        assert_eq!(across(row, flow_size(row, 100, 6, 20, u32::MAX, false)), 40);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_wrapsToTheNextLine_withExactSpaceNeeded
// and testFlowColumn_wrapsToTheNextLine_withExactSpaceNeeded
#[test]
fn a_flow_fills_a_line_exactly_before_wrapping() {
    for row in [true, false] {
        assert_eq!(across(row, flow_size(row, 100, 10, 20, u32::MAX, false)), 40);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_wrapsToTheNextLineMultipleTimes
// and testFlowColumn_wrapsToTheNextLineMultipleTimes
#[test]
fn a_flow_wraps_as_often_as_it_needs() {
    for row in [true, false] {
        assert_eq!(across(row, flow_size(row, 60, 6, 20, u32::MAX, false)), 40);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_wrapsWithMaxItems,
// testFlowColumn_wrapsWithMaxItems, testFlowRow_wrapsWithWeights and testFlowColumn_wrapsWithWeights
#[test]
fn a_flow_wraps_after_its_maximum_items_weighted_or_not() {
    for row in [true, false] {
        for weighted in [false, true] {
            assert_eq!(
                across(row, flow_size(row, 60, 6, 20, 2, weighted)),
                60,
                "weighted {weighted}"
            );
        }
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_staysInOneRow
// and testFlowColumn_staysInOneRow
#[test]
fn a_flow_that_fits_stays_on_one_line() {
    for row in [true, false] {
        assert_eq!(across(row, flow_size(row, 50, 2, 20, u32::MAX, false)), 20);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_wrapsToTheNextLine_Rounding
// and testFlowColumn_wrapsToTheNextLine_Rounding
#[test]
fn a_flow_wraps_what_doesnt_quite_fit() {
    for row in [true, false] {
        assert_eq!(across(row, flow_size(row, 50, 3, 20, u32::MAX, false)), 40);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_empty
// and testFlowColumn_empty
#[test]
fn an_empty_flow_has_no_size() {
    for row in [true, false] {
        assert_eq!(flow_size(row, 100, 0, 20, u32::MAX, false), IntSize::new(0, 0));
    }
}

/// `FlowRowColumnTest`'s equal-height scene: nine items in rows of three, each filling its
/// row's height (as tall as a child 20 px per index, or `weighted`), 10 px apart, rows 20 px
/// apart: the items' heights (or widths, for columns).
fn filled_cross_sizes(row: bool, weighted: bool) -> Vec<i32> {
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let recorded = sizes.clone();
    let _ui = rule(800, move || {
        let recorded = recorded.clone();
        if row {
            FlowRow!(
                modifier = Modifier
                    .fill_max_width()
                    .padding(20.dp())
                    .wrap_content_height_with(Alignment::TOP, false),
                horizontal_arrangement = Arrangement::spaced_by(10.dp()),
                vertical_arrangement = Arrangement::spaced_by(20.dp()),
                max_items_in_each_row = 3,
                |scope| {
                    for index in 0..9 {
                        let recorded = recorded.clone();
                        let mut modifier = Modifier
                            .on_size_changed(move |size: IntSize| {
                                recorded.borrow_mut().push(size.height)
                            })
                            .width(100.dp());
                        if weighted {
                            modifier = modifier.weight(scope, 1.0);
                        }
                        let child = if weighted { index as f32 } else { 20.0 * index as f32 };
                        Box!(modifier = modifier.fill_max_row_height(scope), {
                            Box!(modifier = Modifier.height(child.dp()));
                        });
                    }
                }
            );
        } else {
            FlowColumn!(
                modifier = Modifier
                    .wrap_content_width_with(Alignment::START, false)
                    .padding(20.dp())
                    .fill_max_height(),
                horizontal_arrangement = Arrangement::spaced_by(20.dp()),
                vertical_arrangement = Arrangement::spaced_by(10.dp()),
                max_items_in_each_column = 3,
                |scope| {
                    for index in 0..9 {
                        let recorded = recorded.clone();
                        let mut modifier = Modifier
                            .on_size_changed(move |size: IntSize| {
                                recorded.borrow_mut().push(size.width)
                            })
                            .height(100.dp());
                        if weighted {
                            modifier = modifier.weight(scope, 1.0);
                        }
                        Box!(modifier = modifier.fill_max_column_width(scope), {
                            Box!(modifier = Modifier.width((20.0 * index as f32).dp()));
                        });
                    }
                }
            );
        }
    });
    sizes.borrow().clone()
}

fn assert_equal_by_threes(sizes: &[i32]) {
    assert_eq!(sizes.len(), 9, "{sizes:?}");
    assert_eq!(sizes[0], sizes[1]);
    assert_eq!(sizes[1], sizes[2]);
    assert_ne!(sizes[2], sizes[3]);
    assert_eq!(sizes[3], sizes[4]);
    assert_eq!(sizes[4], sizes[5]);
    assert_ne!(sizes[5], sizes[6]);
    assert_eq!(sizes[6], sizes[7]);
    assert_eq!(sizes[7], sizes[8]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_equalHeight
// and testFlowColumn_equalWidth
#[test]
fn items_filling_their_line_across_are_as_large_as_its_largest() {
    assert_equal_by_threes(&filled_cross_sizes(true, false));
    assert_equal_by_threes(&filled_cross_sizes(false, false));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_equalHeight_worksWithWeight
// and testFlowColumn_equalWidth_worksWithWeight
#[test]
fn weighted_items_fill_their_line_across_too() {
    assert_equal_by_threes(&filled_cross_sizes(true, true));
    assert_equal_by_threes(&filled_cross_sizes(false, true));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_fillMaxRowHeightWithZero
#[test]
fn items_filling_none_of_their_row_have_no_height() {
    let heights = Rc::new(RefCell::new(Vec::new()));
    let final_size = size_cell();
    let (recorded, total) = (heights.clone(), final_size.clone());
    let _ui = rule(800, move || {
        let (recorded, total) = (recorded.clone(), total.clone());
        FlowRow!(
            modifier = Modifier
                .fill_max_width()
                .wrap_content_height_with(Alignment::TOP, false)
                .then(size_into(&total)),
            horizontal_arrangement = Arrangement::spaced_by(10.dp()),
            max_items_in_each_row = 3,
            |scope| {
                for _ in 0..9 {
                    let recorded = recorded.clone();
                    Box!(
                        modifier = Modifier
                            .on_size_changed(move |size: IntSize| recorded
                                .borrow_mut()
                                .push(size.height))
                            .width(100.dp())
                            .fill_max_row_height_with(scope, 0.0)
                    );
                }
            }
        );
    });
    assert_eq!(*heights.borrow(), vec![0; 9]);
    assert_eq!(final_size.get().height, 0);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_fillMaxRowHeightWithZero_InSome
#[test]
fn some_items_filling_none_of_their_row_leave_the_others_alone() {
    let heights = Rc::new(RefCell::new(vec![-1; 9]));
    let final_size = size_cell();
    let (recorded, total) = (heights.clone(), final_size.clone());
    let _ui = rule(800, move || {
        let (recorded, total) = (recorded.clone(), total.clone());
        FlowRow!(
            modifier = Modifier
                .fill_max_width()
                .padding(20.dp())
                .wrap_content_height_with(Alignment::TOP, false)
                .then(size_into(&total)),
            horizontal_arrangement = Arrangement::spaced_by(10.dp()),
            max_items_in_each_row = 3,
            |scope| {
                for index in 0..9 {
                    let recorded = recorded.clone();
                    let modifier = Modifier
                        .on_size_changed(move |size: IntSize| {
                            recorded.borrow_mut()[index] = size.height
                        })
                        .width(100.dp());
                    let modifier = if index % 3 == 0 {
                        modifier.fill_max_row_height_with(scope, 0.0)
                    } else {
                        modifier
                    };
                    Box!(modifier = modifier, {
                        Box!(modifier = Modifier.height(20.dp()));
                    });
                }
            }
        );
    });
    let expected: Vec<i32> = (0..9).map(|index| if index % 3 == 0 { 0 } else { 20 }).collect();
    assert_eq!(*heights.borrow(), expected);
    assert_eq!(final_size.get().height, 60);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_equalHeight_WithFraction
// and testFlowColumn_equalWidth_fraction
#[test]
fn items_fill_a_fraction_of_their_line_across() {
    for row in [true, false] {
        let placed = recorded(9);
        let recorded = placed.clone();
        let _ui = rule(4000, move || {
            let recorded = recorded.clone();
            let item = move |scope_fill: Modifier, index: usize| {
                Box!(modifier = scope_fill.then(bounds_into(&recorded, index)));
            };
            if row {
                FlowRow!(
                    modifier = Modifier
                        .fill_max_width()
                        .padding(20.dp())
                        .wrap_content_height_with(Alignment::TOP, true),
                    horizontal_arrangement = Arrangement::spaced_by(10.dp()),
                    vertical_arrangement = Arrangement::spaced_by(20.dp()),
                    max_items_in_each_row = 3,
                    |scope| {
                        for index in 0..9 {
                            let modifier = Modifier.width(100.dp());
                            let modifier = if index % 3 == 0 {
                                modifier.fill_max_row_height_with(scope, 0.5)
                            } else {
                                modifier.height((200.0 * index as f32).dp())
                            };
                            item(modifier, index);
                        }
                    }
                );
            } else {
                FlowColumn!(
                    modifier = Modifier
                        .wrap_content_width_with(Alignment::START, true)
                        .padding(20.dp())
                        .fill_max_width(),
                    horizontal_arrangement = Arrangement::spaced_by(20.dp()),
                    vertical_arrangement = Arrangement::spaced_by(10.dp()),
                    max_items_in_each_column = 3,
                    |scope| {
                        for index in 0..9 {
                            let modifier = Modifier.height(100.dp());
                            let modifier = if index % 3 == 0 {
                                modifier.fill_max_column_width_with(scope, 0.5)
                            } else {
                                modifier.width((200.0 * index as f32).dp())
                            };
                            item(modifier, index);
                        }
                    }
                );
            }
        });
        let size = |index| {
            let bounds = rect(&placed, index);
            (if row { bounds.height() } else { bounds.width() }) as i32
        };
        assert_eq!(size(0), (0.5 * size(2) as f32).round() as i32);
        assert_ne!(size(1), size(2));
        assert_eq!(size(2), 400);
        assert_eq!(size(3), (0.5 * size(5) as f32).round() as i32);
        assert_eq!(size(5), 1000);
        assert_eq!(size(6), (0.5 * size(8) as f32).round() as i32);
        assert_eq!(size(8), 1600);
    }
}

/// Five items 20 px along a flow in a 200 px box, the last 10 px across: the last's offset
/// across, with `align` on each item or the flow's item alignment `centered`.
fn last_items_cross_offset(row: bool, align_each: bool, centered: bool) -> f32 {
    let placed = recorded(5);
    let recorded = placed.clone();
    let _ui = rule(400, move || {
        let recorded = recorded.clone();
        Box!(modifier = Modifier.size(200.dp()), {
            let recorded = recorded.clone();
            if row {
                let alignment =
                    if centered { Alignment::CENTER_VERTICALLY } else { Alignment::TOP };
                FlowRow!(item_vertical_alignment = alignment, |scope| {
                    for index in 0..5 {
                        let height = if index == 4 { 10.dp() } else { 20.dp() };
                        let modifier = if align_each {
                            Modifier.align(scope, Alignment::CENTER_VERTICALLY)
                        } else {
                            Modifier
                        };
                        Box!(
                            modifier = modifier
                                .size((20.dp(), height))
                                .then(bounds_into(&recorded, index))
                        );
                    }
                });
            } else {
                let alignment =
                    if centered { Alignment::CENTER_HORIZONTALLY } else { Alignment::START };
                FlowColumn!(item_horizontal_alignment = alignment, |scope| {
                    for index in 0..5 {
                        let width = if index == 4 { 10.dp() } else { 20.dp() };
                        let modifier = if align_each {
                            Modifier.align(scope, Alignment::CENTER_HORIZONTALLY)
                        } else {
                            Modifier
                        };
                        Box!(
                            modifier =
                                modifier.size((width, 20.dp())).then(bounds_into(&recorded, index))
                        );
                    }
                });
            }
        });
    });
    let bounds = rect(&placed, 4);
    if row { bounds.top } else { bounds.left }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_alignItemsDefaultsToLeft,
// testFlowRow_alignItemsCenterVertically, testFlowRow_alignItemsCenterVertically_UsingTopLevelAPI, testFlowColumn_alignItemsDefaultsToTop,
// testFlowColumn_alignItemsCenterHorizontally and testFlowColumn_alignItemsCenterHorizontally_UsingTopLevelAPI
// (the custom `Alignment.Vertical`/`Horizontal` objects of `testRow_withCustomVertical_alignment` and its three siblings wait on
// custom alignments, feature:alignment-api)
#[test]
fn items_align_across_their_line_by_their_own_alignment_or_the_flows() {
    for row in [true, false] {
        assert_eq!(last_items_cross_offset(row, false, false), 0.0);
        assert_eq!(last_items_cross_offset(row, true, false), 5.0);
        assert_eq!(last_items_cross_offset(row, false, true), 5.0);
    }
}

// ---- Aligned by a line ---------------------------------------------------------------------

/// A layout `size` long along the flow and `across` across, giving `line` at `at`.
fn with_a_line(
    width: i32,
    height: i32,
    line: impl Into<compose_ui::AlignmentLine>,
    at: i32,
    modifier: Modifier,
) {
    let line = line.into();
    let policy = compose_ui::measure_policy(move |scope, _, _| {
        scope.provide_alignment_lines([(line, at)]);
        scope.layout(width, height)
    });
    compose_ui::Layout(modifier, policy, callback(|| {}));
}

/// Each line's items share a line of their own, and each line of the flow's as long across as
/// what's before and after it takes.
// Compose: compose/foundation/foundation-layout/src/commonMain/kotlin/androidx/compose/foundation/layout/FlowLayout.kt, FlowRow
#[test]
fn a_flow_rows_items_align_by_a_line_in_their_row() {
    let placed = recorded(4);
    let recorded = placed.clone();
    let size = size_cell();
    let measured = size.clone();
    let line = compose_ui::HorizontalAlignmentLine::new(i32::min);
    let _ui = rule(400, move || {
        let recorded = recorded.clone();
        let flow = size_into(&measured);
        FlowRow!(modifier = flow, max_items_in_each_row = 2, |scope| {
            let at = |index| Modifier.align_by(scope, line).then(bounds_into(&recorded, index));
            with_a_line(10, 30, line, 25, at(0));
            with_a_line(10, 30, line, 5, at(1));
            with_a_line(10, 20, line, 15, at(2));
            with_a_line(10, 20, line, 10, at(3));
        });
    });
    // The first row: 25 before its line, 25 after it, past the 30 either takes.
    assert_eq!((rect(&placed, 0).top, rect(&placed, 1).top), (0.0, 20.0));
    // The second, 15 before and 10 after, below the first.
    assert_eq!((rect(&placed, 2).top, rect(&placed, 3).top), (50.0, 55.0));
    assert_eq!(size.get().height, 50 + 25);
}

/// `align_by_baseline`, `align_by_block` and an item with no line, which goes to the row's top.
// Compose: compose/foundation/foundation-layout/src/commonMain/kotlin/androidx/compose/foundation/layout/FlowLayout.kt, FlowRow
#[test]
fn a_flow_row_mixes_baselines_blocks_and_its_own_alignment() {
    let placed = recorded(4);
    let recorded = placed.clone();
    let line = compose_ui::HorizontalAlignmentLine::new(i32::min);
    let _ui = rule(400, move || {
        let recorded = recorded.clone();
        FlowRow!(item_vertical_alignment = Alignment::BOTTOM, |scope| {
            with_a_line(
                10,
                20,
                compose_ui::FIRST_BASELINE,
                16,
                Modifier.align_by_baseline(scope).then(bounds_into(&recorded, 0)),
            );
            Box!(
                modifier = Modifier
                    .align_by_block(scope, |measured| measured.measured_height() / 2)
                    .size(10.dp())
                    .then(bounds_into(&recorded, 1))
            );
            // Asked for a line it doesn't give.
            Box!(
                modifier =
                    Modifier.align_by(scope, line).size(10.dp()).then(bounds_into(&recorded, 2))
            );
            // None asked: the flow's alignment, in the row those make.
            Box!(modifier = Modifier.size(10.dp()).then(bounds_into(&recorded, 3)));
        });
    });
    assert_eq!(rect(&placed, 0).top, 0.0);
    assert_eq!(rect(&placed, 1).top, 16.0 - 5.0);
    assert_eq!(rect(&placed, 2).top, 0.0);
    // 16 before the line and 5 after it.
    assert_eq!(rect(&placed, 3).top, 21.0 - 10.0);
}

/// Across a column, a vertical line; right to left, from the right edge of the column's width.
// Compose: compose/foundation/foundation-layout/src/commonMain/kotlin/androidx/compose/foundation/layout/FlowLayout.kt, FlowColumn
#[test]
fn a_flow_columns_items_align_by_a_line_in_their_column_either_way() {
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let placed = recorded(2);
        let recorded = placed.clone();
        let size = size_cell();
        let measured = size.clone();
        let line = compose_ui::VerticalAlignmentLine::new(i32::min);
        let _ui = rule(400, move || {
            let (recorded, measured) = (recorded.clone(), measured.clone());
            CompositionLocalProvider(
                compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
                || {
                    let recorded = recorded.clone();
                    let flow = size_into(&measured);
                    FlowColumn!(modifier = flow, |scope| {
                        let at = |index| {
                            Modifier.align_by(scope, line).then(bounds_into(&recorded, index))
                        };
                        with_a_line(30, 10, line, 25, at(0));
                        with_a_line(30, 10, line, 5, at(1));
                    });
                },
            );
        });
        // 25 before the line, 25 after it.
        assert_eq!(size.get().width, 50);
        let (first, second) = if direction == LayoutDirection::Ltr {
            (0.0, 20.0)
        } else {
            (50.0 - 30.0, 50.0 - 30.0 - 20.0)
        };
        assert_eq!(rect(&placed, 0).left, first);
        assert_eq!(rect(&placed, 1).left, second);
    }
}

/// An item without the line it asks for goes to 0 across its column: the left, either way, which
/// is the far side right to left.
// Compose: compose/foundation/foundation-layout/src/commonMain/kotlin/androidx/compose/foundation/layout/RowColumnImpl.kt, AlignmentLineCrossAxisAlignment
#[test]
fn a_flow_column_item_without_its_line_stays_at_the_left_either_way() {
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let placed = recorded(1);
        let recorded = placed.clone();
        let size = size_cell();
        let measured = size.clone();
        let line = compose_ui::VerticalAlignmentLine::new(i32::min);
        let _ui = rule(400, move || {
            let (recorded, measured) = (recorded.clone(), measured.clone());
            CompositionLocalProvider(
                compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
                || {
                    let recorded = recorded.clone();
                    let flow = size_into(&measured);
                    FlowColumn!(modifier = flow, |scope| {
                        with_a_line(30, 10, line, 25, Modifier.align_by(scope, line));
                        with_a_line(30, 10, line, 5, Modifier.align_by(scope, line));
                        Box!(
                            modifier = Modifier
                                .align_by(scope, line)
                                .size(10.dp())
                                .then(bounds_into(&recorded, 0))
                        );
                    });
                },
            );
        });
        assert_eq!(size.get().width, 50);
        assert_eq!(rect(&placed, 0).left, 0.0, "{direction:?}");
    }
}

/// A line's move places what's aligned by it again, without anything else changing.
// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/RowColumnModifierTest.kt, row_alignByModifierChange
#[test]
fn a_flow_row_follows_a_change_of_a_line() {
    let placed = recorded(2);
    let recorded = placed.clone();
    let position = Rc::new(mutable_state_of(5));
    let changed = position.clone();
    let line = compose_ui::HorizontalAlignmentLine::new(i32::min);
    let mut ui = rule(400, move || {
        let (recorded, position) = (recorded.clone(), position.clone());
        FlowRow!(|scope| {
            let position = position.clone();
            with_a_line(
                10,
                30,
                line,
                25,
                Modifier.align_by(scope, line).then(bounds_into(&recorded, 0)),
            );
            let policy = compose_ui::measure_policy(move |scope, _, _| {
                scope.provide_alignment_lines([(line, position.get())]);
                scope.layout(10, 30)
            });
            compose_ui::Layout(
                Modifier.align_by(scope, line).then(bounds_into(&recorded, 1)),
                policy,
                callback(|| {}),
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(rect(&placed, 1).top, 20.0);
    changed.set(15);
    ui.wait_for_idle();
    assert_eq!(rect(&placed, 1).top, 10.0);
}

// ---- Arrangements, right to left, clipping and intrinsics ---------------------------------------

/// A flow layout scene: a flow row (or column) in `container`, with its arrangements, maxima
/// and items (width, height in dp).
#[derive(Clone)]
struct Scene {
    row: bool,
    rtl: bool,
    container: Modifier,
    modifier: Modifier,
    horizontal: HorizontalArrangement,
    vertical: VerticalArrangement,
    max_items: u32,
    max_lines: u32,
    items: Vec<(f32, f32)>,
    /// Whether the items' sizes are required (`requiredSize`) rather than coerced.
    required: bool,
}

impl Scene {
    fn row(items: usize) -> Scene {
        Scene {
            row: true,
            rtl: false,
            container: Modifier.size(200.dp()),
            modifier: Modifier,
            horizontal: Arrangement::START,
            vertical: Arrangement::TOP,
            max_items: u32::MAX,
            max_lines: u32::MAX,
            items: vec![(20.0, 20.0); items],
            required: false,
        }
    }

    fn column(items: usize) -> Scene {
        Scene { row: false, ..Scene::row(items) }
    }
}

/// The flow's bounds in the root, and each item's bounds in the flow (`None` if not placed).
struct Laid {
    flow: Rect,
    items: Vec<Option<Rect>>,
}

impl Laid {
    fn item(&self, index: usize) -> Rect {
        self.items[index].unwrap_or_else(|| panic!("item {index} wasn't placed"))
    }
    fn placed(&self) -> usize {
        self.items.iter().filter(|item| item.is_some()).count()
    }
}

fn lay_out(scene: Scene) -> Laid {
    let count = scene.items.len();
    let placed = recorded(count);
    let flow = recorded(1);
    let (items, bounds) = (placed.clone(), flow.clone());
    let _ui = rule(800, move || {
        let (scene, items, bounds) = (scene.clone(), items.clone(), bounds.clone());
        let direction = if scene.rtl { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
        CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
            move || {
                let (scene, items, bounds) = (scene.clone(), items.clone(), bounds.clone());
                Box!(modifier = scene.container.clone(), {
                    let (scene, items) = (scene.clone(), items.clone());
                    let modifier = scene.modifier.clone().then(bounds_into(&bounds, 0));
                    let children = move || {
                        for (index, &(width, height)) in scene.items.iter().enumerate() {
                            let size = (width.dp(), height.dp());
                            let sized = if scene.required {
                                Modifier.required_size(size)
                            } else {
                                Modifier.size(size)
                            };
                            Box!(modifier = sized.then(bounds_into(&items, index)));
                        }
                    };
                    if scene.row {
                        FlowRow!(
                            modifier = modifier,
                            horizontal_arrangement = scene.horizontal.clone(),
                            vertical_arrangement = scene.vertical.clone(),
                            max_items_in_each_row = scene.max_items,
                            max_lines = scene.max_lines,
                            content = move |_| children()
                        );
                    } else {
                        FlowColumn!(
                            modifier = modifier,
                            vertical_arrangement = scene.vertical.clone(),
                            horizontal_arrangement = scene.horizontal.clone(),
                            max_items_in_each_column = scene.max_items,
                            max_lines = scene.max_lines,
                            content = move |_| children()
                        );
                    }
                });
            },
        );
    });
    let flow = rect(&flow, 0);
    let items = placed
        .borrow()
        .iter()
        .map(|item| {
            item.map(|item| {
                Rect::new(
                    item.left - flow.left,
                    item.top - flow.top,
                    item.right - flow.left,
                    item.bottom - flow.top,
                )
            })
        })
        .collect();
    Laid { flow, items }
}

/// The items' main-axis positions in the flow.
fn mains(laid: &Laid, row: bool) -> Vec<i32> {
    (0..laid.items.len())
        .map(|index| {
            let item = laid.item(index);
            (if row { item.left } else { item.top }) as i32
        })
        .collect()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_horizontalArrangementSpaceAround,
// testFlowColumn_verticalArrangementSpaceAround, testFlowRow_horizontalArrangementSpaceAround_withTwoRows and
// testFlowColumn_verticalArrangementSpaceAround_withTwoColumns
#[test]
fn space_around_shares_a_lines_room_around_its_items() {
    for row in [true, false] {
        for (count, max_items) in [(5, u32::MAX), (10, 5)] {
            let mut scene = if row { Scene::row(count) } else { Scene::column(count) };
            scene.max_items = max_items;
            if row {
                scene.modifier = Modifier.fill_max_width();
                scene.horizontal = Arrangement::SPACE_AROUND.into();
            } else {
                scene.modifier = Modifier.fill_max_height();
                scene.vertical = Arrangement::SPACE_AROUND.into();
            }
            // 100 px left over five items: 10 px on either side of each.
            let expected: Vec<i32> = (0..count).map(|index| 10 + (index % 5) as i32 * 40).collect();
            assert_eq!(mains(&lay_out(scene), row), expected, "row {row}, {count} items");
        }
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_horizontalArrangementEnd
// and testFlowColumn_verticalArrangementBottom
#[test]
fn end_arrangement_puts_each_line_at_the_end() {
    for row in [true, false] {
        let mut scene = if row { Scene::row(10) } else { Scene::column(10) };
        scene.max_items = 5;
        if row {
            scene.modifier = Modifier.fill_max_width();
            scene.horizontal = Arrangement::END;
        } else {
            scene.modifier = Modifier.fill_max_height();
            scene.vertical = Arrangement::BOTTOM;
        }
        let expected: Vec<i32> = (0..10).map(|index| 100 + (index % 5) * 20).collect();
        assert_eq!(mains(&lay_out(scene), row), expected);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_horizontalArrangementStart
// and testFlowColumn_verticalArrangementTop
#[test]
fn start_arrangement_puts_each_line_at_the_start() {
    for row in [true, false] {
        let mut scene = if row { Scene::row(10) } else { Scene::column(10) };
        scene.max_items = 5;
        if !row {
            scene.modifier = Modifier.fill_max_height();
        }
        let expected: Vec<i32> = (0..10).map(|index| (index % 5) * 20).collect();
        assert_eq!(mains(&lay_out(scene), row), expected);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_SpaceAligned
// and testFlowColumn_SpaceAligned
#[test]
fn spaced_by_spaces_the_items_of_each_line() {
    for row in [true, false] {
        let mut scene = if row { Scene::row(10) } else { Scene::column(10) };
        scene.max_items = 5;
        scene.items = vec![(10.0, 10.0); 10];
        if row {
            scene.horizontal = Arrangement::spaced_by(10.dp()).into();
        } else {
            scene.vertical = Arrangement::spaced_by(10.dp()).into();
        }
        let expected: Vec<i32> = (0..10).map(|index| (index % 5) * 20).collect();
        assert_eq!(mains(&lay_out(scene), row), expected);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_crossAxisSpacedBy
// and testFlowColumn_crossAxisSpacedBy
#[test]
fn spaced_by_across_spaces_the_lines() {
    for row in [true, false] {
        let mut scene = if row { Scene::row(3) } else { Scene::column(3) };
        scene.max_items = 1;
        if row {
            scene.vertical = Arrangement::spaced_by(20.dp()).into();
        } else {
            scene.horizontal = Arrangement::spaced_by(20.dp()).into();
        }
        let laid = lay_out(scene);
        assert_eq!(if row { laid.flow.height() } else { laid.flow.width() }, 100.0);
        let across: Vec<i32> = (0..3)
            .map(|index| {
                let item = laid.item(index);
                (if row { item.top } else { item.left }) as i32
            })
            .collect();
        assert_eq!(across, vec![0, 40, 80]);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_SpaceAligned_notExact
// and testFlowColumn_SpaceAligned_notExact
#[test]
fn spaced_by_wraps_what_the_spacing_leaves_no_room_for() {
    for row in [true, false] {
        let mut scene = if row { Scene::row(10) } else { Scene::column(10) };
        scene.max_items = 5;
        scene.items = vec![(10.0, 10.0); 10];
        if row {
            scene.container = Modifier.width_in(30.dp(), 40.dp());
            scene.horizontal = Arrangement::spaced_by(10.dp()).into();
        } else {
            scene.container = Modifier.height_in(30.dp(), 40.dp());
            scene.vertical = Arrangement::spaced_by(10.dp()).into();
        }
        let laid = lay_out(scene);
        assert_eq!(if row { laid.flow.width() } else { laid.flow.height() }, 30.0);
        let expected: Vec<i32> = (0..10).map(|index| (index % 2) * 20).collect();
        assert_eq!(mains(&laid, row), expected);
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_horizontalArrangementStart_rtl_fillMaxWidth
// and testFlowRow_horizontalArrangementStart_rtl_wrap
#[test]
fn a_flow_row_starts_its_rows_on_the_right_right_to_left() {
    for (fill, width) in [(true, 200), (false, 100)] {
        let mut scene = Scene::row(6);
        scene.rtl = true;
        scene.max_items = 5;
        if fill {
            scene.modifier = Modifier.fill_max_width();
        }
        let expected: Vec<i32> = (0..6).map(|index| width - 20 - (index % 5) * 20).collect();
        let laid = lay_out(scene);
        let laid_x: Vec<i32> = (0..6).map(|index| laid.item(index).left as i32).collect();
        assert_eq!(laid_x, expected, "fill {fill}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowColumn_verticalArrangementTop_rtl_fillMaxWidth
// and testFlowColumn_verticalArrangementTop_rtl_wrapContentWidth
#[test]
fn a_flow_column_starts_its_columns_on_the_right_right_to_left() {
    for fill in [true, false] {
        let mut scene = Scene::column(10);
        scene.rtl = true;
        scene.max_items = 5;
        scene.modifier = if fill {
            Modifier.fill_max_height().fill_max_width()
        } else {
            Modifier.fill_max_height()
        };
        let laid = lay_out(scene);
        let width = laid.flow.width() as i32;
        assert_eq!(width, if fill { 200 } else { 40 });
        for index in 0..10 {
            let item = laid.item(index);
            assert_eq!(item.top as i32, (index as i32 % 5) * 20, "item {index}");
            assert_eq!(item.left as i32, width - 20 - (index as i32 / 5) * 20, "item {index}");
        }
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_MaxLinesClipped,
// testFlowColumn_MaxLinesClipped, testFlowRow_MaxHeightClipped and testFlowColumn_MaxWidthClipped
// (the default overflow, `Clip`, without its deprecated parameter)
#[test]
fn lines_past_the_maximum_or_the_room_across_are_left_out() {
    for row in [true, false] {
        // Two lines of three 50 px items 20 px apart in 200 px.
        let mut scene = if row { Scene::row(10) } else { Scene::column(10) };
        scene.items = vec![(50.0, 50.0); 10];
        scene.max_lines = 2;
        scene.container = Modifier;
        if row {
            scene.modifier = Modifier.width(200.dp());
            scene.horizontal = Arrangement::spaced_by(20.dp()).into();
        } else {
            scene.modifier = Modifier.height(200.dp());
            scene.vertical = Arrangement::spaced_by(20.dp()).into();
        }
        assert_eq!(lay_out(scene).placed(), 6, "max lines, row {row}");
        // 120 px across holds two lines of 50 px, 20 px apart.
        let mut scene = if row { Scene::row(10) } else { Scene::column(10) };
        scene.items = vec![(50.0, 50.0); 10];
        scene.container = Modifier;
        scene.horizontal = Arrangement::spaced_by(20.dp()).into();
        scene.vertical = Arrangement::spaced_by(20.dp()).into();
        scene.modifier = if row {
            Modifier.width(200.dp()).height(120.dp())
        } else {
            Modifier.height(200.dp()).width(120.dp())
        };
        assert_eq!(lay_out(scene).placed(), 6, "room across, row {row}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_constrainsOverflow
// and testFlowColumn_constrainsOverflow
#[test]
fn items_larger_than_the_room_across_are_left_out() {
    for row in [true, false] {
        let mut scene = if row { Scene::row(2) } else { Scene::column(2) };
        scene.items = vec![(250.0, 250.0); 2];
        scene.required = true;
        if row {
            scene.modifier = Modifier.fill_max_width();
            scene.vertical = Arrangement::spaced_by(20.dp()).into();
        } else {
            scene.modifier = Modifier.fill_max_height();
            scene.horizontal = Arrangement::spaced_by(20.dp()).into();
        }
        let laid = lay_out(scene);
        assert_eq!(if row { laid.flow.width() } else { laid.flow.height() }, 200.0);
        assert_eq!(laid.placed(), 0);
    }
}

/// A flow's size under `modifier` (its intrinsic sizes) in a box of `container` px.
fn intrinsic_flow(mut scene: Scene, container: f32, modifier: Modifier) -> IntSize {
    scene.container = Modifier.size(container.dp());
    scene.modifier = modifier;
    let laid = lay_out(scene);
    IntSize::new(laid.flow.width() as i32, laid.flow.height() as i32)
}

fn with_items(mut scene: Scene, items: Vec<(f32, f32)>) -> Scene {
    scene.items = items;
    scene
}

fn with_max(mut scene: Scene, max_items: u32) -> Scene {
    scene.max_items = max_items;
    scene
}

fn spaced(mut scene: Scene, horizontal: f32, vertical: f32) -> Scene {
    if horizontal > 0.0 {
        scene.horizontal = Arrangement::spaced_by(horizontal.dp()).into();
    }
    if vertical > 0.0 {
        scene.vertical = Arrangement::spaced_by(vertical.dp()).into();
    }
    scene
}

fn widths_of(count: usize, wide: usize, row: bool) -> Vec<(f32, f32)> {
    (0..count)
        .map(|index| {
            let long = if index == wide { 100.0 } else { 20.0 };
            if row { (long, 20.0) } else { (20.0, long) }
        })
        .collect()
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, the intrinsics tests:
// testFlowRow_minIntrinsicWidth, testFlowColumn_minIntrinsicWidth, testFlowColumn_minIntrinsicWidth_wrap, testFlowRow_maxIntrinsicWidth,
// testFlowColumn_maxIntrinsicWidth, testFlowRow_minIntrinsicWidth_withSpaceBy, testFlowColumn_minIntrinsicWidth_withSpaceBy,
// testFlowColumn_minIntrinsicWidth_horizontalArrangement_withSpaceBy_MultipleRows, testFlowColumn_minIntrinsicWidth_horizontalArrangement_withSpaceBy,
// testFlowRow_maxIntrinsicWidth_withSpaceBy, testFlowColumn_maxIntrinsicWidth_withSpaceBy, testFlowRow_minIntrinsicWidth_withAConstraint,
// testFlowColumn_minIntrinsicWidth_withAConstraint, testFlowRow_minIntrinsicWidth_withAConstraint_withSpacedBy,
// testFlowColumn_minIntrinsicWidth_withAConstraint_withSpacedBy, testFlowRow_minIntrinsicWidth_withMaxItems, testFlowColumn_minIntrinsicWidth_withMaxItems,
// testFlowRow_maxIntrinsicWidth_withMaxItems, testFlowColumn_maxIntrinsicWidth_withMaxItems, testFlowRow_minIntrinsicWidth_withAConstraint_withMaxItems,
// testFlowColumn_minIntrinsicWidth_withAConstraint_withMaxItems, testFlowRow_minIntrinsicHeight, testFlowRow_maxIntrinsicHeight,
// testFlowRow_maxIntrinsicHeight_withSpacedBy, testFlowRow_minIntrinsicHeight_withSpaceBy_MultipleRows, testFlowRow_minIntrinsicHeight_withSpaceBy,
// testFlowColumn_minIntrinsicHeight, testFlowColumn_maxIntrinsicHeight and testFlowColumn_maxIntrinsicHeight_withSpacedByOnMainAxis
#[test]
fn a_flows_intrinsics_are_composes() {
    use IntrinsicSize::{Max, Min};
    let row = || spaced(Scene::row(6), 0.0, 0.0);
    let column = || spaced(Scene::column(6), 0.0, 0.0);
    let width = |size: IntSize| size.width;
    let height = |size: IntSize| size.height;
    // (name, scene, container, modifier, what's measured, expected)
    type Case = (&'static str, Scene, f32, Modifier, fn(IntSize) -> i32, i32);
    let cases: Vec<Case> = vec![
        ("row min width", with_max(row(), 5), 200.0, Modifier.width(Min), width, 20),
        ("column min width", with_max(column(), 6), 200.0, Modifier.width(Min), width, 20),
        ("column min width wrapping", with_max(column(), 5), 200.0, Modifier.width(Min), width, 40),
        ("row max width", row(), 200.0, Modifier.width(Max), width, 120),
        ("column max width", column(), 200.0, Modifier.width(Max), width, 20),
        (
            "row min width spaced",
            with_max(spaced(Scene::row(6), 20.0, 0.0), 5),
            200.0,
            Modifier.width(Min),
            width,
            20,
        ),
        (
            "column min width spaced",
            spaced(Scene::column(6), 0.0, 20.0),
            80.0,
            Modifier.width(Min),
            width,
            60,
        ),
        (
            "column min width spaced across",
            with_max(spaced(Scene::column(6), 20.0, 0.0), 5),
            200.0,
            Modifier.width(Min),
            width,
            60,
        ),
        (
            "column min width spaced across, one column",
            with_max(spaced(Scene::column(5), 20.0, 0.0), 5),
            200.0,
            Modifier.width(Min),
            width,
            20,
        ),
        (
            "row max width spaced",
            spaced(Scene::row(6), 10.0, 0.0),
            200.0,
            Modifier.width(Max),
            width,
            170,
        ),
        (
            "column max width spaced",
            spaced(Scene::column(6), 0.0, 20.0),
            200.0,
            Modifier.width(Max),
            width,
            40,
        ),
        (
            "row min width, a wide item",
            with_max(with_items(row(), widths_of(6, 5, true)), 5),
            200.0,
            Modifier.width(Min),
            width,
            100,
        ),
        (
            "row min width, a wide item: height",
            with_max(with_items(row(), widths_of(6, 5, true)), 5),
            200.0,
            Modifier.width(Min),
            height,
            40,
        ),
        (
            "column min width, a tall item",
            with_max(with_items(column(), widths_of(6, 5, false)), 5),
            200.0,
            Modifier.width(Min),
            height,
            100,
        ),
        (
            "column min width, a tall item: width",
            with_max(with_items(column(), widths_of(6, 5, false)), 5),
            200.0,
            Modifier.width(Min),
            width,
            40,
        ),
        (
            "row min width, a wide item, spaced",
            with_max(with_items(spaced(Scene::row(6), 10.0, 0.0), widths_of(6, 5, true)), 5),
            200.0,
            Modifier.width(Min),
            width,
            100,
        ),
        (
            "row min width, a wide item, spaced: height",
            with_max(with_items(spaced(Scene::row(6), 10.0, 0.0), widths_of(6, 5, true)), 5),
            200.0,
            Modifier.width(Min),
            height,
            60,
        ),
        (
            "column min width, a wide item, spaced",
            with_max(with_items(spaced(Scene::column(6), 0.0, 10.0), widths_of(6, 5, true)), 5),
            200.0,
            Modifier.width(Min),
            height,
            140,
        ),
        (
            "column min width, a wide item, spaced: width",
            with_max(with_items(spaced(Scene::column(6), 0.0, 10.0), widths_of(6, 5, true)), 5),
            200.0,
            Modifier.width(Min),
            width,
            120,
        ),
        ("row min width, two per row", with_max(row(), 2), 200.0, Modifier.width(Min), width, 20),
        ("row max width, two per row", with_max(row(), 2), 200.0, Modifier.width(Max), width, 40),
        (
            "column max width, two per column",
            with_max(column(), 2),
            200.0,
            Modifier.width(Max),
            width,
            60,
        ),
        (
            "row min width, ten, a wide item, two per row",
            with_max(with_items(row(), widths_of(10, 5, true)), 2),
            200.0,
            Modifier.width(Min),
            width,
            100,
        ),
        (
            "row min width, ten, a wide item, two per row: height",
            with_max(with_items(row(), widths_of(10, 5, true)), 2),
            200.0,
            Modifier.width(Min),
            height,
            120,
        ),
        (
            "column min width, ten, a wide item, two per column",
            with_max(with_items(column(), widths_of(10, 5, true)), 2),
            200.0,
            Modifier.width(Min),
            width,
            180,
        ),
        (
            "column min width, ten, a wide item, two per column: height",
            with_max(with_items(column(), widths_of(10, 5, true)), 2),
            200.0,
            Modifier.width(Min),
            height,
            40,
        ),
        ("row min height", row(), 200.0, Modifier.height(Min), height, 20),
        (
            "row max height",
            with_items(row(), vec![(20.0, 20.0); 5]),
            200.0,
            Modifier.width(Min).height(Max),
            height,
            100,
        ),
        (
            "row max height spaced",
            with_items(spaced(Scene::row(5), 0.0, 20.0), vec![(20.0, 20.0); 5]),
            200.0,
            Modifier.width(Min).height(Max),
            height,
            180,
        ),
        (
            "row min height spaced, one per row",
            with_max(with_items(spaced(Scene::row(2), 0.0, 20.0), vec![(20.0, 20.0); 2]), 1),
            200.0,
            Modifier.height(Min),
            height,
            60,
        ),
        (
            "row min height spaced",
            with_items(spaced(Scene::row(2), 0.0, 20.0), vec![(20.0, 20.0); 2]),
            200.0,
            Modifier.height(Min),
            height,
            20,
        ),
        ("column min height", with_max(column(), 5), 200.0, Modifier.height(Min), height, 20),
        (
            "column max height",
            with_max(with_items(spaced(Scene::column(5), 20.0, 0.0), vec![(20.0, 20.0); 5]), 5),
            200.0,
            Modifier.height(Max),
            height,
            100,
        ),
        (
            "column max height spaced",
            with_max(with_items(spaced(Scene::column(5), 0.0, 20.0), vec![(20.0, 20.0); 5]), 5),
            200.0,
            Modifier.height(Max),
            height,
            180,
        ),
    ];
    for (name, scene, container, modifier, measured, expected) in cases {
        assert_eq!(measured(intrinsic_flow(scene, container, modifier)), expected, "{name}");
    }
}

/// A flow row of six 20 px items, at most six a row, at its minimum intrinsic width in a box
/// of `container` px, `spacing` apart, while its maximum lines change: its widths.
fn min_intrinsic_widths_by_max_lines(container: f32, spacing: f32) -> Vec<i32> {
    let max_lines = mutable_state_of(1u32);
    let lines = max_lines.handle();
    let width = size_cell();
    let measured = width.clone();
    let mut ui = rule(800, move || {
        let measured = measured.clone();
        Box!(modifier = Modifier.width(container.dp()).wrap_content_height(), {
            FlowRow!(
                modifier = Modifier.width(IntrinsicSize::Min).then(size_into(&measured)),
                horizontal_arrangement = Arrangement::spaced_by(spacing.dp()),
                max_items_in_each_row = 6,
                max_lines = lines.get(),
                {
                    for _ in 0..6 {
                        Box!(modifier = Modifier.size(20.dp()));
                    }
                }
            );
        });
    });
    let mut widths = vec![width.get().width];
    for lines in [2, 4, 6] {
        max_lines.set(lines);
        ui.wait_for_idle();
        widths.push(width.get().width);
    }
    widths
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_minIntrinsicWidth_MaxLines
// and testFlowRow_minIntrinsicWidth_MaxLinesWithSpacedBy (the default overflow, `Clip`)
#[test]
fn a_flow_rows_minimum_intrinsic_width_shows_every_item_its_lines_can() {
    assert_eq!(min_intrinsic_widths_by_max_lines(200.0, 0.0), vec![120, 60, 40, 20]);
    assert_eq!(min_intrinsic_widths_by_max_lines(250.0, 20.0), vec![220, 100, 60, 20]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_crossAxisPositioning_withWeightAndAspectRatio
// and testFlowColumn_crossAxisPositioning_withWeightAndAspectRatio
#[test]
fn weighted_items_sized_by_aspect_ratio_set_their_lines_size_across() {
    for row in [true, false] {
        let placed = recorded(5);
        let recorded = placed.clone();
        let _ui = rule(800, move || {
            let recorded = recorded.clone();
            let container = if row { Modifier.width(50.dp()) } else { Modifier.height(50.dp()) };
            Box!(modifier = container, {
                let recorded = recorded.clone();
                if row {
                    FlowRow!(
                        max_items_in_each_row = 2,
                        vertical_arrangement = Arrangement::spaced_by(10.dp()),
                        |scope| {
                            for index in 0..5 {
                                Box!(
                                    modifier = Modifier
                                        .weight(scope, 1.0)
                                        .aspect_ratio_with(1.0, true)
                                        .then(bounds_into(&recorded, index))
                                );
                            }
                        }
                    );
                } else {
                    FlowColumn!(
                        max_items_in_each_column = 2,
                        horizontal_arrangement = Arrangement::spaced_by(10.dp()),
                        |scope| {
                            for index in 0..5 {
                                Box!(
                                    modifier = Modifier
                                        .weight(scope, 1.0)
                                        .aspect_ratio_with(1.0, true)
                                        .then(bounds_into(&recorded, index))
                                );
                            }
                        }
                    );
                }
            });
        });
        let across: Vec<i32> = (0..5)
            .map(|index| {
                let item = rect(&placed, index);
                (if row { item.top } else { item.left }) as i32
            })
            .collect();
        assert_eq!(across, vec![0, 0, 35, 35, 70], "row {row}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_minIntrinsicWidth_MaxItemsInRow_MaxLines,
// testFlowColumn_minIntrinsicHeight_MaxItemsInColumn_MaxLines, testFlowRow_minIntrinsicHeight_MaxItemsInRow_MaxLines,
// testFlowColumn_minIntrinsicWidth_MaxItemsInColumn_MaxLines, testFlowRow_maxIntrinsicHeight_MaxItemsInRow_MaxLines and
// testFlowColumn_maxIntrinsicWidth_MaxItemsInColumn_MaxLines (their first state, the `Clip` overflow; the rest change the deprecated overflow)
#[test]
fn a_flows_intrinsics_show_what_its_maximum_items_and_lines_allow() {
    use IntrinsicSize::{Max, Min};
    let cases = [
        (true, Modifier.width(Min)),
        (true, Modifier.height(Min)),
        (true, Modifier.width(Min).height(Max)),
        (false, Modifier.height(Min)),
        (false, Modifier.width(Min)),
        (false, Modifier.height(Min).width(Max)),
    ];
    for (index, (row, modifier)) in cases.into_iter().enumerate() {
        let mut scene = if row { Scene::row(6) } else { Scene::column(6) };
        scene.max_items = 2;
        scene.max_lines = 1;
        scene.modifier = modifier;
        let laid = lay_out(scene);
        let size = (laid.flow.width() as i32, laid.flow.height() as i32);
        assert_eq!(size, if row { (40, 20) } else { (20, 40) }, "case {index}");
        assert_eq!(laid.placed(), 2, "case {index}");
    }
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_wrapsWithWeights
// (with spacing: what weights share leaves out the spacing, and what rounding leaves goes one pixel
// at a time from the first, as RowColumnMeasureHelper's)
#[test]
fn weights_share_what_the_spacing_leaves_and_the_rounding_remainder_goes_first_to_first() {
    let bounds = recorded(3);
    let recording = bounds.clone();
    let _ui = rule(100, move || {
        let recording = recording.clone();
        FlowRow!(
            modifier = Modifier.width(100.dp()),
            horizontal_arrangement = Arrangement::spaced_by(10.dp()),
            max_items_in_each_row = 3,
            move |scope| {
                for index in 0..3 {
                    let modifier = Modifier.height(5.dp()).then(bounds_into(&recording, index));
                    Box!(modifier = modifier.weight(scope, 1.0));
                }
            }
        );
    });
    // 80 left: 26.67 each, 27 rounded, 81 together: one pixel less on the first.
    let lefts_and_widths: Vec<_> =
        (0..3).map(|index| (rect(&bounds, index).left, rect(&bounds, index).width())).collect();
    assert_eq!(lefts_and_widths, [(0.0, 26.0), (36.0, 27.0), (73.0, 27.0)]);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/FlowRowColumnTest.kt, testFlowRow_wrapsToTheNextLine_withExactSpaceNeeded
// (at a fractional density: lines wrap by the spacing rounded up, D1352)
#[test]
fn a_line_wraps_by_its_spacing_rounded_up_at_a_fractional_density() {
    let height_in = |width: f32| {
        let size = size_cell();
        let recording = size.clone();
        let mut ui = rule(200, move || {
            let recording = recording.clone();
            FlowRow!(
                modifier = Modifier.width(compose_ui::Dp(width / 1.25)).then(size_into(&recording)),
                // 6.25 px: 7 up, where rounding would say 6.
                horizontal_arrangement = Arrangement::spaced_by(5.dp()),
                |_| {
                    for _ in 0..2 {
                        Box!(modifier = Modifier.size(16.dp()));
                    }
                }
            );
        });
        ui.host().set_density(compose_ui::Density::new(1.25));
        ui.wait_for_idle();
        size.get().height
    };
    // Items of 20 px: 47 px fit them on a line, 46 don't.
    assert_eq!(height_in(47.0), 20);
    assert_eq!(height_in(46.0), 40);
}
