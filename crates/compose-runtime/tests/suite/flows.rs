//! Flows (docs/parity/design/snapshots.md, step S4), from Compose's `SnapshotFlowTests`,
//! `CollectAsStateTests` and `FlowAdapterTest`. Writes in `withMutableSnapshot` are plain writes
//! and sends until step S5; `Dispatchers.Unconfined` and the test dispatcher's `yield()` are
//! `run_ready`. A coroutine context has no counterpart (`observingOnCustomContext`,
//! `theCurrentValueIsNotLostWhenWeReplacedContext`, n/a).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use compose_runtime::flow::SnapshotFlowManager;
use compose_runtime::flow::{MutableStateFlow, Stream, next};
use compose_runtime::platform::run_ready;
use compose_runtime::snapshots::Snapshot;
use compose_runtime::*;

use crate::mock::*;

/// What two collectors saw.
type TwoResults<T> = (Rc<RefCell<Vec<T>>>, Rc<RefCell<Vec<T>>>);

/// Collects `stream` in a task, calling `on_each` with each value: `.onEach { … }.launchIn(scope)`.
fn collect<T: 'static, S: Stream<Item = T> + Unpin + 'static>(
    scope: CoroutineScope,
    mut stream: S,
    mut on_each: impl FnMut(T) + 'static,
) -> Job {
    scope.launch(async move {
        while let Some(value) = next(&mut stream).await {
            on_each(value);
        }
    })
}

