//! Allocation checks with a counting `#[global_allocator]` (docs/09-testing.md): steady-state
//! passes allocate nothing, live allocations stay constant across many clicks, and dropping a
//! composition frees what it allocated.

#![allow(non_snake_case)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;

// ---- The counting allocator ----------------------------------------------------------------

struct Counting;

thread_local! {
    // Const-initialized and without destructors, so the allocator can use them at any time.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
}

fn count(allocations: u64, bytes: i64) {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + allocations));
    let _ = LIVE_BYTES.try_with(|live| live.set(live.get() + bytes));
}

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(1, layout.size() as i64);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(1, layout.size() as i64);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(0, -(layout.size() as i64));
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(1, new_size as i64 - layout.size() as i64);
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

fn live_bytes() -> i64 {
    LIVE_BYTES.with(Cell::get)
}

/// The number of allocations `f` makes on this thread.
fn allocations_in(f: impl FnOnce()) -> u64 {
    let before = allocations();
    f();
    allocations() - before
}

// ---- Hand-expanded composables -------------------------------------------------------------

type Tree = NodeTree<u64>;

thread_local! {
    static RUNS: Cell<u32> = const { Cell::new(0) };
}

fn runs() -> u32 {
    RUNS.with(|runs| runs.replace(0))
}

/// A leaf node showing a number; updating it in place allocates nothing.
fn Leaf(value: u64) {
    let composer = current();
    let node = composer.start_node(key_of("allocations::Leaf"), || value);
    composer.with_applier(|tree: &mut Tree| {
        let current = tree.get_mut(node).expect("live node");
        if *current != value {
            *current = value;
        }
    });
    composer.end_group();
}

/// `#[composable] fn Text(value: u64)`.
fn Text(value: u64) {
    let composer = current();
    composer.start_restart_group(key_of("allocations::Text"));
    let changed = composer.update_kept(|kept: &(u64,)| !kept.0.same(&value), || (value,));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        Leaf(value);
    }
    composer.end_restart_group(|kept| Text(kept.downcast_ref::<(u64,)>().expect("kept").0));
}

/// `#[composable] fn Counter()`: a state, a button-like node, and a text of the count.
fn Counter() {
    let composer = current();
    composer.start_restart_group(key_of("allocations::Counter"));
    if composer.skipping() {
        composer.skip_to_group_end();
    } else {
        let count = remember(|| mutable_state_of(0u64));
        COUNT.with(|handle| handle.set(Some(count)));
        Leaf(1000);
        Text(count.get());
        SideEffectFree(count);
    }
    composer.end_restart_group(|_| Counter());
}

/// `#[composable] fn SideEffectFree(count: MutableState<u64>)`: reads the state itself.
fn SideEffectFree(count: MutableState<u64>) {
    let composer = current();
    composer.start_restart_group(key_of("allocations::Reader"));
    let changed =
        composer.update_kept(|kept: &(MutableState<u64>,)| !kept.0.same(&count), || (count,));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        let doubled = remember(|| Rc::new(Cell::new(0u64)));
        doubled.set(count.get() * 2);
        Leaf(doubled.get());
    }
    composer.end_restart_group(|kept| {
        SideEffectFree(kept.downcast_ref::<(MutableState<u64>,)>().expect("kept").0)
    });
}

thread_local! {
    static COUNT: Cell<Option<MutableState<u64>>> = const { Cell::new(None) };
}

struct RowData {
    id: u64,
    done: OwnedState<bool>,
}

/// `#[composable] fn TodoRow(row: Rc<RowData>)`.
fn TodoRow(row: Rc<RowData>) {
    let composer = current();
    composer.start_restart_group(key_of("allocations::TodoRow"));
    let changed =
        composer.update_kept(|kept: &(Rc<RowData>,)| !kept.0.same(&row), || (row.clone(),));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        let composer = current();
        composer.start_node(key_of("allocations::RowNode"), || row.id);
        Leaf(u64::from(row.done.get()));
        Text(row.id);
        composer.end_group();
    }
    composer.end_restart_group(|kept| {
        TodoRow(kept.downcast_ref::<(Rc<RowData>,)>().expect("kept").0.clone())
    });
}

