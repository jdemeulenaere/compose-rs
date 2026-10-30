//! Prefetch as Compose 1.12.1's, the default (D560): one item ahead in the scroll direction and
//! none behind; a request cancelled when the direction turns, or once its item isn't the next
//! one; urgent when the same scroll again would reach the item; each step only while the time
//! left exceeds that step's average for the item's content type (any time left when urgent), a
//! step that doesn't fit leaving the request for the next idle time; a request composing the
//! first items of the lazy lists in its item. And each of compose-rs's own behaviours, switched
//! on by its optimization (D560-D565).

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
    static COMPOSED: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static LIVE: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    /// How long composing an even (odd) row takes.
    static SLOW_EVEN: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    static SLOW_ODD: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

fn compositions(index: usize) -> usize {
    COMPOSED.with(|composed| composed.borrow().get(&index).copied().unwrap_or(0))
}

fn live_items() -> Vec<usize> {
    let mut items: Vec<usize> = LIVE.with(|live| live.borrow().iter().copied().collect());
    items.sort_unstable();
    items
}

#[composable]
fn Row(index: usize) {
    COMPOSED.with(|composed| *composed.borrow_mut().entry(index).or_insert(0) += 1);
    let slow = if index.is_multiple_of(2) { SLOW_EVEN.get() } else { SLOW_ODD.get() };
    if !slow.is_zero() {
        std::thread::sleep(slow);
    }
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

/// A list of 20 px rows in a 100 px viewport (rows of two content types, even and odd), with
/// `optimizations` (Compose's: none).
fn list(optimizations: Optimizations) -> (ComposeTestRule, Owned<LazyListState>) {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    use_optimizations(optimizations);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items_with(1000, |index| index % 2, Row);
            }
        );
    });
    (ui, state)
}

fn with(name: Optimization) -> Optimizations {
    Optimizations::NONE.with(name, true)
}

/// Compose's prefetch without pausable composition (its flag off): a request composes its item
/// whole, gated by the whole composition's average. Restored when dropped.
pub(crate) struct WholeCompositions;

impl WholeCompositions {
    pub(crate) fn new() -> WholeCompositions {
        ComposeFoundationFlags::set_pausable_composition_in_prefetch_enabled(false);
        WholeCompositions
    }
}

