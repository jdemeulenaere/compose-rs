//! A lazy list's `layout_info`: the items in view with their offsets and sizes, the viewport,
//! the padding, the count, the orientation, and a layout telling its readers.

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, UnitExt};

use crate::lazy_support::{Lazy, LazyColumnOrRow, no_padding, px};
use crate::scroll_support::run;

const ITEM: i32 = 50;

/// Checks `count` items in view from `start_index`, the first at `start_offset`, each
/// `expected_size` long and `spacing` apart, as Compose's `assertVisibleItems`.
fn assert_visible_items(
    info: &LazyListLayoutInfo,
    count: usize,
    start_index: usize,
    start_offset: i32,
    expected_size: i32,
    spacing: i32,
) {
    assert_eq!(info.visible_items_info.len(), count, "{info:?}");
    let mut offset = start_offset;
    for (position, item) in info.visible_items_info.iter().enumerate() {
        assert_eq!(item.index, start_index + position, "{info:?}");
        assert_eq!(item.offset, offset, "offset of item {}: {info:?}", item.index);
        assert_eq!(item.size, expected_size, "{info:?}");
        offset += item.size + spacing;
    }
}

/// Six 50 px items in a 175 px list (3.5 items).
fn six_items(lazy: Lazy, state: LazyListState) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyColumnOrRow(
            lazy,
            Modifier.required_size(px(ITEM * 7 / 2)),
            state,
            no_padding(),
            |scope: LazyListScope| {
                scope.items(6, |_| Box!(modifier = Modifier.required_size(px(ITEM))));
            },
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, visibleItemsAreCorrect
#[test]
fn the_items_in_view_are_in_the_layout_info() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let _ui = six_items(lazy, *state);
        assert_visible_items(&state.layout_info(), 4, 0, 0, ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, visibleItemsAreCorrectAfterScroll
#[test]
fn the_layout_info_follows_a_scroll() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = six_items(lazy, handle);
        let _ = run(&mut ui, async move { handle.scroll_to_item(1, 10).await });
        assert_visible_items(&state.layout_info(), 4, 1, -10, ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, visibleItemsAreCorrectWithSpacing
#[test]
fn the_layout_info_has_the_spacing() {
    for lazy in Lazy::all() {
        let lazy = Lazy { spaced_by: ITEM as f32, ..lazy };
        let state = LazyListState::new(0, 0);
        let _ui = six_items(lazy, *state);
        let info = state.layout_info();
        assert_visible_items(&info, 2, 0, 0, ITEM, ITEM);
        assert_eq!(info.main_axis_item_spacing, ITEM);
    }
}

thread_local! {
    static OBSERVED: RefCell<Option<LazyListLayoutInfo>> = const { RefCell::new(None) };
}

/// Reads the state's layout info in composition, as Compose's `ObservingFun`.
#[composable]
fn Observing(state: LazyListState) {
    let info = state.layout_info();
    OBSERVED.with(|observed| *observed.borrow_mut() = Some(info));
}

fn take_observed() -> Option<LazyListLayoutInfo> {
    OBSERVED.with(|observed| observed.borrow_mut().take())
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, visibleItemsAreObservableWhenWeScroll
#[test]
fn a_scroll_recomposes_what_reads_the_layout_info() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(px(ITEM * 7 / 2)),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(6, |_| Box!(modifier = Modifier.required_size(px(ITEM))));
                },
            );
            Observing(handle);
        });
        take_observed();
        let _ = run(&mut ui, async move { handle.scroll_to_item(1, 0).await });
        let info = take_observed().expect("recomposed");
        assert_visible_items(&info, 4, 1, 0, ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, visibleItemsAreObservableWhenResize
#[test]
fn a_resized_item_recomposes_what_reads_the_layout_info() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let size = mutable_state_of(ITEM * 2);
        let size_handle = size.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyColumnOrRow(lazy, Modifier, handle, no_padding(), move |scope: LazyListScope| {
                scope.item(move || Box!(modifier = Modifier.required_size(px(size_handle.get()))));
            });
            Observing(handle);
        });
        let info = take_observed().expect("composed");
        assert_visible_items(&info, 1, 0, 0, ITEM * 2, 0);
        size.set(ITEM);
        ui.wait_for_idle();
        let info = take_observed().expect("recomposed");
        assert_visible_items(&info, 1, 0, 0, ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, totalCountIsCorrect
#[test]
fn the_layout_info_counts_the_items() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let count = mutable_state_of(10usize);
        let count_handle = count.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            let count = count_handle.get();
            LazyColumnOrRow(lazy, Modifier, handle, no_padding(), move |scope: LazyListScope| {
                scope.items(count, |_| Box!(modifier = Modifier.required_size(10.dp())));
            });
        });
        assert_eq!(state.layout_info().total_items_count, 10);
        count.set(20);
        ui.wait_for_idle();
        assert_eq!(state.layout_info().total_items_count, 20);
    }
}

