//! Snapshot isolation (docs/parity/design/snapshots.md, step S5): read-only, mutable and nested
//! snapshots, apply and merge, reclamation, as Compose's `SnapshotTests` and
//! `SnapshotContextElementTests`.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use compose_runtime::platform::state_records;
use compose_runtime::snapshots::{
    MergeResult, MutableSnapshot, Snapshot, SnapshotApplyConflict, SnapshotApplyResult,
    StateObjectId, StateObserverFn, StateRecord, mutable_state_list_of, mutable_state_map_of,
    snapshot_state_object,
};
use compose_runtime::*;

/// Checks at its end that the test left no snapshot open, as `validateOpenSnapshots`.
struct OpenSnapshots(usize);

fn open_snapshots() -> OpenSnapshots {
    OpenSnapshots(Snapshot::open_snapshot_count())
}

impl Drop for OpenSnapshots {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_eq!(self.0, Snapshot::open_snapshot_count(), "a snapshot wasn't disposed");
        }
    }
}

fn mutable() -> MutableSnapshot {
    Snapshot::take_mutable_snapshot(None, None)
}

fn read_only() -> Snapshot {
    Snapshot::take_snapshot(None)
}

/// As the test file's `atomic`.
fn atomic<R>(block: impl FnOnce() -> R) -> R {
    let snapshot = mutable();
    let result = snapshot.enter(block);
    snapshot.apply().check();
    result
}

/// As the test file's `snapshot`.
fn snapshot_of(block: impl FnOnce()) -> MutableSnapshot {
    let snapshot = mutable();
    snapshot.enter(block);
    snapshot
}

fn observer(log: &Rc<RefCell<Vec<StateObjectId>>>) -> Option<StateObserverFn> {
    let log = log.clone();
    Some(Rc::new(move |state| log.borrow_mut().push(state)))
}

fn counter(count: &Rc<Cell<usize>>) -> Option<StateObserverFn> {
    let count = count.clone();
    Some(Rc::new(move |_| count.set(count.get() + 1)))
}

fn tagged(log: &Rc<RefCell<Vec<i32>>>, tag: i32) -> Option<StateObserverFn> {
    let log = log.clone();
    Some(Rc::new(move |_| log.borrow_mut().push(tag)))
}

/// Whether `block` panics.
fn panics(block: impl FnOnce()) -> bool {
    catch_unwind(AssertUnwindSafe(block)).is_err()
}

/// The panic's message.
fn panic_message(block: impl FnOnce()) -> String {
    let payload = catch_unwind(AssertUnwindSafe(block)).expect_err("a panic");
    match (payload.downcast_ref::<String>(), payload.downcast_ref::<&str>()) {
        (Some(message), _) => message.clone(),
        (_, Some(message)) => message.to_string(),
        _ => String::new(),
    }
}

/// How many applies changed `state` while `block` ran and was sent, as `changesOf`.
fn changes_of(state: StateObjectId, block: impl FnOnce()) -> usize {
    let changes = Rc::new(Cell::new(0));
    let counted = changes.clone();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        if changed.contains(state) {
            counted.set(counted.get() + 1);
        }
    });
    block();
    Snapshot::send_apply_notifications();
    changes.get()
}

/// What `snapshot`'s apply changed while `block` ran and was sent, as `observeChanges`.
fn observe_changes(snapshot: &Snapshot, block: impl FnOnce()) -> HashSet<StateObjectId> {
    let changes: Rc<RefCell<HashSet<StateObjectId>>> = Rc::default();
    let log = changes.clone();
    let target = snapshot.handle();
    let _observer = Snapshot::register_apply_observer(move |changed, snapshot| {
        if *snapshot == target {
            *log.borrow_mut() = changed.iter().collect();
        }
    });
    block();
    Snapshot::send_apply_notifications();
    changes.take()
}

