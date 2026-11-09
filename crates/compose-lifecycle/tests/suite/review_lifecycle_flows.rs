//! Adversarial review tests for `port-lifecycle-flows` (D2150, D2160): `Lifecycle::event_flow`,
//! `flow_with_lifecycle` and `collect_as_state_with_lifecycle` under waker replacement, waker-drop
//! re-entrancy, synchronous waker polling, mid-poll lifecycle restarts, `OnPause`/`OnResume`
//! stability above `min_active_state`, `Created -> Destroyed` completion, `!Unpin` streams,
//! `SnapshotFlow` unwatching while stopped, and `StateFlow` ABA mutations while stopped.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use LifecycleEvent::*;
use LifecycleState::*;
use compose_lifecycle::*;
use compose_runtime::flow::{MutableStateFlow, SnapshotFlowManager, StateFlow, Stream, next};
use compose_runtime::platform::run_ready;
use compose_runtime::snapshots::Snapshot;
use compose_runtime::*;
use compose_test::*;

thread_local! {
    static WAKER_DROP_HOOK: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) };
    static WAKER_WAKE_HOOK: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

struct CountingWake {
    wakes: AtomicUsize,
}

impl CountingWake {
    fn new() -> (Arc<Self>, Waker) {
        let counter = Arc::new(CountingWake { wakes: AtomicUsize::new(0) });
        let waker = Waker::from(counter.clone());
        (counter, waker)
    }

    fn count(&self) -> usize {
        self.wakes.load(Ordering::SeqCst)
    }
}

impl Wake for CountingWake {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

struct DropHookWake {
    wakes: AtomicUsize,
}

impl Wake for DropHookWake {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for DropHookWake {
    fn drop(&mut self) {
        if let Some(hook) = WAKER_DROP_HOOK.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }
}

struct InlineHookWake;

impl Wake for InlineHookWake {
    fn wake(self: Arc<Self>) {
        let hook = WAKER_WAKE_HOOK.with(|slot| slot.borrow().clone());
        if let Some(hook) = hook {
            hook();
        }
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let hook = WAKER_WAKE_HOOK.with(|slot| slot.borrow().clone());
        if let Some(hook) = hook {
            hook();
        }
    }
}

/// A tracked multi-subscriber hot flow recording total subscriptions and drops.
#[derive(Clone, Default)]
struct TrackedHotFlow<T> {
    subscribers: Rc<RefCell<Vec<Weak<TrackedSubscriber<T>>>>>,
    subscribes: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}

struct TrackedSubscriber<T> {
    items: RefCell<VecDeque<T>>,
    waker: RefCell<Option<Waker>>,
}

struct TrackedSubscription<T> {
    subscriber: Rc<TrackedSubscriber<T>>,
    drops: Rc<Cell<usize>>,
}

impl<T> Unpin for TrackedSubscription<T> {}

impl<T> Drop for TrackedSubscription<T> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl<T: Clone> TrackedHotFlow<T> {
    fn subscribe(&self) -> TrackedSubscription<T> {
        self.subscribes.set(self.subscribes.get() + 1);
        let subscriber = Rc::new(TrackedSubscriber {
            items: RefCell::new(VecDeque::new()),
            waker: RefCell::new(None),
        });
        let mut subscribers = self.subscribers.borrow_mut();
        subscribers.retain(|entry| entry.strong_count() > 0);
        subscribers.push(Rc::downgrade(&subscriber));
        TrackedSubscription { subscriber, drops: self.drops.clone() }
    }

