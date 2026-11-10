//! Review of the runtime partials (D2140, D2141): traces of panics in deferred work when it
//! nests, moves or follows another panic, and saved state with placeholders.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::saveable::*;
use compose_runtime::stack_trace::*;
use compose_runtime::*;

use crate::mock::*;
use crate::restoration::RestorationTester;

struct TestPanic;

fn throw_test_panic() -> ! {
    std::panic::panic_any(TestPanic)
}

/// The trace `scenario`'s panic leaves in source-information mode: the names of this file's
/// composables, innermost first, `None` if it leaves none above what was there before.
fn traced(scenario: impl FnOnce()) -> Option<Vec<&'static str>> {
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::SourceInformation);
    let _ = take_compose_stack_trace();
    let before = compose_stack_trace_sequence();
    let panicked = catch_unwind(AssertUnwindSafe(scenario)).expect_err("it panics");
    assert!(panicked.is::<TestPanic>(), "the test's own panic");
    set_diagnostic_stack_trace_mode(ComposeStackTraceMode::Auto);
    let trace = take_compose_stack_trace()?;
    (trace.sequence > before).then(|| {
        trace
            .frames
            .iter()
            .filter(|frame| frame.path.is_some_and(|path| path.starts_with("suite::")))
            .filter_map(|frame| frame.name)
            .collect()
    })
}

#[composable]
fn Outer(content: impl Fn() + 'static) {
    content();
}

#[composable]
fn Inner(content: impl Fn() + 'static) {
    content();
}

/// A child composition whose content is set in an effect, as Compose's `Subcompose`.
#[composable]
fn EffectSubcompose(content: Rc<dyn Fn()>) {
    let context = remember_composition_context();
    let child = remember(move || Rc::new(Composition::with_parent(ViewApplier::new(), context)));
    let c = child.clone();
    DisposableEffect(child, move |scope| {
        let content = content.clone();
        c.set_content(move || content());
        let c = c.clone();
        scope.on_dispose(move || c.dispose())
    });
}

/// A child composition's effect panicking while its parent's effect applies it is traced from the
/// child's effect up through its place in the parent.
#[test]
fn a_child_effect_panicking_inside_a_parent_effect_traces_both() {
    let names = traced(|| {
        composition_test().compose(|| {
            Outer!(|| {
                EffectSubcompose(Rc::new(|| {
                    Inner!(|| DisposableEffect((), |_| throw_test_panic()));
                }));
            })
        })
    });
    assert_eq!(names, Some(vec!["Inner", "EffectSubcompose", "Outer"]));
}

/// An effect catching a child composition's panic, then another effect panicking: the trace
/// taken is the second panic's.
#[test]
#[ignore = "review: a trace a caught panic left during an apply stops the apply's own panic being traced"]
fn a_panic_after_a_caught_one_in_the_same_apply_is_traced_as_its_own() {
    #[composable]
    fn CatchingSubcompose() {
        let context = remember_composition_context();
        DisposableEffect((), move |scope| {
            let child = Rc::new(Composition::with_parent(ViewApplier::new(), context));
            let caught = catch_unwind(AssertUnwindSafe(|| {
                child.set_content(|| Inner!(|| throw_test_panic()));
            }));
            assert!(caught.is_err());
            scope.on_dispose(move || child.dispose())
        });
    }
    let names = traced(|| {
        composition_test().compose(|| {
            CatchingSubcompose();
            Outer!(|| DisposableEffect((), |_| throw_test_panic()));
        })
    });
    assert_eq!(names, Some(vec!["Outer"]));
}

/// Movable content moved into `Outer` in the pass whose effects panic: each effect is traced
/// at the content's new place.
fn moved_effect_trace(effect: fn()) -> Option<Vec<&'static str>> {
    traced(move || {
        let mut test = composition_test();
        let state = mutable_state_of(false);
        let s = state.handle();
        test.compose(move || {
            let movable = remember(move || {
                movable_content_of_with(move |moved: bool| {
                    Inner!(move || {
                        if moved {
                            effect();
                        }
                    })
                })
            });
            if s.get() {
                let movable = movable.clone();
                Outer!(move || movable(true));
            } else {
                movable(false);
            }
        });
        state.set(true);
        test.advance();
    })
}

#[test]
fn a_remember_observer_in_moved_content_is_traced_at_its_new_place() {
    let names = moved_effect_trace(|| DisposableEffect((), |_| throw_test_panic()));
    assert_eq!(names, Some(vec!["Inner", "Outer"]));
}

#[test]
fn a_side_effect_in_moved_content_is_traced_at_its_new_place() {
    let names = moved_effect_trace(|| SideEffect(|| throw_test_panic()));
    assert_eq!(names, Some(vec!["Inner", "Outer"]));
}

#[test]
fn a_keyed_side_effect_in_moved_content_is_traced_at_its_new_place() {
    let names = moved_effect_trace(|| KeyedSideEffect(1, || throw_test_panic()));
    assert_eq!(names, Some(vec!["Inner", "Outer"]));
}

