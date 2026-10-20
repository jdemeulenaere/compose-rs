//! Effects, written on remember observers as in Compose.

use std::cell::Cell;

use crate::compose_arg::ComposeArg;
use crate::composer::current;
use crate::remember::{RememberObserver, remember_observer_keyed};

/// Runs `effect` after every successful pass that ran this call, as Compose's `SideEffect`.
#[allow(non_snake_case)]
pub fn SideEffect(effect: impl FnOnce() + 'static) {
    current().record_side_effect(effect);
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
        DisposableEffectResult {
            on_dispose: Box::new(on_dispose),
        }
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
