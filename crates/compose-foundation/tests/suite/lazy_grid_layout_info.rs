//! A lazy grid's `layout_info`: the items in view with their lines, cells, offsets and sizes,
//! the viewport, the padding, the count and the cells in a line, each orientation, reversed or
//! not.

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntOffset, IntSize, PaddingValues, UnitExt};

use crate::lazy_support::{Lazy, LazyGrid, no_padding, px};
use crate::scroll_support::run;

const ITEM: i32 = 50;

/// Checks `count` items in view in lines of `cells` from `start_index`, the first line at
/// `start_offset`, each item `expected_size`, lines `spacing` apart, as Compose's
/// `assertVisibleItems`.
#[allow(clippy::too_many_arguments)]
fn assert_visible_items(
    lazy: Lazy,
    info: &LazyGridLayoutInfo,
    count: usize,
    cells: usize,
    start_index: usize,
    start_offset: i32,
    expected_size: IntSize,
    spacing: i32,
) {
    assert_eq!(info.visible_items_info.len(), count, "{info:?}");
    if count == 0 {
        return;
    }
    assert_eq!(start_index % cells, 0);
    assert_eq!(info.visible_items_info.len() % cells, 0);
    let (mut index, mut offset, mut line, mut cell) =
        (start_index, start_offset, start_index / cells, 0);
    for item in &info.visible_items_info {
        assert_eq!(item.index, index, "{info:?}");
        let along = if lazy.vertical { item.offset.y } else { item.offset.x };
        assert_eq!(along, offset, "offset of item {index}: {info:?}");
        assert_eq!(item.size, expected_size, "{info:?}");
        let (item_line, item_cell) =
            if lazy.vertical { (item.row, item.column) } else { (item.column, item.row) };
        assert_eq!((item_line, item_cell), (line as i32, cell as i32), "{info:?}");
        index += 1;
        cell += 1;
        if cell == cells {
            cell = 0;
            line += 1;
            offset += spacing + if lazy.vertical { item.size.height } else { item.size.width };
        }
    }
}

fn square() -> IntSize {
    IntSize::new(ITEM, ITEM)
}

/// Twelve items in two cells of a grid 100 px across and 175 px along.
fn twelve_items(lazy: Lazy, state: LazyGridState, cross: i32) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyGrid(
            lazy,
            2,
            lazy.sized(px(ITEM * 7 / 2), px(cross)),
            state,
            no_padding(),
            move |scope: LazyGridScope| {
                scope.items(12, move |_| Box!(modifier = lazy.main_axis_size(px(ITEM))));
            },
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, visibleItemsAreCorrect
#[test]
fn the_items_in_view_are_in_the_layout_info() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let _ui = twelve_items(lazy, *state, ITEM * 2);
        assert_visible_items(lazy, &state.layout_info(), 8, 2, 0, 0, square(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, visibleItemsAreCorrectAfterScroll
#[test]
fn the_layout_info_follows_a_scroll() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = twelve_items(lazy, handle, ITEM * 2);
        let _ = run(&mut ui, async move { handle.scroll_to_item(2, 10).await });
        assert_visible_items(lazy, &state.layout_info(), 8, 2, 2, -10, square(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, visibleItemsAreCorrectWithSpacing
#[test]
fn the_layout_info_has_the_spacing() {
    for lazy in Lazy::all() {
        let lazy = Lazy { spaced_by: ITEM as f32, ..lazy };
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyGrid(
                lazy,
                1,
                lazy.sized(px(ITEM * 7 / 2), px(ITEM)),
                handle,
                no_padding(),
                move |scope: LazyGridScope| {
                    scope.items(12, move |_| Box!(modifier = lazy.main_axis_size(px(ITEM))));
                },
            );
        });
        let info = state.layout_info();
        assert_visible_items(lazy, &info, 2, 1, 0, 0, square(), ITEM);
        assert_eq!(info.main_axis_item_spacing, ITEM);
    }
}

thread_local! {
    static OBSERVED: RefCell<Option<LazyGridLayoutInfo>> = const { RefCell::new(None) };
}

#[composable]
fn Observing(state: LazyGridState) {
    let info = state.layout_info();
    OBSERVED.with(|observed| *observed.borrow_mut() = Some(info));
}

fn take_observed() -> Option<LazyGridLayoutInfo> {
    OBSERVED.with(|observed| observed.borrow_mut().take())
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, visibleItemsAreObservableWhenWeScroll
#[test]
fn a_scroll_recomposes_what_reads_the_layout_info() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyGrid(
                lazy,
                2,
                lazy.sized(px(ITEM * 7 / 2), px(ITEM * 2)),
                handle,
                no_padding(),
                move |scope: LazyGridScope| {
                    scope.items(12, move |_| Box!(modifier = lazy.main_axis_size(px(ITEM))));
                },
            );
            Observing(handle);
        });
        take_observed();
        let _ = run(&mut ui, async move { handle.scroll_to_item(2, 0).await });
        let info = take_observed().expect("recomposed");
        assert_visible_items(lazy, &info, 8, 2, 2, 0, square(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, visibleItemsAreObservableWhenResize
#[test]
fn a_resized_item_recomposes_what_reads_the_layout_info() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let size = mutable_state_of(ITEM * 2);
        let size_handle = size.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyGrid(
                lazy,
                1,
                lazy.cross_axis_size(px(ITEM)),
                handle,
                no_padding(),
                move |scope: LazyGridScope| {
                    scope.item(move || Box!(modifier = Modifier.size(px(size_handle.get()))));
                },
            );
            Observing(handle);
        });
        let info = take_observed().expect("composed");
        let tall =
            if lazy.vertical { IntSize::new(ITEM, ITEM * 2) } else { IntSize::new(ITEM * 2, ITEM) };
        assert_visible_items(lazy, &info, 1, 1, 0, 0, tall, 0);
        size.set(ITEM);
        ui.wait_for_idle();
        let info = take_observed().expect("recomposed");
        assert_visible_items(lazy, &info, 1, 1, 0, 0, square(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, totalCountIsCorrect
#[test]
fn the_layout_info_counts_the_items() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let count = mutable_state_of(10usize);
        let count_handle = count.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            let count = count_handle.get();
            LazyGrid(lazy, 2, Modifier, handle, no_padding(), move |scope: LazyGridScope| {
                scope.items(count, move |_| Box!(modifier = lazy.main_axis_size(10.dp())));
            });
        });
        assert_eq!(state.layout_info().total_items_count, 10);
        count.set(20);
        ui.wait_for_idle();
        assert_eq!(state.layout_info().total_items_count, 20);
    }
}

