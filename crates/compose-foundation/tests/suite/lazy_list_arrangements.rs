//! Lazy lists' main-axis arrangements, as Compose's lists' `LazyArrangementsTest`: short lists
//! arranged as `Arrangement.arrange` says (left to right and right to left), spacing in wrapped
//! content and at the ends, scrolls by the item and its spacing, reversed, changed, negative
//! spacing. Density 1, so Compose's dps are pixels (items of 50 and 40 px in 250).

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Density, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, UnitExt};

use crate::lazy_support::{Lazy, bounds};
use crate::scroll_support::run;

const CONTAINER: &str = "ContainerTag";
const ITEM_SIZE: f32 = 50.0;
const SMALLER_ITEM_SIZE: f32 = 40.0;
const CONTAINER_SIZE: f32 = ITEM_SIZE * 5.0;

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

/// Compose's tests' `Item`: the first 50 px, the second 40.
fn two_items(scope: &LazyListScope) {
    scope.items(2, |index| {
        let size = if index == 0 { ITEM_SIZE } else { SMALLER_ITEM_SIZE };
        Box!(modifier = Modifier.required_size(size.dp()).test_tag(index.to_string()), {});
    });
}

/// Where `arrangement` puts the two items (reversed: the second first), as Compose's
/// `assertArrangementForTwoItems`, against where they are.
fn assert_column_arranged(
    ui: &mut ComposeTestRule,
    arrangement: VerticalArrangement,
    reverse: bool,
) {
    let order: [usize; 2] = if reverse { [1, 0] } else { [0, 1] };
    let sizes =
        order.map(|index| if index == 0 { ITEM_SIZE as i32 } else { SMALLER_ITEM_SIZE as i32 });
    let mut positions = [0; 2];
    arrangement.arrange(&Density::new(1.0), CONTAINER_SIZE as i32, &sizes, &mut positions);
    for (slot, index) in order.into_iter().enumerate() {
        assert_eq!(bounds(ui, &index.to_string()).top, positions[slot] as f32, "item {index}");
    }
}

fn assert_row_arranged(
    ui: &mut ComposeTestRule,
    arrangement: HorizontalArrangement,
    direction: LayoutDirection,
    reverse: bool,
) {
    let order: [usize; 2] = if reverse { [1, 0] } else { [0, 1] };
    let sizes =
        order.map(|index| if index == 0 { ITEM_SIZE as i32 } else { SMALLER_ITEM_SIZE as i32 });
    let mut positions = [0; 2];
    arrangement.arrange(
        &Density::new(1.0),
        CONTAINER_SIZE as i32,
        &sizes,
        direction,
        &mut positions,
    );
    for (slot, index) in order.into_iter().enumerate() {
        assert_eq!(
            bounds(ui, &index.to_string()).left,
            positions[slot] as f32,
            "item {index}, {direction:?}"
        );
    }
}

fn column_with(arrangement: VerticalArrangement, reverse: bool) -> ComposeTestRule {
    rule(move || {
        LazyColumn!(
            modifier = Modifier.required_size(CONTAINER_SIZE.dp()),
            vertical_arrangement = arrangement.clone(),
            reverse_layout = reverse,
            |scope: LazyListScope| two_items(&scope)
        );
    })
}

