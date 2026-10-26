//! A lazy grid's scrolls from its state: `scroll_to_item` (offsets past the bounds, negative,
//! mid-line items), `animate_scroll_by`, `animate_scroll_to_item` (frame by frame along a
//! spring), and `can_scroll_*`. Compose's test runs on a vertical grid of two columns.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_animation::{animate, spring};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{Dp, IntSize, PaddingValues, UnitExt};

use crate::scroll_support::{launch, run};

const ITEMS: usize = 40;
const ITEM: i32 = 100;

/// Forty 100 px items in two columns, `container` px tall, as Compose's grid `LazyScrollTest`.
fn grid(
    state: LazyGridState,
    spacing: i32,
    container: i32,
    before: i32,
    after: i32,
) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.height(container.dp()),
            state = state,
            content_padding =
                PaddingValues { top: before.dp(), bottom: after.dp(), ..PaddingValues::ZERO },
            vertical_arrangement = Arrangement::spaced_by(Dp(spacing as f32)),
            |scope: LazyGridScope| {
                scope.items(ITEMS, |_| Spacer(Modifier.height(ITEM.dp())));
            }
        );
    })
}

fn default_grid(state: LazyGridState) -> ComposeTestRule {
    grid(state, 0, ITEM * 3, 0, 0)
}

