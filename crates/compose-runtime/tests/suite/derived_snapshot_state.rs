//! Compose's `DerivedSnapshotStateTests`, adapted: `derived_state_of` outside composition is
//! lazy and cached, and each snapshot sees its own result.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::platform;
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

// ---- After the review of S2 (D763-D767) ---------------------------------------------------------

/// A read of a nested derived state whose inner calculation sends, the send validating the outer
/// one an observation read, doesn't conflict with the validation in progress.
#[test]
fn a_nested_derived_state_read_by_an_observation_reads_after_a_write() {
    use compose_runtime::snapshots::Snapshot;
    let a = mutable_state_of(1);
    let ah = a.handle();
    let inner = derived_state_of(move || ah.get() * 2);
    let i = inner.as_state();
    let outer = derived_state_of(move || i.get() + 1);
    let o = outer.as_state();
    let observer = platform::ReadObserver::new(|| {});
    assert_eq!(observer.observe(1, || o.get()), 3);
    Snapshot::notify_objects_initialized();
    a.set(2);
    assert_eq!(outer.get(), 5);
}

/// The same from an apply observer, reading what a send told it about.
#[test]
fn a_nested_derived_state_read_in_an_apply_observer_reads_after_a_write() {
    use compose_runtime::snapshots::Snapshot;
    let a = mutable_state_of(1);
    let ah = a.handle();
    let inner = derived_state_of(move || ah.get() * 2);
    let i = inner.as_state();
    let outer = derived_state_of(move || i.get() + 1);
    let o = outer.as_state();
    let observer = platform::ReadObserver::new(|| {});
    assert_eq!(observer.observe(1, || o.get()), 3);
    let seen = Rc::new(Cell::new(0));
    let reads = seen.clone();
    let _observer = Snapshot::register_apply_observer(move |_, _| reads.set(o.get()));
    Snapshot::notify_objects_initialized();
    a.set(2);
    Snapshot::send_apply_notifications();
    assert_eq!(seen.get(), 5);
}

/// A read observer reading the derived state it's told about, on a cached read: no record is
/// held while it runs.
#[test]
fn a_read_observer_can_read_a_derived_state_it_hears_of() {
    use compose_runtime::snapshots::Snapshot;
    let a = mutable_state_of(1);
    let ah = a.handle();
    let derived = derived_state_of(move || ah.get() * 2);
    let read = derived.as_state();
    assert_eq!(read.get(), 2);
    let inside = Rc::new(Cell::new(false));
    let seen = Rc::new(Cell::new(0));
    let (flag, value) = (inside.clone(), seen.clone());
    Snapshot::observe(
        Some(Rc::new(move |_| {
            if !flag.replace(true) {
                ah.set(5);
                value.set(read.get());
                flag.set(false);
            }
        })),
        None,
        || read.get(),
    );
    assert_eq!(seen.get(), 10);
}

/// A freed dependency can't change: the derived state stays valid after an unrelated write.
#[test]
fn a_freed_dependency_counts_as_unchanged() {
    let a = mutable_state_of(1);
    let ah = a.handle();
    let other = mutable_state_of(0);
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let d = derived_state_of(move || {
        counted.set(counted.get() + 1);
        ah.try_get().unwrap_or(-1) * 2
    });
    assert_eq!(d.get(), 2);
    drop(a);
    other.set(1);
    assert_eq!(d.get(), 2);
    assert_eq!(runs.get(), 1);
}

/// A write during the send a recomputation causes is seen by the next read: the mark taken for
/// the fast path is the one before the send (D765).
#[test]
fn a_write_during_a_calculations_send_is_seen_by_the_next_read() {
    use compose_runtime::snapshots::Snapshot;
    let (a, b) = (mutable_state_of(0), mutable_state_of(0));
    let (ah, bh) = (a.handle(), b.handle());
    let derived = derived_state_of(move || ah.get());
    assert_eq!(derived.get(), 0);
    Snapshot::notify_objects_initialized();
    let writes = Rc::new(Cell::new(false));
    let once = writes.clone();
    let id = bh.state_object_id();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        if changed.contains(id) && !once.replace(true) {
            ah.set(2);
        }
    });
    a.set(1);
    b.set(1);
    // Recomputed at 1; its send writes 2.
    assert_eq!(derived.get(), 1);
    assert!(writes.get());
    assert_eq!(derived.get(), 2, "no read is behind what it derives from");
}