fn viewport_size(lazy: Lazy) -> IntSize {
    if lazy.vertical { IntSize::new(90, 45) } else { IntSize::new(45, 90) }
}

/// A grid 45 px along and 90 across, of `cells` cells, with `items` items of `item` px along.
fn viewport_grid(
    lazy: Lazy,
    state: LazyGridState,
    cells: usize,
    padding: PaddingValues,
    items: usize,
    item: Option<i32>,
) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyGrid(
            lazy,
            cells,
            lazy.sized(px(45), px(90)),
            state,
            padding,
            move |scope: LazyGridScope| {
                scope.items(items, move |_| {
                    let modifier = match item {
                        Some(along) => lazy.main_axis_size(px(along)),
                        None => Modifier.size(px(22)),
                    };
                    Box!(modifier = modifier);
                });
            },
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, viewportOffsetsAndSizeAreCorrect
#[test]
fn the_viewport_is_the_grids_size() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let _ui = viewport_grid(lazy, *state, 2, no_padding(), 8, Some(45));
        let info = state.layout_info();
        assert_eq!((info.viewport_start_offset, info.viewport_end_offset), (0, 45));
        assert_eq!(info.viewport_size, viewport_size(lazy));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, viewportOffsetsAndSizeAreCorrectWithContentPadding
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, emptyContent
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, viewportIsLargerThenTheContent
#[test]
fn the_viewport_starts_before_the_padding() {
    for (cells, items, item) in [(2, 8, Some(45)), (1, 0, Some(45)), (1, 1, None)] {
        for lazy in Lazy::all() {
            let state = LazyGridState::new(0, 0);
            let (before, after) = if lazy.reverse_layout { (15, 10) } else { (10, 15) };
            let padding = lazy.padding(px(before), px(after), 2.dp(), 2.dp());
            let _ui = viewport_grid(lazy, *state, cells, padding, items, item);
            let info = state.layout_info();
            let context = format!("{lazy:?}, {items} items");
            assert_eq!(info.viewport_start_offset, -10, "{context}");
            assert_eq!(info.viewport_end_offset, 45 - 10, "{context}");
            assert_eq!(info.before_content_padding, 10, "{context}");
            assert_eq!(info.after_content_padding, 15, "{context}");
            assert_eq!(info.viewport_size, viewport_size(lazy), "{context}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, emptyItemsInVisibleItemsInfo
#[test]
fn empty_items_are_in_the_layout_info() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyGrid(lazy, 2, Modifier, handle, no_padding(), |scope: LazyGridScope| {
                scope.item(|| Box!(modifier = Modifier));
                scope.item(|| {});
            });
        });
        let indices: Vec<usize> =
            state.layout_info().visible_items_info.iter().map(|item| item.index).collect();
        assert_eq!(indices, [0, 1], "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, reverseLayoutIsCorrect
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, orientationIsCorrect
#[test]
fn the_layout_info_says_the_orientation() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let _ui = twelve_items(lazy, *state, ITEM * 2);
        let info = state.layout_info();
        assert_eq!(info.reverse_layout, lazy.reverse_layout);
        assert_eq!(info.orientation == Orientation::Vertical, lazy.vertical);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, viewportOffsetsSmallContentReverseArrangement
#[test]
fn short_content_arranged_from_the_end_starts_after_the_room_left() {
    for lazy in Lazy::all() {
        let lazy = Lazy { reverse_arrangement: true, ..lazy };
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyGrid(
                lazy,
                2,
                lazy.sized(px(ITEM * 5), px(ITEM * 2)),
                handle,
                no_padding(),
                |scope: LazyGridScope| {
                    scope.items(8, |_| Box!(modifier = Modifier.required_size(px(ITEM))));
                },
            );
        });
        let info = state.layout_info();
        assert_eq!((info.viewport_start_offset, info.viewport_end_offset), (0, ITEM * 5));
        assert_visible_items(lazy, &info, 8, 2, 0, ITEM, square(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, contentTypeIsCorrect
#[test]
fn the_layout_info_has_the_content_types() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyGrid(
            Lazy::column(),
            1,
            Modifier.required_size(px(ITEM * 3)),
            handle,
            no_padding(),
            |scope: LazyGridScope| {
                scope.items_with(
                    2,
                    |index| index,
                    |_| Box!(modifier = Modifier.required_size(px(ITEM))),
                );
                scope.item(|| Box!(modifier = Modifier.required_size(px(ITEM))));
            },
        );
    });
    let types: Vec<Option<u64>> =
        state.layout_info().visible_items_info.iter().map(|item| item.content_type).collect();
    assert_eq!(types, [Some(hash_key(&0usize)), Some(hash_key(&1usize)), None]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, snapshotFlowIsNotifiedAboutNewOffsetOnSmallScrolls
#[test]
fn a_pixels_scroll_moves_the_first_items_offset() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyGrid(
                lazy,
                1,
                Modifier.size(15.dp()),
                handle,
                no_padding(),
                |scope: LazyGridScope| {
                    scope.items(100, |_| Box!(modifier = Modifier.size(10.dp())));
                },
            );
            Observing(handle);
        });
        let _ = run(&mut ui, async move { handle.scroll_by(1.0).await });
        let info = take_observed().expect("recomposed");
        let offset =
            info.visible_items_info.first().map_or(IntOffset::new(0, 0), |item| item.offset);
        assert_eq!(if lazy.vertical { offset.y } else { offset.x }, -1);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, maxSpan_returnsNumberOfSlotsPerLine
#[test]
fn the_layout_info_says_the_cells_in_a_line() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyGrid(
                lazy,
                4,
                lazy.sized(px(ITEM * 4), px(ITEM * 2)),
                handle,
                no_padding(),
                |scope: LazyGridScope| {
                    scope.items(8, |_| Box!(modifier = Modifier.required_size(px(ITEM))));
                },
            );
        });
        assert_eq!(state.layout_info().max_span, 4);
    }
}

// Review of D863 for grids: a scroll back to fill a gap at the end keeps the lines laid out
// before it, an empty last line included, as a list's (Compose's grid measurement only adds
// lines to those it laid out).
#[test]
fn an_empty_last_line_stays_laid_out_after_a_scroll_back() {
    for lazy in Lazy::all() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            LazyGrid(
                lazy,
                1,
                lazy.sized(px(100), px(100)),
                handle,
                no_padding(),
                move |scope: LazyGridScope| {
                    scope.items(6, move |index| {
                        let main = if index == 5 { 0 } else { 30 };
                        Spacer(lazy.main_axis_size(px(main)).then(lazy.cross_axis_size(px(100))));
                    });
                },
            );
        });
        // Every item composed first (by a scroll to the end), then a scroll past the end that
        // the measurement scrolls back from in one pass, as Compose's.
        let _ = run(&mut ui, async move { handle.scroll_by(200.0).await });
        let _ = run(&mut ui, async move { handle.scroll_by(-30.0).await });
        let _ = run(&mut ui, async move { handle.scroll_by(1000.0).await });
        let info = state.layout_info();
        let last = info.visible_items_info.last().map(|item| item.index);
        assert_eq!(last, Some(5), "{lazy:?}");
    }
}
