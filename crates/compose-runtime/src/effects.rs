//! Effects, written on remember observers as in Compose.

use std::cell::Cell;
use std::rc::Rc;

use crate::compose_arg::ComposeArg;
use crate::composer::record_effect;
use crate::executor::{CoroutineScope, Job};
use crate::remember::{RememberObserver, remember, remember_observer, remember_observer_keyed};
use crate::runtime::{DeferredEffect, RunEffect};
use crate::state::Owned;

/// Where a [`SideEffect`]'s closure waits for the end of the pass: a remembered slot, so a
/// pass that runs the call again allocates nothing.
struct EffectSlot<F>(Cell<Option<F>>);

impl<F: FnOnce()> RunEffect for EffectSlot<F> {
    fn run(&self) {
        if let Some(effect) = self.0.take() {
            effect();
        }
    }
}

/// Runs `effect` after every successful pass that ran this call, as Compose's `SideEffect`.
#[allow(non_snake_case)]
pub fn SideEffect<F: FnOnce() + 'static>(effect: F) {
    let slot = remember(|| Rc::new(EffectSlot::<F>(Cell::new(None))));
    // Normally empty: the previous run's effect ran at the end of its pass.
    let previous = slot.0.replace(Some(effect));
    drop(previous);
    record_effect(DeferredEffect::Slot(slot));
}

/// Runs `effect` after a successful pass that ran this call with `keys` not
/// [`same`](ComposeArg::same) as the last time, and after the first, as Compose's
/// `SideEffect(key1, …) { … }`: work that only needs redoing when something changed. `keys` is
/// one value or a tuple (`SideEffect(key1, key2)`); an array stands for vararg keys, and an
/// array of another length is a change.
#[allow(non_snake_case)]
pub fn KeyedSideEffect<K: ComposeArg, F: FnOnce() + 'static>(keys: K, effect: F) {
    if crate::remember::remember_keys(keys) {
        return;
    }
    record_effect(DeferredEffect::Boxed(Box::new(effect)));
}

/// The receiver of a [`DisposableEffect`]'s body, as Compose's `DisposableEffectScope`.
pub struct DisposableEffectScope {
    _private: (),
}

/// What a [`DisposableEffect`]'s body returns: its cleanup.
pub struct DisposableEffectResult {
    on_dispose: Box<dyn FnOnce()>,
}

impl DisposableEffectScope {
    /// The cleanup to run when the keys change or the effect leaves the composition.
    pub fn on_dispose(&self, on_dispose: impl FnOnce() + 'static) -> DisposableEffectResult {
        DisposableEffectResult { on_dispose: Box::new(on_dispose) }
    }
}

type EffectBody = Box<dyn FnOnce(&DisposableEffectScope) -> DisposableEffectResult>;

struct DisposableEffectImpl {
    effect: Cell<Option<EffectBody>>,
    on_dispose: Cell<Option<Box<dyn FnOnce()>>>,
}

impl RememberObserver for DisposableEffectImpl {
    fn on_remembered(&self) {
        if let Some(effect) = self.effect.take() {
            let result = effect(&DisposableEffectScope { _private: () });
            self.on_dispose.set(Some(result.on_dispose));
        }
    }

    fn on_forgotten(&self) {
        if let Some(on_dispose) = self.on_dispose.take() {
            on_dispose();
        }
    }
}

/// Runs `effect` after the pass that first runs this call, and again whenever `keys` changes;
/// the cleanup it returns runs before each re-run and when the call leaves the composition. As
/// Compose's `DisposableEffect(keys) { …; onDispose { … } }`.
#[allow(non_snake_case)]
pub fn DisposableEffect<K: ComposeArg>(
    keys: K,
    effect: impl FnOnce(&DisposableEffectScope) -> DisposableEffectResult + 'static,
) {
    remember_observer_keyed(keys, || DisposableEffectImpl {
        effect: Cell::new(Some(Box::new(effect))),
        on_dispose: Cell::new(None),
    });
}

// ---- Launched effects ------------------------------------------------------------------------

struct LaunchedEffectImpl {
    task: Cell<Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>>>,
    job: Cell<Option<Job>>,
}

impl RememberObserver for LaunchedEffectImpl {
    fn on_remembered(&self) {
        if let Some(task) = self.task.take() {
            self.job.set(Some(crate::executor::launch(task)));
        }
    }

    fn on_forgotten(&self) {
        if let Some(job) = self.job.take() {
            job.cancel();
        }
    }
}

/// Runs `block` as a task on the UI thread's executor after the pass that first runs this call,
/// cancelling it (dropping the future) and starting the new `block` whenever `keys` changes, and
/// cancelling it when the call leaves the composition. As Compose's
/// `LaunchedEffect(keys) { … }`. A `block` given on a pass where the keys are the same is dropped
/// unpolled.
#[allow(non_snake_case)]
pub fn LaunchedEffect<K: ComposeArg>(
    keys: K,
    block: impl std::future::Future<Output = ()> + 'static,
) {
    remember_observer_keyed(keys, || LaunchedEffectImpl {
        task: Cell::new(Some(Box::pin(block))),
        job: Cell::new(None),
    });
}

