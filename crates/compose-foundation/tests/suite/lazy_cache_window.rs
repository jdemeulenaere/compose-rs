//! The cache window, as Compose's `LazyListCacheWindowTest`, `LazyGridCacheWindowTest`, their
//! parity tests (a window set to behave like the old one-item strategy) and the precedence tests.
//! Density 1: Compose's tests' pixels are dps. A unit is an item, or a grid's line of two.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Density, Dp, IntSize, UnitExt};

use crate::scroll_support::run;

const ITEM: f32 = 30.0;

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// Lets frames and prefetching finish, as Compose's rule does between its steps.
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

/// What the items' composition saw: the items active, and how many times each was activated.
#[derive(Clone, Default)]
struct Seen {
    live: Rc<RefCell<BTreeSet<usize>>>,
    activated: Rc<RefCell<HashMap<usize, usize>>>,
}

impl Seen {
    fn exists(&self, item: usize) -> bool {
        self.live.borrow().contains(&item)
    }
}

#[composable]
fn Item(index: usize, seen: Rc<Seen>, vertical: bool) {
    let (live, activated) = (seen.live.clone(), seen.activated.clone());
    DisposableEffect(index, move |scope| {
        live.borrow_mut().insert(index);
        *activated.borrow_mut().entry(index).or_default() += 1;
        let live = live.clone();
        scope.on_dispose(move || {
            live.borrow_mut().remove(&index);
        })
    });
    let modifier = if vertical {
        Modifier.size((30.dp(), Dp(ITEM)))
    } else {
        Modifier.size((Dp(ITEM), 30.dp()))
    };
    Spacer(modifier.test_tag(format!("{index}")));
}

fn main_axis(vertical: bool, length: f32) -> Modifier {
    if vertical {
        Modifier.size((100.dp(), Dp(length)))
    } else {
        Modifier.size((Dp(length), 100.dp()))
    }
}

/// What's composed of a list: its rule, its state and what its items saw.
struct List {
    ui: ComposeTestRule,
    state: Owned<LazyListState>,
    seen: Seen,
}

#[derive(Clone)]
struct ListArgs {
    vertical: bool,
    first: usize,
    offset: i32,
    reverse: bool,
    padding: PaddingValues,
    count: Option<MutableState<usize>>,
    items_in_view: f32,
    window: Option<Rc<dyn LazyLayoutCacheWindow>>,
}

impl ListArgs {
    fn new(vertical: bool, window: Rc<dyn LazyLayoutCacheWindow>) -> ListArgs {
        ListArgs {
            vertical,
            first: 0,
            offset: 0,
            reverse: false,
            padding: PaddingValues::ZERO,
            count: None,
            items_in_view: 1.5,
            window: Some(window),
        }
    }
}

fn compose_list(args: ListArgs) -> List {
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(args.first, args.offset);
    let given = *state;
    let seen = Seen::default();
    let (items_seen, vertical) = (seen.clone(), args.vertical);
    let ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let count = args.count.map_or(100, |count| count.get());
        let modifier = main_axis(args.vertical, ITEM * args.items_in_view);
        let (padding, reverse, window) = (args.padding, args.reverse, args.window.clone());
        let seen = items_seen.clone();
        let content = move |scope: LazyListScope| {
            let seen = seen.clone();
            let seen = Rc::new(seen.clone());
            scope.items(count, move |index| Item(index, seen.clone(), vertical));
        };
        if vertical {
            LazyColumn!(
                modifier = modifier,
                state = given,
                content_padding = padding,
                reverse_layout = reverse,
                cache_window = window,
                content = content
            );
        } else {
            LazyRow!(
                modifier = modifier,
                state = given,
                content_padding = padding,
                reverse_layout = reverse,
                cache_window = window,
                content = content
            );
        }
    });
    let mut list = List { ui, state, seen };
    idle(&mut list.ui);
    list
}

impl List {
    fn scroll_by(&mut self, pixels: f32) {
        let state = *self.state;
        let _ = run(&mut self.ui, async move { state.scroll_by(pixels).await });
        idle(&mut self.ui);
    }

    fn displayed(&mut self, item: usize) -> bool {
        self.ui.on_node_with_tag(&format!("{item}")).is_displayed()
    }
}

fn viewport_window() -> Rc<dyn LazyLayoutCacheWindow> {
    lazy_layout_cache_window_fraction(1.0, 1.0)
}

