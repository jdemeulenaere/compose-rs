//! A lifecycle's tasks, as AndroidX's `lifecycleScope`, `repeatOnLifecycle` and `withStateAtLeast`.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::{Rc, Weak};
use std::task::{Poll, Waker};

use compose_runtime::platform::launch;
use compose_runtime::{CoroutineScope, Job};

use crate::lifecycle::{Lifecycle, LifecycleEvent, LifecycleObserver, LifecycleState};

/// The lifecycle was destroyed before reaching the state waited for, as AndroidX's
/// `LifecycleDestroyedException` (a cancellation in Kotlin, a result here).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LifecycleDestroyed;

impl std::fmt::Display for LifecycleDestroyed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the lifecycle was destroyed")
    }
}

impl std::error::Error for LifecycleDestroyed {}

/// Removes an observer when dropped: a task cancelled drops its future.
struct Observing {
    lifecycle: Lifecycle,
    observer: Option<Rc<dyn LifecycleObserver>>,
}

impl Drop for Observing {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            self.lifecycle.remove_observer(&observer);
        }
    }
}

/// What a wait is told by its observer.
struct Outcome<R> {
    value: RefCell<Option<R>>,
    waker: RefCell<Option<Waker>>,
}

impl Lifecycle {
    /// The scope of tasks ending with it, as `Lifecycle.coroutineScope` and `lifecycleScope`:
    /// cancelled when it's destroyed, and from the start if it is already.
    pub fn coroutine_scope(&self) -> CoroutineScope {
        let cell = self.scope_cell();
        if let Some(scope) = cell.get() {
            return **scope;
        }
        let scope = CoroutineScope::new();
        let handle: CoroutineScope = *scope;
        let _ = cell.set(scope);
        if self.current_state() == LifecycleState::Destroyed {
            handle.cancel();
        } else {
            let observer = Rc::new(RefCell::new(None::<Rc<dyn LifecycleObserver>>));
            let removed = observer.clone();
            let added = self.add_observer_fn(move |lifecycle: &Lifecycle, _| {
                if lifecycle.current_state() <= LifecycleState::Destroyed {
                    if let Some(observer) = removed.borrow_mut().take() {
                        lifecycle.remove_observer(&observer);
                    }
                    handle.cancel();
                }
            });
            *observer.borrow_mut() = Some(added);
        }
        handle
    }

