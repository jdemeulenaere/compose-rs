//! Second adversarial review of pausable composition in prefetch (D597, D599).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, UnitExt};

thread_local! {
    static ROW_PARTS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    static ROW_PART_TIME: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    static SCHEDULER: RefCell<Option<compose_ui::PrefetchScheduler>> = const { RefCell::new(None) };
}

fn within(millis: u64) -> Instant {
    Instant::now() + Duration::from_millis(millis)
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

fn in_tree(ui: &ComposeTestRule, tag: &str) -> bool {
    ui.dump_tree().contains(&format!("\"{tag}\""))
}

#[composable]
fn RowPart(row: usize, part: usize) {
    ROW_PARTS.with(|parts| *parts.borrow_mut().entry(row).or_insert(0) += 1);
    let time = ROW_PART_TIME.get();
    if !time.is_zero() {
        std::thread::sleep(time);
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

/// Writes `state` when it runs, as any request (or effect run in an idle time) writing state
/// the frame hasn't composed yet.
struct WriteRequest(OwnedState<u32>);

impl compose_ui::PrefetchRequest for WriteRequest {
    fn execute(&self, _scope: &mut compose_ui::PrefetchRequestScope<'_>) -> bool {
        self.0.set(1);
        false
    }
}

/// A list's request makes its paused composition while another list's items wait to be
/// composed (their keys changed by a write in the same idle time): that list's new items are
/// inserted in the paused composition's first pass. They were its paused content, which their
/// list didn't know of: its next frame showed the old items (the slots they reuse, their content
/// not reached yet; new content would be in no parent, laid out empty), until the other list's
/// request applied its paused composition, which may take several idle times. Now ordinary
/// content (the paused composition is for its item only, D597), shown by the next frame.
#[test]
fn review2_another_lists_items_composed_in_a_requests_first_pass_show_at_once() {
    ROW_PARTS.with(|parts| parts.borrow_mut().clear());
    ROW_PART_TIME.set(Duration::ZERO);
    use_optimizations(Optimizations::NONE);
    let generation = mutable_state_of(0u32);
    let read = generation.handle();
    let state = LazyListState::new(1, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        SCHEDULER.with(|s| *s.borrow_mut() = compose_ui::LOCAL_PREFETCH_SCHEDULER.current());
        Row!({
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(1000, PartsRow);
                }
            );
            let generation = read.get();
            LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
                scope.items_keyed(
                    5,
                    move |index| generation * 100 + index as u32,
                    move |index| {
                        Box!(
                            modifier = Modifier
                                .size((100.dp(), 20.dp()))
                                .test_tag(format!("other {generation}.{index}"))
                        );
                    },
                );
            });
        });
    });
    assert!(in_tree(&ui, "other 0.0"));
    // Row 7 requested.
    ROW_PART_TIME.set(Duration::from_millis(4));
    given.dispatch_raw_delta(5.0);
    ui.wait_for_idle();
    // A write first in the idle time: the other list's keys change.
    let scheduler = SCHEDULER.with(|s| s.borrow().clone()).expect("a scheduler");
    scheduler.schedule_prefetch_first(Rc::new(WriteRequest(generation)));
    assert!(ui.host().prefetch(within(6)), "work left: row 7 paused");
    let parts = ROW_PARTS.with(|parts| parts.borrow().get(&7).copied().unwrap_or(0));
    assert!(parts > 0 && parts < 4, "row 7 paused after {parts} parts");
    ui.host().composition().validate();
    // The frame after: the other list shows its new items.
    ui.wait_for_idle();
    let after_frame = (in_tree(&ui, "other 1.0"), in_tree(&ui, "other 0.0"));
    while ui.host().prefetch(later()) {}
    ui.wait_for_idle();
    let after_prefetch = (in_tree(&ui, "other 1.0"), in_tree(&ui, "other 0.0"));
    ui.host().composition().validate();
    assert_eq!(
        (after_frame, after_prefetch),
        ((true, false), (true, false)),
        "(new shown, old shown) after the frame, then after the request applied"
    );
}

thread_local! {
    static REQUESTERS: RefCell<HashMap<usize, compose_ui::FocusRequester>> =
        RefCell::new(HashMap::new());
}

#[composable]
fn FocusRow(index: usize) {
    let requester = remember(compose_ui::FocusRequester::new);
    REQUESTERS.with(|requesters| requesters.borrow_mut().insert(index, requester));
    let modifier = Modifier
        .size((100.dp(), 20.dp()))
        .test_tag(format!("row {index}"))
        .focus_requester(requester)
        .focusable();
    Row!(modifier = modifier, {
        for part in 0..4 {
            RowPart(index, part);
        }
    });
}

fn requester(row: usize) -> compose_ui::FocusRequester {
    REQUESTERS.with(|requesters| requesters.borrow().get(&row).copied()).expect("remembered")
}

