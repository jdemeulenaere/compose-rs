//! Lazy lists' content padding, as Compose's `LazyListsContentPaddingTest`: applied, not moving
//! the scroll position, items scrolled into it, to the end and back, wrapping content, reversed,
//! padding larger than the list, right to left. Both orientations; density 1, so Compose's dps
//! are pixels (its items are 50 px, its small padding 12 px).

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Dp, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, PaddingValues, UnitExt};

use crate::lazy_support::{Lazy, LazyColumnOrRow, bounds};
use crate::scroll_support::run;

const LIST: &str = "LazyList";
const ITEM: &str = "item";
const CONTAINER: &str = "container";
const ITEM_SIZE: f32 = 50.0;
const SMALL_PADDING: f32 = 12.0;

fn orientations() -> [Lazy; 2] {
    [Lazy::column(), Lazy::row()]
}

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

/// Compose's tests' `PaddingValues(mainAxis, crossAxis)`.
fn main_cross(lazy: Lazy, main: f32, cross: f32) -> PaddingValues {
    lazy.padding(main.dp(), main.dp(), cross.dp(), cross.dp())
}

fn scroll_by(ui: &mut ComposeTestRule, state: LazyListState, distance: f32) {
    let _ = run(ui, async move { state.scroll_by(distance).await });
}

fn scroll_to(ui: &mut ComposeTestRule, state: LazyListState, index: usize) {
    let _ = run(ui, async move { state.scroll_to_item(index, 0).await });
}

/// Compose's `assertScrollPosition`: the index, and the offset within half a pixel.
fn assert_position(lazy: Lazy, state: LazyListState, index: usize, offset: f32) {
    assert_eq!(state.first_visible_item_index(), index, "{lazy:?}");
    let actual = state.first_visible_item_scroll_offset() as f32;
    assert!((actual - offset).abs() <= 0.5, "{lazy:?}: offset {actual}, not {offset}");
}

/// Compose's `assertVisibleItems`: the items in view's indices and offsets.
fn assert_visible(lazy: Lazy, state: LazyListState, expected: &[(usize, f32)]) {
    let info = state.layout_info();
    let actual: Vec<(usize, i32)> =
        info.visible_items_info.iter().map(|item| (item.index, item.offset)).collect();
    let expected: Vec<(usize, i32)> =
        expected.iter().map(|&(index, offset)| (index, offset.round() as i32)).collect();
    assert_eq!(actual, expected, "{lazy:?}");
}

/// Compose's `assertLayoutInfoOffsetRange`.
fn assert_offset_range(lazy: Lazy, state: LazyListState, from: f32, to: f32) {
    let info = state.layout_info();
    let range = (info.viewport_start_offset, info.viewport_end_offset);
    assert_eq!(range, (from.round() as i32, to.round() as i32), "{lazy:?}");
}

/// Four items of `ITEM_SIZE` in a list of `list` with `padding` before and after along the
/// axis, tagged by index.
fn four_items(lazy: Lazy, state: LazyListState, list: Modifier, padding: f32) -> ComposeTestRule {
    let list = std::rc::Rc::new(list);
    rule(move || {
        LazyColumnOrRow(
            lazy,
            (*list).clone(),
            state,
            main_cross(lazy, padding, 0.0),
            |scope: LazyListScope| {
                scope.items_list(vec![0, 1, 2, 3], |_, item: &u32| {
                    Spacer(Modifier.required_size(ITEM_SIZE.dp()).test_tag(item.to_string()));
                });
            },
        );
    })
}

/// Four items of `ITEM_SIZE` in a box of 1.5 items, the list with `padding` along the axis.
fn in_a_small_box(lazy: Lazy, state: LazyListState, padding: f32) -> ComposeTestRule {
    rule(move || {
        Box!(modifier = Modifier.test_tag(CONTAINER).size((ITEM_SIZE * 1.5).dp()), {
            LazyColumnOrRow(
                lazy,
                Modifier,
                state,
                main_cross(lazy, padding, 0.0),
                |scope: LazyListScope| {
                    scope.items(4, |index| {
                        Box!(
                            modifier = Modifier.test_tag(index.to_string()).size(ITEM_SIZE.dp()),
                            {}
                        )
                    });
                },
            );
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, contentPaddingIsApplied
#[test]
fn the_padding_is_applied() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let container = ITEM_SIZE * 2.0;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(container.dp()).test_tag(LIST),
                handle,
                main_cross(lazy, ITEM_SIZE, SMALL_PADDING),
                move |scope: LazyListScope| {
                    let item_scope = scope.item_scope();
                    scope.items_list(vec![1], move |_, _: &i32| {
                        Spacer(
                            lazy.fill_parent_max_cross_axis(&item_scope)
                                .then(lazy.main_axis_size(ITEM_SIZE.dp()))
                                .test_tag(ITEM),
                        );
                    });
                },
            );
        });
        assert_eq!(lazy.cross_start(&mut ui, ITEM), SMALL_PADDING, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, ITEM), ITEM_SIZE, "{lazy:?}");
        assert_eq!(lazy.cross_size(&mut ui, ITEM), container - SMALL_PADDING * 2.0, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, ITEM), ITEM_SIZE, "{lazy:?}");
        scroll_by(&mut ui, handle, ITEM_SIZE);
        assert_eq!(lazy.main_start(&mut ui, ITEM), 0.0, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, ITEM), ITEM_SIZE, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, contentPaddingIsNotAffectingScrollPosition