/// `#[composable] fn TodoList(rows: MutableState<Vec<Rc<RowData>>>)`.
fn TodoList(rows: MutableState<Vec<Rc<RowData>>>) {
    let composer = current();
    composer.start_restart_group(key_of("allocations::TodoList"));
    let changed = composer
        .update_kept(|kept: &(MutableState<Vec<Rc<RowData>>>,)| !kept.0.same(&rows), || (rows,));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        rows.with(|rows| {
            for row in rows {
                key(row.id, || TodoRow(row.clone()));
            }
        });
    }
    composer.end_restart_group(|kept| {
        TodoList(kept.downcast_ref::<(MutableState<Vec<Rc<RowData>>>,)>().expect("kept").0)
    });
}

fn todo_rows(count: u64) -> OwnedState<Vec<Rc<RowData>>> {
    let rows =
        (0..count).map(|id| Rc::new(RowData { id, done: mutable_state_of(false) })).collect();
    mutable_state_of_with(rows, never_equal_policy())
}

// ---- Checks --------------------------------------------------------------------------------

#[test]
fn a_counter_click_allocates_nothing() {
    let composition = Composition::new(Tree::new());
    composition.set_content(Counter);
    let count = COUNT.with(Cell::get).expect("count");
    // Warm up: the first passes size the reusable vectors.
    for _ in 0..3 {
        count.update(|count| count + 1);
        composition.recompose();
    }
    runs();
    for _ in 0..100 {
        let allocated = allocations_in(|| {
            count.update(|count| count + 1);
            assert!(composition.recompose());
        });
        assert_eq!(allocated, 0, "a click and its recomposition allocate nothing");
        assert_eq!(runs(), 2, "the text and the reader recompose, nothing else");
    }
    assert_eq!(composition.with_applier(|tree: &mut Tree| tree.dump()), "1000\n103\n206\n");
}

#[test]
fn a_pass_with_nothing_invalid_allocates_nothing() {
    let composition = Composition::new(Tree::new());
    composition.set_content(Counter);
    let allocated = allocations_in(|| {
        for _ in 0..100 {
            assert!(!composition.recompose());
        }
    });
    assert_eq!(allocated, 0);
}

#[test]
fn toggling_one_of_a_thousand_rows_recomposes_one_row_and_allocates_nothing() {
    let count = if cfg!(miri) { 50 } else { 1000 };
    let rows = todo_rows(count);
    let handle = rows.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || TodoList(handle));
    runs();
    let done: Vec<MutableState<bool>> =
        rows.with(|rows| rows.iter().map(|row| row.done.handle()).collect());
    for index in [3, 25, 49, 3] {
        done[index].set(!done[index].get());
        composition.recompose();
    }
    runs();
    for round in 0..50 {
        let index = (round * 37) % count as usize;
        let allocated = allocations_in(|| {
            done[index].set(!done[index].get());
            assert!(composition.recompose());
        });
        assert_eq!(allocated, 0, "toggling row {index}");
        assert_eq!(runs(), 1, "only the toggled row recomposes");
    }
    composition.validate();
}

#[test]
fn changed_arguments_update_kept_values_in_place() {
    let value = mutable_state_of(0u64);
    let handle = value.handle();
    let composition = Composition::new(Tree::new());
    // The parent passes a new argument each time; the child's kept tuple is updated in place.
    composition.set_content(move || {
        let composer = current();
        composer.start_replace_group(1);
        Text(handle.get());
        composer.end_group();
    });
    for round in 1..4 {
        value.set(round);
        composition.recompose();
    }
    for round in 4..100 {
        let allocated = allocations_in(|| {
            value.set(round);
            composition.recompose();
        });
        assert_eq!(allocated, 0);
    }
}

