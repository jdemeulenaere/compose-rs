//! Adversarial review of D590-D596 (pausable composition in prefetch).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{Optimization, Optimizations};
use compose_ui::{IntSize, UnitExt};

// The requests pause against a real-time deadline. Rather than slow parts racing a short one
// (which a loaded machine loses), every part is instant but one, which stalls until the
// deadline has passed: the request goes on until that part, then pauses at the next pause
// point, whatever the machine's speed. The deadline is generous: everything before the stall
// has `IDLE` to run.

/// The idle time a request pausing at the stall gets.
const IDLE: Duration = Duration::from_millis(400);

/// When the stall ends: just after the deadline.
fn stall_until(deadline: Instant) {
    let end = deadline + Duration::from_millis(1);
    std::thread::sleep(end.saturating_duration_since(Instant::now()));
}

/// A cell (row, cell), its part (counted from 1) that stalls, and the deadline it stalls until.
type CellStall = ((usize, usize), usize, Instant);

thread_local! {
    static CELL_PARTS: RefCell<HashMap<(usize, usize), usize>> = RefCell::new(HashMap::new());
    static CELL_STALL: Cell<Option<CellStall>> = const { Cell::new(None) };
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

fn scroll(ui: &mut ComposeTestRule, state: LazyListState, pixels: f32) {
    state.dispatch_raw_delta(pixels);
    ui.wait_for_idle();
}

/// A part of a carousel's cell: restartable (a pause point), instant unless it's the stall.
#[composable]
fn CellPart(row: usize, cell: usize) {
    let count = CELL_PARTS.with(|parts| {
        let mut parts = parts.borrow_mut();
        let count = parts.entry((row, cell)).or_insert(0);
        *count += 1;
        *count
    });
    if let Some((key, part, deadline)) = CELL_STALL.get()
        && key == (row, cell)
        && part == count
    {
        stall_until(deadline);
    }
    Box!(modifier = Modifier.size((5.dp(), 20.dp())));
}

/// A 100 x 100 row holding a `LazyRow` of 30 px cells, each two slow parts.
#[composable]
fn CarouselRow(row: usize) {
    Column!(modifier = Modifier.size((100.dp(), 100.dp())), {
        LazyRow!(modifier = Modifier.size((100.dp(), 20.dp())), |scope: LazyListScope| {
            scope.items(20, move |cell| {
                Row!({
                    for _ in 0..6 {
                        CellPart(row, cell);
                    }
                });
            });
        });
    });
}

fn carousels(
    first: usize,
    optimizations: Optimizations,
) -> (ComposeTestRule, Owned<LazyListState>) {
    CELL_STALL.set(None);
    CELL_PARTS.with(|parts| parts.borrow_mut().clear());
    use_optimizations(optimizations);
    let state = LazyListState::new(first, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 400), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, CarouselRow);
            }
        );
    });
    (ui, state)
}

/// Parity: the turn leaves the nested request's paused composition pending (nothing cancels
/// it); the trimming request scheduled first then composes the list's items, which completes it
/// whole, ungated, and the carousel's `pending` stays set (its trims wait for good). No panic.
#[test]
fn review_a_turn_while_a_nested_request_is_paused_leaves_it_pending() {
    turn_while_nested_paused(Optimizations::NONE);
}

/// With `KEEP_ITEMS_BEHIND` no trimming request runs first: row 4's request makes a second
/// paused composition of the composition, a misuse: it panics.
#[test]
fn review_a_turn_while_a_nested_request_is_paused_keeping_items_behind() {
    turn_while_nested_paused(Optimizations::NONE.with(Optimization::KEEP_ITEMS_BEHIND, true));
}

fn turn_while_nested_paused(optimizations: Optimizations) {
    let (mut ui, state) = carousels(5, optimizations);
    // 5 px down: rows 5 and 6 show, row 7 requested.
    scroll(&mut ui, *state, 5.0);
    // Row 7 composes (its carousel's first cell with it, as the carousel's first layout's) and
    // applies; its carousel's second cell (a nested request) pauses, out of time at its
    // second part.
    let deadline = Instant::now() + IDLE;
    CELL_STALL.set(Some(((7, 1), 2, deadline)));
    assert!(ui.host().prefetch(deadline), "work left");
    CELL_STALL.set(None);
    let composed =
        |cell| CELL_PARTS.with(|parts| parts.borrow().get(&(7, cell)).copied().unwrap_or(0));
    assert_eq!(composed(0), 6, "row 7 composed whole (its carousel's first cell with it)");
    assert!(
        composed(1) > 0 && composed(1) < 6,
        "the carousel's second cell paused after {} parts",
        composed(1)
    );
    // A turn: row 7's request is cancelled (its own composition was applied; its nested
    // request's paused composition isn't cancelled), row 4 requested.
    scroll(&mut ui, *state, -3.0);
    ui.host().composition().validate();
    // The next idle time: row 4's request.
    ui.host().prefetch(later());
    ui.host().composition().validate();
    while ui.host().prefetch(later()) {}
    scroll(&mut ui, *state, -100.0);
    ui.host().composition().validate();
}

