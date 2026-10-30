//! `SnapshotStateMap` (docs/parity/design/snapshots.md, step S3), from Compose's
//! `SnapshotStateMapTests`: the map API; Kotlin's views of the entries, the keys and the values
//! are snapshots here, written through the map's `retain` and `remove`. Adding through a view,
//! which Kotlin refuses at run time (`verifyMutableMap…Add`, `validate…Add`), doesn't exist. Those
//! taking snapshots wait for step S5; the concurrent ones are n/a (D14).

use std::collections::HashMap;
use std::rc::Rc;

use compose_runtime::snapshots::{OwnedStateMap, Snapshot, SnapshotStateMap};
use compose_runtime::*;

fn default_entries() -> Vec<(i32, f32)> {
    vec![(1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0), (5, 1.0), (6, 2.0), (7, 3.0), (8, 4.0)]
}

fn default_map() -> OwnedStateMap<i32, f32> {
    mutable_state_map_from(default_entries())
}

fn normal_map() -> HashMap<i32, f32> {
    default_entries().into_iter().collect()
}

/// The map holds what the plain map does.
#[track_caller]
fn expected(normal: &HashMap<i32, f32>, map: SnapshotStateMap<i32, f32>) {
    assert_eq!(normal.len(), map.len());
    for (key, value) in normal {
        assert_eq!(map.get(key), Some(*value));
    }
}