fn position(state: &LazyGridState) -> (usize, i32) {
    (state.first_visible_item_index(), state.first_visible_item_scroll_offset())
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, setupWorks
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, canScrollForward
#[test]
fn a_new_grid_is_at_its_start() {
    let state = LazyGridState::new(0, 0);
    let _ui = default_grid(*state);
    assert_eq!(position(&state), (0, 0));
    assert!(state.can_scroll_forward() && !state.can_scroll_backward());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, scrollToItem
#[test]
fn scrolling_to_an_item_mid_line_shows_its_line() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = default_grid(handle);
    let _ = run(&mut ui, async move { handle.scroll_to_item(2, 0).await });
    assert_eq!(position(&state), (2, 0));
    let _ = run(&mut ui, async move {
        let _ = handle.scroll_to_item(0, 0).await;
        handle.scroll_to_item(3, 0).await
    });
    assert_eq!(position(&state), (2, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, scrollToItemWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, scrollToItemWithNegativeOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, scrollToItemWithPositiveOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, scrollToItemWithNegativeOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, scrollToItemWithIndexLargerThanItemsCount
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItemWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItemWithNegativeOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItemWithPositiveOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItemWithNegativeOffsetLargerThanAvailableSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItemWithIndexLargerThanItemsCount
#[test]
fn scrolls_to_an_item_land_where_compose_lands() {
    for animated in [false, true] {
        let scroll = move |ui: &mut ComposeTestRule, state: LazyGridState, index, offset| {
            let _ = run(ui, async move {
                if animated {
                    state.animate_scroll_to_item(index, offset).await
                } else {
                    state.scroll_to_item(index, offset).await
                }
            });
        };
        let state = LazyGridState::new(0, 0);
        let mut ui = default_grid(*state);
        scroll(&mut ui, *state, 10, 10);
        assert_eq!(position(&state), (10, 10), "animated {animated}");
        scroll(&mut ui, *state, 6, 10);
        assert_eq!(position(&state), (6, 10), "animated {animated}");
        scroll(&mut ui, *state, 6, -10);
        assert_eq!(state.first_visible_item_index(), 4, "animated {animated}");
        let info = state.layout_info();
        let item6 = info.visible_items_info.iter().find(|item| item.index == 6).unwrap();
        assert_eq!(item6.offset.y, 10, "animated {animated}");
        scroll(&mut ui, *state, ITEMS - 6, 10);
        assert_eq!(position(&state), (ITEMS - 6, 0), "animated {animated}: not 10");
        let back = if animated { 2 } else { 1 };
        scroll(&mut ui, *state, back, -(ITEM + 10));
        assert_eq!(position(&state), (0, 0), "animated {animated}: not -10");
        scroll(&mut ui, *state, ITEMS + 4, 0);
        assert_eq!(state.first_visible_item_index(), ITEMS - 6, "animated {animated}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollBy
#[test]
fn animate_scroll_by_lands_at_the_distance() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = default_grid(handle);
    let _ = run(&mut ui, async move { handle.animate_scroll_by(320.0, spring()).await });
    // Line 3, its first item 6, 20 px into it.
    assert_eq!(position(&state), (6, 20));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItemWithOffsetLargerThanItemSize_forward
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animateScrollToItemWithOffsetLargerThanItemSize_backward
#[test]
fn an_animated_scroll_with_an_offset_larger_than_a_line_lands_on_the_right_line() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = default_grid(handle);
    let _ = run(&mut ui, async move { handle.animate_scroll_to_item(10, -ITEM * 3).await });
    assert_eq!(position(&state), (4, 0));
    let _ = run(&mut ui, async move {
        let _ = handle.scroll_to_item(10, 0).await;
        handle.animate_scroll_to_item(0, ITEM * 3).await
    });
    assert_eq!(position(&state), (6, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, canScrollBackward
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, canScrollForwardAndBackward
#[test]
fn the_grid_knows_which_ways_it_can_scroll() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = default_grid(handle);
    let _ = run(&mut ui, async move { handle.scroll_to_item(ITEMS, 0).await });
    assert_eq!(state.first_visible_item_index(), ITEMS - 6);
    assert!(!state.can_scroll_forward() && state.can_scroll_backward());
    let _ = run(&mut ui, async move { handle.scroll_to_item(10, 0).await });
    assert_eq!(state.first_visible_item_index(), 10);
    assert!(state.can_scroll_forward() && state.can_scroll_backward());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, canScrollForwardAndBackward_afterSmallScrollFromStart
#[test]
fn a_small_scroll_from_the_start_scrolls_both_ways() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = grid(handle, 0, ITEM * 3 / 2, 0, 0);
    let delta = (ITEM as f32 / 3.0).round();
    let _ = run(&mut ui, async move { handle.scroll_by(delta).await });
    assert_eq!(state.first_visible_item_scroll_offset(), delta as i32);
    assert!(state.can_scroll_forward() && state.can_scroll_backward());
    let _ = run(&mut ui, async move { handle.scroll_by(-delta).await });
    assert!(state.can_scroll_forward() && !state.can_scroll_backward());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, canScrollForwardAndBackward_afterSmallScrollFromEnd
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, canScrollForwardAndBackward_afterSmallScrollFromEnd_withContentPadding
#[test]
fn a_small_scroll_from_the_end_scrolls_both_ways() {
    for after in [0, 2] {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = grid(handle, 0, ITEM * 3 / 2, 0, after);
        let delta = -(ITEM as f32 / 3.0).round();
        let _ = run(&mut ui, async move { handle.scroll_to_item(ITEMS, 0).await });
        assert!(!state.can_scroll_forward() && state.can_scroll_backward());
        let _ = run(&mut ui, async move { handle.scroll_by(delta).await });
        assert!(state.can_scroll_forward() && state.can_scroll_backward());
        let _ = run(&mut ui, async move { handle.scroll_by(-delta).await });
        assert!(!state.can_scroll_forward() && state.can_scroll_backward(), "after {after}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, overScrollingBackShouldIgnoreBeforeContentPadding
#[test]
fn scrolling_back_past_the_start_stops_at_the_start_padding() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = grid(handle, 0, ITEM * 3, 5, 0);
    let item = ITEM as f32;
    let _ = run(&mut ui, async move { handle.scroll_by(item).await });
    let consumed = run(&mut ui, async move { handle.scroll_by(-(item + 10.0)).await });
    assert_eq!(consumed, Ok(-item));
}

/// Checks that an animated scroll from `from` to `to` follows a spring frame by frame, within a
/// pixel, as Compose's `assertSpringAnimation` (positions in lines of two).
fn assert_spring_animation(spacing: i32, to: (usize, i32), from: (usize, i32)) {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = grid(handle, spacing, ITEM * 3, 0, 0);
    if from != (0, 0) {
        let _ = run(&mut ui, async move { handle.scroll_to_item(from.0, from.1).await });
    }
    assert_eq!(position(&state), from);
    let step = ITEM + spacing;
    let start = (from.0 as i32 / 2 * step + from.1) as f32;
    let end = (to.0 as i32 / 2 * step + to.1) as f32;
    ui.set_auto_advance(false);
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
        let actual = state.first_visible_item_index() as i32 / 2 * step
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameForwardToVisibleItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameForwardToVisibleItemWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameForwardToNotVisibleItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameForwardToNotVisibleItemWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameBackward
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameBackwardWithOffset
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameBackwardWithInitialOffset
#[test]
fn an_animated_scroll_follows_a_spring() {
    assert_spring_animation(0, (4, 0), (0, 0));
    assert_spring_animation(0, (4, 35), (0, 0));
    assert_spring_animation(0, (16, 0), (0, 0));
    assert_spring_animation(0, (20, 35), (0, 0));
    assert_spring_animation(0, (2, 0), (12, 0));
    assert_spring_animation(0, (2, 0), (10, 58));
    assert_spring_animation(0, (0, 40), (8, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameForwardWithSpacing
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameForwardWithNegativeSpacing
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameBackwardWithSpacing
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyScrollTest.kt, animatePerFrameBackwardWithNegativeSpacing
#[test]
fn an_animated_scroll_with_spacing_follows_a_spring() {
    for spacing in [10, -10] {
        assert_spring_animation(spacing, (16, 0), (0, 0));
        assert_spring_animation(spacing, (2, 0), (12, 0));
    }
}