impl Drop for WholeCompositions {
    fn drop(&mut self) {
        ComposeFoundationFlags::set_pausable_composition_in_prefetch_enabled(true);
    }
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

fn scroll(ui: &mut ComposeTestRule, state: LazyListState, pixels: f32) {
    state.dispatch_raw_delta(pixels);
    ui.wait_for_idle();
}

#[test]
fn a_scroll_prefetches_one_item_ahead_and_keeps_none_behind() {
    let (mut ui, state) = list(Optimizations::NONE);
    // At rest, nothing: Compose prefetches on scrolls.
    assert!(!ui.host().has_prefetch_work());
    assert_eq!(live_items(), (0..5).collect::<Vec<_>>());
    // 30 px down: rows 1 to 6 show (row 6 from 90 px); the frame composing rows 5 and 6
    // deactivates row 0 in the same composition.
    scroll(&mut ui, *state, 30.0);
    assert_eq!(live_items(), (1..7).collect::<Vec<_>>(), "row 0 deactivated in the frame");
    assert!(ui.host().has_prefetch_work());
    assert!(!ui.host().prefetch(later()));
    assert_eq!(live_items(), (1..8).collect::<Vec<_>>(), "row 7, and only it");
    assert_eq!(compositions(7), 1);
    // Composed and measured, not placed.
    let dump = ui.dump_tree();
    let line = dump.lines().find(|line| line.contains("\"row 7\"")).expect("row 7 in the tree");
    assert!(line.contains("100x20"), "measured: {line}");
    assert_eq!(state.first_visible_item_index(), 1);
    // The frame showing it composes nothing: row 1, unplaced, is deactivated by the next idle
    // time's composition, which prefetches row 8 (D575).
    let composed: usize = COMPOSED.with(|composed| composed.borrow().values().sum());
    scroll(&mut ui, *state, 20.0);
    assert_eq!(COMPOSED.with(|composed| composed.borrow().values().sum::<usize>()), composed);
    assert!(!ui.host().prefetch(later()));
    assert_eq!(live_items(), (2..9).collect::<Vec<_>>());
}

#[test]
fn prefetch_two_ahead_prefetches_two_items() {
    let (mut ui, state) = list(with(Optimization::PREFETCH_TWO_AHEAD));
    scroll(&mut ui, *state, 30.0);
    assert!(!ui.host().prefetch(later()));
    assert_eq!(live_items(), (1..9).collect::<Vec<_>>(), "rows 7 and 8");
}

#[test]
fn keep_items_behind_keeps_two_behind_the_scroll() {
    let (mut ui, state) = list(with(Optimization::KEEP_ITEMS_BEHIND));
    scroll(&mut ui, *state, 30.0);
    assert!(live_items().contains(&0), "row 0 kept: {:?}", live_items());
    for _ in 0..4 {
        scroll(&mut ui, *state, 30.0);
        ui.host().prefetch(later());
    }
    // Rows 7 to 12 show (from 150 px); two behind them stay.
    let first = state.first_visible_item_index();
    let live = live_items();
    assert!(live.contains(&(first - 1)) && live.contains(&(first - 2)), "{live:?}");
    assert!(!live.contains(&(first - 3)), "two at most: {live:?}");
    // Compose's: none.
    let (mut ui, state) = list(Optimizations::NONE);
    for _ in 0..5 {
        scroll(&mut ui, *state, 30.0);
        ui.host().prefetch(later());
    }
    let first = state.first_visible_item_index();
    assert_eq!(live_items()[0], first, "none behind: {:?}", live_items());
}

#[test]
fn prefetch_at_rest_prefetches_after_the_first_layout() {
    let (mut ui, _state) = list(with(Optimization::PREFETCH_AT_REST));
    assert!(ui.host().has_prefetch_work(), "the row after those in view");
    assert!(!ui.host().prefetch(later()));
    assert_eq!(live_items(), (0..6).collect::<Vec<_>>());
}

#[test]
fn a_turn_cancels_the_request_ahead_and_requests_the_item_before() {
    let (mut ui, state) = list(Optimizations::NONE);
    scroll(&mut ui, *state, 30.0);
    ui.host().prefetch(later());
    assert!(live_items().contains(&7));
    // 5 px back up: rows 1 to 6 still show; row 7's request is cancelled, row 0 requested.
    scroll(&mut ui, *state, -5.0);
    assert!(ui.host().has_prefetch_work());
    assert!(!ui.host().prefetch(later()));
    // Row 0 composed, and row 7 deactivated (kept for reuse) in the same composition.
    assert_eq!(live_items(), (0..7).collect::<Vec<_>>(), "row 0 now, row 7 let go");
}

#[test]
fn a_request_for_an_item_no_longer_next_is_cancelled() {
    let (mut ui, state) = list(Optimizations::NONE);
    // 5 px down: rows 0 to 5 show, row 6 requested.
    scroll(&mut ui, *state, 5.0);
    assert!(ui.host().has_prefetch_work());
    // Before it runs, 60 px more: rows 3 to 8 show; row 9 is the next.
    scroll(&mut ui, *state, 60.0);
    assert_eq!(state.first_visible_item_index(), 3);
    assert!(!ui.host().prefetch(later()));
    assert_eq!(live_items(), (3..10).collect::<Vec<_>>());
}

/// Without pausable composition: a composition (a row's whole, 25 ms) starts only once the time
/// left exceeds the whole composition's average (`prefetch_pausable` has the pausable gate).
#[test]
fn an_urgent_request_composes_with_any_time_left() {
    let _whole = WholeCompositions::new();
    let (mut ui, state) = list(Optimizations::NONE);
    SLOW_EVEN.set(Duration::from_millis(25));
    SLOW_ODD.set(Duration::from_millis(25));
    // 5 px down: row 6 requested, not urgent (row 5 ends 15 px past the viewport, more than
    // 5 px); its composition's first sample is taken as is: about 25 ms.
    scroll(&mut ui, *state, 5.0);
    assert!(!ui.host().prefetch(later()));
    assert_eq!(compositions(6), 1);
    // 30 px more: rows 1 to 6 show, row 6 ending 5 px past the viewport: the same scroll again
    // reaches row 7, so its request is urgent, and composes with 5 ms left.
    scroll(&mut ui, *state, 30.0);
    assert!(ui.host().prefetch(Instant::now() + Duration::from_millis(5)), "measuring is left");
    assert_eq!(compositions(7), 1);
    // 6 px more: row 7 shows, ending 19 px past the viewport: row 8's request isn't urgent, and
    // 5 ms is less than the average.
    scroll(&mut ui, *state, 6.0);
    assert_eq!(state.first_visible_item_index(), 2);
    assert!(ui.host().prefetch(Instant::now() + Duration::from_millis(5)), "left for later");
    assert_eq!(compositions(8), 0);
    assert!(ui.host().has_prefetch_work(), "the request stays queued");
    // The next idle time with time for it composes it.
    assert!(!ui.host().prefetch(later()));
    assert_eq!(compositions(8), 1);
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

#[test]
fn prefetch_budget_recent_maximum_gives_a_request_up_until_the_next_scroll() {
    let (mut ui, state) = list(with(Optimization::PREFETCH_BUDGET_RECENT_MAXIMUM));
    SLOW_EVEN.set(Duration::from_millis(25));
    SLOW_ODD.set(Duration::from_millis(25));
    let mut at = 0;
    scroll_until_requested(&mut ui, *state, &mut at, 6);
    assert!(!ui.host().prefetch(later()));
    assert_eq!(compositions(6), 1);
    // Row 7's request, which 5 ms doesn't fit: given up, not queued.
    scroll_until_requested(&mut ui, *state, &mut at, 7);
    assert!(!ui.host().prefetch(Instant::now() + Duration::from_millis(5)));
    assert!(!ui.host().has_prefetch_work());
    assert_eq!(compositions(7), 0);
    // A scroll asks for it again.
    scroll(&mut ui, *state, 1.0);
    assert!(ui.host().has_prefetch_work());
    assert!(!ui.host().prefetch(later()));
    assert_eq!(compositions(7), 1);
}

/// Without pausable composition, as `an_urgent_request_composes_with_any_time_left`.
#[test]
fn each_content_type_has_its_own_averages() {
    let _whole = WholeCompositions::new();
    let (mut ui, state) = list(Optimizations::NONE);
    // Even rows take 25 ms to compose, odd ones no time. Rows 6 (even) and 7 (odd) prefetched:
    // each type has a sample.
    SLOW_EVEN.set(Duration::from_millis(25));
    let mut at = 0;
    scroll_until_requested(&mut ui, *state, &mut at, 6);
    assert!(!ui.host().prefetch(later()));
    scroll_until_requested(&mut ui, *state, &mut at, 7);
    assert!(!ui.host().prefetch(later()));
    assert_eq!((compositions(6), compositions(7)), (1, 1));
    // Row 8 (even): 5 ms is less than its type's average.
    scroll_until_requested(&mut ui, *state, &mut at, 8);
    assert!(ui.host().prefetch(Instant::now() + Duration::from_millis(5)));
    assert_eq!(compositions(8), 0);
    assert!(!ui.host().prefetch(later()));
    // Row 9 (odd): its type's average is far less than 5 ms.
    scroll_until_requested(&mut ui, *state, &mut at, 9);
    assert!(!ui.host().prefetch(Instant::now() + Duration::from_millis(5)));
    assert_eq!(compositions(9), 1);
}

thread_local! {
    /// Each row's cells composed, and how many were when its `LazyRow` was first measured.
    static CELLS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static CELLS_AT_MEASURE: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

/// A row of 30 px cells (a `LazyRow`) in a 100 x 20 item: four show.
#[composable]
fn Carousel(row: usize) {
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
}

#[test]
fn a_request_composes_its_nested_lists_first_items_before_measuring() {
    CELLS.with(|cells| cells.borrow_mut().clear());
    CELLS_AT_MEASURE.with(|at| at.borrow_mut().clear());
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, Carousel);
            }
        );
    });
    // 5 px down: row 6 requested. Its carousel's first two cells are composed before the row
    // is measured (none measured of this type yet: Compose's two).
    scroll(&mut ui, *state, 5.0);
    assert!(!ui.host().prefetch(later()));
    let at = |row: usize| CELLS_AT_MEASURE.with(|at| at.borrow().get(&row).copied());
    assert_eq!(at(6), Some(2));
    // Then as many as the carousels show, on average: four.
    scroll(&mut ui, *state, 20.0);
    assert!(!ui.host().prefetch(later()));
    assert_eq!(at(7), Some(4));
    // The frame showing them composes no cell.
    let cells = CELLS.with(|cells| cells.borrow().values().sum::<usize>());
    scroll(&mut ui, *state, 20.0);
    assert_eq!(CELLS.with(|cells| cells.borrow().values().sum::<usize>()), cells);
}