/// A list 45 px along and 90 across, with four 45 px items and `padding`.
fn viewport_list(
    lazy: Lazy,
    state: LazyListState,
    padding: compose_ui::PaddingValues,
    items: usize,
    item_size: i32,
) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyColumnOrRow(
            lazy,
            lazy.sized(px(45), px(90)),
            state,
            padding,
            move |scope: LazyListScope| {
                scope.items(items, move |_| Box!(modifier = Modifier.required_size(px(item_size))));
            },
        );
    })
}

fn viewport_size(lazy: Lazy) -> IntSize {
    if lazy.vertical { IntSize::new(90, 45) } else { IntSize::new(45, 90) }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, viewportOffsetsAndSizeAreCorrect
#[test]
fn the_viewport_is_the_lists_size() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let _ui = viewport_list(lazy, *state, no_padding(), 4, 45);
        let info = state.layout_info();
        assert_eq!((info.viewport_start_offset, info.viewport_end_offset), (0, 45), "{lazy:?}");
        assert_eq!(info.viewport_size, viewport_size(lazy));
    }
}

/// 10 px of padding before the content and 15 after it, the list's way (so physically the
/// other way round when reversed), as Compose's tests set them.
fn padding_10_15(lazy: Lazy) -> compose_ui::PaddingValues {
    let (before, after) = if lazy.reverse_layout { (15, 10) } else { (10, 15) };
    lazy.padding(px(before), px(after), 2.dp(), 2.dp())
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, viewportOffsetsAndSizeAreCorrectWithContentPadding
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, emptyContent
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, viewportIsLargerThenTheContent
#[test]
fn the_viewport_starts_before_the_padding() {
    // Four items, none, and one smaller than the list.
    for (items, item_size) in [(4, 45), (0, 45), (1, 22)] {
        for lazy in Lazy::all() {
            let state = LazyListState::new(0, 0);
            let _ui = viewport_list(lazy, *state, padding_10_15(lazy), items, item_size);
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, emptyItemsInVisibleItemsInfo
#[test]
fn empty_items_are_in_the_layout_info() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyColumnOrRow(lazy, Modifier, handle, no_padding(), |scope: LazyListScope| {
                scope.item(|| Box!(modifier = Modifier));
                scope.item(|| {});
            });
        });
        let info = state.layout_info();
        let indices: Vec<usize> = info.visible_items_info.iter().map(|item| item.index).collect();
        assert_eq!(indices, [0, 1], "{lazy:?} {info:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, reverseLayoutIsCorrect
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, orientationIsCorrect
#[test]
fn the_layout_info_says_the_orientation_and_the_reversal() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let _ui = six_items(lazy, *state);
        let info = state.layout_info();
        assert_eq!(info.reverse_layout, lazy.reverse_layout);
        let orientation =
            if lazy.vertical { Orientation::Vertical } else { Orientation::Horizontal };
        assert_eq!(info.orientation, orientation);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, contentTypeIsCorrect
#[test]
fn the_layout_info_has_the_content_types() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyColumnOrRow(
            Lazy::column(),
            Modifier.required_size(px(ITEM * 3)),
            handle,
            no_padding(),
            |scope: LazyListScope| {
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
    // Content types are hashed (D432); an item without one has none, Compose's `null`.
    assert_eq!(types, [Some(hash_key(&0usize)), Some(hash_key(&1usize)), None]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, viewportOffsetsSmallContentReverseArrangement
#[test]
fn short_content_arranged_from_the_end_starts_after_the_room_left() {
    for lazy in Lazy::all() {
        let lazy = Lazy { reverse_arrangement: true, ..lazy };
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let _ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(px(ITEM * 5)),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(4, |_| Box!(modifier = Modifier.required_size(px(ITEM))));
                },
            );
        });
        let info = state.layout_info();
        assert_eq!((info.viewport_start_offset, info.viewport_end_offset), (0, ITEM * 5));
        assert_visible_items(&info, 4, 0, ITEM, ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListLayoutInfoTest.kt, snapshotFlowIsNotifiedAboutNewOffsetOnSmallScrolls
#[test]
fn a_pixels_scroll_moves_the_first_items_offset() {
    for lazy in Lazy::all() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            LazyColumnOrRow(
                lazy,
                Modifier.size(15.dp()),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(100, |_| Box!(modifier = Modifier.size(10.dp())));
                },
            );
            Observing(handle);
        });
        let _ = run(&mut ui, async move { handle.scroll_by(1.0).await });
        let info = take_observed().expect("recomposed");
        assert_eq!(info.visible_items_info.first().map(|item| item.offset), Some(-1));
    }
}

// D863: an empty last item stays laid out once the list scrolls back from past its end.
#[test]
fn an_empty_last_item_stays_laid_out_after_a_long_scroll_back() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            LazyColumnOrRow(
                lazy,
                lazy.sized(px(100), px(100)),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
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