thread_local! {
    static ROW_PARTS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    /// The row's part (counted from 1) that stalls until the deadline.
    static ROW_STALL: Cell<Option<(usize, usize, Instant)>> = const { Cell::new(None) };
}

#[composable]
fn RowPart(row: usize, part: usize) {
    let count = ROW_PARTS.with(|parts| {
        let mut parts = parts.borrow_mut();
        let count = parts.entry(row).or_insert(0);
        *count += 1;
        *count
    });
    if let Some((stalled, at, deadline)) = ROW_STALL.get()
        && stalled == row
        && at == count
    {
        stall_until(deadline);
    }
    Box!(modifier = Modifier.size((25.dp(), 20.dp())).test_tag(format!("part {row}.{part}")));
}

#[composable]
fn PartsRow(index: usize) {
    Row!(modifier = Modifier.size((100.dp(), 20.dp())), {
        for part in 0..4 {
            RowPart(index, part);
        }
    });
}

/// A list leaving the composition while a request's paused composition is pending: the pass
/// removing it runs a body around the paused content, so it first completes it (whole, in the
/// frame) and applies it, only to remove it.
#[test]
fn review_a_list_leaving_completes_its_paused_item_first() {
    ROW_PARTS.with(|parts| parts.borrow_mut().clear());
    ROW_STALL.set(None);
    use_optimizations(Optimizations::NONE);
    let shown = mutable_state_of(true);
    let shown_handle = shown.handle();
    let state = LazyListState::new(1, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        if shown_handle.get() {
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(1000, PartsRow);
                }
            );
        }
    });
    scroll(&mut ui, *state, 5.0);
    // Row 7's request pauses out of time at its second part.
    let deadline = Instant::now() + IDLE;
    ROW_STALL.set(Some((7, 2, deadline)));
    assert!(ui.host().prefetch(deadline), "work left");
    ROW_STALL.set(None);
    let before = ROW_PARTS.with(|parts| parts.borrow().get(&7).copied().unwrap_or(0));
    assert!(before > 0 && before < 4, "row 7 paused after {before} parts");
    let started = Instant::now();
    shown.set(false);
    ui.wait_for_idle();
    let after = ROW_PARTS.with(|parts| parts.borrow().get(&7).copied().unwrap_or(0));
    assert_eq!(
        after,
        before,
        "row 7 composed {} more parts ({:?}) in the frame removing its list",
        after - before,
        started.elapsed()
    );
}

/// The content around a list recomposing (an unrelated state read above it, the list itself
/// skipping with the same arguments) while a request's paused composition is pending: the frame
/// completes the paused item, though nothing in the pass reaches it.
#[test]
fn review_a_recomposition_above_the_list_completes_its_paused_item_in_the_frame() {
    ROW_PARTS.with(|parts| parts.borrow_mut().clear());
    ROW_STALL.set(None);
    use_optimizations(Optimizations::NONE);
    let tick = mutable_state_of(0);
    let tick_handle = tick.handle();
    let state = LazyListState::new(1, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        Column!({
            // Read here: a write recomposes this content, not the list.
            Box!(
                modifier =
                    Modifier.size((1.dp(), 1.dp())).test_tag(format!("tick {}", tick_handle.get()))
            );
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(1000, PartsRow);
                }
            );
        });
    });
    scroll(&mut ui, *state, 5.0);
    // Row 7's request pauses out of time at its second part.
    let deadline = Instant::now() + IDLE;
    ROW_STALL.set(Some((7, 2, deadline)));
    assert!(ui.host().prefetch(deadline), "work left");
    ROW_STALL.set(None);
    let before = ROW_PARTS.with(|parts| parts.borrow().get(&7).copied().unwrap_or(0));
    assert!(before > 0 && before < 4, "row 7 paused after {before} parts");
    tick.set(1);
    ui.wait_for_idle();
    let after = ROW_PARTS.with(|parts| parts.borrow().get(&7).copied().unwrap_or(0));
    assert_eq!(after, before, "row 7 completed in the frame recomposing the list's parent");
}
