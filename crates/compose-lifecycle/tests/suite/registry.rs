//! `LifecycleRegistry` against AndroidX's `CommonLifecycleRegistryTest`: the order observers hear
//! events in, added and removed during a dispatch.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use LifecycleEvent::*;
use compose_lifecycle::*;

/// The events heard, by observer number.
type Events = Rc<RefCell<Vec<(u32, LifecycleEvent)>>>;

/// An observer recording its events, then running what's set for one.
struct TestObserver {
    number: u32,
    events: Events,
    actions: RefCell<Vec<(LifecycleEvent, Rc<dyn Fn()>)>>,
    counts: Cell<[u32; 6]>,
}

impl LifecycleObserver for TestObserver {
    fn on_state_changed(&self, _: &Lifecycle, event: LifecycleEvent) {
        self.events.borrow_mut().push((self.number, event));
        let index = [OnCreate, OnStart, OnResume, OnPause, OnStop, OnDestroy]
            .iter()
            .position(|known| *known == event)
            .unwrap();
        let mut counts = self.counts.get();
        counts[index] += 1;
        self.counts.set(counts);
        let actions: Vec<_> = self
            .actions
            .borrow()
            .iter()
            .filter(|(on, _)| *on == event)
            .map(|(_, action)| action.clone())
            .collect();
        for action in actions {
            action();
        }
    }
}

impl TestObserver {
    fn count(&self, event: LifecycleEvent) -> u32 {
        let index = [OnCreate, OnStart, OnResume, OnPause, OnStop, OnDestroy]
            .iter()
            .position(|known| *known == event)
            .unwrap();
        self.counts.get()[index]
    }
}

struct Fixture {
    registry: LifecycleRegistry,
    events: Events,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture { registry: LifecycleRegistry::new(), events: Rc::default() }
    }

    fn observer(&self, number: u32) -> Rc<TestObserver> {
        Rc::new(TestObserver {
            number,
            events: self.events.clone(),
            actions: RefCell::default(),
            counts: Cell::new([0; 6]),
        })
    }

    fn dispatch(&self, event: LifecycleEvent) {
        self.registry.handle_lifecycle_event(event);
    }

    fn fully_initialize(&self) {
        self.dispatch(OnCreate);
        self.dispatch(OnStart);
        self.dispatch(OnResume);
    }

    fn add(&self, observer: &Rc<TestObserver>) {
        self.registry.add_observer(observer.clone());
    }

    fn events(&self) -> Vec<(u32, LifecycleEvent)> {
        self.events.borrow().clone()
    }

    /// Whether `expected` come in this order among the events.
    fn assert_in_order(&self, expected: &[(u32, LifecycleEvent)]) {
        let events = self.events();
        let mut rest = events.iter();
        for wanted in expected {
            assert!(rest.any(|event| event == wanted), "{wanted:?} not in order in {events:?}");
        }
    }
}

fn action(observer: &Rc<TestObserver>, on: LifecycleEvent, action: impl Fn() + 'static) {
    observer.actions.borrow_mut().push((on, Rc::new(action)));
}