const ORIENTATIONS: [bool; 2] = [true, false];

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, prefetchingForwardInitially
#[test]
fn a_window_fills_ahead_initially() {
    for vertical in ORIENTATIONS {
        let list = compose_list(ListArgs::new(vertical, viewport_window()));
        assert!(list.seen.exists(2), "the window has room for one more");
        assert!(!list.seen.exists(3));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, notPrefetchingBackwardInitially
#[test]
fn a_window_doesnt_fill_behind_initially() {
    for vertical in ORIENTATIONS {
        let list =
            compose_list(ListArgs { first: 2, ..ListArgs::new(vertical, viewport_window()) });
        assert!(!list.seen.exists(0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, smallScrollForward_shouldFillEntireWindow
#[test]
fn a_small_scroll_fills_the_whole_window() {
    for vertical in ORIENTATIONS {
        let mut list = compose_list(ListArgs::new(vertical, viewport_window()));
        list.scroll_by(5.0);
        assert!(list.seen.exists(2) && list.seen.exists(3), "{:?}", list.seen.live);
        assert!(!list.seen.exists(4));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, smallScrollBackwardShouldFillEntireWindow
#[test]
fn a_small_scroll_back_fills_the_whole_window() {
    for vertical in ORIENTATIONS {
        let mut list = compose_list(ListArgs {
            first: 5,
            offset: 10,
            ..ListArgs::new(vertical, viewport_window())
        });
        list.scroll_by(-5.0);
        assert!(list.seen.exists(4) && list.seen.exists(3), "{:?}", list.seen.live);
        assert!(!list.seen.exists(2), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, scrollForward_prefetchedItemBecomingVisible_doesNotResetCacheWindow
#[test]
fn an_item_prefetched_becoming_visible_doesnt_reset_the_window() {
    for vertical in ORIENTATIONS {
        let window = lazy_layout_cache_window_fraction_with(1.0, 1.0, false);
        let mut list = compose_list(ListArgs::new(vertical, window));
        list.scroll_by(5.0);
        assert!(list.seen.exists(2) && list.seen.exists(3));
        assert!(!list.seen.exists(4));
        for item in 2..=40 {
            list.scroll_by(ITEM);
            assert!(list.displayed(item), "{item}");
            assert!(list.seen.exists(item + 1) && list.seen.exists(item + 2), "{item}");
            assert!(!list.seen.exists(item + 3), "{item}: {:?}", list.seen.live);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, scrollBackward_prefetchedItemBecomingVisible_doesNotResetCacheWindow
#[test]
fn an_item_prefetched_becoming_visible_backward_doesnt_reset_the_window() {
    for vertical in ORIENTATIONS {
        let window = lazy_layout_cache_window_fraction_with(1.0, 1.0, false);
        let mut list = compose_list(ListArgs {
            first: 99,
            offset: 15,
            count: Some(mutable_state_of(120).handle()),
            ..ListArgs::new(vertical, window)
        });
        list.scroll_by(-5.0);
        assert!(list.seen.exists(98) && list.seen.exists(97), "{:?}", list.seen.live);
        assert!(!list.seen.exists(96));
        for item in (60..=98).rev() {
            list.scroll_by(-ITEM);
            assert!(list.displayed(item), "{item}");
            assert!(list.seen.exists(item - 1) && list.seen.exists(item - 2), "{item}");
            assert!(!list.seen.exists(item - 3), "{item}: {:?}", list.seen.live);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, scrollForward_prefetchedItemBecomingVisible_doesNotRecomposePrefetchedItems
#[test]
fn an_item_prefetched_is_composed_once() {
    for vertical in ORIENTATIONS {
        let window = lazy_layout_cache_window_fraction_with(1.0, 0.0, false);
        let mut list = compose_list(ListArgs::new(vertical, window));
        for _ in 0..60 {
            list.scroll_by(ITEM / 3.0);
        }
        assert!(list.state.first_visible_item_index() >= 19);
        for (item, count) in list.seen.activated.borrow().iter() {
            assert_eq!(*count, 1, "item {item}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, scrollBackward_prefetchedItemBecomingVisible_doesNotRecomposePrefetchedItems
#[test]
fn an_item_prefetched_backward_is_composed_once() {
    for vertical in ORIENTATIONS {
        let window = lazy_layout_cache_window_fraction_with(1.0, 0.0, false);
        let mut list = compose_list(ListArgs { first: 50, ..ListArgs::new(vertical, window) });
        for _ in 0..60 {
            list.scroll_by(-ITEM / 3.0);
        }
        assert!(list.state.first_visible_item_index() <= 31);
        for (item, count) in list.seen.activated.borrow().iter() {
            assert_eq!(*count, 1, "item {item}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, scrollForward_shouldNotDisposeItemsInWindow
#[test]
fn a_scroll_forward_keeps_the_items_in_the_window() {
    for vertical in ORIENTATIONS {
        let mut list =
            compose_list(ListArgs { first: 3, ..ListArgs::new(vertical, viewport_window()) });
        list.scroll_by(ITEM * 2.5);
        assert_eq!(list.state.first_visible_item_index(), 5);
        // As after Compose's next measurement; here it has gone with the prefetching: item
        // 5's half out leaves the 1.5 items' window room for item 4 only. Compose's item 3 stays
        // until then, as the window it composes by is the one before the scroll (`keepAroundItems`),
        // and its test ends there; its backward one forces that measurement (D892).
        assert!(!list.seen.exists(3), "{:?}", list.seen.live);
        assert!(list.seen.exists(4), "{:?}", list.seen.live);
        assert!(list.displayed(5) && list.displayed(6));
        assert!(list.seen.exists(7) && list.seen.exists(8), "{:?}", list.seen.live);
        assert!(!list.seen.exists(9), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, scrollBackward_shouldNotDisposeItemsInWindow
#[test]
fn a_scroll_backward_keeps_the_items_in_the_window() {
    for vertical in ORIENTATIONS {
        let mut list = compose_list(ListArgs {
            first: 6,
            offset: -15,
            ..ListArgs::new(vertical, viewport_window())
        });
        assert!(list.displayed(5) && list.displayed(6));
        assert!(list.seen.exists(7) && list.seen.exists(8), "{:?}", list.seen.live);
        list.scroll_by(-ITEM * 2.5);
        assert_eq!(list.state.first_visible_item_index(), 3);
        // What's left of the window is 1.5 items each way, item 4's half out of it.
        assert!(!list.seen.exists(0), "{:?}", list.seen.live);
        assert!(list.seen.exists(1) && list.seen.exists(2), "{:?}", list.seen.live);
        assert!(list.displayed(3) && list.displayed(4));
        assert!(list.seen.exists(5), "{:?}", list.seen.live);
        for gone in 6..=8 {
            assert!(!list.seen.exists(gone), "{gone}: {:?}", list.seen.live);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, datasetChanged_shouldScheduleNewPrefetching_ifWindowIsNotFull
#[test]
fn more_items_fill_a_window_with_room() {
    for vertical in ORIENTATIONS {
        let count = mutable_state_of(100usize);
        let mut list = compose_list(ListArgs {
            first: 97,
            count: Some(count.handle()),
            ..ListArgs::new(vertical, viewport_window())
        });
        assert!(list.displayed(97) && list.displayed(98));
        assert!(list.seen.exists(99));
        list.scroll_by(ITEM);
        assert!(list.displayed(98) && list.displayed(99));
        assert!(!list.seen.exists(100));
        count.set(200);
        idle(&mut list.ui);
        assert!(list.seen.exists(100), "the window has room: {:?}", list.seen.live);
    }
}

type Tags = Rc<RefCell<BTreeSet<String>>>;

/// Notes `tag` while it's composed: what's prefetched isn't placed, so not in the semantics tree.
#[composable]
fn Noted(tag: String, tags: Tags) {
    DisposableEffect(tag.clone(), move |scope| {
        tags.borrow_mut().insert(tag.clone());
        let (tags, tag) = (tags.clone(), tag.clone());
        scope.on_dispose(move || {
            tags.borrow_mut().remove(&tag);
        })
    });
}

#[composable]
fn Tagged(tag: String, tags: Tags, vertical: bool) {
    Noted(tag.clone(), tags);
    Spacer(main_axis(vertical, ITEM).test_tag(tag));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/CacheWindowLogic.kt, onVisibleItemsUpdated: a refill fills only a window with non-scroll caching, and resets the other's bounds
#[test]
fn more_items_at_rest_fill_only_a_window_caching_at_rest() {
    for vertical in ORIENTATIONS {
        for (caching, filled) in [(true, true), (false, false)] {
            let window = lazy_layout_cache_window_fraction_with(1.0, 1.0, caching);
            let count = mutable_state_of(2usize);
            let mut list = compose_list(ListArgs {
                count: Some(count.handle()),
                ..ListArgs::new(vertical, window)
            });
            assert!(list.displayed(0) && list.displayed(1));
            count.set(100);
            idle(&mut list.ui);
            assert_eq!(list.seen.exists(2), filled, "{caching}: {:?}", list.seen.live);
        }
    }
}

/// A list of items by key, one of them a nested row with two cells.
fn keyed_list(
    vertical: bool,
    items: MutableState<Vec<&'static str>>,
    nested: &'static [&'static str],
    in_view: f32,
) -> (ComposeTestRule, Owned<LazyListState>, Tags) {
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let tags: Tags = Rc::default();
    let all = tags.clone();
    let ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let items = items.get();
        let modifier = main_axis(vertical, ITEM * in_view);
        let tags = all.clone();
        let content = move |scope: LazyListScope| {
            let items = items.clone();
            let keys = items.clone();
            let tags = tags.clone();
            scope.items_keyed(
                items.len(),
                move |index| keys[index],
                move |index| {
                    let item = items[index];
                    let tags = tags.clone();
                    if nested.contains(&item) {
                        Noted(item.to_string(), tags.clone());
                        let inner = remember_lazy_list_state(0, 0);
                        LazyRow!(
                            modifier = main_axis(vertical, ITEM).test_tag(item),
                            state = inner,
                            cache_window = Some(viewport_window()),
                            move |scope: LazyListScope| {
                                let tags = tags.clone();
                                let other = tags.clone();
                                scope.item(move || {
                                    Tagged(format!("first-nested-{item}"), tags.clone(), vertical)
                                });
                                scope.item(move || {
                                    Tagged(format!("second-nested-{item}"), other.clone(), vertical)
                                });
                            }
                        );
                    } else {
                        Tagged(item.to_string(), tags, vertical);
                    }
                },
            );
        };
        if vertical {
            LazyColumn!(
                modifier = modifier,
                state = given,
                cache_window = Some(viewport_window()),
                content = content
            );
        } else {
            LazyRow!(
                modifier = modifier,
                state = given,
                cache_window = Some(viewport_window()),
                content = content
            );
        }
    });
    (ui, state, tags)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, datasetChanged_shouldMakeSureNestedItemsChanged_afterScroll
#[test]
fn another_dataset_prefetches_the_new_items_nested_lists_after_a_scroll() {
    for vertical in ORIENTATIONS {
        let items = mutable_state_of(vec!["a", "b", "c", "d", "e"]);
        let (mut ui, state, tags) = keyed_list(vertical, items.handle(), &["e"], 1.5);
        let has = |tag: &str| tags.borrow().contains(tag);
        idle(&mut ui);
        assert!(ui.on_node_with_tag("a").is_displayed() && ui.on_node_with_tag("b").is_displayed());
        let handle = *state;
        let _ = run(&mut ui, async move { handle.scroll_by(ITEM).await });
        idle(&mut ui);
        assert!(ui.on_node_with_tag("b").is_displayed() && ui.on_node_with_tag("c").is_displayed());
        assert!(has("d"));
        assert!(!has("e"));
        items.set(vec!["a", "b", "c", "e", "f", "g", "h"]);
        idle(&mut ui);
        assert!(has("e"), "in place of d");
        assert!(has("first-nested-e") && has("second-nested-e"));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, datasetChanged_shouldMakeSureNestedItemsChanged_noScroll
#[test]
fn another_dataset_prefetches_the_new_items_nested_lists_without_a_scroll() {
    for vertical in ORIENTATIONS {
        let items = mutable_state_of(vec!["a", "b", "c", "d"]);
        let (mut ui, _state, tags) = keyed_list(vertical, items.handle(), &["e", "f"], 2.0);
        let has = |tag: &str| tags.borrow().contains(tag);
        idle(&mut ui);
        assert!(ui.on_node_with_tag("a").is_displayed() && ui.on_node_with_tag("b").is_displayed());
        assert!(has("c") && has("d"));
        items.set(vec!["a", "b", "e", "f", "g", "h"]);
        idle(&mut ui);
        for tag in
            ["e", "first-nested-e", "second-nested-e", "f", "first-nested-f", "second-nested-f"]
        {
            assert!(has(tag), "{tag}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, datasetChanged_noScrollHappened_shouldKeepAroundWithinBounds_notCrash
#[test]
fn fewer_items_without_a_scroll_keep_within_the_bounds() {
    for vertical in ORIENTATIONS {
        let count = mutable_state_of(100usize);
        let mut list = compose_list(ListArgs {
            first: 96,
            count: Some(count.handle()),
            ..ListArgs::new(vertical, viewport_window())
        });
        assert!(list.displayed(96) && list.displayed(97));
        count.set(50);
        idle(&mut list.ui);
        assert!(list.seen.exists(49), "{:?}", list.seen.live);
        assert!(!list.seen.exists(96) && !list.seen.exists(97));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, datasetChanged_scrollBack_shouldKeepAroundWithinBounds_notCrash
#[test]
fn fewer_items_then_a_scroll_back_keep_within_the_bounds() {
    for vertical in ORIENTATIONS {
        let count = mutable_state_of(40usize);
        let mut list = compose_list(ListArgs {
            count: Some(count.handle()),
            items_in_view: 20.0,
            ..ListArgs::new(vertical, viewport_window())
        });
        list.scroll_by(4.0 * ITEM);
        assert_ne!(list.state.first_visible_item_index(), 0);
        count.set(33);
        idle(&mut list.ui);
        list.scroll_by(-4.0 * ITEM);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, datasetChanged_scrollForward_shouldKeepAroundWithinBounds_notCrash
#[test]
fn fewer_items_then_a_scroll_forward_keep_within_the_bounds() {
    for vertical in ORIENTATIONS {
        let count = mutable_state_of(40usize);
        let mut list = compose_list(ListArgs {
            first: 20,
            count: Some(count.handle()),
            items_in_view: 5.0,
            ..ListArgs::new(vertical, viewport_window())
        });
        list.scroll_by(-4.0 * ITEM);
        assert_ne!(list.state.first_visible_item_index(), 0);
        count.set(33);
        idle(&mut list.ui);
        list.scroll_by(4.0 * ITEM);
        count.set(20);
        idle(&mut list.ui);
        list.scroll_by(4.0 * ITEM);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowTest.kt, scrollForward_longScroll_shouldOnlyPrefetchItemInWindow
#[test]
fn a_long_scroll_prefetches_only_what_the_window_holds() {
    for vertical in ORIENTATIONS {
        let mut list = compose_list(ListArgs::new(vertical, viewport_window()));
        assert!(list.displayed(0) && list.displayed(1));
        assert!(list.seen.exists(2) && !list.seen.exists(3));
        let state = *list.state;
        let _ = run(&mut list.ui, async move { state.animate_scroll_to_item(20, 0).await });
        idle(&mut list.ui);
        assert!(list.displayed(20) && list.displayed(21), "{:?}", list.seen.live);
        assert!(list.seen.exists(22) && !list.seen.exists(23), "{:?}", list.seen.live);
    }
}

// ---- Precedence --------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListPrefetchPrecedenceTest.kt, usesDefaultLayoutCacheWindow
#[test]
fn a_list_uses_the_default_window_by_default() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let _ui = ComposeTestRule::sized(IntSize::new(100, 150), move || {
        LazyColumn!(modifier = Modifier.height(150.dp()), state = given, |scope: LazyListScope| {
            scope.items(10, |_| Spacer(Modifier.height(100.dp())));
        });
    });
    let default = LazyListDefaults::cache_window(given);
    let used = given.cache_window().expect("composed");
    assert_eq!(used.value(), default.value());
    assert!(!used.is_non_scroll_caching_enabled());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListPrefetchPrecedenceTest.kt, providedCacheWindowIsUsedWhenNoStateStrategy
#[test]
fn a_list_uses_the_window_given() {
    let window = lazy_layout_cache_window(0.dp(), 0.dp());
    let state = LazyListState::new(0, 0);
    let (given, kept) = (*state, window.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(100, 150), move || {
        LazyColumn!(
            modifier = Modifier.height(150.dp()),
            state = given,
            cache_window = Some(kept.clone()),
            |scope: LazyListScope| {
                scope.items(10, |_| Spacer(Modifier.height(100.dp())));
            }
        );
    });
    assert!(Rc::ptr_eq(&given.cache_window().expect("composed"), &window));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridPrefetchPrecedenceTest.kt, usesDefaultLayoutCacheWindow
#[test]
fn a_grid_uses_the_default_window_by_default() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let _ui = ComposeTestRule::sized(IntSize::new(100, 150), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.height(150.dp()),
            state = given,
            |scope: LazyGridScope| {
                scope.items(10, |_| Spacer(Modifier.height(100.dp())));
            }
        );
    });
    let default = LazyGridDefaults::cache_window(given);
    assert_eq!(given.cache_window().expect("composed").value(), default.value());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridPrefetchPrecedenceTest.kt, providedCacheWindowIsUsedWhenNoStateStrategy
#[test]
fn a_grid_uses_the_window_given() {
    let window = lazy_layout_cache_window(0.dp(), 0.dp());
    let state = LazyGridState::new(0, 0);
    let (given, kept) = (*state, window.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(100, 150), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.height(150.dp()),
            state = given,
            cache_window = Some(kept.clone()),
            |scope: LazyGridScope| {
                scope.items(10, |_| Spacer(Modifier.height(100.dp())));
            }
        );
    });
    assert!(Rc::ptr_eq(&given.cache_window().expect("composed"), &window));
}

// ---- The default window ---------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutCacheWindow.kt, DynamicLazyLayoutCacheWindow: the items' average size and spacing, in 10 to 50 % of the viewport, nothing behind, no filling at rest
#[test]
fn the_default_window_follows_the_items_size() {
    for (item, count_ahead, spacing) in
        [(10.0, 3, 0.0), (30.0, 1, 0.0), (45.0, 1, 5.0), (150.0, 1, 0.0)]
    {
        use_optimizations(Optimizations::NONE);
        let state = LazyListState::new(0, 0);
        let given = *state;
        let live: Rc<RefCell<BTreeSet<usize>>> = Rc::default();
        let seen = live.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(100, 300), move || {
            let seen = seen.clone();
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                state = given,
                vertical_arrangement = Arrangement::spaced_by(Dp(spacing)),
                move |scope: LazyListScope| {
                    let seen = seen.clone();
                    scope.items(100, move |index| {
                        let seen = seen.clone();
                        DisposableEffect(index, move |scope| {
                            seen.borrow_mut().insert(index);
                            let seen = seen.clone();
                            scope.on_dispose(move || {
                                seen.borrow_mut().remove(&index);
                            })
                        });
                        Spacer(Modifier.size((100.dp(), Dp(item))));
                    });
                }
            );
        });
        idle(&mut ui);
        let at_rest = live.borrow().clone();
        // Far enough that the last item's overflow leaves the window room, whatever the item.
        let period = item + spacing;
        let delta = (period - (100.0 % period)).max(1.0) + 1.0;
        given.dispatch_raw_delta(delta);
        idle(&mut ui);
        let scrolled = live.borrow().clone();
        let first = given.first_visible_item_index();
        let in_view = ((100.0 + delta) / period).ceil() as usize;
        let ahead = scrolled.last().copied().unwrap_or(0) + 1 - in_view.max(first);
        assert!(!at_rest.is_empty());
        assert!(ahead <= count_ahead + 1, "{item}: {ahead} ahead, {scrolled:?}");
    }
}

// The window's numbers, by themselves.
#[test]
fn the_factories_windows_are_what_they_say() {
    let density = Density::new(2.0);
    let dp = lazy_layout_cache_window(10.dp(), 20.dp());
    assert_eq!(dp.calculate_ahead_window(density, 100), 20);
    assert_eq!(dp.calculate_behind_window(density, 100), 40);
    assert!(dp.is_non_scroll_caching_enabled());
    let fraction = lazy_layout_cache_window_fraction_with(0.5, 0.25, false);
    assert_eq!(fraction.calculate_ahead_window(density, 100), 50);
    assert_eq!(fraction.calculate_behind_window(density, 100), 25);
    assert!(!fraction.is_non_scroll_caching_enabled());
}

// ---- Parity: a window set up like the old strategy -------------------------------------------------

/// Compose's tests' `ParityWindow`: set after the first layout, no filling but on a scroll's.
struct ParityWindow {
    ahead: Cell<i32>,
}

impl LazyLayoutCacheWindow for ParityWindow {
    fn calculate_ahead_window(&self, _: Density, _: i32) -> i32 {
        self.ahead.get()
    }
}

fn parity_list(args: ListArgs) -> (List, Rc<ParityWindow>) {
    let parity = Rc::new(ParityWindow { ahead: Cell::new(0) });
    let list = compose_list(ListArgs { window: Some(parity.clone()), ..args });
    parity.ahead.set(ITEM as i32);
    (list, parity)
}

fn parity_args(vertical: bool) -> ListArgs {
    ListArgs::new(vertical, lazy_layout_cache_window(0.dp(), 0.dp()))
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, notPrefetchingForwardInitially
#[test]
fn parity_not_prefetching_forward_initially() {
    for vertical in ORIENTATIONS {
        let list = compose_list(parity_args(vertical));
        assert!(!list.seen.exists(2));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, notPrefetchingBackwardInitially
#[test]
fn parity_not_prefetching_backward_initially() {
    for vertical in ORIENTATIONS {
        let (list, _) = parity_list(ListArgs { first: 2, ..parity_args(vertical) });
        assert!(!list.seen.exists(0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingForwardAfterSmallScroll
#[test]
fn parity_prefetching_forward_after_a_small_scroll() {
    for vertical in ORIENTATIONS {
        let (mut list, _) = parity_list(parity_args(vertical));
        list.scroll_by(5.0);
        assert!(list.seen.exists(2));
        assert!(!list.seen.exists(3), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingBackwardAfterSmallScroll
#[test]
fn parity_prefetching_backward_after_a_small_scroll() {
    for vertical in ORIENTATIONS {
        let (mut list, _) = parity_list(ListArgs { first: 2, offset: 10, ..parity_args(vertical) });
        list.scroll_by(-5.0);
        assert!(list.seen.exists(1));
        assert!(!list.seen.exists(0), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingForwardAndBackward
#[test]
fn parity_prefetching_forward_and_backward() {
    for vertical in ORIENTATIONS {
        let (mut list, _) = parity_list(ListArgs { first: 5, ..parity_args(vertical) });
        list.scroll_by(5.0);
        assert!(list.seen.exists(7));
        assert!(!list.seen.exists(4), "{:?}", list.seen.live);
        list.scroll_by(-2.0);
        list.scroll_by(-1.0);
        assert!(list.seen.exists(4), "{:?}", list.seen.live);
        assert!(!list.seen.exists(7), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingForwardTwice
#[test]
fn parity_prefetching_forward_twice() {
    for vertical in ORIENTATIONS {
        let (mut list, _) = parity_list(parity_args(vertical));
        list.scroll_by(5.0);
        list.scroll_by(ITEM / 2.0);
        list.scroll_by(ITEM / 2.0);
        assert!(list.displayed(2));
        assert!(list.seen.exists(3));
        assert!(!list.seen.exists(4), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingBackwardTwice
#[test]
fn parity_prefetching_backward_twice() {
    for vertical in ORIENTATIONS {
        let (mut list, _) = parity_list(ListArgs { first: 4, ..parity_args(vertical) });
        list.scroll_by(-5.0);
        list.scroll_by(-ITEM / 2.0);
        list.scroll_by(-ITEM / 2.0);
        assert!(list.displayed(2));
        assert!(list.seen.exists(1));
        assert!(!list.seen.exists(0), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingForwardAndBackwardReverseLayout
#[test]
fn parity_prefetching_forward_and_backward_reversed() {
    for vertical in ORIENTATIONS {
        let (mut list, _) =
            parity_list(ListArgs { first: 5, reverse: true, ..parity_args(vertical) });
        list.scroll_by(5.0);
        assert!(list.seen.exists(7));
        assert!(!list.seen.exists(4), "{:?}", list.seen.live);
        list.scroll_by(-2.0);
        list.scroll_by(-1.0);
        assert!(list.seen.exists(4), "{:?}", list.seen.live);
        assert!(!list.seen.exists(7), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingForwardAndBackwardWithContentPadding
#[test]
fn parity_prefetching_forward_and_backward_with_content_padding() {
    for vertical in ORIENTATIONS {
        let half = Dp(ITEM / 2.0);
        let padding = if vertical {
            PaddingValues::symmetric(0.dp(), half)
        } else {
            PaddingValues::symmetric(half, 0.dp())
        };
        let (mut list, _) =
            parity_list(ListArgs { first: 5, offset: 5, padding, ..parity_args(vertical) });
        assert!(list.displayed(4) && list.displayed(5) && list.displayed(6));
        assert!(!list.seen.exists(3) && !list.seen.exists(7), "{:?}", list.seen.live);
        list.scroll_by(5.0);
        assert!(list.seen.exists(7), "{:?}", list.seen.live);
        assert!(!list.seen.exists(3), "{:?}", list.seen.live);
        list.scroll_by(-10.0);
        assert!(list.seen.exists(3), "{:?}", list.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, prefetchingStickyHeaderItem
#[test]
fn parity_prefetching_with_a_sticky_header() {
    for vertical in ORIENTATIONS {
        use_optimizations(Optimizations::NONE);
        let state = LazyListState::new(1, 15);
        let given = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            let modifier = main_axis(vertical, ITEM * 1.5);
            let content = move |scope: LazyListScope| {
                scope.sticky_header(move |_| {
                    Spacer(main_axis(vertical, ITEM).test_tag("header"));
                });
                scope.items(100, move |index| {
                    Spacer(main_axis(vertical, ITEM).test_tag(format!("{index}")));
                });
            };
            if vertical {
                LazyColumn!(modifier = modifier, state = given, content = content);
            } else {
                LazyRow!(modifier = modifier, state = given, content = content);
            }
        });
        idle(&mut ui);
        given.dispatch_raw_delta(-5.0);
        idle(&mut ui);
        assert!(ui.on_node_with_tag("header").is_displayed());
        assert!(ui.on_node_with_tag("0").is_displayed());
        assert!(ui.on_node_with_tag("1").is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, disposingWhilePrefetchingScheduled
#[test]
fn parity_disposing_while_prefetching_is_scheduled() {
    for vertical in ORIENTATIONS {
        use_optimizations(Optimizations::NONE);
        let state = LazyListState::new(0, 0);
        let given = *state;
        let emit = mutable_state_of(true);
        let shown = emit.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            if shown.get() {
                let modifier = main_axis(vertical, ITEM * 1.5);
                let content = move |scope: LazyListScope| {
                    scope.items(1000, move |_| Spacer(main_axis(vertical, ITEM)));
                };
                if vertical {
                    LazyColumn!(modifier = modifier, state = given, content = content);
                } else {
                    LazyRow!(modifier = modifier, state = given, content = content);
                }
            }
        });
        idle(&mut ui);
        given.dispatch_raw_delta(ITEM);
        ui.wait_for_idle();
        // The request is queued, and the list goes.
        emit.set(false);
        idle(&mut ui);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, snappingToOtherPositionWhilePrefetchIsScheduled
#[test]
fn parity_snapping_elsewhere_cancels_what_was_scheduled() {
    for vertical in ORIENTATIONS {
        let (mut list, _) = parity_list(parity_args(vertical));
        let state = *list.state;
        state.dispatch_raw_delta(ITEM);
        list.ui.wait_for_idle();
        let _ = run(&mut list.ui, async move { state.scroll_to_item(100, 0).await });
        idle(&mut list.ui);
        assert!(!list.seen.activated.borrow().contains_key(&3), "{:?}", list.seen.activated);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListCacheWindowParityTest.kt, scrollingByListSizeCancelsPreviousPrefetch
#[test]
fn parity_scrolling_by_the_lists_size_cancels_what_was_scheduled() {
    for vertical in ORIENTATIONS {
        let (mut list, _) = parity_list(parity_args(vertical));
        let state = *list.state;
        state.dispatch_raw_delta(ITEM);
        list.ui.wait_for_idle();
        state.dispatch_raw_delta(ITEM * 3.0);
        list.ui.wait_for_idle();
        state.dispatch_raw_delta(ITEM);
        list.ui.wait_for_idle();
        idle(&mut list.ui);
        list.scroll_by(ITEM * 100.0);
        assert!(!list.seen.exists(3), "{:?}", list.seen.live);
    }
}

// ---- Grids ------------------------------------------------------------------------------------------

struct Grid {
    ui: ComposeTestRule,
    state: Owned<LazyGridState>,
    seen: Seen,
}

#[derive(Clone)]
struct GridArgs {
    vertical: bool,
    first: usize,
    offset: i32,
    padding: PaddingValues,
    count: Option<MutableState<usize>>,
    window: Rc<dyn LazyLayoutCacheWindow>,
}

impl GridArgs {
    fn new(vertical: bool, window: Rc<dyn LazyLayoutCacheWindow>) -> GridArgs {
        GridArgs {
            vertical,
            first: 0,
            offset: 0,
            padding: PaddingValues::ZERO,
            count: None,
            window,
        }
    }
}

fn compose_grid(args: GridArgs) -> Grid {
    use_optimizations(Optimizations::NONE);
    let state = LazyGridState::new(args.first, args.offset);
    let given = *state;
    let seen = Seen::default();
    let (items_seen, vertical) = (seen.clone(), args.vertical);
    let ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let count = args.count.map_or(100, |count| count.get());
        let modifier = main_axis(vertical, ITEM * 1.5);
        let (padding, window) = (args.padding, Some(args.window.clone()));
        let seen = items_seen.clone();
        let content = move |scope: LazyGridScope| {
            let seen = seen.clone();
            let seen = Rc::new(seen.clone());
            scope.items(count, move |index| Item(index, seen.clone(), vertical));
        };
        if vertical {
            LazyVerticalGrid!(
                GridCells::Fixed(2),
                modifier = modifier,
                state = given,
                content_padding = padding,
                cache_window = window,
                content = content
            );
        } else {
            LazyHorizontalGrid!(
                GridCells::Fixed(2),
                modifier = modifier,
                state = given,
                content_padding = padding,
                cache_window = window,
                content = content
            );
        }
    });
    let mut grid = Grid { ui, state, seen };
    idle(&mut grid.ui);
    grid
}

impl Grid {
    fn scroll_by(&mut self, pixels: f32) {
        let state = *self.state;
        let _ = run(&mut self.ui, async move { state.scroll_by(pixels).await });
        idle(&mut self.ui);
    }

    fn displayed(&mut self, item: usize) -> bool {
        self.ui.on_node_with_tag(&format!("{item}")).is_displayed()
    }

    fn all_exist(&self, items: std::ops::Range<usize>) -> bool {
        items.into_iter().all(|item| self.seen.exists(item))
    }

    fn none_exist(&self, items: std::ops::Range<usize>) -> bool {
        items.into_iter().all(|item| !self.seen.exists(item))
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, prefetchingForwardInitially
#[test]
fn a_grids_window_fills_ahead_initially() {
    for vertical in ORIENTATIONS {
        let grid = compose_grid(GridArgs::new(vertical, viewport_window()));
        assert!(grid.all_exist(4..6), "{:?}", grid.seen.live);
        assert!(grid.none_exist(6..8), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, notPrefetchingBackwardInitially
#[test]
fn a_grids_window_doesnt_fill_behind_initially() {
    for vertical in ORIENTATIONS {
        let grid =
            compose_grid(GridArgs { first: 3, ..GridArgs::new(vertical, viewport_window()) });
        assert!(!grid.seen.exists(0), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, smallScrollForward_shouldFillEntireWindow
#[test]
fn a_small_scroll_fills_a_grids_whole_window() {
    for vertical in ORIENTATIONS {
        let mut grid = compose_grid(GridArgs::new(vertical, viewport_window()));
        grid.scroll_by(5.0);
        assert!(grid.all_exist(4..8), "{:?}", grid.seen.live);
        assert!(grid.none_exist(8..10), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, smallScrollBackwardShouldFillEntireWindow
#[test]
fn a_small_scroll_back_fills_a_grids_whole_window() {
    for vertical in ORIENTATIONS {
        let mut grid = compose_grid(GridArgs {
            first: 6,
            offset: 10,
            ..GridArgs::new(vertical, viewport_window())
        });
        grid.scroll_by(-5.0);
        assert!(grid.all_exist(2..6), "{:?}", grid.seen.live);
        assert!(grid.none_exist(0..2), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, scrollForward_shouldNotDisposeItemsInWindow
#[test]
fn a_scroll_forward_keeps_a_grids_items_in_the_window() {
    for vertical in ORIENTATIONS {
        let mut grid =
            compose_grid(GridArgs { first: 6, ..GridArgs::new(vertical, viewport_window()) });
        grid.scroll_by(ITEM * 2.5);
        assert_eq!(grid.state.first_visible_item_index(), 10);
        // As after Compose's next measurement (see the list's).
        assert!(grid.none_exist(6..8), "{:?}", grid.seen.live);
        assert!(grid.all_exist(8..10), "{:?}", grid.seen.live);
        assert!((10..14).all(|item| grid.displayed(item)));
        assert!(grid.all_exist(14..18), "{:?}", grid.seen.live);
        assert!(grid.none_exist(18..20), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, scrollBackward_shouldNotDisposeItemsInWindow
#[test]
fn a_scroll_backward_keeps_a_grids_items_in_the_window() {
    for vertical in ORIENTATIONS {
        let mut grid = compose_grid(GridArgs {
            first: 12,
            offset: -15,
            ..GridArgs::new(vertical, viewport_window())
        });
        assert!((10..14).all(|item| grid.displayed(item)));
        assert!(grid.all_exist(14..16), "{:?}", grid.seen.live);
        grid.scroll_by(-ITEM * 2.5);
        assert_eq!(grid.state.first_visible_item_index(), 6);
        assert!(grid.none_exist(0..2), "{:?}", grid.seen.live);
        assert!(grid.all_exist(2..6), "{:?}", grid.seen.live);
        assert!((6..10).all(|item| grid.displayed(item)));
        assert!(grid.all_exist(10..12), "{:?}", grid.seen.live);
        assert!(grid.none_exist(12..14), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, datasetChanged_shouldScheduleNewPrefetching_ifWindowIsNotFull
#[test]
fn more_items_fill_a_grids_window_with_room() {
    for vertical in ORIENTATIONS {
        let count = mutable_state_of(100usize);
        let mut grid = compose_grid(GridArgs {
            first: 94,
            count: Some(count.handle()),
            ..GridArgs::new(vertical, viewport_window())
        });
        assert!((94..98).all(|item| grid.displayed(item)));
        assert!(grid.all_exist(98..100));
        grid.scroll_by(ITEM);
        assert!(grid.displayed(98) && grid.displayed(99));
        assert!(!grid.seen.exists(100));
        count.set(200);
        idle(&mut grid.ui);
        assert!(grid.all_exist(100..102), "the window has room: {:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowTest.kt, datasetChanged_shouldNotScheduleNewPrefetching_ifWindowIsFull
#[test]
fn more_items_leave_a_full_grid_window_alone() {
    for vertical in ORIENTATIONS {
        let count = mutable_state_of(100usize);
        let mut grid = compose_grid(GridArgs {
            first: 92,
            count: Some(count.handle()),
            ..GridArgs::new(vertical, viewport_window())
        });
        assert!((92..96).all(|item| grid.displayed(item)));
        grid.scroll_by(ITEM);
        assert!((94..98).all(|item| grid.displayed(item)));
        assert!(grid.all_exist(98..100));
        assert!(grid.none_exist(100..102));
        count.set(200);
        idle(&mut grid.ui);
        assert!(grid.none_exist(100..102), "the window is full: {:?}", grid.seen.live);
    }
}

fn parity_grid(args: GridArgs) -> Grid {
    let parity = Rc::new(ParityWindow { ahead: Cell::new(0) });
    let grid = compose_grid(GridArgs { window: parity.clone(), ..args });
    parity.ahead.set(ITEM as i32);
    grid
}

fn parity_grid_args(vertical: bool) -> GridArgs {
    GridArgs::new(vertical, lazy_layout_cache_window(0.dp(), 0.dp()))
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, notPrefetchingForwardInitially
#[test]
fn grid_parity_not_prefetching_forward_initially() {
    for vertical in ORIENTATIONS {
        let grid = compose_grid(parity_grid_args(vertical));
        assert!(grid.none_exist(4..6), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, notPrefetchingBackwardInitially
#[test]
fn grid_parity_not_prefetching_backward_initially() {
    for vertical in ORIENTATIONS {
        let grid = parity_grid(GridArgs { first: 4, ..parity_grid_args(vertical) });
        assert!(grid.none_exist(0..2), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, prefetchingForwardAfterSmallScroll
#[test]
fn grid_parity_prefetching_forward_after_a_small_scroll() {
    for vertical in ORIENTATIONS {
        let mut grid = parity_grid(parity_grid_args(vertical));
        grid.scroll_by(5.0);
        assert!(grid.all_exist(4..6), "{:?}", grid.seen.live);
        assert!(grid.none_exist(6..8), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, prefetchingBackwardAfterSmallScroll
#[test]
fn grid_parity_prefetching_backward_after_a_small_scroll() {
    for vertical in ORIENTATIONS {
        let mut grid = parity_grid(GridArgs { first: 4, offset: 10, ..parity_grid_args(vertical) });
        grid.scroll_by(-5.0);
        assert!(grid.all_exist(2..4), "{:?}", grid.seen.live);
        assert!(grid.none_exist(0..2), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, prefetchingForwardAndBackward
#[test]
fn grid_parity_prefetching_forward_and_backward() {
    for vertical in ORIENTATIONS {
        let mut grid = parity_grid(GridArgs { first: 2, ..parity_grid_args(vertical) });
        grid.scroll_by(5.0);
        assert!(grid.all_exist(6..8), "{:?}", grid.seen.live);
        assert!(!grid.seen.exists(0), "{:?}", grid.seen.live);
        grid.scroll_by(-2.0);
        grid.scroll_by(-1.0);
        assert!(grid.all_exist(0..2), "{:?}", grid.seen.live);
        assert!(!grid.seen.exists(6), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, prefetchingForwardTwice
#[test]
fn grid_parity_prefetching_forward_twice() {
    for vertical in ORIENTATIONS {
        let mut grid = parity_grid(parity_grid_args(vertical));
        grid.scroll_by(5.0);
        grid.scroll_by(ITEM / 2.0);
        grid.scroll_by(ITEM / 2.0);
        assert!(grid.displayed(4) && grid.displayed(5));
        assert!(grid.all_exist(6..8), "{:?}", grid.seen.live);
        assert!(grid.none_exist(8..10), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, prefetchingBackwardTwice
#[test]
fn grid_parity_prefetching_backward_twice() {
    for vertical in ORIENTATIONS {
        let mut grid = parity_grid(GridArgs { first: 8, ..parity_grid_args(vertical) });
        grid.scroll_by(-5.0);
        grid.scroll_by(-ITEM / 2.0);
        grid.scroll_by(-ITEM / 2.0);
        assert!(grid.displayed(4) && grid.displayed(5));
        assert!(grid.all_exist(2..4), "{:?}", grid.seen.live);
        assert!(grid.none_exist(0..2), "{:?}", grid.seen.live);
    }
}

// LazyGridCacheWindowParityTest.prefetchingForwardAndBackwardReverseLayout waits for grids' `reverseLayout`
// (feature:grid-parity).

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, disposingWhilePrefetchingScheduled
#[test]
fn grid_parity_disposing_while_prefetching_is_scheduled() {
    for vertical in ORIENTATIONS {
        use_optimizations(Optimizations::NONE);
        let state = LazyGridState::new(0, 0);
        let given = *state;
        let emit = mutable_state_of(true);
        let shown = emit.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            if shown.get() {
                let modifier = main_axis(vertical, ITEM * 1.5);
                let content = move |scope: LazyGridScope| {
                    scope.items(1000, move |_| Spacer(main_axis(vertical, ITEM)));
                };
                if vertical {
                    LazyVerticalGrid!(
                        GridCells::Fixed(2),
                        modifier = modifier,
                        state = given,
                        content = content
                    );
                } else {
                    LazyHorizontalGrid!(
                        GridCells::Fixed(2),
                        modifier = modifier,
                        state = given,
                        content = content
                    );
                }
            }
        });
        idle(&mut ui);
        given.dispatch_raw_delta(ITEM);
        ui.wait_for_idle();
        // The request is queued, and the grid goes.
        emit.set(false);
        idle(&mut ui);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, prefetchingForwardAndBackwardWithContentPadding
#[test]
fn grid_parity_prefetching_with_content_padding() {
    for vertical in ORIENTATIONS {
        let half = Dp(ITEM / 2.0);
        let padding = if vertical {
            PaddingValues::symmetric(0.dp(), half)
        } else {
            PaddingValues::symmetric(half, 0.dp())
        };
        let mut grid =
            parity_grid(GridArgs { first: 4, offset: 5, padding, ..parity_grid_args(vertical) });
        assert!(grid.displayed(2) && grid.displayed(4) && grid.displayed(6));
        assert!(!grid.seen.exists(0) && !grid.seen.exists(8), "{:?}", grid.seen.live);
        grid.scroll_by(5.0);
        assert!(grid.seen.exists(8), "{:?}", grid.seen.live);
        assert!(!grid.seen.exists(0), "{:?}", grid.seen.live);
        grid.scroll_by(-2.0);
        assert!(grid.seen.exists(0), "{:?}", grid.seen.live);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, snappingToOtherPositionWhilePrefetchIsScheduled
#[test]
fn grid_parity_snapping_elsewhere_cancels_what_was_scheduled() {
    for vertical in ORIENTATIONS {
        let mut grid = parity_grid(parity_grid_args(vertical));
        let state = *grid.state;
        state.dispatch_raw_delta(ITEM);
        grid.ui.wait_for_idle();
        let _ = run(&mut grid.ui, async move { state.scroll_to_item(100, 0).await });
        idle(&mut grid.ui);
        assert!(!grid.seen.activated.borrow().contains_key(&6), "{:?}", grid.seen.activated);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridCacheWindowParityTest.kt, scrollingByListSizeCancelsPreviousPrefetch
#[test]
fn grid_parity_scrolling_by_the_size_cancels_what_was_scheduled() {
    for vertical in ORIENTATIONS {
        let mut grid = parity_grid(parity_grid_args(vertical));
        let state = *grid.state;
        state.dispatch_raw_delta(ITEM);
        grid.ui.wait_for_idle();
        state.dispatch_raw_delta(ITEM * 3.0);
        grid.ui.wait_for_idle();
        state.dispatch_raw_delta(ITEM);
        grid.ui.wait_for_idle();
        idle(&mut grid.ui);
        grid.scroll_by(ITEM * 100.0);
        assert!(!grid.seen.exists(6), "{:?}", grid.seen.live);
    }
}
