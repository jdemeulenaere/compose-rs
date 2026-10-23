//! Adversarial review of D430-D433 (lazy item reuse).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Constraints, IntSize, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier, UnitExt,
};

thread_local! {
    static MEASURES: RefCell<HashMap<u64, u32>> = RefCell::new(HashMap::new());
    static GLOBAL: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
    static DEAD: Cell<u32> = const { Cell::new(0) };
    static SECOND: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
    static PANIC_ON_DEAD: Cell<bool> = const { Cell::new(false) };
    static TICK: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

fn measures(item: u64) -> u32 {
    MEASURES.with(|m| m.borrow().get(&item).copied().unwrap_or(0))
}

/// Reads a global state and the item's saveable state while measuring.
struct Reading {
    item: u64,
    count: MutableState<u64>,
}

impl MeasurePolicy for Reading {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        MEASURES.with(|m| *m.borrow_mut().entry(self.item).or_default() += 1);
        let global = GLOBAL.with(|g| g.get()).expect("global").get();
        if self.item == 0
            && let Some(second) = SECOND.with(|g| g.get())
        {
            let _ = second.get();
        }
        if !self.count.is_alive() {
            DEAD.with(|d| d.set(d.get() + 1));
        }
        if PANIC_ON_DEAD.with(|p| p.get()) {
            // What a real policy does: read its state.
            let _ = self.count.get();
        }
        let _ = global;
        scope.layout(constraints.max_width.min(100), 20)
    }
}

#[composable]
fn ReadingItem(item: u64) {
    let count = remember_saveable(move || mutable_state_of(item));
    let policy = remember(move || Rc::new(Reading { item, count }) as Rc<dyn MeasurePolicy>);
    compose_ui::Layout(Modifier, policy, callback(|| {}));
}

fn reading_list_of(
    count: usize,
) -> (ComposeTestRule, Owned<LazyListState>, OwnedState<u32>, OwnedState<usize>) {
    let count = mutable_state_of(count);
    let count_handle = count.handle();
    MEASURES.with(|m| m.borrow_mut().clear());
    let global = mutable_state_of(0u32);
    GLOBAL.with(|g| g.set(Some(global.handle())));
    let state = LazyListState::new(0, 0);
    let given = *state;
    let tick = mutable_state_of(0u32);
    let tick_handle = tick.handle();
    TICK.with(|t| t.set(Some(tick_handle)));
    std::mem::forget(tick);
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let _ = tick_handle.get();
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            move |scope: LazyListScope| scope
                .items(count_handle.get(), |index| ReadingItem(index as u64))
        );
    });
    ui.wait_for_idle();
    (ui, state, global, count)
}

/// Items removed from the data are deactivated and kept (7): a write to what their measurement
/// read measures them again, with their freed saveable state.
#[test]
fn removed_items_kept_for_reuse_are_not_measured_again() {
    let (mut ui, _state, global, count) = reading_list_of(20);
    count.set(2);
    ui.wait_for_idle();
    // Passes: the cells deactivation retired are freed.
    for tick in 1..3 {
        TICK.with(|t| t.get()).unwrap().set(tick);
        ui.wait_for_idle();
    }
    let before: Vec<u32> = (0..20).map(measures).collect();
    DEAD.with(|d| d.set(0));
    for value in 1..4 {
        global.set(value);
        ui.wait_for_idle();
    }
    let after: Vec<u32> = (0..20).map(measures).collect();
    let again: Vec<u64> = (2..20).filter(|i| after[*i as usize] != before[*i as usize]).collect();
    assert_eq!(DEAD.with(|d| d.get()), 0, "measured with a freed saveable state: {again:?}");
    assert!(again.is_empty(), "items kept for reuse measured again: {again:?}");
}

/// The same, with a policy that reads its state as any would: the frame panics.
#[test]
fn a_kept_items_policy_reading_its_saveable_state_does_not_panic() {
    let (mut ui, _state, global, count) = reading_list_of(20);
    count.set(2);
    ui.wait_for_idle();
    for tick in 1..3 {
        TICK.with(|t| t.get()).unwrap().set(tick);
        ui.wait_for_idle();
    }
    PANIC_ON_DEAD.with(|p| p.set(true));
    global.set(1);
    ui.wait_for_idle();
    PANIC_ON_DEAD.with(|p| p.set(false));
}

thread_local! {
    static KEYS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// A hoisted `FocusRequester` stays attached to an item's node once the item is deactivated
/// (D431 keeps modifier nodes attached; Compose's `resetState` detaches them): asking it focuses
/// a node out of the list's content, and its keys reach the departed item's callback.
#[test]
fn a_requester_of_an_item_kept_for_reuse_does_not_focus_it() {
    KEYS.with(|k| k.borrow_mut().clear());
    let requesters: Rc<Vec<compose_ui::FocusRequester>> =
        Rc::new((0..20).map(|_| compose_ui::FocusRequester::new()).collect());
    let count = mutable_state_of(20usize);
    let count_handle = count.handle();
    let given = requesters.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let requesters = given.clone();
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), move |scope: LazyListScope| {
            let requesters = requesters.clone();
            scope.items_keyed(
                count_handle.get(),
                |index| index as u64,
                move |index| {
                    let item = index as u64;
                    compose_ui::Layout(
                        Modifier
                            .size((100.dp(), 20.dp()))
                            .focus_requester(requesters[index])
                            .on_key_event(move |_event: compose_ui::KeyEvent| {
                                KEYS.with(|k| k.borrow_mut().push(item));
                                true
                            })
                            .focusable(),
                        compose_ui::empty_measure_policy(),
                        callback(|| {}),
                    );
                },
            )
        });
    });
    ui.wait_for_idle();
    count.set(2);
    ui.wait_for_idle();
    // Item 4 is gone from the list (kept for reuse, or disposed).
    let attached = requesters[4].request_focus();
    ui.wait_for_idle();
    let focused = ui.host().with_tree(|tree| tree.focused());
    ui.press_key(compose_ui::Key::Enter);
    let keys = KEYS.with(|k| k.borrow().clone());
    assert!(
        !attached && focused.is_none() && keys.is_empty(),
        "a removed item's requester attached: {attached}, focus {focused:?}, keys to {keys:?}"
    );
}