#[test]
fn draw_prefetched_ahead_notes_the_items_measured_ahead() {
    let (mut ui, state) = list(Optimizations::NONE);
    scroll(&mut ui, *state, 30.0);
    ui.host().prefetch(later());
    assert!(!ui.host().has_draw_ahead_work(), "Compose draws nothing ahead");
    let (mut ui, state) = list(with(Optimization::DRAW_PREFETCHED_AHEAD));
    scroll(&mut ui, *state, 30.0);
    ui.host().prefetch(later());
    assert!(ui.host().has_draw_ahead_work());
    assert!(ui.host().draw_ahead().is_some());
}

#[test]
fn keep_items_behind_lists_the_kept_items_groups() {
    for (optimizations, kept) in
        [(Optimizations::NONE, false), (with(Optimization::KEEP_ITEMS_BEHIND), true)]
    {
        let (mut ui, state) = list(optimizations);
        ui.host().set_keep_unplaced_groups(true);
        scroll(&mut ui, *state, 30.0);
        ui.host().prefetch(later());
        scroll(&mut ui, *state, 30.0);
        let listed = ui.host().with_tree(|tree| !tree.display_list().kept.is_empty());
        assert_eq!(listed, kept, "{optimizations:?}");
    }
}

/// A 300 x 100 grid of 100 x 20 cells, three to a line.
fn grid(optimizations: Optimizations) -> (ComposeTestRule, Owned<LazyGridState>) {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    use_optimizations(optimizations);
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 100.dp())),
            state = given,
            |scope: LazyGridScope| {
                scope.items(900, Row);
            }
        );
    });
    (ui, state)
}

