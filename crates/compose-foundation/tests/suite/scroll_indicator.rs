//! Compose's `scrollIndicatorState` tests of `ScrollTest.kt` and `PagerScrollIndicatorTest.kt`, and
//! the lazy states' estimates (`LazyListLayoutInfo.calculateContentSize` and the grid's).

use compose_foundation::gestures::{Orientation, ScrollableState};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::CompositionLocalProvider;
use compose_test::*;
use compose_ui::{Dp, IntSize, UnitExt};

use crate::scroll_support::run;

fn read(state: &impl ScrollableState) -> (i32, i32, i32) {
    let indicator = state.scroll_indicator_state().expect("it has one");
    (indicator.scroll_offset(), indicator.content_size(), indicator.viewport_size())
}

// ---- ScrollState ------------------------------------------------------------------------------

const CONTENT: i32 = 200;

/// A scroller `size` long around content of `CONTENT`, either way.
fn scroller(size: i32, vertical: bool, reverse: bool) -> (ComposeTestRule, ScrollState) {
    let owned = ScrollState::new(0);
    let state = *owned;
    std::mem::forget(owned);
    let ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        let along = (size as f32).dp();
        let modifier = if vertical {
            Modifier.width(50.dp()).height(along).vertical_scroll_with(state, true, None, reverse)
        } else {
            Modifier.height(50.dp()).width(along).horizontal_scroll_with(state, true, None, reverse)
        };
        Box!(
            modifier = modifier,
            content = move |_| {
                let long = (CONTENT as f32).dp();
                Spacer(if vertical {
                    Modifier.width(50.dp()).height(long)
                } else {
                    Modifier.height(50.dp()).width(long)
                });
            }
        );
    });
    (ui, state)
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scrollIndicatorState_whenContentFits
#[test]
fn a_scroll_whose_content_fits_is_as_long_as_its_viewport() {
    for vertical in [true, false] {
        let (mut ui, state) = scroller(CONTENT + 10, vertical, false);
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, CONTENT + 10, CONTENT + 10));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scrollIndicatorState_whenContentFits_doesNotChangeOnScroll
#[test]
fn a_scroll_whose_content_fits_does_not_scroll() {
    for vertical in [true, false] {
        let (mut ui, state) = scroller(CONTENT + 10, vertical, false);
        ui.wait_for_idle();
        run(&mut ui, async move {
            let _ = state.scroll_to(5).await;
        });
        assert_eq!(read(&state), (0, CONTENT + 10, CONTENT + 10));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scrollIndicatorState_whenContentDoesNotFit_initialAndScrolled
#[test]
fn a_scroll_says_where_it_is() {
    for vertical in [true, false] {
        let (mut ui, state) = scroller(CONTENT - 10, vertical, false);
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, CONTENT, CONTENT - 10));
        run(&mut ui, async move {
            let _ = state.scroll_to(5).await;
        });
        assert_eq!(read(&state), (5, CONTENT, CONTENT - 10));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scrollIndicatorState_reverseLayout
#[test]
fn a_reversed_scroll_says_it_from_the_visual_start() {
    for vertical in [true, false] {
        let (mut ui, state) = scroller(CONTENT - 10, vertical, true);
        ui.wait_for_idle();
        assert_eq!(read(&state), (10, CONTENT, CONTENT - 10));
    }
}

// ---- PagerState -------------------------------------------------------------------------------

struct Pager {
    vertical: bool,
    count: usize,
    page: usize,
    size: f32,
    page_size: PageSize,
    spacing: f32,
    padding: (f32, f32),
    reverse: bool,
    rtl: bool,
    beyond_viewport: usize,
}

/// Compose's test's configurations: along, and a row in each direction.
const PAGER_CONFIGS: [(bool, bool); 3] = [(true, false), (false, false), (false, true)];

impl Pager {
    fn new(vertical: bool, count: usize) -> Pager {
        Pager {
            vertical,
            count,
            page: 0,
            size: 100.0,
            page_size: PageSize::Fixed(50.dp()),
            spacing: 0.0,
            padding: (0.0, 0.0),
            reverse: false,
            rtl: false,
            beyond_viewport: 0,
        }
    }

    fn rule(self) -> (ComposeTestRule, PagerState) {
        let count = self.count;
        let owned = PagerState::new(self.page, 0.0, move || count);
        let state = *owned;
        std::mem::forget(owned);
        let ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
            let direction = if self.rtl {
                compose_ui::LayoutDirection::Rtl
            } else {
                compose_ui::LayoutDirection::Ltr
            };
            let page_size = self.page_size.clone();
            CompositionLocalProvider(
                compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
                move || {
                    let page = |_: PagerScope, _: usize| Spacer(Modifier.fill_max_size());
                    let modifier = Modifier.size(self.size.dp());
                    let (before, after) = (Dp(self.padding.0), Dp(self.padding.1));
                    if self.vertical {
                        VerticalPager!(
                            state,
                            modifier = modifier,
                            content_padding =
                                PaddingValues { top: before, bottom: after, ..PaddingValues::ZERO },
                            page_size = page_size.clone(),
                            page_spacing = Dp(self.spacing),
                            reverse_layout = self.reverse,
                            beyond_viewport_page_count = self.beyond_viewport,
                            page_content = page
                        );
                    } else {
                        HorizontalPager!(
                            state,
                            modifier = modifier,
                            content_padding =
                                PaddingValues { start: before, end: after, ..PaddingValues::ZERO },
                            page_size = page_size.clone(),
                            page_spacing = Dp(self.spacing),
                            reverse_layout = self.reverse,
                            beyond_viewport_page_count = self.beyond_viewport,
                            page_content = page
                        );
                    }
                },
            );
        });
        (ui, state)
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_emptyPager
#[test]
fn an_empty_pager_has_no_content() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager::new(vertical, 0).rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 0, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_whenContentFits_pageSizeFixed
#[test]
fn a_pager_whose_pages_fit() {
    for (vertical, rtl) in PAGER_CONFIGS {
        let (mut ui, state) = Pager { size: 200.0, rtl, ..Pager::new(vertical, 2) }.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 100, 200));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_pageSizeFill_contentDoesNotFit
#[test]
fn a_pager_of_filling_pages() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager { page_size: PageSize::Fill, ..Pager::new(vertical, 4) }.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 400, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_pageSizeFixed_contentDoesNotFit
#[test]
fn a_pager_of_fixed_pages() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager::new(vertical, 4).rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 200, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_withPageSpacing
#[test]
fn a_pagers_spacing_is_in_its_content() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager { spacing: 10.0, ..Pager::new(vertical, 4) }.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 200 + 30, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_withContentPadding
#[test]
fn a_pagers_padding_is_in_its_content() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager { padding: (20.0, 30.0), ..Pager::new(vertical, 4) }.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 200 + 50, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_withReverseLayout
#[test]
fn a_reversed_pager_says_it_from_the_visual_start() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager { reverse: true, ..Pager::new(vertical, 4) }.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (100, 200, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_onScrollToPage
#[test]
fn a_pager_scrolled_to_a_page() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager::new(vertical, 6).rule();
        ui.wait_for_idle();
        run(&mut ui, async move {
            let _ = state.scroll_to_page(3, 0.0).await;
        });
        assert_eq!(read(&state), (150, 300, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_scrollByOffset
#[test]
fn a_pager_scrolled_by_an_offset() {
    for vertical in [true, false] {
        let (mut ui, state) = Pager::new(vertical, 6).rule();
        ui.wait_for_idle();
        state.dispatch_raw_delta(70.0);
        ui.wait_for_idle();
        assert_eq!(read(&state), (70, 300, 100));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_withLargePageCount_valuesCoercedToIntMax
#[test]
fn a_huge_pagers_values_stop_at_the_largest() {
    for vertical in [true, false] {
        let count = i32::MAX as usize;
        let (mut ui, state) = Pager { page: count - 10, ..Pager::new(vertical, count) }.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (i32::MAX, i32::MAX, 100));
    }
}

// ---- Lazy states ------------------------------------------------------------------------------

/// `count` items of 40 in a list of 100, with a gap and paddings.
fn list(reverse: bool) -> (ComposeTestRule, LazyListState) {
    let owned = LazyListState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyColumn!(
            modifier = Modifier.height(100.dp()),
            state = state,
            reverse_layout = reverse,
            content_padding = PaddingValues { top: 5.dp(), bottom: 15.dp(), ..PaddingValues::ZERO },
            vertical_arrangement = Arrangement::spaced_by(10.dp()),
            |scope: LazyListScope| {
                scope.items(20, |_| Spacer(Modifier.height(40.dp())));
            }
        );
    });
    (ui, state)
}

#[test]
fn a_lists_lengths_are_estimated_from_whats_in_view() {
    let (mut ui, state) = list(false);
    ui.wait_for_idle();
    // Each 40, 20 of them, 19 gaps of 10, 20 of padding.
    assert_eq!(read(&state), (0, 800 + 190 + 20, 100));
    run(&mut ui, async move {
        let _ = state.scroll_to_item(3, 7).await;
    });
    // (40 + 10) a line for 3, and the 7.
    assert_eq!(read(&state).0, 150 + 7);
}

#[test]
fn a_reversed_list_says_it_from_the_visual_start() {
    let (mut ui, state) = list(true);
    ui.wait_for_idle();
    assert_eq!(read(&state), (1010 - 100, 1010, 100));
}

#[test]
fn a_grids_lengths_are_estimated_from_the_lines_in_view() {
    let owned = LazyGridState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.height(100.dp()),
            state = state,
            vertical_arrangement = Arrangement::spaced_by(10.dp()),
            |scope: LazyGridScope| {
                scope.items(21, |_| Spacer(Modifier.height(40.dp())));
            }
        );
    });
    ui.wait_for_idle();
    // 11 lines of 40, 10 gaps of 10.
    assert_eq!(read(&state), (0, 440 + 100, 100));
    assert_eq!(state.layout_info().orientation, Orientation::Vertical);
    run(&mut ui, async move {
        let _ = state.scroll_to_item(5, 3).await;
    });
    // In its third line: (40 + 10) * 2, and the 3.
    assert_eq!(read(&state).0, 100 + 3);
}

/// Compose's test's right-to-left configuration, scrolled, and reversed.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_pageSizeFixed_contentDoesNotFit
#[test]
fn a_pager_right_to_left_says_it_the_same() {
    let (mut ui, state) = Pager { rtl: true, ..Pager::new(false, 6) }.rule();
    ui.wait_for_idle();
    assert_eq!(read(&state), (0, 300, 100));
    state.dispatch_raw_delta(70.0);
    ui.wait_for_idle();
    assert_eq!(read(&state), (70, 300, 100));
    let (mut ui, state) = Pager { rtl: true, reverse: true, ..Pager::new(false, 4) }.rule();
    ui.wait_for_idle();
    assert_eq!(read(&state), (100, 200, 100));
}

/// Pages composed beyond the viewport aren't the first in view.
#[test]
fn a_pager_goes_by_the_pages_in_view() {
    let (mut ui, state) = Pager { beyond_viewport: 2, ..Pager::new(false, 6) }.rule();
    ui.wait_for_idle();
    state.dispatch_raw_delta(120.0);
    ui.wait_for_idle();
    assert_eq!(read(&state).0, 120);
}

/// With paddings and spacing, scrolled so a page is only in the padding, and reversed.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/pager/PagerScrollIndicatorTest.kt, scrollIndicatorState_withContentPadding
#[test]
fn a_pager_with_padding_and_spacing_scrolled() {
    for vertical in [true, false] {
        let pager = Pager { padding: (20.0, 30.0), spacing: 10.0, ..Pager::new(vertical, 6) };
        let (mut ui, state) = pager.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 300 + 50 + 50, 100));
        for (delta, total) in [(15.0, 15), (50.0, 65), (50.0, 115)] {
            state.dispatch_raw_delta(delta);
            ui.wait_for_idle();
            assert_eq!(read(&state).0, total);
        }
        let pager = Pager { padding: (20.0, 30.0), reverse: true, ..Pager::new(vertical, 6) };
        let (mut ui, state) = pager.rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (250, 350, 100));
        state.dispatch_raw_delta(70.0);
        ui.wait_for_idle();
        assert_eq!(read(&state).0, 180);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_variableSizeItems
