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
    Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}")), {
        BasicText!("item {}", index);
    });
}

#[composable]
fn List(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items(1000, |index| Row(index));
        }
    );
}

/// A list of 20 px rows in a 100 px viewport, from item `first`.
fn list(first: usize) -> (ComposeTestRule, Owned<LazyListState>) {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    let state = LazyListState::new(first, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || List(given));
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
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 100.dp())),
            state = given,
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
struct CountingMeasurer(compose_ui::platform::FixedTextMeasurer);

impl compose_ui::platform::TextMeasurer for CountingMeasurer {
    fn layout(
        &mut self,
        text: &str,
        style: &compose_ui::TextStyle,
        options: compose_ui::platform::TextLayoutOptions,
        constraints: compose_ui::Constraints,
        density: &compose_ui::Density,
    ) -> compose_ui::platform::ParagraphHandle {
        TEXT_LAYOUTS.with(|layouts| layouts.set(layouts.get() + 1));
        self.0.layout(text, style, options, constraints, density)
    }
    fn metrics(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
    ) -> compose_ui::platform::ParagraphMetrics {
        self.0.metrics(paragraph)
    }
    fn hit_test(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
        position: compose_ui::Offset,
    ) -> compose_ui::platform::TextPosition {
        self.0.hit_test(paragraph, position)
    }
    fn cursor_rect(
        &self,
        paragraph: compose_ui::platform::ParagraphHandle,
        offset: usize,
    ) -> compose_ui::Rect {
        self.0.cursor_rect(paragraph, offset)
    }
    fn text(&self, paragraph: compose_ui::platform::ParagraphHandle) -> &str {
        self.0.text(paragraph)
    }
    fn release(&mut self, paragraph: compose_ui::platform::ParagraphHandle) {
        self.0.release(paragraph)
    }
}

fn text_layouts() -> usize {
    TEXT_LAYOUTS.with(std::cell::Cell::get)
}

