//! Compose's `LazyListScrollIndicatorTest`, `LazyGridScrollIndicatorTest` and
//! `LazyStaggeredGridScrollIndicatorTest`, each both ways, at density 1. `scrollMainAxisBy` is a
//! raw delta here, as in `scroll_indicator.rs`.

use compose_foundation::gestures::{Orientation, ScrollableState};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{Dp, IntSize, Modifier, PaddingValues, UnitExt};

use crate::scroll_support::run;

const BOTH_WAYS: [bool; 2] = [true, false];

fn read(state: &impl ScrollableState) -> (i32, i32, i32) {
    let indicator = state.scroll_indicator_state().expect("it has one");
    (indicator.scroll_offset(), indicator.content_size(), indicator.viewport_size())
}

fn along(vertical: bool, length: f32) -> Modifier {
    if vertical { Modifier.height(length.dp()) } else { Modifier.width(length.dp()) }
}

/// An item `length` along the axis and 30 across.
fn item(vertical: bool, length: f32) -> Modifier {
    if vertical {
        Modifier.height(length.dp()).width(30.dp())
    } else {
        Modifier.width(length.dp()).height(30.dp())
    }
}

/// What a lazy layout under test is given, along its axis.
#[derive(Clone, Copy)]
struct Lazy {
    vertical: bool,
    viewport: f32,
    spacing: f32,
    before: f32,
    after: f32,
    reverse: bool,
    initial: (usize, i32),
}

impl Lazy {
    fn new(vertical: bool, viewport: f32) -> Lazy {
        Lazy {
            vertical,
            viewport,
            spacing: 0.0,
            before: 0.0,
            after: 0.0,
            reverse: false,
            initial: (0, 0),
        }
    }

    fn padding(self) -> PaddingValues {
        let (before, after) = (self.before.dp(), self.after.dp());
        if self.vertical {
            PaddingValues { top: before, bottom: after, ..PaddingValues::ZERO }
        } else {
            PaddingValues { start: before, end: after, ..PaddingValues::ZERO }
        }
    }

    fn list(
        self,
        content: impl Fn(LazyListScope) + Clone + 'static,
    ) -> (ComposeTestRule, LazyListState) {
        let owned = LazyListState::new(self.initial.0, self.initial.1);
        let state = *owned;
        std::mem::forget(owned);
        let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
            let content = content.clone();
            if self.vertical {
                LazyColumn!(
                    modifier = along(true, self.viewport),
                    state = state,
                    content_padding = self.padding(),
                    reverse_layout = self.reverse,
                    vertical_arrangement = Arrangement::spaced_by(self.spacing.dp()),
                    move |scope: LazyListScope| content(scope)
                );
            } else {
                LazyRow!(
                    modifier = along(false, self.viewport),
                    state = state,
                    content_padding = self.padding(),
                    reverse_layout = self.reverse,
                    horizontal_arrangement = Arrangement::spaced_by(self.spacing.dp()),
                    move |scope: LazyListScope| content(scope)
                );
            }
        });
        ui.wait_for_idle();
        (ui, state)
    }

    fn grid(
        self,
        content: impl Fn(LazyGridScope) + Clone + 'static,
    ) -> (ComposeTestRule, LazyGridState) {
        let owned = LazyGridState::new(self.initial.0, self.initial.1);
        let state = *owned;
        std::mem::forget(owned);
        let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
            let content = content.clone();
            if self.vertical {
                LazyVerticalGrid!(
                    GridCells::Fixed(2),
                    modifier = along(true, self.viewport).width(60.dp()),
                    state = state,
                    content_padding = self.padding(),
                    reverse_layout = self.reverse,
                    vertical_arrangement = Arrangement::spaced_by(self.spacing.dp()),
                    move |scope: LazyGridScope| content(scope)
                );
            } else {
                LazyHorizontalGrid!(
                    GridCells::Fixed(2),
                    modifier = along(false, self.viewport).height(60.dp()),
                    state = state,
                    content_padding = self.padding(),
                    reverse_layout = self.reverse,
                    horizontal_arrangement = Arrangement::spaced_by(self.spacing.dp()),
                    move |scope: LazyGridScope| content(scope)
                );
            }
        });
        ui.wait_for_idle();
        (ui, state)
    }

    fn staggered(
        self,
        lanes: usize,
        content: impl Fn(LazyStaggeredGridScope) + Clone + 'static,
    ) -> (ComposeTestRule, LazyStaggeredGridState) {
        let owned = LazyStaggeredGridState::new(self.initial.0, self.initial.1);
        let state = *owned;
        std::mem::forget(owned);
        let mut ui = ComposeTestRule::sized(IntSize::new(800, 800), move || {
            let content = content.clone();
            let across = (lanes * 30) as f32;
            if self.vertical {
                LazyVerticalStaggeredGrid!(
                    StaggeredGridCells::Fixed(lanes),
                    modifier = along(true, self.viewport).width(across.dp()),
                    state = state,
                    content_padding = self.padding(),
                    reverse_layout = self.reverse,
                    vertical_item_spacing = Dp(self.spacing),
                    move |scope: LazyStaggeredGridScope| content(scope)
                );
            } else {
                LazyHorizontalStaggeredGrid!(
                    StaggeredGridCells::Fixed(lanes),
                    modifier = along(false, self.viewport).height(across.dp()),
                    state = state,
                    content_padding = self.padding(),
                    reverse_layout = self.reverse,
                    horizontal_item_spacing = Dp(self.spacing),
                    move |scope: LazyStaggeredGridScope| content(scope)
                );
            }
        });
        ui.wait_for_idle();
        (ui, state)
    }
}