#[test]
fn a_grid_prefetches_the_next_line_and_composes_the_lines_in_view() {
    let (mut ui, state) = grid(Optimizations::NONE);
    // Five lines show: items 0 to 14, and at most the line its estimate composed after them.
    let live = live_items();
    assert!((0..15).all(|item| live.contains(&item)) && live.len() <= 18, "{live:?}");
    assert!(!ui.host().has_prefetch_work());
    // 30 px down: lines 1 to 6 show; line 7 (items 21 to 23) prefetched, line 0 gone.
    state.dispatch_raw_delta(30.0);
    ui.wait_for_idle();
    assert_eq!(live_items(), (3..21).collect::<Vec<_>>());
    assert!(!ui.host().prefetch(later()));
    assert_eq!(live_items(), (3..24).collect::<Vec<_>>());
    // Two lines beyond each side with `GRID_LINES_BEYOND`.
    let (_ui, _state) = grid(with(Optimization::GRID_LINES_BEYOND));
    let live = live_items();
    assert!((0..21).all(|item| live.contains(&item)), "{live:?}");
}

#[test]
fn paragraph_render_nodes_draws_everything_again_when_it_changes() {
    let (mut ui, _state) = list(Optimizations::NONE);
    assert!(!ui.host().frame().drew);
    ui.host().set_optimizations(with(Optimization::PARAGRAPH_RENDER_NODES));
    assert!(ui.host().frame().drew, "paragraphs drawn in their own nodes from now on");
    // One that only Kotlin reads and that draws nothing differently: nothing.
    ui.host().set_optimizations(
        with(Optimization::PARAGRAPH_RENDER_NODES)
            .with(Optimization::FLUSH_LAYERS_BEFORE_DROPPED_PATHS, true),
    );
    assert!(!ui.host().frame().drew);
}

