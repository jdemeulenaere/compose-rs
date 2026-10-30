//! `drop_unless_started` and `drop_unless_resumed` against AndroidX's `DropUnlessLifecycleTest`.

use std::cell::Cell;
use std::rc::Rc;

use LifecycleState::*;
use compose_lifecycle::*;
use compose_test::*;

/// `drop_unless_started` or `drop_unless_resumed`.
type DropUnless = fn(&TestLifecycleOwner, Rc<dyn Fn()>) -> Rc<dyn Fn()>;

fn invoked(state: LifecycleState, drop_unless: DropUnless) -> bool {
    let owner = TestLifecycleOwner::new(Created);
    if state != Created {
        owner.set_current_state(state);
    }
    let has_been_invoked = Rc::new(Cell::new(false));
    let seen = has_been_invoked.clone();
    let _ui = compose_test(move || {
        let seen = seen.clone();
        let under_test = drop_unless(&owner, Rc::new(move || seen.set(true)));
        under_test();
    });
    has_been_invoked.get()
}

fn started(owner: &TestLifecycleOwner, block: Rc<dyn Fn()>) -> Rc<dyn Fn()> {
    Rc::new(drop_unless_started(owner, move || block()))
}

fn resumed(owner: &TestLifecycleOwner, block: Rc<dyn Fn()>) -> Rc<dyn Fn()> {
    Rc::new(drop_unless_resumed(owner, move || block()))
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessStarted_lifecycleInitialized_doNothing
#[test]
fn drop_unless_started_at_initialized_does_nothing() {
    let owner = TestLifecycleOwner::new(Initialized);
    let called = Rc::new(Cell::new(false));
    let seen = called.clone();
    drop_unless_started(&owner, move || seen.set(true))();
    assert!(!called.get());
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessStarted_lifecycleCreated_doNothing
#[test]
fn drop_unless_started_at_created_does_nothing() {
    assert!(!invoked(Created, started));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessStarted_lifecycleStarted_invoke
#[test]
fn drop_unless_started_at_started_invokes() {
    assert!(invoked(Started, started));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessStarted_lifecycleResumed_invoke
#[test]
fn drop_unless_started_at_resumed_invokes() {
    assert!(invoked(Resumed, started));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessStarted_lifecycleDestroyed_doNothing
#[test]
fn drop_unless_started_at_destroyed_does_nothing() {
    assert!(!invoked(Destroyed, started));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessResumed_lifecycleInitialized_doNothing
#[test]
fn drop_unless_resumed_at_initialized_does_nothing() {
    let owner = TestLifecycleOwner::new(Initialized);
    let called = Rc::new(Cell::new(false));
    let seen = called.clone();
    drop_unless_resumed(&owner, move || seen.set(true))();
    assert!(!called.get());
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessResumed_lifecycleCreated_doNothing
#[test]
fn drop_unless_resumed_at_created_does_nothing() {
    assert!(!invoked(Created, resumed));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessResumed_lifecycleStarted_invokes
#[test]
fn drop_unless_resumed_at_started_does_nothing() {
    assert!(!invoked(Started, resumed));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessResumed_lifecycleResumed_invoke
#[test]
fn drop_unless_resumed_at_resumed_invokes() {
    assert!(invoked(Resumed, resumed));
}

// Compose: lifecycle/lifecycle-runtime-compose/src/androidDeviceTest/kotlin/androidx/lifecycle/compose/DropUnlessLifecycleTest.kt, dropUnlessResumed_lifecycleDestroyed_doNothing
#[test]
fn drop_unless_resumed_at_destroyed_does_nothing() {
    assert!(!invoked(Destroyed, resumed));
}