#[test]
fn a_list_of_varying_items_goes_by_their_average() {
    const SIZES: [f32; 5] = [50.0, 100.0, 80.0, 70.0, 120.0];
    let owned = LazyListState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyColumn!(modifier = Modifier.height(300.dp()), state = state, |scope: LazyListScope| {
            scope.items(10, |index| Spacer(Modifier.height(SIZES[index % 5].dp())));
        });
    });
    ui.wait_for_idle();
    // 50 + 100 + 80 + 70: exactly four fit, of 75 on average.
    assert_eq!(state.layout_info().visible_items_info.len(), 4);
    assert_eq!(read(&state), (0, 750, 300));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListScrollIndicatorTest.kt, scrollIndicatorState_reverseLayout_withInitialScroll
#[test]
fn a_reversed_list_scrolled_already_with_padding() {
    let owned = LazyListState::new(2, 25);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyColumn!(
            modifier = Modifier.height(250.dp()),
            state = state,
            reverse_layout = true,
            content_padding =
                PaddingValues { top: 20.dp(), bottom: 30.dp(), ..PaddingValues::ZERO },
            |scope: LazyListScope| {
                scope.items(20, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(read(&state), (1050 - 250 - 125, 1050, 250));
}

/// A grid of sections, each under a sticky header, 60 wide and 150 high, of two columns.
fn sticky_grid(sections: &'static [usize], reverse: bool) -> (ComposeTestRule, LazyGridState) {
    let owned = LazyGridState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.height(150.dp()).width(60.dp()),
            state = state,
            reverse_layout = reverse,
            |scope: LazyGridScope| {
                for &count in sections {
                    scope.sticky_header(|_| Spacer(Modifier.height(50.dp()).width(30.dp())));
                    scope.items(count, |_| Spacer(Modifier.height(50.dp()).width(30.dp())));
                }
            }
        );
    });
    (ui, state)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withStickyHeader
#[test]
fn a_grid_with_a_sticky_header() {
    let (mut ui, state) = sticky_grid(&[20], false);
    ui.wait_for_idle();
    assert_eq!(read(&state), (0, 11 * 50, 150));
    for (delta, total) in [(20.0, 20), (60.0, 80), (20.0, 100)] {
        state.dispatch_raw_delta(delta);
        ui.wait_for_idle();
        assert_eq!(read(&state).0, total);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withMultipleStickyHeaders
#[test]
fn a_grid_with_two_sticky_headers() {
    let (mut ui, state) = sticky_grid(&[4, 4], false);
    ui.wait_for_idle();
    state.dispatch_raw_delta(150.0);
    ui.wait_for_idle();
    assert_eq!(read(&state), (150, 5 * 50, 150));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollIndicatorTest.kt, scrollIndicatorState_withReverseLayout
#[test]
fn a_reversed_grid_says_it_from_the_visual_start() {
    let owned = LazyGridState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.height(150.dp()).width(60.dp()),
            state = state,
            reverse_layout = true,
            |scope: LazyGridScope| {
                scope.items(10, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    ui.wait_for_idle();
    // Five lines of 50, three in view.
    assert_eq!(read(&state), (100, 250, 150));
}

/// `i32::MAX` items: the estimates stop at the largest, as the pager's, with no panic.
#[test]
fn an_endless_lists_estimates_stop_at_the_largest() {
    let owned = LazyListState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyColumn!(modifier = Modifier.height(300.dp()), state = state, |scope: LazyListScope| {
            scope.items(i32::MAX as usize, |_| Spacer(Modifier.height(50.dp())));
        });
    });
    ui.wait_for_idle();
    assert_eq!(read(&state), (0, i32::MAX, 300));
    state.dispatch_raw_delta(5000.0);
    ui.wait_for_idle();
    assert_eq!(read(&state).0, 5000);
}

#[test]
fn an_endless_grids_estimates_stop_at_the_largest() {
    let owned = LazyGridState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.height(300.dp()),
            state = state,
            |scope: LazyGridScope| {
                scope.items(i32::MAX as usize, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(read(&state), (0, i32::MAX, 300));
    state.dispatch_raw_delta(5000.0);
    ui.wait_for_idle();
    assert_eq!(read(&state).0, 5000);
}

/// Before it's laid out, nothing, as Compose's `contentSize`.
#[test]
fn a_scroll_not_laid_out_has_no_content() {
    let owned = ScrollState::new(0);
    assert_eq!(read(&*owned), (0, 0, 0));
}

/// One object, as Compose's: what compares it sees the same one.
#[test]
fn each_state_gives_one_indicator() {
    fn same(state: &impl ScrollableState) {
        let (first, second) =
            (state.scroll_indicator_state().unwrap(), state.scroll_indicator_state().unwrap());
        assert!(std::rc::Rc::ptr_eq(&first, &second));
    }
    let scroll = ScrollState::new(0);
    same(&*scroll);
    let list = LazyListState::new(0, 0);
    same(&*list);
    let grid = LazyGridState::new(0, 0);
    same(&*grid);
    let pager = PagerState::new(0, 0.0, || 1);
    same(&*pager);
}
