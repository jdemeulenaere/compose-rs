//! Adversarial review of parity mode (D560–D569), its regression tests (D575–D579): a grid's
//! backward line prefetched whole, a focus search's rows let go, no composition round for items
//! leaving, no extra item composed backward, nested prefetch for an item the layout composed,
//! a cancelled precomposed item kept for reuse.

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
    static DISPOSED: Cell<usize> = const { Cell::new(0) };
    static CELLS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static CELLS_AT_MEASURE: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

fn live_items() -> Vec<usize> {
    let mut items: Vec<usize> = LIVE.with(|live| live.borrow().iter().copied().collect());
    items.sort_unstable();
    items
}

fn total() -> usize {
    COMPOSED.with(|composed| composed.borrow().values().sum())
}

fn reset() {
    COMPOSED.with(|composed| composed.borrow_mut().clear());
    LIVE.with(|live| live.borrow_mut().clear());
    DISPOSED.set(0);
    CELLS.with(|cells| cells.borrow_mut().clear());
    CELLS_AT_MEASURE.with(|at| at.borrow_mut().clear());
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// Counts its compositions, notes itself live while composed, and its disposal (not a
/// deactivation for reuse, which keeps its remembered value).
#[composable]
fn Cell3(index: usize) {
    COMPOSED.with(|composed| *composed.borrow_mut().entry(index).or_insert(0) += 1);
    DisposableEffect(index, move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(index));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&index));
        })
    });
    Box!(modifier = Modifier.size((100.dp(), 20.dp())));
}

fn grid() -> (ComposeTestRule, Owned<LazyGridState>) {
    reset();
    use_optimizations(Optimizations::NONE);
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 100.dp())),
            state = given,
            |scope: LazyGridScope| {
                scope.items(900, Cell3);
            }
        );
    });
    (ui, state)
}

fn list(optimizations: Optimizations) -> (ComposeTestRule, Owned<LazyListState>) {
    reset();
    use_optimizations(optimizations);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, Cell3);
            }
        );
    });
    (ui, state)
}

/// Compose's grid prefetches the whole line before the first in view (`scheduleLinePrefetch`):
/// each of its items is composed next to the composed ones, from the line's end backward.
#[test]
fn a_grid_scrolled_back_prefetches_the_whole_line_before() {
    let (mut ui, state) = grid();
    state.scroll_by(200.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    // At 195 px: lines 9 (items 27 to 29) to 14 show; line 8 (items 24 to 26) is the one before.
    state.scroll_by(-5.0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 27);
    ui.host().prefetch(later());
    let live = live_items();
    assert!((24..27).all(|item| live.contains(&item)), "line 8 prefetched whole: {live:?}");
    let before = total();
    state.scroll_by(-10.0);
    ui.wait_for_idle();
    assert_eq!(total(), before, "items composed in the frame showing line 8");
}

#[test]
fn a_grid_scrolled_back_line_by_line_composes_nothing_in_its_frames() {
    let (mut ui, state) = grid();
    state.scroll_by(400.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    let mut in_frames = Vec::new();
    for _ in 0..12 {
        let before = total();
        state.scroll_by(-7.0);
        ui.wait_for_idle();
        in_frames.push(total() - before);
        ui.host().prefetch(later());
    }
    // The first frame turns (its line before wasn't prefetched yet).
    assert!(in_frames[1..].iter().all(|composed| *composed == 0), "{in_frames:?}");
}

#[test]
fn a_list_scrolled_back_composes_nothing_in_its_frames_after_the_turn() {
    let (mut ui, state) = list(Optimizations::NONE);
    for _ in 0..10 {
        state.scroll_by(15.0);
        ui.wait_for_idle();
        ui.host().prefetch(later());
    }
    assert!(live_items().contains(&13), "{:?}", live_items());
    let mut in_frames = Vec::new();
    for _ in 0..10 {
        let before = total();
        state.scroll_by(-15.0);
        ui.wait_for_idle();
        in_frames.push(total() - before);
        ui.host().prefetch(later());
    }
    // The turn's frame composes the one row that comes into view, as Compose's measurement
    // does, not two (the estimate's extra one); none after.
    assert_eq!(in_frames[0], 1, "{in_frames:?}");
    assert!(in_frames[1..].iter().all(|composed| *composed == 0), "{in_frames:?}");
}

#[test]
fn a_grid_scrolled_back_composes_only_the_line_that_comes_into_view() {
    let (mut ui, state) = grid();
    state.scroll_by(400.0);
    ui.wait_for_idle();
    // The idle time after deactivates the lines the frame passed (Compose's measurement
    // composes them to step over, and deactivates them).
    ui.host().prefetch(later());
    // The frame scrolled back 7 px composes line 19, three items, not two lines.
    let before = total();
    state.scroll_by(-7.0);
    ui.wait_for_idle();
    assert_eq!(total() - before, 3);
}

/// An item leaving, none coming (the next one prefetched): Compose's measurement deactivates it
/// without composing; here no composition round runs in that frame either (the item is
/// unplaced, and deactivated by the next composition, a prefetch's or the next frame's).
#[test]
fn a_frame_where_an_item_only_leaves_runs_no_composition() {
    let (mut ui, state) = list(Optimizations::NONE);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    assert!(live_items().contains(&7));
    state.scroll_by(20.0);
    let stats = ui.host().frame();
    assert!(!stats.recomposed, "a composition round for row 1 leaving");
    ui.wait_for_idle();
    // Not shown, and deactivated by the next idle time's work.
    assert!(ui.host().has_prefetch_work());
    ui.host().prefetch(later());
    assert!(!live_items().contains(&1), "{:?}", live_items());
}

/// Small items in view, large ones after: the estimate composes more than the frame places;
/// those beyond the next one aren't kept.
#[test]
fn items_an_estimate_composed_beyond_the_next_one_are_let_go() {
    reset();
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 600), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 500.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    COMPOSED.with(|composed| *composed.borrow_mut().entry(index).or_insert(0) += 1);
                    DisposableEffect(index, move |scope| {
                        LIVE.with(|live| live.borrow_mut().insert(index));
                        scope.on_dispose(move || {
                            LIVE.with(|live| live.borrow_mut().remove(&index));
                        })
                    });
                    let height = if index < 10 { 10.0 } else { 300.0 };
                    Box!(modifier = Modifier.size((100.dp(), height.dp())));
                });
            }
        );
    });
    state.scroll_by(60.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    let live = live_items();
    assert!(*live.last().unwrap() <= 13, "composed ahead: {live:?}");
}

