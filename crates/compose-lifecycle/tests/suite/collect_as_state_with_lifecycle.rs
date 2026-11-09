//! `collect_as_state_with_lifecycle` against AndroidX's `CollectAsStateWithLifecycleTests`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};

use LifecycleState::*;
use compose_lifecycle::*;
use compose_runtime::flow::{MutableStateFlow, StateFlow, Stream};
use compose_runtime::*;
use compose_test::*;

/// A multi-subscriber hot stream with no replay, as `MutableSharedFlow` with buffer capacity.
#[derive(Clone, Default)]
struct SharedFlow<T> {
    subscribers: Rc<RefCell<Vec<Weak<Subscriber<T>>>>>,
}

struct Subscriber<T> {
    items: RefCell<VecDeque<T>>,
    waker: RefCell<Option<Waker>>,
}

struct SharedSubscription<T> {
    subscriber: Rc<Subscriber<T>>,
}

impl<T> Unpin for SharedSubscription<T> {}

impl<T: Clone> SharedFlow<T> {
    fn subscribe(&self) -> SharedSubscription<T> {
        let subscriber =
            Rc::new(Subscriber { items: RefCell::new(VecDeque::new()), waker: RefCell::new(None) });
        let mut subscribers = self.subscribers.borrow_mut();
        subscribers.retain(|entry| entry.strong_count() > 0);
        subscribers.push(Rc::downgrade(&subscriber));
        SharedSubscription { subscriber }
    }

    fn try_emit(&self, value: T) -> bool {
        let live: Vec<Rc<Subscriber<T>>> = {
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
        true
    }

    fn active_subscribers(&self) -> usize {
        self.subscribers.borrow().iter().filter(|entry| entry.strong_count() > 0).count()
    }
}

impl<T> Stream for SharedSubscription<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        if let Some(value) = self.subscriber.items.borrow_mut().pop_front() {
            return Poll::Ready(Some(value));
        }
        *self.subscriber.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }
}

fn with_lifecycle_owner(state: LifecycleState, content: impl FnOnce()) {
    let owner = remember(|| TestLifecycleOwner::new(state));
    CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(owner.lifecycle()), content);
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_flowState_getsInitialValue
#[test]
fn a_flow_collected_as_state_with_lifecycle_starts_with_its_initial_value() {
    let shared: SharedFlow<&'static str> = SharedFlow::default();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, out) = (shared, seen.clone());
    let mut ui = compose_test(move || {
        let (flow, out) = (flow.clone(), out.clone());
        with_lifecycle_owner(Created, move || {
            out.set(Some(collect_as_state_with_lifecycle((), "0", move || flow.subscribe()).get()));
        });
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_stateFlowState_getsInitialValue
#[test]
fn a_state_flow_collected_as_state_with_lifecycle_starts_with_its_current_value() {
    let state_flow = MutableStateFlow::new("0").as_state_flow();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let out = seen.clone();
    let mut ui = compose_test(move || {
        let (flow, out) = (state_flow.clone(), out.clone());
        with_lifecycle_owner(Created, move || {
            out.set(Some(flow.collect_as_state_with_lifecycle().get()));
        });
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_flowState_getsSubsequentFlowEmissions
#[test]
fn a_flow_collected_as_state_with_lifecycle_receives_emissions_while_resumed() {
    let shared: SharedFlow<&'static str> = SharedFlow::default();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, out) = (shared.clone(), seen.clone());
    let mut ui = compose_test(move || {
        let (flow, out) = (flow.clone(), out.clone());
        with_lifecycle_owner(Resumed, move || {
            out.set(Some(collect_as_state_with_lifecycle((), "0", move || flow.subscribe()).get()));
        });
    });
    assert!(shared.try_emit("1"));
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("1")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_stateFlowState_getsSubsequentFlowEmissions
#[test]
fn a_state_flow_collected_as_state_with_lifecycle_receives_updates_while_resumed() {
    let state_flow = MutableStateFlow::new("0");
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, out) = (state_flow.clone(), seen.clone());
    let mut ui = compose_test(move || {
        let (flow, out) = (flow.clone(), out.clone());
        with_lifecycle_owner(Resumed, move || {
            out.set(Some(flow.collect_as_state_with_lifecycle().get()));
        });
    });
    state_flow.set_value("1");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("1")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_flowState_doesNotGetEmissionsBelowMinState
#[test]
fn a_flow_does_not_update_state_below_min_active_state() {
    let owner = TestLifecycleOwner::new(Created);
    let backing = MutableStateFlow::new("initialValue");
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, held, out) = (backing.as_state_flow(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        out.set(Some(
            collect_as_state_with_lifecycle_with((), "0", &held, Started, flow.clone()).get(),
        ));
    });
    backing.set_value("1");
    backing.set_value("2");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_stateFlowState_doesNotGetEmissionsBelowMinState
#[test]
fn a_state_flow_does_not_update_state_below_min_active_state() {
    let owner = TestLifecycleOwner::new(Created);
    let state_flow = MutableStateFlow::new("0");
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, held, out) = (state_flow.clone(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        out.set(Some(flow.collect_as_state_with_lifecycle_with(&held, Started).get()));
    });
    state_flow.set_value("1");
    state_flow.set_value("2");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_flowState_getsEmissionsWhenAboveMinState
#[test]
fn a_flow_receives_emissions_once_lifecycle_reaches_min_active_state() {
    let owner = TestLifecycleOwner::new(Created);
    let shared: SharedFlow<&'static str> = SharedFlow::default();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, held, out) = (shared.clone(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let flow = flow.clone();
        out.set(Some(
            collect_as_state_with_lifecycle_with((), "0", &held, Started, move || flow.subscribe())
                .get(),
        ));
    });
    assert!(shared.try_emit("1"));
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));

    owner.set_current_state(Resumed);
    ui.wait_for_idle();
    assert!(shared.try_emit("2"));
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("2")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_stateFlowState_getsEmissionsWhenAboveMinState
#[test]
fn a_state_flow_receives_emissions_while_above_min_active_state() {
    let owner = TestLifecycleOwner::new(Resumed);
    let state_flow = MutableStateFlow::new("0");
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, held, out) = (state_flow.clone(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        out.set(Some(flow.collect_as_state_with_lifecycle_with(&held, Started).get()));
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));

    state_flow.set_value("1");
    state_flow.set_value("2");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("2")));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/CollectAsStateWithLifecycleTests.kt, test_stateFlowState_getsLastEmissionWhenLifecycleIsAboveMin
#[test]
fn a_state_flow_catches_up_to_latest_value_when_lifecycle_rises_above_min_state() {
    let owner = TestLifecycleOwner::new(Created);
    let state_flow = MutableStateFlow::new("0");
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, held, out) = (state_flow.clone(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        out.set(Some(flow.collect_as_state_with_lifecycle_with(&held, Started).get()));
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));

    state_flow.set_value("1");
    state_flow.set_value("2");
    owner.set_current_state(Resumed);
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("2")));
}