    fn emit(&self, value: T) {
        let live: Vec<Rc<TrackedSubscriber<T>>> = {
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

    fn subscribe_count(&self) -> usize {
        self.subscribes.get()
    }

    fn drop_count(&self) -> usize {
        self.drops.get()
    }
}

impl<T> Stream for TrackedSubscription<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        if let Some(value) = self.subscriber.items.borrow_mut().pop_front() {
            return Poll::Ready(Some(value));
        }
        let waker = context.waker().clone();
        let previous = self.subscriber.waker.borrow_mut().replace(waker);
        drop(previous);
        Poll::Pending
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

// ---- EventFlow adversarial tests --------------------------------------------------------------

#[test]
fn an_event_flow_starting_at_initialized_pends_then_emits_full_lifecycle_and_fuses() {
    let registry = LifecycleRegistry::new();
    let mut events = registry.event_flow();
    let (counter, waker) = CountingWake::new();
    let mut cx = Context::from_waker(&waker);

    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Pending);
    assert_eq!(registry.observer_count(), 1);
    assert_eq!(counter.count(), 0);

    registry.set_current_state(Resumed);
    assert_eq!(counter.count(), 1);
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(Some(OnCreate)));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(Some(OnStart)));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(Some(OnResume)));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Pending);

    registry.set_current_state(Started);
    registry.set_current_state(Created);
    registry.set_current_state(Destroyed);
    assert_eq!(registry.observer_count(), 0);
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(Some(OnPause)));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(Some(OnStop)));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(Some(OnDestroy)));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(None));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(None));
}

#[test]
fn an_event_flow_starting_at_resumed_emits_step_up_events_on_first_poll() {
    let owner = TestLifecycleOwner::new(Resumed);
    let collected: Rc<RefCell<Vec<LifecycleEvent>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(*scope, owner.lifecycle().event_flow(), &collected);
    run_ready();
    assert_eq!(*collected.borrow(), [OnCreate, OnStart, OnResume]);
    assert!(job.is_active());
    job.cancel();
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn an_event_flow_created_before_destroy_but_polled_after_destroy_completes_at_once() {
    let owner = TestLifecycleOwner::new(Resumed);
    let mut events = owner.lifecycle().event_flow();
    assert_eq!(owner.observer_count(), 0);

    owner.set_current_state(Destroyed);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(None));
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Ready(None));
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn an_event_flow_replaces_its_waker_across_tasks_and_wakes_only_the_latest() {
    let registry = LifecycleRegistry::new();
    let mut events = registry.event_flow();
    let (count_a, waker_a) = CountingWake::new();
    let (count_b, waker_b) = CountingWake::new();

    assert_eq!(Pin::new(&mut events).poll_next(&mut Context::from_waker(&waker_a)), Poll::Pending);
    assert_eq!(Pin::new(&mut events).poll_next(&mut Context::from_waker(&waker_b)), Poll::Pending);

    registry.set_current_state(Created);
    assert_eq!(count_a.count(), 0, "replaced waker must not be woken");
    assert_eq!(count_b.count(), 1, "latest waker must be woken");
    assert_eq!(
        Pin::new(&mut events).poll_next(&mut Context::from_waker(&waker_b)),
        Poll::Ready(Some(OnCreate))
    );
}

#[test]
fn an_event_flow_replacing_a_waker_whose_drop_mutates_the_lifecycle_does_not_panic() {
    let registry = LifecycleRegistry::new();
    let mut events = registry.event_flow();

    {
        let waker_a = Waker::from(Arc::new(DropHookWake { wakes: AtomicUsize::new(0) }));
        assert_eq!(
            Pin::new(&mut events).poll_next(&mut Context::from_waker(&waker_a)),
            Poll::Pending
        );
    }

    let moved = registry.clone();
    WAKER_DROP_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            moved.set_current_state(Started);
            moved.set_current_state(Destroyed);
        }));
    });

    let (count_b, waker_b) = CountingWake::new();
    let mut cx_b = Context::from_waker(&waker_b);
    let _ = Pin::new(&mut events).poll_next(&mut cx_b);
    assert_eq!(count_b.count(), 1, "replacement waker wakes when old waker drop transitions");

    let mut drained = Vec::new();
    while let Poll::Ready(Some(event)) = Pin::new(&mut events).poll_next(&mut cx_b) {
        drained.push(event);
    }
    assert_eq!(drained, [OnCreate, OnStart, OnStop, OnDestroy]);
    assert_eq!(Pin::new(&mut events).poll_next(&mut cx_b), Poll::Ready(None));
    assert_eq!(registry.observer_count(), 0);
}