#[test]
fn the_padding_does_not_move_the_scroll_position() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size((ITEM_SIZE * 2.0).dp()).test_tag(LIST),
                handle,
                main_cross(lazy, ITEM_SIZE, 0.0),
                move |scope: LazyListScope| {
                    let item_scope = scope.item_scope();
                    scope.items_list(vec![1], move |_, _: &i32| {
                        Spacer(
                            lazy.fill_parent_max_cross_axis(&item_scope)
                                .then(lazy.main_axis_size(ITEM_SIZE.dp()))
                                .test_tag(ITEM),
                        );
                    });
                },
            );
        });
        assert_position(lazy, handle, 0, 0.0);
        scroll_by(&mut ui, handle, ITEM_SIZE);
        assert_position(lazy, handle, 0, ITEM_SIZE);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, scrollForwardItemWithinStartPaddingDisplayed
#[test]
fn an_item_scrolled_into_the_start_padding_shows() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let padding = ITEM_SIZE * 1.5;
        let mut ui = four_items(
            lazy,
            *state,
            Modifier.required_size((padding * 2.0 + ITEM_SIZE).dp()),
            padding,
        );
        for (tag, start) in
            [("0", padding), ("1", ITEM_SIZE + padding), ("2", ITEM_SIZE * 2.0 + padding)]
        {
            assert_eq!(lazy.main_start(&mut ui, tag), start, "{lazy:?}");
        }
        scroll_by(&mut ui, *state, padding);
        assert_position(lazy, *state, 1, padding - ITEM_SIZE);
        for (index, tag) in ["0", "1", "2", "3"].into_iter().enumerate() {
            assert_eq!(lazy.main_start(&mut ui, tag), ITEM_SIZE * index as f32, "{lazy:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, scrollBackwardItemWithinStartPaddingDisplayed
#[test]
fn an_item_scrolled_back_into_the_start_padding_shows() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let padding = ITEM_SIZE * 1.5;
        let mut ui = four_items(
            lazy,
            *state,
            Modifier.required_size((ITEM_SIZE + padding * 2.0).dp()),
            padding,
        );
        scroll_by(&mut ui, *state, ITEM_SIZE * 3.0);
        scroll_by(&mut ui, *state, -ITEM_SIZE * 1.5);
        assert_position(lazy, *state, 1, ITEM_SIZE * 0.5);
        for (index, tag) in ["0", "1", "2", "3"].into_iter().enumerate() {
            let start = ITEM_SIZE * (1.5 + index as f32) - padding;
            assert_eq!(lazy.main_start(&mut ui, tag), start, "{lazy:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, scrollForwardTillTheEnd
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, scrollForwardTillTheEndAndABitBack
#[test]
fn scrolling_to_the_end_shows_the_end_padding() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let padding = ITEM_SIZE * 1.5;
        let mut ui = four_items(
            lazy,
            *state,
            Modifier.required_size((padding * 2.0 + ITEM_SIZE).dp()),
            padding,
        );
        scroll_by(&mut ui, *state, ITEM_SIZE * 3.0);
        for _ in 0..2 {
            assert_position(lazy, *state, 3, 0.0);
            for index in 1..=3 {
                let start = ITEM_SIZE * index as f32 - padding;
                assert_eq!(lazy.main_start(&mut ui, &index.to_string()), start, "{lazy:?}");
            }
            // No room left: nothing changes.
            scroll_by(&mut ui, *state, 10.0);
        }
        scroll_by(&mut ui, *state, -ITEM_SIZE / 2.0);
        assert_position(lazy, *state, 2, ITEM_SIZE / 2.0);
        for index in 1..=3 {
            let start = ITEM_SIZE * (index as f32 + 0.5) - padding;
            assert_eq!(lazy.main_start(&mut ui, &index.to_string()), start, "{lazy:?}");
        }
    }
}

/// Compose's `PaddingValues(beforeContentCrossAxis = 2, beforeContent = 4,
/// afterContentCrossAxis = 6, afterContent = 8)`.
fn uneven(lazy: Lazy) -> PaddingValues {
    lazy.padding(4.dp(), 8.dp(), 2.dp(), 6.dp())
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, contentPaddingAndWrapContent
#[test]
fn a_wrapping_list_wraps_its_padding_too() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            Box!(modifier = Modifier.test_tag(CONTAINER), {
                LazyColumnOrRow(
                    lazy,
                    Modifier,
                    remember_lazy_list_state!(),
                    uneven(lazy),
                    |scope: LazyListScope| {
                        scope.items_list(vec![1], |_, _: &i32| {
                            Spacer(Modifier.required_size(ITEM_SIZE.dp()).test_tag(ITEM))
                        });
                    },
                );
            });
        });
        assert_eq!(lazy.cross_start(&mut ui, ITEM), 2.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, ITEM), 4.0, "{lazy:?}");
        assert_eq!(
            (lazy.cross_size(&mut ui, ITEM), lazy.main_size(&mut ui, ITEM)),
            (ITEM_SIZE, ITEM_SIZE)
        );
        assert_eq!(
            (lazy.cross_start(&mut ui, CONTAINER), lazy.main_start(&mut ui, CONTAINER)),
            (0.0, 0.0)
        );
        assert_eq!(lazy.cross_size(&mut ui, CONTAINER), ITEM_SIZE + 8.0, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, CONTAINER), ITEM_SIZE + 12.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, contentPaddingAndNoContent
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, contentPaddingAndZeroSizedItem
#[test]
fn a_list_without_items_is_its_padding() {
    for lazy in orientations() {
        for no_items_block in [true, false] {
            let mut ui = rule(move || {
                Box!(modifier = Modifier.test_tag(CONTAINER), {
                    LazyColumnOrRow(
                        lazy,
                        Modifier,
                        remember_lazy_list_state!(),
                        uneven(lazy),
                        move |scope: LazyListScope| {
                            if !no_items_block {
                                scope.items(0, |_| {});
                            }
                        },
                    );
                });
            });
            assert_eq!(
                (lazy.cross_start(&mut ui, CONTAINER), lazy.main_start(&mut ui, CONTAINER)),
                (0.0, 0.0)
            );
            assert_eq!(lazy.cross_size(&mut ui, CONTAINER), 8.0, "{lazy:?}");
            assert_eq!(lazy.main_size(&mut ui, CONTAINER), 12.0, "{lazy:?}");
        }
    }
}

