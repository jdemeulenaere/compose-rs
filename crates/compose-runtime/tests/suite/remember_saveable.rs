//! Compose's `RememberSaveableTest`: restoring values, with savers, the registry's
//! `can_be_saved`, moving to another registry, the latest saver saving, unregistering on
//! disposal, and inputs resetting the value. Compose's tests wrap the registry in a delegate to
//! see what `rememberSaveable` asks of it; compose-rs's registry is a concrete type (D1950), so
//! these look at what the registry holds instead: its providers and what it saves.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use compose_runtime::platform::NodeTree;
use compose_runtime::*;

use crate::restoration::RestorationTester;

/// Composes `content` under `registry`, as Compose's tests' `WrapRegistry`.
fn compose_under(registry: SaveableStateRegistry, content: impl Fn() + 'static) -> Composition {
    let composition = Composition::new(NodeTree::<()>::new());
    let content = Rc::new(content);
    composition.set_content(move || {
        let content = content.clone();
        CompositionLocalProvider(
            LOCAL_SAVEABLE_STATE_REGISTRY.provides(Some(registry.clone())),
            move || content(),
        );
    });
    composition
}

/// Compose's `Holder`: a value changed in place.
type Holder = Rc<Cell<i32>>;

/// Compose's `HolderSaver`.
fn holder_saver() -> impl Saver<Holder> {
    saver(
        |holder: &Holder| Some(SavedValue::Int(i64::from(holder.get()))),
        |saved| match saved {
            SavedValue::Int(value) => Some(Rc::new(Cell::new(*value as i32))),
            _ => None,
        },
    )
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, simpleRestore
#[test]
fn a_value_changed_comes_back() {
    // Compose's `IntArray` changed in place; a list in a state here.
    let array: Rc<Cell<Option<MutableState<Vec<i32>>>>> = Rc::default();
    let a = array.clone();
    let mut tester = RestorationTester::set_content(move || {
        a.set(Some(remember_saveable(|| mutable_state_of(vec![0]))))
    });
    let state = array.take().expect("composed");
    assert_eq!(state.get(), vec![0]);
    state.set(vec![1]);
    tester.emulate_saved_instance_state_restore();
    assert_eq!(array.take().map(|state| state.get()), Some(vec![1]));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, restoreWithSaver
#[test]
fn a_value_comes_back_through_its_saver() {
    let holder: Rc<RefCell<Option<Holder>>> = Rc::default();
    let h = holder.clone();
    let mut tester = RestorationTester::set_content(move || {
        *h.borrow_mut() = Some(remember_saveable_with(holder_saver(), || Rc::new(Cell::new(0))));
    });
    let value = holder.take().expect("composed");
    assert_eq!(value.get(), 0);
    value.set(1);
    tester.emulate_saved_instance_state_restore();
    assert_eq!(holder.take().map(|holder| holder.get()), Some(1));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, canBeSavedFromRegistryIsUsed
#[test]
fn the_registrys_can_be_saved_is_used() {
    let called_with: Rc<RefCell<Vec<SavedValue>>> = Rc::default();
    let c = called_with.clone();
    let registry = SaveableStateRegistry::with_can_be_saved(None, move |value| {
        c.borrow_mut().push(value.clone());
        true
    });
    let _composition = compose_under(registry.clone(), || {
        let value = remember_saveable(|| 1);
        assert_eq!(value, 1);
    });
    // Asked when the value registers, as Compose's `requireCanBeSaved`...
    assert_eq!(*called_with.borrow(), vec![SavedValue::Int(1)]);
    // ...and when it's saved.
    registry.perform_save();
    assert_eq!(*called_with.borrow(), vec![SavedValue::Int(1), SavedValue::Int(1)]);
}

/// The values `registry` saves, whatever their keys.
fn saved_values(registry: &SaveableStateRegistry) -> Vec<Vec<SavedValue>> {
    registry.perform_save().values.into_values().collect()
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, providerProvidesCorrectlySavedValue
#[test]
fn the_registered_value_saves_the_value() {
    let registry = SaveableStateRegistry::new(None);
    let _composition = compose_under(registry.clone(), || {
        let value = remember_saveable(|| 2);
        assert_eq!(value, 2);
    });
    assert_eq!(saved_values(&registry), vec![vec![SavedValue::Int(2)]]);
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, providerProvidesCorrectlySavedValueWithSaver
#[test]
fn the_registered_value_saves_the_value_with_its_saver() {
    let registry = SaveableStateRegistry::new(None);
    let _composition = compose_under(registry.clone(), || {
        remember_saveable_with(holder_saver(), || Rc::new(Cell::new(4)));
    });
    assert_eq!(saved_values(&registry), vec![vec![SavedValue::Int(4)]]);
}

#[test]
fn a_skippable_childs_value_moves_to_the_registry_provided_next() {
    // The local is static, as Compose's: another registry provided recomposes the provider's
    // whole content, a child with nothing else changed included, and its value moves.
    #[composable]
    fn Child(recompositions: Rc<Cell<u32>>) {
        recompositions.set(recompositions.get() + 1);
        remember_saveable(|| 1);
    }
    let (first, second) = (SaveableStateRegistry::new(None), SaveableStateRegistry::new(None));
    let use_second = mutable_state_of(false);
    let recompositions = Rc::new(Cell::new(0));
    let (flag, f, s, r) =
        (use_second.handle(), first.clone(), second.clone(), recompositions.clone());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || {
        let registry = if flag.get() { s.clone() } else { f.clone() };
        let r = r.clone();
        CompositionLocalProvider(
            LOCAL_SAVEABLE_STATE_REGISTRY.provides(Some(registry)),
            move || Child(r.clone()),
        );
    });
    assert_eq!(recompositions.get(), 1);
    use_second.set(true);
    composition.recompose();
    assert_eq!(recompositions.get(), 2);
    assert!(!first.has_providers());
    assert_eq!(saved_values(&second), vec![vec![SavedValue::Int(1)]]);
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, unregistersFromPrevProviderAndRegistersToTheNewOne
#[test]
fn a_value_moves_to_the_registry_provided_next() {
    let (first, second) = (SaveableStateRegistry::new(None), SaveableStateRegistry::new(None));
    let use_second = mutable_state_of(false);
    let flag = use_second.handle();
    let (f, s) = (first.clone(), second.clone());
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || {
        let registry = if flag.get() { s.clone() } else { f.clone() };
        CompositionLocalProvider(LOCAL_SAVEABLE_STATE_REGISTRY.provides(Some(registry)), || {
            let value = remember_saveable(|| 1);
            assert_eq!(value, 1);
        });
    });
    assert!(first.has_providers());
    assert!(!second.has_providers());
    let key: Vec<u64> = first.perform_save().values.into_keys().collect();
    use_second.set(true);
    composition.recompose();
    // Unregistered from the first, and registered with the second under the same key.
    assert!(!first.has_providers());
    let saved = second.perform_save();
    assert_eq!(saved.values.keys().copied().collect::<Vec<_>>(), key);
    assert_eq!(saved.values[&key[0]], vec![SavedValue::Int(1)]);
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, theLatestPassedSaverIsUsed
#[test]
fn the_latest_saver_passed_is_used() {
    let saver_used = Rc::new(Cell::new(0));
    let version = mutable_state_of(1);
    let v = version.handle();
    let used = saver_used.clone();
    let mut tester = RestorationTester::set_content(move || {
        // A saver of the same type each time, telling which composition passed it.
        let passed = v.get();
        let used = used.clone();
        remember_saveable_with(
            saver(
                move |_: &i32| {
                    used.set(passed);
                    Some(SavedValue::Int(1))
                },
                |_| Some(1),
            ),
            || 1,
        );
    });
    version.set(2);
    tester.run_on_idle();
    tester.emulate_saved_instance_state_restore();
    assert_eq!(saver_used.get(), 2);
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, unregistersWhenDisposed
#[test]
fn a_value_unregisters_when_it_leaves() {
    #[composable]
    fn Content(emit: MutableState<bool>) {
        if emit.get() {
            remember_saveable(|| 1);
        }
    }
    let registry = SaveableStateRegistry::new(None);
    let do_emit = mutable_state_of(true);
    let emit = do_emit.handle();
    let composition = compose_under(registry.clone(), move || Content(emit));
    assert!(registry.has_providers());
    do_emit.set(false);
    composition.recompose();
    assert!(!registry.has_providers());
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, restoreCorrectValueAfterInputChanges
#[test]
fn the_value_after_its_inputs_changed_comes_back() {
    let counter = Rc::new(Cell::new(0));
    let composed: Rc<Cell<Option<i32>>> = Rc::default();
    let input = mutable_state_of(0);
    let (c, out, i) = (counter.clone(), composed.clone(), input.handle());
    let mut tester = RestorationTester::set_content(move || {
        let c = c.clone();
        out.set(Some(remember_saveable_keyed(i.get(), move || {
            let value = c.get();
            c.set(value + 1);
            value
        })));
    });
    assert_eq!(composed.get(), Some(0));
    // The value resets.
    input.set(1);
    tester.run_on_idle();
    assert_eq!(composed.take(), Some(1));
    tester.emulate_saved_instance_state_restore();
    assert_eq!(composed.get(), Some(1));
    assert_eq!(counter.get(), 2);
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableTest.kt, changingInputIsNotAffectingOrderOfRestoration
#[test]
fn inputs_changing_keep_the_order_values_are_restored_in() {
    // Two calls in one composable share its key here (D290), told apart by their order: the
    // one whose inputs change keeps its place among them.
    let counter = Rc::new(Cell::new(0));
    let input = mutable_state_of(0);
    let with_input: Rc<Cell<Option<i32>>> = Rc::default();
    let without_input: Rc<RefCell<Option<String>>> = Rc::default();
    let (c, i, wi, wo) =
        (counter.clone(), input.handle(), with_input.clone(), without_input.clone());
    let mut tester = RestorationTester::set_content(move || {
        let next = || {
            let value = c.get();
            c.set(value + 1);
            value
        };
        wi.set(Some(remember_saveable_keyed(i.get(), next)));
        *wo.borrow_mut() = Some(remember_saveable(|| next().to_string()));
    });
    assert!(with_input.take().is_some());
    input.set(1);
    tester.run_on_idle();
    let expected_with_input = with_input.take().expect("composed");
    let expected_without_input = without_input.take().expect("composed");
    assert_eq!((expected_with_input, expected_without_input.as_str()), (2, "1"));
    tester.emulate_saved_instance_state_restore();
    assert_eq!(with_input.get(), Some(expected_with_input));
    assert_eq!(without_input.take(), Some(expected_without_input));
}

#[test]
fn a_value_replaced_when_inputs_change_goes_at_the_end_of_the_pass() {
    // Compose's garbage collector takes the replaced value; here it's dropped when the pass is
    // applied, as `remember_keyed`'s replaced value is, and what's saved is the new one.
    let input = mutable_state_of(0);
    let values: Rc<RefCell<Vec<Weak<i32>>>> = Rc::default();
    let previous_alive_in_pass: Rc<Cell<Option<bool>>> = Rc::default();
    let (i, v, alive) = (input.handle(), values.clone(), previous_alive_in_pass.clone());
    let rc_saver = || {
        saver(
            |value: &Rc<i32>| Some(SavedValue::Int(i64::from(**value))),
            |saved| match saved {
                SavedValue::Int(value) => Some(Rc::new(*value as i32)),
                _ => None,
            },
        )
    };
    let tester = RestorationTester::set_content(move || {
        let input = i.get();
        let value = remember_saveable_keyed_with(input, rc_saver(), || Rc::new(10 + input));
        let mut values = v.borrow_mut();
        if let Some(previous) = values.last() {
            alive.set(Some(previous.upgrade().is_some()));
        }
        values.push(Rc::downgrade(&value));
    });
    input.set(1);
    tester.run_on_idle();
    // The first value lived through the pass replacing it, and went when it applied.
    assert_eq!(previous_alive_in_pass.get(), Some(true));
    let values = values.borrow();
    assert!(values[0].upgrade().is_none());
    assert_eq!(values.last().and_then(Weak::upgrade).as_deref(), Some(&11));
    assert_eq!(
        tester.registry().perform_save().values.into_values().collect::<Vec<_>>(),
        vec![vec![SavedValue::Int(11)]]
    );
}

// ---- RememberSaveableWithMutableStateTest ---------------------------------------------------

/// Compose's `Holder` (`data class Holder(var value: Int)`): mutated in place, equal by value.
#[derive(Clone, Debug)]
struct MutableHolder(Rc<Cell<i32>>);

impl MutableHolder {
    fn new(value: i32) -> Self {
        Self(Rc::new(Cell::new(value)))
    }
}

impl PartialEq for MutableHolder {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

fn mutable_holder_saver() -> impl Saver<MutableHolder> {
    saver(
        |holder: &MutableHolder| Some(SavedValue::Int(i64::from(holder.0.get()))),
        |saved| match saved {
            SavedValue::Int(value) => Some(MutableHolder::new(*value as i32)),
            _ => None,
        },
    )
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableWithMutableStateTest.kt, restoreWithSaver
#[test]
fn a_mutable_state_comes_back_through_its_state_saver() {
    let state_cell: Rc<Cell<Option<MutableState<MutableHolder>>>> = Rc::default();
    let s = state_cell.clone();
    let mut tester = RestorationTester::set_content(move || {
        s.set(Some(remember_saveable_with(mutable_holder_saver(), || {
            mutable_state_of(MutableHolder::new(0))
        })));
    });
    tester.run_on_idle();
    let state = state_cell.take().expect("composed");
    assert_eq!(state.get(), MutableHolder::new(0));
    state.get().0.set(1);
    tester.emulate_saved_instance_state_restore();
    tester.run_on_idle();
    assert_eq!(state_cell.take().map(|s| s.get()), Some(MutableHolder::new(1)));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableWithMutableStateTest.kt, nullableStateRestoresNonNullValue
#[test]
fn an_optional_state_set_to_some_comes_back_with_its_value() {
    let state_cell: Rc<Cell<Option<MutableState<Option<String>>>>> = Rc::default();
    let s = state_cell.clone();
    let mut tester = RestorationTester::set_content(move || {
        s.set(Some(remember_saveable(|| mutable_state_of(None))));
    });
    let state = state_cell.take().expect("composed");
    assert_eq!(state.get(), None);
    state.set(Some(String::from("value")));
    tester.emulate_saved_instance_state_restore();
    assert_eq!(state_cell.take().map(|s| s.get()), Some(Some(String::from("value"))));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableWithMutableStateTest.kt, nullableStateRestoresNullValue
#[test]
fn an_optional_state_set_to_none_comes_back_as_none() {
    let state_cell: Rc<Cell<Option<MutableState<Option<String>>>>> = Rc::default();
    let s = state_cell.clone();
    let mut tester = RestorationTester::set_content(move || {
        s.set(Some(remember_saveable(|| mutable_state_of(Some(String::from("initial"))))));
    });
    let state = state_cell.take().expect("composed");
    assert_eq!(state.get().as_deref(), Some("initial"));
    state.set(None);
    tester.emulate_saved_instance_state_restore();
    assert_eq!(state_cell.take().map(|s| s.get()), Some(None));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableWithMutableStateTest.kt, stateSaverReturnsNull
#[test]
fn a_state_whose_saver_saves_nothing_runs_init_again() {
    let state_cell: Rc<Cell<Option<MutableState<String>>>> = Rc::default();
    let s = state_cell.clone();
    let never_saves = || {
        saver(
            |_: &String| None,
            |saved| match saved {
                SavedValue::Text(value) => Some(value.clone()),
                _ => None,
            },
        )
    };
    let mut tester = RestorationTester::set_content(move || {
        s.set(Some(remember_saveable_with(never_saves(), || {
            mutable_state_of(String::from("value"))
        })));
    });
    let state = state_cell.take().expect("composed");
    assert_eq!(state.get(), "value");
    state.set(String::from("unsaved"));
    tester.emulate_saved_instance_state_restore();
    assert_eq!(state_cell.take().map(|s| s.get()), Some(String::from("value")));
}

#[test]
fn a_saver_returning_none_keeps_the_order_of_siblings_under_the_same_key() {
    let first_cell: Rc<Cell<Option<MutableState<String>>>> = Rc::default();
    let second_cell: Rc<Cell<Option<MutableState<String>>>> = Rc::default();
    let (f, s) = (first_cell.clone(), second_cell.clone());
    let never_saves = || {
        saver(
            |_: &String| None,
            |saved| match saved {
                SavedValue::Text(value) => Some(value.clone()),
                _ => None,
            },
        )
    };
    let mut tester = RestorationTester::set_content(move || {
        f.set(Some(remember_saveable_with(never_saves(), || {
            mutable_state_of(String::from("first-init"))
        })));
        s.set(Some(remember_saveable(|| mutable_state_of(String::from("second-init")))));
    });
    first_cell.take().expect("composed").set(String::from("first-edited"));
    second_cell.take().expect("composed").set(String::from("second-saved"));
    tester.emulate_saved_instance_state_restore();
    assert_eq!(first_cell.take().map(|st| st.get()), Some(String::from("first-init")));
    assert_eq!(second_cell.take().map(|st| st.get()), Some(String::from("second-saved")));
}
