//! Grids' sticky headers, as Compose's `LazyGridHeadersTest`, in both orientations. Density 1.
//! Left: `lazyVerticalGridwithPinnedItem` (feature:pinnable) and the three focus tests, which pin
//! each item (feature:pinnable), as the list's.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Dp, IntSize, UnitExt};

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
