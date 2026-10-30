//! Compose's `scrollIndicatorState` tests of `ScrollTest.kt` and `PagerScrollIndicatorTest.kt`, and
//! the lazy states' estimates (`LazyListLayoutInfo.calculateContentSize` and the grid's).

use compose_foundation::gestures::{Orientation, ScrollableState};
use compose_foundation::layout::*;
use compose_foundation::*;
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
}

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
        }
    }

    fn rule(self) -> (ComposeTestRule, PagerState) {
        let count = self.count;
        let owned = PagerState::new(self.page, 0.0, move || count);
        let state = *owned;
        std::mem::forget(owned);
        let ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
            let page = |_: PagerScope, _: usize| Spacer(Modifier.fill_max_size());
            let modifier = Modifier.size(self.size.dp());
            let (before, after) = (Dp(self.padding.0), Dp(self.padding.1));
            if self.vertical {
                VerticalPager!(
                    state,
                    modifier = modifier,
                    content_padding =
                        PaddingValues { top: before, bottom: after, ..PaddingValues::ZERO },
                    page_size = self.page_size.clone(),
                    page_spacing = Dp(self.spacing),
                    reverse_layout = self.reverse,
                    page_content = page
                );
            } else {
                HorizontalPager!(
                    state,
                    modifier = modifier,
                    content_padding =
                        PaddingValues { start: before, end: after, ..PaddingValues::ZERO },
                    page_size = self.page_size.clone(),
                    page_spacing = Dp(self.spacing),
                    reverse_layout = self.reverse,
                    page_content = page
                );
            }
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
    for vertical in [true, false] {
        let (mut ui, state) = Pager::new(vertical, 2).rule();
        ui.wait_for_idle();
        assert_eq!(read(&state), (0, 100, 100));
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