/// D482 at parity: a focus search that laid rows out beyond the bounds lets them go once over.
fn focus_search_list(
    optimizations: Optimizations,
    rows: usize,
    target: Option<usize>,
) -> ComposeTestRule {
    reset();
    use_optimizations(optimizations);
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 300), move || {
        Column!({
            LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
                scope.items(rows, move |index| {
                    DisposableEffect(index, move |scope| {
                        LIVE.with(|live| live.borrow_mut().insert(index));
                        scope.on_dispose(move || {
                            LIVE.with(|live| live.borrow_mut().remove(&index));
                        })
                    });
                    let modifier =
                        Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}"));
                    let modifier = if index == 0 || Some(index) == target {
                        modifier.clickable(|| {})
                    } else {
                        modifier
                    };
                    Box!(modifier = modifier);
                });
            });
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    });
    ui.on_node_with_tag("row 0").request_focus();
    ui.wait_for_idle();
    ui.press_key(compose_ui::Key::Tab);
    for _ in 0..60 {
        ui.host().frame();
    }
    ui.wait_for_idle();
    ui
}

#[test]
fn a_focus_search_finding_a_row_far_on_lets_the_rows_it_laid_out_go() {
    let mut ui = focus_search_list(Optimizations::NONE, 200, Some(51));
    ui.on_node_with_tag("row 51").assert_is_focused();
    let live = live_items();
    assert!(live.len() <= 7, "{live:?}");
}

#[test]
fn a_focus_search_leaving_the_list_lets_the_rows_it_laid_out_go() {
    for optimizations in [Optimizations::NONE, crate::legacy::optimizations()] {
        let mut ui = focus_search_list(optimizations, 41, None);
        ui.on_node_with_tag("after").assert_is_focused();
        let live = live_items();
        assert!(live.len() <= 7, "{optimizations:?}: {live:?}");
    }
}

/// A row of 30 px cells (a `LazyRow`) in a 100 x 20 item, noting how many of its cells were
/// composed when it was first measured.
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

/// A request for an item already composed (kept behind, `KEEP_ITEMS_BEHIND`) still prefetches
/// its nested lists and learns their count, as Compose's request does for its precomposed item.
#[test]
fn a_request_for_an_item_already_composed_learns_its_nested_lists_count() {
    reset();
    use_optimizations(Optimizations::NONE.with(Optimization::KEEP_ITEMS_BEHIND, true));
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
    // 60 px down (rows 1 and 2 kept behind), no prefetch; then 5 px back: row 2, composed, is
    // requested, and its carousel shows 4 cells.
    state.scroll_by(60.0);
    ui.wait_for_idle();
    state.scroll_by(-5.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    // Forward again: a row never composed is requested; its carousel's 4 cells (the count
    // learned) are composed before it's measured, not 2.
    state.scroll_by(25.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    let at = |row: usize| CELLS_AT_MEASURE.with(|at| at.borrow().get(&row).copied());
    assert_eq!(at(9), Some(4));
}

/// A nested `LazyRow` remembered scrolled to its 5th cell: nested prefetch composes from its
/// first visible item, and the frame showing the row composes no cell.
#[test]
fn a_nested_list_starting_scrolled_is_prefetched_from_its_first_visible_item() {
    reset();
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, |row| {
                    let inner = remember_lazy_list_state(5, 0);
                    LazyRow!(
                        modifier = Modifier.size((100.dp(), 20.dp())),
                        state = inner,
                        |scope: LazyListScope| {
                            scope.items(20, move |_cell| {
                                CELLS
                                    .with(|cells| *cells.borrow_mut().entry(row).or_insert(0) += 1);
                                Box!(modifier = Modifier.size((30.dp(), 20.dp())));
                            });
                        }
                    );
                });
            }
        );
    });
    state.scroll_by(5.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    let cells = || CELLS.with(|cells| cells.borrow().values().sum::<usize>());
    let before = cells();
    state.scroll_by(20.0);
    ui.host().frame();
    assert_eq!(cells(), before, "cells composed in the frame showing the row");
}