fn used_records<T: 'static>(state: MutableState<T>) -> usize {
    state_records(state).0
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, aSnapshotCanBeCreated
#[test]
fn a_snapshot_can_be_created() {
    let _open = open_snapshots();
    read_only().dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, aMutableStateCanBeCreated, aMutableStateCanBeReadOutsideASnapshot, aMutableStateCanBeWrittenToOutsideASnapshot
#[test]
fn a_state_works_outside_a_snapshot() {
    let state = mutable_state_of("0");
    assert_eq!(state.get(), "0");
    state.set("1");
    assert_eq!(state.get(), "1");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, snapshotsAreIsolatedFromGlobalChanges
#[test]
fn snapshots_are_isolated_from_global_changes() {
    let _open = open_snapshots();
    let state = mutable_state_of("0");
    let snapshot = read_only();
    state.set("1");
    assert_eq!(state.get(), "1");
    assert_eq!(snapshot.enter(|| state.get()), "0");
    snapshot.dispose();
    assert_eq!(state.get(), "1");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, mutableSnapshotsCanBeApplied
#[test]
fn mutable_snapshots_can_be_applied() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| {
        assert_eq!(state.get(), 0);
        state.set(1);
        assert_eq!(state.get(), 1);
    });
    assert_eq!(state.get(), 0);
    snapshot.apply().check();
    assert_eq!(state.get(), 1);
    snapshot.dispose();
    atomic(|| {
        assert_eq!(state.get(), 1);
        state.set(2);
        assert_eq!(state.get(), 2);
    });
    assert_eq!(state.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, multipleSnapshotsAreIsolatedAndCanBeApplied
#[test]
fn multiple_snapshots_are_isolated_and_can_be_applied() {
    let _open = open_snapshots();
    let count = 2;
    let states: Vec<_> = (0..count).map(|_| mutable_state_of(0)).collect();
    let snapshots: Vec<_> = (0..count).map(|_| mutable()).collect();
    for (index, snapshot) in snapshots.iter().enumerate() {
        snapshot.enter(|| states[index].set(index));
    }
    assert!(states.iter().all(|state| state.get() == 0));
    for (index, snapshot) in snapshots.iter().enumerate() {
        snapshot.enter(|| {
            for (other, state) in states.iter().enumerate() {
                assert_eq!(state.get(), if other == index { other } else { 0 });
            }
        });
    }
    for snapshot in &snapshots {
        snapshot.apply().check();
    }
    for (index, state) in states.iter().enumerate() {
        assert_eq!(state.get(), index);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, applyingASnapshotThatCollidesWithAGlobalChangeWillFail
#[test]
fn applying_a_snapshot_that_collides_with_a_global_change_fails() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = snapshot_of(|| state.set(1));
    state.set(2);
    assert!(matches!(snapshot.apply(), SnapshotApplyResult::Failure(_)));
    assert_eq!(state.get(), 2);
    snapshot.dispose();
    assert_eq!(state.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, applyingCollidingSnapshotsWillFail
#[test]
fn applying_colliding_snapshots_fails() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot1 = snapshot_of(|| state.set(1));
    let snapshot2 = snapshot_of(|| state.set(2));
    assert_eq!(state.get(), 0);
    snapshot1.apply().check();
    assert_eq!(state.get(), 1);
    assert!(!snapshot2.apply().succeeded());
    assert_eq!(state.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, stateReadsCanBeObserved
#[test]
fn state_reads_can_be_observed() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let reads: Rc<RefCell<Vec<_>>> = Rc::default();
    let snapshot = Snapshot::take_snapshot(observer(&reads));
    let result = snapshot.enter(|| state.get());
    assert_eq!(result, 0);
    assert_eq!(*reads.borrow(), [state.state_object_id()]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, stateWritesCanBeObserved
#[test]
fn state_writes_can_be_observed() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let writes: Rc<RefCell<Vec<_>>> = Rc::default();
    let snapshot = Snapshot::take_mutable_snapshot(None, observer(&writes));
    snapshot.enter(|| {
        assert!(writes.borrow().is_empty());
        state.set(2);
        assert_eq!(writes.borrow().len(), 1);
    });
    snapshot.dispose();
    assert_eq!(*writes.borrow(), [state.state_object_id()]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, appliesCanBeObserved
#[test]
fn applies_can_be_observed() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let id = state.state_object_id();
    let observed: Rc<RefCell<Option<Snapshot>>> = Rc::default();
    let log = observed.clone();
    let _observer = Snapshot::register_apply_observer(move |changed, snapshot| {
        assert!(changed.contains(id));
        *log.borrow_mut() = Some(snapshot.handle());
    });
    let snapshot = mutable();
    snapshot.enter(|| state.set(2));
    assert!(observed.borrow().is_none());
    snapshot.apply().check();
    assert!(observed.borrow().as_ref().is_some_and(|observed| *observed == snapshot));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, aNestedSnapshotCanBeTaken
#[test]
fn a_nested_snapshot_can_be_taken() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = read_only();
    let nested = snapshot.take_nested_snapshot(None);
    state.set(1);
    assert_eq!(nested.enter(|| state.get()), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, aNestedMutableSnapshotCanBeTaken
#[test]
fn a_nested_mutable_snapshot_can_be_taken() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| state.set(1));
    let nested = snapshot.take_nested_mutable_snapshot(None, None);
    nested.enter(|| state.set(2));
    assert_eq!(state.get(), 0);
    assert_eq!(snapshot.enter(|| state.get()), 1);
    assert_eq!(nested.enter(|| state.get()), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, aNestedSnapshotOfAMutableSnapshotCanBeTaken
#[test]
fn a_nested_snapshot_of_a_mutable_snapshot_can_be_taken() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| state.set(1));
    let nested = snapshot.take_nested_snapshot(None);
    snapshot.enter(|| state.set(2));
    assert_eq!(state.get(), 0);
    assert_eq!(snapshot.enter(|| state.get()), 2);
    assert_eq!(nested.enter(|| state.get()), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, aNestedMutableSnapshotCanBeAppliedToItsParent
#[test]
fn a_nested_mutable_snapshot_can_be_applied_to_its_parent() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| state.set(1));
    let nested = snapshot.take_nested_mutable_snapshot(None, None);
    nested.enter(|| state.set(2));
    assert_eq!(state.get(), 0);
    assert_eq!(snapshot.enter(|| state.get()), 1);
    assert_eq!(nested.enter(|| state.get()), 2);
    nested.apply().check();
    nested.dispose();
    assert_eq!(state.get(), 0);
    assert_eq!(snapshot.enter(|| state.get()), 2);
    snapshot.apply().check();
    snapshot.dispose();
    assert_eq!(state.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, aParentSnapshotCanAccessAStatObjectedCreateByANestedSnapshot
#[test]
fn a_parent_snapshot_can_access_a_state_made_by_a_nested_snapshot() {
    let _open = open_snapshots();
    let snapshot = mutable();
    let nested = snapshot.take_nested_mutable_snapshot(None, None);
    nested.enter(Snapshot::notify_objects_initialized);
    let state = nested.enter(|| mutable_state_of(1));
    assert_eq!(nested.enter(|| state.get()), 1);
    nested.apply().check();
    nested.dispose();
    assert_eq!(snapshot.enter(|| state.get()), 1);
    snapshot.apply().check();
    snapshot.dispose();
    assert_eq!(state.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, atomicChangesNest
#[test]
fn atomic_changes_nest() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    atomic(|| {
        state.set(1);
        atomic(|| {
            state.set(2);
            assert_eq!(Snapshot::global(|| state.get()), 0);
        });
        assert_eq!(state.get(), 2);
        assert_eq!(Snapshot::global(|| state.get()), 0);
    });
    assert_eq!(state.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, siblingNestedMutableSnapshotsAreIsolatedFromEachOther
#[test]
fn sibling_nested_mutable_snapshots_are_isolated_from_each_other() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| state.set(10));
    let nested1 = snapshot.take_nested_mutable_snapshot(None, None);
    nested1.enter(|| state.set(1));
    let nested2 = snapshot.take_nested_mutable_snapshot(None, None);
    nested2.enter(|| state.set(2));
    assert_eq!(state.get(), 0);
    assert_eq!(snapshot.enter(|| state.get()), 10);
    assert_eq!(nested1.enter(|| state.get()), 1);
    assert_eq!(nested2.enter(|| state.get()), 2);
    drop((nested1, nested2, snapshot));
    assert_eq!(state.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readingInANestedSnapshotNotifiesTheParent
#[test]
fn reading_in_a_nested_snapshot_notifies_the_parent() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let read: Rc<RefCell<Vec<_>>> = Rc::default();
    let snapshot = Snapshot::take_snapshot(observer(&read));
    let nested = snapshot.take_nested_snapshot(None);
    assert_eq!(nested.enter(|| state.get()), 0);
    assert!(read.borrow().contains(&state.state_object_id()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readingInANestedSnapshotNotifiesNestedAndItsParent
#[test]
fn reading_in_a_nested_snapshot_notifies_nested_and_its_parent() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let parent_read: Rc<RefCell<Vec<_>>> = Rc::default();
    let nested_read: Rc<RefCell<Vec<_>>> = Rc::default();
    let snapshot = Snapshot::take_snapshot(observer(&parent_read));
    let nested = snapshot.take_nested_snapshot(observer(&nested_read));
    assert_eq!(nested.enter(|| state.get()), 0);
    assert!(parent_read.borrow().contains(&state.state_object_id()));
    assert!(nested_read.borrow().contains(&state.state_object_id()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, writingToANestedSnapshotNotifiesTheParent
#[test]
fn writing_to_a_nested_snapshot_notifies_the_parent() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let written: Rc<RefCell<Vec<_>>> = Rc::default();
    let snapshot = Snapshot::take_mutable_snapshot(None, observer(&written));
    let nested = snapshot.take_nested_mutable_snapshot(None, None);
    nested.enter(|| state.set(2));
    assert!(written.borrow().contains(&state.state_object_id()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, writingToANestedSnapshotNotifiesNestedAndItsParent
#[test]
fn writing_to_a_nested_snapshot_notifies_nested_and_its_parent() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let parent_written: Rc<RefCell<Vec<_>>> = Rc::default();
    let nested_written: Rc<RefCell<Vec<_>>> = Rc::default();
    let snapshot = Snapshot::take_mutable_snapshot(None, observer(&parent_written));
    let nested = snapshot.take_nested_mutable_snapshot(None, observer(&nested_written));
    nested.enter(|| state.set(2));
    assert!(parent_written.borrow().contains(&state.state_object_id()));
    assert!(nested_written.borrow().contains(&state.state_object_id()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, creatingAStateInANestedSnapshotAndMutatingInParentApplies
#[test]
fn a_state_made_in_a_nested_snapshot_and_written_in_its_parent_applies() {
    let _open = open_snapshots();
    let states = RefCell::new(Vec::new());
    let snapshot = mutable();
    let nested = snapshot.take_nested_mutable_snapshot(None, None);
    nested.enter(|| states.borrow_mut().push(mutable_state_of(0)));
    nested.apply().check();
    nested.dispose();
    snapshot.enter(|| {
        for state in states.borrow().iter() {
            state.update(|value| value + 1);
        }
    });
    snapshot.apply().check();
    snapshot.dispose();
    for state in states.borrow().iter() {
        assert_eq!(state.get(), 1);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, snapshotsChangesCanMerge
#[test]
fn snapshots_changes_can_merge() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot1 = mutable();
    let snapshot2 = mutable();
    snapshot1.enter(|| state.set(1));
    snapshot2.enter(|| state.set(1));
    assert_eq!(state.get(), 0);
    snapshot1.apply().check();
    assert_eq!(state.get(), 1);
    snapshot2.apply().check();
    assert_eq!(state.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, mergedSnapshotsDoNotRepeatChangeNotifications
#[test]
fn merged_snapshots_do_not_repeat_change_notifications() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot1 = mutable();
    let snapshot2 = mutable();
    let changes = changes_of(state.state_object_id(), || {
        snapshot1.enter(|| state.set(1));
        snapshot2.enter(|| state.set(1));
        snapshot1.apply().check();
        snapshot2.apply().check();
    });
    assert_eq!(changes, 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, statesWithStructuralEqualityPolicyMerge
#[test]
fn states_with_the_structural_policy_merge() {
    let _open = open_snapshots();
    #[derive(Clone, PartialEq)]
    struct Value(i32, i32);
    let state = mutable_state_of_with(Value(1, 2), structural_equality_policy());
    let snapshot1 = mutable();
    let snapshot2 = mutable();
    snapshot1.enter(|| state.set(Value(3, 4)));
    snapshot2.enter(|| state.set(Value(3, 4)));
    snapshot1.apply().check();
    snapshot2.apply().check();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, stateUsingNeverEqualPolicyCannotBeMerged
#[test]
fn a_state_with_the_never_equal_policy_cant_be_merged() {
    let _open = open_snapshots();
    let state = mutable_state_of_with(0, never_equal_policy());
    let snapshot1 = mutable();
    let snapshot2 = mutable();
    snapshot1.enter(|| state.set(1));
    snapshot2.enter(|| state.set(1));
    snapshot1.apply().check();
    let payload = catch_unwind(AssertUnwindSafe(|| snapshot2.apply().check())).expect_err("fails");
    assert!(payload.downcast_ref::<SnapshotApplyConflict>().is_some());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, nestedSnapshotsAreIsolated
#[test]
fn nested_snapshots_are_isolated() {
    let _open = open_snapshots();
    let state1 = mutable_state_of(0);
    let state2 = mutable_state_of(0);
    let parent = mutable();
    parent.enter(|| state1.set(1));
    Snapshot::with_mutable_snapshot(|| state2.set(2));
    let snapshot = parent.take_nested_snapshot(None);
    parent.apply().check();
    parent.dispose();
    snapshot.enter(|| {
        assert_eq!(state1.get(), 1);
        assert_eq!(state2.get(), 0);
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnlySnapshotValidAfterParentDisposed
#[test]
fn a_read_only_snapshot_is_valid_after_its_parent_is_disposed() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let parent = mutable();
    parent.enter(|| state.set(1));
    let child = parent.take_nested_snapshot(None);
    parent.apply().check();
    parent.dispose();
    child.enter(|| assert_eq!(state.get(), 1));
    let reads: Rc<RefCell<Vec<_>>> = Rc::default();
    let nested_child = child.take_nested_snapshot(observer(&reads));
    nested_child.enter(|| assert_eq!(state.get(), 1));
    assert_eq!(reads.borrow().len(), 1);
    drop((nested_child, child));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, transparentSnapshotAdvancesCorrectly
#[test]
fn an_observation_advances_correctly() {
    let _open = open_snapshots();
    let state = Snapshot::observe(Some(Rc::new(|_| {})), None, || {
        Snapshot::notify_objects_initialized();
        let state = atomic(|| mutable_state_of(0));
        assert_eq!(state.get(), 0);
        state
    });
    assert_eq!(state.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testTakeSnapshotNested
#[test]
fn a_snapshot_taken_in_a_mutable_one_is_read_only() {
    let _open = open_snapshots();
    assert!(panics(|| {
        Snapshot::with_mutable_snapshot(|| {
            let snapshot = read_only();
            snapshot.enter(|| {
                let state = mutable_state_of(0);
                state.set(1);
            });
        });
    }));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, nestedMutableSnapshotCanNotSeeOtherSnapshotChange
#[test]
fn a_nested_mutable_snapshot_cant_see_another_snapshots_change() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot1 = mutable();
    let snapshot2 = mutable();
    snapshot2.enter(|| state.set(1));
    snapshot1.enter(|| Snapshot::with_mutable_snapshot(|| assert_eq!(state.get(), 0)));
    drop((snapshot1, snapshot2));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, nestedSnapshotCanNotSeeOtherSnapshotChange
#[test]
fn a_nested_snapshot_cant_see_another_snapshots_change() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot1 = mutable();
    let snapshot2 = mutable();
    snapshot2.enter(|| state.set(1));
    snapshot1.enter(|| {
        let nested = read_only();
        nested.enter(|| assert_eq!(state.get(), 0));
    });
    drop((snapshot1, snapshot2));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, canTakeNestedSnapshotsFromApplyObserver
#[test]
fn nested_snapshots_can_be_taken_from_an_apply_observer() {
    let _open = open_snapshots();
    let taken: Rc<RefCell<Option<Snapshot>>> = Rc::default();
    let log = taken.clone();
    let _observer = Snapshot::register_apply_observer(move |_, snapshot| {
        assert!(log.borrow().is_none(), "already took a nested snapshot");
        *log.borrow_mut() = Some(snapshot.take_nested_snapshot(None));
    });
    let state = mutable_state_of("initial");
    Snapshot::with_mutable_snapshot(|| state.set("before observer snapshot"));
    state.set("after observer snapshot");
    let snapshot = taken.take().expect("taken by the observer");
    snapshot.enter(|| assert_eq!(state.get(), "before observer snapshot"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, canTakeNestedMutableSnapshotsFromApplyObserver
#[test]
fn nested_mutable_snapshots_can_be_taken_from_an_apply_observer() {
    let _open = open_snapshots();
    let taken: Rc<RefCell<Option<MutableSnapshot>>> = Rc::default();
    let log = taken.clone();
    let _observer = Snapshot::register_apply_observer(move |_, snapshot| {
        assert!(log.borrow().is_none(), "already took a nested snapshot");
        let snapshot = snapshot.as_mutable().expect("a mutable snapshot was applied");
        *log.borrow_mut() = Some(snapshot.take_nested_mutable_snapshot(None, None));
    });
    let state = mutable_state_of("initial");
    Snapshot::with_mutable_snapshot(|| state.set("before observer snapshot"));
    state.set("after observer snapshot");
    let snapshot = taken.take().expect("taken by the observer");
    snapshot.enter(|| {
        assert_eq!(state.get(), "before observer snapshot");
        state.set("change made by observer snapshot");
    });
    assert!(!snapshot.apply().succeeded(), "its parent was applied");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, cannotTakeSnapshotOfClosedSnapshotAfterApplyReturns
#[test]
fn a_snapshot_of_a_closed_snapshot_cant_be_taken_after_apply_returns() {
    let _open = open_snapshots();
    // In Compose's test the state is made after the snapshot, so it's the write that throws.
    let state = mutable_state_of("initial");
    let snapshot = mutable();
    snapshot.enter(|| state.set("mutated"));
    snapshot.apply().check();
    assert!(panics(|| drop(snapshot.take_nested_snapshot(None))));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, cannotApplyASnapshotTwice
#[test]
fn a_snapshot_cant_be_applied_twice() {
    let _open = open_snapshots();
    let state = mutable_state_of("initial");
    let snapshot = mutable();
    snapshot.enter(|| state.set("mutated"));
    snapshot.apply().check();
    let message = panic_message(|| snapshot.apply().check());
    assert!(message.contains("Snapshot is not open") && message.contains("applied="), "{message}");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, cannotApplyAfterADispose
#[test]
fn a_snapshot_cant_be_applied_after_a_dispose() {
    let _open = open_snapshots();
    let state = mutable_state_of("initial");
    let snapshot = mutable();
    snapshot.enter(|| state.set("mutated"));
    snapshot.dispose();
    let message = panic_message(|| snapshot.apply().check());
    assert!(message.contains("Snapshot is not open") && message.contains("applied="), "{message}");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testRecordsAreReusedCorrectly
#[test]
fn records_are_reused_correctly() {
    let _open = open_snapshots();
    let value = mutable_state_of(0);
    Snapshot::with_mutable_snapshot(|| value.update(|value| value + 1));
    let mutable1 = mutable();
    let readable1 = read_only();
    mutable1.enter(|| value.update(|value| value + 1));
    mutable1.apply().check();
    mutable1.dispose();
    Snapshot::with_mutable_snapshot(|| value.update(|value| value + 1));
    assert_eq!(readable1.enter(|| value.get()), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testUnsafeSnapshotEnterAndLeave
#[test]
fn a_snapshot_can_be_entered_and_left_unsafely() {
    let _open = open_snapshots();
    let snapshot = read_only();
    let old = snapshot.unsafe_enter();
    assert_eq!(snapshot, Snapshot::current(), "expected the taken snapshot to be current");
    snapshot.unsafe_leave(old);
    assert_ne!(snapshot, Snapshot::current(), "expected the taken snapshot not to be current");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testUnsafeSnapshotLeaveThrowsIfNotCurrent
#[test]
fn unsafely_leaving_a_snapshot_that_isnt_current_panics() {
    let _open = open_snapshots();
    let snapshot = read_only();
    assert!(panics(|| snapshot.unsafe_leave(None)));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testNestedWithinTransparentSnapshotDisposedCorrectly, testNestedWithinTransparentMutableSnapshotDisposedCorrectly
#[test]
fn a_snapshot_nested_within_an_observation_is_disposed_correctly() {
    let _open = open_snapshots();
    Snapshot::observe(Some(Rc::new(|_| {})), None, || {
        let inner = Snapshot::current().take_nested_snapshot(None);
        inner.enter(|| {});
        inner.dispose();
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testTransparentSnapshotMergedWithNestedReadObserver, testTransparentMutableSnapshotMergedWithNestedReadObserver
#[test]
fn an_observation_is_merged_with_a_nested_read_observer() {
    let _open = open_snapshots();
    let (outer, inner) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let state = mutable_state_of(0);
    Snapshot::observe(counter(&outer), None, || {
        let nested = Snapshot::current().take_nested_snapshot(counter(&inner));
        nested.enter(|| state.get());
    });
    assert_eq!(outer.get(), 1);
    assert_eq!(inner.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testSimpleReclaimingState
#[test]
fn a_state_is_back_to_one_record_after_an_apply() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    assert_eq!(used_records(state.handle()), 1);
    Snapshot::with_mutable_snapshot(|| {
        state.set(1);
        assert_eq!(used_records(state.handle()), 2);
    });
    assert_eq!(used_records(state.handle()), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testDeferredReclaimingState_Merged
#[test]
fn records_are_reclaimed_after_merged_applies() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    assert_eq!(used_records(state.handle()), 1);
    let snapshots = [mutable(), mutable(), mutable()];
    for snapshot in &snapshots {
        snapshot.enter(|| state.set(1));
    }
    for snapshot in &snapshots {
        snapshot.apply().check();
    }
    assert_eq!(used_records(state.handle()), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testDeferredReclaimingState_MultipleObjects
#[test]
fn records_of_several_states_are_reclaimed_when_nothing_can_read_them() {
    let _open = open_snapshots();
    let states = [mutable_state_of(0), mutable_state_of(0), mutable_state_of(0)];
    let read_only_snapshot = read_only();
    let snapshots = [mutable(), mutable(), mutable()];
    for (index, snapshot) in snapshots.iter().enumerate() {
        snapshot.enter(|| states[index].set(index + 1));
    }
    snapshots[0].apply().check();
    snapshots[1].apply().check();
    read_only_snapshot.enter(|| {
        assert!(states.iter().all(|state| state.get() == 0));
    });
    read_only_snapshot.dispose();
    snapshots[2].apply().check();
    assert!(states.iter().all(|state| used_records(state.handle()) == 1));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testWriteCount
#[test]
fn writes_are_counted() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let written: Rc<RefCell<Vec<_>>> = Rc::default();
    let snapshot = Snapshot::take_mutable_snapshot(None, observer(&written));
    snapshot.enter(|| {
        assert!(written.borrow().is_empty());
        assert_eq!(snapshot.write_count(), 0);
        state.set(2);
        assert_eq!(written.borrow().len(), 1);
        assert_eq!(snapshot.write_count(), 1);
    });
    snapshot.dispose();
    assert_eq!(*written.borrow(), [state.state_object_id()]);
    assert_eq!(Snapshot::current().write_count(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testTransparentSnapshotWriteCount
#[test]
fn writes_in_an_observation_are_counted_in_the_global_snapshot() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    Snapshot::notify_objects_initialized();
    Snapshot::observe(None, Some(Rc::new(|_| {})), || {
        assert_eq!(Snapshot::current().write_count(), 0);
        state.set(2);
        assert_eq!(Snapshot::current().write_count(), 1);
    });
    assert_eq!(Snapshot::current().write_count(), 1);
}

fn born_accessible<T, V: PartialEq + std::fmt::Debug>(
    create: impl FnOnce() -> T,
    id: impl Fn(&T) -> StateObjectId,
    read: impl Fn(&T) -> V,
    update: impl FnOnce(&T) -> V,
) {
    let snapshot = mutable();
    let created = RefCell::new(None);
    let modified = observe_changes(&snapshot, || {
        let (state, initial) = snapshot.enter(|| {
            let state = create();
            let initial = read(&state);
            (state, initial)
        });
        assert_eq!(initial, read(&state));
        let new_value = snapshot.enter(|| update(&state));
        assert_ne!(initial, new_value);
        assert_eq!(initial, read(&state));
        snapshot.apply().check();
        assert_eq!(new_value, read(&state));
        *created.borrow_mut() = Some(id(&state));
        // The state lives on in `created` by its id only: it's dropped here.
    });
    assert!(!modified.contains(&created.take().expect("made")));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testSnapshotStateIsBornAccessible
#[test]
fn a_state_is_born_accessible() {
    let _open = open_snapshots();
    born_accessible(
        || mutable_state_of("A"),
        |state| state.state_object_id(),
        |state| state.get(),
        |state| {
            state.set("B");
            "B"
        },
    );
    born_accessible(
        || mutable_state_of(1),
        |state| state.state_object_id(),
        |state| state.get(),
        |state| {
            state.set(2);
            2
        },
    );
    born_accessible(
        mutable_state_list_of::<i32>,
        |list| list.state_object_id(),
        |list| list.is_empty(),
        |list| {
            list.push(1);
            list.is_empty()
        },
    );
    born_accessible(
        mutable_state_map_of::<i32, i32>,
        |map| map.state_object_id(),
        |map| map.is_empty(),
        |map| {
            map.insert(23, 42);
            map.is_empty()
        },
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readObserverIsMergedOnNestedReadonlySnapshot
#[test]
fn a_read_observer_is_merged_on_a_nested_read_only_snapshot() {
    let _open = open_snapshots();
    let result: Rc<RefCell<Vec<i32>>> = Rc::default();
    let state = mutable_state_of("");
    let snapshot = Snapshot::take_snapshot(tagged(&result, 1));
    snapshot.enter(|| {
        Snapshot::observe(tagged(&result, 2), None, || {
            Snapshot::observe(tagged(&result, 3), None, || state.get());
        });
    });
    assert_eq!(*result.borrow(), [3, 2, 1]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, earlyReturnWithMutableSnapshot
#[test]
fn an_early_return_applies_a_mutable_snapshot() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    Snapshot::with_mutable_snapshot(|| {
        state.set(1);
        #[allow(clippy::needless_return)]
        return;
    });
    assert_eq!(state.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, earlyReturnGlobal
#[test]
fn an_early_return_leaves_the_global_snapshot() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| {
        Snapshot::global(|| {
            state.set(1);
            #[allow(clippy::needless_return)]
            return;
        });
        assert_eq!(snapshot, Snapshot::current());
    });
    snapshot.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, throwInWithMutableSnapshot
#[test]
fn a_panic_in_with_mutable_snapshot_applies_nothing() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let message = panic_message(|| {
        Snapshot::with_mutable_snapshot(|| {
            state.set(1);
            panic!("Test error");
        })
    });
    assert_eq!(message, "Test error");
    assert_eq!(state.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, throwInApplyWithMutableSnapshot
#[test]
fn a_conflict_in_with_mutable_snapshot_panics() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let payload = catch_unwind(AssertUnwindSafe(|| {
        Snapshot::with_mutable_snapshot(|| {
            Snapshot::global(|| state.set(1));
            state.set(2);
        })
    }))
    .expect_err("a conflict");
    assert!(payload.downcast_ref::<SnapshotApplyConflict>().is_some());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_returnsResultOfBlock
#[test]
fn read_only_returns_the_result_of_its_block() {
    let _open = open_snapshots();
    assert_eq!(Snapshot::with_read_only(|| 42), 42);
    let snapshot = mutable();
    assert_eq!(snapshot.enter(|| Snapshot::with_read_only(|| "hello")), "hello");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_globalSnapshot_prohibitsWritesDuringBlockOnly
#[test]
fn read_only_in_the_global_snapshot_prohibits_writes_during_its_block() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    state.set(1);
    assert_eq!(state.get(), 1);
    let message = panic_message(|| Snapshot::with_read_only(|| state.set(2)));
    assert_eq!(message, "Cannot modify a state object in a read-only snapshot");
    assert_eq!(state.get(), 1);
    state.set(3);
    assert_eq!(state.get(), 3);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_mutableSnapshot_prohibitsWritesDuringBlockOnly
#[test]
fn read_only_in_a_mutable_snapshot_prohibits_writes_during_its_block() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| {
        state.set(1);
        assert_eq!(state.get(), 1);
        assert!(panics(|| Snapshot::with_read_only(|| state.set(2))));
        assert_eq!(state.get(), 1);
        state.set(3);
        assert_eq!(state.get(), 3);
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_readOnlySnapshot_prohibitsWrites
#[test]
fn read_only_in_a_read_only_snapshot_prohibits_writes() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = read_only();
    snapshot.enter(|| {
        let result = Snapshot::with_read_only(|| {
            assert!(panics(|| state.set(1)));
            state.get()
        });
        assert_eq!(result, 0);
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_insideReadOnlySnapshot
#[test]
fn read_only_inside_a_read_only_snapshot() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = read_only();
    snapshot.enter(|| {
        assert!(panics(|| Snapshot::with_read_only(|| state.set(1))));
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_nestedMutableSnapshot_prohibitsWritesDuringBlockOnly
#[test]
fn read_only_in_a_nested_mutable_snapshot_prohibits_writes_during_its_block() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let parent = mutable();
    let nested = parent.take_nested_mutable_snapshot(None, None);
    nested.enter(|| {
        state.set(1);
        assert_eq!(state.get(), 1);
        assert!(panics(|| Snapshot::with_read_only(|| state.set(2))));
        assert_eq!(state.get(), 1);
        state.set(3);
        assert_eq!(state.get(), 3);
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_nestedReadOnlySnapshot_prohibitsWrites
#[test]
fn read_only_in_a_nested_read_only_snapshot_prohibits_writes() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let parent = mutable();
    let nested = parent.take_nested_snapshot(None);
    nested.enter(|| {
        assert!(panics(|| Snapshot::with_read_only(|| state.set(1))));
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_transparentObserverSnapshot_prohibitsWritesDuringBlockOnly
#[test]
fn read_only_in_an_observation_prohibits_writes_during_its_block() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    Snapshot::observe(Some(Rc::new(|_| {})), None, || {
        state.set(1);
        assert_eq!(state.get(), 1);
        assert!(panics(|| Snapshot::with_read_only(|| state.set(2))));
        assert_eq!(state.get(), 1);
        state.set(3);
        assert_eq!(state.get(), 3);
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_transparentObserverSnapshotFromReadOnly_prohibitsWrites
#[test]
fn read_only_without_read_observation_in_a_read_only_snapshot_prohibits_writes() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = read_only();
    snapshot.enter(|| {
        Snapshot::without_read_observation(|| {
            assert!(panics(|| Snapshot::with_read_only(|| state.set(1))));
        });
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_nestedReadOnlyBlocks
#[test]
fn read_only_blocks_nest() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    Snapshot::with_read_only(|| {
        assert!(panics(|| state.set(1)));
        Snapshot::with_read_only(|| assert!(panics(|| state.set(2))));
        assert!(panics(|| state.set(3)));
    });
    state.set(4);
    assert_eq!(state.get(), 4);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, readOnly_exceptionInBlockReleasesReadOnly
#[test]
fn a_panic_in_read_only_releases_it() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    assert!(panics(|| Snapshot::with_read_only(|| panic!("in the block"))));
    state.set(1);
    assert_eq!(state.get(), 1);
}

/// A policy taking the applied value at a conflict, and showing what it was given.
struct TakesApplied;

thread_local! {
    static SEEN: RefCell<Option<(i32, i32, i32)>> = const { RefCell::new(None) };
}

impl MutationPolicy<i32> for TakesApplied {
    fn equivalent(a: &i32, b: &i32) -> bool {
        a == b
    }
    fn merge(previous: &i32, current: &i32, applied: &i32) -> Option<i32> {
        SEEN.with_borrow_mut(|seen| *seen = Some((*previous, *current, *applied)));
        Some(*applied)
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, testMergePolicy
#[test]
fn a_policy_merges_conflicts() {
    let _open = open_snapshots();
    let state = mutable_state_of_with(0, TakesApplied);
    let snapshots = [mutable(), mutable(), mutable(), mutable()];
    for (index, snapshot) in snapshots.iter().enumerate() {
        snapshot.enter(|| state.set(10 * (index as i32 + 1)));
    }
    snapshots[0].apply().check();
    assert_eq!(SEEN.with_borrow(|seen| *seen), None);
    for (index, expected) in [(1, (0, 10, 20)), (2, (0, 20, 30)), (3, (0, 30, 40))] {
        snapshots[index].apply().check();
        assert_eq!(SEEN.with_borrow_mut(Option::take), Some(expected));
        assert_eq!(state.get(), expected.2);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, stateWrittenToBeforeSnapshotApplied
#[test]
fn a_state_written_before_the_snapshot_that_made_it_applied() {
    let _open = open_snapshots();
    let snapshot1 = mutable();
    let state = snapshot1.enter(|| mutable_state_of(0));
    let snapshot2 = mutable();
    let observed = Rc::new(Cell::new(false));
    let log = observed.clone();
    let id = state.state_object_id();
    let _handle = Snapshot::register_apply_observer(move |changed, _| {
        if changed.contains(id) {
            log.set(true);
        }
    });
    snapshot2.enter(|| state.set(1));
    snapshot1.apply().check();
    snapshot2.apply().check();
    Snapshot::send_apply_notifications();
    assert_eq!(state.get(), 1);
    assert!(observed.get(), "the apply observer should have heard it");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, nestedMutableSnapshotMergeWithEqualValuesDoesNotLeakOpenSnapshot
#[test]
fn a_nested_merge_of_equal_values_leaks_no_open_snapshot() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let parent = mutable();
    let child = parent.take_nested_mutable_snapshot(None, None);
    parent.enter(|| state.set(1));
    child.enter(|| state.set(1));
    child.apply().check();
    child.dispose();
    parent.enter(|| assert_eq!(state.get(), 1));
    parent.apply().check();
    parent.dispose();
    assert_eq!(state.get(), 1);
    for value in 0..5 {
        atomic(|| state.set(value + 2));
    }
    assert!(used_records(state.handle()) <= 2, "its records should have gone");
}

struct Sum;

impl MutationPolicy<i32> for Sum {
    fn equivalent(a: &i32, b: &i32) -> bool {
        a == b
    }
    fn merge(previous: &i32, current: &i32, applied: &i32) -> Option<i32> {
        Some(current + applied - previous)
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, nestedMutableSnapshotCustomPolicyMergeAppliesMergedValueToParent
#[test]
fn a_nested_merge_by_a_policy_applies_the_merged_value_to_the_parent() {
    let _open = open_snapshots();
    let state = mutable_state_of_with(1, Sum);
    let parent = mutable();
    let child = parent.take_nested_mutable_snapshot(None, None);
    parent.enter(|| state.update(|value| value + 10));
    child.enter(|| state.update(|value| value + 100));
    child.apply().check();
    child.dispose();
    parent.enter(|| assert_eq!(state.get(), 111));
    parent.apply().check();
    parent.dispose();
    assert_eq!(state.get(), 111);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, disposingParentMutableSnapshotWhileNestedSnapshotIsActiveDoesNotCorruptState
#[test]
fn disposing_a_parent_while_a_nested_snapshot_is_active_corrupts_nothing() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let parent = mutable();
    parent.enter(|| state.set(99));
    let child = parent.take_nested_mutable_snapshot(None, None);
    parent.dispose();
    assert_eq!(child.enter(|| state.get()), 99);
    child.enter(|| state.set(100));
    assert_eq!(child.enter(|| state.get()), 100);
    assert_eq!(state.get(), 0);
    child.dispose();
    assert_eq!(state.get(), 0);

    let read_only_state = mutable_state_of(0);
    let read_only_parent = mutable();
    read_only_parent.enter(|| read_only_state.set(50));
    let read_only_child = read_only_parent.take_nested_snapshot(None);
    read_only_parent.dispose();
    assert_eq!(read_only_child.enter(|| read_only_state.get()), 50);
    read_only_state.set(1);
    assert_eq!(read_only_state.get(), 1);
    assert_eq!(read_only_child.enter(|| read_only_state.get()), 50);
    read_only_child.dispose();
    assert_eq!(read_only_state.get(), 1);

    let deep_state = mutable_state_of(0);
    let root = mutable();
    root.enter(|| deep_state.set(10));
    let middle = root.take_nested_mutable_snapshot(None, None);
    middle.enter(|| deep_state.set(20));
    let leaf = middle.take_nested_mutable_snapshot(None, None);
    root.dispose();
    middle.dispose();
    assert_eq!(leaf.enter(|| deep_state.get()), 20);
    leaf.enter(|| deep_state.set(30));
    assert_eq!(leaf.enter(|| deep_state.get()), 30);
    assert_eq!(deep_state.get(), 0);
    leaf.dispose();
    assert_eq!(deep_state.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, globalWriteToNewlyCreatedObject
#[test]
fn a_global_write_to_a_state_just_made() {
    let _open = open_snapshots();
    let state = Snapshot::with_mutable_snapshot(|| {
        Snapshot::with_mutable_snapshot(|| {
            let state = mutable_state_of(1);
            Snapshot::global(|| state.set(2));
            state
        })
    });
    assert_eq!(state.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, nestedThreadWrite: the "background thread" is `Snapshot::global`, as in the test.
#[test]
fn a_global_write_from_within_nested_snapshots() {
    let _open = open_snapshots();
    fn simulate_composition(made: Option<MutableState<Option<&'static str>>>) -> Made {
        atomic(|| match made {
            Some(state) => Made::Before(state),
            None => Made::Now(mutable_state_of(None)),
        })
    }
    enum Made {
        Before(MutableState<Option<&'static str>>),
        Now(OwnedState<Option<&'static str>>),
    }
    impl Made {
        fn handle(&self) -> MutableState<Option<&'static str>> {
            match self {
                Made::Before(state) => *state,
                Made::Now(state) => state.handle(),
            }
        }
    }
    const VALUE: Option<&str> = Some("from background");
    let run =
        |nested: bool, in_snapshot: bool, made: Option<MutableState<Option<&'static str>>>| {
            let write = |state: MutableState<Option<&'static str>>| {
                Snapshot::global(|| {
                    if in_snapshot {
                        atomic(|| state.set(VALUE));
                    } else {
                        state.set(VALUE);
                    }
                })
            };
            if nested {
                atomic(|| {
                    let state = simulate_composition(made);
                    write(state.handle());
                    state
                })
            } else {
                let state = simulate_composition(made);
                write(state.handle());
                state
            }
        };
    for (nested, in_snapshot) in [(false, false), (true, false), (false, true), (true, true)] {
        assert_eq!(run(nested, in_snapshot, None).handle().get(), VALUE);
        let before = mutable_state_of(None);
        run(nested, in_snapshot, Some(before.handle()));
        assert_eq!(before.get(), VALUE);
    }
}

// ---- Beyond the ports: what compose-rs's own paths add -------------------------------------------

/// A write through a view with a `Drop` value keeps each value until nothing reads it.
#[test]
fn values_live_as_long_as_a_snapshot_can_read_them() {
    let _open = open_snapshots();
    let drops = Rc::new(Cell::new(0));
    #[derive(Clone)]
    struct Counted(Rc<Cell<i32>>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let state = mutable_state_of_with(Counted(drops.clone()), never_equal_policy());
    let snapshot = read_only();
    state.set(Counted(drops.clone()));
    assert_eq!(drops.get(), 0, "the snapshot reads the first value");
    state.set(Counted(drops.clone()));
    assert_eq!(drops.get(), 1, "the second overwritten in place: nothing could read it");
    snapshot.dispose();
    assert_eq!(drops.get(), 2, "the first, once the snapshot is gone");
    assert_eq!(used_records(state.handle()), 1);
}

/// An abandoned snapshot's values go with it, and a state is back to one record.
#[test]
fn an_abandoned_snapshot_drops_its_values() {
    let _open = open_snapshots();
    let value = Rc::new(());
    let state = mutable_state_of_with(value.clone(), never_equal_policy());
    let snapshot = mutable();
    snapshot.enter(|| state.set(value.clone()));
    assert_eq!(used_records(state.handle()), 2);
    assert_eq!(Rc::strong_count(&value), 3);
    snapshot.dispose();
    assert_eq!(Rc::strong_count(&value), 2);
    assert_eq!(used_records(state.handle()), 1);
    assert!(Rc::ptr_eq(&state.get(), &value));
}

/// `edit` copies the value when the snapshot must not see the write in place.
#[test]
fn an_edit_in_a_snapshot_edits_a_copy() {
    let _open = open_snapshots();
    let state = mutable_state_of(vec![1]);
    let snapshot = mutable();
    snapshot.enter(|| {
        state.edit(|value| value.push(2));
        assert_eq!(state.get(), [1, 2]);
        state.edit(|value| value.push(3));
        assert_eq!(used_records(state.handle()), 2, "a second edit needs no second copy");
    });
    assert_eq!(state.get(), [1]);
    snapshot.apply().check();
    assert_eq!(state.get(), [1, 2, 3]);
}

/// An `edit` that panics in a snapshot told of what it may have mutated: the apply carries it.
#[test]
fn a_panicking_edit_in_a_snapshot_counts_as_a_write() {
    let _open = open_snapshots();
    let state = mutable_state_of(vec![1]);
    let id = state.state_object_id();
    let snapshot = mutable();
    let changes = changes_of(id, || {
        snapshot.enter(|| {
            assert!(panics(|| state.edit(|value| {
                value.push(2);
                panic!("halfway");
            })));
        });
        snapshot.apply().check();
    });
    assert_eq!(changes, 1);
    assert_eq!(state.get(), [1, 2]);
}

/// Global writes with a snapshot open still reach observers and invalidate at the send; a read in
/// the global snapshot sees them.
#[test]
fn global_writes_with_a_snapshot_open_are_sent() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = read_only();
    let changes = changes_of(state.state_object_id(), || state.set(1));
    assert_eq!(changes, 1);
    assert_eq!(state.get(), 1);
    assert_eq!(snapshot.enter(|| state.get()), 0);
}

/// Taking a snapshot sends what's pending, as `takeNewSnapshot` advances the global snapshot.
#[test]
fn taking_a_snapshot_sends_the_global_changes() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    Snapshot::notify_objects_initialized();
    let heard = Rc::new(Cell::new(0));
    let count = heard.clone();
    let _observer = Snapshot::register_apply_observer(move |_, _| count.set(count.get() + 1));
    state.set(1);
    assert_eq!(heard.get(), 0);
    let snapshot = mutable();
    assert_eq!(heard.get(), 1);
    snapshot.dispose();
}

/// Composition's read registration and an observation's stop at a snapshot taken elsewhere, as
/// that snapshot's observers wouldn't include them; one taken here lets them go on.
#[test]
fn a_snapshot_taken_elsewhere_hides_reads_from_the_observation_in_progress() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let outside = read_only();
    let observed = Rc::new(Cell::new(0));
    let count = observed.clone();
    let observer = platform::ReadObserver::new(move || count.set(count.get() + 1));
    observer.observe(1, || {
        outside.enter(|| state.get());
    });
    state.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(observed.get(), 0, "read through a snapshot taken outside it");
    observer.observe(1, || {
        let inside = read_only();
        inside.enter(|| state.get());
    });
    state.set(2);
    Snapshot::send_apply_notifications();
    assert_eq!(observed.get(), 1, "read through a snapshot taken in it");
}

/// The global snapshot's writes in `Snapshot::global`, a snapshot being current, are global.
#[test]
fn snapshot_global_is_the_global_snapshot() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| {
        assert!(Snapshot::is_in_snapshot());
        Snapshot::global(|| {
            assert!(!Snapshot::is_in_snapshot());
            state.set(1);
            assert_eq!(state.get(), 1);
        });
        assert_eq!(state.get(), 0);
    });
    assert_eq!(state.get(), 1);
}

/// A panicking apply observer leaves nothing pinned once the snapshot is disposed: the records are
/// reclaimed.
#[test]
fn a_panicking_apply_observer_leaves_nothing_pinned() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = mutable();
    snapshot.enter(|| state.set(1));
    let observer = Snapshot::register_apply_observer(|_, _| panic!("in the observer"));
    assert!(panics(|| snapshot.apply().check()));
    assert_eq!(state.get(), 1, "applied before the observers");
    drop(observer);
    snapshot.dispose();
    assert_eq!(used_records(state.handle()), 1);
    assert_eq!(state.get(), 1);
    state.set(2);
    assert_eq!(state.get(), 2);
    assert!(!Snapshot::is_in_snapshot());
}

/// An `edit` whose closure opens a snapshot: the snapshot can read the state afterwards.
#[test]
fn a_snapshot_opened_in_an_edit_can_read_the_state() {
    let _open = open_snapshots();
    let state = mutable_state_of(vec![1]);
    let taken = state.edit(|value| {
        value.push(2);
        read_only()
    });
    assert_eq!(taken.enter(|| state.get()), [1, 2]);
    state.set(vec![3]);
    assert_eq!(taken.enter(|| state.get()), [1, 2]);
    assert_eq!(state.get(), [3]);
}

/// Taking and applying snapshots while a `with` closure reads the state: what can't be reclaimed
/// now is, later, and a read in the closure goes on seeing its value.
#[test]
fn a_with_closure_outlives_the_snapshot_it_applies() {
    let _open = open_snapshots();
    let state = mutable_state_of(1);
    let snapshot = mutable();
    snapshot.enter(|| state.set(2));
    state.with(|value| {
        snapshot.apply().check();
        drop(mutable());
        assert_eq!(*value, 1);
    });
    assert_eq!(state.get(), 2);
    Snapshot::notify_objects_initialized();
    state.set(3);
    Snapshot::send_apply_notifications();
    assert_eq!(state.get(), 3);
    drop(snapshot);
    assert_eq!(used_records(state.handle()), 1);
    assert!(!Snapshot::is_in_snapshot());
}

/// A policy that panics at an apply leaves the snapshot as it was: disposing it abandons what it
/// wrote, which nothing can then see.
#[test]
fn a_panicking_merge_leaves_the_snapshot_to_abandon() {
    struct Panics;
    impl MutationPolicy<i32> for Panics {
        fn equivalent(a: &i32, b: &i32) -> bool {
            a == b
        }
        fn merge(_: &i32, _: &i32, _: &i32) -> Option<i32> {
            panic!("in the merge")
        }
    }
    let _open = open_snapshots();
    let state = mutable_state_of_with(0, Panics);
    let snapshot = mutable();
    snapshot.enter(|| state.set(1));
    state.set(2);
    assert!(panics(|| snapshot.apply().check()));
    assert!(snapshot.has_pending_changes());
    snapshot.dispose();
    assert_eq!(state.get(), 2);
    assert_eq!(used_records(state.handle()), 1);
}

/// A write through a view conflicting with a `with` closure panics saying so, as without a view.
#[test]
fn an_edit_in_a_snapshot_during_a_with_closure_names_the_conflict() {
    let _open = open_snapshots();
    let state = mutable_state_of(vec![1]);
    let snapshot = mutable();
    let message = panic_message(|| {
        state.with(|_| snapshot.enter(|| state.edit(|value| value.push(2))));
    });
    assert!(message.contains("is borrowed by `with` or `edit`"), "{message}");
}

/// A snapshot's handle doesn't dispose it, its owner does; a read-only one's view survives.
#[test]
fn a_handle_doesnt_own_its_snapshot() {
    let _open = open_snapshots();
    let state = mutable_state_of(0);
    let snapshot = read_only();
    drop(snapshot.handle());
    state.set(1);
    assert_eq!(snapshot.enter(|| state.get()), 0);
    assert_eq!(snapshot.handle(), snapshot);
    assert_eq!(snapshot.root(), snapshot);
    assert!(snapshot.read_only());
    let nested = snapshot.take_nested_snapshot(None);
    assert_eq!(nested.root(), snapshot);
    let mutable_snapshot = mutable();
    assert!(!mutable_snapshot.has_pending_changes());
    mutable_snapshot.enter(|| state.set(2));
    assert!(mutable_snapshot.has_pending_changes());
}

/// A state object of an app's own: a counter whose conflicts add up, as Compose's documentation of
/// `StateObject` shows.
#[derive(Debug, PartialEq)]
struct CounterRecord(i32);

impl StateRecord for CounterRecord {
    fn copy(&self) -> Self {
        CounterRecord(self.0)
    }
    fn merge(previous: &Self, current: &Self, applied: &Self) -> MergeResult<Self> {
        MergeResult::New(CounterRecord(current.0 + applied.0 - previous.0))
    }
}

#[test]
fn an_apps_state_object_keeps_its_records_by_snapshot() {
    let _open = open_snapshots();
    let counter = snapshot_state_object(CounterRecord(1));
    let (first, second) = (mutable(), mutable());
    first.enter(|| counter.writable(|record| record.0 += 10));
    second.enter(|| counter.overwritable(|record| CounterRecord(record.0 + 100)));
    assert_eq!(counter.readable(|record| record.0), 1);
    assert_eq!(first.enter(|| counter.with_current(|record| record.0)), 11);
    assert_eq!(second.enter(|| counter.readable(|record| record.0)), 101);
    let heard = changes_of(counter.state_object_id(), || {
        first.apply().check();
        second.apply().check();
    });
    assert_eq!(heard, 2);
    assert_eq!(counter.readable(|record| record.0), 111);
}

/// `MergeResult`'s other outcomes: the current value standing tells of nothing, the applied one
/// does.
#[test]
fn a_state_objects_merge_can_keep_either_record() {
    struct Keeps<const CURRENT: bool>(i32);
    impl<const CURRENT: bool> StateRecord for Keeps<CURRENT> {
        fn copy(&self) -> Self {
            Keeps(self.0)
        }
        fn merge(_: &Self, _: &Self, _: &Self) -> MergeResult<Self> {
            if CURRENT { MergeResult::Current } else { MergeResult::Applied }
        }
    }
    let _open = open_snapshots();
    let current = snapshot_state_object(Keeps::<true>(0));
    let applied = snapshot_state_object(Keeps::<false>(0));
    let (first, second) = (mutable(), mutable());
    let heard = Rc::new(RefCell::new(Vec::new()));
    let log = heard.clone();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        log.borrow_mut().extend(changed.iter())
    });
    first.enter(|| {
        current.writable(|record| record.0 = 1);
        applied.writable(|record| record.0 = 1);
    });
    second.enter(|| {
        current.writable(|record| record.0 = 2);
        applied.writable(|record| record.0 = 2);
    });
    first.apply().check();
    heard.borrow_mut().clear();
    second.apply().check();
    assert_eq!(*heard.borrow(), [applied.state_object_id()]);
    assert_eq!(applied.readable(|record| record.0), 2);
    assert_eq!(current.readable(|record| record.0), 1);
}

fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

/// Pending once.
async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(move |_| {
        if std::mem::replace(&mut yielded, true) { Poll::Ready(()) } else { Poll::Pending }
    })
    .await;
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotContextElementTests.kt, coroutineEntersExpectedSnapshot
#[test]
fn a_future_enters_the_expected_snapshot() {
    let _open = open_snapshots();
    let snapshot = read_only();
    let future = snapshot.enter_future(async { assert_eq!(snapshot, Snapshot::current()) });
    assert!(poll_once(pin!(future)).is_ready());
    assert_ne!(snapshot, Snapshot::current());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotContextElementTests.kt, snapshotRestoredAfterResume
#[test]
fn a_snapshot_is_restored_after_a_future_resumes() {
    let _open = open_snapshots();
    let (one, two) = (read_only(), read_only());
    let a = one.enter_future(async {
        assert_eq!(one, Snapshot::current(), "expected snapshot one, A");
        yield_once().await;
        assert_eq!(one, Snapshot::current(), "expected snapshot one, B");
    });
    let b = two.enter_future(async {
        assert_eq!(two, Snapshot::current(), "expected snapshot two, A");
        yield_once().await;
        assert_eq!(two, Snapshot::current(), "expected snapshot two, B");
    });
    let (mut a, mut b) = (pin!(a), pin!(b));
    assert!(poll_once(a.as_mut()).is_pending());
    assert!(poll_once(b.as_mut()).is_pending());
    assert!(poll_once(a.as_mut()).is_ready());
    assert!(poll_once(b.as_mut()).is_ready());
}
