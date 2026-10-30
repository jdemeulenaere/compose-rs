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

// ---- list_saver and map_saver -----------------------------------------------------------------

#[derive(PartialEq, Debug)]
struct Size {
    x: i32,
    y: i32,
}

#[derive(PartialEq, Debug)]
struct NullableSize {
    x: Option<i32>,
    y: Option<i32>,
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/ListSaverTest.kt, simpleSaveAndRestore
#[test]
fn a_list_saver_saves_and_restores() {
    let saver = list_saver(
        |size: &Size| vec![size.x, size.y],
        |items: Vec<i32>| Some(Size { x: items[0], y: items[1] }),
    );
    let saved = saver.save(&Size { x: 2, y: 3 }).expect("saved");
    assert_eq!(saver.restore(&saved), Some(Size { x: 2, y: 3 }));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/ListSaverTest.kt, customListIsTransformedToArrayList
#[test]
fn a_list_saver_saves_a_state_list_as_a_plain_list() {
    let saver = list_saver(
        |list: &compose_runtime::snapshots::SnapshotStateList<String>| {
            list.iter_cloned().collect::<Vec<String>>()
        },
        |_: Vec<String>| None,
    );
    let list = compose_runtime::mutable_state_list_from([String::from("One"), String::from("Two")]);
    assert_eq!(
        saver.save(&list.handle()),
        Some(SavedValue::List(vec![
            SavedValue::Text(String::from("One")),
            SavedValue::Text(String::from("Two")),
        ]))
    );
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/ListSaverTest.kt, nullableListItemsAreSupported
// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/ListSaverTest.kt, nullableTypeIsSupported
#[test]
fn a_list_saver_saves_none_items() {
    let saver = list_saver(
        |size: &Option<NullableSize>| {
            vec![size.as_ref().and_then(|size| size.x), size.as_ref().and_then(|size| size.y)]
        },
        |items: Vec<Option<i32>>| Some(Some(NullableSize { x: items[0], y: items[1] })),
    );
    let original = Some(NullableSize { x: None, y: Some(3) });
    let saved = saver.save(&original).expect("saved");
    assert_eq!(saver.restore(&saved), Some(original));
    let saved = saver.save(&None).expect("saved");
    assert_eq!(saver.restore(&saved), Some(Some(NullableSize { x: None, y: None })));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/MapSaverTest.kt, simpleSaveAndRestore
#[test]
fn a_map_saver_saves_and_restores() {
    let saver = map_saver(
        |size: &Size| {
            vec![
                (String::from("x"), SavedValue::Int(i64::from(size.x))),
                (String::from("y"), SavedValue::Int(i64::from(size.y))),
            ]
        },
        |entries| {
            let get = |name: &str| match entries.get(name) {
                Some(SavedValue::Int(value)) => Some(*value as i32),
                _ => None,
            };
            Some(Size { x: get("x")?, y: get("y")? })
        },
    );
    let saved = saver.save(&Size { x: 2, y: 3 }).expect("saved");
    assert_eq!(saver.restore(&saved), Some(Size { x: 2, y: 3 }));
}

// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/MapSaverTest.kt, nullableMapItemsAreSupported
// Compose: compose/runtime/runtime-saveable/src/androidHostTest/kotlin/androidx/compose/runtime/saveable/MapSaverTest.kt, nullableTypeIsSupported
#[test]
fn a_map_saver_saves_none_items() {
    let saver = map_saver(
        |size: &Option<NullableSize>| {
            let item = |value: Option<i32>| value.map(i64::from).save();
            vec![
                (String::from("x"), item(size.as_ref().and_then(|size| size.x))),
                (String::from("y"), item(size.as_ref().and_then(|size| size.y))),
            ]
        },
        |entries| {
            let get = |name: &str| Option::<i64>::restore(entries.get(name)?);
            Some(Some(NullableSize {
                x: get("x")?.map(|value| value as i32),
                y: get("y")?.map(|value| value as i32),
            }))
        },
    );
    let original = Some(NullableSize { x: None, y: Some(3) });
    let saved = saver.save(&original).expect("saved");
    assert_eq!(saver.restore(&saved), Some(original));
    let saved = saver.save(&None).expect("saved");
    assert_eq!(saver.restore(&saved), Some(Some(NullableSize { x: None, y: None })));
}