// ---- Re-collection & disposal edge cases -----------------------------------------------------

#[test]
fn replacing_a_state_flow_switches_to_the_new_flow() {
    let owner = TestLifecycleOwner::new(Resumed);
    let (flow1, flow2) = (MutableStateFlow::new("one"), MutableStateFlow::new("two"));
    let (given1, given2) = (flow1.as_state_flow(), flow2.as_state_flow());
    let which = mutable_state_of(1);
    let read = which.handle();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (held, out) = (owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let active = if read.get() == 1 { &given1 } else { &given2 };
        out.set(Some(active.collect_as_state_with_lifecycle_with(&held, Started).get()));
    });
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("one")));

    which.set(2);
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("two")));

    flow1.set_value("stale");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("two")));

    flow2.set_value("fresh");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("fresh")));
}

#[test]
fn replacing_a_stream_key_restarts_collection_keeping_the_current_value_until_emission() {
    let owner = TestLifecycleOwner::new(Resumed);
    let (shared1, shared2): (SharedFlow<&'static str>, SharedFlow<&'static str>) =
        Default::default();
    let (given1, given2) = (shared1.clone(), shared2.clone());
    let which = mutable_state_of(1);
    let read = which.handle();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (held, out) = (owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let key = read.get();
        let chosen = if key == 1 { given1.clone() } else { given2.clone() };
        out.set(Some(
            collect_as_state_with_lifecycle_with(key, "init", &held, Started, move || {
                chosen.subscribe()
            })
            .get(),
        ));
    });
    shared1.try_emit("from_1");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("from_1")));
    assert_eq!(shared1.active_subscribers(), 1);
    assert_eq!(shared2.active_subscribers(), 0);

    which.set(2);
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("from_1")));
    assert_eq!(shared1.active_subscribers(), 0);
    assert_eq!(shared2.active_subscribers(), 1);

    shared1.try_emit("ignored");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("from_1")));

    shared2.try_emit("from_2");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("from_2")));
}

#[test]
fn replacing_the_lifecycle_moves_the_observer_to_the_new_owner() {
    let owner1 = TestLifecycleOwner::new(Created);
    let owner2 = TestLifecycleOwner::new(Resumed);
    let state_flow = MutableStateFlow::new("0");
    let use_second = mutable_state_of(false);
    let read = use_second.handle();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, first, second, out) =
        (state_flow.clone(), owner1.lifecycle(), owner2.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let lifecycle = if read.get() { &second } else { &first };
        out.set(Some(flow.collect_as_state_with_lifecycle_with(lifecycle, Started).get()));
    });
    state_flow.set_value("1");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));
    assert_eq!(owner1.observer_count(), 1);
    assert_eq!(owner2.observer_count(), 0);

    use_second.set(true);
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("1")));
    assert_eq!(owner1.observer_count(), 0);
    assert_eq!(owner2.observer_count(), 1);
}

