//! `NavBackStack` and `remember_nav_back_stack`, against Navigation 3's `RememberNavBackStackTest`
//! and `NavBackStackSerializerTest`: `Saveable` for kotlinx.serialization. An enum of keys needs no
//! registering of subclasses, so `configurationRestore` and `defaultConfigurationFails` have no
//! analogue, nor `noSerializerFail` and `encode_withCustomModuleAndMissingSubtype`: a key that can't
//! be saved doesn't compile.

use std::cell::Cell;

use compose_navigation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::UiHost;

#[derive(Clone, PartialEq, Debug)]
enum Key {
    Object,
    Class,
    Data(i32),
    Sealed(Sealed),
    Argument(Argument),
}

/// Kotlin's sealed class, of one object.
#[derive(Clone, PartialEq, Debug)]
enum Sealed {
    One,
}

/// A value with its own way to save, Kotlin's `TestArg` with its `DeepLinkSerializer`.
#[derive(Clone, PartialEq, Debug)]
struct Argument {
    age: i32,
    height: f64,
}

impl Saveable for Argument {
    fn save(&self) -> SavedValue {
        SavedValue::Text(format!("{}--{}", self.age, self.height))
    }

    fn restore(saved: &SavedValue) -> Option<Argument> {
        let SavedValue::Text(text) = saved else { return None };
        let (age, height) = text.split_once("--")?;
        Some(Argument { age: age.parse().ok()?, height: height.parse().ok()? })
    }
}

impl Saveable for Key {
    fn save(&self) -> SavedValue {
        match self {
            Key::Object => SavedValue::Text("object".to_string()),
            Key::Class => SavedValue::Text("class".to_string()),
            Key::Sealed(Sealed::One) => SavedValue::Text("sealed".to_string()),
            Key::Data(value) => SavedValue::Int(i64::from(*value)),
            Key::Argument(argument) => SavedValue::List(vec![argument.save()]),
        }
    }

    fn restore(saved: &SavedValue) -> Option<Key> {
        match saved {
            SavedValue::Text(name) => match name.as_str() {
                "object" => Some(Key::Object),
                "class" => Some(Key::Class),
                "sealed" => Some(Key::Sealed(Sealed::One)),
                _ => None,
            },
            SavedValue::Int(value) => Some(Key::Data(i32::try_from(*value).ok()?)),
            SavedValue::List(items) => Some(Key::Argument(Argument::restore(items.first()?)?)),
            _ => None,
        }
    }
}

thread_local! {
    static STACK: Cell<Option<NavBackStack<Key>>> = const { Cell::new(None) };
}

#[composable(non_restartable)]
fn Remembering(start: Vec<Key>) {
    STACK.set(Some(remember_nav_back_stack(start)));
}

fn stack() -> NavBackStack<Key> {
    STACK.get().unwrap()
}

