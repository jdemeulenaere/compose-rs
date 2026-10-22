//! Prefetch review (D156): failing tests for `UiHost::prefetch`, `PrefetchScheduler` and the
//! lazy list's `LazyListPrefetch`. Each test states the input, what Jetpack Compose does (or
//! what the commit's docs promise), and what happens instead.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Constraints, IntSize, Measurable, MeasurePolicy, MeasureResult, MeasureScope, UnitExt,
};

thread_local! {
    static COMPOSED: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static MEASURED: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static CELLS: Cell<usize> = const { Cell::new(0) };
    static SLOW: Cell<Option<usize>> = const { Cell::new(None) };
}

fn reset() {
    COMPOSED.with(|map| map.borrow_mut().clear());
    MEASURED.with(|map| map.borrow_mut().clear());
    CELLS.with(|cells| cells.set(0));
    SLOW.with(|slow| slow.set(None));
}

fn compositions(index: usize) -> usize {
    COMPOSED.with(|map| map.borrow().get(&index).copied().unwrap_or(0))
}

fn total_compositions() -> usize {
    COMPOSED.with(|map| map.borrow().values().sum())
}

fn last_composed() -> usize {
    COMPOSED.with(|map| map.borrow().keys().copied().max().unwrap_or(0))
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// A 100 x 20 leaf counting its measurements.
struct Counting(usize);

impl MeasurePolicy for Counting {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _measurables: &[Measurable],
        _constraints: Constraints,
    ) -> MeasureResult {
        MEASURED.with(|map| *map.borrow_mut().entry(self.0).or_insert(0) += 1);
        scope.layout(100, 20)
    }
}

#[composable]
fn Item(index: usize) {
    COMPOSED.with(|map| *map.borrow_mut().entry(index).or_insert(0) += 1);
    if SLOW.with(|slow| slow.get()) == Some(index) {
        SLOW.with(|slow| slow.set(None));
        std::thread::sleep(Duration::from_millis(40));
    }
    let policy = remember(|| Rc::new(Counting(index)) as Rc<dyn MeasurePolicy>);
    compose_ui::Layout(
        Modifier.test_tag(format!("row {index}")),
        policy,
        compose_runtime::callback(|| {}),
    );
}

#[composable]
fn List(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items(1000, |index| Item(index));
        }
    );
}

fn list(first: usize) -> (ComposeTestRule, Owned<LazyListState>) {
    reset();
    let state = LazyListState::new(first, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || List(given));
    (ui, state)
}

// ---- Findings -------------------------------------------------------------------------------

/// Input: a list scrolled, then `UiHost::prefetch` between frames.
///
/// The commit's docs (`UiHost::prefetch`): what it composes "asks for none" (no frame), as
/// Compose's prefetch, which never schedules a frame. Instead the host is left with work: the
/// request's write of the composed range set `frame_requested` while `framing` (so no waker was
/// called), and inserting the items marked the tree to draw. Desktop's next `about_to_wait`
/// therefore redraws (a full layout and draw of a list nobody scrolled), and a `prefetch` that
/// ran out of time is never resumed in idle time (`has_pending_work` gates it): a frame always
/// goes first. On Android the stale flag stays set with no frame scheduled.
#[test]
fn prefetch_leaves_the_host_without_pending_work() {
    let (mut ui, state) = list(0);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    assert!(!ui.host().has_pending_work());
    assert!(!ui.host().prefetch(later()));
    assert!(!ui.host().has_pending_work(), "prefetch asked for a frame");
}

/// Input: after a scroll (a request queued), the list is scrolled again (`scroll_by`, as a drag
/// or the wheel between frames), then the platform's idle time runs `prefetch` before the frame.
///
/// The commit's docs: "A frame with work waiting (state changed) goes first: nothing runs then".
/// Instead only composition invalidations are checked: a scroll (a write read by layout only,
/// `remeasure`) doesn't stop it, so prefetch composes the items ahead of where the list *was*,
/// with the stale `wanted` range, while the frame for the new position waits (Android posts the
/// prefetch right after the frame, so any input in between lands here).
#[test]
fn prefetch_waits_for_a_pending_scroll() {
    let (mut ui, state) = list(0);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_composed();
    state.scroll_by(200.0);
    assert!(ui.host().has_pending_work(), "a frame is due");
    ui.host().prefetch(later());
    assert_eq!(compositions(last + 1), 0, "prefetch ran before the due frame");
}