    /// Runs `block` as a task each time the lifecycle reaches `state`, cancelling it when it goes
    /// below, until it's destroyed, as `repeatOnLifecycle`. Panics for `Initialized`, as AndroidX
    /// throws. A cancelled block's future is dropped at once, so a new one never runs alongside
    /// it: AndroidX's mutex has nothing to do.
    pub async fn repeat_on_lifecycle<F, Fut>(&self, state: LifecycleState, block: F)
    where
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        assert!(
            state != LifecycleState::Initialized,
            "repeatOnLifecycle cannot start work with the INITIALIZED lifecycle state."
        );
        if self.current_state() == LifecycleState::Destroyed {
            return;
        }
        let start = LifecycleEvent::up_to(state);
        let cancel = LifecycleEvent::down_from(state);
        let job = Rc::new(RefCell::new(None::<Job>));
        let outcome = Rc::new(Outcome { value: RefCell::new(None), waker: RefCell::new(None) });
        let (running, told) = (job.clone(), outcome.clone());
        let observer: Rc<dyn LifecycleObserver> =
            Rc::new(move |_: &Lifecycle, event: LifecycleEvent| {
                if Some(event) == start {
                    *running.borrow_mut() = Some(launch(block()));
                    return;
                }
                if Some(event) == cancel
                    && let Some(job) = running.borrow_mut().take()
                {
                    job.cancel();
                }
                if event == LifecycleEvent::OnDestroy {
                    *told.value.borrow_mut() = Some(());
                    if let Some(waker) = told.waker.borrow_mut().take() {
                        waker.wake();
                    }
                }
            });
        self.add_observer(observer.clone());
        let _observing = Observing { lifecycle: self.clone(), observer: Some(observer) };
        // Cancelled, the work goes with this future, as `launchedJob?.cancel()` in `finally`.
        struct CancelOnDrop(Rc<RefCell<Option<Job>>>);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                if let Some(job) = self.0.borrow_mut().take() {
                    job.cancel();
                }
            }
        }
        let _work = CancelOnDrop(job);
        wait(&outcome).await;
    }

    /// `block`'s result once the lifecycle is at least at `state`, at once if it is, as
    /// `withStateAtLeast`: destroyed first, an error. Panics below `Created`, as AndroidX throws.
    pub async fn with_state_at_least<R: 'static>(
        &self,
        state: LifecycleState,
        block: impl FnOnce() -> R + 'static,
    ) -> Result<R, LifecycleDestroyed> {
        assert!(
            state >= LifecycleState::Created,
            "target state must be CREATED or greater, found {state:?}"
        );
        self.with_state_at_least_unchecked(state, block).await
    }

    /// As `withCreated`.
    pub async fn with_created<R: 'static>(
        &self,
        block: impl FnOnce() -> R + 'static,
    ) -> Result<R, LifecycleDestroyed> {
        self.with_state_at_least_unchecked(LifecycleState::Created, block).await
    }

    /// As `withStarted`.
    pub async fn with_started<R: 'static>(
        &self,
        block: impl FnOnce() -> R + 'static,
    ) -> Result<R, LifecycleDestroyed> {
        self.with_state_at_least_unchecked(LifecycleState::Started, block).await
    }

    /// As `withResumed`.
    pub async fn with_resumed<R: 'static>(
        &self,
        block: impl FnOnce() -> R + 'static,
    ) -> Result<R, LifecycleDestroyed> {
        self.with_state_at_least_unchecked(LifecycleState::Resumed, block).await
    }

    async fn with_state_at_least_unchecked<R: 'static>(
        &self,
        state: LifecycleState,
        block: impl FnOnce() -> R + 'static,
    ) -> Result<R, LifecycleDestroyed> {
        if self.current_state() == LifecycleState::Destroyed {
            return Err(LifecycleDestroyed);
        }
        if self.current_state() >= state {
            return Ok(block());
        }
        let outcome = Rc::new(Outcome { value: RefCell::new(None), waker: RefCell::new(None) });
        let told = outcome.clone();
        let block = Cell::new(Some(block));
        let up_to = LifecycleEvent::up_to(state);
        // Told, it goes from the lifecycle at once, as AndroidX's.
        let itself = Rc::new(RefCell::new(None::<Weak<dyn LifecycleObserver>>));
        let removed = itself.clone();
        let observer: Rc<dyn LifecycleObserver> =
            Rc::new(move |lifecycle: &Lifecycle, event: LifecycleEvent| {
                let result = if Some(event) == up_to {
                    block.take().map(|block| Ok(block()))
                } else if event == LifecycleEvent::OnDestroy {
                    Some(Err(LifecycleDestroyed))
                } else {
                    None
                };
                if let Some(result) = result {
                    if let Some(observer) = removed.borrow().as_ref().and_then(Weak::upgrade) {
                        lifecycle.remove_observer(&observer);
                    }
                    *told.value.borrow_mut() = Some(result);
                    if let Some(waker) = told.waker.borrow_mut().take() {
                        waker.wake();
                    }
                }
            });
        *itself.borrow_mut() = Some(Rc::downgrade(&observer));
        self.add_observer(observer.clone());
        let _observing = Observing { lifecycle: self.clone(), observer: Some(observer) };
        wait(&outcome).await
    }
}

async fn wait<R>(outcome: &Outcome<R>) -> R {
    std::future::poll_fn(|context| match outcome.value.borrow_mut().take() {
        Some(value) => Poll::Ready(value),
        None => {
            *outcome.waker.borrow_mut() = Some(context.waker().clone());
            Poll::Pending
        }
    })
    .await
}
