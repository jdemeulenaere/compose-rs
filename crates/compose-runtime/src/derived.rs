//! `derived_state_of`: a state computed from other state, as Compose's `derivedStateOf`.
//!
//! The calculation runs under a read observer of its own, so the derived state learns which
//! state it read (in composition as anywhere else: those reads don't register the composing
//! scope). A write to one of them recomputes it, and its readers are invalidated only when the
//! result changed (by the state's policy): a list's emptiness read by a screen recomposes it
//! when the list becomes empty, not on every edit.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use crate::observe::ReadObserver;
use crate::remember::Remember;
use crate::state::{MutationPolicy, OwnedState, State, StructuralEqualityPolicy};

/// Owns a derived state: dropping it stops the recomputations. `remember(|| derived_state_of(…))`
/// keeps it in the slot and returns its `Copy` [`State`] handle.
pub struct DerivedState<T: 'static> {
    inner: Rc<DerivedInner<T>>,
}

struct DerivedInner<T: 'static> {
    calculation: Box<dyn Fn() -> T>,
    result: OwnedState<T>,
    /// Hears writes to what the calculation read; its notification holds a weak reference.
    observer: ReadObserver,
    /// A dependency changed and the result wasn't recomputed yet.
    dirty: Cell<bool>,
}

/// A state whose value is `calculation`'s, recomputed when the state it read changes, as
/// Compose's `derivedStateOf`. Readers are invalidated only when the value changes (`==`).
#[track_caller]
pub fn derived_state_of<T: PartialEq + 'static>(
    calculation: impl Fn() -> T + 'static,
) -> DerivedState<T> {
    derived_state_of_with(StructuralEqualityPolicy, calculation)
}

/// Like [`derived_state_of`], with a mutation policy deciding when the value changed, as
/// `derivedStateOf(policy, calculation)`.
#[track_caller]
pub fn derived_state_of_with<T: 'static, P: MutationPolicy<T>>(
    policy: P,
    calculation: impl Fn() -> T + 'static,
) -> DerivedState<T> {
    let inner = Rc::new_cyclic(|weak: &Weak<DerivedInner<T>>| {
        let weak = weak.clone();
        let observer = ReadObserver::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.recompute();
            }
        });
        let value = observer.observe_exclusive(0, &calculation);
        DerivedInner {
            calculation: Box::new(calculation),
            result: crate::state::mutable_state_of_with(value, policy),
            observer,
            dirty: Cell::new(false),
        }
    });
    DerivedState { inner }
}

impl<T: 'static> DerivedInner<T> {
    /// Drains the invalidations; whether a dependency changed since the last calculation.
    fn changed(&self) -> bool {
        let mut changed = self.dirty.get();
        self.observer.drain_invalidated(|_| changed = true);
        changed
    }

    fn calculate(&self) -> T {
        self.observer.observe_exclusive(0, &self.calculation)
    }

    /// Recomputes the value if a dependency changed, and stores it (readers hear of it if it
    /// changed). During composition, where state can't be written, the value is returned and the
    /// store waits for the notification after the pass.
    fn recompute(&self) -> Option<T> {
        if !self.changed() {
            return None;
        }
        let value = self.calculate();
        if crate::runtime::with_runtime(|runtime| runtime.is_composing()) {
            self.dirty.set(true);
            return Some(value);
        }
        self.dirty.set(false);
        self.result.set(value);
        None
    }
}

impl<T: 'static> DerivedState<T> {
    /// The value, recomputed first if a dependency changed. Reading it registers the reader, as
    /// reading any state does.
    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let fresh = self.inner.recompute();
        let kept = self.inner.result.get();
        fresh.unwrap_or(kept)
    }

    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        match self.inner.recompute() {
            Some(fresh) => {
                // Registers the read.
                self.inner.result.with(|_| {});
                f(&fresh)
            }
            None => self.inner.result.with(f),
        }
    }

    /// The `Copy` read-only handle of the value.
    pub fn as_state(&self) -> State<T> {
        self.inner.result.as_state()
    }
}

impl<T: 'static> Remember for DerivedState<T> {
    type Stored = DerivedState<T>;
    type Output = State<T>;

    fn into_stored(self) -> DerivedState<T> {
        self
    }

    fn output(stored: &DerivedState<T>) -> State<T> {
        stored.as_state()
    }
}

impl<T: 'static> std::fmt::Debug for DerivedState<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "DerivedState({:?})", self.inner.result)
    }
}