fn sorted<T: PartialOrd>(mut items: Vec<T>) -> Vec<T> {
    items.sort_by(|a, b| a.partial_cmp(b).expect("comparable"));
    items
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, canCreateAnMapEmpty
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, canCreateAnInitializedMap
#[test]
fn a_map_state_can_be_made() {
    assert!(mutable_state_map_of::<i32, i32>().is_empty());
    let map = mutable_state_map_from([(1, 1), (2, 2)]);
    assert_eq!(map.get(&1), Some(1));
    assert_eq!(map.get(&2), Some(2));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateSize
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateIsEmpty
#[test]
fn a_map_knows_its_size() {
    assert_eq!(default_map().len(), 8);
    assert!(!default_map().is_empty());
    assert!(mutable_state_map_of::<i32, f32>().is_empty());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateContainsKey
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validate_containsValue
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateGet
#[test]
fn a_map_finds_keys_and_values() {
    let (map, normal) = (default_map(), normal_map());
    for key in 1..=9 {
        assert_eq!(map.contains_key(&key), normal.contains_key(&key));
        assert_eq!(map.get(&key), normal.get(&key).copied());
    }
    assert!(map.contains_value(&1.0));
    assert!(!map.contains_value(&100.0));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesSize
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesIterator
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesContains
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesContainsAll
#[test]
fn a_map_lists_its_entries() {
    let (map, normal) = (default_map(), normal_map());
    let entries = map.entries();
    assert_eq!(entries.len(), normal.len());
    for (key, value) in &entries {
        assert_eq!(normal.get(key), Some(value));
    }
    assert!(entries.contains(&(1, 1.0)));
    assert!(!entries.contains(&(1, 2.0)));
    assert!(!entries.contains(&(100, 1.0)));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysSize
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysIsEmpty
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysIterator
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysContains
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysContainsAll
#[test]
fn a_map_lists_its_keys() {
    let map = default_map();
    assert_eq!(sorted(map.keys()), (1..=8).collect::<Vec<i32>>());
    assert!(map.keys().contains(&1));
    assert!(!map.keys().contains(&100));
    assert!(mutable_state_map_of::<i32, f32>().keys().is_empty());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesSize
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesIsEmpty
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesIterator
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesContains
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesContainsAll
#[test]
fn a_map_lists_its_values() {
    let map = default_map();
    assert_eq!(sorted(map.values()), [1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0]);
    assert!(map.values().contains(&1.0));
    assert!(!map.values().contains(&100.0));
    assert!(mutable_state_map_of::<i32, f32>().values().is_empty());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateClear
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesClear
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysClear
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesClear
#[test]
fn a_map_clears() {
    let map = default_map();
    map.clear();
    assert!(map.is_empty());
    assert_eq!(map.get(&1), None);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validatePut
#[test]
fn a_map_writes_a_key() {
    let (map, mut normal) = (default_map(), normal_map());
    assert_eq!(map.insert(100, 100.0), None);
    normal.insert(100, 100.0);
    expected(&normal, *map);
    assert_eq!(map.insert(1, 50.0), Some(1.0));
    normal.insert(1, 50.0);
    expected(&normal, *map);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validatePutAll
#[test]
fn a_map_writes_several_keys() {
    let (map, mut normal) = (default_map(), normal_map());
    map.insert_all([(100, 100.0), (1, 50.0)]);
    normal.extend([(100, 100.0), (1, 50.0)]);
    expected(&normal, *map);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateRemove
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysRemove
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesRemove
#[test]
fn a_map_removes_a_key() {
    let (map, mut normal) = (default_map(), normal_map());
    assert_eq!(map.remove(&1), Some(1.0));
    normal.remove(&1);
    expected(&normal, *map);
    assert_eq!(map.remove(&100), None);
    expected(&normal, *map);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesRemoveAll
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesIteratorRemove
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysRemoveAll
#[test]
fn a_map_removes_several_by_key() {
    let (map, mut normal) = (default_map(), normal_map());
    assert!(map.retain(|key, _| key % 2 != 0));
    normal.retain(|key, _| key % 2 != 0);
    expected(&normal, *map);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateEntriesRetainAll
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateKeysRetainAll
#[test]
fn a_map_retains_several_by_key() {
    let (map, mut normal) = (default_map(), normal_map());
    assert!(map.retain(|key, _| key % 2 == 0));
    normal.retain(|key, _| key % 2 == 0);
    expected(&normal, *map);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesRemove
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesRemoveAll
// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, validateValuesRetainAll
#[test]
fn a_map_retains_by_value() {
    let (map, mut normal) = (default_map(), normal_map());
    assert!(map.retain(|_, value| *value > 2.0));
    normal.retain(|_, value| *value > 2.0);
    expected(&normal, *map);
    assert!(map.retain(|_, value| *value != 3.0));
    normal.retain(|_, value| *value != 3.0);
    expected(&normal, *map);
}

/// `update` reads and writes one key, a `None` removing it.
#[test]
fn a_map_updates_a_key() {
    let map = mutable_state_map_from([(1, 10)]);
    map.update(1, |value| value.map(|value| value + 1));
    assert_eq!(map.get(&1), Some(11));
    map.update(2, |value| Some(value.copied().unwrap_or(0) + 1));
    assert_eq!(map.get(&2), Some(1));
    map.update(1, |_| None);
    assert_eq!(map.get(&1), None);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, toStringOfSnapshotStateMapDoesNotTriggerReadObserver
#[test]
fn formatting_a_map_reads_nothing() {
    let map = mutable_state_map_from([(0, 0)]);
    let reads = Rc::new(std::cell::Cell::new(0));
    let observer: compose_runtime::snapshots::StateObserverFn = {
        let reads = reads.clone();
        Rc::new(move |_| reads.set(reads.get() + 1))
    };
    Snapshot::observe(Some(observer.clone()), None, || map.len());
    assert_eq!(reads.get(), 1);
    reads.set(0);
    Snapshot::observe(Some(observer), None, || drop(format!("{}", *map)));
    assert_eq!(reads.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateMapTests.kt, testValueOfStateMapToString
#[test]
fn a_map_displays_its_entries() {
    let map = mutable_state_map_from([(0, 0)]);
    assert!(format!("{}", *map).starts_with("SnapshotStateMap(value={0=0})@"));
}

/// What leaves a map as it was writes nothing; `clear` always writes.
#[test]
fn an_operation_leaving_the_map_as_it_was_writes_nothing() {
    let map = mutable_state_map_from([(0, 0)]);
    Snapshot::notify_objects_initialized();
    let changes = Rc::new(std::cell::Cell::new(0));
    let counted = changes.clone();
    let _observer =
        Snapshot::register_apply_observer(move |set, _| counted.set(counted.get() + set.len()));
    assert_eq!(map.remove(&100), None);
    assert!(!map.retain(|_, _| true));
    map.insert_all([]);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 0);
    map.clear();
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}
