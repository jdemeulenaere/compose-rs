//! Grids' content padding, as Compose's `LazyGridsContentPaddingTest` (vertical grids; its row
//! cases are commented out there): applied, changed, not moving the scroll position, items in the
//! padding, to the end and back, wrapping content, no items, padding larger than the grid, right
//! to left. Density 1, so Compose's dps are pixels (its items are 50 px, the small padding 12).

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, PaddingValues, UnitExt};

use crate::lazy_support::{bounds, is_displayed};
use crate::scroll_support::run;

const LIST: &str = "LazyList";
const ITEM: &str = "item";
const CONTAINER: &str = "container";
const ITEM_SIZE: f32 = 50.0;
const SMALL_PADDING: f32 = 12.0;

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

fn vertical(top: f32, bottom: f32) -> PaddingValues {
    PaddingValues { top: top.dp(), bottom: bottom.dp(), ..PaddingValues::ZERO }
}

fn scroll_by(ui: &mut ComposeTestRule, state: LazyGridState, distance: f32) {
    let _ = run(ui, async move { state.scroll_by(distance).await });
}

fn assert_position(state: LazyGridState, index: usize, offset: f32) {
    assert_eq!(state.first_visible_item_index(), index);
    let actual = state.first_visible_item_scroll_offset() as f32;
    assert!((actual - offset).abs() <= 0.5, "offset {actual}, not {offset}");
}

fn assert_visible(state: LazyGridState, expected: &[(usize, f32)]) {
    let info = state.layout_info();
    let actual: Vec<(usize, i32)> =
        info.visible_items_info.iter().map(|item| (item.index, item.offset.y)).collect();
    let expected: Vec<(usize, i32)> =
        expected.iter().map(|&(index, offset)| (index, offset.round() as i32)).collect();
    assert_eq!(actual, expected);
}

fn top(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    bounds(ui, tag).top
}

/// Four items of `ITEM_SIZE` in a one-column grid `size` px square with `padding` above and
/// below.
fn four_items(state: LazyGridState, size: f32, padding: f32) -> ComposeTestRule {
    rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.required_size(size.dp()).test_tag(LIST),
            state = state,
            content_padding = vertical(padding, padding),
            |scope: LazyGridScope| {
                scope.items(4, |index| {
                    Spacer(Modifier.required_size(ITEM_SIZE.dp()).test_tag(index.to_string()))
                });
            }
        );
    })
}

/// Four items in a box of 1.5 items, the grid with `padding` above and below.
fn in_a_small_box(state: LazyGridState, padding: f32) -> ComposeTestRule {
    rule(move || {
        Box!(modifier = Modifier.test_tag(CONTAINER).size((ITEM_SIZE * 1.5).dp()), {
            LazyVerticalGrid!(
                GridCells::Fixed(1),
                state = state,
                content_padding = vertical(padding, padding),
                |scope: LazyGridScope| {
                    scope.items(4, |index| {
                        Box!(
                            modifier = Modifier.test_tag(index.to_string()).size(ITEM_SIZE.dp()),
                            {}
                        )
                    });
                }
            );
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_contentPaddingIsApplied
#[test]
fn the_padding_is_applied() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let container = ITEM_SIZE * 2.0;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.required_size(container.dp()).test_tag(LIST),
            state = handle,
            content_padding = PaddingValues {
                start: SMALL_PADDING.dp(),
                top: ITEM_SIZE.dp(),
                end: SMALL_PADDING.dp(),
                bottom: ITEM_SIZE.dp()
            },
            |scope: LazyGridScope| {
                scope.items(1, |_| Spacer(Modifier.height(ITEM_SIZE.dp()).test_tag(ITEM)));
            }
        );
    });
    let item = bounds(&mut ui, ITEM);
    assert_eq!((item.left, item.top), (SMALL_PADDING, ITEM_SIZE));
    assert_eq!((item.width(), item.height()), (container - SMALL_PADDING * 2.0, ITEM_SIZE));
    scroll_by(&mut ui, handle, ITEM_SIZE);
    let item = bounds(&mut ui, ITEM);
    assert_eq!((item.top, item.height()), (0.0, ITEM_SIZE));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_contentPaddingChanges_shouldApplyPadding
#[test]
fn a_changed_padding_applies() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let end = mutable_state_of(0.0f32);
    let end_handle = end.handle();
    let mut ui = rule(move || {
        let padding = PaddingValues { end: end_handle.get().dp(), ..PaddingValues::ZERO };
        LazyVerticalGrid!(
            GridCells::Adaptive(180.dp()),
            modifier = Modifier.fill_max_size().test_tag(LIST),
            state = handle,
            content_padding = padding,
            |scope: LazyGridScope| {
                scope.items(10, |index| {
                    Box!(
                        modifier = Modifier
                            .padding(8.dp())
                            .fill_max_size()
                            .aspect_ratio(1.0)
                            .test_tag(index.to_string()),
                        {}
                    );
                });
            }
        );
    });
    end.set(2.0);
    ui.wait_for_idle();
    // The last item of the first row (Compose's test's item 1, on a phone's two columns).
    let info = state.layout_info();
    let item = info
        .visible_items_info
        .iter()
        .filter(|item| item.row == 0)
        .max_by_key(|item| item.column)
        .copied()
        .unwrap();
    assert_eq!(info.viewport_size.width - (item.offset.x + item.size.width), 2);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_contentPaddingIsNotAffectingScrollPosition
#[test]
fn the_padding_does_not_move_the_scroll_position() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.required_size((ITEM_SIZE * 2.0).dp()).test_tag(LIST),
            state = handle,
            content_padding = vertical(ITEM_SIZE, ITEM_SIZE),
            |scope: LazyGridScope| scope
                .items(1, |_| Spacer(Modifier.height(ITEM_SIZE.dp()).test_tag(ITEM)))
        );
    });
    assert_position(handle, 0, 0.0);
    scroll_by(&mut ui, handle, ITEM_SIZE);
    assert_position(handle, 0, ITEM_SIZE);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_scrollForwardItemWithinStartPaddingDisplayed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_scrollBackwardItemWithinStartPaddingDisplayed
