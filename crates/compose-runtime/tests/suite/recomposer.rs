//! The plain recomposition scenarios of Compose's `RecomposerTests` (common, jvm and integration)
//! and `ComposeIntoTests`, on `Composition::recompose`: compose-rs has no `Recomposer` (the
//! recomposer feature), and the snapshot, thread and Android-view tests are n/a.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use compose_runtime::platform::{NodeTree, live_scope_count};
use compose_runtime::*;

use crate::mock::*;

/// Counts per name, as `RecomposerTests`' `Counter`.
#[derive(Clone, Default)]
struct Counts(Rc<RefCell<HashMap<String, u32>>>);

impl ComposeArg for Counts {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Counts {
    fn increment(&self, name: &str) {
        *self.0.borrow_mut().entry(name.to_string()).or_default() += 1;
    }

    fn get(&self, name: &str) -> u32 {
        self.0.borrow().get(name).copied().unwrap_or(0)
    }
}

/// States composables read to be recomposed on demand, by id, as the `Trigger` map.
#[derive(Clone)]
struct Triggers(Rc<HashMap<i32, OwnedState<i32>>>);

impl ComposeArg for Triggers {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Triggers {
    fn subscribe(&self, id: i32) {
        if let Some(trigger) = self.0.get(&id) {
            trigger.get();
        }
    }

