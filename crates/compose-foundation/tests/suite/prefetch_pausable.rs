//! Prefetch with pausable composition, at the tip Compose's only way (D590-D596, D892): a request composes
//! its item in a paused composition, gated by the averages of a stretch and a pause, pausing at
//! the runtime's pause points once the time left is no more than those averages, resuming after
//! the next frame, applied once complete (gated by its own average), then its nested lists'
//! first items, then measured. Nothing of it is in the tree, nor its effects run, before it's
//! applied; a frame wanting it completes it; a turn cancels it.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{Optimization, Optimizations};
use compose_ui::{Constraints, IntSize, UnitExt};

thread_local! {
    /// Each row's parts composed.
    static PARTS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    /// The rows whose effects run (remembered, not forgotten).
    static LIVE: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    /// How long composing a part takes.
    static PART_TIME: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    /// Each row's focus requester.
    static REQUESTERS: RefCell<HashMap<usize, compose_ui::FocusRequester>> =
        RefCell::new(HashMap::new());
}

fn parts(row: usize) -> usize {
    PARTS.with(|parts| parts.borrow().get(&row).copied().unwrap_or(0))
}

fn live(row: usize) -> bool {
    LIVE.with(|live| live.borrow().contains(&row))
}

/// A part of a row: a restartable composable, so a pause point of its own.
#[composable]
fn Part(row: usize, part: usize) {
    PARTS.with(|parts| *parts.borrow_mut().entry(row).or_insert(0) += 1);
    let time = PART_TIME.get();
    if !time.is_zero() {
        std::thread::sleep(time);
    }
    Box!(modifier = Modifier.size((25.dp(), 20.dp())).test_tag(format!("part {row}.{part}")));
}

/// A 100 x 20 row of four parts, whose effect runs while it's in the composition.
#[composable]
fn ListRow(index: usize) {
    DisposableEffect(index, move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(index));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&index));
        })
    });
    let requester = remember(compose_ui::FocusRequester::new);
    REQUESTERS.with(|requesters| requesters.borrow_mut().insert(index, requester));
    let modifier = Modifier
        .size((100.dp(), 20.dp()))
        .test_tag(format!("row {index}"))
        .focus_requester(requester);
    Row!(modifier = modifier, {
        for part in 0..4 {
            Part(index, part);
        }
    });
}

/// A list of 20 px rows in a 100 px viewport, with `optimizations`.
fn list(optimizations: Optimizations) -> (ComposeTestRule, Owned<LazyListState>) {
    list_from(0, optimizations)
}

/// `list`, scrolled to row `first`.
fn list_from(
    first: usize,
    optimizations: Optimizations,
) -> (ComposeTestRule, Owned<LazyListState>) {
    PARTS.with(|parts| parts.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    PART_TIME.set(Duration::ZERO);
    use_optimizations(optimizations);
    let state = LazyListState::new(first, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, ListRow);
            }
        );
    });
    (ui, state)
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

fn within(millis: u64) -> Instant {
    Instant::now() + Duration::from_millis(millis)
}

fn scroll(ui: &mut ComposeTestRule, state: LazyListState, pixels: f32) {
    state.dispatch_raw_delta(pixels);
    ui.wait_for_idle();
}

fn in_tree(ui: &ComposeTestRule, tag: &str) -> bool {
    ui.dump_tree().contains(&format!("\"{tag}\""))
}

/// Scrolls 6 px at a time (from `at` px, which it moves on) until row `row` is the one after
/// those in view: its request isn't urgent (the row before it ends 14 to 20 px past the
/// viewport, more than 6 px).
fn scroll_until_requested(
    ui: &mut ComposeTestRule,
    state: LazyListState,
    at: &mut i32,
    row: usize,
) {
    while (*at as usize + 99) / 20 + 1 < row {
        *at += 6;
        scroll(ui, state, 6.0);
    }
    assert_eq!((*at as usize + 99) / 20 + 1, row, "row {row} is the next");
}

/// From row 1, 5 px down (rows 1 to 6 in view): row 7 requested (not urgent), its parts taking 4 ms each; then row
/// 6's paused composition in a 6 ms idle time: started (nothing learned yet), paused once out
/// of time. No row left yet, so it's composed anew (not in another's place).
fn first_request_paused() -> (ComposeTestRule, Owned<LazyListState>) {
    let (mut ui, state) = list_from(1, Optimizations::NONE);
    PART_TIME.set(Duration::from_millis(4));
    scroll(&mut ui, *state, 5.0);
    assert!(ui.host().has_prefetch_work(), "row 7 requested");
    assert!(ui.host().prefetch(within(6)), "work left");
    assert!(parts(7) > 0 && parts(7) < 4, "paused after {} parts", parts(7));
    (ui, state)
}