fn sending(block: impl FnOnce()) {
    block();
    Snapshot::send_apply_notifications();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, observeBasicChanges
#[test]
fn a_snapshot_flow_gives_its_value_and_each_change() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let result = Rc::new(Cell::new(0));
    let seen = result.clone();
    let scope = CoroutineScope::new();
    let collector =
        collect(*scope, snapshot_flow(move || handle.get() * 2), move |value| seen.set(value));
    run_ready();
    assert_eq!(result.get(), 2, "value after initial run");
    Snapshot::notify_objects_initialized();
    sending(|| state.set(5));
    run_ready();
    assert_eq!(result.get(), 10, "value after an update");
    collector.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, coalesceChanges
#[test]
fn a_snapshot_flow_coalesces_changes_between_its_polls() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let scope = CoroutineScope::new();
    let collector = collect(*scope, snapshot_flow(move || handle.get()), move |_| {
        counted.set(counted.get() + 1)
    });
    assert_eq!(runs.get(), 0, "the collector hasn't run yet");
    run_ready();
    assert_eq!(runs.get(), 1, "its initial run");
    Snapshot::notify_objects_initialized();
    sending(|| state.update(|value| value + 1));
    run_ready();
    assert_eq!(runs.get(), 2, "made one change");
    sending(|| state.update(|value| value + 1));
    sending(|| state.update(|value| value + 1));
    run_ready();
    assert_eq!(runs.get(), 3, "coalesced two changes");
    collector.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, ignoreUnrelatedChanges
#[test]
fn a_snapshot_flow_ignores_unrelated_changes() {
    let (state, unrelated) = (mutable_state_of(1), mutable_state_of(1));
    let handle = state.handle();
    let calculations = Rc::new(Cell::new(0));
    let counted = calculations.clone();
    let runs = Rc::new(Cell::new(0));
    let collected = runs.clone();
    let scope = CoroutineScope::new();
    let flow = snapshot_flow(move || {
        counted.set(counted.get() + 1);
        handle.get()
    });
    let collector = collect(*scope, flow, move |_| collected.set(collected.get() + 1));
    run_ready();
    assert_eq!(runs.get(), 1, "initial run");
    Snapshot::notify_objects_initialized();
    sending(|| unrelated.update(|value| value + 1));
    run_ready();
    assert_eq!(runs.get(), 1, "after changing unrelated state");
    assert_eq!(calculations.get(), 1, "its block didn't run again");
    collector.cancel();
}

/// A change leaving the block's result as it was yields nothing, as `newValue != lastValue`.
#[test]
fn a_snapshot_flow_yields_only_a_different_value() {
    let state = mutable_state_of(1);
    let handle = state.handle();
    let results: Rc<RefCell<Vec<bool>>> = Rc::default();
    let log = results.clone();
    let scope = CoroutineScope::new();
    let collector = collect(*scope, snapshot_flow(move || handle.get() > 0), move |value| {
        log.borrow_mut().push(value)
    });
    run_ready();
    Snapshot::notify_objects_initialized();
    sending(|| state.set(2));
    run_ready();
    sending(|| state.set(0));
    run_ready();
    assert_eq!(*results.borrow(), [true, false]);
    collector.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, nestedDerivedStateWorks
#[test]
fn a_snapshot_flow_follows_nested_derived_states() {
    let truth = mutable_state_of(true);
    let handle = truth.handle();
    let first = derived_state_of(move || handle.get());
    let read_first = first.as_state();
    let second = derived_state_of(move || read_first.get());
    let read = second.as_state();
    let results: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let log = results.clone();
    let collector1 =
        collect(*scope, snapshot_flow(move || read.get()), move |_| log.borrow_mut().push(1));
    let log = results.clone();
    let collector2 =
        collect(*scope, snapshot_flow(move || read.get()), move |_| log.borrow_mut().push(2));
    run_ready();
    Snapshot::notify_objects_initialized();
    sending(|| truth.set(false));
    run_ready();
    assert_eq!(*results.borrow(), [1, 2, 1, 2]);
    collector1.cancel();
    collector2.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, sharingOneSnapshotFlowManager_watchingSameStateObject
#[test]
fn flows_sharing_a_manager_watch_one_state() {
    let state = mutable_state_of(false);
    let handle = state.handle();
    let manager = SnapshotFlowManager::new();
    let scope = CoroutineScope::new();
    let (result1, result2): TwoResults<bool> = Default::default();
    let log = result1.clone();
    let collector1 =
        collect(*scope, snapshot_flow_with(&manager, move || handle.get()), move |v| {
            log.borrow_mut().push(v)
        });
    let log = result2.clone();
    let collector2 =
        collect(*scope, snapshot_flow_with(&manager, move || handle.get()), move |v| {
            log.borrow_mut().push(v)
        });
    run_ready();
    Snapshot::notify_objects_initialized();
    sending(|| state.set(true));
    run_ready();
    assert_eq!(*result1.borrow(), [false, true]);
    assert_eq!(*result2.borrow(), [false, true]);
    collector1.cancel();
    collector2.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, sharingOneSnapshotFlowManager_watchingDifferentStateObjects
#[test]
fn flows_sharing_a_manager_watch_different_states() {
    let (state1, state2) = (mutable_state_of(0), mutable_state_of(0));
    let (handle1, handle2) = (state1.handle(), state2.handle());
    let manager = SnapshotFlowManager::new();
    let scope = CoroutineScope::new();
    let (result1, result2): TwoResults<i32> = Default::default();
    let log = result1.clone();
    let collector1 =
        collect(*scope, snapshot_flow_with(&manager, move || handle1.get()), move |v| {
            log.borrow_mut().push(v)
        });
    let log = result2.clone();
    let collector2 =
        collect(*scope, snapshot_flow_with(&manager, move || handle2.get()), move |v| {
            log.borrow_mut().push(v)
        });
    run_ready();
    Snapshot::notify_objects_initialized();
    sending(|| state1.update(|value| value + 1));
    run_ready();
    sending(|| state2.update(|value| value + 1));
    run_ready();
    sending(|| {
        state1.update(|value| value + 1);
        state2.update(|value| value + 1);
    });
    run_ready();
    assert_eq!(*result1.borrow(), [0, 1, 2]);
    assert_eq!(*result2.borrow(), [0, 1, 2]);
    collector1.cancel();
    collector2.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, snapshotFlowManagerApplyObserverCorruption
#[test]
fn a_collector_writing_and_sending_while_a_manager_signals_is_fine() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let manager = SnapshotFlowManager::new();
    let scope = CoroutineScope::new();
    let collector1 = collect(*scope, snapshot_flow_with(&manager, move || handle.get()), |_| {});
    let done = Rc::new(Cell::new(false));
    let finished = done.clone();
    let collector2 =
        collect(*scope, snapshot_flow_with(&manager, move || handle.get()), move |v| {
            if v == 1 {
                handle.set(2);
                Snapshot::send_apply_notifications();
            }
            if v == 2 {
                finished.set(true);
            }
        });
    run_ready();
    Snapshot::notify_objects_initialized();
    sending(|| state.set(1));
    run_ready();
    run_ready();
    assert!(done.get());
    collector1.cancel();
    collector2.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, cancelSoleManagedSnapshotFlowThenReuseManager
#[test]
fn a_manager_is_reused_after_its_only_flow_is_cancelled() {
    let state = mutable_state_of(false);
    let handle = state.handle();
    let manager = SnapshotFlowManager::new();
    let scope = CoroutineScope::new();
    let results: Rc<RefCell<Vec<bool>>> = Rc::default();
    let log = results.clone();
    let collector1 =
        collect(*scope, snapshot_flow_with(&manager, move || handle.get()), move |v| {
            log.borrow_mut().push(v)
        });
    run_ready();
    Snapshot::notify_objects_initialized();
    sending(|| state.set(true));
    run_ready();
    collector1.cancel();
    let log = results.clone();
    let collector2 =
        collect(*scope, snapshot_flow_with(&manager, move || handle.get()), move |v| {
            log.borrow_mut().push(v)
        });
    let log = results.clone();
    let collector3 =
        collect(*scope, snapshot_flow_with(&manager, move || handle.get()), move |v| {
            log.borrow_mut().push(v)
        });
    run_ready();
    sending(|| state.set(false));
    run_ready();
    assert_eq!(*results.borrow(), [false, true, true, true, false, false]);
    collector2.cancel();
    collector3.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/SnapshotFlowTests.kt, twoSnapshotFlowsWithDistinctManagers
#[test]
fn flows_with_their_own_managers_watch_one_state() {
    let state = mutable_state_of(false);
    let handle = state.handle();
    let scope = CoroutineScope::new();
    let (result1, result2): TwoResults<bool> = Default::default();
    let log = result1.clone();
    let collector1 =
        collect(*scope, snapshot_flow(move || handle.get()), move |v| log.borrow_mut().push(v));
    let log = result2.clone();
    let collector2 =
        collect(*scope, snapshot_flow(move || handle.get()), move |v| log.borrow_mut().push(v));
    run_ready();
    Snapshot::notify_objects_initialized();
    sending(|| state.set(true));
    run_ready();
    assert_eq!(*result1.borrow(), [false, true]);
    assert_eq!(*result2.borrow(), [false, true]);
    collector1.cancel();
    collector2.cancel();
}

/// A flow's drop unregisters what it watched: a later change signals nothing, and a manager's
/// `dispose` stops it hearing changes at all.
#[test]
fn a_dropped_flow_and_a_disposed_manager_watch_nothing() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let manager = SnapshotFlowManager::new();
    let scope = CoroutineScope::new();
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let flow = snapshot_flow_with(&manager, move || {
        counted.set(counted.get() + 1);
        handle.get()
    });
    let collector = collect(*scope, flow, |_| {});
    run_ready();
    assert_eq!(manager.watched_states(), 1);
    collector.cancel();
    assert_eq!(manager.watched_states(), 0);
    Snapshot::notify_objects_initialized();
    sending(|| state.set(1));
    run_ready();
    assert_eq!(runs.get(), 1);
    manager.dispose();
}

// ---- collect_as_state ---------------------------------------------------------------------------

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CollectAsStateTests.kt, stateFlow_collectAsState
#[test]
fn a_state_flow_collected_as_state_follows_it() {
    let flow = MutableStateFlow::new(String::from("initial"));
    let collected = flow.as_state_flow();
    let mut test = composition_test();
    test.compose(move || Text(&collected.collect_as_state().get()));
    test.validate(|v| v.text("initial"));
    flow.set_value(String::from("updated"));
    test.expect_changes();
    test.validate(|v| v.text("updated"));
}

/// A stream giving one value, as `flow { emit(value) }`.
struct Once<T>(Option<T>, Rc<Cell<bool>>);

impl<T: Unpin> Stream for Once<T> {
    type Item = T;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<T>> {
        self.1.set(true);
        Poll::Ready(self.0.take())
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CollectAsStateTests.kt, flow_collectAsState
#[test]
fn a_stream_collected_as_state_starts_at_its_initial_value() {
    let mut test = composition_test();
    test.compose(|| {
        let state = collect_as_state((), String::from("initial"), || {
            Once(Some(String::from("updated")), Rc::default())
        });
        Text(&state.get());
    });
    test.validate(|v| v.text("initial"));
    test.expect_changes();
    test.validate(|v| v.text("updated"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CollectAsStateTests.kt, flow_collectAsState_restartsOnFlowChange
#[test]
fn a_stream_collected_as_state_restarts_when_its_key_changes() {
    let (emitted1, emitted2): (Rc<Cell<bool>>, Rc<Cell<bool>>) = Default::default();
    let current = mutable_state_of(1);
    let handle = current.handle();
    let (first, second) = (emitted1.clone(), emitted2.clone());
    let mut test = composition_test();
    test.compose(move || {
        let which = handle.get();
        let (first, second) = (first.clone(), second.clone());
        let state = collect_as_state(which, String::from("initial"), move || {
            if which == 1 {
                Once(Some(String::from("flow 1")), first)
            } else {
                Once(Some(String::from("flow 2")), second)
            }
        });
        Text(&state.get());
    });
    test.expect_changes();
    test.validate(|v| v.text("flow 1"));
    assert!(emitted1.get() && !emitted2.get());
    current.set(2);
    test.expect_changes();
    test.advance();
    test.validate(|v| v.text("flow 2"));
    assert!(emitted2.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CollectAsStateTests.kt, collectAsState_mutationPolicy_referential
#[test]
fn a_state_flow_collected_with_a_policy_uses_it() {
    let flow = MutableStateFlow::new(Rc::new(vec!["Alice", "Bob"]));
    let collected = flow.as_state_flow();
    let recompositions = Rc::new(Cell::new(0));
    let counted = recompositions.clone();
    let mut test = composition_test();
    test.compose(move || {
        counted.set(counted.get() + 1);
        Text(&collected.collect_as_state_with(ReferentialEqualityPolicy).get().join(","));
    });
    assert_eq!(recompositions.get(), 1);
    test.advance();
    assert_eq!(recompositions.get(), 1, "the same value, by its policy");
    // Equal, as a `StateFlow` compares: nothing changes.
    flow.set_value(Rc::new(vec!["Alice", "Bob"]));
    test.expect_no_changes();
    assert_eq!(recompositions.get(), 1);
    flow.set_value(Rc::new(vec!["Carol"]));
    test.expect_changes();
    assert_eq!(recompositions.get(), 2);
}

/// A stream values are handed to, as `MutableSharedFlow(extraBufferCapacity = 1)`.
#[derive(Clone, Default)]
struct Shared<T>(Rc<RefCell<(VecDeque<T>, Option<Waker>)>>);

impl<T> Shared<T> {
    fn try_emit(&self, value: T) {
        let waker = {
            let mut inner = self.0.borrow_mut();
            inner.0.push_back(value);
            inner.1.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn subscribers(&self) -> usize {
        Rc::strong_count(&self.0) - 1
    }
}

struct Subscription<T>(Shared<T>);

impl<T> Stream for Subscription<T> {
    type Item = T;
    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        let mut inner = (self.0).0.borrow_mut();
        match inner.0.pop_front() {
            Some(value) => Poll::Ready(Some(value)),
            None => {
                inner.1 = Some(context.waker().clone());
                Poll::Pending
            }
        }
    }
}

#[composable]
fn Collected(stream: Shared<&'static str>, key: i32, initial: &'static str) {
    let state = collect_as_state(key, Some(initial), move || {
        struct Mapped(Subscription<&'static str>);
        impl Stream for Mapped {
            type Item = Option<&'static str>;
            fn poll_next(
                mut self: Pin<&mut Self>,
                context: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                Pin::new(&mut self.0).poll_next(context).map(|value| value.map(Some))
            }
        }
        Mapped(Subscription(stream))
    });
    Text(state.get().unwrap_or("null"));
}

impl compose_runtime::ComposeArg for Shared<&'static str> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, weReceiveSubmittedValue
// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, weReceiveSecondValue
// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, testCollectionWithInitialValue
// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, testOverridingInitialValue
#[test]
fn a_collected_stream_starts_at_its_initial_value_and_follows() {
    let stream: Shared<&'static str> = Shared::default();
    let given = stream.clone();
    let mut test = composition_test();
    test.compose(move || Collected(given.clone(), 0, "value"));
    test.validate(|v| v.text("value"));
    test.advance();
    stream.try_emit("value2");
    test.expect_changes();
    test.validate(|v| v.text("value2"));
    stream.try_emit("value3");
    test.expect_changes();
    test.validate(|v| v.text("value3"));
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, noUpdatesAfterDispose
#[test]
fn a_collected_stream_is_dropped_with_its_composable() {
    let stream: Shared<&'static str> = Shared::default();
    let given = stream.clone();
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut test = composition_test();
    test.compose(move || {
        if handle.get() {
            Collected(given.clone(), 0, "value");
        }
    });
    test.advance();
    let subscribed = stream.subscribers();
    shown.set(false);
    test.expect_changes();
    test.validate(|_| {});
    test.advance();
    assert!(stream.subscribers() < subscribed, "the task and its stream are gone");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, theCurrentValueIsNotLostWhenWeUpdatedInitial
#[test]
fn a_new_initial_value_doesnt_replace_the_current_one() {
    let stream: Shared<&'static str> = Shared::default();
    let given = stream.clone();
    let initial = mutable_state_of("initial1");
    let handle = initial.handle();
    let mut test = composition_test();
    test.compose(move || Collected(given.clone(), 0, handle.get()));
    test.advance();
    stream.try_emit("value");
    test.expect_changes();
    initial.set("initial2");
    test.advance();
    test.validate(|v| v.text("value"));
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, replacingStreams
// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, theCurrentValueIsNotLostWhenWeReplacedStreams
#[test]
fn a_replaced_stream_takes_over_and_keeps_the_current_value() {
    let (stream1, stream2): (Shared<&'static str>, Shared<&'static str>) = Default::default();
    let (given1, given2) = (stream1.clone(), stream2.clone());
    let which = mutable_state_of(1);
    let handle = which.handle();
    let mut test = composition_test();
    test.compose(move || {
        let key = handle.get();
        Collected(if key == 1 { given1.clone() } else { given2.clone() }, key, "null");
    });
    test.advance();
    stream1.try_emit("value");
    test.expect_changes();
    which.set(2);
    test.advance();
    test.validate(|v| v.text("value"));
    test.advance();
    stream2.try_emit("stream2");
    test.expect_changes();
    test.validate(|v| v.text("stream2"));
    stream1.try_emit("old stream");
    test.advance();
    test.validate(|v| v.text("stream2"));
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, testInitialValueOfStateFlow
// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, updatingValueOfStateFlow
#[test]
fn a_state_flow_collected_as_state_starts_at_its_value_and_follows() {
    let flow = MutableStateFlow::new("value");
    let collected = flow.as_state_flow();
    let mut test = composition_test();
    test.compose(move || Text(collected.collect_as_state().get()));
    test.validate(|v| v.text("value"));
    test.advance();
    flow.set_value("updated");
    test.expect_changes();
    test.validate(|v| v.text("updated"));
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, stateFlowHandlesNullValue
#[test]
fn a_state_flow_of_none_values_works() {
    let flow = MutableStateFlow::new(Some("value"));
    let collected = flow.as_state_flow();
    let mut test = composition_test();
    test.compose(move || Text(collected.collect_as_state().get().unwrap_or("null")));
    test.validate(|v| v.text("value"));
    test.advance();
    flow.set_value(None);
    test.expect_changes();
    test.validate(|v| v.text("null"));
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/FlowAdapterTest.kt, updatingTheInstanceOfAStateFlow
#[test]
fn a_replaced_state_flow_takes_over() {
    let (flow1, flow2) = (MutableStateFlow::new("one"), MutableStateFlow::new("two"));
    let (given1, given2) = (flow1.as_state_flow(), flow2.as_state_flow());
    let which = mutable_state_of(1);
    let handle = which.handle();
    let mut test = composition_test();
    test.compose(move || {
        let flow = if handle.get() == 1 { &given1 } else { &given2 };
        Text(flow.collect_as_state().get());
    });
    test.validate(|v| v.text("one"));
    which.set(2);
    test.expect_changes();
    test.advance();
    test.validate(|v| v.text("two"));
}

/// A `StateFlow` is set from any thread, and wakes who collects it on theirs.
#[test]
fn a_state_flow_is_set_from_another_thread() {
    let flow = MutableStateFlow::new(0);
    let setter = flow.clone();
    let collected = flow.as_state_flow();
    let mut test = composition_test();
    test.compose(move || Text(&collected.collect_as_state().get().to_string()));
    test.advance();
    std::thread::spawn(move || setter.set_value(1)).join().expect("the thread");
    test.expect_changes();
    assert_eq!(flow.value(), 1);
    assert!(flow.compare_and_set(1, 2));
    assert!(!flow.compare_and_set(1, 3));
    assert_eq!(flow.value(), 2);
}

/// A snapshot flow's block runs in a read-only snapshot (D775): writing there refuses, as Compose's.
#[test]
fn a_snapshot_flows_block_is_in_a_read_only_snapshot() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let mut flow = snapshot_flow(move || {
        assert!(Snapshot::current().read_only());
        handle.get()
    });
    let values = collect_now(&mut flow);
    assert_eq!(values, [0]);
    Snapshot::with_mutable_snapshot(|| state.set(1));
    assert_eq!(collect_now(&mut flow), [1]);
}

/// What's polled now of a flow.
fn collect_now<S: Stream + Unpin>(flow: &mut S) -> Vec<S::Item> {
    let mut values = Vec::new();
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    while let std::task::Poll::Ready(Some(value)) =
        std::pin::Pin::new(&mut *flow).poll_next(&mut context)
    {
        values.push(value);
    }
    values
}

/// As a `StateFlow`'s collector, a subscription compares values: changed and changed back between
/// its polls, it gives nothing.
#[test]
fn a_state_flow_subscription_compares_values() {
    let flow = MutableStateFlow::new("A");
    let mut subscription = flow.subscribe();
    assert_eq!(collect_now(&mut subscription), ["A"]);
    flow.set_value("B");
    flow.set_value("A");
    assert_eq!(collect_now(&mut subscription), Vec::<&str>::new());
    flow.set_value("B");
    assert_eq!(collect_now(&mut subscription), ["B"]);
}

/// A disposed manager stays so: a flow's first run with it panics, as `runAndWatch`.
#[test]
fn a_disposed_manager_stays_disposed() {
    let manager = SnapshotFlowManager::new();
    manager.dispose();
    let mut flow = snapshot_flow_with(&manager, || 1);
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| collect_now(&mut flow)));
    let message = run.expect_err("disposed");
    assert!(message.downcast_ref::<&str>().is_some_and(|message| message.contains("disposed")));
}

/// Collecting a state flow looks at its value when the state is made, not at each recomposition.
#[test]
fn a_state_flow_collected_as_state_clones_its_value_once() {
    #[derive(PartialEq)]
    struct Counted(Rc<Cell<u32>>);
    // A flow's value crosses threads by `T: Send`, which a flow on one thread doesn't ask.
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Counted(self.0.clone())
        }
    }
    let clones = Rc::new(Cell::new(0));
    let flow = MutableStateFlow::new(Counted(clones.clone()));
    let collected = flow.as_state_flow();
    let trigger = mutable_state_of(0);
    let read = trigger.handle();
    let mut test = composition_test();
    test.compose(move || {
        let _ = collected.collect_as_state();
        Text(&read.get().to_string());
    });
    test.advance();
    let at_first = clones.get();
    trigger.set(1);
    test.expect_changes();
    assert_eq!(clones.get(), at_first, "a recomposition looks at nothing");
}
