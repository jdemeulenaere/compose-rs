//! `DefaultLifecycleObserver` against AndroidX's `DefaultLifecycleObserverTest`, and the events'
//! and states' relations.

use std::cell::RefCell;
use std::rc::Rc;

use LifecycleEvent::*;
use compose_lifecycle::*;

/// Records every method called.
#[derive(Default)]
struct AllObservers(RefCell<Vec<String>>);

impl DefaultLifecycleObserver for AllObservers {
    fn on_create(&self, _: &Lifecycle) {
        self.0.borrow_mut().push("on_create".into());
    }
    fn on_start(&self, _: &Lifecycle) {
        self.0.borrow_mut().push("on_start".into());
    }
    fn on_resume(&self, _: &Lifecycle) {
        self.0.borrow_mut().push("on_resume".into());
    }
    fn on_pause(&self, _: &Lifecycle) {
        self.0.borrow_mut().push("on_pause".into());
    }
    fn on_stop(&self, _: &Lifecycle) {
        self.0.borrow_mut().push("on_stop".into());
    }
    fn on_destroy(&self, _: &Lifecycle) {
        self.0.borrow_mut().push("on_destroy".into());
    }
}

impl LifecycleObserver for AllObservers {
    fn on_state_changed(&self, _: &Lifecycle, event: LifecycleEvent) {
        self.0.borrow_mut().push(format!("{event:?}"));
    }
}

const ALL: [LifecycleEvent; 6] = [OnCreate, OnStart, OnResume, OnPause, OnStop, OnDestroy];
const METHODS: [&str; 6] =
    ["on_create", "on_start", "on_resume", "on_pause", "on_stop", "on_destroy"];

// Compose: lifecycle/lifecycle-common/src/jvmTest/java/androidx/lifecycle/DefaultLifecycleObserverTest.kt, eachEvent
#[test]
fn each_event_calls_its_method() {
    let observer = Rc::new(AllObservers::default());
    let adapter = DefaultLifecycleObserverAdapter::new(observer.clone(), None);
    let lifecycle = LifecycleRegistry::new().lifecycle();
    for event in ALL {
        adapter.on_state_changed(&lifecycle, event);
    }
    assert_eq!(*observer.0.borrow(), METHODS);
}

// Compose: lifecycle/lifecycle-common/src/jvmTest/java/androidx/lifecycle/DefaultLifecycleObserverTest.kt, defaultLifecycleObserverAndLifecycleEventObserver
#[test]
fn both_observers_of_an_object_are_called_the_method_first() {
    let observer = Rc::new(AllObservers::default());
    let adapter = DefaultLifecycleObserverAdapter::new(observer.clone(), Some(observer.clone()));
    let lifecycle = LifecycleRegistry::new().lifecycle();
    let mut expected = Vec::new();
    for (event, method) in ALL.iter().zip(METHODS) {
        adapter.on_state_changed(&lifecycle, *event);
        expected.extend([method.to_string(), format!("{event:?}")]);
    }
    assert_eq!(*observer.0.borrow(), expected);
}

// Compose: lifecycle/lifecycle-common/src/commonMain/kotlin/androidx/lifecycle/Lifecycle.kt, Event.upFrom, downFrom, upTo, downTo, targetState (no test of AndroidX's alone)
#[test]
fn the_events_lead_from_state_to_state() {
    use LifecycleState::*;
    assert_eq!(LifecycleEvent::up_from(Initialized), Some(OnCreate));
    assert_eq!(LifecycleEvent::up_from(Resumed), None);
    assert_eq!(LifecycleEvent::down_from(Resumed), Some(OnPause));
    assert_eq!(LifecycleEvent::down_from(Initialized), None);
    assert_eq!(LifecycleEvent::up_to(Started), Some(OnStart));
    assert_eq!(LifecycleEvent::up_to(Destroyed), None);
    assert_eq!(LifecycleEvent::down_to(Destroyed), Some(OnDestroy));
    assert_eq!(LifecycleEvent::down_to(Resumed), None);
    for (event, state) in
        [(OnCreate, Created), (OnStop, Created), (OnStart, Started), (OnPause, Started)]
    {
        assert_eq!(event.target_state(), state);
    }
    assert!(Resumed.is_at_least(Started) && !Created.is_at_least(Started));
}