#[test]
fn a_composition_out_of_time_pauses_and_resumes_after_the_next_frame() {
    let (mut ui, state) = first_request_paused();
    // Nothing of it applied: not in the tree, its effect not run.
    assert!(!in_tree(&ui, "row 7"));
    assert!(!in_tree(&ui, "part 7.0"));
    assert!(!live(7));
    ui.host().composition().validate();
    // The next idle time resumes it where it paused, applies it and measures it.
    assert!(!ui.host().prefetch(later()));
    assert_eq!(parts(7), 4, "each part composed once");
    assert!(live(7));
    let dump = ui.dump_tree();
    let line = dump.lines().find(|line| line.contains("\"row 7\"")).expect("row 7 in the tree");
    assert!(line.contains("100x20"), "measured: {line}");
    ui.host().composition().validate();
    // The frame showing it composes nothing.
    scroll(&mut ui, *state, 20.0);
    assert_eq!(parts(7), 4);
    assert!(in_tree(&ui, "part 7.3"));
}

/// A paused item's focus requester focuses nothing until it's applied, as Compose's modifier
/// nodes attach as the composition is applied.
#[test]
fn a_paused_items_focus_requester_attaches_as_it_is_applied() {
    let (mut ui, _state) = first_request_paused();
    let requester = REQUESTERS.with(|requesters| requesters.borrow().get(&7).copied());
    let requester = requester.expect("row 7's requester remembered");
    assert!(!requester.request_focus(), "focused a node not in the tree");
    assert!(!ui.host().prefetch(later()));
    assert!(requester.request_focus());
}

/// Items that left are let go by the next idle time's trimming request while a request's
/// composition is paused: the trim's pass leaves the paused item alone, so it doesn't wait for
/// the request (D592).
#[test]
fn items_leaving_go_while_a_request_is_paused() {
    let (mut ui, state) = first_request_paused();
    assert!(live(1));
    // 15 px more: row 1 leaves the view, row 7 not in it yet.
    scroll(&mut ui, *state, 15.0);
    assert!(live(1), "kept until the next idle time (D575)");
    ui.host().prefetch(within(1));
    assert!(!live(1), "let go in the idle time");
    ui.host().composition().validate();
    while ui.host().prefetch(later()) {}
    assert_eq!(parts(7), 4);
    assert!(live(7));
    ui.host().composition().validate();
}

#[test]
fn a_frame_wanting_a_paused_item_completes_it() {
    let (mut ui, state) = first_request_paused();
    // Row 7 comes into view: the frame completes its composition, applies it, and shows it.
    scroll(&mut ui, *state, 20.0);
    assert_eq!(parts(7), 4, "each part composed once");
    assert!(live(7));
    assert!(in_tree(&ui, "part 7.3"));
    ui.host().composition().validate();
    // Its request has nothing left; the next row's composes.
    assert!(!ui.host().prefetch(later()));
    assert_eq!(parts(7), 4);
    ui.host().composition().validate();
}

#[test]
fn a_turn_cancels_a_paused_composition_and_lets_its_item_go() {
    let (mut ui, state) = first_request_paused();
    let composed = parts(7);
    // 3 px back up: row 7's request is cancelled (in the layout: its content goes before the
    // next pass), row 0 requested, and row 7 let go in that pass, nothing of it composed again.
    scroll(&mut ui, *state, -3.0);
    assert_eq!(parts(7), composed, "not composed again");
    assert!(!live(7), "its effect never ran");
    assert!(!in_tree(&ui, "row 7"));
    ui.host().composition().validate();
    ui.host().prefetch(later());
    ui.host().composition().validate();
    // Down again: row 7 is composed whole, from the start.
    scroll(&mut ui, *state, 25.0);
    assert!(in_tree(&ui, "part 7.3"));
    assert!(live(7));
    assert_eq!(parts(7), composed + 4, "composed again from its start");
    ui.host().composition().validate();
}

#[test]
fn a_composition_starts_only_if_a_stretch_and_a_pause_fit() {
    let (mut ui, state) = list(Optimizations::NONE);
    // Parts of 20 ms: the stretches between pause points average several milliseconds.
    PART_TIME.set(Duration::from_millis(20));
    scroll(&mut ui, *state, 5.0);
    assert!(!ui.host().prefetch(later()));
    let mut at = 5;
    scroll_until_requested(&mut ui, *state, &mut at, 7);
    // 1 ms: less than a stretch's average.
    assert!(ui.host().prefetch(within(1)));
    assert_eq!(parts(7), 0, "not started");
    assert!(ui.host().has_prefetch_work(), "left for the next idle time");
    assert!(!ui.host().prefetch(later()));
    assert_eq!(parts(7), 4);
}