thread_local! {
    /// How long rows take to compose and to measure, when set.
    static SLOW: Cell<Option<(Duration, Duration)>> = const { Cell::new(None) };
}

#[composable]
fn SlowRow(index: usize) {
    if let Some((compose, _)) = SLOW.get() {
        std::thread::sleep(compose);
    }
    Box!(
        modifier = Modifier
            .layout(|scope, measurable, constraints: Constraints| {
                if let Some((_, measure)) = SLOW.get() {
                    std::thread::sleep(measure);
                }
                let placeable = scope.measure(measurable, constraints);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            })
            .size((100.dp(), 20.dp()))
            .test_tag(format!("row {index}"))
    );
}

/// A request cancelled after composing its item, before measuring it: the item goes to the
/// reuse pool, as Compose's precomposed slot does (`PrecomposedSlotHandle.dispose`), rather than
/// being disposed as an item never measured is (D460).
#[test]
fn an_item_composed_ahead_and_cancelled_is_kept_for_reuse() {
    reset();
    use_optimizations(Optimizations::NONE);
    SLOW.set(None);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, SlowRow);
            }
        );
    });
    // Composing a row takes 3 ms, measuring it 6 ms. 5 px down: row 6 prefetched whole (the
    // averages' first samples).
    SLOW.set(Some((Duration::from_millis(3), Duration::from_millis(6))));
    state.scroll_by(5.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    // 18 px more, 6 px a frame: row 7 requested, not urgent; 7 ms composes it, and leaves too
    // little to measure it.
    for _ in 0..3 {
        state.scroll_by(6.0);
        ui.wait_for_idle();
    }
    assert!(ui.host().prefetch(Instant::now() + Duration::from_millis(7)), "measuring left");
    SLOW.set(None);
    let list = |ui: &mut ComposeTestRule| {
        let row = ui.on_node_with_tag("row 2").node_id();
        ui.host().with_tree(|tree| {
            let item = tree.parent_of(row).expect("an item");
            let list = tree.parent_of(item).expect("the list");
            tree.children_of(list).count()
        })
    };
    let children = list(&mut ui);
    // 3 px back: row 7's request is cancelled, row 0's composes in its place; row 7 is kept.
    state.scroll_by(-3.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    assert_eq!(list(&mut ui), children, "row 7 disposed rather than kept for reuse");
}

thread_local! {
    static SLOW_CELLS: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// A cell 40, 20 or 10 px tall by its column, composing in `SLOW_CELLS`.
#[composable]
fn Uneven(index: usize) {
    COMPOSED.with(|composed| *composed.borrow_mut().entry(index).or_insert(0) += 1);
    std::thread::sleep(SLOW_CELLS.get());
    let height = [40.0, 20.0, 10.0][index % 3];
    Box!(modifier = Modifier.size((100.dp(), height.dp())));
}

/// A grid's request is urgent by its last visible item's own size, as Compose's
/// (`lastItem.offset + lastItem.size + spacing - viewportEndOffset < -delta`), not its line's:
/// a short last item makes it urgent sooner.
#[test]
fn a_grids_urgency_goes_by_the_last_visible_items_size() {
    reset();
    use_optimizations(Optimizations::NONE);
    SLOW_CELLS.set(Duration::ZERO);
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 100.dp())),
            state = given,
            |scope: LazyGridScope| {
                scope.items(900, Uneven);
            }
        );
    });
    SLOW_CELLS.set(Duration::from_millis(25));
    // 5 px down: line 3 (items 9 to 11) requested and composed, the average's first sample.
    state.scroll_by(5.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    // 10 px more twice: line 3 shows from 95 px; its last item, 10 px tall, ends 5 px past the
    // viewport, less than the 10 px scrolled: line 4's requests are urgent (by the line's 40 px,
    // they wouldn't be) and compose with 5 ms left.
    for _ in 0..2 {
        state.scroll_by(10.0);
        ui.wait_for_idle();
    }
    ui.host().prefetch(Instant::now() + Duration::from_millis(5));
    SLOW_CELLS.set(Duration::ZERO);
    assert_eq!(COMPOSED.with(|composed| composed.borrow().get(&12).copied()), Some(1));
}