/// The list of `list`, measuring text with `CountingMeasurer`.
fn counted_list() -> (ComposeTestRule, Owned<LazyListState>) {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    let state = LazyListState::new(0, 0);
    let given = *state;
    let ui = compose_test_with(
        IntSize::new(200, 200),
        compose_ui::Density::default(),
        Box::new(CountingMeasurer(compose_ui::platform::FixedTextMeasurer::new())),
        move || List(given),
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

thread_local! {
    /// How many times each painted row's draw code ran.
    static DRAWN: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

fn draws(index: usize) -> usize {
    DRAWN.with(|drawn| drawn.borrow().get(&index).copied().unwrap_or(0))
}

/// Row `index`'s color: what tells its group apart in the display list.
fn row_color(index: usize) -> compose_ui::Color {
    compose_ui::Color::from_argb(0xFF00_0000 | index as u32)
}

#[composable]
fn PaintedRow(index: usize) {
    let painted = Modifier.draw_behind(move |scope| {
        DRAWN.with(|drawn| *drawn.borrow_mut().entry(index).or_insert(0) += 1);
        let bounds = scope.bounds();
        scope.draw_rect(bounds, compose_ui::Paint::fill(row_color(index)));
    });
    Box!(modifier = painted.size((100.dp(), 20.dp())), {
        BasicText!("item {}", index);
    });
}

#[composable]
fn PaintedList(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items(1000, |index| PaintedRow(index));
        }
    );
}

/// Each drawn row's group (its lazy item's), by row: its key and version.
fn row_groups(list: &compose_ui::platform::FlatDisplayList) -> HashMap<usize, (u64, u64)> {
    let mut rows = HashMap::new();
    for group in &list.groups {
        for op in &list.ops[group.start..group.end] {
            if let compose_ui::platform::DrawOp::DrawRect(_, paint) = op {
                let index = (paint.color.argb() & 0x00FF_FFFF) as usize;
                let first = rows.insert(index, (group.key, group.version)).is_none();
                assert!(first, "row {index} drawn once");
            }
        }
    }
    rows
}

/// A display list's dump, without its paragraph handles (a fresh measurer numbers them anew).
fn dump_without_paragraphs(list: &compose_ui::platform::FlatDisplayList) -> String {
    list.dump()
        .lines()
        .map(|line| match line.find("paragraph ") {
            Some(at) => {
                let rest = &line[at + "paragraph ".len()..];
                let after = rest.find(' ').map_or("", |space| &rest[space..]);
                format!("{}paragraph _{after}", &line[..at])
            }
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prefetch with retained drawing (D155, D156): the frame that brings prefetched items into the
/// placed range draws them for the first time and nothing else again (the items that stay keep
/// their versions, and their draw code doesn't run: a scroll only moves them), and the list it
/// assembles is what a fresh host at that position draws.
#[test]
fn a_frame_revealing_prefetched_items_draws_only_them() {
    DRAWN.with(|drawn| drawn.borrow_mut().clear());
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || PaintedList(given));
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let before = row_groups(&ui.host().display_list());
    let last_drawn = *before.keys().max().unwrap();
    let newest = before.values().map(|(_, version)| *version).max().unwrap();
    let draws_before: HashMap<usize, usize> =
        before.keys().map(|&index| (index, draws(index))).collect();

    // Prefetched between frames: composed and measured, not drawn, and no frame asked for.
    assert!(ui.host().has_prefetch_work());
    assert!(!ui.host().prefetch(later()));
    assert!(!ui.host().needs_frame(), "prefetch asks for no frame");
    assert_eq!(draws(last_drawn + 1), 0, "a prefetched item isn't drawn");
    assert_eq!(draws(last_drawn + 2), 0, "a prefetched item isn't drawn");

    // Two rows on: the prefetched items are placed now, found composed.
    state.scroll_by(40.0);
    let stats = ui.host().frame();
    assert!(!stats.recomposed, "nothing composed in the frame");
    assert!(stats.drew);
    let list = ui.host().display_list();
    let after = row_groups(&list);
    for revealed in [last_drawn + 1, last_drawn + 2] {
        let (_, version) = after
            .get(&revealed)
            .unwrap_or_else(|| panic!("row {revealed} drawn in\n{}", list.dump()));
        assert!(*version > newest, "row {revealed} drawn for the first time");
        assert_eq!(draws(revealed), 1, "row {revealed} drawn once");
    }
    let mut kept = 0;
    for (index, group) in &before {
        let Some(now) = after.get(index) else { continue };
        assert_eq!(now, group, "row {index} keeps its group and version");
        assert_eq!(draws(*index), draws_before[index], "row {index}'s draw code didn't run");
        kept += 1;
    }
    assert!(kept >= 5, "the rows in view stay: {kept}");
    for index in after.keys() {
        let new = !before.contains_key(index);
        assert!(!new || *index > last_drawn, "only prefetched rows are new, not row {index}");
    }

    // The same flat list as a fresh host's at that position.
    let position = (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
    assert_eq!(position, (3, 10));
    let fresh_state = LazyListState::new(position.0, position.1);
    let mut fresh =
        ComposeTestRule::sized(IntSize::new(200, 200), move || PaintedList(*fresh_state));
    assert_eq!(
        dump_without_paragraphs(&list),
        dump_without_paragraphs(&fresh.host().display_list()),
    );
}

// ---- Drawing ahead -----------------------------------------------------------------------------

/// The row whose rectangle the group's own content draws, if any.
fn row_of(group: &compose_ui::platform::DrawGroup) -> Option<usize> {
    group.content.ops.iter().find_map(|op| match op {
        compose_ui::platform::DrawOp::DrawRect(_, paint) => {
            Some((paint.color.argb() & 0x00FF_FFFF) as usize)
        }
        _ => None,
    })
}

/// Draws ahead every item waiting: each item's group, by row.
fn draw_all_ahead(ui: &mut ComposeTestRule) -> HashMap<usize, compose_ui::platform::DrawGroup> {
    let mut drawn = HashMap::new();
    while let Some(groups) = ui.host().draw_ahead() {
        let group = groups.into_iter().next().expect("the item's group first");
        let row = row_of(&group).expect("a painted row");
        assert!(drawn.insert(row, group).is_none(), "row {row} drawn ahead twice");
    }
    assert!(!ui.host().has_draw_ahead_work());
    drawn
}

/// The painted list, scrolled 30 px down and prefetched: its two prefetched rows (the last
/// drawn, plus one and two) wait to be drawn ahead.
fn prefetched_painted_list() -> (ComposeTestRule, Owned<LazyListState>, usize) {
    DRAWN.with(|drawn| drawn.borrow_mut().clear());
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || PaintedList(given));
    assert!(!ui.host().has_draw_ahead_work(), "nothing measured ahead yet");
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last_drawn = *row_groups(&ui.host().display_list()).keys().max().unwrap();
    assert!(!ui.host().prefetch(later()));
    (ui, state, last_drawn)
}

/// A prefetched item is drawn ahead between frames (D350): its group is the one the frame
/// placing it shares, same content and version, its draw code not run again; nothing is asked
/// of a frame, and the frame's list is a fresh host's.
#[test]
fn a_prefetched_item_drawn_ahead_is_kept_by_the_frame_placing_it() {
    let (mut ui, state, last_drawn) = prefetched_painted_list();
    assert!(ui.host().has_draw_ahead_work());
    let ahead = draw_all_ahead(&mut ui);
    assert_eq!(ahead.len(), 2, "the two prefetched rows");
    for row in [last_drawn + 1, last_drawn + 2] {
        assert!(ahead.contains_key(&row), "row {row} drawn ahead");
        assert_eq!(draws(row), 1, "row {row} drawn once");
    }
    assert!(ui.host().draw_ahead().is_none());
    assert!(!ui.host().needs_frame(), "drawing ahead asks for no frame");

    // Two rows on: the frame places them, drawing neither again.
    state.scroll_by(40.0);
    assert!(ui.host().draw_ahead().is_none(), "a frame has work");
    ui.host().frame();
    let list = ui.host().display_list();
    let after = row_groups(&list);
    for (row, group) in &ahead {
        assert_eq!(after.get(row), Some(&(group.key, group.version)), "row {row} kept");
        assert_eq!(draws(*row), 1, "row {row} not drawn again");
        let shared = ui.host().with_tree(|tree| {
            let frame = tree.display_list().groups.iter().find(|kept| kept.key == group.key);
            frame.is_some_and(|kept| std::sync::Arc::ptr_eq(&kept.content, &group.content))
        });
        assert!(shared, "row {row}'s recording shared, not drawn again");
    }
    let fresh_state = LazyListState::new(3, 10);
    let mut fresh =
        ComposeTestRule::sized(IntSize::new(200, 200), move || PaintedList(*fresh_state));
    assert_eq!(
        dump_without_paragraphs(&list),
        dump_without_paragraphs(&fresh.host().display_list()),
    );
}

/// A host that doesn't draw ahead (desktop) keeps no item measured ahead once a frame places
/// it, or once it's gone.
#[test]
fn items_measured_ahead_are_forgotten_once_placed_or_gone() {
    let (mut ui, state, _) = prefetched_painted_list();
    assert_eq!(ui.host().with_tree(|tree| tree.measured_ahead_count()), 2);
    state.scroll_by(40.0);
    ui.wait_for_idle();
    // The frame placing them forgot them (asking `has_draw_ahead_work` would too).
    assert_eq!(ui.host().with_tree(|tree| tree.measured_ahead_count()), 0, "both placed");
    // Given up: the frames draw them.
    let (mut ui, _, last_drawn) = prefetched_painted_list();
    ui.host().cancel_draw_ahead();
    assert!(!ui.host().has_draw_ahead_work());
    assert!(ui.host().draw_ahead().is_none());
    assert_eq!(draws(last_drawn + 1), 0);
    let (mut ui, state, last_drawn) = prefetched_painted_list();
    // Back up far: the items prefetched below are disposed before any is drawn ahead.
    state.scroll_by(-30.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    assert!(!ui.host().has_draw_ahead_work(), "disposed");
    assert!(ui.host().draw_ahead().is_none());
    assert_eq!(draws(last_drawn + 1), 0, "a disposed item isn't drawn ahead");
    assert_eq!(draws(last_drawn + 2), 0, "a disposed item isn't drawn ahead");
}

/// What an item drawn ahead read changes before a frame places it: the write asks for a frame
/// (drawing ahead waits for it), and the frame placing the item draws it again.
#[test]
fn an_item_drawn_ahead_draws_again_when_what_it_read_changes() {
    let tint = mutable_state_of(0u32);
    let read = tint.handle();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, move |index| {
                    let painted = Modifier.draw_behind(move |scope| {
                        let color = compose_ui::Color::from_argb(
                            0xFF00_0000 | (read.get() << 16) | index as u32,
                        );
                        scope.draw_rect(scope.bounds(), compose_ui::Paint::fill(color));
                    });
                    Box!(modifier = painted.size((100.dp(), 20.dp())));
                });
            }
        );
    });
    state.scroll_by(30.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    let mut ahead = Vec::new();
    while let Some(groups) = ui.host().draw_ahead() {
        ahead.push((groups[0].key, groups[0].version));
    }
    assert_eq!(ahead.len(), 2);
    tint.set(1);
    assert!(ui.host().needs_frame(), "the write asks for a frame");
    assert!(ui.host().draw_ahead().is_none(), "the frame goes first");
    ui.wait_for_idle();
    state.scroll_by(40.0);
    ui.host().frame();
    let list = ui.host().display_list();
    for (key, version) in ahead {
        let group = list.groups.iter().find(|group| group.key == key).expect("placed");
        assert_ne!(group.version, version, "drawn again");
        let tinted = list.ops[group.start..group.end].iter().any(|op| {
            matches!(op, compose_ui::platform::DrawOp::DrawRect(_, paint)
                if paint.color.argb() >> 16 & 0xFF == 1)
        });
        assert!(tinted, "with the new tint");
    }
}

