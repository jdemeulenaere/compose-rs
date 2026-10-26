//! Compose's `RestorationInVariousScenariosTest`: `remember_saveable` values find their place
//! again after the content is recreated from saved state, in loops, under keys, in extra
//! functions and after conditional content. A [`RestorationTester`] stands for
//! `StateRestorationTester`: the composition and its registry are recreated from what the
//! registry saved, through its bytes. Content with conditions or loops is a `#[composable]`
//! function, as Compose's composable lambdas are: a plain closure's `if` gets no groups, so a
//! value after it would take the place of one the condition left out.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::platform::NodeTree;
use compose_runtime::saveable::SavedState;
use compose_runtime::*;

/// Composes content under a saveable state registry, and recreates it from the saved state, as
/// `StateRestorationTester`.
struct RestorationTester {
    content: Rc<dyn Fn()>,
    registry: SaveableStateRegistry,
    composition: Option<Composition>,
}

impl RestorationTester {
    fn set_content(content: impl Fn() + 'static) -> RestorationTester {
        let mut tester = RestorationTester {
            content: Rc::new(content),
            registry: SaveableStateRegistry::new(None),
            composition: None,
        };
        tester.compose();
        tester
    }

    fn compose(&mut self) {
        let composition = Composition::new(NodeTree::<()>::new());
        let (registry, content) = (self.registry.clone(), self.content.clone());
        composition.set_content(move || {
            let content = content.clone();
            CompositionLocalProvider(
                LOCAL_SAVEABLE_STATE_REGISTRY.provides(Some(registry.clone())),
                move || content(),
            );
        });
        self.composition = Some(composition);
    }

    /// Recomposes what's pending, as `runOnIdle`.
    fn run_on_idle(&self) {
        self.composition.as_ref().expect("composed").recompose();
    }

    /// Saves the state, disposes the content and composes it again restoring it, as
    /// `emulateSavedInstanceStateRestore`.
    fn emulate_saved_instance_state_restore(&mut self) {
        self.run_on_idle();
        let bytes = self.registry.perform_save().to_bytes();
        self.composition = None;
        self.registry = SaveableStateRegistry::new(SavedState::from_bytes(&bytes));
        self.compose();
    }
}

type States = Rc<RefCell<[Option<MutableState<i32>>; 2]>>;

fn states() -> States {
    Rc::default()
}

fn value(states: &States, index: usize) -> Option<i32> {
    states.borrow()[index].map(|state| state.get())
}

/// Sets the states' values, and forgets them to see that the content composed again.
fn set_and_forget(states: &States, values: [Option<i32>; 2]) {
    let mut states = states.borrow_mut();
    for (state, value) in states.iter_mut().zip(values) {
        if let Some(value) = value {
            state.expect("composed").set(value);
        }
        *state = None;
    }
}