#[test]
fn an_event_flow_handles_reentrant_poll_and_lifecycle_mutation_inside_waker_wake() {
    let registry = LifecycleRegistry::new();
    let events_slot = Rc::new(RefCell::new(Some(registry.event_flow())));
    let collected: Rc<RefCell<Vec<LifecycleEvent>>> = Rc::default();
    let inline_waker = Waker::from(Arc::new(InlineHookWake));

    {
        let mut guard = events_slot.borrow_mut();
        let stream = guard.as_mut().unwrap();
        assert_eq!(
            Pin::new(stream).poll_next(&mut Context::from_waker(&inline_waker)),
            Poll::Pending
        );
    }

    let (slot_for_hook, seen, reg_for_hook, waker_for_hook) =
        (events_slot.clone(), collected.clone(), registry.clone(), inline_waker.clone());
    WAKER_WAKE_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Rc::new(move || {
            let mut guard = slot_for_hook.borrow_mut();
            let Some(stream) = guard.as_mut() else { return };
            let mut cx = Context::from_waker(&waker_for_hook);
            while let Poll::Ready(Some(event)) = Pin::new(&mut *stream).poll_next(&mut cx) {
                seen.borrow_mut().push(event);
                if event == OnStart {
                    reg_for_hook.set_current_state(Destroyed);
                }
            }
        }));
    });

    registry.set_current_state(Resumed);
    WAKER_WAKE_HOOK.with(|hook| *hook.borrow_mut() = None);

    assert_eq!(*collected.borrow(), [OnCreate, OnStart, OnStop, OnDestroy]);
    let mut guard = events_slot.borrow_mut();
    let stream = guard.as_mut().unwrap();
    assert_eq!(
        Pin::new(stream).poll_next(&mut Context::from_waker(&inline_waker)),
        Poll::Ready(None)
    );
    assert_eq!(registry.observer_count(), 0);
}

#[test]
fn an_event_flow_dropped_inside_its_waker_removes_its_observer_cleanly() {
    let registry = LifecycleRegistry::new();
    let events_slot = Rc::new(RefCell::new(Some(registry.event_flow())));
    let inline_waker = Waker::from(Arc::new(InlineHookWake));

    {
        let mut guard = events_slot.borrow_mut();
        assert_eq!(
            Pin::new(guard.as_mut().unwrap()).poll_next(&mut Context::from_waker(&inline_waker)),
            Poll::Pending
        );
    }
    assert_eq!(registry.observer_count(), 1);

    let slot_for_hook = events_slot.clone();
    WAKER_WAKE_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Rc::new(move || {
            let dropped = slot_for_hook.borrow_mut().take();
            drop(dropped);
        }));
    });

    registry.set_current_state(Resumed);
    WAKER_WAKE_HOOK.with(|hook| *hook.borrow_mut() = None);

    assert!(events_slot.borrow().is_none());
    assert_eq!(registry.observer_count(), 0);
}

// ---- FlowWithLifecycle adversarial tests ------------------------------------------------------