#[test]
fn providing_a_new_local_lifecycle_owner_moves_default_collection_to_it() {
    let owner1 = TestLifecycleOwner::new(Created);
    let owner2 = TestLifecycleOwner::new(Resumed);
    let state_flow = MutableStateFlow::new("0");
    let use_second = mutable_state_of(false);
    let read = use_second.handle();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, first, second, out) =
        (state_flow.clone(), owner1.lifecycle(), owner2.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let provided = if read.get() { second.clone() } else { first.clone() };
        let (flow, out) = (flow.clone(), out.clone());
        CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(provided), move || {
            out.set(Some(flow.collect_as_state_with_lifecycle().get()));
        });
    });
    state_flow.set_value("1");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));
    assert_eq!(owner1.observer_count(), 1);
    assert_eq!(owner2.observer_count(), 0);

    use_second.set(true);
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("1")));
    assert_eq!(owner1.observer_count(), 0);
    assert_eq!(owner2.observer_count(), 1);
}

#[test]
fn changing_min_active_state_starts_and_stops_collection() {
    let owner = TestLifecycleOwner::new(Started);
    let shared: SharedFlow<&'static str> = SharedFlow::default();
    let min_state = mutable_state_of(Resumed);
    let read = min_state.handle();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, held, out) = (shared.clone(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        let flow = flow.clone();
        out.set(Some(
            collect_as_state_with_lifecycle_with((), "0", &held, read.get(), move || {
                flow.subscribe()
            })
            .get(),
        ));
    });
    ui.wait_for_idle();
    assert_eq!(shared.active_subscribers(), 0);

    min_state.set(Started);
    ui.wait_for_idle();
    assert_eq!(shared.active_subscribers(), 1);
    shared.try_emit("active");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("active")));

    min_state.set(Resumed);
    ui.wait_for_idle();
    assert_eq!(shared.active_subscribers(), 0);
    assert_eq!(owner.observer_count(), 1);
}

#[composable]
fn WatchedFlow(
    flow: StateFlow<&'static str>,
    owner: Lifecycle,
    out: Rc<Cell<Option<&'static str>>>,
) {
    out.set(Some(flow.collect_as_state_with_lifecycle_with(owner, Started).get()));
}

#[test]
fn disposing_collected_state_removes_its_lifecycle_observer() {
    let owner = TestLifecycleOwner::new(Resumed);
    let state_flow = MutableStateFlow::new("0");
    let visible = mutable_state_of(true);
    let read = visible.handle();
    let seen: Rc<Cell<Option<&'static str>>> = Rc::default();
    let (flow, held, out) = (state_flow.as_state_flow(), owner.lifecycle(), seen.clone());
    let mut ui = compose_test(move || {
        if read.get() {
            WatchedFlow!(flow.clone(), held.clone(), out.clone());
        }
    });
    ui.wait_for_idle();
    assert_eq!(owner.observer_count(), 1);

    visible.set(false);
    ui.wait_for_idle();
    assert_eq!(owner.observer_count(), 0);
    state_flow.set_value("after_dispose");
    ui.run_on_idle(|_| assert_eq!(seen.get(), Some("0")));
}

#[test]
fn collecting_a_state_flow_with_lifecycle_clones_its_value_only_on_initial_composition() {
    #[derive(PartialEq)]
    struct Counted(Rc<Cell<u32>>);
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Counted(self.0.clone())
        }
    }

    let owner = TestLifecycleOwner::new(Created);
    let clones = Rc::new(Cell::new(0));
    let state_flow = MutableStateFlow::new(Counted(clones.clone())).as_state_flow();
    let unrelated = mutable_state_of(0);
    let read = unrelated.handle();
    let held = owner.lifecycle();
    let mut ui = compose_test(move || {
        let _ = state_flow.collect_as_state_with_lifecycle_with(&held, Started);
        let _ = read.get();
    });
    ui.wait_for_idle();
    let initial_clones = clones.get();
    unrelated.set(1);
    ui.wait_for_idle();
    assert_eq!(clones.get(), initial_clones);
}

#[test]
#[should_panic(expected = "repeatOnLifecycle cannot start work with the INITIALIZED")]
fn collect_as_state_with_lifecycle_refuses_the_initialized_state() {
    let owner = TestLifecycleOwner::default();
    let state_flow = MutableStateFlow::new(0);
    let held = owner.lifecycle();
    let _ui = compose_test(move || {
        let _ = state_flow.collect_as_state_with_lifecycle_with(&held, Initialized);
    });
}