/// A side effect disposing its own composition, then panicking: the trace guard finds nothing to
/// walk, and must not panic again while unwinding (an abort).
#[test]
fn an_effect_disposing_its_composition_then_panicking_unwinds() {
    let names = traced(|| {
        let composition: Rc<Cell<Option<Rc<Composition>>>> = Rc::default();
        let c = composition.clone();
        let own = Rc::new(Composition::new(ViewApplier::new()));
        composition.set(Some(own.clone()));
        own.set_content(move || {
            let c = c.clone();
            Outer!(move || {
                let c = c.clone();
                SideEffect(move || {
                    if let Some(own) = c.take() {
                        own.dispose();
                    }
                    throw_test_panic()
                })
            })
        });
    });
    // Nothing of the composition is left to trace.
    assert!(names.is_none() || names == Some(vec!["Outer"]), "{names:?}");
}

/// The same with a remember observer, whose target is a slot of the disposed table.
#[test]
fn an_observer_disposing_its_composition_then_panicking_unwinds() {
    let names = traced(|| {
        let composition: Rc<Cell<Option<Rc<Composition>>>> = Rc::default();
        let c = composition.clone();
        let own = Rc::new(Composition::new(ViewApplier::new()));
        composition.set(Some(own.clone()));
        own.set_content(move || {
            let c = c.clone();
            Outer!(move || {
                let c = c.clone();
                DisposableEffect((), move |_| {
                    if let Some(own) = c.take() {
                        own.dispose();
                    }
                    throw_test_panic()
                })
            })
        });
    });
    assert!(names.is_none() || names == Some(vec!["Outer"]), "{names:?}");
}

const KEY: u64 = 7;

/// Bytes written before tag 5 existed (no placeholder) still read.
#[test]
fn saved_bytes_without_placeholders_still_read() {
    // CRS1, one key, its two values: Int(3) (tag 1) and Bool(true) (tag 0), as main wrote them.
    let mut bytes = b"CRS1".to_vec();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&KEY.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&3i64.to_le_bytes());
    bytes.extend_from_slice(&[0, 1]);
    let mut expected = SavedState::default();
    expected.values.insert(KEY, vec![SavedValue::Int(3), SavedValue::Bool(true)]);
    assert_eq!(SavedState::from_bytes(&bytes), Some(expected.clone()));
    // And a state without placeholders encodes as it did.
    assert_eq!(expected.to_bytes(), bytes);
    let restored = SaveableStateRegistry::new(SavedState::from_bytes(&bytes));
    assert_eq!(restored.consume_restored(KEY), Some(SavedValue::Int(3)));
    assert_eq!(restored.consume_restored(KEY), Some(SavedValue::Bool(true)));
    assert_eq!(restored.consume_restored(KEY), None);
}

/// A placeholder is one byte, and round-trips at its position.
#[test]
fn a_placeholder_round_trips_in_bytes() {
    let mut state = SavedState::default();
    state.values.insert(KEY, vec![SavedValue::Null, SavedValue::Int(1), SavedValue::Null]);
    let encoded = state.to_bytes();
    assert_eq!(encoded.len(), 4 + 4 + 8 + 4 + 1 + 9 + 1);
    assert_eq!(SavedState::from_bytes(&encoded), Some(state));
}

/// Siblings under one key whose first saves nothing keep their order through a
/// `SaveableStateHolder`, whose saved states nest in a value.
#[test]
fn a_saver_returning_none_keeps_siblings_in_order_through_a_holder() {
    let first: Rc<Cell<Option<MutableState<i64>>>> = Rc::default();
    let second: Rc<Cell<Option<MutableState<i64>>>> = Rc::default();
    let (f, s) = (first.clone(), second.clone());
    let never_saves = || saver(|_: &i64| None, |_| None);
    let mut tester = RestorationTester::set_content(move || {
        let holder = remember_saveable_state_holder();
        let (f, s) = (f.clone(), s.clone());
        holder.SaveableStateProvider(1u8, move || {
            f.set(Some(remember_saveable_with(never_saves(), || mutable_state_of(1))));
            s.set(Some(remember_saveable(|| mutable_state_of(2))));
        });
    });
    first.take().expect("composed").set(10);
    second.take().expect("composed").set(20);
    tester.emulate_saved_instance_state_restore();
    assert_eq!(first.take().map(|state| state.get()), Some(1));
    assert_eq!(second.take().map(|state| state.get()), Some(20));
}

/// A provider giving a placeholder itself saves nothing, as one giving `None`: the state stays
/// empty, so a holder's or host's "nothing saved" holds (D2200).
#[test]
fn a_provider_giving_a_placeholder_saves_nothing() {
    let registry = SaveableStateRegistry::new(None);
    registry.register_provider(KEY, || Some(SavedValue::Null));
    assert!(registry.perform_save().is_empty());
    // Among others under its key it's a placeholder, as `None`.
    registry.register_provider(KEY, || Some(SavedValue::Int(1)));
    assert_eq!(
        registry.perform_save().values.get(&KEY),
        Some(&vec![SavedValue::Null, SavedValue::Int(1)])
    );
}
