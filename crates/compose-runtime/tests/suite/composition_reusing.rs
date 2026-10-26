//! Compose's `CompositionReusingTests`, adapted: `ReusableContent`, `ReusableContentHost` and
//! the node lifecycle (reuse, deactivation, release) under them, on the mock views.

#![allow(non_snake_case)]

use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReuseIsCalledBeforeSetter
#[test]
fn a_reused_node_is_reset_before_its_setters_run() {
    #[composable]
    fn Content(reuse_key: MutableState<i32>, on_reuse: Action, on_set: Action) {
        ReusableContent(reuse_key.get(), || {
            LinearWithCallbacks(on_reuse.clone(), nothing(), nothing(), on_set.clone(), || {});
        });
    }
    let mut test = composition_test();
    let reuse_key = mutable_state_of(0);
    let reuse_calls = Counter::new();
    let calls_when_set = Log::new();
    let on_reuse: Action = {
        let calls = reuse_calls.clone();
        Rc::new(move || {
            calls.increment();
        })
    };
    let on_set: Action = {
        let (calls, log) = (reuse_calls.clone(), calls_when_set.clone());
        Rc::new(move || log.push(calls.get().to_string()))
    };
    let k = reuse_key.handle();
    test.compose(move || Content(k, on_reuse.clone(), on_set.clone()));
    test.validate(|v| v.empty_linear());
    assert_eq!(calls_when_set.take(), ["0"]);
    reuse_key.set(1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(calls_when_set.take(), ["1"], "reset (onReuse) before the setter ran");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onDeactivateIsCalledWhenReusableContentDeactivated
#[test]
fn deactivated_content_deactivates_its_nodes() {
    #[composable]
    fn Content(active: MutableState<bool>, on_deactivate: Action) {
        ReusableContentHost(active.get(), || {
            ReusableContent(0, || {
                LinearWithCallbacks(nothing(), on_deactivate.clone(), nothing(), nothing(), || {});
            });
        });
    }
    let mut test = composition_test();
    let active = mutable_state_of(true);
    let deactivate_calls = Counter::new();
    let on_deactivate: Action = {
        let calls = deactivate_calls.clone();
        Rc::new(move || {
            calls.increment();
        })
    };
    let a = active.handle();
    test.compose(move || Content(a, on_deactivate.clone()));
    test.validate(|v| v.empty_linear());
    assert_eq!(deactivate_calls.get(), 0);
    active.set(false);
    test.expect_changes();
    test.revalidate();
    assert_eq!(deactivate_calls.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, onReuseIsCalledBeforeSetterAfterDeactivation
#[test]
fn a_node_reactivated_is_reset_before_its_setters_run() {
    #[composable]
    fn Content(active: MutableState<bool>, on_reuse: Action, on_set: Action) {
        ReusableContentHost(active.get(), || {
            ReusableContent(0, || {
                LinearWithCallbacks(on_reuse.clone(), nothing(), nothing(), on_set.clone(), || {});
            });
        });
    }
    let mut test = composition_test();
    let active = mutable_state_of(true);
    let reuse_calls = Counter::new();
    let calls_when_set = Log::new();
    let on_reuse: Action = {
        let calls = reuse_calls.clone();
        Rc::new(move || {
            calls.increment();
        })
    };
    let on_set: Action = {
        let (calls, log) = (reuse_calls.clone(), calls_when_set.clone());
        Rc::new(move || log.push(calls.get().to_string()))
    };
    let a = active.handle();
    test.compose(move || Content(a, on_reuse.clone(), on_set.clone()));
    test.validate(|v| v.empty_linear());
    active.set(false);
    test.expect_changes();
    test.revalidate();
    active.set(true);
    test.expect_changes();
    test.revalidate();
    assert_eq!(calls_when_set.entries(), ["0", "1"]);
}