/// Three items of `ITEM_SIZE` in a reversed list of three, with `before` and `after` padding.
fn reversed_three(lazy: Lazy, state: LazyListState, before: f32, after: f32) -> ComposeTestRule {
    let lazy = Lazy { reverse_layout: true, ..lazy };
    rule(move || {
        LazyColumnOrRow(
            lazy,
            Modifier.required_size((ITEM_SIZE * 3.0).dp()).test_tag(LIST),
            state,
            lazy.padding(before.dp(), after.dp(), Dp(0.0), Dp(0.0)),
            |scope: LazyListScope| {
                scope.items(3, |index| {
                    Box!(
                        modifier =
                            Modifier.required_size(ITEM_SIZE.dp()).test_tag(index.to_string()),
                        {}
                    )
                });
            },
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, contentPaddingAndReverseLayout
#[test]
fn a_reversed_list_pads_its_ends() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let (top, bottom, list) = (ITEM_SIZE * 2.0, ITEM_SIZE / 2.0, ITEM_SIZE * 3.0);
        let mut ui = reversed_three(lazy, *state, top, bottom);
        assert_eq!(lazy.main_start(&mut ui, "0"), list - bottom - ITEM_SIZE, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "1"), list - bottom - ITEM_SIZE * 2.0, "{lazy:?}");
        // Partly in view.
        assert_eq!(lazy.main_start(&mut ui, "2"), -ITEM_SIZE / 2.0, "{lazy:?}");
        // To the top.
        scroll_by(&mut ui, *state, ITEM_SIZE * 2.5);
        assert_eq!(lazy.main_start(&mut ui, "2"), top, "{lazy:?}");
        assert!(
            !ui.on_node_with_tag("1").is_displayed() && !ui.on_node_with_tag("0").is_displayed(),
            "{lazy:?}"
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, contentLargePaddingAndReverseLayout
#[test]
fn a_reversed_list_with_large_padding_shows_little() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let (top, bottom) = (ITEM_SIZE * 2.0, ITEM_SIZE * 2.0);
        let mut ui = reversed_three(lazy, *state, top, bottom);
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0, "{lazy:?}");
        ui.on_node_with_tag("1").assert_does_not_exist();
        // To the top.
        scroll_by(&mut ui, *state, ITEM_SIZE * 5.0);
        assert_eq!(lazy.main_start(&mut ui, "2"), top, "{lazy:?}");
        assert!(
            !ui.on_node_with_tag("1").is_displayed() && !ui.on_node_with_tag("0").is_displayed(),
            "{lazy:?}"
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, overscrollWithContentPadding
#[test]
fn a_scroll_past_the_end_into_the_padding_stops_at_the_end() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            Box!(
                modifier =
                    Modifier.test_tag(CONTAINER).size((ITEM_SIZE + SMALL_PADDING * 2.0).dp()),
                {
                    LazyColumnOrRow(
                        lazy,
                        Modifier,
                        handle,
                        main_cross(lazy, SMALL_PADDING, 0.0),
                        |scope: LazyListScope| {
                            let item_scope = scope.item_scope();
                            scope.items(2, move |index| {
                                Box!(
                                    modifier = Modifier
                                        .test_tag(index.to_string())
                                        .fill_parent_max_size(&item_scope),
                                    {}
                                );
                            });
                        },
                    );
                }
            );
        });
        assert_eq!(lazy.main_start(&mut ui, "0"), SMALL_PADDING, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, "0"), ITEM_SIZE, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "1"), SMALL_PADDING + ITEM_SIZE, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, "1"), ITEM_SIZE, "{lazy:?}");
        // Past the most it can (the item's size) by more than the padding.
        scroll_by(&mut ui, handle, ITEM_SIZE + SMALL_PADDING * 1.5);
        assert_eq!(lazy.main_start(&mut ui, "1"), SMALL_PADDING, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, "1"), ITEM_SIZE, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "0"), SMALL_PADDING - ITEM_SIZE, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, "0"), ITEM_SIZE, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, totalPaddingLargerParentSize_initialState