/// A stack starting empty is given `key`, the host is destroyed, and a new one composes with what it
/// saved, through bytes.
fn restored_with(key: Key) -> Vec<Key> {
    let mut ui = compose_test(|| Remembering(Vec::new()));
    ui.wait_for_idle();
    assert!(stack().is_empty());
    stack().push(key);
    ui.wait_for_idle();
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    STACK.set(None);
    let host = UiHost::headless(100, 100);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    let mut ui = ComposeTestRule::with_host(host, || Remembering(Vec::new()));
    ui.wait_for_idle();
    stack().iter_cloned().collect()
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt, simpleObjectRestore
#[test]
fn a_key_with_no_value_comes_back() {
    assert_eq!(restored_with(Key::Object), [Key::Object]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt, simpleClassRestore
#[test]
fn another_kind_of_key_comes_back_as_itself() {
    assert_eq!(restored_with(Key::Class), [Key::Class]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt, simpleDataClassRestore
#[test]
fn a_key_comes_back_with_its_value() {
    assert_eq!(restored_with(Key::Data(2)), [Key::Data(2)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt, simpleClassRestore_deepLinkSerializer
#[test]
fn a_key_comes_back_with_a_value_saving_its_own_way() {
    let argument = Argument { age: 12, height: 2.1 };
    assert_eq!(restored_with(Key::Argument(argument.clone())), [Key::Argument(argument)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt, simpleDataClassRestore_deepLinkSerializer
#[test]
fn the_fields_of_a_value_saving_its_own_way_are_kept() {
    let restored = restored_with(Key::Argument(Argument { age: 12, height: 2.1 }));
    let Key::Argument(argument) = &restored[0] else { panic!("{restored:?}") };
    assert_eq!(argument.age, 12);
    assert_eq!(argument.height, 2.1);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt, sealedClassRestore
#[test]
fn a_key_of_a_nested_enum_comes_back() {
    assert_eq!(restored_with(Key::Sealed(Sealed::One)), [Key::Sealed(Sealed::One)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/RememberNavBackStackTest.kt, polymorphicClassRestore_deepLinkSerializer
#[test]
fn keys_of_every_kind_come_back_in_their_order() {
    let keys = [
        Key::Object,
        Key::Data(1),
        Key::Sealed(Sealed::One),
        Key::Argument(Argument { age: 3, height: 0.5 }),
        Key::Data(1),
    ];
    let mut ui = compose_test(|| Remembering(Vec::new()));
    ui.wait_for_idle();
    stack().add_all(keys.clone());
    ui.wait_for_idle();
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(100, 100);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    let mut ui = ComposeTestRule::with_host(host, || Remembering(Vec::new()));
    ui.wait_for_idle();
    assert_eq!(stack().iter_cloned().collect::<Vec<_>>(), keys);
}

/// What it's given to start with is its first keys, and what's restored goes before them.
#[test]
fn a_stack_starts_with_its_elements_unless_restored() {
    let mut ui = compose_test(|| Remembering(vec![Key::Object]));
    ui.wait_for_idle();
    assert_eq!(stack().iter_cloned().collect::<Vec<_>>(), [Key::Object]);
    stack().push(Key::Class);
    ui.wait_for_idle();
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(100, 100);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    let mut ui = ComposeTestRule::with_host(host, || Remembering(vec![Key::Object]));
    ui.wait_for_idle();
    assert_eq!(stack().iter_cloned().collect::<Vec<_>>(), [Key::Object, Key::Class]);
}

fn round_trip(keys: Vec<Key>) -> Vec<Key> {
    let saved = keys.save();
    let bytes = {
        let mut state = SavedState::default();
        state.values.insert(1, vec![saved]);
        state.to_bytes()
    };
    let state = SavedState::from_bytes(&bytes).expect("saved state bytes");
    Vec::<Key>::restore(&state.values[&1][0]).expect("restored")
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavBackStackSerializerTest.kt, encodeDecode_withDefaultConfigAndMixedSubtypes_preservesValues
#[test]
fn keys_of_mixed_kinds_save_and_restore_by_value() {
    let keys = vec![Key::Object, Key::Data(42), Key::Sealed(Sealed::One)];
    assert_eq!(round_trip(keys.clone()), keys);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavBackStackSerializerTest.kt, encodeDecode_withCustomModuleAndRegisteredSubtypes_preservesValues
#[test]
fn keys_saving_their_own_way_save_and_restore_by_value() {
    let keys = vec![Key::Class, Key::Argument(Argument { age: 7, height: 1.5 })];
    assert_eq!(round_trip(keys.clone()), keys);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavBackStackSerializerTest.kt, encodeDecode_withEmptyBackStack_returnsEmptyList
#[test]
fn an_empty_stack_saves_and_restores_empty() {
    assert_eq!(round_trip(Vec::new()), []);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/NavBackStackSerializerTest.kt, encodeDecode_withDefaultConfig_preservesOrder
#[test]
fn a_saved_stack_keeps_its_order() {
    let keys = vec![Key::Data(3), Key::Data(1), Key::Object, Key::Data(2)];
    assert_eq!(round_trip(keys.clone()), keys);
}

/// A key that can't be restored (of another version of the app) starts it again.
#[test]
fn a_stack_that_can_t_be_restored_starts_again() {
    assert_eq!(Vec::<Key>::restore(&SavedValue::List(vec![SavedValue::Bool(true)])), None);
    let mut ui = compose_test(|| Remembering(Vec::new()));
    ui.wait_for_idle();
    let mut state = ui.host().save_state();
    for values in state.values.values_mut() {
        *values = vec![SavedValue::List(vec![SavedValue::Bool(true)])];
    }
    let bytes = state.to_bytes();
    drop(ui);
    let host = UiHost::headless(100, 100);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    let mut ui = ComposeTestRule::with_host(host, || Remembering(vec![Key::Object]));
    ui.wait_for_idle();
    assert_eq!(stack().iter_cloned().collect::<Vec<_>>(), [Key::Object]);
}

/// A stack is state: what reads it recomposes, and it can be edited as a list.
#[test]
fn a_stack_s_edits_recompose_what_reads_it() {
    let owner = NavBackStack::new([1, 2]);
    let stack = *owner;
    let seen = std::rc::Rc::new(Cell::new(0));
    let read = seen.clone();
    let mut ui = compose_test(move || read.set(stack.len()));
    ui.wait_for_idle();
    assert_eq!(seen.get(), 2);
    assert_eq!(stack.remove_last_or_none(), Some(2));
    ui.wait_for_idle();
    assert_eq!(seen.get(), 1);
    stack.remove_last_or_none();
    assert_eq!(stack.remove_last_or_none(), None);
}
