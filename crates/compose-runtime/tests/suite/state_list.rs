//! `SnapshotStateList` (docs/parity/design/snapshots.md, step S3), from Compose's
//! `SnapshotStateListTests`: the list API, sublists and iterators. Those taking snapshots wait for
//! step S5; the concurrent ones are n/a (D14).

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::snapshots::{Snapshot, SnapshotStateList, snapshot_state_list};
use compose_runtime::*;

fn items<T: Clone>(list: SnapshotStateList<T>) -> Vec<T> {
    list.iter_cloned().collect()
}

/// The states changed while `block` ran and was sent.
fn changes_of(block: impl FnOnce()) -> usize {
    let changes = Rc::new(Cell::new(0));
    let counted = changes.clone();
    let _observer =
        Snapshot::register_apply_observer(move |set, _| counted.set(counted.get() + set.len()));
    block();
    Snapshot::send_apply_notifications();
    changes.get()
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canCreateAStateList
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canCreateAStateListOfInts
#[test]
fn a_list_state_can_be_made() {
    let empty = mutable_state_list_of::<Rc<str>>();
    assert!(empty.is_empty());
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    for (index, item) in list.iter_cloned().enumerate() {
        assert_eq!(index, item);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateSize
#[test]
fn a_list_knows_its_size() {
    assert_eq!(mutable_state_list_from([0, 1, 2, 3]).len(), 4);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateContains
#[test]
fn a_list_knows_what_it_contains() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    for item in 0..5 {
        assert!(list.contains(&item));
    }
    assert!(!list.contains(&5));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateContainsAll
#[test]
fn a_list_contains_all_of_some_values() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    assert!(list.contains_all(&[1, 2, 3]));
    assert!(!list.contains_all(&[0, 2, 3, 5]));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateGet
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateGet_IndexOutOfBound
#[test]
fn a_list_gives_an_element_by_index_or_none() {
    let list = mutable_state_list_from([0, 1, 2, 3]);
    for index in 0..4 {
        assert_eq!(list.get(index), Some(index));
    }
    assert_eq!(list.get(4), None);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateIsEmpty
#[test]
fn a_list_knows_if_its_empty() {
    assert!(!mutable_state_list_from([0, 1, 2]).is_empty());
    assert!(mutable_state_list_of::<i32>().is_empty());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateIterator
#[test]
fn a_list_iterates_in_order() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let mut expected = 0;
    for item in list.iter_cloned() {
        assert_eq!(item, expected);
        expected += 1;
    }
    assert_eq!(expected, 5);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateIterator_remove
#[test]
#[should_panic(expected = "remove before next or previous")]
fn an_iterator_removes_once_per_step() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let mut iterator = list.list_iterator(0);
    iterator.next();
    iterator.next();
    iterator.remove();
    iterator.remove();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateIterator_previous_remove
#[test]
fn an_iterator_removes_what_previous_gave() {
    let list = mutable_state_list_from([100]);
    let mut iterator = list.list_iterator(1);
    assert_eq!(iterator.previous(), Some(100));
    iterator.remove();
    assert!(list.is_empty());
    assert_eq!(iterator.next_index(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateLastIndexOf
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_indexOf
#[test]
fn a_list_finds_the_first_and_last_index_of_a_value() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 0, 1, 2, 3, 4]);
    for item in 0..5 {
        assert_eq!(list.index_of(&item), Some(item));
        assert_eq!(list.last_index_of(&item), Some(item + 5));
    }
    assert_eq!(list.index_of(&5), None);
    assert_eq!(list.last_index_of(&5), None);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateListIterator
#[test]
fn a_cursor_moves_both_ways() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let mut iterator = list.list_iterator(0);
    for index in 0..list.len() {
        assert!(iterator.has_next());
        assert_eq!(iterator.next_index(), index);
        assert_eq!(iterator.next(), Some(index));
    }
    assert!(!iterator.has_next());
    assert_eq!(iterator.next(), None);
    for index in (0..list.len()).rev() {
        assert!(iterator.has_previous());
        assert_eq!(iterator.previous_index(), Some(index));
        assert_eq!(iterator.previous(), Some(index));
    }
    assert!(!iterator.has_previous());
    assert_eq!(iterator.previous(), None);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateListIterator_add
#[test]
fn a_cursor_adds_where_it_is() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let mut iterator = list.list_iterator(0);
    iterator.next();
    iterator.next();
    iterator.add(100);
    iterator.add(101);
    iterator.next();
    iterator.add(102);
    iterator.previous();
    iterator.previous();
    iterator.add(103);
    // As a `java.util.ListIterator` does it.
    assert_eq!(items(*list), [0, 1, 100, 101, 103, 2, 102, 3, 4, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateListIterator_mutationError_set
#[test]
fn a_cursor_sets_what_it_last_gave() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let mut iterator = list.list_iterator(0);
    iterator.next();
    iterator.set(100);
    iterator.next();
    iterator.next();
    iterator.set(101);
    assert_eq!(items(*list), [100, 1, 101, 3, 4]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validateListIterator_index
#[test]
fn a_cursor_starts_at_an_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5]);
    let mut iterator = list.list_iterator(3);
    for index in 0..3 {
        assert!(iterator.has_next());
        assert_eq!(iterator.next(), Some(index + 3));
    }
    assert!(!iterator.has_next());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_size
#[test]
fn a_sublist_shows_its_range() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(list.sub_list(2..5).to_vec(), [2, 3, 4]);
    assert_eq!(list.sub_list(0..7).len(), 7);
    assert!(list.sub_list(3..3).is_empty());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_contains
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_containsAll
#[test]
fn a_sublist_contains_only_its_range() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    for (item, inside) in
        [(0, false), (1, false), (2, true), (3, true), (4, true), (5, false), (6, false)]
    {
        assert_eq!(sub_list.contains(&item), inside);
    }
    assert!(sub_list.contains_all(&[2, 3, 4]));
    assert!(!sub_list.contains_all(&[2, 3, 4, 5]));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_get
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_get_outOfRange
#[test]
fn a_sublist_gives_an_element_by_its_own_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    for index in 0..3 {
        assert_eq!(sub_list.get(index), Some(index + 2));
    }
    assert_eq!(sub_list.get(3), None);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_indexOf
#[test]
fn a_sublist_finds_by_its_own_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    for index in 0..sub_list.len() {
        assert_eq!(sub_list.index_of(&(index + 2)), Some(index));
    }
    assert_eq!(sub_list.index_of(&0), None);
    assert_eq!(sub_list.index_of(&5), None);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_add
#[test]
fn a_sublist_adds_at_its_end() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    sub_list.push(100);
    assert_eq!(sub_list.to_vec(), [2, 3, 4, 100]);
    assert_eq!(items(*list), [0, 1, 2, 3, 4, 100, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_add_index
#[test]
fn a_sublist_adds_at_its_own_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    sub_list.insert(1, 100);
    assert_eq!(sub_list.to_vec(), [2, 100, 3, 4]);
    assert_eq!(items(*list), [0, 1, 2, 100, 3, 4, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_addAll
#[test]
fn a_sublist_adds_several_at_its_end() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    assert!(sub_list.add_all([100, 101, 102]));
    assert_eq!(sub_list.to_vec(), [2, 3, 4, 100, 101, 102]);
    assert_eq!(items(*list), [0, 1, 2, 3, 4, 100, 101, 102, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_addAll_index
#[test]
fn a_sublist_adds_several_at_its_own_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    assert!(sub_list.insert_all(1, [100, 101, 102]));
    assert_eq!(sub_list.to_vec(), [2, 100, 101, 102, 3, 4]);
    assert_eq!(items(*list), [0, 1, 2, 100, 101, 102, 3, 4, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_clear
#[test]
fn a_sublist_clears_only_its_range() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    sub_list.clear();
    assert!(sub_list.is_empty());
    assert_eq!(items(*list), [0, 1, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_listIterator
#[test]
fn a_sublists_cursor_stays_in_its_range() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let sub_list = list.sub_list(1..4);
    let mut iterator = sub_list.list_iterator(0);
    for index in 0..sub_list.len() {
        assert!(iterator.has_next());
        assert_eq!(sub_list.get(iterator.next_index()), Some(index + 1));
        assert_eq!(iterator.next(), Some(index + 1));
    }
    assert!(!iterator.has_next());
    for index in (0..sub_list.len()).rev() {
        assert!(iterator.has_previous());
        assert_eq!(iterator.previous(), Some(index + 1));
    }
    assert!(!iterator.has_previous());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_listIterator_add
#[test]
#[should_panic(expected = "Cannot modify a state list through an iterator")]
fn a_sublists_cursor_cant_add() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let mut iterator = list.sub_list(1..4).list_iterator(0);
    iterator.next();
    iterator.add(1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_listIterator_remove
#[test]
#[should_panic(expected = "Cannot modify a state list through an iterator")]
fn a_sublists_cursor_cant_remove() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let mut iterator = list.sub_list(1..4).list_iterator(0);
    iterator.next();
    iterator.remove();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_listIterator_set
#[test]
#[should_panic(expected = "Cannot modify a state list through an iterator")]
fn a_sublists_cursor_cant_set() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4]);
    let mut iterator = list.sub_list(1..4).list_iterator(0);
    iterator.next();
    iterator.set(100);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_remove
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_removeAt
#[test]
fn a_sublist_removes_by_value_and_by_its_own_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    assert!(!sub_list.remove_item(&0), "outside its range");
    assert!(sub_list.remove_item(&3));
    assert_eq!(sub_list.remove(0), 2);
    assert_eq!(sub_list.to_vec(), [4]);
    assert_eq!(items(*list), [0, 1, 4, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_removeAll
#[test]
fn a_sublist_removes_several_only_in_its_range() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    assert!(sub_list.remove_all(&[0, 2, 4, 6]));
    assert_eq!(sub_list.to_vec(), [3]);
    assert_eq!(items(*list), [0, 1, 3, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_retainAll
#[test]
fn a_sublist_retains_only_in_its_range() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    assert!(sub_list.retain_all(&[0, 2, 4, 6]));
    assert_eq!(sub_list.to_vec(), [2, 4]);
    assert_eq!(items(*list), [0, 1, 2, 4, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_set
#[test]
fn a_sublist_sets_by_its_own_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..5);
    assert_eq!(sub_list.set(1, 100), 3);
    assert_eq!(sub_list.to_vec(), [2, 100, 4]);
    assert_eq!(items(*list), [0, 1, 2, 100, 4, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_subList
#[test]
fn a_sublist_has_sublists() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let sub_list = list.sub_list(2..6);
    let inner = sub_list.sub_list(1..3);
    assert_eq!(inner.to_vec(), [3, 4]);
    assert_eq!(inner.get(0), Some(3));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, validate_subList_concurrentChanges
#[test]
#[should_panic(expected = "ConcurrentModification")]
fn a_sublist_used_after_another_changed_the_list_panics() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5, 6]);
    let (first, second) = (list.sub_list(1..6), list.sub_list(1..6));
    assert!(first.remove_item(&3));
    second.remove_item(&4);
}

/// A write that keeps the shape keeps a sublist valid.
#[test]
fn a_sublist_outlives_a_set_of_the_list() {
    let list = mutable_state_list_from([0, 1, 2]);
    let sub_list = list.sub_list(0..2);
    list.set(0, 10);
    assert_eq!(sub_list.get(0), Some(10));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canModifyAStateList_add
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canModifyAStateList_addIndex
#[test]
fn a_list_adds_at_the_end_and_at_an_index() {
    let list = mutable_state_list_from([0, 1, 2]);
    list.push(3);
    assert_eq!(items(*list), [0, 1, 2, 3]);
    list.insert(1, 100);
    assert_eq!(items(*list), [0, 100, 1, 2, 3]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canRemoveFromAStateList
#[test]
fn a_list_removes_a_value() {
    let list = mutable_state_list_from([0, 1, 2]);
    assert!(list.remove_item(&1));
    assert_eq!(items(*list), [0, 2]);
    assert!(!list.remove_item(&1));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canRemoveAllFromAStateList
#[test]
fn a_list_removes_several_values() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5]);
    assert!(list.remove_all(&[2, 4]));
    assert_eq!(items(*list), [0, 1, 3, 5]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canRemoveAtFromAStateList
#[test]
fn a_list_removes_at_an_index() {
    let list = mutable_state_list_from([0, 1, 2, 3, 4, 5]);
    assert_eq!(list.remove(2), 2);
    assert_eq!(items(*list), [0, 1, 3, 4, 5]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canRetainAllOfAStateList
#[test]
fn a_list_retains_some_values() {
    let list = snapshot_state_list(7, |index| index);
    assert!(list.retain_all(&[2, 4, 6, 8]));
    assert_eq!(items(*list), [2, 4, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canSetAnElementOfAStateList
#[test]
fn a_list_sets_an_element() {
    let list = snapshot_state_list(7, |index| index);
    assert_eq!(list.set(2, 100), 2);
    assert_eq!(items(*list), [0, 1, 100, 3, 4, 5, 6]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canCreateViaLambdaExtension
#[test]
fn a_list_is_made_from_a_size_and_a_function() {
    let list = snapshot_state_list(5, |index| index);
    assert_eq!(items(*list), items(*mutable_state_list_from([0, 1, 2, 3, 4])));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canReverseTheList
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canReverseUsingIterators
#[test]
fn a_list_reverses_in_place_and_through_two_cursors() {
    let expected: Vec<usize> = (0..100).rev().collect();
    let list = snapshot_state_list(100, |index| index);
    list.reverse();
    assert_eq!(items(*list), expected);
    for size in [100, 101] {
        let expected: Vec<usize> = (0..size).rev().collect();
        let list = snapshot_state_list(size, |index| index);
        let mut forward = list.list_iterator(0);
        let mut backward = list.list_iterator(list.len());
        for _ in 0..list.len() / 2 {
            let forward_value = forward.next().expect("a value");
            let backward_value = backward.previous().expect("a value");
            backward.set(forward_value);
            forward.set(backward_value);
        }
        assert_eq!(items(*list), expected);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canIterateForwards
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canIterateBackwards
#[test]
fn a_cursor_goes_through_the_whole_list_each_way() {
    let list = snapshot_state_list(100, |index| index);
    let mut forward = list.list_iterator(0);
    let (mut expected, mut count) = (0, 0);
    while forward.has_next() {
        count += 1;
        assert_eq!(forward.next(), Some(expected));
        expected += 1;
    }
    assert_eq!(count, 100);
    let mut backward = list.list_iterator(list.len());
    let (mut expected, mut count) = (100, 0);
    while backward.has_previous() {
        count += 1;
        expected -= 1;
        assert_eq!(backward.previous(), Some(expected));
    }
    assert_eq!(count, 100);
}

/// An unpredictable order, as Kotlin's `shuffle`.
fn shuffle(list: SnapshotStateList<usize>) {
    let mut seed: u64 = 7;
    for index in (1..list.len()).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        list.swap(index, (seed >> 33) as usize % (index + 1));
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canShuffleTheList
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, canSortTheList
#[test]
fn a_list_shuffles_and_sorts_in_place() {
    let list = snapshot_state_list(100, |index| index);
    shuffle(*list);
    assert_ne!(items(*list), (0..100).collect::<Vec<usize>>());
    let mut distinct = items(*list);
    distinct.dedup();
    assert_eq!(distinct.len(), 100);
    list.sort_by(Ord::cmp);
    assert_eq!(items(*list), (0..100).collect::<Vec<usize>>());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, toStringOfSnapshotStateListDoesNotTriggerReadObserver
#[test]
fn formatting_a_list_reads_nothing() {
    let list = mutable_state_list_from([0]);
    let reads = Rc::new(Cell::new(0));
    let observer: compose_runtime::snapshots::StateObserverFn = {
        let reads = reads.clone();
        Rc::new(move |_| reads.set(reads.get() + 1))
    };
    Snapshot::observe(Some(observer.clone()), None, || list.len());
    assert_eq!(reads.get(), 1);
    reads.set(0);
    Snapshot::observe(Some(observer), None, || drop(format!("{}", *list)));
    assert_eq!(reads.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, testValueOfStateListToString
#[test]
fn a_list_displays_its_elements() {
    let list = mutable_state_list_from([0, 1, 2]);
    assert!(format!("{}", *list).starts_with("SnapshotStateList(value=[0, 1, 2])@"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, testWritingTheSameValueDoesNotChangeTheList
#[test]
fn writing_the_same_value_doesnt_change_the_list() {
    let list = mutable_state_list_from([0, 1, 2, 3]);
    Snapshot::notify_objects_initialized();
    assert_eq!(
        changes_of(|| {
            for index in 0..4 {
                list.set(index, index);
            }
        }),
        0
    );
    assert_eq!(
        changes_of(|| {
            list.set(0, 10);
        }),
        1
    );
}

/// What leaves a list as it was writes nothing; `clear` always writes, as Compose's `clearImpl`.
#[test]
fn an_operation_leaving_the_list_as_it_was_writes_nothing() {
    let list = mutable_state_list_from([0, 1, 2, 3]);
    Snapshot::notify_objects_initialized();
    let unchanged = changes_of(|| {
        assert!(!list.retain(|_| true));
        assert!(!list.remove_item(&100));
        assert!(!list.remove_all(&[100]));
        assert!(!list.retain_all(&[0, 1, 2, 3]));
        list.remove_range(1..1);
        list.truncate(4);
        assert!(!list.add_all([]));
    });
    assert_eq!(unchanged, 0);
    let empty = mutable_state_list_of::<i32>();
    Snapshot::notify_objects_initialized();
    assert_eq!(changes_of(|| empty.clear()), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateListTests.kt, stateList_forEach_singleRead
// On `Snapshot::observe`: taking a snapshot with its read observer comes with step S5.
#[test]
fn iterating_a_list_is_one_read() {
    let list = mutable_state_list_from([0, 1, 2, 3]);
    let reads = Rc::new(Cell::new(0));
    let counted = reads.clone();
    let mut count = 0;
    Snapshot::observe(Some(Rc::new(move |_| counted.set(counted.get() + 1))), None, || {
        for _ in list.iter_cloned() {
            count += 1;
        }
    });
    assert_eq!(count, 4);
    assert_eq!(reads.get(), 1);
}

/// A write copies only while something shares the list: `to_list` hands out the list itself, and
/// what it handed out stays as it was.
#[test]
fn to_list_shares_the_list_until_a_write_copies_it() {
    let list = mutable_state_list_from([0, 1, 2]);
    let (first, second) = (list.to_list(), list.to_list());
    assert!(first.ptr_eq(&second));
    list.push(3);
    assert_eq!(first.as_slice(), [0, 1, 2]);
    assert_eq!(list.to_list().as_slice(), [0, 1, 2, 3]);
    assert!(!first.ptr_eq(&list.to_list()));
}

/// A composable reading a list recomposes when it changes, and not otherwise.
#[test]
fn a_composable_reading_a_list_recomposes_when_it_changes() {
    use compose_runtime::platform::NodeTree;
    let list = mutable_state_list_from([1, 2]);
    let handle = list.handle();
    let sums: Rc<std::cell::RefCell<Vec<i32>>> = Rc::default();
    let log = sums.clone();
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || log.borrow_mut().push(handle.with(|list| list.iter().sum())));
    assert_eq!(*sums.borrow(), [3]);
    list.remove_item(&100);
    assert!(!composition.has_invalidations());
    list.push(3);
    assert!(composition.recompose());
    assert_eq!(*sums.borrow(), [3, 6]);
}
