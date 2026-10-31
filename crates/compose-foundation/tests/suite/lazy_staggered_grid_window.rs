//! A staggered grid's cache window, as Compose's `LazyStaggeredGridCacheWindowTest`, at density 1:
//! what exists is what's composed, placed or not, as items note it.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Constraints, Dp, IntSize};

use crate::lazy_support::Lazy;
use crate::scroll_support::run;

const SIZES: [i32; 12] = [100, 60, 80, 120, 90, 70, 110, 50, 130, 80, 90, 70];

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// `rule.runOnIdle { prefetchScheduler.executeActiveRequests() }`, until they ended.
fn idle(ui: &mut ComposeTestRule) {
    for _ in 0..50 {
        ui.wait_for_idle();
        if !ui.host().has_prefetch_work() {
            return;
        }
        ui.host().prefetch(later());
    }
    panic!("prefetching never ended");
}

fn orientations() -> [Lazy; 2] {
    [Lazy::column(), Lazy::row()]
}

struct Window {
    ui: ComposeTestRule,
    state: LazyStaggeredGridState,
    live: Rc<RefCell<BTreeSet<usize>>>,
}

impl Window {
    /// `scrollBy` of the test: the scroll, its measurement, and the requests it made.
    fn scroll(&mut self, pixels: i32) {
        let state = self.state;
        let _ = run(&mut self.ui, async move { state.scroll_by(pixels as f32).await });
        state.force_remeasure();
        idle(&mut self.ui);
    }

    fn exists(&self, item: usize) -> bool {
        self.live.borrow().contains(&item)
    }

    fn assert_exist(&self, items: &[usize]) {
        for item in items {
            assert!(self.exists(*item), "{item} in {:?}", self.live.borrow());
        }
    }

    fn assert_dont_exist(&self, items: &[usize]) {
        for item in items {
            assert!(!self.exists(*item), "{item} in {:?}", self.live.borrow());
        }
    }

    fn assert_displayed(&mut self, items: &[usize]) {
        for item in items {
            assert!(self.ui.on_node_with_tag(&item.to_string()).is_displayed(), "{item}");
        }
    }
}

/// `composeStaggeredGrid`.
#[allow(clippy::too_many_arguments)]
fn grid(
    lazy: Lazy,
    count: compose_runtime::MutableState<usize>,
    lanes: compose_runtime::MutableState<usize>,
    size: i32,
    first: usize,
    window: Rc<dyn LazyLayoutCacheWindow>,
    item_size: impl Fn(usize) -> i32 + Clone + 'static,
) -> Window {
    use_optimizations(Optimizations::NONE);
    let owned = LazyStaggeredGridState::new(first, 0);
    let state = *owned;
    std::mem::forget(owned);
    let live: Rc<RefCell<BTreeSet<usize>>> = Rc::default();
    let noted = live.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let (noted, item_size) = (noted.clone(), item_size.clone());
        let modifier = lazy.main_axis_size(Dp(size as f32));
        let cells = StaggeredGridCells::Fixed(lanes.get());
        let content = move |scope: LazyStaggeredGridScope| {
            let (noted, item_size) = (noted.clone(), item_size.clone());
            scope.items(count.get(), move |index| {
                let main = Dp(item_size(index) as f32);
                Spacer(lazy.main_axis_size(main).test_tag(index.to_string()));
                let noted = noted.clone();
                DisposableEffect(index, move |effects| {
                    noted.borrow_mut().insert(index);
                    let noted = noted.clone();
                    effects.on_dispose(move || {
                        noted.borrow_mut().remove(&index);
                    })
                });
            });
        };
        if lazy.vertical {
            LazyVerticalStaggeredGrid!(
                cells,
                modifier = modifier,
                state = state,
                cache_window = Some(window.clone()),
                content = content
            );
        } else {
            LazyHorizontalStaggeredGrid!(
                cells,
                modifier = modifier,
                state = state,
                cache_window = Some(window.clone()),
                content = content
            );
        }
    });
    idle(&mut ui);
    Window { ui, state, live }
}

fn window(ahead: i32, behind: i32) -> Rc<dyn LazyLayoutCacheWindow> {
    lazy_layout_cache_window(Dp(ahead as f32), Dp(behind as f32))
}

fn plain(
    lazy: Lazy,
    count: usize,
    size: i32,
    first: usize,
    cache: Rc<dyn LazyLayoutCacheWindow>,
    item_size: impl Fn(usize) -> i32 + Clone + 'static,
) -> Window {
    let (count, lanes) = (mutable_state_of(count), mutable_state_of(2usize));
    let window = grid(lazy, count.handle(), lanes.handle(), size, first, cache, item_size);
    std::mem::forget(count);
    std::mem::forget(lanes);
    window
}

fn varied(index: usize) -> i32 {
    SIZES[index % SIZES.len()]
}

fn seven(index: usize) -> i32 {
    [100, 60, 80, 120, 90, 70, 110][index % 7]
}