    fn recompose(&self, id: i32) {
        self.0[&id].update(|count| count + 1);
    }
}

// `@NonSkippableComposable` in Compose: a `NeverEqual` argument keeps them from skipping.
#[composable]
fn RecomposeTestComponentsA(counts: Counts, triggers: Triggers, always: NeverEqual) {
    counts.increment("A");
    triggers.subscribe(99);
    // Captured, so the content isn't memoized either (D710).
    Linear!({
        for id in 100..=102 {
            key(id, || RecomposeTestComponentsB(counts.clone(), triggers.clone(), id, always));
        }
    });
}

#[composable]
fn RecomposeTestComponentsB(counts: Counts, triggers: Triggers, id: i32, _always: NeverEqual) {
    counts.increment(&id.to_string());
    triggers.subscribe(id);
    Text(&id.to_string());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, testRecomposition
#[test]
fn invalidated_scopes_recompose_on_the_next_frame_and_once_each() {
    let mut test = composition_test();
    let counts = Counts::default();
    let triggers =
        Triggers(Rc::new([99, 100, 102].into_iter().map(|id| (id, mutable_state_of(0))).collect()));
    let (c, t) = (counts.clone(), triggers.clone());
    test.compose(move || RecomposeTestComponentsA(c.clone(), t.clone(), NeverEqual));
    let all = |counts: &Counts| ["A", "100", "101", "102"].map(|name| counts.get(name));
    assert_eq!(all(&counts), [1, 1, 1, 1]);

    triggers.recompose(100);
    triggers.recompose(102);
    // Nothing happens synchronously.
    assert_eq!(all(&counts), [1, 1, 1, 1]);
    test.expect_no_changes();
    assert_eq!(all(&counts), [1, 2, 1, 2]);

    // The parent and a child: the child recomposes once. The `NeverEqual` arguments are kept
    // (Compose's `changed` writes them too), so only the views are unchanged.
    triggers.recompose(99);
    triggers.recompose(102);
    let views = test.with_views(|views| views.changes);
    test.advance();
    assert_eq!(test.with_views(|views| views.changes), views, "the views are unchanged");
    assert_eq!(all(&counts), [2, 3, 2, 3]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, testInsertDuringRecomposition
// (in the composition itself; composition_context.rs has it in a child)
#[test]
fn inserting_content_that_writes_state_while_composing() {
    #[composable]
    fn B(value: i32) {
        let _ = value;
    }
    #[composable]
    fn A(some_state: MutableState<i32>) {
        B(some_state.get());
        some_state.update(|state| state + 1);
    }
    #[composable]
    fn T(
        include_a: MutableState<bool>,
        some_state: MutableState<i32>,
        some_other_state: MutableState<i32>,
    ) {
        // Takes up some slot space (bug 157111271).
        remember_keyed((1, 2, 3, 4, 5, 6, 7, 8), || 1);
        if include_a.get() {
            Wrap!({
                B(0);
                B(some_other_state.get());
                B(2);
                B(3);
                B(4);
                A(some_state);
            });
        }
    }
    let mut test = composition_test();
    let (include_a, some_state, some_other_state) =
        (mutable_state_of(false), mutable_state_of(0), mutable_state_of(1));
    let (i, s, o) = (include_a.handle(), some_state.handle(), some_other_state.handle());
    test.compose(move || T(i, s, o));
    include_a.set(true);
    test.advance_ignoring_pending_work();
    some_other_state.set(10);
    test.advance_ignoring_pending_work();
    test.advance_ignoring_pending_work();
    assert!(some_state.get() > 0);
    test.verify_consistent();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, testMultipleRecompose
#[test]
fn recomposing_a_skipped_composable_with_a_default_argument() {
    #[derive(Clone)]
    struct Model;
    impl ComposeArg for Model {
        fn same(&self, _: &Self) -> bool {
            false
        }
    }
    #[composable]
    fn Validate(model: Option<Model>) {
        assert!(model.is_some());
    }
    #[composable]
    fn Use(value: i32) {
        let _ = value;
    }
    // `a: A = A()`: the caller evaluates the default (parity behaviour 4).
    #[composable]
    fn UseA(model: Model, state2: MutableState<i32>) {
        Validate(Some(model));
        Use(state2.get());
    }
    #[composable]
    fn Test(state1: MutableState<i32>, state2: MutableState<i32>) {
        Use(state1.get());
        UseA(Model, state2);
    }
    let mut test = composition_test();
    let (state1, state2) = (mutable_state_of(1), mutable_state_of(1));
    let (one, two) = (state1.handle(), state2.handle());
    test.compose(move || Test(one, two));
    state1.set(2);
    test.advance();
    state2.set(2);
    test.advance();
    test.advance();
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/RecomposerTests.jvm.kt, disposedInvalidatedCompositionDoesNotLeak
#[test]
fn a_disposed_invalidated_composition_does_not_leak() {
    let state = mutable_state_of(0);
    let scopes = live_scope_count();
    let frame_requested = Rc::new(Cell::new(false));
    let marker = Rc::new(());
    let weak: Weak<()> = Rc::downgrade(&marker);
    let composition = Composition::new(NodeTree::<()>::new());
    let requested = frame_requested.clone();
    composition.set_frame_requester(move || requested.set(true));
    let s = state.handle();
    composition.set_content(move || {
        let _ = &marker;
        // Invalidates the composition.
        s.get();
    });
    state.set(1);
    compose_runtime::snapshots::Snapshot::send_apply_notifications();
    assert!(frame_requested.get(), "never requested a frame");
    assert!(composition.has_invalidations());
    assert!(weak.upgrade().is_some(), "composition prematurely dropped");
    drop(composition);
    assert!(weak.upgrade().is_none(), "the content was not dropped with the composition");
    assert_eq!(live_scope_count(), scopes, "the composition's scopes were not freed");
    state.set(2);
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, testSlotKeyChangeCausesRecreate
#[test]
fn a_key_change_recreates_the_node() {
    let mut test = composition_test();
    let (trigger, released) = (mutable_state_of(0), Counter::new());
    let i = Rc::new(Cell::new(1));
    let (t, r, count) = (trigger.handle(), released.clone(), i.clone());
    test.compose(move || {
        t.get();
        // Recreates the view on every composition.
        count.set(count.get() + 1);
        let r = r.clone();
        key(count.get(), || {
            LinearWithCallbacks(
                nothing(),
                nothing(),
                Rc::new(move || {
                    r.increment();
                }),
                nothing(),
                || {},
            );
        });
    });
    trigger.set(1);
    test.advance();
    assert_eq!(i.get(), 3, "composed twice");
    assert_eq!(released.get(), 1, "the first view was released");
    test.validate(|v| v.empty_linear());
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/ComposeIntoTests.kt, testMultipleSetContentCalls
#[test]
fn setting_the_same_content_again_keeps_its_state() {
    let initialization_count = Rc::new(Cell::new(0));
    let count = initialization_count.clone();
    let content = Rc::new(move || {
        let count = count.clone();
        DisposableEffect((), move |scope| {
            count.set(count.get() + 1);
            scope.on_dispose(|| {})
        });
    });
    let composition = Composition::new(NodeTree::<()>::new());
    let c = content.clone();
    composition.set_content(move || c());
    assert_eq!(initialization_count.get(), 1);
    composition.set_content(move || content());
    assert_eq!(initialization_count.get(), 1, "the hierarchy was kept");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/ComposeIntoTests.kt, testCompositionCanBeCollectedWithPendingInvalidate
#[test]
fn a_composition_is_dropped_while_a_scope_of_it_is_held() {
    let scopes = live_scope_count();
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let scope = Rc::new(Cell::new(None));
    let composition = Composition::new(NodeTree::<()>::new());
    let s = scope.clone();
    composition.set_content(move || {
        let _ = &marker;
        s.set(Some(current_recompose_scope()));
    });
    let held = scope.take().expect("no scope captured");
    drop(composition);
    assert!(weak.upgrade().is_none(), "the composition leaked");
    held.invalidate();
    assert_eq!(live_scope_count(), scopes);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, readDuringWithoutReadObservationDoesntCauseRecomposition
#[test]
fn a_read_without_observation_recomposes_nothing() {
    #[composable]
    fn Use(value: i32) {
        let _ = value;
    }
    let mut test = composition_test();
    let some_state = mutable_state_of(0);
    let recompositions = Rc::new(Cell::new(0));
    let (s, r) = (some_state.handle(), recompositions.clone());
    test.compose(move || {
        r.set(r.get() + 1);
        Use(compose_runtime::snapshots::Snapshot::without_read_observation(|| s.get()));
    });
    assert_eq!(recompositions.get(), 1);
    some_state.update(|value| value + 1);
    test.advance();
    assert_eq!(recompositions.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, stateChangesDuringApplyChangesAreNotifiedBeforeFrameFinished
#[test]
fn state_changes_during_apply_changes_are_notified_before_the_frame_finished() {
    use compose_runtime::snapshots::{Snapshot, StateObjectId};
    let count = mutable_state_of(0);
    let count_from_effect = mutable_state_of(0);
    let recompositions = Rc::new(Cell::new(0));
    let (read, written, counted) =
        (count.handle(), count_from_effect.handle(), recompositions.clone());
    let mut test = composition_test();
    test.compose(move || {
        counted.set(counted.get() + 1);
        let value = read.get();
        SideEffect(move || written.set(value));
    });
    assert_eq!(count_from_effect.get(), 0);
    assert_eq!(recompositions.get(), 1);
    count.set(1);
    let applications: Rc<RefCell<Vec<Vec<StateObjectId>>>> = Rc::default();
    let log = applications.clone();
    let _observer = Snapshot::register_apply_observer(move |applied, _| {
        log.borrow_mut().push(applied.iter().collect())
    });
    assert!(applications.borrow().is_empty());
    assert_eq!(test.advance_count(), 1);
    assert_eq!(recompositions.get(), 2);
    assert_eq!(*applications.borrow(), [vec![count_from_effect.state_object_id()]]);
}