#[test]
fn pausing_and_resuming_above_started_keeps_the_upstream_subscription_alive() {
    let owner = TestLifecycleOwner::new(Resumed);
    let hot: TrackedHotFlow<i32> = TrackedHotFlow::default();
    let subscribed = hot.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || subscribed.subscribe()).flow_with_lifecycle(&owner),
        &collected,
    );
    run_ready();

    assert_eq!(hot.subscribe_count(), 1);
    assert_eq!(hot.drop_count(), 0);

    hot.emit(1);
    run_ready();
    assert_eq!(*collected.borrow(), [1]);

    // Resumed -> Started (OnPause): still >= Started, must NOT cancel or resubscribe!
    owner.set_current_state(Started);
    run_ready();
    assert_eq!(hot.subscribe_count(), 1);
    assert_eq!(hot.drop_count(), 0);

    hot.emit(2);
    run_ready();
    assert_eq!(*collected.borrow(), [1, 2]);

    // Started -> Resumed (OnResume): still >= Started, must NOT cancel or resubscribe!
    owner.set_current_state(Resumed);
    run_ready();
    assert_eq!(hot.subscribe_count(), 1);
    assert_eq!(hot.drop_count(), 0);

    hot.emit(3);
    run_ready();
    assert_eq!(*collected.borrow(), [1, 2, 3]);

    // Resumed -> Created (OnPause + OnStop): falls below Started, cancels upstream!
    owner.set_current_state(Created);
    run_ready();
    assert_eq!(hot.subscribe_count(), 1);
    assert_eq!(hot.drop_count(), 1);
    hot.emit(4);
    run_ready();
    assert_eq!(*collected.borrow(), [1, 2, 3]);

    // Created -> Started (OnStart): resubscribes!
    owner.set_current_state(Started);
    hot.emit(5);
    run_ready();
    assert_eq!(hot.subscribe_count(), 2);
    assert_eq!(hot.drop_count(), 1);
    assert_eq!(*collected.borrow(), [1, 2, 3, 5]);

    owner.set_current_state(Destroyed);
    run_ready();
    assert!(!job.is_active());
    assert_eq!(hot.drop_count(), 2);
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn created_min_active_state_stays_subscribed_across_upper_transitions_and_completes_on_destroy() {
    let registry = LifecycleRegistry::new();
    let hot: TrackedHotFlow<i32> = TrackedHotFlow::default();
    let subscribed = hot.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || subscribed.subscribe()).flow_with_lifecycle_with(&registry, Created),
        &collected,
    );
    run_ready();
    assert_eq!(hot.subscribe_count(), 0);

    registry.set_current_state(Created);
    hot.emit(10);
    run_ready();
    assert_eq!(hot.subscribe_count(), 1);
    assert_eq!(hot.drop_count(), 0);
    assert_eq!(*collected.borrow(), [10]);

    registry.set_current_state(Started);
    registry.set_current_state(Resumed);
    registry.set_current_state(Started);
    registry.set_current_state(Created);
    hot.emit(20);
    run_ready();
    assert_eq!(hot.subscribe_count(), 1);
    assert_eq!(hot.drop_count(), 0);
    assert_eq!(*collected.borrow(), [10, 20]);

    // Created -> Destroyed fires OnDestroy where both `down == Some(OnDestroy)` and `OnDestroy`
    // apply: upstream must drop, observer must be removed, and downstream must complete.
    registry.set_current_state(Destroyed);
    run_ready();
    assert!(!job.is_active());
    assert_eq!(hot.drop_count(), 1);
    assert_eq!(registry.observer_count(), 0);
}

#[test]
fn a_completed_finite_upstream_waits_for_stop_start_to_restart_and_destroy_to_complete() {
    struct FiniteCold(VecDeque<i32>);
    impl Unpin for FiniteCold {}
    impl Stream for FiniteCold {
        type Item = i32;
        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<i32>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    let owner = TestLifecycleOwner::new(Resumed);
    let subscribes = Rc::new(Cell::new(0usize));
    let counted = subscribes.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || {
            counted.set(counted.get() + 1);
            FiniteCold(VecDeque::from([10, 20]))
        })
        .flow_with_lifecycle_with(&owner, Started),
        &collected,
    );
    run_ready();
    assert_eq!(*collected.borrow(), [10, 20]);
    assert_eq!(subscribes.get(), 1);
    assert!(job.is_active(), "downstream remains open until OnDestroy");

    // Pausing to Started and resuming back to Resumed never dropped below Started: no restart.
    owner.set_current_state(Started);
    owner.set_current_state(Resumed);
    run_ready();
    assert_eq!(subscribes.get(), 1);
    assert_eq!(*collected.borrow(), [10, 20]);
    assert!(job.is_active());

    // Dropping below Started (to Created) and re-entering Started restarts collection.
    owner.set_current_state(Created);
    run_ready();
    assert_eq!(subscribes.get(), 1);
    owner.set_current_state(Started);
    run_ready();
    assert_eq!(subscribes.get(), 2);
    assert_eq!(*collected.borrow(), [10, 20, 10, 20]);
    assert!(job.is_active());

    owner.set_current_state(Destroyed);
    run_ready();
    assert!(!job.is_active());
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn rapid_lifecycle_bounces_between_polls_cancel_intermediate_streams() {
    let owner = TestLifecycleOwner::new(Created);
    let hot: TrackedHotFlow<i32> = TrackedHotFlow::default();
    let subscribed = hot.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || subscribed.subscribe()).flow_with_lifecycle(&owner),
        &collected,
    );
    run_ready();
    assert_eq!(hot.subscribe_count(), 0);

    // Bounce Created -> Started -> Created before run_ready(): subscribed stream drops unpolled.
    owner.set_current_state(Started);
    hot.emit(99);
    owner.set_current_state(Created);
    run_ready();
    assert_eq!(hot.subscribe_count(), 1);
    assert_eq!(hot.drop_count(), 1);
    assert!(collected.borrow().is_empty());

    // Now start, collect 1, then bounce Started -> Created -> Started synchronously.
    owner.set_current_state(Started);
    hot.emit(1);
    run_ready();
    assert_eq!(*collected.borrow(), [1]);

    owner.set_current_state(Created);
    hot.emit(100);
    owner.set_current_state(Started);
    hot.emit(2);
    run_ready();
    assert_eq!(hot.subscribe_count(), 3);
    assert_eq!(hot.drop_count(), 2);
    assert_eq!(*collected.borrow(), [1, 2]);

    owner.set_current_state(Destroyed);
    run_ready();
    assert!(!job.is_active());
}