/// Input: one item whose first composition is slow (40 ms: an image decoded, a cold text cache)
/// is the first item a list prefetches; the list then keeps scrolling a row at a time, with
/// 16 ms of idle time between frames (60 Hz), its other items composing in microseconds.
///
/// Compose: the per-content-type averages also gate prefetching, but a request that doesn't fit
/// stays scheduled (`execute` returns true) and the averages keep learning. Here the running
/// average (3/4 old + 1/4 new) is only updated by items prefetched, and a request short of time
/// gives up: after one slow item the average stays above any frame's idle time, so this list
/// never prefetches again; every later item is composed in the frame that scrolls it in.
#[test]
fn one_slow_item_does_not_disable_prefetch_for_good() {
    let (mut ui, state) = list(0);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_composed();
    SLOW.with(|slow| slow.set(Some(last + 1)));
    ui.host().prefetch(later());
    assert_eq!(compositions(last + 1), 1);
    let mut prefetched = 0;
    for _ in 0..10 {
        state.scroll_by(20.0);
        ui.wait_for_idle();
        let before = total_compositions();
        ui.host().prefetch(Instant::now() + Duration::from_millis(16));
        prefetched += total_compositions() - before;
    }
    assert!(prefetched > 0, "nothing prefetched in 10 idle times after one slow item");
}

/// Input: a list prefetching fast items, then one taking 40 ms (on a phone: a cold text
/// layout, a little core), then an idle time of 20 ms.
///
/// An average (3/4 old + 1/4 new) says the next item takes about 10 ms, so it starts, and if
/// it's as slow, it overruns the next frame by 20 ms (on the Pixel 8 Pro, about 1 % of the
/// idle times ran more than 1.2 ms past the vsync, delaying the next frame). Items are no
/// steadier than that: one taking 40 ms is evidence that the next may too. The idle time is
/// left alone while the recent maximum (decaying) doesn't fit (D341).
#[test]
fn after_a_slow_item_an_idle_time_shorter_than_it_is_left_alone() {
    let (mut ui, state) = list(0);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    let last = last_composed();
    SLOW.with(|slow| slow.set(Some(last + 1)));
    state.scroll_by(20.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    assert_eq!(compositions(last + 1), 1, "the slow item, prefetched");
    state.scroll_by(20.0);
    ui.wait_for_idle();
    let before = total_compositions();
    ui.host().prefetch(Instant::now() + Duration::from_millis(20));
    assert_eq!(total_compositions(), before, "an item started in 20 ms after one took 40");
}

/// Input: a list that scrolled, then an idle time of 0.2 ms, before any item was timed.
///
/// With nothing learned yet, the estimate is 0: any idle time fits. An item takes much longer
/// than 0.2 ms on a phone, so it would overrun the next frame. A step starts only if it fits
/// with a safety margin (0.5 ms) to spare (D341).
#[test]
fn an_idle_time_within_the_safety_margin_is_left_alone() {
    let (mut ui, state) = list(0);
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let before = total_compositions();
    ui.host().prefetch(Instant::now() + Duration::from_micros(200));
    assert_eq!(total_compositions(), before);
    // Given up until the next scroll, whose idle time does it.
    state.scroll_by(1.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    assert!(total_compositions() > before);
}

/// A row of cells: a `LazyRow` of 30 x 20 cells in a 100 x 20 item.
#[composable]
fn Cells(row: usize) {
    COMPOSED.with(|map| *map.borrow_mut().entry(row).or_insert(0) += 1);
    LazyRow!(modifier = Modifier.size((100.dp(), 20.dp())), |scope: LazyListScope| {
        scope.items(20, |_cell| {
            CELLS.with(|cells| cells.set(cells.get() + 1));
            Box!(modifier = Modifier.size((30.dp(), 20.dp())));
        });
    });
}

/// Input: a `LazyColumn` of `LazyRow`s (a feed of carousels), scrolled; `prefetch` in idle time;
/// then a scroll revealing the prefetched rows.
///
/// Compose (1.7+): the outer prefetch composes and premeasures the nested list's first items
/// too (`LazyLayoutPrefetchState.NestedPrefetchScope`), so the frame that scrolls the row in
/// composes nothing. Here the prefetched row's `LazyRow` is composed with one cell (its initial
/// composed range), and its premeasurement writes the range it wants: the cells are composed in
/// a frame, the jank prefetch is meant to remove (prefetch even wakes a frame for it).
#[test]
fn a_prefetched_row_of_a_nested_lazy_row_is_composed_ahead() {
    reset();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, Cells);
            }
        );
    });
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_composed();
    ui.host().prefetch(later());
    assert_eq!(compositions(last + 1), 1);
    assert_eq!(compositions(last + 2), 1);
    let cells = CELLS.with(|cells| cells.get());
    // The frame scrolling the prefetched rows in.
    state.scroll_by(40.0);
    ui.host().frame();
    let in_frame = CELLS.with(|cells| cells.get()) - cells;
    assert_eq!(in_frame, 0, "{in_frame} cells of prefetched rows composed in the frame");
}