/// As Compose's `derivedStateOf(calculation)`, a calculation always writes a new record, so what
/// reads it is computed again; readers compare structurally.
#[test]
fn without_a_policy_an_equal_result_still_recomputes_what_reads_it() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let inner = derived_state_of(move || handle.get() > 0);
    let read_inner = inner.as_state();
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let outer = derived_state_of(move || {
        counted.set(counted.get() + 1);
        read_inner.get()
    });
    assert!(outer.get());
    state.set(2);
    assert!(outer.get());
    assert_eq!(runs.get(), 2);
}

/// With a policy, a result it finds equivalent keeps the record, and stops the chain.
#[test]
fn with_a_policy_an_equivalent_result_stops_the_chain() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let inner = derived_state_of_with(StructuralEqualityPolicy, move || handle.get() > 0);
    let read_inner = inner.as_state();
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let outer = derived_state_of(move || {
        counted.set(counted.get() + 1);
        read_inner.get()
    });
    assert!(outer.get());
    state.set(2);
    assert!(outer.get());
    assert_eq!(runs.get(), 1);
    state.set(0);
    assert!(!outer.get());
    assert_eq!(runs.get(), 2);
}

/// A never-equal state over an inner one whose policy finds the result equivalent isn't computed:
/// nothing it read changed. Over one without a policy it is, and its readers are told.
#[test]
fn a_never_equal_derived_state_follows_what_it_reads() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let kept = derived_state_of_with(StructuralEqualityPolicy, move || handle.get() > 0);
    let unkept = derived_state_of(move || handle.get() > 0);
    let (read_kept, read_unkept) = (kept.as_state(), unkept.as_state());
    let (over_kept_runs, over_unkept_runs) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (counted_kept, counted_unkept) = (over_kept_runs.clone(), over_unkept_runs.clone());
    let over_kept = derived_state_of_with(NeverEqualPolicy, move || {
        counted_kept.set(counted_kept.get() + 1);
        read_kept.get()
    });
    let over_unkept = derived_state_of_with(NeverEqualPolicy, move || {
        counted_unkept.set(counted_unkept.get() + 1);
        read_unkept.get()
    });
    assert!(over_kept.get() && over_unkept.get());
    state.set(2);
    assert!(over_kept.get() && over_unkept.get());
    assert_eq!(over_kept_runs.get(), 1);
    assert_eq!(over_unkept_runs.get(), 2);
}

thread_local! {
    static COMPARED: RefCell<Vec<(i32, i32)>> = const { RefCell::new(Vec::new()) };
}

struct Recording;

impl MutationPolicy<i32> for Recording {
    fn equivalent(a: &i32, b: &i32) -> bool {
        COMPARED.with(|compared| compared.borrow_mut().push((*a, *b)));
        a == b
    }
}

/// The policy is asked `equivalent(new, old)`, as `DerivedState.kt` does.
#[test]
fn a_policy_compares_the_new_result_with_the_old() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let derived = derived_state_of_with(Recording, move || handle.get());
    assert_eq!(derived.get(), 1);
    state.set(2);
    assert_eq!(derived.get(), 2);
    assert_eq!(COMPARED.with(|compared| compared.borrow()[0]), (2, 1));
}

/// On a recomputation, read observers hear the derived state first, as Compose's value getter.
#[test]
fn a_read_observer_hears_a_derived_state_before_what_its_calculation_reads() {
    use compose_runtime::snapshots::{Snapshot, StateObjectId};
    let state = mutable_state_of(0);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get());
    let read = derived.as_state();
    let reads: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
    let log = reads.clone();
    Snapshot::observe(Some(Rc::new(move |id| log.borrow_mut().push(id))), None, || read.get());
    assert_eq!(*reads.borrow(), [read.state_object_id(), state.state_object_id()]);
}

