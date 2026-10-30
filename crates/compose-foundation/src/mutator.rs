//! `MutatorMutex`, as Compose's: one mutation of shared state at a time (a scroll, a drag, an
//! animation), a new one interrupting the one running unless that one has a higher priority.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// How much a mutation matters, as Compose's `MutatePriority`: a mutation interrupts the one
/// running if its priority is the same or higher, and is refused otherwise.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum MutatePriority {
    /// What an application does (an animated scroll it starts).
    #[default]
    Default,
    /// What the user does (a drag).
    UserInput,
    /// What must not be interrupted by the user (an animation the user can't stop).
    PreventUserInput,
}

impl compose_runtime::ComposeArg for MutatePriority {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A mutation that didn't run to its end: interrupted by another of the same or a higher
/// priority, or refused for one of a higher priority running. Compose throws a cancellation
/// (`MutationInterruptedException`) where compose-rs returns this (D850).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MutationInterrupted;

impl std::fmt::Display for MutationInterrupted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("mutation interrupted")
    }
}

impl std::error::Error for MutationInterrupted {}

/// One mutation at a time, as Compose's `MutatorMutex`: [`mutate`](Self::mutate) runs a block
/// once the mutations running of the same or a lower priority are interrupted, and refuses to
/// if one of a higher priority runs; [`try_mutate`](Self::try_mutate) runs a block at once if
/// nothing runs.
///
/// An interrupted mutation's block is never polled again: its `mutate` returns
/// [`MutationInterrupted`] when its task next runs, and the block is dropped then. The new
/// mutation doesn't wait for that, as it can't run meanwhile (D850).
///
/// Clones share the mutex.
#[derive(Clone, Default)]
pub struct MutatorMutex {
    inner: Rc<RefCell<MutexState>>,
}

#[derive(Default)]
struct MutexState {
    /// The mutation running.
    current: Option<Rc<Mutator>>,
    /// `try_mutate` holds the lock.
    locked: bool,
    /// Mutations waiting for `try_mutate` to let go.
    waiting: Vec<Waker>,
}

struct Mutator {
    priority: MutatePriority,
    interrupted: Cell<bool>,
    /// The task to wake when it's interrupted.
    waker: RefCell<Option<Waker>>,
}

impl MutatorMutex {
    pub fn new() -> MutatorMutex {
        MutatorMutex::default()
    }

    /// Takes the mutex for a mutation of `priority`: the running one is interrupted, unless
    /// its priority is higher (then `None`).
    fn take(&self, priority: MutatePriority) -> Option<Rc<Mutator>> {
        let mutator =
            Rc::new(Mutator { priority, interrupted: Cell::new(false), waker: RefCell::new(None) });
        let old = {
            let mut state = self.inner.borrow_mut();
            if state.current.as_ref().is_some_and(|current| priority < current.priority) {
                return None;
            }
            state.current.replace(mutator.clone())
        };
        if let Some(old) = old {
            old.interrupted.set(true);
            let waker = old.waker.borrow_mut().take();
            if let Some(waker) = waker {
                waker.wake();
            }
        }
        Some(mutator)
    }

    /// Runs `block` as a mutation of `priority`, as `mutate`: a mutation running of the same or
    /// a lower priority is interrupted first; one of a higher priority makes this one return
    /// [`MutationInterrupted`] without running `block`, as does a later mutation interrupting
    /// this one.
    pub async fn mutate<R>(
        &self,
        priority: MutatePriority,
        block: impl Future<Output = R>,
    ) -> Result<R, MutationInterrupted> {
        let Some(mutator) = self.take(priority) else { return Err(MutationInterrupted) };
        let _release = Release { mutex: self.inner.clone(), mutator: mutator.clone() };
        Unlocked { mutex: &self.inner, mutator: &mutator }.await?;
        Interruptible { mutator: &mutator, block: std::pin::pin!(block) }.await
    }

    /// The same, giving `receiver` to `block`, as `mutateWith`.
    pub async fn mutate_with<T, R, F: Future<Output = R>>(
        &self,
        receiver: T,
        priority: MutatePriority,
        block: impl FnOnce(T) -> F,
    ) -> Result<R, MutationInterrupted> {
        let Some(mutator) = self.take(priority) else { return Err(MutationInterrupted) };
        let _release = Release { mutex: self.inner.clone(), mutator: mutator.clone() };
        Unlocked { mutex: &self.inner, mutator: &mutator }.await?;
        Interruptible { mutator: &mutator, block: std::pin::pin!(block(receiver)) }.await
    }

    /// Whether a mutation is running (not `try_mutate`): no state read.
    pub(crate) fn is_mutating(&self) -> bool {
        self.inner.borrow().current.is_some()
    }

    /// Runs `block` now if no mutation is running nor `try_mutate` (and returns `true`), else
    /// returns `false`, as `tryMutate`. Mutations started meanwhile wait for it.
    pub fn try_mutate(&self, block: impl FnOnce()) -> bool {
        {
            let mut state = self.inner.borrow_mut();
            if state.current.is_some() || state.locked {
                return false;
            }
            state.locked = true;
        }
        let _unlock = Unlock(self.inner.clone());
        block();
        true
    }
}

impl std::fmt::Debug for MutatorMutex {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("MutatorMutex")
    }
}

/// Lets go of `try_mutate`'s lock, however its block ends.
struct Unlock(Rc<RefCell<MutexState>>);

impl Drop for Unlock {
    fn drop(&mut self) {
        let waiting = {
            let mut state = self.0.borrow_mut();
            state.locked = false;
            std::mem::take(&mut state.waiting)
        };
        for waker in waiting {
            waker.wake();
        }
    }
}

/// Lets go of the mutex when a mutation ends however it ends, unless another took it over.
struct Release {
    mutex: Rc<RefCell<MutexState>>,
    mutator: Rc<Mutator>,
}

impl Drop for Release {
    fn drop(&mut self) {
        let mut state = self.mutex.borrow_mut();
        if state.current.as_ref().is_some_and(|current| Rc::ptr_eq(current, &self.mutator)) {
            state.current = None;
        }
    }
}

/// Waits for `try_mutate` to let go of the lock (only a mutation started from inside its block
/// meets it held).
struct Unlocked<'a> {
    mutex: &'a Rc<RefCell<MutexState>>,
    mutator: &'a Rc<Mutator>,
}

impl Future for Unlocked<'_> {
    type Output = Result<(), MutationInterrupted>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.mutator.interrupted.get() {
            return Poll::Ready(Err(MutationInterrupted));
        }
        let mut state = self.mutex.borrow_mut();
        if !state.locked {
            return Poll::Ready(Ok(()));
        }
        state.waiting.push(context.waker().clone());
        *self.mutator.waker.borrow_mut() = Some(context.waker().clone());
        Poll::Pending
    }
}

/// Polls a mutation's block until it ends or the mutation is interrupted.
struct Interruptible<'a, F> {
    mutator: &'a Mutator,
    block: Pin<&'a mut F>,
}

impl<F: Future> Future for Interruptible<'_, F> {
    type Output = Result<F::Output, MutationInterrupted>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.mutator.interrupted.get() {
            return Poll::Ready(Err(MutationInterrupted));
        }
        {
            let mut waker = self.mutator.waker.borrow_mut();
            if !waker.as_ref().is_some_and(|waker| waker.will_wake(context.waker())) {
                *waker = Some(context.waker().clone());
            }
        }
        self.block.as_mut().poll(context).map(Ok)
    }
}