/// Input: items holding a `BoxWithConstraints` of a fixed size (its constraints differ from the
/// items' hint), prefetched, then scrolled in.
///
/// Compose: `BoxWithConstraints` subcomposes while measuring, so premeasuring the prefetched item
/// composes its content. Here premeasure writes the constraints and the content is composed
/// again outside prefetch: by the next item's `recompose` for all but the last, and in a frame
/// (woken by prefetch) for the last.
#[test]
fn a_prefetched_box_with_constraints_is_ready_for_the_frame() {
    reset();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, |index| {
                    BoxWithConstraints!(
                        modifier = Modifier.size((100.dp(), 20.dp())),
                        move |constraints: BoxWithConstraintsScope| {
                            let _ = constraints.max_height();
                            COMPOSED.with(|map| *map.borrow_mut().entry(index).or_insert(0) += 1);
                        }
                    );
                });
            }
        );
    });
    state.scroll_by(30.0);
    ui.wait_for_idle();
    ui.host().prefetch(later());
    let before = total_compositions();
    state.scroll_by(40.0);
    let stats = ui.host().frame();
    assert_eq!(total_compositions(), before, "content of prefetched items composed in the frame");
    assert!(!stats.recomposed);
}

thread_local! {
    static AUTO: Cell<Option<usize>> = const { Cell::new(None) };
}

#[composable]
fn AutoItem(index: usize) {
    COMPOSED.with(|map| *map.borrow_mut().entry(index).or_insert(0) += 1);
    let requester = remember(compose_ui::FocusRequester::new);
    if AUTO.with(|auto| auto.get()) == Some(index) {
        let shown = requester;
        LaunchedEffect((), async move {
            shown.request_focus();
        });
    }
    Box!(
        modifier = Modifier
            .size((100.dp(), 20.dp()))
            .focus_requester(requester)
            .focusable()
            .test_tag(format!("row {index}"))
    );
}

fn auto_list() -> (ComposeTestRule, Owned<LazyListState>) {
    reset();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, AutoItem);
            }
        );
    });
    (ui, state)
}

/// Input: an item that asks for the focus when it's composed (`LaunchedEffect` +
/// `FocusRequester`, an autofocused field) is first composed by prefetch, below the viewport.
///
/// Without prefetch (the item composed in a frame, as one of those beyond the viewport) the
/// request is handled at the next frame, once the item is placed, and `bring_into_view` scrolls
/// the list to show it. With prefetch the effect runs in `UiHost::prefetch`, the next frame
/// handles the request first thing (`handle_focus_requests`, before layout), while the item is
/// still unplaced at (0, 0): its bounds look in view, nothing scrolls, then the layout places it
/// below the viewport. The focused field is off screen (keys go to something not shown).
#[test]
fn an_item_focused_when_prefetched_is_scrolled_into_view() {
    let (mut ui, state) = auto_list();
    state.scroll_by(30.0);
    ui.wait_for_idle();
    let last = last_composed();
    AUTO.with(|auto| auto.set(Some(last + 1)));
    ui.host().prefetch(later());
    ui.wait_for_idle();
    AUTO.with(|auto| auto.set(None));
    let tag = format!("row {}", last + 1);
    ui.on_node_with_tag(&tag).assert_is_focused();
    let bounds = ui.on_node_with_tag(&tag).fetch_semantics_node().bounds;
    assert!(
        bounds.top >= 0.0 && bounds.bottom <= 100.0,
        "focused item at {bounds:?}, outside the 100 px viewport (first visible item {})",
        state.first_visible_item_index()
    );
}