fn row_with(
    arrangement: HorizontalArrangement,
    direction: LayoutDirection,
    reverse: bool,
) -> ComposeTestRule {
    rule(move || {
        let arrangement = arrangement.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
            LazyRow!(
                modifier = Modifier.required_size(CONTAINER_SIZE.dp()),
                horizontal_arrangement = arrangement.clone(),
                reverse_layout = reverse,
                |scope: LazyListScope| two_items(&scope)
            );
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_defaultArrangementIsTop
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_centerArrangement
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_bottomArrangement
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_spacedArrangementNotFillingViewport
#[test]
fn a_short_column_is_arranged() {
    let mut ui = rule(|| {
        LazyColumn!(
            modifier = Modifier.required_size(CONTAINER_SIZE.dp()),
            |scope: LazyListScope| { two_items(&scope) }
        );
    });
    assert_column_arranged(&mut ui, Arrangement::TOP, false);
    let arrangements: [VerticalArrangement; 3] =
        [Arrangement::CENTER.into(), Arrangement::BOTTOM, Arrangement::spaced_by(10.dp()).into()];
    for arrangement in arrangements {
        let mut ui = column_with(arrangement.clone(), false);
        assert_column_arranged(&mut ui, arrangement, false);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_defaultArrangementIsStart
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_centerArrangement
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_endArrangement
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_spacedArrangementNotFillingViewport
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_rtl_startArrangement
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_rtl_endArrangement
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_rtl_spacedArrangementNotFillingViewport
#[test]
fn a_short_row_is_arranged() {
    let mut ui = rule(|| {
        LazyRow!(modifier = Modifier.required_size(CONTAINER_SIZE.dp()), |scope: LazyListScope| {
            two_items(&scope)
        });
    });
    assert_row_arranged(&mut ui, Arrangement::START, LayoutDirection::Ltr, false);
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let arrangements: [HorizontalArrangement; 3] =
            [Arrangement::CENTER.into(), Arrangement::END, Arrangement::spaced_by(10.dp()).into()];
        for arrangement in arrangements {
            let mut ui = row_with(arrangement.clone(), direction, false);
            assert_row_arranged(&mut ui, arrangement, direction, false);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_spacing_affects_wrap_content
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_spacing_affects_wrap_content
#[test]
fn the_spacing_counts_in_wrapped_content() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let mut ui = rule(move || {
            let content = |scope: LazyListScope| {
                scope.items(2, |_| Box!(modifier = Modifier.required_size(ITEM_SIZE.dp()), {}))
            };
            if lazy.vertical {
                LazyColumn!(
                    modifier = Modifier.test_tag(CONTAINER),
                    vertical_arrangement = Arrangement::spaced_by(ITEM_SIZE.dp()),
                    content = content
                );
            } else {
                LazyRow!(
                    modifier = Modifier.test_tag(CONTAINER),
                    horizontal_arrangement = Arrangement::spaced_by(ITEM_SIZE.dp()),
                    content = content
                );
            }
        });
        assert_eq!(lazy.cross_size(&mut ui, CONTAINER), ITEM_SIZE, "{lazy:?}");
        assert_eq!(lazy.main_size(&mut ui, CONTAINER), ITEM_SIZE * 3.0, "{lazy:?}");
    }
}

/// `count` items of `size` spaced by `spacing` in a list of `list`, with `state`.
fn spaced(
    lazy: Lazy,
    list: f32,
    spacing: f32,
    count: usize,
    size: f32,
    state: LazyListState,
) -> ComposeTestRule {
    rule(move || {
        let content = move |scope: LazyListScope| {
            scope.items(count, move |index| {
                Spacer(Modifier.required_size(size.dp()).test_tag(index.to_string()))
            });
        };
        let modifier = Modifier.required_size(list.dp()).test_tag(CONTAINER);
        if lazy.vertical {
            LazyColumn!(
                modifier = modifier,
                state = state,
                vertical_arrangement = Arrangement::spaced_by(spacing.dp()),
                content = content
            );
        } else {
            LazyRow!(
                modifier = modifier,
                state = state,
                horizontal_arrangement = Arrangement::spaced_by(spacing.dp()),
                content = content
            );
        }
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_spacing_scrolledToTheTop
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_spacing_scrolledToTheBottom
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_spacing_scrolledToTheStart
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_spacing_scrolledToTheEnd
#[test]
fn the_spacing_shows_at_both_ends() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let mut ui = spaced(lazy, ITEM_SIZE * 3.5, ITEM_SIZE, 3, ITEM_SIZE, *state);
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "1"), ITEM_SIZE * 2.0, "{lazy:?}");
        lazy.drag_main_axis_by(&mut ui, CONTAINER, ITEM_SIZE * 2.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), ITEM_SIZE * 0.5, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "2"), ITEM_SIZE * 2.5, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_scrollingByExactlyTheItemSizePlusSpacer_switchesTheFirstVisibleItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_scrollingByExactlyTheItemSizePlusHalfTheSpacer_staysOnTheSameItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_scrollingByExactlyTheItemSizePlusSpacer_switchesTheFirstVisibleItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_scrollingByExactlyTheItemSizePlusHalfTheSpacer_staysOnTheSameItem
#[test]
fn the_first_item_changes_once_past_its_spacing() {
    for lazy in [Lazy::column(), Lazy::row()] {
        for (distance, expected) in [(34.0, (1, 0)), (32.0, (0, 32))] {
            let state = LazyListState::new(0, 0);
            let handle = *state;
            let mut ui = spaced(lazy, 90.0, 4.0, 5, 30.0, handle);
            let _ = run(&mut ui, async move { handle.scroll_by(distance).await });
            assert!(!ui.on_node_with_tag("0").is_displayed(), "{lazy:?}");
            let position =
                (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
            assert_eq!(position, expected, "{lazy:?} by {distance}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_defaultArrangementIsBottomWithReverseLayout
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_defaultArrangementIsEndWithReverseLayout
#[test]
fn a_reversed_short_list_is_arranged_from_the_end() {
    // By default (no arrangement given: from the end when reversed, as Compose's, D875), and
    // given explicitly.
    let mut ui = rule(|| {
        LazyColumn!(
            modifier = Modifier.required_size(CONTAINER_SIZE.dp()),
            reverse_layout = true,
            |scope: LazyListScope| two_items(&scope)
        );
    });
    assert_column_arranged(&mut ui, Arrangement::BOTTOM, true);
    let mut ui = rule(|| {
        LazyRow!(
            modifier = Modifier.required_size(CONTAINER_SIZE.dp()),
            reverse_layout = true,
            |scope: LazyListScope| two_items(&scope)
        );
    });
    assert_row_arranged(&mut ui, Arrangement::END, LayoutDirection::Ltr, true);
    let mut ui = column_with(Arrangement::BOTTOM, true);
    assert_column_arranged(&mut ui, Arrangement::BOTTOM, true);
    let mut ui = row_with(Arrangement::END, LayoutDirection::Ltr, true);
    assert_row_arranged(&mut ui, Arrangement::END, LayoutDirection::Ltr, true);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_whenArrangementChanges
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_whenArrangementChanges
#[test]
fn a_new_arrangement_applies() {
    let at_end = mutable_state_of(false);
    let handle = at_end.handle();
    let mut ui = rule(move || {
        let arrangement = if handle.get() { Arrangement::BOTTOM } else { Arrangement::TOP };
        LazyColumn!(
            modifier = Modifier.required_size(CONTAINER_SIZE.dp()),
            vertical_arrangement = arrangement,
            |scope: LazyListScope| two_items(&scope)
        );
    });
    assert_column_arranged(&mut ui, Arrangement::TOP, false);
    at_end.set(true);
    ui.wait_for_idle();
    assert_column_arranged(&mut ui, Arrangement::BOTTOM, false);

    let at_end = mutable_state_of(false);
    let handle = at_end.handle();
    let mut ui = rule(move || {
        let arrangement = if handle.get() { Arrangement::END } else { Arrangement::START };
        LazyRow!(
            modifier = Modifier.required_size(CONTAINER_SIZE.dp()),
            horizontal_arrangement = arrangement,
            |scope: LazyListScope| two_items(&scope)
        );
    });
    assert_row_arranged(&mut ui, Arrangement::START, LayoutDirection::Ltr, false);
    at_end.set(true);
    ui.wait_for_idle();
    assert_row_arranged(&mut ui, Arrangement::END, LayoutDirection::Ltr, false);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_negativeSpacing_itemsVisible
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_negativeSpacing_itemsVisible
#[test]
fn items_overlap_by_a_negative_spacing() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let half = ITEM_SIZE / 2.0;
        let mut ui = spaced(lazy, ITEM_SIZE, -half, 100, ITEM_SIZE, handle);
        assert_eq!(
            (lazy.main_start(&mut ui, "0"), lazy.main_start(&mut ui, "1")),
            (0.0, half),
            "{lazy:?}"
        );
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            (0, 0)
        );
        let _ = run(&mut ui, async move { handle.scroll_by(half).await });
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            (1, 0)
        );
        assert_eq!(
            (lazy.main_start(&mut ui, "0"), lazy.main_start(&mut ui, "1")),
            (-half, 0.0),
            "{lazy:?}"
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, column_negativeSpacingLargerThanItem_itemsVisible
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyArrangementsTest.kt, row_negativeSpacingLargerThanItem_itemsVisible
#[test]
fn a_negative_spacing_larger_than_the_items_stacks_them() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(2, 0);
        let mut ui = spaced(lazy, ITEM_SIZE, -ITEM_SIZE * 1.5, 4, ITEM_SIZE, *state);
        for index in 0..4 {
            assert_eq!(lazy.main_start(&mut ui, &index.to_string()), 0.0, "{lazy:?}: {index}");
        }
        let position = (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
        assert_eq!(position, (0, 0), "{lazy:?}");
    }
}
