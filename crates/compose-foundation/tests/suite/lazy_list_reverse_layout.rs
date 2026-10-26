//! Reversed lazy lists, as Compose's `LazyListsReverseLayoutTest`: items (and an item's several
//! nodes) from the end, the initial position, drags each way, right to left, the parameter
//! changing. Density 1, so Compose's dps are pixels (its items are 50 px).

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, UnitExt};

use crate::lazy_support::{Lazy, LazyColumnOrRow, no_padding};

const CONTAINER: &str = "ContainerTag";
const ITEM_SIZE: f32 = 50.0;

/// A reversed column and a reversed row.
fn reversed() -> [Lazy; 2] {
    [Lazy { reverse_layout: true, ..Lazy::column() }, Lazy { reverse_layout: true, ..Lazy::row() }]
}

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

fn tagged_box(tag: &str) {
    Box!(modifier = Modifier.required_size(ITEM_SIZE.dp()).test_tag(tag.to_string()), {});
}

/// A wrapping list of an item emitting two boxes, or of two items, right to left or not.
fn two(lazy: Lazy, one_item: bool, rtl: bool) -> ComposeTestRule {
    rule(move || {
        let list = move || {
            LazyColumnOrRow(
                lazy,
                Modifier,
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    if one_item {
                        scope.item(|| {
                            tagged_box("0");
                            tagged_box("1");
                        });
                    } else {
                        scope.item(|| tagged_box("0"));
                        scope.item(|| tagged_box("1"));
                    }
                },
            );
        };
        if rtl {
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), list);
        } else {
            list();
        }
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, column_emitTwoElementsAsOneItem_positionedReversed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, column_emitTwoItems_positionedReversed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_emitTwoElementsAsOneItem_positionedReversed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_emitTwoItems_positionedReversed
#[test]
fn items_and_an_items_nodes_are_laid_out_from_the_end() {
    for lazy in reversed() {
        for one_item in [true, false] {
            let mut ui = two(lazy, one_item, false);
            assert_eq!(lazy.main_start(&mut ui, "1"), 0.0, "{lazy:?}, one item: {one_item}");
            assert_eq!(lazy.main_start(&mut ui, "0"), ITEM_SIZE, "{lazy:?}, one item: {one_item}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_rtl_emitTwoElementsAsOneItem_positionedReversed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_rtl_emitTwoItems_positionedReversed
#[test]
fn a_reversed_row_right_to_left_lays_out_from_the_left() {
    let lazy = reversed()[1];
    for one_item in [true, false] {
        let mut ui = two(lazy, one_item, true);
        assert_eq!(lazy.main_start(&mut ui, "1"), ITEM_SIZE, "one item: {one_item}");
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0, "one item: {one_item}");
    }
}

/// `count` items of `ITEM_SIZE` in a reversed list of two, right to left or not.
fn in_a_list_of_two(lazy: Lazy, state: LazyListState, count: usize, rtl: bool) -> ComposeTestRule {
    rule(move || {
        let list = move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size((ITEM_SIZE * 2.0).dp()).test_tag(CONTAINER),
                state,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items(count, |index| tagged_box(&index.to_string()))
                },
            );
        };
        if rtl {
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), list);
        } else {
            list();
        }
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, column_initialScrollPositionIs0
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_initialScrollPositionIs0
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, column_scrollInWrongDirectionDoesNothing
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_scrollInWrongDirectionDoesNothing
#[test]
fn a_reversed_list_starts_at_its_start_and_a_drag_back_does_nothing() {
    for lazy in reversed() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_list_of_two(lazy, *state, 3, false);
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            (0, 0)
        );
        // Down (or right): reversed, that's backward.
        lazy.drag_main_axis_by(&mut ui, CONTAINER, ITEM_SIZE);
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            (0, 0)
        );
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "0"), ITEM_SIZE, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, column_scrollForwardHalfWay
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_scrollForwardHalfWay
#[test]
fn a_drag_forward_halfway_moves_the_items_towards_the_end() {
    for lazy in reversed() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_list_of_two(lazy, *state, 3, false);
        lazy.drag_main_axis_by(&mut ui, CONTAINER, -ITEM_SIZE * 0.5);
        assert_eq!(state.first_visible_item_index(), 0, "{lazy:?}");
        let scrolled = state.first_visible_item_scroll_offset() as f32;
        assert!(scrolled > 0.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "2"), -ITEM_SIZE + scrolled, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "1"), scrolled, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "0"), ITEM_SIZE + scrolled, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, column_scrollForwardTillTheEnd
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_scrollForwardTillTheEnd
#[test]
fn a_drag_past_the_end_stops_there() {
    for lazy in reversed() {
        let state = LazyListState::new(0, 0);
        let mut ui = in_a_list_of_two(lazy, *state, 4, false);
        // A bit more than it can, to check it stops where it should.
        lazy.drag_main_axis_by(&mut ui, CONTAINER, -ITEM_SIZE * 2.2);
        let offset = state.first_visible_item_scroll_offset() as f32
            + ITEM_SIZE * state.first_visible_item_index() as f32;
        assert_eq!(offset, ITEM_SIZE * 2.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "3"), 0.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "2"), ITEM_SIZE, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_rtl_scrollForwardHalfWay
#[test]
fn a_reversed_row_right_to_left_drags_forward_to_the_right() {
    let lazy = reversed()[1];
    let state = LazyListState::new(0, 0);
    let mut ui = in_a_list_of_two(lazy, *state, 3, true);
    lazy.drag_main_axis_by(&mut ui, CONTAINER, ITEM_SIZE * 0.5);
    assert_eq!(state.first_visible_item_index(), 0);
    let scrolled = state.first_visible_item_scroll_offset() as f32;
    assert!(scrolled > 0.0);
    assert_eq!(lazy.main_start(&mut ui, "0"), -scrolled);
    assert_eq!(lazy.main_start(&mut ui, "1"), ITEM_SIZE - scrolled);
    assert_eq!(lazy.main_start(&mut ui, "2"), ITEM_SIZE * 2.0 - scrolled);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, column_whenParameterChanges
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListsReverseLayoutTest.kt, row_whenParameterChanges
#[test]
fn turning_the_reversal_off_lays_out_from_the_start() {
    for lazy in reversed() {
        let reverse = mutable_state_of(true);
        let handle = reverse.handle();
        let mut ui = rule(move || {
            let lazy = Lazy { reverse_layout: handle.get(), ..lazy };
            LazyColumnOrRow(
                lazy,
                Modifier,
                remember_lazy_list_state!(),
                no_padding(),
                |scope: LazyListScope| {
                    scope.item(|| {
                        tagged_box("0");
                        tagged_box("1");
                    });
                },
            );
        });
        assert_eq!(
            (lazy.main_start(&mut ui, "1"), lazy.main_start(&mut ui, "0")),
            (0.0, ITEM_SIZE)
        );
        reverse.set(false);
        ui.wait_for_idle();
        assert_eq!(
            (lazy.main_start(&mut ui, "0"), lazy.main_start(&mut ui, "1")),
            (0.0, ITEM_SIZE)
        );
    }
}
