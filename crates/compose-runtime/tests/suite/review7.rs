//! Review of the runtime's milestone 1: a branch switch in a loop without keys (D716) and the
//! imminent invalidation of a forward write (D712).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::platform::NodeTree;
use compose_runtime::*;

use crate::mock::*;

/// The id each row remembered in its collapsed branch, in row order, at each pass.
type Seen = Rc<RefCell<Vec<Option<u32>>>>;

#[composable]
fn Rows(items: MutableState<Vec<bool>>, next_id: Rc<Cell<u32>>, seen: Seen) {
    seen.borrow_mut().clear();
    for expanded in items.get() {
        if expanded {
            Text("expanded");
            seen.borrow_mut().push(None);
        } else {
            let next = next_id.clone();
            let id = remember(move || {
                next.set(next.get() + 1);
                next.get()
            });
            Text("collapsed");
            seen.borrow_mut().push(Some(id));
        }
    }
}

/// A row switching branches gets new state; the next row, in the branch it was in, keeps its
/// own, as Compose's `startReplaceGroup` deletes the unexpected group and inserts a new one
/// without looking further.
#[test]
fn a_branch_switch_in_a_loop_without_keys_keeps_the_next_rows_state() {
    let mut test = composition_test();
    let items = mutable_state_of(vec![true, false]);
    let (next_id, seen): (Rc<Cell<u32>>, Seen) = (Rc::default(), Rc::default());
    let (i, n, s) = (items.handle(), next_id.clone(), seen.clone());
    test.compose(move || Rows(i, n.clone(), s.clone()));
    assert_eq!(*seen.borrow(), [None, Some(1)]);
    items.set(vec![false, false]);
    test.expect_changes();
    assert_eq!(*seen.borrow(), [Some(2), Some(1)], "row 1 gets new state, row 2 keeps its own");
    test.validate(|v| {
        v.text("collapsed");
        v.text("collapsed");
    });
    test.verify_consistent();
}

#[composable]
fn Writer(trigger: MutableState<i32>, value: MutableState<i32>) {
    let current = trigger.get();
    value.set(current * 10);
}

#[composable]
fn Reader(value: MutableState<i32>, seen: Rc<Cell<i32>>) {
    seen.set(value.get());
}

/// A write to a state read by a scope the pass hasn't reached yet invalidates it for this pass
/// (D712): it recomposes in the same `recompose`, and no frame is asked for after.
#[test]
fn a_forward_write_recomposes_its_reader_in_the_same_pass() {
    let (trigger, value) = (mutable_state_of(0), mutable_state_of(0));
    let seen = Rc::new(Cell::new(-1));
    let requested = Rc::new(Cell::new(false));
    let composition = Composition::new(NodeTree::<()>::new());
    let r = requested.clone();
    composition.set_frame_requester(move || r.set(true));
    let (t, v, s) = (trigger.handle(), value.handle(), seen.clone());
    composition.set_content(move || {
        Writer(t, v);
        Reader(v, s.clone());
    });
    assert_eq!(seen.get(), 0);
    trigger.set(1);
    requested.set(false);
    composition.recompose();
    assert_eq!(seen.get(), 10, "the reader ran in the pass that wrote");
    assert!(!composition.has_invalidations(), "nothing left for another pass");
    assert!(!requested.get(), "no frame asked for");
}