/// Owns the scope: it is freed a pass after it is forgotten (and cancelled).
struct ScopeObserver(Owned<CoroutineScope>);

impl RememberObserver for ScopeObserver {
    fn on_forgotten(&self) {
        self.0.cancel();
    }

    fn on_abandoned(&self) {
        self.0.cancel();
    }
}

/// A coroutine scope bound to this point of the composition, as Compose's
/// `rememberCoroutineScope()`: launch tasks from handlers (`scope.launch(async move { … })`);
/// they are cancelled when the call leaves the composition.
pub fn remember_coroutine_scope() -> CoroutineScope {
    remember_observer(|| ScopeObserver(CoroutineScope::new())).0.handle()
}

/// A state holding the latest `value` this call was given, as Compose's
/// `rememberUpdatedState(newValue)`: an effect started earlier (keyed on something else, a
/// timeout) reads the newest callback through it. It's written during composition when the
/// value changes (by [`same`](ComposeArg::same)), so a composable reading it recomposes then.
pub fn remember_updated_state<T: ComposeArg + Clone>(value: T) -> crate::state::State<T> {
    let state = crate::remember::remember(|| {
        crate::state::mutable_state_of_with(value.clone(), crate::state::ReferentialEqualityPolicy)
    });
    state.set_while_composing(value);
    state.as_state()
}

/// The receiver of a [`produce_state`] producer, as Compose's `ProduceStateScope`: the state it
/// produces (it derefs to its [`MutableState`](crate::MutableState) handle: `scope.set(…)`),
/// and [`await_dispose`](ProduceStateScope::await_dispose).
pub struct ProduceStateScope<T: 'static> {
    state: crate::state::MutableState<T>,
}

impl<T: 'static> Clone for ProduceStateScope<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for ProduceStateScope<T> {}

impl<T: 'static> std::ops::Deref for ProduceStateScope<T> {
    type Target = crate::state::MutableState<T>;
    fn deref(&self) -> &crate::state::MutableState<T> {
        &self.state
    }
}

impl<T: 'static> ProduceStateScope<T> {
    /// Waits until the producer is cancelled (its key changed, or the call left the
    /// composition), then runs `on_dispose`, as Compose's `awaitDispose`: for a producer whose
    /// source pushes values through a callback it registered, to unregister it. It never
    /// completes.
    pub fn await_dispose(
        &self,
        on_dispose: impl FnOnce() + 'static,
    ) -> impl std::future::Future<Output = std::convert::Infallible> + 'static {
        let guard = OnDrop(Some(on_dispose));
        async move {
            let _guard = guard;
            std::future::pending::<std::convert::Infallible>().await
        }
    }
}

/// Runs its function when dropped: when a producer's task is cancelled.
struct OnDrop<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        if let Some(on_dispose) = self.0.take() {
            on_dispose();
        }
    }
}

/// State a coroutine produces, as Compose's `produceState(initialValue, key1, …) { … }`:
/// starting at `initial`, set by `producer`'s coroutine, launched as a [`LaunchedEffect`] keyed
/// on `key` (`()` for none, a tuple for several): again when `key` changes, cancelled when this
/// leaves the composition. `producer` itself runs only when the coroutine starts.
pub fn produce_state<T, K, F>(
    initial: T,
    key: K,
    producer: impl FnOnce(ProduceStateScope<T>) -> F + 'static,
) -> crate::state::State<T>
where
    T: PartialEq + 'static,
    K: ComposeArg,
    F: std::future::Future<Output = ()> + 'static,
{
    let state = crate::remember::remember(|| crate::state::mutable_state_of(initial));
    launch_producer(state, key, producer)
}

/// Like [`produce_state`], with the state's mutation policy, as
/// `produceState(initialValue, key1, mutationPolicy) { … }`.
pub fn produce_state_with<T, K, P, F>(
    initial: T,
    key: K,
    policy: P,
    producer: impl FnOnce(ProduceStateScope<T>) -> F + 'static,
) -> crate::state::State<T>
where
    T: 'static,
    K: ComposeArg,
    P: crate::state::MutationPolicy<T>,
    F: std::future::Future<Output = ()> + 'static,
{
    let state = crate::remember::remember(|| crate::state::mutable_state_of_with(initial, policy));
    launch_producer(state, key, producer)
}

fn launch_producer<T: 'static, K: ComposeArg, F: std::future::Future<Output = ()> + 'static>(
    state: crate::state::MutableState<T>,
    key: K,
    producer: impl FnOnce(ProduceStateScope<T>) -> F + 'static,
) -> crate::state::State<T> {
    LaunchedEffect(key, async move { producer(ProduceStateScope { state }).await });
    state.into()
}
