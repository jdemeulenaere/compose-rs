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
    let (seen, from, writer) = (read.clone(), toread.clone(), toread.clone());
    let lifecycle = owner.lifecycle();
    platform::launch(async move {
        let value = lifecycle.with_started(move || from.get()).await.unwrap();
        seen.set(value);
    });
    // Next in line: the block has run by then if its first poll ran it, not if it yielded first.
    platform::launch(async move { writer.set(2) });
    platform::run_ready();
    assert_eq!(read.get(), 1);
    assert_eq!(toread.get(), 2);
}

/// As AndroidX's `removeObserver(this)` before `runCatching(block)`: a block sees it gone.
#[test]
fn a_block_run_as_the_state_changes_finds_its_observer_gone() {
    let owner = TestLifecycleOwner::new(LifecycleState::Created);
    let count = Rc::new(Cell::new(usize::MAX));
    let (seen, lifecycle, counted) = (count.clone(), owner.lifecycle(), owner.clone());
    platform::launch(async move {
        lifecycle.with_started(move || seen.set(counted.observer_count())).await.unwrap();
    });
    platform::run_ready();
    assert_eq!(owner.observer_count(), 1);
    owner.set_current_state(LifecycleState::Started);
    assert_eq!(count.get(), 0);
}

/// As AndroidX's `co.resumeWith(runCatching(block))`: a block's failure is for who waits for it,
/// and the lifecycle's other observers still hear the event.
#[test]
fn a_block_s_failure_reaches_its_waiter_not_the_lifecycle() {
    let owner = TestLifecycleOwner::new(LifecycleState::Created);
    let lifecycle = owner.lifecycle();
    platform::launch(async move {
        lifecycle.with_started(|| panic!("the block's failure")).await.unwrap();
    });
    platform::run_ready();
    let heard = Rc::new(Cell::new(false));
    let told = heard.clone();
    owner.lifecycle().add_observer_fn(move |_, event| {
        if event == LifecycleEvent::OnStart {
            told.set(true);
        }
    });
    owner.set_current_state(LifecycleState::Started);
    assert!(heard.get(), "the observer after it heard the event");
    assert_eq!(owner.current_state(), LifecycleState::Started);
    let failure = std::panic::catch_unwind(platform::run_ready).unwrap_err();
    assert_eq!(failure.downcast_ref::<&str>(), Some(&"the block's failure"));
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