/// On a cached read of a diamond, what both sides read is heard once.
#[test]
fn a_cached_diamond_tells_what_it_read_once() {
    use compose_runtime::snapshots::{Snapshot, StateObjectId};
    let state = mutable_state_of(1);
    let handle = state.handle();
    let left = derived_state_of(move || handle.get() + 1);
    let right = derived_state_of(move || handle.get() + 2);
    let (read_left, read_right) = (left.as_state(), right.as_state());
    let top = derived_state_of(move || read_left.get() + read_right.get());
    let read = top.as_state();
    assert_eq!(read.get(), 5);
    let reads: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
    let log = reads.clone();
    Snapshot::observe(Some(Rc::new(move |id| log.borrow_mut().push(id))), None, || read.get());
    let heard = reads.borrow();
    assert_eq!(heard.iter().filter(|id| **id == state.state_object_id()).count(), 1);
    assert_eq!(heard.len(), 4, "the top, each side, the state");
}

/// An observation started in a calculation adds to it: what's read is a dependency too, as
/// Compose merges read observers.
#[test]
fn a_read_in_an_observation_inside_a_calculation_is_a_dependency() {
    use compose_runtime::snapshots::{Snapshot, SnapshotStateObserver};
    let state = mutable_state_of(1);
    let handle = state.handle();
    let observer = Rc::new(SnapshotStateObserver::new(|run| run()));
    observer.start();
    let changes = Rc::new(Cell::new(0));
    let counted = changes.clone();
    let callback: Rc<dyn Fn(&&'static str)> = Rc::new(move |_| counted.set(counted.get() + 1));
    let inside = observer.clone();
    let derived =
        derived_state_of(move || inside.observe_reads("scope", &callback, || handle.get()));
    assert_eq!(derived.get(), 1);
    Snapshot::notify_objects_initialized();
    state.set(2);
    assert_eq!(derived.get(), 2, "a dependency of the derived state");
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1, "and read in the observation");
    observer.stop();
}

/// A `with` closure holds no record: writing what the state read and reading it again works, the
/// value it's looking at staying as it was.
#[test]
fn a_with_closure_can_write_a_dependency_and_read_again() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() * 2);
    let read = derived.as_state();
    let (first, second) = read.with(|value| {
        handle.set(2);
        (*value, read.get())
    });
    assert_eq!((first, second), (2, 4));
}

// ---- In snapshots (step S5) ---------------------------------------------------------------------

mod in_snapshots {
    use super::*;
    use compose_runtime::snapshots::{Snapshot, StateObjectId};

    fn atomic<R>(block: impl FnOnce() -> R) -> R {
        Snapshot::with_mutable_snapshot(block)
    }

    // Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, statesCanBeUsedInSnapshot
    #[test]
    fn states_can_be_used_in_a_snapshot() {
        let a = mutable_state_of(1);
        let b = mutable_state_of(10);
        let (read_a, read_b) = (a.handle(), b.handle());
        let c = derived_state_of(move || read_a.get() + read_b.get());
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        assert_eq!(c.get(), 11);
        a.update(|value| value + 1);
        assert_eq!(c.get(), 12);
        b.update(|value| value + 10);
        assert_eq!(c.get(), 22);
        snapshot.dispose();
    }

    // Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, snapshotsAreIsolatedFromGlobalChanges
    #[test]
    fn snapshots_are_isolated_from_global_changes() {
        let state = mutable_state_of(0);
        let read = state.handle();
        let derived = derived_state_of(move || read.get());
        let snapshot = Snapshot::take_snapshot(None);
        state.set(1);
        assert_eq!(state.get(), 1);
        assert_eq!(derived.get(), 1);
        assert_eq!(snapshot.enter(|| state.get()), 0);
        assert_eq!(snapshot.enter(|| derived.get()), 0);
    }

    // Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, mutableSnapshotsCanBeApplied
    #[test]
    fn mutable_snapshots_can_be_applied() {
        let state = mutable_state_of(0);
        let read = state.handle();
        let derived = derived_state_of(move || read.get());
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        snapshot.enter(|| {
            assert_eq!(state.get(), 0);
            assert_eq!(derived.get(), 0);
            state.set(1);
            assert_eq!(state.get(), 1);
            assert_eq!(derived.get(), 1);
        });
        assert_eq!(state.get(), 0);
        assert_eq!(derived.get(), 0);
        snapshot.apply().check();
        assert_eq!(state.get(), 1);
        assert_eq!(derived.get(), 1);
        snapshot.dispose();
        atomic(|| {
            assert_eq!(state.get(), 1);
            assert_eq!(derived.get(), 1);
            state.set(2);
            assert_eq!(state.get(), 2);
            assert_eq!(derived.get(), 2);
        });
        assert_eq!(state.get(), 2);
        assert_eq!(derived.get(), 2);
    }

    // Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, multipleSnapshotsAreIsolatedAndCanBeApplied
    #[test]
    fn multiple_snapshots_are_isolated_and_can_be_applied() {
        let count = 2;
        let states: Vec<_> = (0..count).map(|_| mutable_state_of(0)).collect();
        let derived: Vec<_> = states
            .iter()
            .map(|state| {
                let read = state.handle();
                derived_state_of(move || read.get())
            })
            .collect();
        let snapshots: Vec<_> =
            (0..count).map(|_| Snapshot::take_mutable_snapshot(None, None)).collect();
        for index in 0..count {
            assert_eq!(states[index].get(), 0);
            assert_eq!(derived[index].get(), 0);
        }
        for (index, snapshot) in snapshots.iter().enumerate() {
            snapshot.enter(|| states[index].set(index));
        }
        for index in 0..count {
            assert_eq!(states[index].get(), 0);
            assert_eq!(derived[index].get(), 0);
        }
        for (index, snapshot) in snapshots.iter().enumerate() {
            snapshot.enter(|| {
                for other in 0..count {
                    let expected = if other == index { other } else { 0 };
                    assert_eq!(states[other].get(), expected);
                    assert_eq!(derived[other].get(), expected);
                }
            });
        }
        for snapshot in &snapshots {
            snapshot.apply().check();
        }
        for index in 0..count {
            assert_eq!(states[index].get(), index);
            assert_eq!(derived[index].get(), index);
        }
    }

    fn logging(
        log: &Rc<RefCell<Vec<StateObjectId>>>,
    ) -> Option<compose_runtime::snapshots::StateObserverFn> {
        let log = log.clone();
        Some(Rc::new(move |id| log.borrow_mut().push(id)))
    }

    // Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, stateReadsCanBeObserved
    #[test]
    fn state_reads_can_be_observed() {
        let state = mutable_state_of(0);
        let read = state.handle();
        let derived = derived_state_of(move || read.get());
        let reads: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
        let snapshot = Snapshot::take_snapshot(logging(&reads));
        assert_eq!(snapshot.enter(|| derived.get()), 0);
        assert_eq!(reads.borrow().len(), 2, "1 for the derived state, 1 for the state");
        assert!(reads.borrow().contains(&state.state_object_id()));
        assert!(reads.borrow().contains(&derived.as_state().state_object_id()));
    }

    // Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, stateReadsCanBeObservedEvenIfCached
    #[test]
    fn state_reads_can_be_observed_even_if_cached() {
        let state = mutable_state_of(0);
        let read = state.handle();
        let derived = derived_state_of(move || read.get());
        assert_eq!(derived.get(), 0);
        let reads: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
        let snapshot = Snapshot::take_snapshot(logging(&reads));
        assert_eq!(snapshot.enter(|| derived.get()), 0);
        assert_eq!(
            *reads.borrow(),
            [derived.as_state().state_object_id(), state.state_object_id()]
        );
    }

    // Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/DerivedSnapshotStateTests.kt, stateUpdatedInSnapshotIsNotRecalculated
    #[test]
    fn a_state_updated_in_a_snapshot_is_not_recalculated() {
        let runs = Rc::new(Cell::new(0));
        let dependency = mutable_state_of(0);
        let (count, read) = (runs.clone(), dependency.handle());
        let a = derived_state_of(move || {
            count.set(count.get() + 1);
            read.get()
        });
        let read_a = a.as_state();
        let b = derived_state_of(move || read_a.get());
        let read_in_a_snapshot = || {
            let snapshot = Snapshot::take_mutable_snapshot(None, None);
            snapshot.enter(|| b.get());
            snapshot.apply().check();
        };
        read_in_a_snapshot();
        dependency.update(|value| value + 1);
        read_in_a_snapshot();
        read_in_a_snapshot();
        assert_eq!(runs.get(), 2);
    }