#[test]
fn a_stream_restarting_or_destroying_its_lifecycle_during_poll_discards_stale_emission() {
    let owner = TestLifecycleOwner::new(Started);
    let inside = owner.clone();
    let generation_num = Rc::new(Cell::new(0u32));
    let gen_counter = generation_num.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();

    struct BounceInPoll {
        owner: TestLifecycleOwner,
        generation: u32,
        step: u8,
    }
    impl Unpin for BounceInPoll {}
    impl Stream for BounceInPoll {
        type Item = i32;
        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<i32>> {
            match (self.generation, self.step) {
                (1, 0) => {
                    self.step = 1;
                    Poll::Ready(Some(10))
                }
                (1, _) => {
                    // Full stop-and-restart inside poll_next, then return a stale item!
                    self.owner.set_current_state(Created);
                    self.owner.set_current_state(Started);
                    Poll::Ready(Some(999))
                }
                (2, 0) => {
                    self.step = 1;
                    Poll::Ready(Some(20))
                }
                _ => {
                    // Destroy inside poll_next, then return a stale item!
                    self.owner.set_current_state(Destroyed);
                    Poll::Ready(Some(888))
                }
            }
        }
    }

    let job = collect_in(
        *scope,
        (move || {
            let next_gen = gen_counter.get() + 1;
            gen_counter.set(next_gen);
            BounceInPoll { owner: inside.clone(), generation: next_gen, step: 0 }
        })
        .flow_with_lifecycle(&owner),
        &collected,
    );
    run_ready();

    assert!(!job.is_active());
    assert_eq!(*collected.borrow(), [10, 20]);
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn flow_with_lifecycle_replaces_waker_and_survives_lifecycle_mutation_in_waker_drop() {
    let owner = TestLifecycleOwner::new(Created);
    let hot: TrackedHotFlow<i32> = TrackedHotFlow::default();
    let subscribed = hot.clone();
    let mut flow = (move || subscribed.subscribe()).flow_with_lifecycle(&owner);

    let (count_a, waker_a) = CountingWake::new();
    assert_eq!(Pin::new(&mut flow).poll_next(&mut Context::from_waker(&waker_a)), Poll::Pending);

    {
        let waker_drop = Waker::from(Arc::new(DropHookWake { wakes: AtomicUsize::new(0) }));
        assert_eq!(
            Pin::new(&mut flow).poll_next(&mut Context::from_waker(&waker_drop)),
            Poll::Pending
        );
    }

    let (moved_owner, moved_hot) = (owner.clone(), hot.clone());
    WAKER_DROP_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            moved_owner.set_current_state(Started);
            moved_hot.emit(42);
        }));
    });

    let (count_b, waker_b) = CountingWake::new();
    let mut cx_b = Context::from_waker(&waker_b);
    let _ = Pin::new(&mut flow).poll_next(&mut cx_b);
    assert_eq!(count_a.count(), 0, "first waker was replaced");
    assert_eq!(count_b.count(), 1, "latest waker woken when old waker drop started lifecycle");
    assert_eq!(Pin::new(&mut flow).poll_next(&mut cx_b), Poll::Ready(Some(42)));

    owner.set_current_state(Destroyed);
    assert_eq!(Pin::new(&mut flow).poll_next(&mut cx_b), Poll::Ready(None));
    assert_eq!(owner.observer_count(), 0);
}

