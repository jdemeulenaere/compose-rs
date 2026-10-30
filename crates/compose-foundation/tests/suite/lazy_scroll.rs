//! A lazy list's scrolls from its state: `scroll_to_item` (offsets past the bounds, negative),
//! `animate_scroll_by`, `animate_scroll_to_item` (frame by frame along a spring), and
//! `can_scroll_*`, in both orientations.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_animation::{animate, spring};
use compose_foundation::layout::*;
use compose_foundation::*;

use compose_test::*;
use compose_ui::{Dp, IntSize, PaddingValues, UnitExt};

use crate::scroll_support::{launch, run};

const TAG: &str = "LazyList";
const ITEMS: usize = 20;
const ITEM: i32 = 100;

/// Twenty 100 px items in a list `container` px long, as Compose's `LazyScrollTest`'s content.
fn list(
    vertical: bool,
    state: LazyListState,
    spacing: i32,
    container: i32,
    before: i32,
    after: i32,
) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let spacing = Dp(spacing as f32);
        if vertical {
            LazyColumn!(
                modifier = Modifier.height(container.dp()).test_tag(TAG),
                state = state,
                content_padding =
                    PaddingValues { top: before.dp(), bottom: after.dp(), ..PaddingValues::ZERO },
                vertical_arrangement = Arrangement::spaced_by(spacing),
                |scope: LazyListScope| {
                    scope.items(ITEMS, |_| Spacer(Modifier.height(ITEM.dp())));
                }
            );
        } else {
            LazyRow!(
                modifier = Modifier.width(container.dp()).test_tag(TAG),
                state = state,
                content_padding =
                    PaddingValues { start: before.dp(), end: after.dp(), ..PaddingValues::ZERO },
                horizontal_arrangement = Arrangement::spaced_by(spacing),
                |scope: LazyListScope| {
                    scope.items(ITEMS, |_| Spacer(Modifier.width(ITEM.dp())));
                }
            );
        }
    })
}

/// The default content: no spacing, three items long, no padding.
fn default_list(vertical: bool, state: LazyListState) -> ComposeTestRule {
    list(vertical, state, 0, ITEM * 3, 0, 0)
}

fn position(state: &LazyListState) -> (usize, i32) {
    (state.first_visible_item_index(), state.first_visible_item_scroll_offset())
}

fn item_offset(state: &LazyListState, index: usize) -> i32 {
    state.layout_info().visible_items_info.iter().find(|item| item.index == index).unwrap().offset
}