#[test]
fn padding_larger_in_all_than_the_list_at_first() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE);
        assert_eq!(lazy.main_start(&mut ui, "0"), ITEM_SIZE, "{lazy:?}");
        ui.on_node_with_tag("1").assert_does_not_exist();
        assert_position(lazy, *state, 0, 0.0);
        assert_visible(lazy, *state, &[(0, 0.0)]);
        assert_offset_range(lazy, *state, -ITEM_SIZE, ITEM_SIZE * 0.5);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollByPadding
#[test]
fn padding_larger_in_all_than_the_list_scrolled_by_the_padding() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE);
        scroll_by(&mut ui, *state, ITEM_SIZE);
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "1"), ITEM_SIZE, "{lazy:?}");
        assert!(!ui.on_node_with_tag("2").is_displayed(), "{lazy:?}");
        assert_position(lazy, *state, 1, 0.0);
        assert_visible(lazy, *state, &[(0, -ITEM_SIZE), (1, 0.0)]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollToLastItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollToLastItemByDelta
#[test]
fn padding_larger_in_all_than_the_list_at_the_last_item() {
    for lazy in orientations() {
        for by_delta in [false, true] {
            let state = LazyListState::new(0, 0);
            let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE);
            if by_delta {
                scroll_by(&mut ui, *state, ITEM_SIZE * 3.0);
                assert!(!ui.on_node_with_tag("1").is_displayed(), "{lazy:?}");
            } else {
                scroll_to(&mut ui, *state, 3);
                ui.on_node_with_tag("1").assert_does_not_exist();
            }
            assert_eq!(lazy.main_start(&mut ui, "2"), 0.0, "{lazy:?}");
            assert_eq!(lazy.main_start(&mut ui, "3"), ITEM_SIZE, "{lazy:?}");
            assert_position(lazy, *state, 3, 0.0);
            assert_visible(lazy, *state, &[(2, -ITEM_SIZE), (3, 0.0)]);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, totalPaddingLargerParentSize_scrollTillTheEnd
#[test]
fn padding_larger_in_all_than_the_list_at_the_end() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE);
        // The whole end padding in view.
        scroll_by(&mut ui, *state, ITEM_SIZE * 4.5);
        assert!(!ui.on_node_with_tag("2").is_displayed(), "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "3"), -ITEM_SIZE * 0.5, "{lazy:?}");
        assert_position(lazy, *state, 3, ITEM_SIZE * 1.5);
        assert_visible(lazy, *state, &[(3, -ITEM_SIZE * 1.5)]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, eachPaddingLargerParentSize_initialState
#[test]
fn each_padding_larger_than_the_list_at_first() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE * 2.0);
        assert!(!ui.on_node_with_tag("0").is_displayed(), "{lazy:?}");
        assert_position(lazy, *state, 0, 0.0);
        assert_visible(lazy, *state, &[(0, 0.0)]);
        assert_offset_range(lazy, *state, -ITEM_SIZE * 2.0, -ITEM_SIZE * 0.5);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollByPadding
#[test]
fn each_padding_larger_than_the_list_scrolled_by_the_padding() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE * 2.0);
        scroll_by(&mut ui, *state, ITEM_SIZE * 2.0);
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "1"), ITEM_SIZE, "{lazy:?}");
        assert!(!ui.on_node_with_tag("2").is_displayed(), "{lazy:?}");
        assert_position(lazy, *state, 2, 0.0);
        assert_visible(lazy, *state, &[(0, -ITEM_SIZE * 2.0), (1, -ITEM_SIZE), (2, 0.0)]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollToLastItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollToLastItemByDelta
#[test]
fn each_padding_larger_than_the_list_at_the_last_item() {
    for lazy in orientations() {
        for by_delta in [false, true] {
            let state = LazyListState::new(0, 0);
            let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE * 2.0);
            if by_delta {
                scroll_by(&mut ui, *state, ITEM_SIZE * 3.0);
            } else {
                scroll_to(&mut ui, *state, 3);
            }
            assert!(!ui.on_node_with_tag("0").is_displayed(), "{lazy:?}");
            assert_eq!(lazy.main_start(&mut ui, "1"), 0.0, "{lazy:?}");
            assert_eq!(lazy.main_start(&mut ui, "2"), ITEM_SIZE, "{lazy:?}");
            assert!(!ui.on_node_with_tag("3").is_displayed(), "{lazy:?}");
            assert_position(lazy, *state, 3, 0.0);
            assert_visible(lazy, *state, &[(1, -ITEM_SIZE * 2.0), (2, -ITEM_SIZE), (3, 0.0)]);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, eachPaddingLargerParentSize_scrollTillTheEnd
#[test]
fn each_padding_larger_than_the_list_at_the_end() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_small_box(lazy, *state, ITEM_SIZE * 2.0);
        // The box, the start padding and all the items: only the end padding in view.
        scroll_by(&mut ui, *state, ITEM_SIZE * 1.5 + ITEM_SIZE * 2.0 + ITEM_SIZE * 3.0);
        assert!(!ui.on_node_with_tag("3").is_displayed(), "{lazy:?}");
        assert_position(lazy, *state, 3, ITEM_SIZE * 3.5);
        assert_visible(lazy, *state, &[(3, -ITEM_SIZE * 3.5)]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsContentPaddingTest.kt, unevenPaddingWithRtl
#[test]
fn uneven_padding_right_to_left() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let padding = PaddingValues { start: 20.dp(), end: 8.dp(), ..PaddingValues::ZERO };
        let mut ui = rule(move || {
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
                LazyColumnOrRow(
                    lazy,
                    Modifier.test_tag("list").then(lazy.main_axis_size((ITEM_SIZE * 2.0).dp())),
                    handle,
                    padding,
                    |scope: LazyListScope| {
                        scope.items(4, |index| {
                            Box!(
                                modifier =
                                    Modifier.test_tag(index.to_string()).size(ITEM_SIZE.dp()),
                                {}
                            )
                        });
                    },
                );
            });
        });
        // Right to left, the start padding (20) is on the right, the end (8) on the left.
        if lazy.vertical {
            assert_eq!((lazy.main_start(&mut ui, "0"), lazy.cross_start(&mut ui, "0")), (0.0, 8.0));
            assert_eq!(bounds(&mut ui, "list").width(), 28.0 + ITEM_SIZE);
        } else {
            assert_eq!(lazy.cross_start(&mut ui, "0"), 0.0);
            assert_eq!(lazy.main_start(&mut ui, "0"), ITEM_SIZE * 2.0 - ITEM_SIZE - 20.0);
        }
        scroll_by(&mut ui, handle, ITEM_SIZE * 4.0);
        if lazy.vertical {
            assert_eq!(
                (lazy.main_start(&mut ui, "3"), lazy.cross_start(&mut ui, "3")),
                (ITEM_SIZE, 8.0)
            );
        } else {
            assert_eq!((lazy.cross_start(&mut ui, "3"), lazy.main_start(&mut ui, "3")), (0.0, 8.0));
        }
    }
}
