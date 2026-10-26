//! Compose's `SaveableStateRegistryTest` and `AutoSaverTest`: registering, unregistering,
//! saving and restoring values. Keys are `u64` composite hashes (no empty or blank keys to
//! reject), and values are `SavedValue`s, saveable by type (no `canBeSaved` callback).

use compose_runtime::saveable::*;

fn int(value: i64) -> SavedValue {
    SavedValue::Int(value)
}

fn text(value: &str) -> SavedValue {
    SavedValue::Text(value.to_string())
}

const KEY: u64 = 7;

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/AutoSaverTest.kt, simpleSave
#[test]
fn the_auto_saver_saves_the_value_as_is() {
    assert_eq!(Saver::<i32>::save(&AutoSaver, &2), Some(int(2)));
    assert_eq!(Saver::<i32>::restore(&AutoSaver, &int(2)), Some(2));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, saveSimpleValue
#[test]
fn a_simple_value_is_saved() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(KEY, || Some(int(10)));
    assert_eq!(registry.perform_save().values.get(&KEY), Some(&vec![int(10)]));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, unregisteredValuesAreNotSaved
#[test]
fn unregistered_values_are_not_saved() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(KEY, || Some(int(10))).unregister();
    assert!(!registry.perform_save().values.contains_key(&KEY));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, registerAgainAfterUnregister
#[test]
fn a_key_registers_again_after_unregistering() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(KEY, || Some(text("value1"))).unregister();
    registry.register_provider(KEY, || Some(text("value2")));
    assert_eq!(registry.perform_save().values.get(&KEY), Some(&vec![text("value2")]));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, registerMultipleValues
#[test]
fn several_values_are_saved() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(1, || Some(int(100)));
    let entry = registry.register_provider(2, || Some(int(100)));
    registry.register_provider(3, || Some(text("value")));
    registry.register_provider(4, || Some(SavedValue::List(vec![text("item")])));
    entry.unregister();
    let saved = registry.perform_save().values;
    assert_eq!(saved.get(&1), Some(&vec![int(100)]));
    assert!(!saved.contains_key(&2));
    assert_eq!(saved.get(&3), Some(&vec![text("value")]));
    assert_eq!(saved.get(&4), Some(&vec![SavedValue::List(vec![text("item")])]));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, nullValuesAreNotSaved
#[test]
fn nothing_saved_leaves_the_key_out() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(KEY, || None);
    assert!(!registry.perform_save().values.contains_key(&KEY));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, restoreSimpleValues
#[test]
fn restored_values_are_given_back() {
    let restored = SavedState {
        values: [(1, vec![text("value")]), (2, vec![SavedValue::Float(2.0)])].into_iter().collect(),
    };
    let registry = SaveableStateRegistry::new(Some(restored));
    assert_eq!(registry.consume_restored(1), Some(text("value")));
    assert_eq!(registry.consume_restored(2), Some(SavedValue::Float(2.0)));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, restoreClearsTheStoredValue
#[test]
fn a_restored_value_is_given_back_once() {
    let restored = SavedState { values: [(KEY, vec![text("value")])].into_iter().collect() };
    let registry = SaveableStateRegistry::new(Some(restored));
    assert_eq!(registry.consume_restored(KEY), Some(text("value")));
    assert_eq!(registry.consume_restored(KEY), None);
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, unusedRestoredValueSavedAgain
#[test]
fn an_unused_restored_value_is_saved_again() {
    let restored = SavedState { values: [(1, vec![text("value")])].into_iter().collect() };
    let registry = SaveableStateRegistry::new(Some(restored));
    registry.register_provider(2, || Some(int(1)));
    let saved = registry.perform_save().values;
    assert_eq!(saved.get(&1), Some(&vec![text("value")]));
    assert_eq!(saved.get(&2), Some(&vec![int(1)]));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, registeringTheSameKeysTwice
#[test]
fn values_registered_under_one_key_are_restored_in_order() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(KEY, || Some(int(100)));
    registry.register_provider(KEY, || Some(int(200)));
    let restored = SaveableStateRegistry::new(Some(registry.perform_save()));
    assert_eq!(restored.consume_restored(KEY), Some(int(100)));
    assert_eq!(restored.consume_restored(KEY), Some(int(200)));
    assert_eq!(restored.consume_restored(KEY), None);
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/SaveableStateRegistryTest.kt, registeringAndUnregisteringTheSameKeys
#[test]
fn unregistering_one_of_a_keys_values() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(KEY, || Some(int(1)));
    let entry = registry.register_provider(KEY, || Some(int(2)));
    registry.register_provider(KEY, || Some(int(3)));
    entry.unregister();
    let restored = SaveableStateRegistry::new(Some(registry.perform_save()));
    assert_eq!(restored.consume_restored(KEY), Some(int(1)));
    assert_eq!(restored.consume_restored(KEY), Some(int(3)));
}
