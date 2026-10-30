//! Compose's `DerivedSnapshotStateTests`, adapted: `derived_state_of` outside composition is
//! lazy and cached. The tests that take snapshots wait for snapshot isolation (D702).

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::*;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, aStateFreeCalculationCanBeUsed
#[test]
fn a_calculation_reading_no_state_gives_its_value() {
    let a = derived_state_of(|| 10);
    assert_eq!(a.get(), 10);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, theCalculationIsCached
#[test]
fn the_calculation_runs_at_the_first_read_and_is_cached() {
    let runs = Rc::new(Cell::new(0));
    let i = Rc::new(Cell::new(0));
    let (r, value) = (runs.clone(), i.clone());
    let a = derived_state_of(move || {
        r.set(r.get() + 1);
        value.get()
    });
    assert_eq!(runs.get(), 0, "the calculation runs only when the value is first requested");
    i.set(1);
    assert_eq!(a.get(), 1, "computed at the first read");
    i.set(2);
    assert_eq!(a.get(), 1, "cached: what it read isn't state");
    assert_eq!(runs.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, statesCanBeUsedInGlobalSnapshot
#[test]
fn a_derived_state_follows_the_states_it_reads() {
    let (a, b) = (mutable_state_of(1), mutable_state_of(10));
    let (ah, bh) = (a.handle(), b.handle());
    let c = derived_state_of(move || ah.get() + bh.get());
    assert_eq!(c.get(), 11);
    a.update(|a| a + 1);
    assert_eq!(c.get(), 12);
    b.update(|b| b + 10);
    assert_eq!(c.get(), 22);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, nullResultIsCached
#[test]
fn a_none_result_is_cached() {
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let a = derived_state_of(move || {
        r.set(r.get() + 1);
        None::<i32>
    });
    assert_eq!(a.get(), None);
    assert_eq!(runs.get(), 1);
    assert_eq!(a.get(), None);
    assert_eq!(runs.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, stateUpdatedInSnapshotIsNotRecalculated
#[test]
fn a_derived_state_read_through_another_recomputes_once_per_change() {
    // Compose reads `b` in mutable snapshots it applies; here, plain reads.
    let runs = Rc::new(Cell::new(0));
    let dependency = mutable_state_of(0);
    let (r, d) = (runs.clone(), dependency.handle());
    let a = derived_state_of(move || {
        r.set(r.get() + 1);
        d.get()
    });
    let a_state = a.as_state();
    let b = derived_state_of(move || a_state.get());
    b.get();
    dependency.update(|d| d + 1);
    b.get();
    b.get();
    assert_eq!(runs.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, nestedDerivedStatesProduceAResult
#[test]
fn nested_derived_states_produce_a_result() {
    let n = 3;
    let state = mutable_state_of(1);
    let handle = state.handle();
    let first: Vec<DerivedState<i32>> =
        (0..n).map(|_| derived_state_of(move || handle.get())).collect();
    let first_states: Rc<Vec<State<i32>>> =
        Rc::new(first.iter().map(DerivedState::as_state).collect());
    let second: Vec<DerivedState<i32>> = (0..n)
        .map(|_| {
            let states = first_states.clone();
            derived_state_of(move || states.iter().map(State::get).sum())
        })
        .collect();
    let second_states: Vec<State<i32>> = second.iter().map(DerivedState::as_state).collect();
    let third = derived_state_of(move || second_states.iter().map(State::get).sum::<i32>());
    assert_eq!(third.get(), n * n);
    state.set(2);
    assert_eq!(third.get(), n * n * 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, stateReadsCanBeObserved
// On `Snapshot::observe`: taking a snapshot with its read observer comes with step S5.
#[test]
fn a_read_observer_hears_a_derived_state_and_what_it_read() {
    use compose_runtime::snapshots::{Snapshot, StateObjectId};
    let state = mutable_state_of(0);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get());
    let read = derived.as_state();
    let reads: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
    let log = reads.clone();
    let result =
        Snapshot::observe(Some(Rc::new(move |id| log.borrow_mut().push(id))), None, || read.get());
    assert_eq!(result, 0);
    assert_eq!(reads.borrow().len(), 2, "1 for the derived state, 1 for the state");
    assert!(reads.borrow().contains(&state.state_object_id()));
    assert!(reads.borrow().contains(&read.state_object_id()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, stateReadsCanBeObservedEvenIfCached
#[test]
fn a_read_observer_hears_what_a_cached_derived_state_read() {
    use compose_runtime::snapshots::{Snapshot, StateObjectId};
    let state = mutable_state_of(0);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get());
    let read = derived.as_state();
    assert_eq!(read.get(), 0);
    let reads: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
    let log = reads.clone();
    let result =
        Snapshot::observe(Some(Rc::new(move |id| log.borrow_mut().push(id))), None, || read.get());
    assert_eq!(result, 0);
    assert_eq!(*reads.borrow(), [read.state_object_id(), state.state_object_id()]);
}