/// Deactivated, content drawn in a repaint boundary is drawn still: `UiTree::deactivate`
/// unplaces it but marks nothing above it to draw again.
#[test]
fn deactivated_content_is_not_drawn_from_a_kept_recording() {
    use compose_foundation::layout::*;
    let active = mutable_state_of(true);
    let handle = active.handle();
    let mut ui = compose_test(move || {
        Column!(modifier = Modifier.size(100.dp()), {
            Box!(modifier = Modifier.size(10.dp()).background(compose_ui::Color::BLUE));
            ReusableContentHost(handle.get(), move || {
                Box!(modifier = Modifier.size(20.dp()).background(compose_ui::Color::RED));
            });
        });
    });
    ui.wait_for_idle();
    let before = ui.dump_display_list();
    active.set(false);
    ui.wait_for_idle();
    let after = ui.dump_display_list();
    assert_ne!(before, after, "deactivated content still drawn:\n{after}");
}

/// `ReusableContentHost(false)` in an ordinary layout: its content is deactivated ("unplaced,
/// not drawn, not hit, out of the semantics"), but the Column measures and places every child,
/// so the next layout places it again.
#[test]
fn deactivated_content_in_a_column_is_not_shown() {
    use compose_foundation::layout::*;
    let active = mutable_state_of(true);
    let handle = active.handle();
    let other = mutable_state_of(0u32);
    let other_handle = other.handle();
    let clicks = Rc::new(Cell::new(0u32));
    let counted = clicks.clone();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        Column!({
            Box!(modifier = Modifier.size((10 + other_handle.get() as i32).dp()).test_tag("other"));
            let counted = counted.clone();
            ReusableContentHost(handle.get(), move || {
                let counted = counted.clone();
                Box!(
                    modifier = Modifier
                        .size(20.dp())
                        .test_tag("hosted")
                        .clickable(move || { counted.set(counted.get() + 1) })
                );
            });
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("hosted").assert_exists();
    active.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("hosted").assert_does_not_exist();
    // Something else in the column changes: it's measured and placed again.
    other.set(1);
    ui.wait_for_idle();
    let shown = ui.count_nodes(&compose_test::has_test_tag("hosted"));
    if shown > 0 {
        ui.on_node_with_tag("hosted").perform_click();
        eprintln!("clicks on deactivated content: {}", clicks.get());
    }
    let semantics = ui.dump_semantics();
    let display = ui.dump_display_list();
    assert_eq!(shown, 0, "deactivated content shown again:\n{semantics}\n{display}");
}

/// No write to what a kept item read: a density change marks every node to measure, the list
/// measures its items (not those kept), and the next in-place measure of the list (an item's
/// change) measures every kept item, with its freed state.
#[test]
fn a_density_change_then_an_items_change_does_not_measure_kept_items() {
    let second = mutable_state_of(0u32);
    SECOND.with(|g| g.set(Some(second.handle())));
    let (mut ui, _state, _global, count) = reading_list_of(20);
    count.set(2);
    ui.wait_for_idle();
    for tick in 1..3 {
        TICK.with(|t| t.get()).unwrap().set(tick);
        ui.wait_for_idle();
    }
    ui.host().set_density(compose_ui::Density::new(2.0));
    ui.wait_for_idle();
    DEAD.with(|d| d.set(0));
    second.set(1);
    ui.wait_for_idle();
    SECOND.with(|g| g.set(None));
    assert_eq!(DEAD.with(|d| d.get()), 0, "kept items measured with their freed state");
}

thread_local! {
    static INNER_NODES: RefCell<std::collections::HashSet<compose_ui::platform::NodeId>> =
        RefCell::new(std::collections::HashSet::new());
}

struct InnerRecording;

impl MeasurePolicy for InnerRecording {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _: &[Measurable],
        _constraints: Constraints,
    ) -> MeasureResult {
        let node = scope.node_id();
        INNER_NODES.with(|nodes| nodes.borrow_mut().insert(node));
        scope.layout(20, 20)
    }
}

/// A feed of carousels: an outer item reused for another keeps its node, but its inner list's
/// `ItemSlots` is a `remember`, computed anew, so every inner item is disposed and created
/// anew (Compose keeps a `SubcomposeLayout`'s slots on its reused node).
#[test]
fn a_reused_rows_inner_list_reuses_its_items() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| scope.items(1000, |_row| {
                LazyRow!(modifier = Modifier.size((100.dp(), 20.dp())), |scope: LazyListScope| {
                    scope.items(20, |_| {
                        compose_ui::Layout(
                            Modifier,
                            remember(|| Rc::new(InnerRecording) as Rc<dyn MeasurePolicy>),
                            callback(|| {}),
                        );
                    })
                });
            })
        );
    });
    ui.wait_for_idle();
    for _ in 0..10 {
        state.scroll_by(20.0);
        ui.wait_for_idle();
    }
    let early = INNER_NODES.with(|nodes| nodes.borrow().len());
    for _ in 0..50 {
        state.scroll_by(20.0);
        ui.wait_for_idle();
    }
    let late = INNER_NODES.with(|nodes| nodes.borrow().len());
    assert!(late < early + 20, "inner item nodes: {early} after 10 rows, {late} after 60");
}
