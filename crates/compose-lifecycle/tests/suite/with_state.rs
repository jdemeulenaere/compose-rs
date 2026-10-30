//! `with_started` and the like against AndroidX's `WithLifecycleStateTest`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_lifecycle::*;
use compose_runtime::platform;

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/WithLifecycleStateTest.kt, testInitialResumed
#[test]
fn a_block_runs_at_once_when_the_state_is_reached() {
    let owner = TestLifecycleOwner::new(LifecycleState::Resumed);
    let read = Rc::new(Cell::new(0));
    let toread = Rc::new(Cell::new(1));
    let (seen, from) = (read.clone(), toread.clone());
    let lifecycle = owner.lifecycle();
    platform::launch(async move {
        let value = lifecycle.with_started(move || from.get()).await.unwrap();
        seen.set(value);
    });
    // Whatever else was launched has not run in between: the first poll gives the result.
    toread.set(1);
    platform::run_ready();
    assert_eq!(read.get(), 1);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/WithLifecycleStateTest.kt, testBlockRunsWithLifecycleStateChange
#[test]
fn a_block_runs_as_the_state_changes_before_what_follows_the_change() {
    let owner = TestLifecycleOwner::new(LifecycleState::Initialized);
    let order = Rc::new(RefCell::new(Vec::new()));
    let (block_order, resumed_order) = (order.clone(), order.clone());
    let lifecycle = owner.lifecycle();
    platform::launch(async move {
        lifecycle.with_started(move || block_order.borrow_mut().push("block")).await.unwrap();
        resumed_order.borrow_mut().push("resumed task");
    });
    platform::run_ready();
    assert!(order.borrow().is_empty());
    owner.set_current_state(LifecycleState::Resumed);
    order.borrow_mut().push("after the state's set");
    platform::run_ready();
    assert_eq!(*order.borrow(), ["block", "after the state's set", "resumed task"]);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/WithLifecycleStateTest.kt, testBlockCancelledWhenInitiallyDestroyed
#[test]
fn a_destroyed_lifecycle_gives_an_error() {
    let owner = TestLifecycleOwner::new(LifecycleState::Created);
    owner.set_current_state(LifecycleState::Destroyed);
    let result = Rc::new(Cell::new(None));
    let (told, lifecycle) = (result.clone(), owner.lifecycle());
    platform::launch(async move {
        told.set(Some(lifecycle.with_started(|| ()).await));
    });
    platform::run_ready();
    assert_eq!(result.get(), Some(Err(LifecycleDestroyed)));
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/WithLifecycleStateTest.kt, testBlockCancelledWhenDestroyedWhileSuspended
#[test]
fn a_lifecycle_destroyed_during_the_wait_gives_an_error() {
    let owner = TestLifecycleOwner::new(LifecycleState::Created);
    let result = Rc::new(Cell::new(None));
    let (told, lifecycle) = (result.clone(), owner.lifecycle());
    platform::launch(async move {
        told.set(Some(lifecycle.with_started(|| ()).await));
    });
    platform::run_ready();
    assert_eq!(result.get(), None, "it waits");
    assert_eq!(owner.observer_count(), 1);
    owner.set_current_state(LifecycleState::Destroyed);
    platform::run_ready();
    assert_eq!(result.get(), Some(Err(LifecycleDestroyed)));
}

#[test]
fn a_cancelled_wait_leaves_no_observer() {
    let owner = TestLifecycleOwner::new(LifecycleState::Created);
    let lifecycle = owner.lifecycle();
    let job = platform::launch(async move {
        let _ = lifecycle.with_resumed(|| ()).await;
    });
    platform::run_ready();
    assert_eq!(owner.observer_count(), 1);
    job.cancel();
    assert_eq!(owner.observer_count(), 0);
}

#[test]
#[should_panic(expected = "target state must be CREATED or greater")]
fn with_state_at_least_refuses_a_state_below_created() {
    let owner = TestLifecycleOwner::new(LifecycleState::Created);
    let lifecycle = owner.lifecycle();
    let mut future = Box::pin(lifecycle.with_state_at_least(LifecycleState::Initialized, || ()));
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    let _ = future.as_mut().poll(&mut context);
}