#[test]
fn dropping_flow_with_lifecycle_inside_subscribe_flow_drops_new_stream_without_waking_stale_waker()
{
    struct GuardStream(Rc<Cell<bool>>);
    impl Unpin for GuardStream {}
    impl Drop for GuardStream {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    impl Stream for GuardStream {
        type Item = i32;
        fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<i32>> {
            Poll::Pending
        }
    }

    let owner = TestLifecycleOwner::new(Created);
    let stream_dropped = Rc::new(Cell::new(false));
    let stream_dropped_flag = stream_dropped.clone();
    type BoxedStream = Pin<Box<dyn Stream<Item = i32>>>;
    let holder: Rc<RefCell<Option<BoxedStream>>> = Rc::default();
    let holder_in_subscribe = holder.clone();

    let gated = flow_with_lifecycle(
        move || {
            let dropped_flow = holder_in_subscribe.borrow_mut().take();
            drop(dropped_flow);
            GuardStream(stream_dropped_flag.clone())
        },
        &owner,
    );
    *holder.borrow_mut() = Some(Box::pin(gated));

    let (count, waker) = CountingWake::new();
    {
        let mut guard = holder.borrow_mut();
        assert_eq!(
            guard.as_mut().unwrap().as_mut().poll_next(&mut Context::from_waker(&waker)),
            Poll::Pending
        );
    }
    assert_eq!(owner.observer_count(), 1);

    owner.set_current_state(Started);
    assert!(holder.borrow().is_none());
    assert!(stream_dropped.get(), "newly created stream must be dropped immediately");
    assert_eq!(owner.observer_count(), 0);
    assert_eq!(count.count(), 0, "dropped FlowWithLifecycle must not wake its old waker");
}

#[test]
fn non_unpin_streams_work_with_flow_with_lifecycle_and_collect_as_state_with_lifecycle() {
    struct PinnedCellStream {
        value: Cell<Option<i32>>,
        _pin: PhantomPinned,
    }
    impl Stream for PinnedCellStream {
        type Item = i32;
        fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<i32>> {
            Poll::Ready(self.as_ref().get_ref().value.take())
        }
    }

    let owner = TestLifecycleOwner::new(Started);
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (|| PinnedCellStream { value: Cell::new(Some(7)), _pin: PhantomPinned })
            .flow_with_lifecycle(&owner),
        &collected,
    );
    run_ready();
    assert_eq!(*collected.borrow(), [7]);
    owner.set_current_state(Destroyed);
    run_ready();
    assert!(!job.is_active());

    let ui_owner = TestLifecycleOwner::new(Resumed);
    let held = ui_owner.lifecycle();
    let seen: Rc<Cell<i32>> = Rc::default();
    let out = seen.clone();
    let mut ui = compose_test(move || {
        out.set(
            collect_as_state_with_lifecycle_with(0, 0, &held, Started, || PinnedCellStream {
                value: Cell::new(Some(99)),
                _pin: PhantomPinned,
            })
            .get(),
        );
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), 99));
}

#[test]
fn destroyed_min_active_state_never_invokes_subscribe_flow() {
    let owner = TestLifecycleOwner::new(Resumed);
    let subscribes = Rc::new(Cell::new(0usize));
    let counted = subscribes.clone();
    let collected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let scope = CoroutineScope::new();
    let job = collect_in(
        *scope,
        (move || {
            counted.set(counted.get() + 1);
            owner_empty_stream()
        })
        .flow_with_lifecycle_with(&owner, Destroyed),
        &collected,
    );
    run_ready();
    assert_eq!(subscribes.get(), 0);
    owner.set_current_state(Destroyed);
    run_ready();
    assert!(!job.is_active());
    assert_eq!(subscribes.get(), 0);
}