/// `Text` as `#[composable]` expands it: `update_kept`'s steps inline, with no closures.
fn InlineText(value: u64) {
    let composer = current();
    composer.start_restart_group(key_of("allocations::InlineText"));
    let mut kept = composer.take_kept();
    let changed = match kept.get::<(u64,)>() {
        Some(kept) => !kept.0.same(&value),
        None => true,
    };
    if changed {
        kept.set((value,));
    }
    if !kept.put(changed) && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        Leaf(value);
    }
    composer.end_restart_group(|kept| InlineText(kept.downcast_ref::<(u64,)>().expect("kept").0));
}

#[test]
fn inline_kept_arguments_skip_and_update_in_place() {
    let value = mutable_state_of(0u64);
    let other = mutable_state_of(0u64);
    let (handle, other_handle) = (value.handle(), other.handle());
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let composer = current();
        composer.start_replace_group(1);
        other_handle.get();
        InlineText(handle.get());
        composer.end_group();
    });
    assert_eq!(runs(), 1);
    // The parent recomposes with the same argument: the child skips.
    other.set(1);
    composition.recompose();
    assert_eq!(runs(), 0);
    for round in 1..4 {
        value.set(round);
        composition.recompose();
    }
    assert_eq!(runs(), 3);
    for round in 4..100 {
        let allocated = allocations_in(|| {
            value.set(round);
            composition.recompose();
        });
        assert_eq!(allocated, 0);
    }
    assert_eq!(runs(), 96);
    composition.validate();
}

#[test]
fn live_allocations_stay_constant_across_a_thousand_clicks() {
    let composition = Composition::new(Tree::new());
    composition.set_content(Counter);
    let count = COUNT.with(Cell::get).expect("count");
    for _ in 0..10 {
        count.update(|count| count + 1);
        composition.recompose();
    }
    let before = live_bytes();
    for _ in 0..if cfg!(miri) { 50 } else { 1000 } {
        count.update(|count| count + 1);
        composition.recompose();
    }
    assert_eq!(live_bytes(), before);
}

#[test]
fn adding_and_removing_rows_returns_to_the_same_live_allocations() {
    let rows = todo_rows(10);
    let handle = rows.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || TodoList(handle));
    let mut next_id = 10;
    let mut add_and_remove = || {
        handle.edit(|rows| {
            for _ in 0..if cfg!(miri) { 10 } else { 100 } {
                next_id += 1;
                rows.push(Rc::new(RowData { id: next_id, done: mutable_state_of(false) }));
            }
        });
        composition.recompose();
        handle.edit(|rows| rows.truncate(10));
        composition.recompose();
        // Rule R2: the removed rows' cells are freed at the end of the next pass.
        handle.edit(|_| {});
        composition.recompose();
    };
    // Reused arena entries keep the capacity of their earlier occupants: warm up until it settles.
    for _ in 0..10 {
        add_and_remove();
    }
    let before = live_bytes();
    for _ in 0..if cfg!(miri) { 5 } else { 40 } {
        add_and_remove();
    }
    assert_eq!(live_bytes(), before);
    assert_eq!(live_state_count(), 11, "the ten rows' cells and the list's");
}

#[test]
fn dropping_a_composition_frees_what_it_allocated() {
    let build_and_drop = || {
        let rows = todo_rows(if cfg!(miri) { 20 } else { 200 });
        let handle = rows.handle();
        let composition = Composition::new(Tree::new());
        composition.set_content(move || TodoList(handle));
        drop(composition);
        drop(rows);
    };
    // The first runs size the thread's arenas, which keep their capacity.
    build_and_drop();
    build_and_drop();
    let before = live_bytes();
    for _ in 0..if cfg!(miri) { 3 } else { 10 } {
        build_and_drop();
    }
    assert_eq!(live_bytes(), before, "everything a composition allocates is freed with it");
    assert_eq!(live_state_count(), 0);
    assert_eq!(live_scope_count(), 0);
}