/// The observer as what `add_observer` and `remove_observer` take.
fn erased(observer: &Rc<TestObserver>) -> Rc<dyn LifecycleObserver> {
    observer.clone()
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, getCurrentState
#[test]
fn an_event_moves_the_state() {
    let f = Fixture::new();
    f.dispatch(OnResume);
    assert_eq!(f.registry.current_state(), LifecycleState::Resumed);
    f.dispatch(OnDestroy);
    assert_eq!(f.registry.current_state(), LifecycleState::Destroyed);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, moveInitializedToDestroyed
#[test]
#[should_panic(expected = "State must be at least 'CREATED' to be moved to 'DESTROYED'")]
fn initialized_cannot_be_destroyed() {
    Fixture::new().dispatch(OnDestroy);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, moveDestroyedToAny
#[test]
#[should_panic(expected = "State is 'DESTROYED' and cannot be moved to `Created`")]
fn destroyed_cannot_be_left() {
    let f = Fixture::new();
    f.dispatch(OnCreate);
    f.dispatch(OnDestroy);
    f.dispatch(OnCreate);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, setCurrentState
#[test]
fn the_state_can_be_set() {
    let f = Fixture::new();
    f.registry.set_current_state(LifecycleState::Resumed);
    assert_eq!(f.registry.current_state(), LifecycleState::Resumed);
    f.registry.set_current_state(LifecycleState::Destroyed);
    assert_eq!(f.registry.current_state(), LifecycleState::Destroyed);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, addRemove
#[test]
fn observers_are_added_and_removed() {
    let f = Fixture::new();
    let observer = f.observer(1);
    f.add(&observer);
    assert_eq!(f.registry.observer_count(), 1);
    f.registry.remove_observer(&erased(&observer));
    assert_eq!(f.registry.observer_count(), 0);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, addAndObserve
#[test]
fn an_observer_hears_each_event_once() {
    let f = Fixture::new();
    let observer = f.observer(1);
    f.add(&observer);
    f.dispatch(OnCreate);
    assert_eq!(observer.count(OnCreate), 1);
    assert_eq!(f.events(), [(1, OnCreate)]);
    f.dispatch(OnCreate);
    assert_eq!(observer.count(OnCreate), 1);
    f.dispatch(OnStart);
    assert_eq!(observer.count(OnStop), 0);
    f.dispatch(OnStop);
    assert_eq!(observer.count(OnStop), 1);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, add2RemoveOne
#[test]
fn an_observer_removed_hears_no_more() {
    let f = Fixture::new();
    let (first, second, third) = (f.observer(1), f.observer(2), f.observer(3));
    f.add(&first);
    f.add(&second);
    f.add(&third);
    f.dispatch(OnCreate);
    assert_eq!((first.count(OnCreate), second.count(OnCreate), third.count(OnCreate)), (1, 1, 1));
    f.registry.remove_observer(&erased(&second));
    f.dispatch(OnStart);
    assert_eq!((first.count(OnStart), second.count(OnStart), third.count(OnStart)), (1, 0, 1));
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, removeWhileTraversing
#[test]
fn an_observer_removed_during_a_dispatch_is_not_told() {
    let f = Fixture::new();
    let (first, second) = (f.observer(1), f.observer(2));
    let (registry, removed) = (f.registry.clone(), erased(&second));
    action(&first, OnCreate, move || registry.remove_observer(&removed));
    f.add(&first);
    f.add(&second);
    f.dispatch(OnCreate);
    assert_eq!((first.count(OnCreate), second.count(OnCreate)), (1, 0));
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, constructionOrder
#[test]
fn an_observer_added_late_hears_the_events_it_missed_in_order() {
    let f = Fixture::new();
    f.fully_initialize();
    let observer = f.observer(1);
    f.add(&observer);
    assert_eq!(f.events(), [(1, OnCreate), (1, OnStart), (1, OnResume)]);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, constructionDestruction1
#[test]
fn a_pause_during_catching_up_stops_it_short() {
    let f = Fixture::new();
    f.fully_initialize();
    let observer = f.observer(1);
    let registry = f.registry.clone();
    action(&observer, OnStart, move || registry.handle_lifecycle_event(OnPause));
    f.add(&observer);
    assert_eq!(f.events(), [(1, OnCreate), (1, OnStart)]);
    assert_eq!(observer.count(OnResume), 0);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, constructionDestruction2
#[test]
fn a_destruction_during_catching_up_brings_it_down() {
    let f = Fixture::new();
    f.fully_initialize();
    let observer = f.observer(1);
    let registry = f.registry.clone();
    action(&observer, OnStart, move || {
        registry.handle_lifecycle_event(OnPause);
        registry.handle_lifecycle_event(OnStop);
        registry.handle_lifecycle_event(OnDestroy);
    });
    f.add(&observer);
    assert_eq!(f.events(), [(1, OnCreate), (1, OnStart), (1, OnStop), (1, OnDestroy)]);
    assert_eq!(observer.count(OnResume), 0);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, twoObserversChangingState
#[test]
fn a_state_change_during_a_dispatch_reaches_every_observer() {
    let f = Fixture::new();
    let (first, second) = (f.observer(1), f.observer(2));
    let registry = f.registry.clone();
    action(&first, OnCreate, move || registry.handle_lifecycle_event(OnStart));
    f.add(&first);
    f.add(&second);
    f.dispatch(OnCreate);
    assert_eq!((first.count(OnCreate), second.count(OnCreate)), (1, 1));
    assert_eq!((first.count(OnStart), second.count(OnStart)), (1, 1));
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, addDuringTraversing
#[test]
fn an_observer_added_during_a_dispatch_waits_for_those_before_it() {
    let f = Fixture::new();
    let (first, second, third) = (f.observer(1), f.observer(2), f.observer(3));
    let (registry, added) = (f.registry.clone(), erased(&third));
    action(&first, OnStart, move || registry.add_observer(added.clone()));
    f.add(&first);
    f.add(&second);
    f.dispatch(OnCreate);
    f.dispatch(OnStart);
    assert_eq!(
        f.events(),
        [(1, OnCreate), (2, OnCreate), (1, OnStart), (3, OnCreate), (2, OnStart), (3, OnStart)]
    );
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, addDuringAddition
#[test]
fn an_observer_added_during_an_addition_comes_after() {
    let f = Fixture::new();
    let (first, second, third) = (f.observer(1), f.observer(2), f.observer(3));
    let (registry, added) = (f.registry.clone(), erased(&third));
    action(&second, OnCreate, move || registry.add_observer(added.clone()));
    let (registry, added) = (f.registry.clone(), erased(&second));
    action(&first, OnResume, move || registry.add_observer(added.clone()));
    f.add(&first);
    f.dispatch(OnCreate);
    f.dispatch(OnStart);
    f.dispatch(OnResume);
    assert_eq!(
        f.events(),
        [
            (1, OnCreate),
            (1, OnStart),
            (1, OnResume),
            (2, OnCreate),
            (2, OnStart),
            (2, OnResume),
            (3, OnCreate),
            (3, OnStart),
            (3, OnResume)
        ]
    );
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, subscribeToDead
#[test]
fn an_observer_added_to_a_destroyed_lifecycle_hears_nothing() {
    let f = Fixture::new();
    f.dispatch(OnCreate);
    let first = f.observer(1);
    f.add(&first);
    assert_eq!(first.count(OnCreate), 1);
    f.dispatch(OnDestroy);
    assert_eq!(first.count(OnDestroy), 1);
    let second = f.observer(2);
    f.add(&second);
    assert_eq!(second.count(OnCreate), 0);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, downEvents
#[test]
fn down_events_go_newest_first() {
    let f = Fixture::new();
    f.fully_initialize();
    let (first, second) = (f.observer(1), f.observer(2));
    f.add(&first);
    f.add(&second);
    f.dispatch(OnPause);
    f.assert_in_order(&[(2, OnPause), (1, OnPause)]);
    f.dispatch(OnStop);
    f.assert_in_order(&[(2, OnStop), (1, OnStop)]);
    f.dispatch(OnDestroy);
    f.assert_in_order(&[(2, OnDestroy), (1, OnDestroy)]);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, downEventsAddition
#[test]
fn an_observer_added_during_a_down_event_goes_only_as_far_as_the_one_adding_it() {
    let f = Fixture::new();
    f.dispatch(OnCreate);
    f.dispatch(OnStart);
    let (first, second, third) = (f.observer(1), f.observer(2), f.observer(3));
    let (registry, added) = (f.registry.clone(), erased(&third));
    action(&second, OnStop, move || registry.add_observer(added.clone()));
    f.add(&first);
    f.add(&second);
    f.dispatch(OnStop);
    f.assert_in_order(&[(2, OnStop), (3, OnCreate), (1, OnStop)]);
    f.dispatch(OnDestroy);
    f.assert_in_order(&[(3, OnDestroy), (2, OnDestroy), (1, OnDestroy)]);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, downEventsRemoveAll
#[test]
fn removing_every_observer_during_a_down_event_ends_it() {
    let f = Fixture::new();
    f.fully_initialize();
    let (first, second, third) = (f.observer(1), f.observer(2), f.observer(3));
    let registry = f.registry.clone();
    let (one, two, three) = (erased(&first), erased(&second), erased(&third));
    action(&second, OnStop, move || {
        registry.remove_observer(&three);
        registry.remove_observer(&two);
        registry.remove_observer(&one);
        assert_eq!(registry.observer_count(), 0);
    });
    f.add(&first);
    f.add(&second);
    f.add(&third);
    f.dispatch(OnPause);
    f.assert_in_order(&[(3, OnPause), (2, OnPause), (1, OnPause)]);
    assert_eq!((third.count(OnPause), second.count(OnPause), first.count(OnPause)), (1, 1, 1));
    f.dispatch(OnStop);
    f.assert_in_order(&[(3, OnStop), (2, OnStop)]);
    assert_eq!(first.count(OnStop), 0);
    f.dispatch(OnPause);
    assert_eq!((third.count(OnPause), second.count(OnPause), first.count(OnPause)), (1, 1, 1));
}

/// `first` on start removes itself and adds the next two.
fn removes_itself_and_adds(f: &Fixture) -> (Rc<TestObserver>, Rc<TestObserver>, Rc<TestObserver>) {
    let (first, second, third) = (f.observer(1), f.observer(2), f.observer(3));
    let registry = f.registry.clone();
    let (itself, two, three) = (Rc::downgrade(&first), erased(&second), erased(&third));
    action(&first, OnStart, move || {
        let itself: Rc<dyn LifecycleObserver> = itself.upgrade().unwrap();
        registry.remove_observer(&itself);
        assert_eq!(registry.observer_count(), 0);
        registry.add_observer(two.clone());
        registry.add_observer(three.clone());
    });
    (first, second, third)
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, deadParentInAddition
#[test]
fn observers_added_by_one_removed_during_its_addition_wait_for_each_other() {
    let f = Fixture::new();
    f.fully_initialize();
    let (first, _second, _third) = removes_itself_and_adds(&f);
    f.add(&first);
    assert_eq!(
        f.events(),
        [
            (1, OnCreate),
            (1, OnStart),
            (2, OnCreate),
            (3, OnCreate),
            (2, OnStart),
            (2, OnResume),
            (3, OnStart),
            (3, OnResume)
        ]
    );
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, deadParentWhileTraversing
#[test]
fn observers_added_by_one_removed_during_a_dispatch_wait_for_each_other() {
    let f = Fixture::new();
    let (first, _second, _third) = removes_itself_and_adds(&f);
    f.add(&first);
    f.dispatch(OnCreate);
    f.dispatch(OnStart);
    assert_eq!(
        f.events(),
        [(1, OnCreate), (1, OnStart), (2, OnCreate), (3, OnCreate), (2, OnStart), (3, OnStart)]
    );
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, removeCascade
#[test]
fn removals_cascading_through_additions_keep_the_order() {
    let f = Fixture::new();
    let (first, second, third, fourth) =
        (f.observer(1), f.observer(2), f.observer(3), f.observer(4));
    let registry = f.registry.clone();
    let itself = Rc::downgrade(&second);
    action(&second, OnStart, move || {
        let itself: Rc<dyn LifecycleObserver> = itself.upgrade().unwrap();
        registry.remove_observer(&itself);
    });
    let registry = f.registry.clone();
    let itself = Rc::downgrade(&first);
    let (two, three, four) = (erased(&second), erased(&third), erased(&fourth));
    action(&first, OnResume, move || {
        let itself: Rc<dyn LifecycleObserver> = itself.upgrade().unwrap();
        registry.remove_observer(&itself);
        registry.add_observer(two.clone());
        registry.add_observer(three.clone());
        registry.add_observer(four.clone());
    });
    f.fully_initialize();
    f.add(&first);
    assert_eq!(
        f.events(),
        [
            (1, OnCreate),
            (1, OnStart),
            (1, OnResume),
            (2, OnCreate),
            (2, OnStart),
            (3, OnCreate),
            (3, OnStart),
            (4, OnCreate),
            (4, OnStart),
            (3, OnResume),
            (4, OnResume)
        ]
    );
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, changeStateDuringDescending
#[test]
fn a_resume_during_a_pause_turns_it_around() {
    let f = Fixture::new();
    let (first, second) = (f.observer(1), f.observer(2));
    let (registry, added) = (f.registry.clone(), erased(&second));
    action(&first, OnPause, move || {
        registry.handle_lifecycle_event(OnResume);
        registry.add_observer(added.clone());
    });
    f.fully_initialize();
    f.add(&first);
    f.dispatch(OnPause);
    f.assert_in_order(&[(1, OnPause), (2, OnCreate), (2, OnStart), (1, OnResume), (2, OnResume)]);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, siblingLimitationCheck
#[test]
fn an_observer_is_never_ahead_of_the_one_before_it() {
    let f = Fixture::new();
    f.fully_initialize();
    let (first, second, third) = (f.observer(1), f.observer(2), f.observer(3));
    let (registry, added) = (f.registry.clone(), erased(&second));
    action(&first, OnStart, move || registry.add_observer(added.clone()));
    let (registry, added) = (f.registry.clone(), erased(&third));
    action(&first, OnResume, move || registry.add_observer(added.clone()));
    f.add(&first);
    assert_eq!(
        f.events(),
        [
            (1, OnCreate),
            (1, OnStart),
            (2, OnCreate),
            (1, OnResume),
            (3, OnCreate),
            (2, OnStart),
            (2, OnResume),
            (3, OnStart),
            (3, OnResume)
        ]
    );
}

const SIBLINGS_AFTER_REMOVAL: [(u32, LifecycleEvent); 10] = [
    (1, OnCreate),
    (1, OnStart),
    (2, OnCreate),
    (1, OnResume),
    (3, OnCreate),
    (3, OnStart),
    (4, OnCreate),
    (4, OnStart),
    (3, OnResume),
    (4, OnResume),
];

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, siblingRemovalLimitationCheck1
#[test]
fn removing_the_sibling_limiting_observers_lets_them_on_before_they_are_added() {
    let f = Fixture::new();
    f.fully_initialize();
    let (first, second, third, fourth) =
        (f.observer(1), f.observer(2), f.observer(3), f.observer(4));
    let (registry, added) = (f.registry.clone(), erased(&second));
    action(&first, OnStart, move || registry.add_observer(added.clone()));
    let registry = f.registry.clone();
    let (two, three, four) = (erased(&second), erased(&third), erased(&fourth));
    action(&first, OnResume, move || {
        registry.remove_observer(&two);
        registry.add_observer(three.clone());
        registry.add_observer(four.clone());
    });
    f.add(&first);
    assert_eq!(f.events(), SIBLINGS_AFTER_REMOVAL);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, siblingRemovalLimitationCheck2
#[test]
fn removing_the_sibling_limiting_observers_lets_them_on_during_their_addition() {
    let f = Fixture::new();
    f.fully_initialize();
    let (first, second, third, fourth) =
        (f.observer(1), f.observer(2), f.observer(3), f.observer(4));
    let (registry, removed) = (f.registry.clone(), erased(&second));
    action(&third, OnCreate, move || registry.remove_observer(&removed));
    let (registry, added) = (f.registry.clone(), erased(&second));
    action(&first, OnStart, move || registry.add_observer(added.clone()));
    let registry = f.registry.clone();
    let (three, four) = (erased(&third), erased(&fourth));
    action(&first, OnResume, move || {
        registry.add_observer(three.clone());
        registry.add_observer(four.clone());
    });
    f.add(&first);
    assert_eq!(f.events(), SIBLINGS_AFTER_REMOVAL);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/CommonLifecycleRegistryTest.kt, sameObserverReAddition
#[test]
fn an_observer_removed_can_be_added_again() {
    let f = Fixture::new();
    let observer = f.observer(1);
    f.add(&observer);
    f.registry.remove_observer(&erased(&observer));
    f.add(&observer);
    f.dispatch(OnCreate);
    assert_eq!(observer.count(OnCreate), 1);
}
