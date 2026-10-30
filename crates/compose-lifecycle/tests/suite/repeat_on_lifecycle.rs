//! `repeat_on_lifecycle` against AndroidX's `RepeatOnLifecycleTest`. Tasks don't start until the
//! executor runs them (`run_ready`), and a cancelled future is dropped at once: the tests with a
//! `NonCancellable` tear-down have nothing to wait for, and only check the order (D837).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_lifecycle::*;
use compose_runtime::platform;

use LifecycleState::*;

/// Numbered steps, each expected in turn, as AndroidX's `Expectations`.
#[derive(Clone, Default)]
struct Expectations(Rc<RefCell<Vec<u32>>>);

impl Expectations {
    fn expect(&self, step: u32) {
        self.0.borrow_mut().push(step);
    }

    fn assert_in_order(&self, last: u32) {
        let expected: Vec<u32> = (1..=last).collect();
        assert_eq!(*self.0.borrow(), expected);
    }
}

fn run() {
    platform::run_ready();
}

fn repeat(
    owner: &TestLifecycleOwner,
    state: LifecycleState,
    block: impl Fn() + 'static,
) -> compose_runtime::Job {
    let lifecycle = owner.lifecycle();
    let block = Rc::new(block);
    owner.lifecycle().coroutine_scope().launch(async move {
        lifecycle
            .repeat_on_lifecycle(state, move || {
                let block = block.clone();
                async move { block() }
            })
            .await;
    })
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlockRunsWhenCreatedStateIsReached
#[test]
fn a_block_runs_when_created_is_reached() {
    let owner = TestLifecycleOwner::new(Created);
    let steps = Expectations::default();
    steps.expect(1);
    let inner = steps.clone();
    let job = repeat(&owner, Created, move || inner.expect(2));
    run();
    steps.expect(3);
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    steps.assert_in_order(3);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlockRunsWhenStartedStateIsReached
#[test]
fn a_block_runs_when_started_is_reached() {
    let owner = TestLifecycleOwner::new(Created);
    let steps = Expectations::default();
    steps.expect(1);
    let inner = steps.clone();
    let job = repeat(&owner, Started, move || inner.expect(2));
    run();
    owner.set_current_state(Started);
    run();
    steps.expect(3);
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    steps.assert_in_order(3);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlockRunsWhenResumedStateIsReached
#[test]
fn a_block_runs_when_resumed_is_reached() {
    let owner = TestLifecycleOwner::new(Created);
    let steps = Expectations::default();
    steps.expect(1);
    let inner = steps.clone();
    let job = repeat(&owner, Resumed, move || inner.expect(3));
    run();
    owner.set_current_state(Started);
    run();
    steps.expect(2);
    owner.set_current_state(Resumed);
    run();
    steps.expect(4);
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    steps.assert_in_order(4);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlocksRepeatsExecution
#[test]
fn a_block_runs_again_each_time_the_state_is_reached() {
    let owner = TestLifecycleOwner::new(Created);
    let steps = Expectations::default();
    let restarted = Rc::new(Cell::new(false));
    steps.expect(1);
    let (inner, again) = (steps.clone(), restarted.clone());
    let job = repeat(&owner, Resumed, move || inner.expect(if again.get() { 5 } else { 2 }));
    run();
    owner.set_current_state(Resumed);
    run();
    steps.expect(3);
    owner.set_current_state(Started);
    run();
    steps.expect(4);
    restarted.set(true);
    owner.set_current_state(Resumed);
    run();
    steps.expect(6);
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    steps.assert_in_order(6);
}

/// Pends forever, telling when dropped (a task cancelled), as `awaitCancellation` in a `finally`.
async fn await_cancellation(on_cancel: impl FnOnce() + 'static) {
    struct OnDrop<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for OnDrop<F> {
        fn drop(&mut self) {
            if let Some(on_drop) = self.0.take() {
                on_drop();
            }
        }
    }
    let _guard = OnDrop(Some(on_cancel));
    std::future::pending::<()>().await;
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlocksRepeatsExecutionSerially
#[test]
fn a_new_run_comes_after_the_last_one_s_tear_down() {
    let owner = TestLifecycleOwner::new(Created);
    let steps = Expectations::default();
    let restarted = Rc::new(Cell::new(false));
    steps.expect(1);
    let lifecycle = owner.lifecycle();
    let (inner, again) = (steps.clone(), restarted.clone());
    owner.lifecycle().coroutine_scope().launch(async move {
        lifecycle
            .repeat_on_lifecycle(Resumed, move || {
                let (inner, again) = (inner.clone(), again.clone());
                async move {
                    if again.get() {
                        inner.expect(6);
                    } else {
                        inner.expect(2);
                        let torn_down = inner.clone();
                        await_cancellation(move || torn_down.expect(4)).await;
                    }
                }
            })
            .await;
    });
    run();
    owner.set_current_state(Resumed);
    run();
    steps.expect(3);
    owner.set_current_state(Started);
    steps.expect(5);
    restarted.set(true);
    owner.set_current_state(Resumed);
    run();
    steps.expect(7);
    owner.set_current_state(Destroyed);
    steps.assert_in_order(7);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlockIsCancelledWhenLifecycleIsDestroyed
#[test]
fn a_block_is_cancelled_when_the_lifecycle_is_destroyed() {
    let owner = TestLifecycleOwner::new(Resumed);
    let steps = Expectations::default();
    steps.expect(1);
    let lifecycle = owner.lifecycle();
    let inner = steps.clone();
    let job = owner.lifecycle().coroutine_scope().launch(async move {
        lifecycle
            .repeat_on_lifecycle(Started, move || {
                let inner = inner.clone();
                async move {
                    inner.expect(2);
                    let cancelled = inner.clone();
                    await_cancellation(move || cancelled.expect(4)).await;
                }
            })
            .await;
    });
    run();
    steps.expect(3);
    owner.set_current_state(Destroyed);
    run();
    steps.expect(5);
    assert!(!job.is_active());
    steps.assert_in_order(5);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlockRunsOnSubsequentLifecycleState
#[test]
fn a_block_runs_at_once_in_a_state_beyond_its_own() {
    let owner = TestLifecycleOwner::new(Resumed);
    let steps = Expectations::default();
    steps.expect(1);
    let inner = steps.clone();
    let job = repeat(&owner, Started, move || inner.expect(2));
    run();
    steps.expect(3);
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
    steps.assert_in_order(3);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlockDoesNotStartIfLifecycleIsDestroyed
#[test]
fn a_block_does_not_start_on_a_destroyed_lifecycle() {
    let owner = TestLifecycleOwner::new(Created);
    owner.set_current_state(Destroyed);
    let job = repeat(&owner, Started, || panic!("unreached"));
    run();
    assert!(!job.is_active());
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testCancellingTheReturnedJobCancelsTheBlock
#[test]
fn cancelling_the_task_cancels_the_block() {
    let owner = TestLifecycleOwner::new(Resumed);
    let steps = Expectations::default();
    steps.expect(1);
    let lifecycle = owner.lifecycle();
    let inner = steps.clone();
    let job = owner.lifecycle().coroutine_scope().launch(async move {
        lifecycle
            .repeat_on_lifecycle(Started, move || {
                let inner = inner.clone();
                async move {
                    inner.expect(2);
                    let cancelled = inner.clone();
                    await_cancellation(move || cancelled.expect(4)).await;
                }
            })
            .await;
    });
    run();
    steps.expect(3);
    job.cancel();
    run();
    steps.expect(5);
    assert!(!job.is_active());
    steps.assert_in_order(5);
    assert_eq!(owner.observer_count(), 1, "the lifecycle scope's own, not the block's");
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testCancellingTheJobDoesNotRestartTheBlockOnNewStates
#[test]
fn cancelling_the_task_does_not_restart_the_block_on_new_states() {
    let owner = TestLifecycleOwner::new(Resumed);
    let steps = Expectations::default();
    steps.expect(1);
    let restarted = Rc::new(Cell::new(false));
    let (inner, again) = (steps.clone(), restarted.clone());
    let job = repeat(&owner, Resumed, move || {
        assert!(!again.get(), "unreached");
        inner.expect(2);
    });
    run();
    steps.expect(3);
    job.cancel();
    assert!(!job.is_active());
    owner.set_current_state(Started);
    restarted.set(true);
    steps.expect(4);
    owner.set_current_state(Resumed);
    run();
    steps.expect(5);
    owner.set_current_state(Destroyed);
    steps.assert_in_order(5);
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testBlockDoesNotStartWithDestroyedState
#[test]
fn a_block_does_not_start_for_the_destroyed_state() {
    let owner = TestLifecycleOwner::new(Started);
    let job = repeat(&owner, Destroyed, || panic!("unreached"));
    run();
    owner.set_current_state(Destroyed);
    run();
    assert!(!job.is_active());
}

// Compose: lifecycle/lifecycle-runtime/src/commonTest/kotlin/androidx/lifecycle/RepeatOnLifecycleTest.kt, testExceptionWithInitializedState
#[test]
#[should_panic(expected = "repeatOnLifecycle cannot start work with the INITIALIZED")]
fn the_initialized_state_is_refused() {
    let owner = TestLifecycleOwner::new(Started);
    repeat(&owner, Initialized, || {});
    run();
}

#[test]
fn the_lifecycle_scope_ends_with_it() {
    let owner = TestLifecycleOwner::new(Started);
    let scope = owner.lifecycle().coroutine_scope();
    assert_eq!(owner.lifecycle().coroutine_scope(), scope, "the same one");
    let job = scope.launch(std::future::pending());
    run();
    assert!(job.is_active());
    owner.set_current_state(Destroyed);
    assert!(!job.is_active());
    assert!(!scope.is_active());
    assert_eq!(owner.observer_count(), 0);
}