/// With a content padding, the items under it are among those shown, as Compose's
/// `visibleItemsInfo`: a scroll back prefetches the item before the first one laid out under
/// the start padding, not the one before the first visible item (which the layout already
/// laid out there).
#[test]
fn a_scroll_back_prefetches_the_item_before_those_under_the_start_padding() {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            content_padding = PaddingValues::symmetric(0.dp(), 30.dp()),
            |scope: LazyListScope| {
                scope.items_with(1000, |index| index % 2, Row);
            }
        );
    });
    // Rows 0 to 3 from 30 px, row 3 under the end padding.
    assert_eq!(live_items(), (0..4).collect::<Vec<_>>());
    // 70 px down: row 3 first visible, 10 px into it, row 2 under the start padding at the top,
    // rows 2 to 6 shown; row 7 prefetched.
    scroll(&mut ui, *state, 70.0);
    assert_eq!(live_items(), (2..7).collect::<Vec<_>>());
    assert!(!ui.host().prefetch(later()));
    assert_eq!(live_items(), (2..8).collect::<Vec<_>>());
    // 5 px back: row 3 still first visible, rows 2 (at 5 px) and 1 (at -15 px) under the start
    // padding, laid out by the layout itself; row 0, before row 1, is the one prefetched.
    scroll(&mut ui, *state, -5.0);
    assert_eq!(state.first_visible_item_index(), 3);
    assert_eq!(ui.on_node_with_tag("row 2").fetch_semantics_node().bounds.top, 5.0);
    assert_eq!(ui.on_node_with_tag("row 1").fetch_semantics_node().bounds.top, -15.0);
    assert!(!live_items().contains(&0), "{:?}", live_items());
    assert!(!ui.host().prefetch(later()));
    assert!(live_items().contains(&0), "{:?}", live_items());
}

/// Prefetching and drawing ahead are turns, as the host's other entry points (D751, D755): what
/// an item's effect writes, or its drawing, asks for no tick of the platform. Without their
/// turns the write would, and show one tick later.
#[test]
fn prefetching_and_drawing_ahead_are_turns() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let written = mutable_state_of(0);
    let handle = written.handle();
    let writes = std::rc::Rc::new(Cell::new(0));
    // Writes without reading: a drawing that read it would invalidate itself.
    let write = move || {
        writes.set(writes.get() + 1);
        handle.set(writes.get());
    };
    let state = LazyListState::new(0, 0);
    let given = *state;
    use_optimizations(with(Optimization::DRAW_PREFETCHED_AHEAD));
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let write = write.clone();
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                let write = write.clone();
                scope.items(1000, move |_| {
                    let (effect, draw) = (write.clone(), write.clone());
                    SideEffect(effect);
                    Box!(
                        modifier = Modifier.size((100.dp(), 20.dp())).draw_behind(move |_| draw()),
                        {}
                    );
                });
            }
        );
    });
    state.dispatch_raw_delta(30.0);
    ui.wait_for_idle();
    let wakes = std::sync::Arc::new(AtomicUsize::new(0));
    let counted = wakes.clone();
    compose_runtime::platform::set_wake_handler(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    let before = written.get();
    ui.host().prefetch(later());
    assert_ne!(written.get(), before, "an item's effect ran");
    assert_eq!(wakes.swap(0, Ordering::SeqCst), 0, "prefetching is a turn");
    assert!(ui.host().has_draw_ahead_work());
    let before = written.get();
    assert!(ui.host().draw_ahead().is_some());
    assert_ne!(written.get(), before, "an item was drawn");
    assert_eq!(wakes.swap(0, Ordering::SeqCst), 0, "drawing ahead is a turn");
}