#[composable]
fn KeyedList(state: LazyListState, order: MutableState<Vec<usize>>) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            let order = order.get();
            let keys = order.clone();
            scope.items_keyed(
                order.len(),
                move |index| keys[index],
                move |index| PaintedRow(order[index]),
            );
        }
    );
}

/// Two lists scrolled alike, back and forth, their keys reordered on the way, one drawing its
/// prefetched items ahead: every frame draws the same.
#[test]
fn drawing_ahead_changes_nothing_a_frame_draws() {
    let rule = || {
        let state = LazyListState::new(0, 0);
        let order = mutable_state_of((0..300).collect::<Vec<usize>>());
        let (given, keys) = (*state, order.handle());
        let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || KeyedList(given, keys));
        (ui, state, order)
    };
    let (mut plain, plain_state, plain_order) = rule();
    let (mut ahead, state, order) = rule();
    let mut drawn_ahead = 0;
    for frame in 0..80 {
        let delta = match frame % 10 {
            7 => -35.0,
            9 => -10.0,
            _ => 23.0,
        };
        if frame % 13 == 12 {
            // The first item in view swaps with one six rows below (prefetched, maybe drawn
            // ahead).
            let first = plain_state.first_visible_item_index();
            for order in [&plain_order, &order] {
                let mut keys = order.get();
                keys.swap(first, first + 6);
                order.set(keys);
            }
        }
        plain_state.scroll_by(delta);
        plain.wait_for_idle();
        state.scroll_by(delta);
        ahead.wait_for_idle();
        assert_eq!(
            dump_without_paragraphs(&plain.host().display_list()),
            dump_without_paragraphs(&ahead.host().display_list()),
            "frame {frame}"
        );
        plain.host().prefetch(later());
        ahead.host().prefetch(later());
        while ahead.host().draw_ahead().is_some() {
            drawn_ahead += 1;
        }
    }
    assert!(drawn_ahead > 20, "items were drawn ahead: {drawn_ahead}");
}
