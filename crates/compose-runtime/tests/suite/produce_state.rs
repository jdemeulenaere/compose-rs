//! Compose's `ProduceStateTests`, adapted: `produce_state`'s initial value, updates, restarts
//! on a key change, cancellation, `await_dispose` and a mutation policy.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_runtime::*;

use crate::mock::*;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt, produceState_initialValue
#[test]
fn produced_state_starts_at_its_initial_value() {
    #[composable]
    fn Content() {
        let state = produce_state(String::from("initial"), (), |_| async {});
        Text(&state.get());
    }
    let mut test = composition_test();
    test.compose(Content);
    test.validate(|v| v.text("initial"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt, produceState_updates
#[test]
fn produced_state_follows_its_producer() {
    #[composable]
    fn Content() {
        let state = produce_state(String::from("initial"), (), |scope| async move {
            delay(Duration::from_millis(99)).await;
            scope.set("updated".into());
        });
        Text(&state.get());
    }
    let mut test = composition_test();
    test.compose(Content);
    test.validate(|v| v.text("initial"));
    test.advance_time_by(Duration::from_millis(100));
    test.expect_changes();
    test.validate(|v| v.text("updated"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt, produceState_restartsOnKeyChange
#[test]
fn a_producer_restarts_when_its_key_changes() {
    #[composable]
    fn Content(key: MutableState<i32>, starts: Counter) {
        let current = key.get();
        let state = produce_state(String::from("initial"), current, move |scope| {
            starts.increment();
            async move { scope.set(format!("key {current}")) }
        });
        Text(&state.get());
    }
    let mut test = composition_test();
    let key = mutable_state_of(0);
    let starts = Counter::new();
    let (k, s) = (key.handle(), starts.clone());
    test.compose(move || Content(k, s.clone()));
    assert_eq!(starts.get(), 0, "the producer runs once its coroutine starts");
    test.advance_time_by(Duration::from_millis(100));
    test.expect_changes();
    test.validate(|v| v.text("key 0"));
    assert_eq!(starts.get(), 1);
    key.set(1);
    // Compose sees the restarted producer's write at the next `advance` (its test scheduler
    // sends no apply notifications); writes here are seen at once (D16): the same `advance`
    // shows it.
    test.expect_changes();
    test.validate(|v| v.text("key 1"));
    assert_eq!(starts.get(), 2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt, produceState_cancelledOnDisposal
#[test]
fn a_producer_is_cancelled_when_its_call_leaves() {
    struct SetOnDrop(Rc<Cell<bool>>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    #[composable]
    fn Content(show: MutableState<bool>, cancelled: Rc<Cell<bool>>) {
        if show.get() {
            produce_state(String::from("initial"), (), move |scope| async move {
                // Kotlin's `try { … } finally { cancelled = true }`.
                let _finally = SetOnDrop(cancelled);
                scope.await_dispose(|| {}).await;
            });
            Text("shown");
        }
    }
    let mut test = composition_test();
    let show = mutable_state_of(true);
    let cancelled = Rc::new(Cell::new(false));
    let (s, c) = (show.handle(), cancelled.clone());
    test.compose(move || Content(s, c.clone()));
    test.advance();
    assert!(!cancelled.get());
    show.set(false);
    test.expect_changes();
    assert!(cancelled.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt, produceState_awaitDispose
#[test]
fn await_dispose_runs_when_the_producer_is_cancelled() {
    #[composable]
    fn Content(show: MutableState<bool>, disposed: Rc<Cell<bool>>) {
        if show.get() {
            produce_state(String::from("initial"), (), move |scope| async move {
                scope.await_dispose(move || disposed.set(true)).await;
            });
            Text("shown");
        }
    }
    let mut test = composition_test();
    let show = mutable_state_of(true);
    let disposed = Rc::new(Cell::new(false));
    let (s, d) = (show.handle(), disposed.clone());
    test.compose(move || Content(s, d.clone()));
    test.advance();
    assert!(!disposed.get());
    show.set(false);
    test.expect_changes();
    assert!(disposed.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/ProduceStateTests.kt, produceState_mutationPolicy_referential
#[test]
fn produced_state_takes_a_mutation_policy() {
    thread_local! {
        static PEOPLE: (Rc<Vec<&'static str>>, Rc<Vec<&'static str>>) =
            (Rc::new(vec!["Alice", "Bob"]), Rc::new(vec!["Alice", "Bob"]));
    }
    fn described(people: &Rc<Vec<&'static str>>) -> String {
        format!("{people:?}@{:x}", Rc::as_ptr(people) as usize)
    }
    #[composable]
    fn Content() {
        let (people, people2) = PEOPLE.with(Clone::clone);
        let state = produce_state_with(
            people,
            (),
            referential_equality_policy(),
            move |scope| async move {
                delay(Duration::from_millis(100)).await;
                scope.set(people2);
            },
        );
        Text(&described(&state.get()));
    }
    let mut test = composition_test();
    test.compose(Content);
    let (people, people2) = PEOPLE.with(Clone::clone);
    let expected = Rc::new(std::cell::RefCell::new(people));
    let shown = expected.clone();
    test.validate(move |v| v.text(&described(&shown.borrow())));
    test.advance_time_by(Duration::from_millis(100));
    *expected.borrow_mut() = people2;
    test.advance();
    test.revalidate();
}