// ---- A list ------------------------------------------------------------------------------------

/// `count` items of 50.
fn even_list(lazy: Lazy, count: usize) -> (ComposeTestRule, LazyListState) {
    let vertical = lazy.vertical;
    lazy.list(move |scope: LazyListScope| {
        scope.items(count, move |_| Spacer(item(vertical, 50.0)));
    })
}

const VARYING: [f32; 5] = [50.0, 100.0, 80.0, 70.0, 120.0];

fn varying_list(lazy: Lazy) -> (ComposeTestRule, LazyListState) {
    let vertical = lazy.vertical;
    lazy.list(move |scope: LazyListScope| {
        scope.items(10, move |index| Spacer(item(vertical, VARYING[index % 5])));
    })
}

/// The test's own `visibleItemsAverageSize`.
fn list_average(state: &LazyListState) -> i32 {
    let info = state.layout_info();
    let items = &info.visible_items_info;
    items.iter().map(|it| it.size).sum::<i32>() / items.len() as i32 + info.main_axis_item_spacing
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_emptyList
#[test]
fn an_empty_list_has_no_content() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_list(Lazy::new(vertical, 100.0), 0);
        assert_eq!(read(&state), (0, 0, 100));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_whenContentFits
#[test]
fn a_list_that_fits_is_as_long_as_its_items() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_list(Lazy::new(vertical, 200.0), 3);
        assert_eq!(read(&state), (0, 150, 200));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_uniformSizeItems
#[test]
fn a_list_of_even_items_counts_those_out_of_view() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_list(Lazy::new(vertical, 250.0), 10);
        assert_eq!(read(&state), (0, 500, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_uniformSizeItems_withItemSpacing
#[test]
fn a_list_of_even_items_counts_the_gaps_between_them() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_list(Lazy { spacing: 20.0, ..Lazy::new(vertical, 250.0) }, 10);
        assert_eq!(read(&state), (0, 500 + 9 * 20, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_variableSizeItems
#[test]
fn a_list_of_varying_items_either_way_goes_by_their_average() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = varying_list(Lazy::new(vertical, 300.0));
        assert_eq!(read(&state), (0, list_average(&state) * 10, 300));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_variableSizeItems_withItemSpacing
#[test]
fn a_list_of_varying_items_has_one_gap_less_than_items() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = varying_list(Lazy { spacing: 20.0, ..Lazy::new(vertical, 300.0) });
        assert_eq!(read(&state), (0, list_average(&state) * 10 - 20, 300));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_withContentPadding
#[test]
fn a_lists_padding_is_in_its_content() {
    for vertical in BOTH_WAYS {
        let lazy = Lazy { before: 20.0, after: 30.0, ..Lazy::new(vertical, 250.0) };
        let (_ui, state) = even_list(lazy, 10);
        assert_eq!(read(&state), (0, 500 + 50, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_withReverseLayout
#[test]
fn a_reversed_list_starts_at_the_far_end() {
    for vertical in BOTH_WAYS {
        let lazy = Lazy { before: 20.0, after: 30.0, reverse: true, ..Lazy::new(vertical, 250.0) };
        let (_ui, state) = even_list(lazy, 20);
        assert_eq!(read(&state), (1050 - 250, 1050, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_withInitialScroll
#[test]
fn a_list_scrolled_already_says_how_far() {
    for vertical in BOTH_WAYS {
        let lazy =
            Lazy { before: 20.0, after: 30.0, initial: (2, 25), ..Lazy::new(vertical, 250.0) };
        let (_ui, state) = even_list(lazy, 20);
        assert_eq!(read(&state), (2 * 50 + 25, 1050, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_reverseLayout_withInitialScroll
#[test]
fn a_reversed_list_scrolled_already_counts_from_the_far_end_either_way() {
    for vertical in BOTH_WAYS {
        let lazy = Lazy {
            before: 20.0,
            after: 30.0,
            reverse: true,
            initial: (2, 25),
            ..Lazy::new(vertical, 250.0)
        };
        let (_ui, state) = even_list(lazy, 20);
        assert_eq!(read(&state), (1050 - 250 - 125, 1050, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_onScrollByOffset
#[test]
fn a_list_follows_a_scroll_by_an_offset() {
    for vertical in BOTH_WAYS {
        let (mut ui, state) = even_list(Lazy::new(vertical, 250.0), 20);
        state.dispatch_raw_delta(80.0);
        ui.wait_for_idle();
        assert_eq!(read(&state), (80, 1000, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_onScrollToIndexAndOffset
#[test]
fn a_list_follows_a_jump_and_a_scroll_after_it() {
    for vertical in BOTH_WAYS {
        let (mut ui, state) = even_list(Lazy::new(vertical, 250.0), 20);
        run(&mut ui, async move {
            let _ = state.scroll_to_item(2, 0).await;
        });
        state.dispatch_raw_delta(10.0);
        ui.wait_for_idle();
        assert_eq!(read(&state), (2 * 50 + 10, 1000, 250));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_withContentPadding_onScroll
#[test]
fn a_list_with_padding_follows_a_scroll() {
    for vertical in BOTH_WAYS {
        let lazy = Lazy { before: 70.0, after: 30.0, ..Lazy::new(vertical, 250.0) };
        let (mut ui, state) = even_list(lazy, 20);
        state.dispatch_raw_delta(40.0);
        ui.wait_for_idle();
        let first = state.first_visible_item_index() as i32;
        let offset = state.first_visible_item_scroll_offset();
        assert_eq!(read(&state), (50 * first + offset, 1000 + 100, 250));
        assert_eq!(50 * first + offset, 40);
    }
}

// ---- A grid of two lines across ----------------------------------------------------------------

/// `count` items of 50.
fn even_grid(lazy: Lazy, count: usize) -> (ComposeTestRule, LazyGridState) {
    let vertical = lazy.vertical;
    lazy.grid(move |scope: LazyGridScope| {
        scope.items(count, move |_| Spacer(item(vertical, 50.0)));
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_emptyGrid
#[test]
fn an_empty_grid_has_no_content() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_grid(Lazy::new(vertical, 100.0), 0);
        assert_eq!(read(&state), (0, 0, 100));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_whenContentFits
#[test]
fn a_grid_that_fits_is_as_long_as_its_lines() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_grid(Lazy::new(vertical, 150.0), 4);
        assert_eq!(read(&state), (0, 100, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_uniformSizeItems
#[test]
fn a_grid_of_even_items_counts_the_lines_out_of_view() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_grid(Lazy::new(vertical, 150.0), 10);
        assert_eq!(read(&state), (0, 250, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_variableItemSizes
#[test]
fn a_grid_of_varying_items_goes_by_the_longest_of_each_line() {
    const SIZES: [f32; 5] = [50.0, 80.0, 60.0, 90.0, 70.0];
    for vertical in BOTH_WAYS {
        let (_ui, state) = Lazy::new(vertical, 200.0).grid(move |scope: LazyGridScope| {
            scope.items(10, move |index| Spacer(item(vertical, SIZES[index % 5])));
        });
        // Lines of 80, 90 and 70 in view, of the 5 there are.
        assert_eq!(read(&state), (0, 80 * 5, 200));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withMainAxisSpacing
#[test]
fn a_grid_counts_the_gaps_between_its_lines() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_grid(Lazy { spacing: 10.0, ..Lazy::new(vertical, 150.0) }, 10);
        assert_eq!(read(&state), (0, 250 + 4 * 10, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withContentPadding
#[test]
fn a_grids_padding_is_in_its_content() {
    for vertical in BOTH_WAYS {
        let lazy = Lazy { before: 20.0, after: 30.0, ..Lazy::new(vertical, 150.0) };
        let (_ui, state) = even_grid(lazy, 10);
        assert_eq!(read(&state), (0, 250 + 50, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withReverseLayout
#[test]
fn a_reversed_grid_starts_at_the_far_end_either_way() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_grid(Lazy { reverse: true, ..Lazy::new(vertical, 150.0) }, 10);
        assert_eq!(read(&state), (100, 250, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_onScrollByOffset
#[test]
fn a_grid_follows_a_scroll_by_an_offset() {
    for vertical in BOTH_WAYS {
        let (mut ui, state) = even_grid(Lazy::new(vertical, 150.0), 20);
        state.dispatch_raw_delta(75.0);
        ui.wait_for_idle();
        assert_eq!(read(&state), (75, 500, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_onScrollToItem
#[test]
fn a_grid_follows_a_jump_to_an_item_by_its_line() {
    for vertical in BOTH_WAYS {
        let (mut ui, state) = even_grid(Lazy::new(vertical, 150.0), 20);
        run(&mut ui, async move {
            let _ = state.scroll_to_item(7, 0).await;
        });
        assert_eq!(read(&state), (3 * 50, 500, 150));
    }
}

/// The sticky ones across too, `scroll_indicator.rs` having them down.
fn sticky_sections(vertical: bool, sections: &'static [usize]) -> (ComposeTestRule, LazyGridState) {
    Lazy::new(vertical, 150.0).grid(move |scope: LazyGridScope| {
        for &count in sections {
            scope.sticky_header(move |_| Spacer(item(vertical, 50.0)));
            scope.items(count, move |_| Spacer(item(vertical, 50.0)));
        }
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withStickyHeader
#[test]
fn a_grid_scrolling_across_under_a_sticky_header() {
    let (mut ui, state) = sticky_sections(false, &[20]);
    assert_eq!(read(&state), (0, 11 * 50, 150));
    for (delta, total) in [(20.0, 20), (60.0, 80), (20.0, 100)] {
        state.dispatch_raw_delta(delta);
        ui.wait_for_idle();
        assert_eq!(read(&state).0, total);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withMultipleStickyHeaders
#[test]
fn a_grid_scrolling_across_to_its_second_sticky_header() {
    let (mut ui, state) = sticky_sections(false, &[4, 4]);
    state.dispatch_raw_delta(150.0);
    ui.wait_for_idle();
    assert_eq!(read(&state), (150, 5 * 50, 150));
}

// ---- A staggered grid --------------------------------------------------------------------------

const STAGGERED: [f32; 4] = [20.0, 40.0, 30.0, 50.0];

/// `count` items of the varying lengths, in 3 lanes.
fn staggered(lazy: Lazy, count: usize) -> (ComposeTestRule, LazyStaggeredGridState) {
    let vertical = lazy.vertical;
    lazy.staggered(3, move |scope: LazyStaggeredGridScope| {
        scope.items(count, move |index| Spacer(item(vertical, STAGGERED[index % 4])));
    })
}

/// The test's own `visibleItemsAverageSize`.
fn staggered_average(state: &LazyStaggeredGridState) -> i32 {
    let info = state.layout_info();
    let items = &info.visible_items_info;
    let length = |size: IntSize| {
        if info.orientation == Orientation::Vertical { size.height } else { size.width }
    };
    items.iter().map(|it| length(it.size)).sum::<i32>() / items.len() as i32
        + info.main_axis_item_spacing
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_emptyGrid
#[test]
fn an_empty_staggered_grid_has_no_content() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = Lazy::new(vertical, 100.0).staggered(2, |_: LazyStaggeredGridScope| {});
        assert_eq!(read(&state), (0, 0, 100));
    }
}

/// `count` items of 50 in 2 lanes.
fn even_staggered(lazy: Lazy, count: usize) -> (ComposeTestRule, LazyStaggeredGridState) {
    let vertical = lazy.vertical;
    lazy.staggered(2, move |scope: LazyStaggeredGridScope| {
        scope.items(count, move |_| Spacer(item(vertical, 50.0)));
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_whenContentFits
#[test]
fn a_staggered_grid_that_fits_is_as_long_as_its_lanes() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_staggered(Lazy::new(vertical, 150.0), 4);
        assert_eq!(read(&state), (0, 100, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_uniformSizeItems
#[test]
fn a_staggered_grid_of_even_items_counts_those_out_of_view() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = even_staggered(Lazy::new(vertical, 150.0), 10);
        assert_eq!(read(&state), (0, 250, 150));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_staggeredContent
#[test]
fn a_staggered_grid_goes_by_its_items_average_over_its_lanes() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = staggered(Lazy::new(vertical, 100.0), 10);
        assert_eq!(read(&state), (0, staggered_average(&state) * 10 / 3, 100));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_staggeredContent_withMainAxisSpacing
#[test]
fn a_staggered_grids_spacing_is_in_its_average_less_one_gap() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = staggered(Lazy { spacing: 10.0, ..Lazy::new(vertical, 100.0) }, 10);
        assert_eq!(read(&state), (0, staggered_average(&state) * 10 / 3 - 10, 100));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_staggeredContent_withContentPadding
#[test]
fn a_staggered_grids_padding_is_in_its_content() {
    for vertical in BOTH_WAYS {
        let lazy = Lazy { before: 20.0, after: 30.0, ..Lazy::new(vertical, 100.0) };
        let (_ui, state) = staggered(lazy, 10);
        assert_eq!(read(&state), (0, staggered_average(&state) * 10 / 3 + 50, 100));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_staggeredContent_withReverseLayout
#[test]
fn a_reversed_staggered_grid_starts_at_the_far_end() {
    for vertical in BOTH_WAYS {
        let (_ui, state) = staggered(Lazy { reverse: true, ..Lazy::new(vertical, 100.0) }, 10);
        let content = staggered_average(&state) * 10 / 3;
        assert_eq!(read(&state), (content - 100, content, 100));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_staggeredContent_onScrollByOffset
#[test]
fn a_staggered_grid_follows_a_scroll_by_an_offset() {
    for vertical in BOTH_WAYS {
        let (mut ui, state) = staggered(Lazy::new(vertical, 100.0), 12);
        state.dispatch_raw_delta(50.0);
        ui.wait_for_idle();
        let average = staggered_average(&state);
        let offset = average * state.first_visible_item_index() as i32 / 3
            + state.first_visible_item_scroll_offset();
        assert_eq!(read(&state), (offset, average * 12 / 3, 100));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollIndicatorTest.kt, scrollIndicatorState_staggeredContent_onScrollToItem
#[test]
fn a_staggered_grid_follows_a_jump_to_an_item() {
    for vertical in BOTH_WAYS {
        let (mut ui, state) = staggered(Lazy::new(vertical, 100.0), 15);
        run(&mut ui, async move {
            let _ = state.scroll_to_item(7, 0).await;
        });
        let average = staggered_average(&state);
        let offset = average * state.first_visible_item_index() as i32 / 3
            + state.first_visible_item_scroll_offset();
        assert!(offset > 0);
        assert_eq!(read(&state), (offset, average * 15 / 3, 100));
    }
}

/// One object, as the other states'.
#[test]
fn a_staggered_grids_state_gives_one_indicator() {
    let owned = LazyStaggeredGridState::new(0, 0);
    let (first, second) =
        (owned.scroll_indicator_state().unwrap(), owned.scroll_indicator_state().unwrap());
    assert!(std::rc::Rc::ptr_eq(&first, &second));
}
