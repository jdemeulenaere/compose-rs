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

/// The lazy items composed now.
fn items_composed(ui: &ComposeTestRule) -> usize {
    ui.dump_tree().lines().filter(|line| line.trim_start().starts_with("LazyItem")).count()
}

#[test]
fn prefetch_changes_nothing_in_view_and_composes_no_more_than_it_prefetches() {
    // Two lists scrolled alike, one prefetching between frames.
    let (mut plain, plain_state) = list(0);
    let (mut prefetched, state) = list(0);
    for frame in 0..60 {
        let delta = if frame < 40 { 15.0 } else { -15.0 };
        plain_state.scroll_by(delta);
        plain.wait_for_idle();
        state.scroll_by(delta);
        prefetched.wait_for_idle();
        assert_eq!(rows_in_view(&mut plain), rows_in_view(&mut prefetched));
        assert_eq!(plain_state.first_visible_item_index(), state.first_visible_item_index());
        assert_eq!(
            plain_state.first_visible_item_scroll_offset(),
            state.first_visible_item_scroll_offset()
        );
        // No more items drawn (prefetched ones aren't placed), and only those prefetched
        // composed besides (those left behind go as without prefetch).
        let drawn = |ui: &ComposeTestRule| ui.dump_display_list().matches("paragraph").count();
        let (with, without) = (drawn(&prefetched), drawn(&plain));
        assert!(with <= without, "frame {frame}: {with} items drawn, {without} without");
        let (with, without) = (items_composed(&prefetched), items_composed(&plain));
        assert!(with <= without + 2, "frame {frame}: {with} items composed, {without} without");
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

thread_local! {
    static TEXT_LAYOUTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The fixed-advance measurer, counting the paragraphs it lays out.
struct CountingMeasurer(compose_ui::FixedTextMeasurer);

impl compose_ui::TextMeasurer for CountingMeasurer {
    fn layout(
        &mut self,
        text: &str,
        style: &compose_ui::TextStyle,
        options: compose_ui::TextLayoutOptions,
        constraints: compose_ui::Constraints,
        density: &compose_ui::Density,
    ) -> compose_ui::ParagraphHandle {
        TEXT_LAYOUTS.with(|layouts| layouts.set(layouts.get() + 1));
        self.0.layout(text, style, options, constraints, density)
    }
    fn metrics(&self, paragraph: compose_ui::ParagraphHandle) -> compose_ui::ParagraphMetrics {
        self.0.metrics(paragraph)
    }
    fn hit_test(
        &self,
        paragraph: compose_ui::ParagraphHandle,
        position: compose_ui::Offset,
    ) -> compose_ui::TextPosition {
        self.0.hit_test(paragraph, position)
    }
    fn cursor_rect(
        &self,
        paragraph: compose_ui::ParagraphHandle,
        offset: usize,
    ) -> compose_ui::Rect {
        self.0.cursor_rect(paragraph, offset)
    }
    fn text(&self, paragraph: compose_ui::ParagraphHandle) -> &str {
        self.0.text(paragraph)
    }
    fn release(&mut self, paragraph: compose_ui::ParagraphHandle) {
        self.0.release(paragraph)
    }
}

fn text_layouts() -> usize {
    TEXT_LAYOUTS.with(std::cell::Cell::get)
}

/// The list of `list`, measuring text with `CountingMeasurer`.
fn counted_list() -> (ComposeTestRule, LazyListState) {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let ui = compose_test_with(
        IntSize::new(200, 200),
        compose_ui::Density::default(),
        Box::new(CountingMeasurer(compose_ui::FixedTextMeasurer::new())),
        move || List(given.clone()),
    );
    (ui, state)
}

#[test]
fn a_frame_revealing_prefetched_items_lays_out_no_text() {
    let (mut ui, state) = counted_list();
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let before = text_layouts();
    ui.host().prefetch(later());
    assert_eq!(text_layouts(), before + 2, "the two items ahead, laid out once");
    // A row at a time: the prefetched items come into the wanted range, then into view.
    for _ in 0..6 {
        let before = text_layouts();
        state.scroll_by(20.0);
        let stats = ui.host().frame();
        assert_eq!(text_layouts(), before, "text laid out in the frame");
        assert_eq!(stats.bodies_run, 0, "composed in the frame");
        ui.host().prefetch(later());
    }
}

#[test]
fn a_prefetch_with_nothing_to_do_does_nothing() {
    let (mut ui, state) = counted_list();
    state.scroll_by(30.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    // A scroll within the same items wants no other: nothing is asked for.
    state.scroll_by(5.0);
    ui.wait_for_idle();
    assert!(!ui.host().has_prefetch_work(), "a request with nothing to do");
    let (layouts, live) = (text_layouts(), LIVE.with(|live| live.borrow().len()));
    let composed: usize = COMPOSED.with(|composed| composed.borrow().values().sum());
    assert!(!ui.host().prefetch(later()));
    assert_eq!(text_layouts(), layouts);
    assert_eq!(LIVE.with(|live| live.borrow().len()), live);
    assert_eq!(COMPOSED.with(|composed| composed.borrow().values().sum::<usize>()), composed);
}

#[test]
fn prefetch_asks_for_no_frame() {
    let (mut ui, state) = list(0);
    let wakes = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = wakes.clone();
    ui.host().set_frame_waker(move || counter.set(counter.get() + 1));
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let before = wakes.get();
    assert!(!ui.host().prefetch(later()));
    assert_eq!(wakes.get(), before, "prefetch woke the platform");
    assert!(!ui.host().needs_frame(), "prefetch left work for a frame");
}