/// Scrolls `steps` times by random amounts, mostly forward, each frame followed by an idle
/// time of a random length, with `optimizations`: after each frame, every row in view shows all
/// its parts and runs its effect (applied), and the composition is consistent.
fn scroll_at_random(optimizations: Optimizations, seed: u64, steps: usize) {
    let (mut ui, state) = list(optimizations);
    PART_TIME.set(Duration::from_micros(300));
    let mut random = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut next = move |bound: u64| {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        random % bound
    };
    for step in 0..steps {
        let pixels = next(60) as f32 - 15.0;
        scroll(&mut ui, *state, pixels);
        let first = state.first_visible_item_index();
        let dump = ui.dump_tree();
        for row in first..first + 5 {
            for part in 0..4 {
                assert!(
                    dump.contains(&format!("\"part {row}.{part}\"")),
                    "seed {seed}, step {step}: row {row} lacks part {part}\n{dump}"
                );
            }
        }
        ui.host().composition().validate();
        // The rows in view are applied: their effects run.
        for row in first..first + 5 {
            assert!(live(row), "seed {seed}, step {step}: row {row}'s effect doesn't run");
        }
        let deadline = Instant::now() + Duration::from_micros(next(3_000));
        ui.host().prefetch(deadline);
        ui.host().composition().validate();
    }
    // Everything settles: the requests run out, nothing left paused.
    while ui.host().prefetch(later()) {}
    ui.host().composition().validate();
}

#[test]
fn compose_rs_prefetch_flags_work_with_pausable_composition() {
    let flags = [
        Optimizations::NONE,
        Optimizations::NONE.with(Optimization::PREFETCH_TWO_AHEAD, true),
        Optimizations::NONE.with(Optimization::KEEP_ITEMS_BEHIND, true),
        Optimizations::NONE.with(Optimization::PREFETCH_AT_REST, true),
        Optimizations::NONE.with(Optimization::PREFETCH_BUDGET_RECENT_MAXIMUM, true),
        Optimizations::NONE
            .with(Optimization::PREFETCH_TWO_AHEAD, true)
            .with(Optimization::KEEP_ITEMS_BEHIND, true)
            .with(Optimization::PREFETCH_AT_REST, true)
            .with(Optimization::PREFETCH_BUDGET_RECENT_MAXIMUM, true),
    ];
    for (index, optimizations) in flags.into_iter().enumerate() {
        for seed in 0..3 {
            scroll_at_random(optimizations, seed * 10 + index as u64, 30);
        }
    }
}

thread_local! {
    static CELLS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static CELLS_AT_MEASURE: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

/// Four parts, then a row of 30 px cells (a `LazyRow`), in a 100 x 100 item.
#[composable]
fn Carousel(row: usize) {
    Column!({
        for part in 0..4 {
            Part(row, part);
        }
        LazyRow!(
            modifier = Modifier
                .layout(move |scope, measurable, constraints: Constraints| {
                    let cells = CELLS.with(|cells| cells.borrow().get(&row).copied().unwrap_or(0));
                    CELLS_AT_MEASURE.with(|at| {
                        at.borrow_mut().entry(row).or_insert(cells);
                    });
                    let placeable = scope.measure(measurable, constraints);
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                })
                .size((100.dp(), 20.dp())),
            |scope: LazyListScope| {
                scope.items(20, move |_cell| {
                    CELLS.with(|cells| *cells.borrow_mut().entry(row).or_insert(0) += 1);
                    Box!(modifier = Modifier.size((30.dp(), 20.dp())));
                });
            }
        );
    });
}

#[test]
fn a_paused_item_prefetches_its_nested_lists_once_applied() {
    CELLS.with(|cells| cells.borrow_mut().clear());
    CELLS_AT_MEASURE.with(|at| at.borrow_mut().clear());
    PARTS.with(|parts| parts.borrow_mut().clear());
    PART_TIME.set(Duration::ZERO);
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 400), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            // The next row, whose size is the viewport, from the first pixel on.
            cache_window = Some(lazy_layout_cache_window_with(100.dp(), 0.dp(), false)),
            |scope: LazyListScope| {
                scope.items(100, Carousel);
            }
        );
    });
    PART_TIME.set(Duration::from_millis(3));
    // Rows are 100 x 100: 5 px down, row 2 requested, learned whole.
    scroll(&mut ui, *state, 5.0);
    assert!(!ui.host().prefetch(later()));
    // 100 px more: rows 1 and 2 show, row 3 requested.
    scroll(&mut ui, *state, 100.0);
    // Row 3 paused on the way: its carousel not composed yet.
    assert!(ui.host().prefetch(within(5)));
    assert!(parts(3) < 4, "{} parts", parts(3));
    assert_eq!(CELLS.with(|cells| cells.borrow().get(&3).copied()), None);
    // Resumed, applied, then its carousel's first cells composed, then measured.
    assert!(!ui.host().prefetch(later()));
    let at = |row: usize| CELLS_AT_MEASURE.with(|at| at.borrow().get(&row).copied());
    assert!(at(3).is_some_and(|cells| cells >= 2), "{:?}", at(3));
    ui.host().composition().validate();
}