#[test]
fn items_scrolled_into_the_start_padding_show() {
    let padding = ITEM_SIZE * 1.5;
    let size = padding * 2.0 + ITEM_SIZE;
    let state = LazyGridState::new(0, 0);
    let mut ui = four_items(*state, size, padding);
    for index in 0..3 {
        assert_eq!(top(&mut ui, &index.to_string()), ITEM_SIZE * index as f32 + padding);
    }
    scroll_by(&mut ui, *state, padding);
    assert_position(*state, 1, padding - ITEM_SIZE);
    for index in 0..4 {
        assert_eq!(top(&mut ui, &index.to_string()), ITEM_SIZE * index as f32);
    }

    let state = LazyGridState::new(0, 0);
    let mut ui = four_items(*state, size, padding);
    scroll_by(&mut ui, *state, ITEM_SIZE * 3.0);
    scroll_by(&mut ui, *state, -ITEM_SIZE * 1.5);
    assert_position(*state, 1, ITEM_SIZE * 0.5);
    for index in 0..4 {
        assert_eq!(top(&mut ui, &index.to_string()), ITEM_SIZE * (1.5 + index as f32) - padding);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_scrollForwardTillTheEnd
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_scrollForwardTillTheEndAndABitBack
#[test]
fn scrolling_to_the_end_shows_the_end_padding() {
    let padding = ITEM_SIZE * 1.5;
    let state = LazyGridState::new(0, 0);
    let mut ui = four_items(*state, padding * 2.0 + ITEM_SIZE, padding);
    scroll_by(&mut ui, *state, ITEM_SIZE * 3.0);
    for _ in 0..2 {
        assert_position(*state, 3, 0.0);
        for index in 1..=3 {
            assert_eq!(top(&mut ui, &index.to_string()), ITEM_SIZE * index as f32 - padding);
        }
        // No room left: nothing changes.
        scroll_by(&mut ui, *state, 10.0);
    }
    scroll_by(&mut ui, *state, -ITEM_SIZE / 2.0);
    assert_position(*state, 2, ITEM_SIZE / 2.0);
    for index in 1..=3 {
        assert_eq!(top(&mut ui, &index.to_string()), ITEM_SIZE * (index as f32 + 0.5) - padding);
    }
}

fn uneven() -> PaddingValues {
    PaddingValues { start: 2.dp(), top: 4.dp(), end: 6.dp(), bottom: 8.dp() }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_contentPaddingFixedWidthContainer
#[test]
fn a_fixed_width_container_holds_the_padding() {
    let mut ui = rule(|| {
        Box!(modifier = Modifier.test_tag(CONTAINER).width((ITEM_SIZE + 8.0).dp()), {
            LazyVerticalGrid!(
                GridCells::Fixed(1),
                content_padding = uneven(),
                |scope: LazyGridScope| {
                    scope.items(1, |_| Spacer(Modifier.size(ITEM_SIZE.dp()).test_tag(ITEM)));
                }
            );
        });
    });
    let item = bounds(&mut ui, ITEM);
    assert_eq!(
        (item.left, item.top, item.width(), item.height()),
        (2.0, 4.0, ITEM_SIZE, ITEM_SIZE)
    );
    let container = bounds(&mut ui, CONTAINER);
    assert_eq!(
        (container.left, container.top, container.width(), container.height()),
        (0.0, 0.0, ITEM_SIZE + 8.0, ITEM_SIZE + 12.0)
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_contentPaddingAndNoContent
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_contentPaddingAndZeroSizedItem
#[test]
fn a_grid_without_items_is_its_padding() {
    for no_items_block in [true, false] {
        let mut ui = rule(move || {
            Box!(modifier = Modifier.test_tag(CONTAINER), {
                LazyVerticalGrid!(
                    GridCells::Fixed(1),
                    content_padding = uneven(),
                    move |scope: LazyGridScope| {
                        if !no_items_block {
                            scope.items(0, |_| {});
                        }
                    }
                );
            });
        });
        let container = bounds(&mut ui, CONTAINER);
        assert_eq!(
            (container.left, container.top, container.width(), container.height()),
            (0.0, 0.0, 8.0, 12.0)
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, column_overscrollWithContentPadding
#[test]
fn a_scroll_past_the_end_into_the_padding_stops_at_the_end() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        Box!(
            modifier = Modifier.test_tag(CONTAINER).size((ITEM_SIZE + SMALL_PADDING * 2.0).dp()),
            {
                LazyVerticalGrid!(
                    GridCells::Fixed(1),
                    state = handle,
                    content_padding = vertical(SMALL_PADDING, SMALL_PADDING),
                    |scope: LazyGridScope| {
                        scope.items(2, |index| {
                            Box!(
                                modifier =
                                    Modifier.test_tag(index.to_string()).height(ITEM_SIZE.dp()),
                                {}
                            )
                        });
                    }
                );
            }
        );
    });
    assert_eq!((top(&mut ui, "0"), bounds(&mut ui, "0").height()), (SMALL_PADDING, ITEM_SIZE));
    assert_eq!(
        (top(&mut ui, "1"), bounds(&mut ui, "1").height()),
        (SMALL_PADDING + ITEM_SIZE, ITEM_SIZE)
    );
    scroll_by(&mut ui, handle, ITEM_SIZE + SMALL_PADDING * 1.5);
    assert_eq!((top(&mut ui, "1"), bounds(&mut ui, "1").height()), (SMALL_PADDING, ITEM_SIZE));
    assert_eq!(
        (top(&mut ui, "0"), bounds(&mut ui, "0").height()),
        (SMALL_PADDING - ITEM_SIZE, ITEM_SIZE)
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, totalPaddingLargerParentSize_initialState
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollByPadding
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollToLastItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollToLastItemByDelta
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollTillTheEnd
#[test]
fn padding_larger_in_all_than_the_grid() {
    let state = LazyGridState::new(0, 0);
    let mut ui = in_a_small_box(*state, ITEM_SIZE);
    assert_eq!(top(&mut ui, "0"), ITEM_SIZE);
    ui.on_node_with_tag("1").assert_does_not_exist();
    assert_position(*state, 0, 0.0);
    assert_visible(*state, &[(0, 0.0)]);
    let info = state.layout_info();
    assert_eq!((info.viewport_start_offset, info.viewport_end_offset), (-50, 25));

    let state = LazyGridState::new(0, 0);
    let mut ui = in_a_small_box(*state, ITEM_SIZE);
    scroll_by(&mut ui, *state, ITEM_SIZE);
    assert_eq!((top(&mut ui, "0"), top(&mut ui, "1")), (0.0, ITEM_SIZE));
    assert!(!is_displayed(&mut ui, "2", CONTAINER));
    assert_position(*state, 1, 0.0);
    assert_visible(*state, &[(0, -ITEM_SIZE), (1, 0.0)]);

    for by_delta in [false, true] {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = in_a_small_box(handle, ITEM_SIZE);
        if by_delta {
            scroll_by(&mut ui, handle, ITEM_SIZE * 3.0);
            assert!(!is_displayed(&mut ui, "1", CONTAINER));
        } else {
            let _ = run(&mut ui, async move { handle.scroll_to_item(3, 0).await });
            ui.on_node_with_tag("1").assert_does_not_exist();
        }
        assert_eq!(
            (top(&mut ui, "2"), top(&mut ui, "3")),
            (0.0, ITEM_SIZE),
            "by delta: {by_delta}"
        );
        assert_position(handle, 3, 0.0);
        assert_visible(handle, &[(2, -ITEM_SIZE), (3, 0.0)]);
    }

    let state = LazyGridState::new(0, 0);
    let mut ui = in_a_small_box(*state, ITEM_SIZE);
    scroll_by(&mut ui, *state, ITEM_SIZE * 4.5);
    assert!(!is_displayed(&mut ui, "2", CONTAINER));
    assert_eq!(top(&mut ui, "3"), -ITEM_SIZE * 0.5);
    assert_position(*state, 3, ITEM_SIZE * 1.5);
    assert_visible(*state, &[(3, -ITEM_SIZE * 1.5)]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, eachPaddingLargerParentSize_initialState
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollByPadding
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollToLastItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollToLastItemByDelta
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollTillTheEnd
#[test]
fn each_padding_larger_than_the_grid() {
    let state = LazyGridState::new(0, 0);
    let mut ui = in_a_small_box(*state, ITEM_SIZE * 2.0);
    assert!(!is_displayed(&mut ui, "0", CONTAINER));
    assert_position(*state, 0, 0.0);
    assert_visible(*state, &[(0, 0.0)]);
    let info = state.layout_info();
    assert_eq!((info.viewport_start_offset, info.viewport_end_offset), (-100, -25));

    let state = LazyGridState::new(0, 0);
    let mut ui = in_a_small_box(*state, ITEM_SIZE * 2.0);
    scroll_by(&mut ui, *state, ITEM_SIZE * 2.0);
    assert_eq!((top(&mut ui, "0"), top(&mut ui, "1")), (0.0, ITEM_SIZE));
    assert!(!is_displayed(&mut ui, "2", CONTAINER));
    assert_position(*state, 2, 0.0);
    assert_visible(*state, &[(0, -ITEM_SIZE * 2.0), (1, -ITEM_SIZE), (2, 0.0)]);

    for by_delta in [false, true] {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = in_a_small_box(handle, ITEM_SIZE * 2.0);
        if by_delta {
            scroll_by(&mut ui, handle, ITEM_SIZE * 3.0);
        } else {
            let _ = run(&mut ui, async move { handle.scroll_to_item(3, 0).await });
        }
        assert!(!is_displayed(&mut ui, "0", CONTAINER), "by delta: {by_delta}");
        assert_eq!(
            (top(&mut ui, "1"), top(&mut ui, "2")),
            (0.0, ITEM_SIZE),
            "by delta: {by_delta}"
        );
        assert!(!is_displayed(&mut ui, "3", CONTAINER), "by delta: {by_delta}");
        assert_position(handle, 3, 0.0);
        assert_visible(handle, &[(1, -ITEM_SIZE * 2.0), (2, -ITEM_SIZE), (3, 0.0)]);
    }

    let state = LazyGridState::new(0, 0);
    let mut ui = in_a_small_box(*state, ITEM_SIZE * 2.0);
    scroll_by(&mut ui, *state, ITEM_SIZE * 1.5 + ITEM_SIZE * 2.0 + ITEM_SIZE * 3.0);
    assert!(!is_displayed(&mut ui, "3", CONTAINER));
    assert_position(*state, 3, ITEM_SIZE * 3.5);
    assert_visible(*state, &[(3, -ITEM_SIZE * 3.5)]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridsContentPaddingTest.kt, verticalGrid_unevenPaddingWithRtl
#[test]
fn uneven_padding_right_to_left() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
            LazyVerticalGrid!(
                GridCells::Fixed(1),
                modifier = Modifier.test_tag("list").height((ITEM_SIZE * 2.0).dp()),
                state = handle,
                content_padding =
                    PaddingValues { start: 20.dp(), end: 8.dp(), ..PaddingValues::ZERO },
                |scope: LazyGridScope| {
                    scope.items(4, |index| {
                        Box!(
                            modifier = Modifier.test_tag(index.to_string()).size(ITEM_SIZE.dp()),
                            {}
                        )
                    });
                }
            );
        });
    });
    let first = bounds(&mut ui, "0");
    // Right to left, the left padding is the end's.
    assert_eq!((first.top, first.left), (0.0, 8.0));
    scroll_by(&mut ui, handle, ITEM_SIZE * 4.0);
    let last = bounds(&mut ui, "3");
    assert_eq!((last.top, last.left), (ITEM_SIZE, 8.0));
}
