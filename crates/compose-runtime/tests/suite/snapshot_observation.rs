//! Snapshot observation (docs/parity/design/snapshots.md, step S1): the global snapshot's apply
//! notifications, apply and write observers, `Snapshot::observe`, `without_read_observation`
//! and `SnapshotStateObserver`, as Compose's `SnapshotTests` (their observer tests) and
//! `SnapshotStateObserverTestsCommon`/`Jvm` (those needing no isolated snapshot, derived or
//! computed state).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::snapshots::{self, Snapshot, SnapshotStateObserver, StateObjectId};
use compose_runtime::*;

/// How many applies changed `state` while `block` ran and was sent, as `changesOf`.
fn changes_of<T: 'static>(state: MutableState<T>, block: impl FnOnce()) -> usize {
    let changes = Rc::new(Cell::new(0));
    let counted = changes.clone();
    let id = state.state_object_id();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        if changed.contains(id) {
            counted.set(counted.get() + 1);
        }
    });
    block();
    Snapshot::send_apply_notifications();
    changes.get()
}

/// How many reads `block` made, as `readsOf` (observed without a snapshot: isolation comes
/// later).
fn reads_of(block: impl FnOnce()) -> usize {
    let reads = Rc::new(Cell::new(0));
    let counted = reads.clone();
    Snapshot::observe(Some(Rc::new(move |_| counted.set(counted.get() + 1))), None, block);
    reads.get()
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, globalChangesCanBeObserved
#[test]
fn global_changes_are_heard_at_the_send() {
    let state = mutable_state_of(0);
    Snapshot::notify_objects_initialized();
    let observed = Rc::new(Cell::new(false));
    let seen = observed.clone();
    let id = state.state_object_id();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        assert!(changed.contains(id));
        seen.set(true);
    });
    state.set(2);
    assert!(!observed.get(), "nothing observed yet");
    Snapshot::send_apply_notifications();
    assert!(observed.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, applyObserverNotificationIsPendingWhileSendingApplyNotifications
#[test]
fn notifications_are_pending_while_sent() {
    let state = mutable_state_of(0);
    let pending_while_observing = Rc::new(Cell::new(false));
    let seen = pending_while_observing.clone();
    let _observer = Snapshot::register_apply_observer(move |_, _| {
        seen.set(Snapshot::is_apply_observer_notification_pending());
    });
    assert!(!Snapshot::is_apply_observer_notification_pending());
    state.set(1);
    Snapshot::send_apply_notifications();
    assert!(pending_while_observing.get());
    assert!(!Snapshot::is_apply_observer_notification_pending());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, changingAnEqualityPolicyStateToItsCurrentValueIsNotConsideredAChange
#[test]
fn writing_an_equivalent_value_is_not_a_change() {
    let value = Rc::new(0);
    let state = mutable_state_of_with(value.clone(), referential_equality_policy());
    let handle = state.handle();
    assert_eq!(changes_of(handle, || handle.set(value)), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, changingANeverEqualPolicyStateToItsCurrentValueIsConsideredAChange
#[test]
fn writing_the_same_value_under_a_never_equal_policy_is_a_change() {
    let value = Rc::new(0);
    let state = mutable_state_of_with(value.clone(), never_equal_policy());
    let handle = state.handle();
    assert_eq!(changes_of(handle, || handle.set(value)), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, toStringOfMutableStateDoesNotTriggerReadObserver
#[test]
fn formatting_a_state_reads_nothing() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    assert_eq!(
        reads_of(|| {
            handle.get();
        }),
        1
    );
    assert_eq!(reads_of(|| drop(format!("{handle} {handle:?}"))), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, toStringValueOfMutableState
#[test]
fn a_state_displays_its_value() {
    let state = mutable_state_of(10);
    let id = format!("{:?}", state.state_object_id());
    assert!(format!("{}", *state).starts_with("MutableState(value=10)@"));
    state.set(20);
    assert!(format!("{}", *state).starts_with("MutableState(value=20)@"));
    assert_ne!(id, "");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, toStringOfDerivedStateDoesNotTriggerReadObservers
#[test]
fn formatting_a_derived_state_reads_nothing() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() + 1);
    assert_eq!(reads_of(|| drop(format!("{}", derived.as_state()))), 0);
    assert_eq!(derived.as_state().get(), 1);
    assert_eq!(reads_of(|| drop(format!("{}", derived.as_state()))), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, toStringValueOfDerivedState
#[test]
fn a_derived_state_displays_its_value_once_calculated() {
    let state = mutable_state_of(10);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() + 10);
    let shown = || format!("{}", derived.as_state());
    assert!(shown().starts_with("DerivedState(value=<Not calculated>)@"), "{}", shown());
    assert_eq!(derived.as_state().get(), 20);
    assert!(shown().starts_with("DerivedState(value=20)@"), "{}", shown());
    state.set(20);
    assert!(shown().starts_with("DerivedState(value=<Not calculated>)@"), "{}", shown());
    assert_eq!(derived.as_state().get(), 30);
    assert!(shown().starts_with("DerivedState(value=30)@"), "{}", shown());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, stateReadsCanBeObserved
// On `Snapshot::observe`: taking a snapshot with its read observer comes with step S5.
#[test]
fn a_read_observer_hears_the_state_read() {
    let state = mutable_state_of(0);
    let read: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
    let log = read.clone();
    let observer: compose_runtime::snapshots::StateObserverFn =
        Rc::new(move |id| log.borrow_mut().push(id));
    let result = Snapshot::observe(Some(observer), None, || state.get());
    assert_eq!(result, 0);
    assert_eq!(*read.borrow(), [state.state_object_id()]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotTests.kt, stateWritesCanBeObserved
// On `Snapshot::observe`: taking a mutable snapshot with its write observer comes with step S5.
#[test]
fn a_write_observer_hears_the_write_at_once() {
    let state = mutable_state_of(0);
    let written: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
    let log = written.clone();
    let observer: compose_runtime::snapshots::StateObserverFn =
        Rc::new(move |id| log.borrow_mut().push(id));
    Snapshot::observe(None, Some(observer), || {
        assert!(written.borrow().is_empty());
        state.set(2);
        assert_eq!(written.borrow().len(), 1);
    });
    assert_eq!(*written.borrow(), [state.state_object_id()]);
}

/// A write observer of `Snapshot::observe` hears the write at once; a global write observer
/// hears global writes at once too, before any send.
#[test]
fn write_observers_hear_writes_at_once() {
    let state = mutable_state_of(0);
    let written: Rc<RefCell<Vec<StateObjectId>>> = Rc::default();
    let global = Rc::new(Cell::new(0));
    let counted = global.clone();
    let _global = Snapshot::register_global_write_observer(move |_| counted.set(counted.get() + 1));
    let log = written.clone();
    Snapshot::observe(None, Some(Rc::new(move |id| log.borrow_mut().push(id))), || {
        state.set(2);
        assert_eq!(written.borrow().len(), 1);
    });
    assert_eq!(*written.borrow(), [state.state_object_id()]);
    assert_eq!(global.get(), 1);
}

// ---- SnapshotStateObserver ---------------------------------------------------------------

/// A started observer that runs its callbacks at once, `Snapshot.notifyObjectsInitialized()`,
/// `block`, a write to its state and a send, as `runSimpleTest`.
fn run_simple_test(block: impl FnOnce(&SnapshotStateObserver, MutableState<i32>)) {
    let observer = SnapshotStateObserver::new(|run| run());
    let state = mutable_state_of(0);
    observer.start();
    Snapshot::notify_objects_initialized();
    block(&observer, state.handle());
    state.update(|value| value + 1);
    Snapshot::send_apply_notifications();
    observer.stop();
}

/// A scope callback, as Compose's `(T) -> Unit`.
type Callback = Rc<dyn Fn(&&'static str)>;

fn counting(changes: &Rc<Cell<u32>>) -> Callback {
    let changes = changes.clone();
    Rc::new(move |_| changes.set(changes.get() + 1))
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, stateChangeTriggersCallback
#[test]
fn a_state_change_calls_its_scopes_callback() {
    let changes = Rc::new(Cell::new(0));
    let state = mutable_state_of(0);
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    let counted = changes.clone();
    let on_change: Rc<dyn Fn(&&'static str)> = Rc::new(move |affected| {
        assert_eq!(*affected, "Hello World");
        assert_eq!(counted.get(), 0);
        counted.set(counted.get() + 1);
    });
    observer.observe_reads("Hello World", &on_change, || state.get());
    Snapshot::notify_objects_initialized();
    state.update(|value| value + 1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
    observer.stop();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, multipleStagesWorksTogether
#[test]
fn several_stages_are_called_each() {
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    let models = [mutable_state_of(0), mutable_state_of(0), mutable_state_of(0)];
    let changes: Vec<Rc<Cell<u32>>> = (0..3).map(|_| Rc::default()).collect();
    let names = ["Stage1", "Stage2", "Stage3"];
    let callbacks: Vec<Callback> = (0..3)
        .map(|stage| {
            let (changes, name) = (changes[stage].clone(), names[stage]);
            Rc::new(move |affected: &&'static str| {
                assert_eq!(*affected, name);
                assert_eq!(changes.get(), 0);
                changes.set(changes.get() + 1);
            }) as Rc<dyn Fn(&&'static str)>
        })
        .collect();
    for stage in 0..3 {
        let model = models[stage].handle();
        observer.observe_reads(names[stage], &callbacks[stage], || model.get());
    }
    Snapshot::notify_objects_initialized();
    for model in &models {
        model.update(|value| value + 1);
    }
    Snapshot::send_apply_notifications();
    assert_eq!(changes.iter().map(|changes| changes.get()).collect::<Vec<_>>(), [1, 1, 1]);
    observer.stop();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, enclosedStagesCorrectlyObserveChanges
#[test]
fn nested_stages_record_their_own_reads() {
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    let (stage1, stage2_1, stage2_2) =
        (mutable_state_of(0), mutable_state_of(0), mutable_state_of(0));
    let stage1_changes = Rc::new(Cell::new(0));
    let stage2_changes: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let counted = stage1_changes.clone();
    let on_stage1: Rc<dyn Fn(&&'static str)> = Rc::new(move |affected| {
        assert_eq!(*affected, "stage 1");
        assert_eq!(counted.get(), 0);
        counted.set(counted.get() + 1);
    });
    let log = stage2_changes.clone();
    let on_stage2: Rc<dyn Fn(&&'static str)> = Rc::new(move |affected| {
        assert_ne!(*affected, "stage 1", "stage 1 called in stage 2");
        assert!(!log.borrow().contains(affected));
        log.borrow_mut().push(affected);
    });
    let (a, b, c) = (stage2_1.handle(), stage2_2.handle(), stage1.handle());
    observer.observe_reads("stage 1 - value 1", &on_stage2, || {
        a.get();
        observer.observe_reads("stage 2 - value 2", &on_stage2, || {
            b.get();
            observer.observe_reads("stage 1", &on_stage1, || c.get());
        });
    });
    Snapshot::notify_objects_initialized();
    stage2_1.update(|value| value + 1);
    stage2_2.update(|value| value + 1);
    stage1.update(|value| value + 1);
    Snapshot::send_apply_notifications();
    assert_eq!(stage1_changes.get(), 1);
    let mut stage2 = stage2_changes.borrow().clone();
    stage2.sort();
    assert_eq!(stage2, ["stage 1 - value 1", "stage 2 - value 2"]);
    observer.stop();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, withoutReadObservationStopsObserving
#[test]
fn reads_without_observation_are_not_observed() {
    let changes = Rc::new(Cell::new(0));
    run_simple_test(|observer, state| {
        observer.observe_reads("data", &counting(&changes), || {
            Snapshot::without_read_observation(|| state.get())
        });
    });
    assert_eq!(changes.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, changeAfterWithoutReadObservationIsObserving
#[test]
fn a_read_after_without_read_observation_is_observed() {
    let changes = Rc::new(Cell::new(0));
    run_simple_test(|observer, state| {
        observer.observe_reads("data", &counting(&changes), || {
            Snapshot::without_read_observation(|| state.get());
            state.get()
        });
    });
    assert_eq!(changes.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, nestedWithoutReadObservation
#[test]
fn nested_reads_without_observation_are_not_observed() {
    let changes = Rc::new(Cell::new(0));
    run_simple_test(|observer, state| {
        observer.observe_reads("data", &counting(&changes), || {
            Snapshot::without_read_observation(|| {
                Snapshot::without_read_observation(|| state.get());
                state.get()
            })
        });
    });
    assert_eq!(changes.get(), 0);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, simpleObserving
#[test]
fn an_observed_read_is_called_back() {
    let changes = Rc::new(Cell::new(0));
    run_simple_test(|observer, state| {
        observer.observe_reads("data", &counting(&changes), || state.get());
    });
    assert_eq!(changes.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, observeWithinWithoutReadObservation
#[test]
fn an_observation_inside_without_read_observation_observes() {
    let (outer, inner) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    run_simple_test(|observer, state| {
        observer.observe_reads("data", &counting(&outer), || {
            Snapshot::without_read_observation(|| {
                observer.observe_reads("data", &counting(&inner), || state.get())
            })
        });
    });
    assert_eq!((outer.get(), inner.get()), (0, 1));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, withoutReadsPausesNestedObservation
#[test]
fn without_read_observation_pauses_nested_observations() {
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    run_simple_test(|observer, state| {
        observer.observe_reads("scope1", &counting(&first), || {
            observer.observe_reads("scope2", &counting(&second), || {
                Snapshot::without_read_observation(|| state.get())
            })
        });
    });
    assert_eq!((first.get(), second.get()), (0, 0));
}

fn recursive_apply(states: usize) {
    let observer = SnapshotStateObserver::new(|run| run());
    let states: Rc<Vec<OwnedState<i32>>> =
        Rc::new((0..states).map(|_| mutable_state_of(0)).collect());
    observer.start();
    Snapshot::notify_objects_initialized();
    let written = states.clone();
    let on_change: Rc<dyn Fn(&String)> = Rc::new(move |scope| {
        if scope == "scope" && written[0].get() < 2 {
            for state in written.iter() {
                state.update(|value| value + 1);
                Snapshot::send_apply_notifications();
            }
        }
    });
    let read = |states: &Rc<Vec<OwnedState<i32>>>| {
        for state in states.iter() {
            state.get();
        }
    };
    observer.observe_reads("scope".to_string(), &on_change, || read(&states));
    for index in 0..10 {
        observer.observe_reads(format!("scope {index}"), &on_change, || read(&states));
    }
    for state in states.iter() {
        state.update(|value| value + 1);
    }
    Snapshot::send_apply_notifications();
    observer.stop();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, testRecursiveApplyChanges_SingleRecursive
#[test]
fn a_callback_writing_and_sending_again_is_fine() {
    recursive_apply(2);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, testRecursiveApplyChanges_MultiRecursive
#[test]
fn a_callback_writing_and_sending_several_times_is_fine() {
    recursive_apply(4);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsCommon.kt, readingValueAfterClearInvalidates
#[test]
fn a_scope_observed_again_after_a_clear_is_called_back() {
    let changes = Rc::new(Cell::new(0));
    run_simple_test(|observer, state| {
        let callback = counting(&changes);
        observer.observe_reads("scope", &callback, || state.get());
        observer.clear(&"scope");
        observer.observe_reads("scope", &callback, || state.get());
    });
    assert_eq!(changes.get(), 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/snapshots/SnapshotStateObserverTestsJvm.kt, nestedObservationIsClearingThePreviousScopesBeforeReexecuting
#[test]
fn a_nested_observation_run_again_forgets_its_old_reads() {
    let observer = SnapshotStateObserver::new(|run| run());
    let (state1, state2) = (mutable_state_of(true), mutable_state_of(0));
    let changes = Rc::new(Cell::new(0));
    let on_changed1: Rc<dyn Fn(&&'static str)> = Rc::new(|_| {});
    let on_changed2 = counting(&changes);
    let (first, second) = (state1.handle(), state2.handle());
    let run = || {
        observer.observe_reads("data1", &on_changed1, || {
            observer.observe_reads("data2", &on_changed2, || {
                if first.get() {
                    second.update(|value| value + 1);
                }
            })
        })
    };
    observer.start();
    Snapshot::notify_objects_initialized();
    run();
    assert_eq!(changes.get(), 0);
    state1.set(false);
    Snapshot::send_apply_notifications();
    run();
    assert_eq!(changes.get(), 1);
    state2.update(|value| value + 1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
    observer.stop();
}

/// A stopped observer hears nothing; started again, it hears what changes then.
#[test]
fn a_stopped_observer_hears_nothing() {
    let changes = Rc::new(Cell::new(0));
    let observer = SnapshotStateObserver::new(|run| run());
    let state = mutable_state_of(0);
    let handle = state.handle();
    let callback = counting(&changes);
    observer.observe_reads("scope", &callback, || handle.get());
    Snapshot::notify_objects_initialized();
    state.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 0, "not started");
    observer.start();
    observer.observe_reads("scope", &callback, || handle.get());
    state.set(2);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}

// ---- When invalidation happens (D751) ------------------------------------------------------

/// A write outside composition invalidates its readers at the send, not at the write: a scope
/// reading it is valid until then, and the frame is asked for then.
#[test]
fn readers_are_invalidated_at_the_send() {
    let state = mutable_state_of(0);
    let requested = Rc::new(Cell::new(0));
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    let counted = requested.clone();
    composition.set_frame_requester(move || counted.set(counted.get() + 1));
    let handle = state.handle();
    composition.set_content(move || {
        handle.get();
    });
    state.set(1);
    state.set(2);
    assert_eq!(requested.get(), 0, "nothing invalidated at the writes");
    Snapshot::send_apply_notifications();
    assert_eq!(requested.get(), 1, "invalidated once, at the send");
    assert!(composition.recompose());
}

/// The executor's turn sends at its end: what a task writes invalidates its readers before
/// `run_ready` returns.
#[test]
fn a_tasks_writes_are_sent_when_its_turn_ends() {
    let state = mutable_state_of(0);
    let requested = Rc::new(Cell::new(false));
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    let flag = requested.clone();
    composition.set_frame_requester(move || flag.set(true));
    let handle = state.handle();
    composition.set_content(move || {
        handle.get();
    });
    let scope = CoroutineScope::new();
    scope.launch(async move { handle.set(3) });
    compose_runtime::platform::run_ready();
    assert!(requested.get());
}

/// A state made and written in the same global snapshot isn't a change (Compose's
/// `overwritableRecord` writes the record made with it), until the snapshot advances.
#[test]
fn a_state_written_where_it_was_made_is_not_a_change() {
    let changes = Rc::new(Cell::new(0));
    let counted = changes.clone();
    let _observer = Snapshot::register_apply_observer(move |changed, _| {
        counted.set(counted.get() + changed.len());
    });
    let state = mutable_state_of(0);
    state.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 0);
    Snapshot::notify_objects_initialized();
    state.set(2);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}

/// What a pass's effects made is initialized when the pass ends, as `Recomposer`'s
/// `notifyObjectsInitialized` closes a frame after applying changes: an observation reading
/// such a state hears its next write, with nothing else having changed in between.
// Compose: compose/runtime/runtime/src/commonMain/kotlin/androidx/compose/runtime/Recomposer.kt, the frame's Snapshot.notifyObjectsInitialized
#[test]
fn a_state_made_by_a_passs_effect_is_initialized_when_the_pass_ends() {
    let made: Rc<RefCell<Option<OwnedState<i32>>>> = Rc::default();
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    let slot = made.clone();
    composition.set_content(move || {
        let slot = slot.clone();
        SideEffect(move || *slot.borrow_mut() = Some(mutable_state_of(0)));
    });
    let state = made.borrow().as_ref().expect("the effect ran").handle();
    assert_eq!(changes_of_observed(state, || state.set(1)), 1);
}

/// What a turn made is initialized when it ends: a state a task made and an observation read is
/// changed by a later turn's write, however quiet the turns between (an animation a task
/// started, read only by drawing, stepping on frames that change nothing else).
#[test]
fn a_state_made_in_a_turn_is_initialized_when_the_turn_ends() {
    let made: Rc<RefCell<Option<OwnedState<i32>>>> = Rc::default();
    let slot = made.clone();
    let scope = CoroutineScope::new();
    scope.launch(async move { *slot.borrow_mut() = Some(mutable_state_of(0)) });
    compose_runtime::platform::run_ready();
    let state = made.borrow().as_ref().expect("the task ran").handle();
    let changes = changes_of_observed(state, || {
        scope.launch(async move { state.set(1) });
        compose_runtime::platform::run_ready();
    });
    assert_eq!(changes, 1);
}

/// How many times a started `SnapshotStateObserver` that read `state` is called back for what
/// `block` does, sent.
fn changes_of_observed(state: MutableState<i32>, block: impl FnOnce()) -> u32 {
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    let changes = Rc::new(Cell::new(0));
    let callback = counting(&changes);
    observer.observe_reads("scope", &callback, || state.get());
    block();
    Snapshot::send_apply_notifications();
    observer.stop();
    changes.get()
}

/// A write outside any turn has no end of turn to send it: its first asks the host for one (its
/// wake handler). A write in a turn doesn't: the turn sends (D751).
#[test]
fn a_write_outside_a_turn_wakes_the_host_and_one_in_a_turn_does_not() {
    let wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = wakes.clone();
    compose_runtime::platform::set_wake_handler(move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let woken = || wakes.swap(0, std::sync::atomic::Ordering::SeqCst);
    let (first, second) = (mutable_state_of(0), mutable_state_of(0));
    Snapshot::notify_objects_initialized();
    first.set(1);
    second.set(1);
    assert_eq!(woken(), 1, "the first change since a send asks for a turn");
    Snapshot::send_apply_notifications();
    {
        let _turn = compose_runtime::platform::Turn::begin();
        first.set(2);
        assert_eq!(woken(), 0, "a turn sends at its end");
    }
    assert_eq!(woken(), 0);
    second.set(2);
    assert_eq!(woken(), 1);
    Snapshot::send_apply_notifications();
}

// ---- What a send holds (D756) ------------------------------------------------------------------

/// Counts what apply observers hear: applies, and the states in each.
fn heard() -> (Rc<Cell<usize>>, Rc<RefCell<Vec<usize>>>, snapshots::ObserverHandle) {
    let (applies, sizes): (Rc<Cell<usize>>, Rc<RefCell<Vec<usize>>>) = Default::default();
    let (counted, listed) = (applies.clone(), sizes.clone());
    let handle = Snapshot::register_apply_observer(move |changed, _| {
        counted.set(counted.get() + 1);
        listed.borrow_mut().push(changed.len());
    });
    (applies, sizes, handle)
}

/// `notifyObjectsInitialized` on the global snapshot is `advanceGlobalSnapshot`, which notifies:
/// what was written before it is heard, once, then and not at the next send.
#[test]
fn notifying_objects_initialized_sends_what_changed() {
    let state = mutable_state_of(0);
    Snapshot::notify_objects_initialized();
    let (applies, sizes, _handle) = heard();
    state.set(1);
    Snapshot::notify_objects_initialized();
    assert_eq!(applies.get(), 1);
    assert_eq!(*sizes.borrow(), [1]);
    Snapshot::notify_objects_initialized();
    Snapshot::send_apply_notifications();
    assert_eq!(applies.get(), 1, "nothing more to hear");
}

/// A state written by a pass's effect and again after the pass ended, with the global snapshot
/// advanced in between and no send, is one change (the design's `in_global_modified`).
#[test]
fn a_change_set_holds_a_state_once() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    Snapshot::notify_objects_initialized();
    let (applies, sizes, _observer) = heard();
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    composition.set_content(move || {
        SideEffect(move || handle.set(1));
    });
    state.set(2);
    Snapshot::send_apply_notifications();
    assert_eq!(applies.get(), 1);
    assert_eq!(*sizes.borrow(), [1], "the state is in the changes once");
    // The next change is heard again.
    state.set(3);
    Snapshot::send_apply_notifications();
    assert_eq!(*sizes.borrow(), [1, 1]);
}

/// A state a pass writes that it didn't make is a change, though nothing advanced the global
/// snapshot since the state was made: the pass is a snapshot of its own, as Compose's.
#[test]
fn a_write_in_composition_to_a_state_made_before_the_pass_is_a_change() {
    let (applies, sizes, _observer) = heard();
    let state = mutable_state_of(0);
    let handle = state.handle();
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    composition.set_content(move || handle.set(1));
    Snapshot::send_apply_notifications();
    assert_eq!(applies.get(), 1);
    assert_eq!(*sizes.borrow(), [1]);
}

/// A state made and written in the same pass isn't a change, as in Compose's composition snapshot.
#[test]
fn a_write_in_composition_to_a_state_the_pass_made_is_not_a_change() {
    let (applies, _sizes, _observer) = heard();
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    composition.set_content(|| {
        let state = remember(|| mutable_state_of(0));
        state.set(1);
    });
    Snapshot::send_apply_notifications();
    assert_eq!(applies.get(), 0);
}

/// `set_content` sends first, as `recompose`: an earlier write doesn't invalidate what the pass
/// reads with its new value.
#[test]
fn set_content_sends_first() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    Snapshot::notify_objects_initialized();
    state.set(1);
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    composition.set_content(move || {
        handle.get();
    });
    assert!(!composition.has_invalidations(), "the pass read the value written");
}

/// A turn that panicked keeps its writes unsent; a later write outside a turn still asks the
/// host for a tick, which sends both.
#[test]
fn writes_kept_by_a_turn_that_panicked_dont_stop_later_wakes() {
    let wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = wakes.clone();
    compose_runtime::platform::set_wake_handler(move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let woken = || wakes.swap(0, std::sync::atomic::Ordering::SeqCst);
    let (first, second) = (mutable_state_of(0), mutable_state_of(0));
    Snapshot::notify_objects_initialized();
    let (applies, sizes, _observer) = heard();
    let handle = first.handle();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _turn = compose_runtime::platform::Turn::begin();
        handle.set(1);
        panic!("in the turn");
    }));
    assert!(unwound.is_err());
    assert_eq!(woken(), 0);
    second.set(1);
    assert_eq!(woken(), 1, "the write outside a turn asks for a tick");
    second.set(2);
    assert_eq!(woken(), 0, "once");
    compose_runtime::platform::run_ready();
    assert_eq!(applies.get(), 1);
    assert_eq!(*sizes.borrow(), [2]);
    first.set(2);
    assert_eq!(woken(), 1, "and a send starts over");
    Snapshot::send_apply_notifications();
}

/// An `edit` that panicked recorded no write: the next write to the state is a change.
#[test]
fn a_write_after_an_edit_that_panicked_is_a_change() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    Snapshot::notify_objects_initialized();
    let (applies, _sizes, _observer) = heard();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        handle.edit(|_| panic!("in the edit"));
    }));
    assert!(unwound.is_err());
    state.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(applies.get(), 1);
}

/// A `set` whose old value panics when dropped recorded its write first.
#[test]
fn a_write_whose_old_value_panics_when_dropped_is_a_change() {
    #[derive(PartialEq)]
    struct PanicsOnDrop(bool);
    impl Drop for PanicsOnDrop {
        fn drop(&mut self) {
            if self.0 && !std::thread::panicking() {
                panic!("dropping the old value");
            }
        }
    }
    let state = mutable_state_of(PanicsOnDrop(true));
    let handle = state.handle();
    Snapshot::notify_objects_initialized();
    let (applies, _sizes, _observer) = heard();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        handle.set(PanicsOnDrop(false));
    }));
    assert!(unwound.is_err());
    Snapshot::send_apply_notifications();
    assert_eq!(applies.get(), 1);
}

// ---- SnapshotStateObserver and the apply observers (D757) --------------------------------------

/// `start` registers an apply observer, which sends what's pending first: a change made before
/// it isn't heard.
#[test]
fn a_change_before_start_is_not_heard() {
    let state = mutable_state_of(0);
    Snapshot::notify_objects_initialized();
    let observer = SnapshotStateObserver::new(|run| run());
    let changes = Rc::new(Cell::new(0));
    let callback = counting(&changes);
    observer.observe_reads("scope", &callback, || state.get());
    state.set(1);
    observer.start();
    assert_eq!(changes.get(), 0);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 0, "sent before it started");
    state.set(2);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}

/// `notify_changes` is for an observer that isn't started too: its callers tell it directly.
#[test]
fn notify_changes_works_on_an_observer_not_started() {
    let state = mutable_state_of(0);
    Snapshot::notify_objects_initialized();
    let observer = Rc::new(SnapshotStateObserver::new(|run| run()));
    let changes = Rc::new(Cell::new(0));
    let callback = counting(&changes);
    observer.observe_reads("scope", &callback, || state.get());
    let told = observer.clone();
    let _apply = Snapshot::register_apply_observer(move |changed, snapshot| {
        told.notify_changes(changed, snapshot);
    });
    state.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}

/// An observation of the same scope nested inside `without_read_observation` observes: it started
/// inside the barrier, whatever observation of that scope started outside it.
#[test]
fn the_same_scope_observed_inside_without_read_observation_observes() {
    let state = mutable_state_of(0);
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    Snapshot::notify_objects_initialized();
    let changes = Rc::new(Cell::new(0));
    let callback = counting(&changes);
    observer.observe_reads("scope", &callback, || {
        Snapshot::without_read_observation(|| {
            observer.observe_reads("scope", &callback, || state.get());
        });
    });
    state.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}

/// And what the outer one reads after it still registers.
#[test]
fn the_outer_observation_of_a_scope_records_again_after_a_nested_one_in_a_barrier() {
    let (inner, outer) = (mutable_state_of(0), mutable_state_of(0));
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    Snapshot::notify_objects_initialized();
    let changes = Rc::new(Cell::new(0));
    let callback = counting(&changes);
    observer.observe_reads("scope", &callback, || {
        Snapshot::without_read_observation(|| {
            observer.observe_reads("scope", &callback, || inner.get());
            // In the barrier, outside the nested observation: hidden.
            outer.get();
        });
    });
    outer.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 0, "read in the barrier");
    inner.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}

/// A started observer is an apply observer as others are, heard in registration order, as
/// `SnapshotStateObserver.start` registers one with `registerApplyObserver`.
#[test]
fn a_started_observer_hears_in_registration_order() {
    let state = mutable_state_of(0);
    Snapshot::notify_objects_initialized();
    let order: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let log = order.clone();
    let _before = Snapshot::register_apply_observer(move |_, _| log.borrow_mut().push("before"));
    let observer = SnapshotStateObserver::new(|run| run());
    let log = order.clone();
    let callback: Callback = Rc::new(move |_| log.borrow_mut().push("observer"));
    observer.observe_reads("scope", &callback, || state.get());
    observer.start();
    let log = order.clone();
    let _after = Snapshot::register_apply_observer(move |_, _| log.borrow_mut().push("after"));
    state.set(1);
    Snapshot::send_apply_notifications();
    assert_eq!(*order.borrow(), ["before", "observer", "after"]);
}

/// A write in composition is heard once, at the send, as Compose's composition snapshot applied.
#[test]
fn a_write_in_composition_calls_an_observer_back_once_at_the_send() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    Snapshot::notify_objects_initialized();
    let changes = Rc::new(Cell::new(0));
    let callback = counting(&changes);
    observer.observe_reads("scope", &callback, || state.get());
    let composition = Composition::new(compose_runtime::platform::NodeTree::<()>::new());
    composition.set_content(move || handle.set(1));
    assert_eq!(changes.get(), 0, "not before the send");
    Snapshot::send_apply_notifications();
    assert_eq!(changes.get(), 1);
}

/// A derived state read under a scope calls it back when its result changes (its own cases come
/// with step S2).
#[test]
fn a_derived_state_read_under_a_scope_calls_it_back_when_it_changes() {
    let state = mutable_state_of(0);
    let handle = state.handle();
    let derived = derived_state_of(move || handle.get() / 2);
    let observer = SnapshotStateObserver::new(|run| run());
    observer.start();
    Snapshot::notify_objects_initialized();
    let changes = Rc::new(Cell::new(0));
    let callback = counting(&changes);
    let read = derived.as_state();
    observer.observe_reads("scope", &callback, || read.get());
    state.set(1);
    Snapshot::send_apply_notifications();
    compose_runtime::platform::flush_derived_states();
    assert_eq!(changes.get(), 0, "the result is the same");
    state.set(2);
    Snapshot::send_apply_notifications();
    compose_runtime::platform::flush_derived_states();
    assert_eq!(changes.get(), 1);
}
