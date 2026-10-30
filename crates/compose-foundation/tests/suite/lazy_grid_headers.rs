//! Grids' sticky headers, as Compose's `LazyGridHeadersTest`, in both orientations. Density 1.
//! Left: `lazyVerticalGridwithPinnedItem` (feature:pinnable). The focus tests are here without their
//! pinning: the item focused is one in view.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Dp, FocusRequester, IntSize, UnitExt};

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::lazy_support::{Lazy, no_padding};
use crate::scroll_support::run;

const GRID: &str = "LazyGrid";
const FIRST: &str = "firstHeaderTag";
const SECOND: &str = "secondHeaderTag";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

/// A cell or a header, `size` long and filling across.
fn tall(lazy: Lazy, size: f32, tag: String) {
    Spacer(lazy.main_axis_size(Dp(size)).then(lazy.fill_max_cross_axis()).test_tag(tag));
}

/// A grid of 3 cells a line, `length` long, as the tests' `LazyVerticalGrid`/`LazyHorizontalGrid`.
fn grid(
    lazy: Lazy,
    length: f32,
    state: LazyGridState,
    padding: PaddingValues,
    content: impl Fn(LazyGridScope) + 'static,
) -> ComposeTestRule {
    let content = std::rc::Rc::new(content);
    rule(move || {
        let content = content.clone();
        crate::lazy_support::LazyGrid(
            lazy,
            3,
            lazy.main_axis_size(Dp(length)).then(lazy.cross_axis_size(600.dp())).test_tag(GRID),
            state,
            padding,
            move |scope: LazyGridScope| content(scope),
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyVerticalGridShowsHeader, lazyHorizontalGridShowsHeader
#[test]
fn a_grid_shows_its_header() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let mut ui = grid(lazy, 300.0, *state, no_padding(), move |scope| {
            scope.sticky_header(move |_| tall(lazy, 101.0, FIRST.into()));
            scope.items(6, move |index| tall(lazy, 101.0, (index + 1).to_string()));
            scope.sticky_header(move |_| tall(lazy, 101.0, SECOND.into()));
        });
        assert!(ui.on_node_with_tag(FIRST).is_displayed());
        assert!(ui.on_node_with_tag("1").is_displayed());
        assert!(ui.on_node_with_tag("2").is_displayed());
        ui.on_node_with_tag(SECOND).assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyVerticalGridShowsHeadersOnScroll, lazyHorizontalGridShowsHeadersOnScroll
#[test]
fn a_grids_header_sticks_on_scroll() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let mut ui = grid(lazy, 300.0, *state, no_padding(), move |scope| {
            scope.sticky_header(move |_| tall(lazy, 101.0, FIRST.into()));
            scope.items(3, move |index| tall(lazy, 101.0, (index + 1).to_string()));
            scope.sticky_header(move |_| tall(lazy, 101.0, SECOND.into()));
        });
        lazy.drag_main_axis_by(&mut ui, GRID, 102.0);
        assert!(ui.on_node_with_tag(FIRST).is_displayed());
        assert_eq!(lazy.main_start(&mut ui, FIRST), 0.0);
        let info = state.layout_info();
        let first = &info.visible_items_info[0];
        assert_eq!(first.index, 0);
        assert_eq!((first.offset.x, first.offset.y), (0, 0));
        assert!(ui.on_node_with_tag("2").is_displayed());
        assert!(ui.on_node_with_tag(SECOND).is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyVerticalGridHeaderIsReplaced, lazyHorizontalGridHeaderIsReplaced
#[test]
fn a_grids_header_is_replaced_by_the_next() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let mut ui = grid(lazy, 300.0, *state, no_padding(), move |scope| {
            scope.sticky_header(move |_| tall(lazy, 101.0, FIRST.into()));
            scope.sticky_header(move |_| tall(lazy, 101.0, SECOND.into()));
            scope.items(6, move |index| tall(lazy, 101.0, (index + 1).to_string()));
        });
        lazy.drag_main_axis_by(&mut ui, GRID, 105.0);
        assert!(!ui.on_node_with_tag(FIRST).is_displayed());
        assert!(ui.on_node_with_tag(SECOND).is_displayed());
        assert!(ui.on_node_with_tag("1").is_displayed());
        assert!(ui.on_node_with_tag("2").is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, headerIsDisplayedWhenItIsFullyInContentPadding
#[test]
fn a_grids_header_shows_in_the_content_padding() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.required_size(400.dp()),
            state = handle,
            content_padding = PaddingValues { top: 200.dp(), ..PaddingValues::ZERO },
            |scope: LazyGridScope| {
                scope
                    .sticky_header(|_| Spacer(Modifier.required_size(100.dp()).test_tag("header")));
                scope.items(12, |index| {
                    Spacer(Modifier.required_size(100.dp()).test_tag(index.to_string()))
                });
            }
        );
    });
    let _ = run(&mut ui, async move { handle.scroll_to_item(1, 50).await });
    let lazy = Lazy::column();
    assert_eq!(lazy.main_start(&mut ui, "header"), 50.0);
    let info = state.layout_info();
    let first = &info.visible_items_info[0];
    assert_eq!(first.index, 0);
    assert_eq!(first.offset.y, 50 - 200);
    assert_eq!(lazy.main_start(&mut ui, "0"), 150.0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyVerticalGridShowsHeader2
#[test]
fn a_grids_header_is_pushed_out_by_the_next() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let lazy = Lazy::column();
    let mut ui = grid(lazy, 350.0, handle, no_padding(), move |scope| {
        scope.sticky_header(move |_| tall(lazy, 100.0, FIRST.into()));
        scope.sticky_header(move |_| tall(lazy, 100.0, SECOND.into()));
        scope.items(100, move |index| tall(lazy, 100.0, index.to_string()));
    });
    let _ = run(&mut ui, async move { handle.scroll_by(20.0).await });
    assert_eq!(lazy.main_start(&mut ui, FIRST), -20.0);
    assert_eq!(lazy.main_start(&mut ui, SECOND), 80.0);
    assert_eq!(lazy.main_start(&mut ui, "0"), 180.0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyGrid_withEmptyHeader_shouldNotCrash
#[test]
fn a_grids_empty_header_is_fine() {
    let state = LazyGridState::new(0, 0);
    let lazy = Lazy::column();
    let mut ui = grid(lazy, 350.0, *state, no_padding(), move |scope| {
        scope.sticky_header(|_| {});
        scope.items(2, move |index| tall(lazy, 100.0, (index + 1).to_string()));
    });
    assert!(ui.on_node_with_tag("1").is_displayed());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyGrid_withEmptyHeader_showsHeadersOnScroll
#[test]
fn a_grids_empty_headers_dont_replace_the_real_one() {
    let state = LazyGridState::new(0, 0);
    let lazy = Lazy::column();
    let mut ui = grid(lazy, 300.0, *state, no_padding(), move |scope| {
        scope.sticky_header(move |_| tall(lazy, 101.0, FIRST.into()));
        for _ in 0..10 {
            scope.item(move || tall(lazy, 101.0, "item".into()));
            scope.sticky_header(|_| {});
        }
    });
    lazy.drag_main_axis_by(&mut ui, GRID, 10.0);
    assert!(ui.on_node_with_tag(FIRST).is_displayed());
    assert_eq!(lazy.main_start(&mut ui, FIRST), 0.0);
    let info = state.layout_info();
    assert_eq!(info.visible_items_info[0].index, 0);
    assert_eq!(info.visible_items_info[0].offset.y, 0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowStrategy.kt, firstVisibleItemIndex: a sticking item's line is `UnknownRow`, so nothing's requested before it: unlike a list's
#[test]
fn scrolling_back_under_a_grids_header_requests_nothing() {
    use compose_foundation::lazy_layout_cache_window;
    let state = LazyGridState::new(30, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = handle,
            cache_window = Some(lazy_layout_cache_window(100.dp(), 0.dp())),
            |scope: LazyGridScope| {
                for _ in 0..4 {
                    scope.sticky_header(|_| Spacer(Modifier.height(50.dp()).fill_max_width()));
                    scope.items(21, |_| Spacer(Modifier.height(50.dp()).fill_max_width()));
                }
            }
        );
    });
    ui.wait_for_idle();
    handle.dispatch_raw_delta(-5.0);
    ui.wait_for_idle();
    assert_eq!(handle.prefetch_requests(), Vec::<(usize, bool)>::new());
    // Forward, the window fills as ever.
    handle.dispatch_raw_delta(10.0);
    ui.wait_for_idle();
    assert!(!handle.prefetch_requests().is_empty());
}

/// `Fixed(3)` lines of 50 px in a height of 200, with what `content` gives, and `window` if given.
fn three_a_line(
    first: usize,
    window: Option<Rc<dyn LazyLayoutCacheWindow>>,
    content: impl Fn(&LazyGridScope, Rc<RefCell<std::collections::BTreeSet<usize>>>) + 'static,
) -> (ComposeTestRule, Owned<LazyGridState>, Rc<RefCell<std::collections::BTreeSet<usize>>>) {
    use_optimizations(Optimizations::NONE);
    let state = LazyGridState::new(first, 0);
    let handle = *state;
    let live: Rc<RefCell<std::collections::BTreeSet<usize>>> = Rc::default();
    let seen = live.clone();
    let content = Rc::new(content);
    let ui = ComposeTestRule::sized(IntSize::new(300, 200), move || {
        let (seen, content) = (seen.clone(), content.clone());
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = handle,
            cache_window = window.clone(),
            move |scope: LazyGridScope| content(&scope, seen.clone())
        );
    });
    (ui, state, live)
}

#[composable]
fn Cell50(index: usize, seen: Rc<RefCell<std::collections::BTreeSet<usize>>>) {
    let noted = seen.clone();
    DisposableEffect(index, move |scope| {
        noted.borrow_mut().insert(index);
        let noted = noted.clone();
        scope.on_dispose(move || {
            noted.borrow_mut().remove(&index);
        })
    });
    Spacer(Modifier.height(50.dp()).test_tag(index.to_string()));
}

fn idle(ui: &mut ComposeTestRule) {
    let later = std::time::Instant::now() + std::time::Duration::from_secs(60);
    for _ in 0..50 {
        ui.wait_for_idle();
        if !ui.host().has_prefetch_work() {
            return;
        }
        ui.host().prefetch(later);
    }
}

/// Scrolls `steps` times by `step`, letting each settle, and notes (the first item in view, the
/// items requested) as each new request shows, and how many steps had any.
fn walk(
    ui: &mut ComposeTestRule,
    state: LazyGridState,
    steps: usize,
    step: f32,
) -> (Vec<(usize, Vec<usize>)>, usize) {
    let (mut notes, mut live) = (Vec::new(), 0);
    for _ in 0..steps {
        state.dispatch_raw_delta(step);
        ui.wait_for_idle();
        let requested: Vec<usize> =
            state.prefetch_requests().into_iter().map(|(item, _)| item).collect();
        if !requested.is_empty() {
            live += 1;
            let note = (state.first_visible_item_index(), requested);
            if notes.last() != Some(&note) {
                notes.push(note);
            }
        }
        idle(ui);
    }
    (notes, live)
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/CacheWindowLogic.kt, onVisibleItemsUpdated: a line going from a size of 0 to its own refills the window, which is how a grid's window recovers once a header stops sticking; with `forEachVisibleItem` over LazyGridCacheWindowStrategy.kt's lines from `lineIndex` -1
#[test]
fn a_grids_window_recovers_when_its_header_stops_sticking() {
    let (mut ui, state, _) = three_a_line(0, None, |scope, seen| {
        let before = seen.clone();
        scope.items(30, move |index| Cell50(index, before.clone()));
        scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
        scope.items(150, move |index| Cell50(31 + index, seen.clone()));
    });
    let handle = *state;
    idle(&mut ui);
    walk(&mut ui, handle, 200, 5.0);
    let (notes, _) = walk(&mut ui, handle, 170, -5.0);
    let expected: Vec<(usize, Vec<usize>)> = (2..=8)
        .rev()
        .map(|line| ((line + 1) * 3, vec![line * 3, line * 3 + 1, line * 3 + 2]))
        .collect();
    assert_eq!(notes, expected);
}

// Compose: the same, a header at the very start: with it sticking all along, the lines before those in view weigh 0, and a request goes on at each step it's needed
#[test]
fn a_grids_window_goes_on_under_a_header_sticking_all_along() {
    let (mut ui, state, _) = three_a_line(0, None, |scope, seen| {
        scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
        scope.items(300, move |index| Cell50(1 + index, seen.clone()));
    });
    let handle = *state;
    idle(&mut ui);
    let (_, live) = walk(&mut ui, handle, 300, 5.0);
    assert_eq!(live, 59);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGrid.kt, keepAroundItems: `(lastVisibleItemIndex + 1)..perLaneCacheWindowEndItemIndex[0]`, whatever its start of -1
#[test]
fn a_grids_window_keeps_ahead_while_its_header_sticks() {
    let window = lazy_layout_cache_window(0.dp(), 200.dp());
    let (mut ui, state, live) = three_a_line(60, Some(window), |scope, seen| {
        let before = seen.clone();
        scope.items(2, move |index| Cell50(index, before.clone()));
        scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
        scope.items(300, move |index| Cell50(3 + index, seen.clone()));
    });
    let handle = *state;
    idle(&mut ui);
    // Lines 21 to 25 show, 10 px in; back to line 21's start, line 25 goes out of view, within the
    // window. Further back, a line coming from a size of 0 would refill the window first, its end
    // the last in view by the time `keepAroundItems` reads it.
    let _ = run(&mut ui, async move { handle.scroll_by(10.0).await });
    idle(&mut ui);
    assert!(ui.on_node_with_tag("74").is_displayed());
    let _ = run(&mut ui, async move { handle.scroll_by(-10.0).await });
    idle(&mut ui);
    assert!(!ui.on_node_with_tag("74").is_displayed());
    state.force_remeasure();
    idle(&mut ui);
    assert!((60..=74).all(|item| live.borrow().contains(&item)), "{:?}", live.borrow());
}

// Compose: the same, the start of the window isn't one to keep from
#[test]
fn a_grids_window_keeps_nothing_behind_while_its_header_sticks() {
    let window = lazy_layout_cache_window(0.dp(), 200.dp());
    let (mut ui, state, live) = three_a_line(0, Some(window), |scope, seen| {
        scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
        scope.items(300, move |index| Cell50(1 + index, seen.clone()));
    });
    let handle = *state;
    idle(&mut ui);
    let _ = run(&mut ui, async move { handle.scroll_by(300.0).await });
    idle(&mut ui);
    state.force_remeasure();
    idle(&mut ui);
    assert!(!live.borrow().contains(&1), "{:?}", live.borrow());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfo.kt, visibleLinesAverageMainAxisSize: "Filter out exiting items": a sticking item's line -1 is no line
#[test]
fn a_grids_default_window_leaves_its_header_out_of_the_average() {
    let state = LazyGridState::new(10, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = handle,
            |scope: LazyGridScope| {
                scope.sticky_header(|_| Spacer(Modifier.height(62.dp())));
                scope.items(100, |_| Spacer(Modifier.height(30.dp())));
            }
        );
    });
    ui.wait_for_idle();
    let window = LazyGridDefaults::cache_window(handle);
    assert_eq!(window.calculate_ahead_window(compose_ui::Density::new(1.0), 100), 30);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStickyItems.kt, applyStickyItems: `item.position(offset, 0, …)`, at the start across
#[test]
fn a_grids_header_sticks_at_the_start_across() {
    let mut ui = rule(|| {
        LazyVerticalGrid!(
            GridCells::FixedSize(100.dp()),
            modifier = Modifier.size((350.dp(), 200.dp())),
            horizontal_arrangement = Arrangement::CENTER,
            |scope: LazyGridScope| {
                scope.sticky_header(|_| {
                    Spacer(Modifier.height(50.dp()).fill_max_width().test_tag("header"))
                });
                scope.items(30, |index| {
                    Spacer(Modifier.height(50.dp()).test_tag(index.to_string()))
                });
            }
        );
    });
    let header = crate::lazy_support::bounds(&mut ui, "header");
    assert_eq!((header.left, header.width()), (0.0, 300.0));
    assert_eq!(crate::lazy_support::bounds(&mut ui, "0").left, 25.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStickyItems.kt: `stickingItems` placed after all others, so on top: hit first
#[test]
fn a_grids_header_is_hit_before_the_item_under_it() {
    let (header_hits, item_hits) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (header, item) = (header_hits.clone(), item_hits.clone());
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        let (header, item) = (header.clone(), item.clone());
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((100.dp(), 200.dp())),
            state = handle,
            move |scope: LazyGridScope| {
                let (header, item) = (header.clone(), item.clone());
                scope.sticky_header(move |_| {
                    let header = header.clone();
                    Spacer(
                        Modifier
                            .height(50.dp())
                            .fill_max_width()
                            .clickable(move || header.set(header.get() + 1)),
                    );
                });
                scope.items(10, move |_| {
                    let item = item.clone();
                    Spacer(
                        Modifier
                            .height(50.dp())
                            .fill_max_width()
                            .clickable(move || item.set(item.get() + 1)),
                    );
                });
            }
        );
    });
    let _ = run(&mut ui, async move { handle.scroll_by(60.0).await });
    let finger = crate::scroll_support::Touch::down(&mut ui, compose_ui::Offset::new(50.0, 10.0));
    finger.up(&mut ui);
    ui.wait_for_idle();
    assert_eq!((header_hits.get(), item_hits.get()), (1, 0));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridIntervalContent.kt, stickyHeader: `content.invoke(this, headerIndex)`
#[test]
fn a_grids_header_is_told_its_index() {
    let told = Rc::new(RefCell::new(Vec::new()));
    let noted = told.clone();
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        let noted = noted.clone();
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = handle,
            move |scope: LazyGridScope| {
                for _ in 0..3 {
                    let noted = noted.clone();
                    scope.sticky_header(move |index| {
                        noted.borrow_mut().push(index);
                        Spacer(Modifier.height(50.dp()));
                    });
                    scope.items(3, |_| Spacer(Modifier.height(50.dp())));
                }
            }
        );
    });
    let _ = run(&mut ui, async move { handle.scroll_to_item(5, 0).await });
    ui.wait_for_idle();
    assert_eq!(told.borrow().last(), Some(&4));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridMeasure.kt: applyStickyItems gets `firstItemIndex`, of the first line laid out, those under the padding too
#[test]
fn a_grids_header_is_the_one_over_the_first_line_under_the_padding() {
    let state = LazyGridState::new(3, 0);
    let mut ui = {
        let handle = *state;
        rule(move || {
            LazyVerticalGrid!(
                GridCells::Fixed(1),
                modifier = Modifier.size((100.dp(), 300.dp())),
                state = handle,
                content_padding = PaddingValues { top: 100.dp(), ..PaddingValues::ZERO },
                |scope: LazyGridScope| {
                    scope.sticky_header(|_| {
                        Spacer(Modifier.height(50.dp()).fill_max_width().test_tag("first"))
                    });
                    scope.items(2, |_| Spacer(Modifier.height(50.dp())));
                    scope.sticky_header(|_| {
                        Spacer(Modifier.height(50.dp()).fill_max_width().test_tag("second"))
                    });
                    scope.items(20, |_| Spacer(Modifier.height(50.dp())));
                }
            );
        })
    };
    let lazy = Lazy::column();
    // Item 1 from 0, under the padding: the first section's. The second header is where it is.
    assert_eq!(lazy.main_start(&mut ui, "first"), 0.0);
    assert_eq!(lazy.main_start(&mut ui, "second"), 100.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStickyItems.kt, applyStickyItems: `item.mainAxisSizeWithSpacings`, so a header is pushed out with its spacing
#[test]
fn a_grids_header_is_pushed_out_with_its_spacing() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((100.dp(), 300.dp())),
            state = handle,
            vertical_arrangement = Arrangement::spaced_by(10.dp()),
            |scope: LazyGridScope| {
                scope.sticky_header(|_| {
                    Spacer(Modifier.height(50.dp()).fill_max_width().test_tag("first"))
                });
                scope.sticky_header(|_| {
                    Spacer(Modifier.height(50.dp()).fill_max_width().test_tag("second"))
                });
                scope.items(20, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    let _ = run(&mut ui, async move { handle.scroll_by(20.0).await });
    let lazy = Lazy::column();
    assert_eq!(lazy.main_start(&mut ui, "second"), 40.0);
    assert_eq!(lazy.main_start(&mut ui, "first"), -20.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemInfo.kt, UnknownRow and UnknownColumn; LazyLayoutStickyItems.kt: a sticking item has neither
#[test]
fn a_grids_sticking_header_has_no_line() {
    assert_eq!(LazyGridItemInfo::UNKNOWN_ROW, -1);
    assert_eq!(LazyGridItemInfo::UNKNOWN_COLUMN, -1);
    let state = LazyGridState::new(0, 0);
    let mut ui = {
        let handle = *state;
        rule(move || {
            LazyVerticalGrid!(
                GridCells::Fixed(1),
                modifier = Modifier.size((100.dp(), 200.dp())),
                state = handle,
                |scope: LazyGridScope| {
                    scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
                    scope.items(20, |_| Spacer(Modifier.height(50.dp())));
                }
            );
        })
    };
    ui.wait_for_idle();
    let info = state.layout_info();
    let header = info.visible_items_info[0];
    assert_eq!((header.index, header.row, header.column), (0, -1, -1));
    assert_eq!((info.visible_items_info[1].row, info.visible_items_info[1].column), (1, 0));
    // The scroll scope's lines don't take it for one: it says the first line's item is the first.
    let handle = *state;
    let _ = run(&mut ui, async move { handle.scroll_to_item(5, 0).await });
    let scope =
        handle.lazy_layout_scroll_scope(compose_foundation::gestures::ScrollScope::new(|_| 0.0));
    assert_eq!(scope.first_visible_item_index(), 5);
    // An animated scroll lands where asked.
    let _ = run(&mut ui, async move { handle.animate_scroll_to_item(3, 0).await });
    assert_eq!(state.first_visible_item_index(), 3);
    let _ = run(&mut ui, async move { handle.animate_scroll_to_item(5, 10).await });
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (5, 10)
    );
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowStrategy.kt, schedulePrefetch's window: no request but for what the window computes once the header's composed
#[test]
fn a_grid_with_a_header_not_composed_yet_tells_the_window_nothing() {
    // Scrolled past the second header, back by a line into the first section: its header isn't
    // composed any more, the line is. The frame's first layout lacks it: no request's made from it.
    let (mut ui, state, _) = three_a_line(0, None, |scope, seen| {
        scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
        let before = seen.clone();
        scope.items(30, move |index| Cell50(1 + index, before.clone()));
        scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
        scope.items(150, move |index| Cell50(32 + index, seen.clone()));
    });
    let handle = *state;
    idle(&mut ui);
    let _ = run(&mut ui, async move { handle.scroll_by(650.0).await });
    idle(&mut ui);
    handle.dispatch_raw_delta(-60.0);
    ui.wait_for_idle();
    assert_eq!(handle.prefetch_requests(), Vec::<(usize, bool)>::new());
    assert_eq!(handle.prefetch_units_held(), Vec::<usize>::new());
}

/// A column or a row of `Fixed(1)` in 35 px, a 5 px header (or two) and 10 px focusable items.
#[allow(clippy::too_many_arguments)]
fn focusable_grid(
    lazy: Lazy,
    headers: usize,
    padding: f32,
    direction: compose_ui::LayoutDirection,
    state: LazyGridState,
    requesters: Rc<RefCell<Vec<FocusRequester>>>,
) -> ComposeTestRule {
    rule(move || {
        let requesters = requesters.clone();
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
            move || {
                let requesters = requesters.clone();
                crate::lazy_support::LazyGrid(
                    lazy,
                    1,
                    lazy.main_axis_size(35.dp()),
                    state,
                    PaddingValues::all(Dp(padding)),
                    move |scope: LazyGridScope| {
                        for _ in 0..headers {
                            scope.sticky_header(move |_| {
                                Spacer(
                                    lazy.main_axis_size(5.dp()).then(lazy.fill_max_cross_axis()),
                                );
                            });
                        }
                        let requesters = requesters.clone();
                        scope.items(10, move |index| {
                            let requester = remember(FocusRequester::new);
                            let mut all = requesters.borrow_mut();
                            if all.len() <= index {
                                all.resize(index + 1, requester);
                            }
                            all[index] = requester;
                            drop(all);
                            Spacer(
                                lazy.main_axis_size(10.dp())
                                    .then(lazy.fill_max_cross_axis())
                                    .focus_requester(requester)
                                    .focusable(),
                            );
                        });
                    },
                );
            },
        );
    })
}

fn main_offset(state: &LazyGridState, lazy: Lazy, item: usize) -> i32 {
    let info = state.layout_info();
    let found = info.visible_items_info.iter().find(|it| it.index == item).expect("in view");
    if lazy.vertical { found.offset.y } else { found.offset.x }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyGrid_withHeader_focusScrollsRevealsEntireItemUnderHeaderIgnoringContentPadding: without its pinning, the item focused in view; whatever the padding
#[test]
fn a_focused_grid_item_comes_out_from_under_its_header() {
    for padding in [0.0, 5.0] {
        let lazy = Lazy::column();
        let state = LazyGridState::new(2, 0);
        let requesters: Rc<RefCell<Vec<FocusRequester>>> = Rc::default();
        let mut ui = focusable_grid(
            lazy,
            1,
            padding,
            compose_ui::LayoutDirection::Ltr,
            *state,
            requesters.clone(),
        );
        ui.wait_for_idle();
        assert!(requesters.borrow()[1].request_focus());
        ui.wait_for_idle();
        // Index 2 is the second item, after the header: its start at the header's end.
        assert_eq!(main_offset(&state, lazy, 2), 5 - padding as i32, "padding {padding}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyGrid_withMultipleHeaders_focusScrollsRevealsEntireItemUnderHeaders: only the one sticking counts
#[test]
fn a_focused_grid_item_comes_out_from_under_the_header_sticking() {
    let lazy = Lazy::column();
    let state = LazyGridState::new(3, 0);
    let requesters: Rc<RefCell<Vec<FocusRequester>>> = Rc::default();
    let mut ui =
        focusable_grid(lazy, 2, 0.0, compose_ui::LayoutDirection::Ltr, *state, requesters.clone());
    ui.wait_for_idle();
    assert!(requesters.borrow()[1].request_focus());
    ui.wait_for_idle();
    assert_eq!(main_offset(&state, lazy, 3), 5);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridHeadersTest.kt, lazyGrid_withHeader_layoutOrientations_focusScrollsRevealsEntireItemUnderHeader
#[test]
fn a_focused_grid_item_comes_out_from_under_its_header_in_any_layout() {
    for reverse_layout in [true, false] {
        for direction in [compose_ui::LayoutDirection::Ltr, compose_ui::LayoutDirection::Rtl] {
            for lazy in Lazy::grids() {
                let lazy = Lazy { reverse_layout, ..lazy };
                let state = LazyGridState::new(2, 0);
                let requesters: Rc<RefCell<Vec<FocusRequester>>> = Rc::default();
                let mut ui = focusable_grid(lazy, 1, 0.0, direction, *state, requesters.clone());
                ui.wait_for_idle();
                assert!(requesters.borrow()[1].request_focus());
                ui.wait_for_idle();
                assert_eq!(main_offset(&state, lazy, 2), 5, "{lazy:?} {direction:?}");
            }
        }
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfo.kt, visibleLinesAverageMainAxisSize: the same with the header in place, at the first line
#[test]
fn a_grids_default_window_leaves_out_a_header_first_in_view() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = handle,
            |scope: LazyGridScope| {
                scope.sticky_header(|_| Spacer(Modifier.height(62.dp())));
                scope.items(100, |_| Spacer(Modifier.height(30.dp())));
            }
        );
    });
    ui.wait_for_idle();
    let window = LazyGridDefaults::cache_window(handle);
    assert_eq!(window.calculate_ahead_window(compose_ui::Density::new(1.0), 100), 30);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridScrollScope.kt, calculateDistanceTo: from the items with a line: an animated scroll to the header's own index, sticking at the start, isn't told it's already there
#[test]
fn an_animated_scroll_back_to_a_sticking_header_goes_all_the_way() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((100.dp(), 200.dp())),
            state = handle,
            |scope: LazyGridScope| {
                scope.sticky_header(|_| Spacer(Modifier.height(50.dp())));
                scope.items(50, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    let _ = run(&mut ui, async move { handle.scroll_by(230.0).await });
    let _ = run(&mut ui, async move { handle.animate_scroll_to_item(0, 0).await });
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 0)
    );
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutBringIntoViewSpec.kt, StickyHeaderBringIntoViewSpec: `containerSize - nonStickyItemsOffset`: an item partly past the end comes in, by the distance past it
#[test]
fn a_focused_grid_item_past_the_end_comes_in_as_far_as_it_is_past() {
    let lazy = Lazy::column();
    let state = LazyGridState::new(2, 0);
    let requesters: Rc<RefCell<Vec<FocusRequester>>> = Rc::default();
    let mut ui =
        focusable_grid(lazy, 1, 0.0, compose_ui::LayoutDirection::Ltr, *state, requesters.clone());
    ui.wait_for_idle();
    // Index 5, the fifth item, from 30 to 40 in 35: 5 px.
    assert!(requesters.borrow()[4].request_focus());
    ui.wait_for_idle();
    assert_eq!(main_offset(&state, lazy, 5), 25);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutPinnableItem.kt, the pinned stay composed and placed out of view; back in view it's placed where it is, once
#[test]
fn a_pinned_item_scrolled_back_is_placed_once_where_it_is() {
    use_optimizations(Optimizations::NONE);
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(1),
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = handle,
            |scope: LazyGridScope| {
                scope.items(100, |index| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 50.dp()))
                            .focusable()
                            .test_tag(index.to_string())
                    );
                });
            }
        );
    });
    ui.on_node_with_tag("1").request_focus();
    let _ = run(&mut ui, async move { handle.scroll_to_item(40, 0).await });
    ui.wait_for_idle();
    // Pinned, it's placed out of view.
    let bounds = ui.on_node_with_tag("1").get_unclipped_bounds_in_root();
    assert!(bounds.bottom <= 0.dp(), "{bounds:?}");
    let _ = run(&mut ui, async move { handle.scroll_to_item(1, 0).await });
    ui.wait_for_idle();
    ui.on_node_with_tag("1").assert_top_position_in_root_is_equal_to(0.dp());
    ui.on_node_with_tag("2").assert_top_position_in_root_is_equal_to(50.dp());
}
