//! Compose's `JvmCompositionTests`, adapted: a composable given unchanged NaN arguments skips
//! (the `Dp` ones are in the facade's suite, which has the ui types; the thread and snapshot
//! tests are n/a: state is thread-bound, D14).

#![allow(non_snake_case)]

use compose_runtime::*;

use crate::mock::*;

#[composable]
fn WithFloatParameter(value: f32, composition_count: Counter) {
    composition_count.increment();
    let _ = value;
}

#[composable]
fn WithDoubleParameter(value: f64, composition_count: Counter) {
    composition_count.increment();
    let _ = value;
}

/// The parent recomposes with an unchanged NaN argument: the child skips.
fn nan_argument_skips(child: fn(Counter)) {
    let mut test = composition_test();
    let trigger = mutable_state_of(false);
    let (parent_count, child_count) = (Counter::new(), Counter::new());
    let (t, p, c) = (trigger.handle(), parent_count.clone(), child_count.clone());
    test.compose(move || {
        t.get();
        p.increment();
        child(c.clone());
    });
    assert_eq!((parent_count.get(), child_count.get()), (1, 1));
    trigger.set(true);
    test.advance();
    assert_eq!((parent_count.get(), child_count.get()), (2, 1), "the child skipped");
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/JvmCompositionTests.kt, floatNaNArgumentSkips
#[test]
fn a_float_nan_argument_skips() {
    nan_argument_skips(|count| WithFloatParameter(std::hint::black_box(f32::NAN), count));
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/JvmCompositionTests.kt, doubleNaNArgumentSkips
#[test]
fn a_double_nan_argument_skips() {
    nan_argument_skips(|count| WithDoubleParameter(std::hint::black_box(f64::NAN), count));
}
