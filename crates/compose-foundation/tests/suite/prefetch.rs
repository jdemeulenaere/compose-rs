//! Prefetch: after a scroll, a lazy list composes and measures the next items in the scroll
//! direction between frames (`UiHost::prefetch`), so the frame that scrolls them in finds them
//! ready; nothing shows, and the list's state stays.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, UnitExt};

thread_local! {
    /// How many times each item was composed, and the items composed now.
    static COMPOSED: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static LIVE: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

fn compositions(index: usize) -> usize {
    COMPOSED.with(|composed| composed.borrow().get(&index).copied().unwrap_or(0))
}

fn live(index: usize) -> bool {
    LIVE.with(|live| live.borrow().contains(&index))
}

#[composable]
fn Row(index: usize) {
    COMPOSED.with(|composed| *composed.borrow_mut().entry(index).or_insert(0) += 1);
    DisposableEffect(index, move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(index));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&index));
        })
    });
    Box!(modifier = Modifier.size_wh(100.dp(), 20.dp()).test_tag(format!("row {index}")), {
        BasicText!("item {}", index);
    });
}

#[composable]
fn List(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size_wh(100.dp(), 100.dp()),
        state = state,
        |scope: LazyListScope| {
            scope.items(1000, |index| Row(index));
        }
    );
}

/// A list of 20 px rows in a 100 px viewport, from item `first`.
fn list(first: usize) -> (ComposeTestRule, LazyListState) {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    let state = LazyListState::new(first, 0);
    let given = state.clone();
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || List(given.clone()));
    (ui, state)
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// The largest item composed.
fn last_live() -> usize {
    LIVE.with(|live| live.borrow().iter().copied().max().unwrap_or(0))
}

#[test]
fn after_a_scroll_down_prefetch_composes_the_next_items_beyond_the_composed_ones() {
    let (mut ui, state) = list(0);
    assert!(!ui.host().has_prefetch_work(), "nothing to prefetch before a scroll");
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_live();
    assert!(ui.host().has_prefetch_work());
    assert!(!ui.host().prefetch(later()), "all done in time");
    assert!(!ui.host().has_prefetch_work());
    assert_eq!(compositions(last + 1), 1);
    assert_eq!(compositions(last + 2), 1);
    assert!(!live(last + 3), "two items ahead");
    // The list's state didn't move.
    assert_eq!(state.first_visible_item_index(), 1);
    assert_eq!(state.first_visible_item_scroll_offset(), 10);
    // Composed and measured, not placed: nothing shows until a frame places them out of view.
    let dump = ui.dump_tree();
    let line = dump.lines().find(|line| line.contains(&format!("\"row {}\"", last + 1)));
    let line = line.unwrap_or_else(|| panic!("row {} in\n{dump}", last + 1));
    assert!(line.contains("100x20"), "measured: {line}");
}

#[test]
fn a_scroll_that_reveals_prefetched_items_does_not_compose_them_again() {
    let (mut ui, state) = list(0);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_live();
    ui.host().prefetch(later());
    // Two rows on: the prefetched ones are wanted now, and found composed.
    state.scroll_by(40.0);
    let stats = ui.host().frame();
    assert!(!stats.recomposed, "nothing composed in the frame");
    ui.wait_for_idle();
    assert_eq!(compositions(last + 1), 1);
    assert_eq!(compositions(last + 2), 1);
    assert_eq!(state.first_visible_item_index(), 3);
    assert_eq!(state.first_visible_item_scroll_offset(), 10);
    // The scroll asks for the next ones.
    assert!(ui.host().has_prefetch_work());
    ui.host().prefetch(later());
    assert_eq!(compositions(last + 3), 1);
    assert_eq!(compositions(last + 4), 1);
}

#[test]
fn a_scroll_up_prefetches_the_items_before() {
    let (mut ui, state) = list(100);
    let first = LIVE.with(|live| live.borrow().iter().copied().min().unwrap());
    state.scroll_by(-30.0);
    ui.wait_for_idle();
    let first_now = LIVE.with(|live| live.borrow().iter().copied().min().unwrap());
    assert!(first_now < first);
    ui.host().prefetch(later());
    assert_eq!(compositions(first_now - 1), 1);
    assert_eq!(compositions(first_now - 2), 1);
    assert!(!live(first_now - 3));
}

#[test]
fn prefetch_past_its_deadline_does_nothing() {
    let (mut ui, state) = list(0);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_live();
    assert!(ui.host().prefetch(Instant::now()), "work left");
    assert!(ui.host().has_prefetch_work());
    assert_eq!(compositions(last + 1), 0);
    // The next idle time does it.
    assert!(!ui.host().prefetch(later()));
    assert_eq!(compositions(last + 1), 1);
}

#[test]
fn prefetched_items_left_far_behind_are_disposed() {
    let (mut ui, state) = list(100);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_live();
    ui.host().prefetch(later());
    assert!(live(last + 2));
    // Back up, far: the items prefetched below go, as the composed ones out of view do.
    state.scroll_by(-240.0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 89);
    assert!(!live(last + 1) && !live(last + 2), "{:?}", LIVE.with(|live| live.borrow().clone()));
}

/// The rows in the viewport, with their bounds.
fn rows_in_view(ui: &mut ComposeTestRule) -> Vec<(usize, compose_ui::Rect)> {
    let mut rows = Vec::new();
    for index in 0..20 {
        let tag = format!("row {index}");
        if ui.count_nodes(&has_test_tag(&tag)) == 1 {
            let bounds = ui.on_node_with_tag(&tag).fetch_semantics_node().bounds;
            if bounds.bottom > 0.0 && bounds.top < 100.0 {
                rows.push((index, bounds));
            }
        }
    }
    rows
}

#[test]
fn prefetch_changes_nothing_in_view() {
    // Two lists scrolled alike, one prefetching between frames.
    let (mut plain, plain_state) = list(0);
    let (mut prefetched, state) = list(0);
    for _ in 0..8 {
        plain_state.scroll_by(15.0);
        plain.wait_for_idle();
        state.scroll_by(15.0);
        prefetched.wait_for_idle();
        assert_eq!(rows_in_view(&mut plain), rows_in_view(&mut prefetched));
        assert_eq!(plain_state.first_visible_item_index(), state.first_visible_item_index());
        assert_eq!(
            plain_state.first_visible_item_scroll_offset(),
            state.first_visible_item_scroll_offset()
        );
        let before = prefetched.dump_display_list();
        prefetched.host().prefetch(later());
        assert_eq!(prefetched.dump_display_list(), before);
    }
}

#[test]
fn a_grid_prefetches_its_next_rows() {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    let state = LazyGridState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size_wh(300.dp(), 100.dp()),
            state = given.clone(),
            |scope: LazyGridScope| {
                scope.items(300, Row);
            }
        );
    });
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_live();
    assert_eq!(last % 3, 2, "whole rows composed");
    assert!(!ui.host().prefetch(later()));
    // Two rows of three.
    assert!((last + 1..=last + 6).all(|index| compositions(index) == 1));
    assert!(!live(last + 7));
    assert_eq!(state.first_visible_item_index(), 3);
}
