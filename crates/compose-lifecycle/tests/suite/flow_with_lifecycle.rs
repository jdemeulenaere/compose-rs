//! `flow_with_lifecycle` and `Lifecycle::event_flow` against AndroidX's `FlowWithLifecycleTest`
//! and `LifecycleEventFlowTest`.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use LifecycleEvent::*;
use LifecycleState::*;
use compose_lifecycle::*;
use compose_runtime::flow::{MutableStateFlow, Stream, next};
use compose_runtime::platform::{Delay, run_ready};
use compose_runtime::{CoroutineScope, Job, delay};

fn run() {
    run_ready();
}

/// A finite stream of items from an iterator.
struct Values<I>(I);

impl<I: Iterator + Unpin> Stream for Values<I> {
    type Item = I::Item;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.next())
    }
}

fn values_of<const N: usize>(items: [i32; N]) -> Values<std::array::IntoIter<i32, N>> {
    Values(items.into_iter())
}

/// A stream yielding `1`, `2`, then waiting on a 10 s delay without completing.
struct TwoThenDelay {
    step: u8,
    sleep: Option<Pin<Box<Delay>>>,
}

impl TwoThenDelay {
    fn new() -> Self {
        TwoThenDelay { step: 0, sleep: None }
    }
}

impl Unpin for TwoThenDelay {}