#[composable]
fn FunctionWithState(states: States, index: usize) {
    states.borrow_mut()[index] = Some(remember_saveable(|| mutable_state_of(0)));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, insideForLoop
#[test]
fn values_in_a_loop_are_restored() {
    let states = states();
    let s = states.clone();
    let mut tester = RestorationTester::set_content(move || {
        for index in 0..2 {
            s.borrow_mut()[index] = Some(remember_saveable(|| mutable_state_of(0)));
        }
    });
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(0), Some(0)));
    set_and_forget(&states, [Some(1), Some(2)]);
    tester.emulate_saved_instance_state_restore();
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(1), Some(2)));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, insideForLoop_withKey
#[test]
fn values_in_a_keyed_loop_are_restored() {
    let states = states();
    let s = states.clone();
    let mut tester = RestorationTester::set_content(move || {
        for index in 0..2 {
            key(index, || s.borrow_mut()[index] = Some(remember_saveable(|| mutable_state_of(0))));
        }
    });
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(0), Some(0)));
    set_and_forget(&states, [Some(1), Some(2)]);
    tester.emulate_saved_instance_state_restore();
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(1), Some(2)));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, insideForLoop_withExtraFunction
#[test]
fn values_in_a_function_called_in_a_loop_are_restored() {
    let states = states();
    let s = states.clone();
    let mut tester = RestorationTester::set_content(move || {
        for index in 0..2 {
            FunctionWithState(s.clone(), index);
        }
    });
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(0), Some(0)));
    set_and_forget(&states, [Some(1), Some(2)]);
    tester.emulate_saved_instance_state_restore();
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(1), Some(2)));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, changingLoopCountWithExtraStateAfter
#[test]
fn a_value_after_a_loop_that_shrank_is_restored() {
    let number = Rc::new(Cell::new(2));
    let states = states();
    let outside: Rc<Cell<Option<MutableState<String>>>> = Rc::default();
    #[composable]
    fn Content(
        number: Rc<Cell<usize>>,
        states: States,
        outside: Rc<Cell<Option<MutableState<String>>>>,
    ) {
        for index in 0..number.get() {
            states.borrow_mut()[index] = Some(remember_saveable(|| mutable_state_of(0)));
        }
        outside.set(Some(remember_saveable(|| mutable_state_of("0".to_string()))));
    }
    let (n, s, o) = (number.clone(), states.clone(), outside.clone());
    let mut tester =
        RestorationTester::set_content(move || Content(n.clone(), s.clone(), o.clone()));
    tester.run_on_idle();
    set_and_forget(&states, [Some(1), None]);
    outside.take().expect("composed").set("1".into());
    number.set(1);
    tester.emulate_saved_instance_state_restore();
    assert_eq!(value(&states, 0), Some(1));
    assert_eq!(outside.take().map(|state| state.get()), Some("1".to_string()));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, twoStates
#[test]
fn two_values_are_restored() {
    let states = states();
    let s = states.clone();
    let mut tester = RestorationTester::set_content(move || {
        s.borrow_mut()[0] = Some(remember_saveable(|| mutable_state_of(0)));
        s.borrow_mut()[1] = Some(remember_saveable(|| mutable_state_of(0)));
    });
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(0), Some(0)));
    set_and_forget(&states, [Some(1), Some(2)]);
    tester.emulate_saved_instance_state_restore();
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(1), Some(2)));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, twoStates_firstStateIsConditional
#[test]
fn a_value_after_conditional_content_left_out_is_restored() {
    let need_first = Rc::new(Cell::new(true));
    let states = states();
    #[composable]
    fn Content(need_first: Rc<Cell<bool>>, states: States) {
        if need_first.get() {
            states.borrow_mut()[0] = Some(remember_saveable(|| mutable_state_of(0)));
        }
        states.borrow_mut()[1] = Some(remember_saveable(|| mutable_state_of(0)));
    }
    let (n, s) = (need_first.clone(), states.clone());
    let mut tester = RestorationTester::set_content(move || Content(n.clone(), s.clone()));
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(0), Some(0)));
    set_and_forget(&states, [None, Some(1)]);
    need_first.set(false);
    tester.emulate_saved_instance_state_restore();
    assert_eq!(value(&states, 0), None);
    assert_eq!(value(&states, 1), Some(1));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, twoStates_withExtraFunction
#[test]
fn two_values_in_functions_are_restored() {
    let states = states();
    let s = states.clone();
    let mut tester = RestorationTester::set_content(move || {
        FunctionWithState(s.clone(), 0);
        FunctionWithState(s.clone(), 1);
    });
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(0), Some(0)));
    set_and_forget(&states, [Some(1), Some(2)]);
    tester.emulate_saved_instance_state_restore();
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(1), Some(2)));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RestorationInVariousScenariosTest.kt, twoStates_withExtraFunction_firstStateIsConditional
#[test]
fn a_value_in_a_function_after_conditional_content_left_out_is_restored() {
    let need_first = Rc::new(Cell::new(true));
    let states = states();
    #[composable]
    fn Content(need_first: Rc<Cell<bool>>, states: States) {
        if need_first.get() {
            FunctionWithState(states.clone(), 0);
        }
        FunctionWithState(states, 1);
    }
    let (n, s) = (need_first.clone(), states.clone());
    let mut tester = RestorationTester::set_content(move || Content(n.clone(), s.clone()));
    assert_eq!((value(&states, 0), value(&states, 1)), (Some(0), Some(0)));
    set_and_forget(&states, [None, Some(1)]);
    need_first.set(false);
    tester.emulate_saved_instance_state_restore();
    assert_eq!(value(&states, 0), None);
    assert_eq!(value(&states, 1), Some(1));
}