fn owner_empty_stream() -> impl Stream<Item = i32> {
    struct Empty;
    impl Stream for Empty {
        type Item = i32;
        fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<i32>> {
            Poll::Ready(None)
        }
    }
    Empty
}

// ---- collect_as_state_with_lifecycle adversarial tests ----------------------------------------

#[test]
fn a_state_flow_mutated_back_to_its_previous_value_while_stopped_catches_up_on_restart() {
    let owner = TestLifecycleOwner::new(Resumed);
    let state_flow = MutableStateFlow::new("A");
    let seen: Rc<Cell<&'static str>> = Rc::new(Cell::new(""));
    let (flow, held, out) = (state_flow.as_state_flow(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        out.set(flow.collect_as_state_with_lifecycle_with(&held, Started).get());
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), "A"));

    state_flow.set_value("B");
    ui.run_on_idle(|_| assert_eq!(seen.get(), "B"));

    // Move below Started, then mutate "B" -> "A" while stopped: state must stay "B" until Started!
    owner.set_current_state(Created);
    ui.wait_for_idle();
    state_flow.set_value("A");
    ui.run_on_idle(|_| assert_eq!(seen.get(), "B"));

    // Re-entering Started must emit current value "A" and update state from "B" back to "A".
    owner.set_current_state(Started);
    ui.run_on_idle(|_| assert_eq!(seen.get(), "A"));
}

#[test]
fn swapping_mutable_state_flows_while_stopped_keeps_value_until_lifecycle_starts() {
    let owner = TestLifecycleOwner::new(Created);
    let flow1 = MutableStateFlow::new("first_init");
    let flow2 = MutableStateFlow::new("second_init");
    let use_second = mutable_state_of(false);
    let read = use_second.handle();
    let seen: Rc<Cell<&'static str>> = Rc::new(Cell::new(""));
    let (f1, f2, held, out) = (flow1.clone(), flow2.clone(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let chosen = if read.get() { &f2 } else { &f1 };
        out.set(chosen.collect_as_state_with_lifecycle_with(&held, Started).get());
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), "first_init"));

    // Swap to flow2 while still Created (< Started): as AndroidX's unkeyed `remember`, state keeps
    // its current value until the new LaunchedEffect can collect at >= Started.
    use_second.set(true);
    ui.run_on_idle(|_| assert_eq!(seen.get(), "first_init"));

    flow2.set_value("second_updated");
    ui.run_on_idle(|_| assert_eq!(seen.get(), "first_init"));

    owner.set_current_state(Started);
    ui.run_on_idle(|_| assert_eq!(seen.get(), "second_updated"));
}

#[test]
fn snapshot_flow_collected_with_lifecycle_watches_only_while_at_least_min_state() {
    let owner = TestLifecycleOwner::new(Created);
    let manager = SnapshotFlowManager::new();
    let source = mutable_state_of(1);
    let read_source = source.handle();
    let seen: Rc<Cell<i32>> = Rc::default();
    let (mgr, held, out) = (manager.clone(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let mgr = mgr.clone();
        out.set(
            collect_as_state_with_lifecycle_with((), 0, &held, Started, move || {
                snapshot_flow_with(&mgr, move || read_source.get())
            })
            .get(),
        );
    });
    ui.wait_for_idle();
    assert_eq!(seen.get(), 0);
    assert_eq!(manager.watched_states(), 0, "not watching while Created");

    Snapshot::with_mutable_snapshot(|| source.set(5));
    ui.wait_for_idle();
    assert_eq!(seen.get(), 0);

    // Move to Resumed (>= Started): subscribes and watches.
    owner.set_current_state(Resumed);
    ui.wait_for_idle();
    assert_eq!(seen.get(), 5);
    assert_eq!(manager.watched_states(), 1);

    // Pause to Started (still >= Started): keeps watching without unwatching!
    owner.set_current_state(Started);
    ui.wait_for_idle();
    assert_eq!(manager.watched_states(), 1);
    Snapshot::with_mutable_snapshot(|| source.set(8));
    ui.wait_for_idle();
    assert_eq!(seen.get(), 8);

    // Stop to Created (< Started): drops SnapshotFlow and unwatches!
    owner.set_current_state(Created);
    ui.wait_for_idle();
    assert_eq!(manager.watched_states(), 0);
    Snapshot::with_mutable_snapshot(|| source.set(13));
    ui.wait_for_idle();
    assert_eq!(seen.get(), 8);

    // Start again: catches up to 13!
    owner.set_current_state(Started);
    ui.wait_for_idle();
    assert_eq!(seen.get(), 13);
    assert_eq!(manager.watched_states(), 1);
}