impl Stream for TwoThenDelay {
    type Item = i32;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<i32>> {
        match self.step {
            0 => {
                self.step = 1;
                Poll::Ready(Some(1))
            }
            1 => {
                self.step = 2;
                Poll::Ready(Some(2))
            }
            _ => {
                let sleep =
                    self.sleep.get_or_insert_with(|| Box::pin(delay(Duration::from_secs(10))));
                match sleep.as_mut().poll(context) {
                    Poll::Ready(()) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

/// A multi-subscriber hot stream with no replay: only active subscriptions receive emitted items.
#[derive(Clone, Default)]
struct HotFlow<T> {
    subscribers: Rc<RefCell<Vec<Weak<HotSubscriber<T>>>>>,
}

struct HotSubscriber<T> {
    items: RefCell<VecDeque<T>>,
    waker: RefCell<Option<Waker>>,
}

struct HotSubscription<T> {
    subscriber: Rc<HotSubscriber<T>>,
}

impl<T> Unpin for HotSubscription<T> {}

impl<T: Clone> HotFlow<T> {
    fn subscribe(&self) -> HotSubscription<T> {
        let subscriber = Rc::new(HotSubscriber {
            items: RefCell::new(VecDeque::new()),
            waker: RefCell::new(None),
        });
        let mut subscribers = self.subscribers.borrow_mut();
        subscribers.retain(|entry| entry.strong_count() > 0);
        subscribers.push(Rc::downgrade(&subscriber));
        HotSubscription { subscriber }
    }

    fn emit(&self, value: T) {
        let live: Vec<Rc<HotSubscriber<T>>> = {
            let mut subscribers = self.subscribers.borrow_mut();
            subscribers.retain(|entry| entry.strong_count() > 0);
            subscribers.iter().filter_map(Weak::upgrade).collect()
        };
        for subscriber in live {
            subscriber.items.borrow_mut().push_back(value.clone());
            let waker = subscriber.waker.borrow_mut().take();
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
}

impl<T> Stream for HotSubscription<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        if let Some(value) = self.subscriber.items.borrow_mut().pop_front() {
            return Poll::Ready(Some(value));
        }
        *self.subscriber.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }
}

/// A stream calling `action` on each item yielded upstream.
struct Inspect<S, F> {
    stream: S,
    action: F,
}

impl<S: Unpin, F> Unpin for Inspect<S, F> {}

impl<S: Stream + Unpin, F: FnMut(&S::Item)> Stream for Inspect<S, F> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<S::Item>> {
        let this = &mut *self;
        let polled = Pin::new(&mut this.stream).poll_next(context);
        if let Poll::Ready(Some(ref item)) = polled {
            (this.action)(item);
        }
        polled
    }
}

fn collect_in<T: 'static, S: Stream<Item = T> + Unpin + 'static>(
    scope: CoroutineScope,
    mut stream: S,
    sink: &Rc<RefCell<Vec<T>>>,
) -> Job {
    let sink = sink.clone();
    scope.launch(async move {
        while let Some(item) = next(&mut stream).await {
            sink.borrow_mut().push(item);
        }
    })
}

fn collect_take<T: 'static, S: Stream<Item = T> + Unpin + 'static>(
    mut stream: S,
    limit: usize,
) -> Vec<T> {
    let items: Rc<RefCell<Vec<T>>> = Rc::default();
    let seen = items.clone();
    let scope = CoroutineScope::new();
    let job = scope.launch(async move {
        while seen.borrow().len() < limit
            && let Some(item) = next(&mut stream).await
        {
            seen.borrow_mut().push(item);
        }
    });
    run();
    assert!(!job.is_active(), "expected the bounded collection to finish");
    items.take()
}

fn check_restarts_on_resumed<F: Flow<Item = i32>>(
    flow: F,
    expected_before_restart: &[i32],
    expected_after_restart: &[i32],
    before_restart: impl FnOnce(),
    while_stopped: impl FnOnce(),
    after_restart: impl FnOnce(),
) {
    let owner = TestLifecycleOwner::new(Started);
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(*scope, flow.flow_with_lifecycle_with(&owner, Resumed), &collected);
    run();
    assert!(collected.borrow().is_empty());

    owner.set_current_state(Resumed);
    before_restart();
    run();
    assert_eq!(*collected.borrow(), expected_before_restart);

    owner.set_current_state(Started);
    while_stopped();
    run();
    assert_eq!(*collected.borrow(), expected_before_restart);

    owner.set_current_state(Resumed);
    after_restart();
    run();
    assert_eq!(*collected.borrow(), expected_after_restart);

    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    assert_eq!(owner.observer_count(), 0);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testFiniteFlowCompletes
#[test]
fn a_finite_flow_yields_its_items_when_at_min_state() {
    let owner = TestLifecycleOwner::new(Created);
    let result =
        collect_take((|| values_of([1, 2, 3])).flow_with_lifecycle_with(&owner, Created), 3);
    assert_eq!(result, [1, 2, 3]);
    assert_eq!(owner.observer_count(), 0);
    owner.set_current_state(Destroyed);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testFlowStartsInSubsequentLifecycleState
#[test]
fn a_flow_starts_when_lifecycle_is_already_past_min_state() {
    let owner = TestLifecycleOwner::new(Resumed);
    let result =
        collect_take((|| values_of([1, 2, 3])).flow_with_lifecycle_with(&owner, Created), 3);
    assert_eq!(result, [1, 2, 3]);
    assert_eq!(owner.observer_count(), 0);
    owner.set_current_state(Destroyed);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testFlowDoesNotCollectIfLifecycleIsDestroyed
#[test]
fn a_destroyed_lifecycle_collects_nothing_and_completes() {
    let owner = TestLifecycleOwner::new(Created);
    owner.set_current_state(Destroyed);
    let called = Rc::new(Cell::new(false));
    let seen = called.clone();
    let result = collect_take(
        (move || {
            seen.set(true);
            values_of([1, 2, 3])
        })
        .flow_with_lifecycle_with(&owner, Resumed),
        3,
    );
    assert!(result.is_empty());
    assert!(!called.get());
    assert_eq!(owner.observer_count(), 0);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testCollectionRestartsWithFlowThatCompletes
#[test]
fn a_completed_upstream_flow_is_collected_again_when_reentering_min_state() {
    check_restarts_on_resumed(|| values_of([1, 2]), &[1, 2], &[1, 2, 1, 2], || {}, || {}, || {});
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testCollectionRestartsWithFlowThatDoesNotComplete
#[test]
fn an_incomplete_upstream_flow_cancels_below_min_state_and_restarts_above() {
    check_restarts_on_resumed(TwoThenDelay::new, &[1, 2], &[1, 2, 1, 2], || {}, || {}, || {});
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testCollectionRestartsWithAHotFlow
#[test]
fn a_hot_flow_ignores_items_while_below_min_state_and_resumes_after() {
    let hot = HotFlow::default();
    let (subscribed, before, during, after) = (hot.clone(), hot.clone(), hot.clone(), hot);
    check_restarts_on_resumed(
        move || subscribed.subscribe(),
        &[1, 2],
        &[1, 2, 4],
        move || {
            before.emit(1);
            before.emit(2);
        },
        move || during.emit(3),
        move || after.emit(4),
    );
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testCancellingCoroutineDoesNotGetUpdates
#[test]
fn cancelling_the_collecting_task_stops_updates_and_removes_the_observer() {
    let owner = TestLifecycleOwner::new(Started);
    let hot = HotFlow::default();
    let subscribed = hot.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || subscribed.subscribe()).flow_with_lifecycle_with(&owner, Resumed),
        &collected,
    );
    run();
    owner.set_current_state(Resumed);
    hot.emit(1);
    hot.emit(2);
    run();
    assert_eq!(*collected.borrow(), [1, 2]);
    assert_eq!(owner.observer_count(), 1);

    job.cancel();
    assert_eq!(owner.observer_count(), 0);
    run();
    hot.emit(3);
    run();
    assert_eq!(*collected.borrow(), [1, 2]);
    owner.set_current_state(Destroyed);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testDestroyedLifecycleDoesNotGetUpdates
#[test]
fn destroying_the_lifecycle_completes_the_flow_and_stops_updates() {
    let owner = TestLifecycleOwner::new(Started);
    let hot = HotFlow::default();
    let subscribed = hot.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || subscribed.subscribe()).flow_with_lifecycle_with(&owner, Resumed),
        &collected,
    );
    run();
    owner.set_current_state(Resumed);
    hot.emit(1);
    hot.emit(2);
    run();
    assert_eq!(*collected.borrow(), [1, 2]);

    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    assert_eq!(owner.observer_count(), 0);
    hot.emit(3);
    run();
    assert_eq!(*collected.borrow(), [1, 2]);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testWithLaunchIn
#[test]
fn flow_with_lifecycle_defaults_to_started_and_runs_in_the_lifecycle_scope() {
    let owner = TestLifecycleOwner::new(Started);
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let job = collect_in(
        owner.lifecycle().coroutine_scope(),
        (|| values_of([1, 2, 3])).flow_with_lifecycle(&owner),
        &collected,
    );
    run();
    assert_eq!(*collected.borrow(), [1, 2, 3]);
    assert!(job.is_active());
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    assert_eq!(owner.observer_count(), 0);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testOnEachBeforeOperatorOnlyExecutesInTheRightState
#[test]
fn an_upstream_stage_before_flow_with_lifecycle_runs_only_while_active() {
    let owner = TestLifecycleOwner::new(Resumed);
    let hot = HotFlow::default();
    let subscribed = hot.clone();
    let seen_upstream: Rc<RefCell<Vec<i32>>> = Rc::default();
    let recorded = seen_upstream.clone();
    let discarded: Rc<RefCell<Vec<i32>>> = Rc::default();

    collect_in(
        owner.lifecycle().coroutine_scope(),
        flow_with_lifecycle_with(
            move || {
                let recorded = recorded.clone();
                Inspect {
                    stream: subscribed.subscribe(),
                    action: move |value: &i32| recorded.borrow_mut().push(*value),
                }
            },
            &owner,
            Resumed,
        ),
        &discarded,
    );
    run();

    hot.emit(1);
    hot.emit(2);
    run();
    assert_eq!(*seen_upstream.borrow(), [1, 2]);

    owner.set_current_state(Started);
    run();
    hot.emit(3);
    run();
    assert_eq!(*seen_upstream.borrow(), [1, 2]);

    owner.set_current_state(Resumed);
    run();
    hot.emit(4);
    run();
    assert_eq!(*seen_upstream.borrow(), [1, 2, 4]);

    owner.set_current_state(Destroyed);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testExtensionFailsWithInitializedState
#[test]
#[should_panic(expected = "repeatOnLifecycle cannot start work with the INITIALIZED")]
fn flow_with_lifecycle_refuses_the_initialized_state() {
    let owner = TestLifecycleOwner::default();
    let _ = (|| values_of([1, 2, 3])).flow_with_lifecycle_with(&owner, Initialized);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/FlowWithLifecycleTest.kt, testExtensionDoesNotCollectInDestroyedState
#[test]
fn destroyed_as_min_active_state_never_collects() {
    let owner = TestLifecycleOwner::new(Started);
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (|| values_of([1, 2, 3])).flow_with_lifecycle_with(&owner, Destroyed),
        &collected,
    );
    run();
    assert!(collected.borrow().is_empty());
    owner.set_current_state(Destroyed);
    run();
    assert!(collected.borrow().is_empty());
    assert!(!job.is_active());
}

// ---- Lifecycle::event_flow (against AndroidX's LifecycleEventFlowTest) -----------------------

// Compose: lifecycle/lifecycle-runtime-testing/src/commonTest/kotlin/androidx/lifecycle/testing/LifecycleEventFlowTest.kt, testLifecycleEventFlow
#[test]
fn an_event_flow_emits_up_to_current_state_and_subsequent_transitions() {
    let owner = TestLifecycleOwner::default();
    let collected: Rc<RefCell<Vec<LifecycleEvent>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(*scope, owner.lifecycle().event_flow(), &collected);
    run();
    owner.set_current_state(Created);
    run();
    assert_eq!(*collected.borrow(), [OnCreate, OnStart, OnStop]);
    job.cancel();
    assert_eq!(owner.observer_count(), 0);
}

// Compose: lifecycle/lifecycle-runtime-testing/src/commonTest/kotlin/androidx/lifecycle/testing/LifecycleEventFlowTest.kt, testEventFlowStopsCollectingAfterDestroyed
#[test]
fn an_event_flow_emits_on_destroy_and_completes() {
    let owner = TestLifecycleOwner::default();
    let collected: Rc<RefCell<Vec<LifecycleEvent>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(*scope, owner.lifecycle().event_flow(), &collected);
    run();
    owner.set_current_state(Created);
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    assert_eq!(owner.observer_count(), 0);
    assert_eq!(*collected.borrow(), [OnCreate, OnStart, OnStop, OnDestroy]);
}

// Compose: lifecycle/lifecycle-runtime-testing/src/commonTest/kotlin/androidx/lifecycle/testing/LifecycleEventFlowTest.kt, testEventFlowStopsCollectingAfterJobCancelled
#[test]
fn cancelling_an_event_flow_task_stops_collecting_and_removes_its_observer() {
    let owner = TestLifecycleOwner::default();
    let collected: Rc<RefCell<Vec<LifecycleEvent>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(*scope, owner.lifecycle().event_flow(), &collected);
    run();
    owner.set_current_state(Created);
    run();
    assert_eq!(*collected.borrow(), [OnCreate, OnStart, OnStop]);
    assert_eq!(owner.observer_count(), 1);

    collected.borrow_mut().clear();
    job.cancel();
    assert_eq!(owner.observer_count(), 0);
    owner.set_current_state(Resumed);
    run();
    assert!(collected.borrow().is_empty());
}

// ---- Additional edge cases -------------------------------------------------------------------

#[test]
fn an_event_flow_on_an_already_destroyed_lifecycle_completes_without_observing() {
    let owner = TestLifecycleOwner::new(Created);
    owner.set_current_state(Destroyed);
    let collected: Rc<RefCell<Vec<LifecycleEvent>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(*scope, owner.lifecycle().event_flow(), &collected);
    run();
    assert!(!job.is_active());
    assert!(collected.borrow().is_empty());
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn an_event_flow_drains_queued_events_before_ending_after_on_destroy() {
    let owner = TestLifecycleOwner::new(Resumed);
    let mut events = owner.lifecycle().event_flow();
    let collected: Rc<RefCell<Vec<LifecycleEvent>>> = Rc::default();
    let seen = collected.clone();
    let trigger = owner.clone();
    let scope = CoroutineScope::new();
    let job = scope.launch(async move {
        while let Some(event) = next(&mut events).await {
            seen.borrow_mut().push(event);
            if event == OnCreate {
                trigger.set_current_state(Destroyed);
            }
        }
    });
    run();
    assert!(!job.is_active());
    assert_eq!(*collected.borrow(), [OnCreate, OnStart, OnResume, OnPause, OnStop, OnDestroy]);
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn unpolled_streams_register_no_observers() {
    let owner = TestLifecycleOwner::default();
    let events = owner.lifecycle().event_flow();
    let gated = flow_with_lifecycle(|| values_of([1, 2]), &owner);
    assert_eq!(owner.observer_count(), 0);
    drop((events, gated));
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn a_state_flow_works_directly_with_flow_with_lifecycle() {
    let owner = TestLifecycleOwner::new(Created);
    let state_flow = MutableStateFlow::new(10);
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job =
        collect_in(*scope, state_flow.as_state_flow().flow_with_lifecycle(&owner), &collected);
    run();
    assert!(collected.borrow().is_empty());

    state_flow.set_value(20);
    owner.set_current_state(Started);
    run();
    assert_eq!(*collected.borrow(), [20]);

    owner.set_current_state(Created);
    state_flow.set_value(30);
    run();
    assert_eq!(*collected.borrow(), [20]);

    owner.set_current_state(Started);
    run();
    assert_eq!(*collected.borrow(), [20, 30]);

    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
}

#[test]
fn a_stream_moving_its_lifecycle_below_min_state_during_poll_emits_nothing_after() {
    struct StopsSelf(TestLifecycleOwner, u8);
    impl Unpin for StopsSelf {}
    impl Stream for StopsSelf {
        type Item = i32;
        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<i32>> {
            if self.1 == 0 {
                self.1 = 1;
                Poll::Ready(Some(1))
            } else {
                self.0.set_current_state(Started);
                Poll::Ready(Some(2))
            }
        }
    }

    let owner = TestLifecycleOwner::new(Resumed);
    let inside = owner.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || StopsSelf(inside.clone(), 0)).flow_with_lifecycle_with(&owner, Resumed),
        &collected,
    );
    run();
    assert_eq!(*collected.borrow(), [1]);
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
}
