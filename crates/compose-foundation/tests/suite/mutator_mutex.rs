//! `MutatorMutex`: one mutation at a time, a new one of the same or a higher priority
//! interrupting the one running, a lower one refused.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;

use compose_foundation::{MutatePriority, MutationInterrupted, MutatorMutex};
use compose_runtime::platform::run_ready;
use compose_runtime::{CoroutineScope, Owned};

/// A future that never completes, as `suspendCancellableCoroutine {}` never resumed.
fn forever() -> impl Future<Output = ()> {
    std::future::pending()
}

/// Launches `future` on this thread's executor and runs it as far as it goes.
fn launch(scope: &Owned<CoroutineScope>, future: impl Future<Output = ()> + 'static) {
    scope.launch(future);
    run_ready();
}

const PRIORITIES: [MutatePriority; 3] =
    [MutatePriority::Default, MutatePriority::UserInput, MutatePriority::PreventUserInput];

/// Runs a mutation through `mutate` or `mutate_with`, as Compose's test runs each case with
/// and without a receiver.
async fn mutate_either(
    mutex: &MutatorMutex,
    with_receiver: bool,
    priority: MutatePriority,
    block: impl Future<Output = ()>,
) -> Result<(), MutationInterrupted> {
    if with_receiver {
        let receiver = Rc::new(7);
        let given = receiver.clone();
        mutex
            .mutate_with(receiver, priority, |receiver| async move {
                assert!(Rc::ptr_eq(&receiver, &given), "mutate_with passes its receiver");
                block.await
            })
            .await
    } else {
        mutex.mutate(priority, block).await
    }
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, newMutatorCancelsOld
#[test]
fn a_new_mutation_interrupts_the_one_running() {
    for with_receiver in [false, true] {
        let scope = CoroutineScope::new();
        let mutex = MutatorMutex::new();
        let first: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
        let (into, by) = (first.clone(), mutex.clone());
        launch(&scope, async move {
            *into.borrow_mut() =
                Some(mutate_either(&by, with_receiver, MutatePriority::Default, forever()).await);
        });
        assert_eq!(*first.borrow(), None, "the first mutation runs until interrupted");
        let second = Rc::new(Cell::new(false));
        let (ran, by) = (second.clone(), mutex.clone());
        launch(&scope, async move {
            let result = mutate_either(&by, with_receiver, MutatePriority::Default, async {}).await;
            ran.set(result.is_ok());
        });
        assert!(second.get(), "the second mutation ran");
        assert_eq!(*first.borrow(), Some(Err(MutationInterrupted)), "the first was interrupted");
    }
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, mutatorsCancelByPriority
#[test]
fn mutations_interrupt_each_other_by_priority() {
    for with_receiver in [false, true] {
        for first_priority in PRIORITIES {
            for second_priority in PRIORITIES {
                let scope = CoroutineScope::new();
                let mutex = MutatorMutex::new();
                let first: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
                let (into, by) = (first.clone(), mutex.clone());
                launch(&scope, async move {
                    let result = mutate_either(&by, with_receiver, first_priority, forever()).await;
                    *into.borrow_mut() = Some(result);
                });
                let second: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
                let (into, by) = (second.clone(), mutex.clone());
                launch(&scope, async move {
                    let result = mutate_either(&by, with_receiver, second_priority, async {}).await;
                    *into.borrow_mut() = Some(result);
                });
                let interrupts = second_priority >= first_priority;
                let expected_second = if interrupts { Ok(()) } else { Err(MutationInterrupted) };
                assert_eq!(
                    *second.borrow(),
                    Some(expected_second),
                    "{second_priority:?} after {first_priority:?}: refused only when lower"
                );
                let expected_first = interrupts.then_some(Err(MutationInterrupted));
                assert_eq!(
                    *first.borrow(),
                    expected_first,
                    "{first_priority:?} interrupted by {second_priority:?}"
                );
                scope.cancel();
            }
        }
    }
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, tryMutateBlockingSuspendsSubsequentMutate
#[test]
fn a_mutation_started_during_try_mutate_waits_for_it() {
    let scope = CoroutineScope::new();
    let mutex = MutatorMutex::new();
    let in_try_mutate = Rc::new(Cell::new(false));
    let mutated = Rc::new(Cell::new(false));
    let (inside, ran, by) = (in_try_mutate.clone(), mutated.clone(), mutex.clone());
    let locked = mutex.try_mutate(|| {
        in_try_mutate.set(true);
        // A mutation launched and run from inside: it waits for the lock.
        scope.launch(async move {
            let _ = by
                .mutate(MutatePriority::Default, async {
                    assert!(!inside.get(), "mutated before try_mutate finished");
                    ran.set(true);
                })
                .await;
        });
        run_ready();
        assert!(!mutated.get(), "the mutation waits while try_mutate holds the lock");
        in_try_mutate.set(false);
    });
    assert!(locked);
    run_ready();
    assert!(mutated.get(), "the mutation ran once try_mutate let go");
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, tryMutateDoesNotOverrideActiveCaller
#[test]
fn try_mutate_does_not_run_during_a_mutation() {
    let scope = CoroutineScope::new();
    let mutex = MutatorMutex::new();
    let by = mutex.clone();
    launch(&scope, async move {
        let _ = by.mutate(MutatePriority::Default, forever()).await;
    });
    let ran = Cell::new(false);
    assert!(!mutex.try_mutate(|| ran.set(true)), "a mutation is running");
    assert!(!ran.get());
    scope.cancel();
    // Its task cancelled, the mutation let go.
    assert!(mutex.try_mutate(|| ran.set(true)));
    assert!(ran.get());
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, tryMutateBlockingTryMutateLocks
// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, tryLockLockedMutexDoesNotLock
#[test]
fn try_mutate_inside_try_mutate_does_not_run() {
    let mutex = MutatorMutex::new();
    let mut inner = None;
    assert!(mutex.try_mutate(|| inner = Some(mutex.try_mutate(|| {}))));
    assert_eq!(inner, Some(false), "the mutex is locked while try_mutate runs");
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, tryLockUnlockedMutexLocks
// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/MutatorMutexTest.kt, unlockLockedMutex
#[test]
fn try_mutate_locks_again_once_it_let_go() {
    let mutex = MutatorMutex::new();
    assert!(mutex.try_mutate(|| {}));
    assert!(mutex.try_mutate(|| {}), "the lock was let go");
}

/// A mutation whose task is cancelled lets go, and one that panics too.
#[test]
fn a_mutation_lets_go_when_it_ends_however_it_ends() {
    let scope = CoroutineScope::new();
    let mutex = MutatorMutex::new();
    let by = mutex.clone();
    launch(&scope, async move {
        let _ = by.mutate(MutatePriority::PreventUserInput, async {}).await;
    });
    assert!(mutex.try_mutate(|| {}), "a finished mutation let go");
    let lower = Rc::new(Cell::new(None));
    let (into, by) = (lower.clone(), mutex.clone());
    launch(&scope, async move {
        into.set(Some(by.mutate(MutatePriority::Default, async {}).await.is_ok()));
    });
    assert_eq!(lower.get(), Some(true), "nothing of a higher priority is left running");
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mutex.try_mutate(|| panic!("in try_mutate"));
    }));
    assert!(panicked.is_err());
    assert!(mutex.try_mutate(|| {}), "a panic in try_mutate let go of the lock");
}