#[composable]
fn CappedCollector(
    flow: StateFlow<i32>,
    cap: LifecycleState,
    parent: Lifecycle,
    out: Rc<Cell<i32>>,
) {
    let child = remember_lifecycle_owner!(max_lifecycle = cap, parent = Some(parent));
    out.set(flow.collect_as_state_with_lifecycle_with(&child, Started).get());
}

#[test]
fn collect_as_state_with_lifecycle_follows_a_remembered_child_lifecycle_owner_cap() {
    let parent = TestLifecycleOwner::new(Resumed);
    let flow = MutableStateFlow::new(1);
    let cap = mutable_state_of(Started);
    let read_cap = cap.handle();
    let seen: Rc<Cell<i32>> = Rc::default();
    let (given, parent_lc, out) = (flow.as_state_flow(), parent.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        CappedCollector!(given.clone(), read_cap.get(), parent_lc.clone(), out.clone());
    });
    ui.wait_for_idle();
    assert_eq!(seen.get(), 1);

    flow.set_value(2);
    ui.run_on_idle(|_| assert_eq!(seen.get(), 2));

    // Cap the child lifecycle to Created: stops collecting while parent remains Resumed.
    cap.set(Created);
    ui.wait_for_idle();
    flow.set_value(3);
    ui.run_on_idle(|_| assert_eq!(seen.get(), 2));

    // Raise cap back to Resumed: catches up to 3.
    cap.set(Resumed);
    ui.run_on_idle(|_| assert_eq!(seen.get(), 3));
}

#[test]
fn lifecycle_as_state_and_collected_state_flow_converge_on_the_same_idle_turn() {
    let owner = TestLifecycleOwner::new(Created);
    let flow = MutableStateFlow::new(10);
    let seen: Rc<Cell<(LifecycleState, i32)>> = Rc::new(Cell::new((Initialized, 0)));
    let (given, held, out) = (flow.as_state_flow(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let lc_state = held.as_state().get();
        let value = given.collect_as_state_with_lifecycle_with(&held, Started).get();
        out.set((lc_state, value));
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), (Created, 10)));

    flow.set_value(25);
    owner.set_current_state(Started);
    ui.run_on_idle(|_| assert_eq!(seen.get(), (Started, 25)));
}

#[test]
#[should_panic(expected = "CompositionLocal LocalLifecycleOwner not present")]
fn default_collect_as_state_with_lifecycle_panics_when_no_local_owner_is_present() {
    use compose_runtime::platform::{Applier, NodeRef};
    struct NoopApplier;
    impl Applier for NoopApplier {
        type Node = ();
        fn root(&self) -> NodeRef {
            NodeRef::new(0)
        }
        fn insert(&mut self, _: NodeRef, _: usize, (): ()) -> NodeRef {
            NodeRef::new(1)
        }
        fn remove(&mut self, _: NodeRef, _: usize, _: usize) {}
        fn move_nodes(&mut self, _: NodeRef, _: usize, _: usize, _: usize) {}
        fn release(&mut self, _: NodeRef) {}
    }

    let flow = MutableStateFlow::new(1).as_state_flow();
    let composition = Composition::new(NoopApplier);
    composition.set_content(move || {
        let _ = flow.collect_as_state_with_lifecycle();
    });
}