/// A request reusing a row that left (its slot in the pool) pauses: neither the old row's
/// requester nor the new one's focuses anything until it's applied, and then the new one's
/// does.
#[test]
fn review2_a_paused_reused_items_requesters_attach_at_apply() {
    ROW_PARTS.with(|parts| parts.borrow_mut().clear());
    ROW_PART_TIME.set(Duration::ZERO);
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(1, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, FocusRow);
            }
        );
    });
    let old = requester(1);
    assert!(old.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag("row 1").assert_is_focused();
    // Focus elsewhere: row 3 (row 1 focused would stay pinned).
    assert!(requester(3).request_focus());
    ui.wait_for_idle();
    // 25 px down: row 1 leaves, rows 7 and 8 next.
    given.dispatch_raw_delta(25.0);
    ui.wait_for_idle();
    ROW_PART_TIME.set(Duration::from_millis(4));
    // The trim lets row 1 go (to the pool), the next request reuses its slot and pauses.
    let mut paused_row = None;
    for _ in 0..4 {
        ui.host().prefetch(within(6));
        for row in [7, 8] {
            let parts = ROW_PARTS.with(|parts| parts.borrow().get(&row).copied().unwrap_or(0));
            if parts > 0 && parts < 4 {
                paused_row = Some(row);
            }
        }
        if paused_row.is_some() {
            break;
        }
        ui.wait_for_idle();
    }
    let Some(row) = paused_row else {
        panic!("no paused request: {:?}", ROW_PARTS.with(|p| p.borrow().clone()));
    };
    ui.host().composition().validate();
    assert!(!old.request_focus(), "row 1's requester focuses its slot kept for reuse");
    let new = requester(row);
    assert!(!new.request_focus(), "row {row}'s requester focuses before apply");
    ui.wait_for_idle();
    ROW_PART_TIME.set(Duration::ZERO);
    while ui.host().prefetch(later()) {}
    ui.wait_for_idle();
    assert!(new.request_focus(), "row {row}'s requester after apply");
    ui.wait_for_idle();
    ui.on_node_with_tag(&format!("row {row}")).assert_is_focused();
    assert!(!old.request_focus());
    ui.host().composition().validate();
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
            RowPart(row, part);
        }
        LazyRow!(
            modifier = Modifier
                .layout(move |scope, measurable, constraints: compose_ui::Constraints| {
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

/// The list's `LOCAL_PARENT_LIST` provider is remembered across reuse with no key (D599): given
/// another state (a tab's), the list's items' nested lists still note themselves in the first
/// state's list, so the new state's requests find no nested list to prefetch (on main, the
/// provider was made from the current state at each composition).
fn nested_prefetch_after_switch(switch: bool) -> Option<usize> {
    CELLS.with(|cells| cells.borrow_mut().clear());
    CELLS_AT_MEASURE.with(|at| at.borrow_mut().clear());
    ROW_PARTS.with(|parts| parts.borrow_mut().clear());
    ROW_PART_TIME.set(Duration::ZERO);
    use_optimizations(Optimizations::NONE);
    let (first, second) = (LazyListState::new(50, 0), LazyListState::new(0, 0));
    let (first_given, second_given) = (*first, *second);
    let tab = mutable_state_of(false);
    let tab_read = tab.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 400), move || {
        let state = if tab_read.get() { second_given } else { first_given };
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = state,
            // The next row, whose size is the viewport, from the first pixel on.
            cache_window = Some(lazy_layout_cache_window_with(100.dp(), 0.dp(), false)),
            |scope: LazyListScope| {
                // A content type each: no row reuses another's carousel (whose slots it
                // would keep, composing as many cells as it had).
                scope.items_keyed_with(100, |row| row, |row| row, Carousel);
            }
        );
    });
    let state = if switch {
        tab.set(true);
        ui.wait_for_idle();
        second_given
    } else {
        first_given.request_scroll_to_item(0, 0);
        ui.wait_for_idle();
        first_given
    };
    CELLS.with(|cells| cells.borrow_mut().clear());
    CELLS_AT_MEASURE.with(|at| at.borrow_mut().clear());
    state.dispatch_raw_delta(5.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    state.dispatch_raw_delta(100.0);
    ui.wait_for_idle();
    assert!(!ui.host().prefetch(later()));
    ui.host().composition().validate();
    CELLS_AT_MEASURE.with(|at| at.borrow().get(&3).copied())
}

#[test]
fn review2_nested_lists_are_prefetched_after_the_list_gets_another_state() {
    let same = nested_prefetch_after_switch(false);
    let switched = nested_prefetch_after_switch(true);
    assert!(same.is_some_and(|cells| cells >= 2), "without a switch: {same:?}");
    assert_eq!(switched, same, "row 3's carousel's cells when first measured, after a switch");
}

thread_local! {
    static REUSED_REQUESTER: Cell<Option<compose_ui::FocusRequester>> = const { Cell::new(None) };
}

/// A node reused in paused content while active (its `ReusableContent` given another key, not
/// a deactivated slot): reset as composition reaches it, before the pass updates it, as when
/// composed at once (its old content's focus cleared then). Reset at apply instead, the reset
/// came after the update: the old focus stayed until then, and cleared what was set since.
#[test]
fn review2_a_paused_reused_active_node_is_reset_before_its_update() {
    use_optimizations(Optimizations::NONE);
    let key = mutable_state_of(0u32);
    let read = key.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        ReusableContent(read.get(), || {
            compose_runtime::platform::PausableContent(|| {
                let requester = remember(compose_ui::FocusRequester::new);
                REUSED_REQUESTER.set(Some(requester));
                Box!(
                    modifier = Modifier
                        .size((100.dp(), 20.dp()))
                        .test_tag("reused")
                        .focus_requester(requester)
                        .focusable()
                );
            });
        });
    });
    let old = REUSED_REQUESTER.get().expect("composed");
    assert!(old.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag("reused").assert_is_focused();
    key.set(1);
    let paused = ui.host().composition().recompose_pausable();
    assert!(!paused.resume(|| true), "paused content: paused at its first pause point");
    assert!(paused.resume(|| false), "complete");
    assert!(
        !ui.dump_tree().contains("focusable focused"),
        "reset as the pass reused it, as composed at once: its old content's focus cleared"
    );
    let new = REUSED_REQUESTER.get().expect("composed");
    // The reused node shows the new content's requester, as composed at once it would.
    assert!(new.request_focus(), "the new content's requester on the active node");
    paused.apply();
    ui.wait_for_idle();
    ui.on_node_with_tag("reused").assert_is_focused();
    ui.host().composition().validate();
}
