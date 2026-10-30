//! `SnapshotStateSet` (docs/parity/design/snapshots.md, step S3), from Compose's
//! `SnapshotStateSetTests`. The one taking a snapshot waits for step S5; the concurrent ones are
//! n/a (D14).

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::snapshots::{Snapshot, SnapshotStateSet};
use compose_runtime::*;

fn items<T: Clone + std::hash::Hash + Eq>(set: SnapshotStateSet<T>) -> Vec<T> {
    set.iter_cloned().collect()
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, canCreateAStateSet
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, canCreateAStateSetOfInts
#[test]
fn a_set_state_can_be_made() {
    assert!(mutable_state_set_of::<i32>().is_empty());
    let set = mutable_state_set_from([0, 1, 2, 3, 4]);
    for (index, item) in set.iter_cloned().enumerate() {
        assert_eq!(index, item);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateSize
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateIsEmpty
#[test]
fn a_set_knows_its_size() {
    assert_eq!(mutable_state_set_from([0, 1, 2, 3]).len(), 4);
    assert!(!mutable_state_set_from([0, 1, 2]).is_empty());
    assert!(mutable_state_set_of::<i32>().is_empty());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateContains
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateContainsAll
#[test]
fn a_set_knows_what_it_contains() {
    let set = mutable_state_set_from([0, 1, 2, 3, 4]);
    for item in 0..5 {
        assert!(set.contains(&item));
    }
    assert!(!set.contains(&5));
    assert!(set.contains_all(&[1, 2, 3]));
    assert!(!set.contains_all(&[0, 2, 3, 5]));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateIterator
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateIterator_ordered
#[test]
fn a_set_iterates_in_insertion_order() {
    let set = mutable_state_set_from([3, 1, 4, 5, 9, 2]);
    assert_eq!(items(*set), [3, 1, 4, 5, 9, 2]);
    set.insert(0);
    set.insert(3);
    assert_eq!(items(*set), [3, 1, 4, 5, 9, 2, 0]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateIterator_remove
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, validateIterator_orderedAfterRemove
#[test]
fn a_set_keeps_its_order_after_a_removal() {
    let set = mutable_state_set_from([0, 1, 2, 3, 4]);
    assert!(set.remove(&2));
    assert_eq!(items(*set), [0, 1, 3, 4]);
}

/// Past half tombstones the entries are compacted, and the order and the lookups hold.
#[test]
fn a_set_with_many_removals_keeps_its_order_and_lookups() {
    let set = mutable_state_set_from(0..100);
    for item in (0..100).filter(|item| item % 10 != 0) {
        assert!(set.remove(&item));
    }
    assert_eq!(items(*set), (0..100).step_by(10).collect::<Vec<i32>>());
    assert!(set.contains(&50));
    assert!(!set.contains(&51));
    assert!(set.remove(&50));
    assert!(set.insert(51));
    assert_eq!(items(*set).last(), Some(&51));
    assert!(!set.contains(&50));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, canRemoveFromAStateSet
#[test]
fn a_set_removes_a_value() {
    let set = mutable_state_set_from([0, 1, 2]);
    assert!(set.remove(&1));
    assert_eq!(set.len(), 2);
    assert!(set.contains(&0));
    assert!(set.contains(&2));
    assert!(!set.remove(&1));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, canRemoveAllFromAStateSet
#[test]
fn a_set_removes_several_values() {
    let set = mutable_state_set_from([0, 1, 2, 3, 4, 5]);
    assert!(set.remove_all(&[2, 4]));
    assert_eq!(items(*set), [0, 1, 3, 5]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, canRetainAllOfAStateSet
#[test]
fn a_set_retains_some_values() {
    let set = mutable_state_set_from([0, 1, 2, 3, 4, 5, 6]);
    assert!(set.retain_all(&[2, 4, 6, 8]));
    assert_eq!(items(*set), [2, 4, 6]);
}

fn changes_of(block: impl FnOnce()) -> Vec<snapshots::StateObjectId> {
    let changes: Rc<std::cell::RefCell<Vec<snapshots::StateObjectId>>> = Rc::default();
    let log = changes.clone();
    let _observer =
        Snapshot::register_apply_observer(move |set, _| log.borrow_mut().extend(set.iter()));
    block();
    Snapshot::send_apply_notifications();
    changes.take()
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, testWritingANewValueDoesObserveChange
#[test]
fn writing_a_new_value_changes_the_set() {
    let set = mutable_state_set_from([0, 1, 2]);
    Snapshot::notify_objects_initialized();
    let changes = changes_of(|| {
        for item in 0..4 {
            set.insert(item);
        }
    });
    assert_eq!(changes, [set.state_object_id()]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, testWritingTheSameValueDoesNotChangeTheSet
#[test]
fn writing_the_same_value_doesnt_change_the_set() {
    let set = mutable_state_set_from([0, 1, 2, 3]);
    Snapshot::notify_objects_initialized();
    let changes = changes_of(|| {
        for item in 0..4 {
            set.insert(item);
        }
    });
    assert!(changes.is_empty());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, toStringOfSnapshotStateSetDoesNotTriggerReadObserver
#[test]
fn formatting_a_set_reads_nothing() {
    let set = mutable_state_set_from([0]);
    let reads = Rc::new(Cell::new(0));
    let observer: snapshots::StateObserverFn = {
        let reads = reads.clone();
        Rc::new(move |_| reads.set(reads.get() + 1))
    };
    Snapshot::observe(Some(observer.clone()), None, || set.len());
    assert_eq!(reads.get(), 1);
    reads.set(0);
    Snapshot::observe(Some(observer), None, || drop(format!("{}", *set)));
    assert_eq!(reads.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateSetTests.kt, testValueOfStateSetToString
#[test]
fn a_set_displays_its_elements() {
    let set = mutable_state_set_from([0, 1, 2]);
    assert!(format!("{}", *set).starts_with("SnapshotStateSet(value=[0, 1, 2])@"));
}