const ORIENTATIONS: [bool; 2] = [true, false];

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, setupWorks
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, canScrollForward
#[test]
fn a_new_list_is_at_its_start() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(0, 0);
        let _ui = default_list(vertical, *state);
        assert_eq!(position(&state), (0, 0));
        assert!(state.can_scroll_forward() && !state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, scrollToItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, scrollToItemWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, scrollToItemWithNegativeOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, scrollToItemWithPositiveOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, scrollToItemWithNegativeOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, scrollToItemWithIndexLargerThanItemsCount
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItemWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItemWithNegativeOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItemWithPositiveOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItemWithNegativeOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItemWithIndexLargerThanItemsCount
#[test]
fn scrolls_to_an_item_land_where_compose_lands() {
    for animated in [false, true] {
        for vertical in ORIENTATIONS {
            let context = format!("animated {animated}, vertical {vertical}");
            let scroll = move |ui: &mut ComposeTestRule, state: LazyListState, index, offset| {
                let _ = run(ui, async move {
                    if animated {
                        state.animate_scroll_to_item(index, offset).await
                    } else {
                        state.scroll_to_item(index, offset).await
                    }
                });
            };
            let state = LazyListState::new(0, 0);
            let mut ui = default_list(vertical, *state);
            scroll(&mut ui, *state, 3, 0);
            assert_eq!(position(&state), (3, 0), "{context}");
            scroll(&mut ui, *state, 5, 10);
            assert_eq!(position(&state), (5, 10), "{context}");
            scroll(&mut ui, *state, 3, 10);
            assert_eq!(position(&state), (3, 10), "{context}");
            // A negative offset: before the item.
            scroll(&mut ui, *state, 3, -10);
            assert_eq!(state.first_visible_item_index(), 2, "{context}");
            assert_eq!(item_offset(&state, 3), 10, "{context}");
            // Past the end: the end.
            scroll(&mut ui, *state, ITEMS - 3, 10);
            assert_eq!(position(&state), (ITEMS - 3, 0), "{context}: not 10");
            scroll(&mut ui, *state, 1, -(ITEM + 10));
            assert_eq!(position(&state), (0, 0), "{context}: not -10");
            scroll(&mut ui, *state, ITEMS + 2, 0);
            assert_eq!(state.first_visible_item_index(), ITEMS - 3, "{context}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollBy
#[test]
fn animate_scroll_by_lands_at_the_distance() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = default_list(vertical, handle);
        let _ = run(&mut ui, async move { handle.animate_scroll_by(320.0, spring()).await });
        assert_eq!(position(&state), (3, 20));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollBySemantics
#[test]
fn animated_scrolls_move_the_semantics_scroll_range() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = default_list(vertical, handle);
        let range = |ui: &mut ComposeTestRule| {
            let config = ui.on_node_with_tag(TAG).fetch_semantics_node().config;
            let range = if vertical {
                config.get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
            } else {
                config.get(&compose_ui::SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE)
            };
            *range.expect("a scroll range")
        };
        let _ = run(&mut ui, async move { handle.animate_scroll_by(1.0, spring()).await });
        let first = range(&mut ui);
        assert!(first.value as i32 != 0 && (first.value as i32) < first.max_value as i32);
        let _ = run(&mut ui, async move { handle.animate_scroll_by(1.0, spring()).await });
        let second = range(&mut ui);
        assert!(second.value as i32 != first.value as i32);
        assert!((second.value as i32) < second.max_value as i32);
        let _ = run(&mut ui, async move { handle.animate_scroll_by(10_000.0, spring()).await });
        let last = range(&mut ui);
        assert_eq!(last.value as i32, last.max_value as i32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItemWithOffsetLargerThanItemSize_forward
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animateScrollToItemWithOffsetLargerThanItemSize_backward
#[test]
fn an_animated_scroll_with_an_offset_larger_than_an_item_lands_on_the_right_item() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = default_list(vertical, handle);
        let _ = run(&mut ui, async move { handle.animate_scroll_to_item(10, -ITEM * 3).await });
        assert_eq!(position(&state), (7, 0));
        let _ = run(&mut ui, async move {
            let _ = handle.scroll_to_item(10, 0).await;
            handle.animate_scroll_to_item(0, ITEM * 3).await
        });
        assert_eq!(position(&state), (3, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, canScrollBackward
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, canScrollForwardAndBackward
#[test]
fn the_list_knows_which_ways_it_can_scroll() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = default_list(vertical, handle);
        let _ = run(&mut ui, async move { handle.scroll_to_item(ITEMS, 0).await });
        assert_eq!(state.first_visible_item_index(), ITEMS - 3);
        assert!(!state.can_scroll_forward() && state.can_scroll_backward());
        let _ = run(&mut ui, async move { handle.scroll_to_item(1, 0).await });
        assert_eq!(state.first_visible_item_index(), 1);
        assert!(state.can_scroll_forward() && state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, canScrollForwardAndBackward_afterSmallScrollFromStart
#[test]
fn a_small_scroll_from_the_start_scrolls_both_ways() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = list(vertical, handle, 0, ITEM * 3 / 2, 0, 0);
        let delta = (ITEM as f32 / 3.0).round();
        let _ = run(&mut ui, async move { handle.scroll_by(delta).await });
        assert_eq!(state.first_visible_item_scroll_offset(), delta as i32);
        assert!(state.can_scroll_forward() && state.can_scroll_backward());
        let _ = run(&mut ui, async move { handle.scroll_by(-delta).await });
        assert!(state.can_scroll_forward() && !state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, canScrollForwardAndBackward_afterSmallScrollFromEnd
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, canScrollForwardAndBackward_afterSmallScrollFromEnd_withContentPadding
#[test]
fn a_small_scroll_from_the_end_scrolls_both_ways() {
    for after in [0, 2] {
        for vertical in ORIENTATIONS {
            let state = LazyListState::new(0, 0);
            let handle = *state;
            let mut ui = list(vertical, handle, 0, ITEM * 3 / 2, 0, after);
            let delta = -(ITEM as f32 / 3.0).round();
            let _ = run(&mut ui, async move { handle.scroll_to_item(ITEMS, 0).await });
            assert!(!state.can_scroll_forward() && state.can_scroll_backward());
            let _ = run(&mut ui, async move { handle.scroll_by(delta).await });
            assert!(state.can_scroll_forward() && state.can_scroll_backward());
            let _ = run(&mut ui, async move { handle.scroll_by(-delta).await });
            assert!(!state.can_scroll_forward() && state.can_scroll_backward(), "after {after}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, overScrollingBackShouldIgnoreBeforeContentPadding
#[test]
fn scrolling_back_past_the_start_stops_at_the_start_padding() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = list(vertical, handle, 0, ITEM * 3, 5, 0);
        let item = ITEM as f32;
        let _ = run(&mut ui, async move { handle.scroll_by(item).await });
        let consumed = run(&mut ui, async move { handle.scroll_by(-(item + 10.0)).await });
        assert_eq!(consumed, Ok(-item));
    }
}

/// Checks that an animated scroll from `from` to `to` follows a spring from the one position
/// to the other frame by frame, as Compose's `assertSpringAnimation` (within a pixel).
fn assert_spring_animation(vertical: bool, spacing: i32, to: (usize, i32), from: (usize, i32)) {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = list(vertical, handle, spacing, ITEM * 3, 0, 0);
    if from != (0, 0) {
        let _ = run(&mut ui, async move { handle.scroll_to_item(from.0, from.1).await });
    }
    assert_eq!(position(&state), from);
    let step = ITEM + spacing;
    let start = (from.0 as i32 * step + from.1) as f32;
    let end = (to.0 as i32 * step + to.1) as f32;
    ui.set_auto_advance(false);
    // The spring's values at the same frames, from a plain animation launched alongside.
    let expected: Rc<RefCell<Vec<f32>>> = Rc::default();
    let into = expected.clone();
    launch(async move {
        animate(start, end, 0.0, spring(), |value, _| into.borrow_mut().push(value)).await;
    });
    launch(async move {
        let _ = handle.animate_scroll_to_item(to.0, to.1).await;
    });
    let mut frames = 0;
    loop {
        ui.advance_time_by_frame();
        frames += 1;
        let values = expected.borrow().clone();
        let Some(&value) = values.last() else { continue };
        let actual = state.first_visible_item_index() as i32 * step
            + state.first_visible_item_scroll_offset();
        assert!(
            (actual - value.round() as i32).abs() <= 1,
            "frame {frames}: at {actual}, the spring at {value} (from {start} to {end})"
        );
        if !state.is_scroll_in_progress() || frames > 500 {
            break;
        }
    }
    assert_eq!(position(&state), to);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameForwardToVisibleItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameForwardToVisibleItemWithOffset
#[test]
fn an_animated_scroll_to_an_item_in_view_follows_a_spring() {
    for vertical in ORIENTATIONS {
        assert_spring_animation(vertical, 0, (2, 0), (0, 0));
        assert_spring_animation(vertical, 0, (2, 35), (0, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameForwardToNotVisibleItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameForwardToNotVisibleItemWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameBackward
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameBackwardWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameBackwardWithInitialOffset
#[test]
fn an_animated_scroll_to_an_item_out_of_view_follows_a_spring() {
    for vertical in ORIENTATIONS {
        assert_spring_animation(vertical, 0, (8, 0), (0, 0));
        assert_spring_animation(vertical, 0, (10, 35), (0, 0));
        assert_spring_animation(vertical, 0, (1, 0), (6, 0));
        assert_spring_animation(vertical, 0, (1, 0), (5, 58));
        assert_spring_animation(vertical, 0, (0, 20), (8, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameWithSpacing
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameWithNegativeSpacing
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameBackwardWithSpacing
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyScrollTest.kt, animatePerFrameBackwardWithNegativeSpacing
#[test]
fn an_animated_scroll_with_spacing_follows_a_spring() {
    for vertical in ORIENTATIONS {
        for spacing in [10, -10] {
            assert_spring_animation(vertical, spacing, (8, 0), (0, 0));
            assert_spring_animation(vertical, spacing, (1, 0), (6, 0));
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListAnimateScrollScope.kt, animateToItem_stickyHeader_shouldNotConsiderItemFound
#[test]
fn a_sticking_header_is_not_an_item_found_in_view() {
    for vertical in ORIENTATIONS {
        let state = LazyListState::new(3, 0);
        let handle = *state;
        let _ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            let modifier = if vertical {
                Modifier.width(150.dp()).height(100.dp())
            } else {
                Modifier.height(150.dp()).width(100.dp())
            };
            if vertical {
                LazyColumn!(modifier = modifier, state = handle, |scope: LazyListScope| {
                    scope.sticky_header(|_| Box!(modifier = Modifier.size(150.dp())));
                    scope.items(20, |_| Box!(modifier = Modifier.size(150.dp())));
                });
            } else {
                LazyRow!(modifier = modifier, state = handle, |scope: LazyListScope| {
                    scope.sticky_header(|_| Box!(modifier = Modifier.size(150.dp())));
                    scope.items(20, |_| Box!(modifier = Modifier.size(150.dp())));
                });
            }
        });
        let scope = state.lazy_layout_scroll_scope(ScrollScope::new(|_| 0.0));
        // Sticking, the header isn't in view: its distance is estimated at the average size.
        assert_eq!(scope.calculate_distance_to(0, 0), -3 * 150);
    }
}