fn alternating(index: usize) -> i32 {
    if index.is_multiple_of(2) { 80 } else { 120 }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt initialPrefetchForward_noOverhang_oneItemPerLane
#[test]
fn the_first_window_takes_an_item_per_lane() {
    for lazy in orientations() {
        let window = plain(lazy, 4, 100, 0, window(1, 0), |_| 100);
        window.assert_exist(&[0, 1, 2, 3]);
        window.assert_dont_exist(&[4, 5]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt initialPrefetchForward_noOverhang_multipleItemsPerLane
#[test]
fn the_first_window_takes_several_items_per_lane() {
    for lazy in orientations() {
        let window = plain(lazy, 8, 100, 0, window(200, 0), |_| 100);
        window.assert_exist(&[0, 1, 2, 3, 4, 5]);
        window.assert_dont_exist(&[6, 7]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt initialPrefetchForward_overhang_noFetchForward
#[test]
fn an_overhang_as_large_as_the_window_fetches_nothing() {
    for lazy in orientations() {
        let window = plain(lazy, 4, 50, 0, window(1, 0), |_| 100);
        window.assert_exist(&[0, 1]);
        window.assert_dont_exist(&[2, 3]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt initialPrefetchForward_overhang_fetchForward
#[test]
fn a_window_past_the_overhang_fetches() {
    for lazy in orientations() {
        let window = plain(lazy, 4, 50, 0, window(51, 0), |_| 100);
        window.assert_exist(&[0, 1, 2, 3]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt staggered_initialPrefetchForward_variesByLane
#[test]
fn the_first_window_varies_by_lane() {
    for lazy in orientations() {
        let window = plain(lazy, 100, 150, 0, window(100, 0), seven);
        window.assert_exist(&[0, 1, 2, 3, 4, 5, 6]);
        window.assert_dont_exist(&[7]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt staggered_scrollForward_prefetchesCorrectly
#[test]
fn a_scroll_forward_fills_ahead_and_keeps_behind() {
    for lazy in orientations() {
        let mut window = plain(lazy, 100, 150, 0, window(100, 80), varied);
        window.scroll(200);
        window.assert_displayed(&[3, 4, 5, 6, 7, 8, 9]);
        window.assert_exist(&[2, 10]);
        window.assert_dont_exist(&[0, 1, 11]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt staggered_scrollBackward_prefetchesCorrectly
#[test]
fn a_scroll_backward_fills_ahead_and_keeps_behind() {
    for lazy in orientations() {
        let mut window = plain(lazy, 100, 150, 0, window(100, 80), varied);
        window.scroll(200);
        window.scroll(-100);
        window.assert_displayed(&[2, 3, 4, 5, 6]);
        window.assert_exist(&[0, 1, 7]);
        window.assert_dont_exist(&[8, 9, 10]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt staggered_scrollAlternating_retainsAndDisposesCorrectly
#[test]
fn scrolls_both_ways_keep_and_let_go() {
    for lazy in orientations() {
        let mut window = plain(lazy, 100, 150, 0, window(100, 80), varied);
        window.scroll(200);
        window.assert_exist(&[2]);
        window.assert_dont_exist(&[0]);
        window.scroll(-100);
        window.assert_exist(&[0, 1, 7]);
        window.assert_dont_exist(&[8]);
        window.scroll(100);
        window.assert_exist(&[2]);
        window.assert_dont_exist(&[0, 1]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt asymmetricCacheWindow_scrollForwardAndBackward
#[test]
fn a_window_larger_ahead_than_behind() {
    for lazy in orientations() {
        let mut window = plain(lazy, 100, 150, 0, window(150, 30), varied);
        window.scroll(200);
        window.assert_dont_exist(&[0, 1, 2]);
        window.assert_exist(&[10, 11]);
        window.scroll(-100);
        window.assert_exist(&[0, 1]);
        window.assert_dont_exist(&[7, 8]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt nonInitialEntryPoint_scrollForward_fromMiddle
#[test]
fn a_scroll_forward_from_the_middle() {
    for lazy in orientations() {
        let mut window = plain(lazy, 100, 150, 50, window(100, 100), alternating);
        window.scroll(50);
        window.assert_displayed(&[50, 51, 52, 53, 54]);
        window.assert_exist(&[55, 56]);
        window.assert_dont_exist(&[57]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt nonInitialEntryPoint_scrollBackward_fromMiddle
#[test]
fn a_scroll_backward_from_the_middle() {
    for lazy in orientations() {
        let mut window = plain(lazy, 100, 150, 50, window(100, 100), alternating);
        window.scroll(-50);
        window.assert_exist(&[46, 47, 48, 49]);
        window.assert_dont_exist(&[44]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt staggered_datasetChanged_reschedulesCorrectly_andDoesNotCrash
#[test]
fn fewer_items_reschedule_the_window() {
    for lazy in orientations() {
        let count = mutable_state_of(10usize);
        let lanes = mutable_state_of(2usize);
        let mut window =
            grid(lazy, count.handle(), lanes.handle(), 150, 0, window(100, 0), |index| {
                SIZES[index]
            });
        window.assert_exist(&[5, 6]);
        window.assert_dont_exist(&[7]);
        window.scroll(100);
        window.assert_exist(&[9]);
        count.set(8);
        idle(&mut window.ui);
        window.assert_exist(&[7]);
        window.assert_dont_exist(&[8, 9]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt laneCountChanged_resizesArraysAndClearsCache
#[test]
fn another_lane_count_starts_the_window_again() {
    for lazy in orientations() {
        let count = mutable_state_of(10usize);
        let lanes = mutable_state_of(2usize);
        let mut window =
            grid(lazy, count.handle(), lanes.handle(), 100, 0, window(100, 0), |_| 100);
        window.assert_exist(&[0, 1, 2, 3]);
        window.assert_dont_exist(&[4]);
        lanes.set(3);
        idle(&mut window.ui);
        window.assert_exist(&[0, 1, 2, 3, 4, 5]);
        window.assert_dont_exist(&[6]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt cacheWindow_hitsBeginningOfLayout
#[test]
fn the_window_stops_at_the_first_item() {
    for lazy in orientations() {
        let mut window = plain(lazy, 10, 90, 4, window(100, 0), |_| 50);
        window.assert_displayed(&[4, 5, 6, 7]);
        window.assert_dont_exist(&[0, 1, 2, 3]);
        window.scroll(-100);
        window.assert_displayed(&[0, 1, 2, 3]);
        window.assert_dont_exist(&[4, 5]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCacheWindowTest.kt cacheWindow_hitsEndOfLayout
#[test]
fn the_window_stops_at_the_last_item() {
    for lazy in orientations() {
        let mut window = plain(lazy, 8, 90, 0, window(150, 0), |_| 50);
        window.assert_displayed(&[0, 1, 2, 3]);
        window.assert_exist(&[4, 5, 6, 7]);
        window.scroll(110);
        window.assert_displayed(&[4, 5, 6, 7]);
        window.assert_dont_exist(&[0, 1, 2, 3]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPrefetcherTest.kt fullSpanIsPrefetchedCorrectly
#[test]
fn a_full_line_is_prefetched_across_the_lanes() {
    for lazy in orientations() {
        use_optimizations(Optimizations::NONE);
        let owned = LazyStaggeredGridState::new(0, 0);
        let state = *owned;
        std::mem::forget(owned);
        let seen: Rc<RefCell<Option<Constraints>>> = Rc::default();
        let noted = seen.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            let noted = noted.clone();
            let modifier = lazy.sized(Dp(150.0), Dp(60.0));
            let cells = StaggeredGridCells::Fixed(2);
            let cache = window(40, 0);
            let content = move |scope: LazyStaggeredGridScope| {
                let noted = noted.clone();
                scope.items_with_span(
                    100,
                    |_| (),
                    |index| {
                        if index % 10 == 0 {
                            StaggeredGridItemSpan::FullLine
                        } else {
                            StaggeredGridItemSpan::SingleLane
                        }
                    },
                    move |index| {
                        let noted = noted.clone();
                        Spacer(lazy.main_axis_size(Dp(if index == 0 { 30.0 } else { 60.0 })).then(
                            Modifier.layout(move |scope, measurable, constraints: Constraints| {
                                if index == 10 {
                                    *noted.borrow_mut() = Some(constraints);
                                }
                                let placeable = scope.measure(measurable, constraints);
                                scope.place(&placeable, 0, 0);
                                scope.layout(placeable.width, placeable.height)
                            }),
                        ));
                    },
                );
            };
            if lazy.vertical {
                LazyVerticalStaggeredGrid!(
                    cells,
                    modifier = modifier,
                    state = state,
                    cache_window = Some(cache),
                    content = content
                );
            } else {
                LazyHorizontalStaggeredGrid!(
                    cells,
                    modifier = modifier,
                    state = state,
                    cache_window = Some(cache),
                    content = content
                );
            }
        });
        idle(&mut ui);
        // Items 1 to 9 in 30 + 4.5 lines of 60: item 10 next.
        let _ = run(&mut ui, async move { state.scroll_by(150.0).await });
        idle(&mut ui);
        let constraints = seen.borrow().expect("prefetched and measured");
        let across = if lazy.vertical { constraints.max_width } else { constraints.max_height };
        assert_eq!(across, 60, "{constraints:?}");
    }
}

/// Lanes known from before, the shorter asking first for an item past the next: composed across
/// that next one, and kept with it.
#[test]
fn a_request_past_the_next_item_is_kept() {
    for lazy in orientations() {
        let mut window = plain(lazy, 100, 150, 0, window(100, 0), varied);
        window.scroll(400);
        window.scroll(-400);
        window.scroll(30);
        let shown: Vec<usize> =
            window.state.layout_info().visible_items_info.iter().map(|it| it.index).collect();
        let last = *shown.last().expect("items");
        window.assert_exist(&[last + 1, last + 2]);
    }
}
