//! Hand-expanded composables for the runtime benchmarks (what `#[composable]` generates).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;

pub type Tree = NodeTree<u64>;

/// A leaf node showing a number; updating it in place allocates nothing.
pub fn Leaf(value: u64) {
    let composer = current();
    let node = composer.start_node(key_of("benchmarks::Leaf"), || value);
    composer.with_applier(|tree: &mut Tree| {
        let current = tree.get_mut(node).expect("live node");
        if *current != value {
            *current = value;
        }
    });
    composer.end_group();
}

/// `#[composable] fn Text(value: u64)`.
pub fn Text(value: u64) {
    let composer = current();
    composer.start_restart_group(key_of("benchmarks::Text"));
    let changed = composer.update_kept(|kept: &(u64,)| !kept.0.same(&value), || (value,));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        Leaf(value);
    }
    composer.end_restart_group(|kept| Text(kept.downcast_ref::<(u64,)>().expect("kept").0));
}

thread_local! {
    static COUNT: Cell<Option<MutableState<u64>>> = const { Cell::new(None) };
}

/// `#[composable] fn Counter()`: a remembered state and a text showing it.
pub fn Counter() {
    let composer = current();
    composer.start_restart_group(key_of("benchmarks::Counter"));
    if composer.skipping() {
        composer.skip_to_group_end();
    } else {
        let count = remember(|| mutable_state_of(0u64));
        COUNT.with(|handle| handle.set(Some(count)));
        Leaf(1000);
        Text(count.get());
    }
    composer.end_restart_group(|_| Counter());
}

/// The count of the last composed [`Counter`].
pub fn counter_state() -> MutableState<u64> {
    COUNT.with(Cell::get).expect("a composed Counter")
}

pub struct RowData {
    pub id: u64,
    pub done: OwnedState<bool>,
}

/// `#[composable] fn TodoRow(row: Rc<RowData>)`.
pub fn TodoRow(row: Rc<RowData>) {
    let composer = current();
    composer.start_restart_group(key_of("benchmarks::TodoRow"));
    let changed =
        composer.update_kept(|kept: &(Rc<RowData>,)| !kept.0.same(&row), || (row.clone(),));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        composer.start_node(key_of("benchmarks::RowNode"), || row.id);
        Leaf(u64::from(row.done.get()));
        Text(row.id);
        composer.end_group();
    }
    composer.end_restart_group(|kept| {
        TodoRow(kept.downcast_ref::<(Rc<RowData>,)>().expect("kept").0.clone())
    });
}

pub type Rows = MutableState<Vec<Rc<RowData>>>;

/// `#[composable] fn TodoList(rows: MutableState<Vec<Rc<RowData>>>)`: keyed rows.
pub fn TodoList(rows: Rows) {
    let composer = current();
    composer.start_restart_group(key_of("benchmarks::TodoList"));
    let changed = composer.update_kept(|kept: &(Rows,)| !kept.0.same(&rows), || (rows,));
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        rows.with(|rows| {
            for row in rows {
                key(row.id, || TodoRow(row.clone()));
            }
        });
    }
    composer.end_restart_group(|kept| TodoList(kept.downcast_ref::<(Rows,)>().expect("kept").0));
}

pub fn todo_rows(count: u64) -> OwnedState<Vec<Rc<RowData>>> {
    let rows =
        (0..count).map(|id| Rc::new(RowData { id, done: mutable_state_of(false) })).collect();
    mutable_state_of_with(rows, never_equal_policy())
}

/// `#[composable] fn Skipper(trigger: MutableState<u64>, children: u64)`: reads `trigger`, then
/// calls `children` composables whose arguments never change, so they all skip.
pub fn Skipper(trigger: MutableState<u64>, children: u64) {
    let composer = current();
    composer.start_restart_group(key_of("benchmarks::Skipper"));
    let changed = composer.update_kept(
        |kept: &(MutableState<u64>, u64)| !kept.0.same(&trigger) || !kept.1.same(&children),
        || (trigger, children),
    );
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        trigger.get();
        for child in 0..children {
            Text(child);
        }
    }
    composer.end_restart_group(|kept| {
        let (trigger, children) = *kept.downcast_ref::<(MutableState<u64>, u64)>().expect("kept");
        Skipper(trigger, children)
    });
}