    /// A calculation in a snapshot that reads other states than the global one's doesn't stop a
    /// write to what the global one's read from reaching a reader of the derived state (the
    /// debt S2 left: S5's records per snapshot).
    #[test]
    fn a_snapshots_calculation_leaves_the_global_ones_dependencies() {
        let (switch, left, right) =
            (mutable_state_of(true), mutable_state_of(1), mutable_state_of(2));
        let (read_switch, read_left, read_right) = (switch.handle(), left.handle(), right.handle());
        let derived =
            derived_state_of(
                move || {
                    if read_switch.get() { read_left.get() } else { read_right.get() }
                },
            );
        let read = derived.as_state();
        let notified = Rc::new(Cell::new(0));
        let count = notified.clone();
        let observer = platform::ReadObserver::new(move || count.set(count.get() + 1));
        observer.observe(1, || read.get());
        assert_eq!(read.get(), 1);
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        snapshot.enter(|| {
            switch.set(false);
            assert_eq!(read.get(), 2);
        });
        // Nothing changed for the global snapshot.
        assert_eq!(read.get(), 1);
        assert_eq!(notified.get(), 0);
        snapshot.dispose();
        left.set(10);
        Snapshot::send_apply_notifications();
        assert_eq!(read.get(), 10);
        assert_eq!(notified.get(), 1);
    }

    /// What a snapshot's calculation read counts once its record is the global one's: the applied
    /// snapshot's record stands, and a write to what it read reaches a reader.
    #[test]
    fn an_applied_calculations_dependencies_count() {
        let (switch, left, right) =
            (mutable_state_of(true), mutable_state_of(1), mutable_state_of(2));
        let (read_switch, read_left, read_right) = (switch.handle(), left.handle(), right.handle());
        let derived =
            derived_state_of(
                move || {
                    if read_switch.get() { read_left.get() } else { read_right.get() }
                },
            );
        let read = derived.as_state();
        let notified = Rc::new(Cell::new(0));
        let count = notified.clone();
        let observer = platform::ReadObserver::new(move || count.set(count.get() + 1));
        observer.observe(1, || read.get());
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        snapshot.enter(|| {
            switch.set(false);
            assert_eq!(read.get(), 2);
        });
        snapshot.apply().check();
        drop(snapshot);
        // The switch's apply told the reader.
        assert_eq!(notified.get(), 1);
        observer.drain_invalidated(|_| {});
        observer.observe(1, || assert_eq!(read.get(), 2));
        right.set(20);
        Snapshot::send_apply_notifications();
        assert_eq!(notified.get(), 2);
        assert_eq!(read.get(), 20);
    }

    /// The mark carries the snapshot: a read in a snapshot, then in another with no write between,
    /// doesn't take the first's result for its own.
    #[test]
    fn a_result_valid_in_one_snapshot_isnt_valid_in_another() {
        let state = mutable_state_of(1);
        let read = state.handle();
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let derived = derived_state_of(move || {
            count.set(count.get() + 1);
            read.get()
        });
        let first = Snapshot::take_mutable_snapshot(None, None);
        let second = Snapshot::take_mutable_snapshot(None, None);
        first.enter(|| state.set(10));
        second.enter(|| state.set(20));
        assert_eq!(first.enter(|| derived.get()), 10);
        assert_eq!(second.enter(|| derived.get()), 20);
        assert_eq!(first.enter(|| derived.get()), 10);
        assert_eq!(second.enter(|| derived.get()), 20);
        assert_eq!(runs.get(), 2, "each snapshot keeps its own result");
        assert_eq!(derived.get(), 1);
    }

    /// A derived state made in a snapshot is readable elsewhere, as a state is.
    #[test]
    fn a_derived_state_made_in_a_snapshot_is_born_accessible() {
        let state = mutable_state_of(1);
        let read = state.handle();
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        let derived = snapshot.enter(|| {
            let derived = derived_state_of(move || read.get());
            assert_eq!(derived.get(), 1);
            derived
        });
        state.set(2);
        assert_eq!(derived.get(), 2);
        assert_eq!(snapshot.enter(|| derived.get()), 1);
    }
}
