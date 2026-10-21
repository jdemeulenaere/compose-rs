//! Effects, written on remember observers as in Compose.

use std::cell::Cell;
use std::rc::Rc;

use crate::compose_arg::ComposeArg;
use crate::composer::record_effect;
use crate::executor::{CoroutineScope, Job};
use crate::remember::{RememberObserver, remember, remember_observer, remember_observer_keyed};
use crate::runtime::{DeferredEffect, RunEffect};

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
            self.job.set(Some(CoroutineScope::new().launch(task)));
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

struct ScopeObserver(CoroutineScope);

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
    remember_observer(|| ScopeObserver(CoroutineScope::new())).0.clone()
}

/// The latest value given, for effects started earlier to read, as Compose's
/// `rememberUpdatedState`: an effect keyed on something else (a timeout) calls the newest
/// callback. Not state: reading it observes nothing.
pub struct UpdatedState<T: 'static> {
    value: std::rc::Rc<std::cell::RefCell<T>>,
}

impl<T: 'static> Clone for UpdatedState<T> {
    fn clone(&self) -> Self {
        UpdatedState { value: self.value.clone() }
    }
}

impl<T: Clone + 'static> UpdatedState<T> {
    /// The latest value.
    pub fn get(&self) -> T {
        self.value.borrow().clone()
    }
}

impl<T: 'static> UpdatedState<T> {
    /// Reads the latest value in place.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.value.borrow())
    }
}

/// Remembers `value`, updated at each composition, as `rememberUpdatedState(value)`.
pub fn remember_updated_state<T: Clone + 'static>(value: T) -> UpdatedState<T> {
    let cell =
        crate::remember::remember(|| std::rc::Rc::new(std::cell::RefCell::new(value.clone())));
    *cell.borrow_mut() = value;
    UpdatedState { value: cell }
}

/// State a coroutine produces, as Compose's `produceState(initialValue, key) { … }`: starting at
/// `initial`, set by `producer`'s coroutine (launched, as `LaunchedEffect`, again when `key`
/// changes; cancelled when this leaves the composition).
pub fn produce_state<T, K, F>(
    initial: T,
    key: K,
    producer: impl FnOnce(crate::state::MutableState<T>) -> F,
) -> crate::state::State<T>
where
    T: PartialEq + 'static,
    K: ComposeArg,
    F: std::future::Future<Output = ()> + 'static,
{
    let state = crate::remember::remember(|| crate::state::mutable_state_of(initial));
    LaunchedEffect(key, producer(state));
    state.into()
}
