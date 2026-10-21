//! `derived_state_of`: a state computed from other state, as Compose's `derivedStateOf`.
//!
//! The calculation runs under a read observer of its own, so the derived state learns which
//! state it read (in composition as anywhere else: those reads don't register the composing
//! scope). A write to one of them marks the result dirty, at once and without computing (a
//! handler's writes may be half done): a read recomputes it first, so no read is ever behind
//! what it derives from, and each frame (`flush_derived_states`, from `Composition::recompose`
//! and `has_invalidations`) recomputes the dirty ones, invalidating their readers only when the
//! result changed (by the state's policy): a list's emptiness read by a screen recomposes it
//! when the list becomes empty, not on every edit.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::observe::ReadObserver;
use crate::remember::Remember;
use crate::state::{MutationPolicy, OwnedState, State, StructuralEqualityPolicy};

/// A derived state's recomputation, by its result cell.
pub(crate) trait Refresh {
    /// Recomputes the result if it is dirty, and stores it.
    fn refresh(&self);
}

thread_local! {
    /// Derived states whose dependencies changed since the last flush.
    static PENDING: RefCell<Vec<Weak<dyn Refresh>>> = const { RefCell::new(Vec::new()) };
}

/// Recomputes the derived states whose dependencies changed, telling their readers if they did.
/// For hosts and compositions, before a frame recomposes.
pub fn flush_derived_states() {
    loop {
        let pending = PENDING
            .try_with(|pending| std::mem::take(&mut *pending.borrow_mut()))
            .unwrap_or_default();
        if pending.is_empty() {
            return;
        }
        for derived in pending {
            if let Some(derived) = derived.upgrade() {
                derived.refresh();
            }
        }
    }
}

/// Whether a derived state's dependencies changed since the last flush.
pub(crate) fn has_pending_derived_states() -> bool {
    PENDING.try_with(|pending| !pending.borrow().is_empty()).unwrap_or(false)
}

/// Owns a derived state: dropping it stops the recomputations. `remember(|| derived_state_of(…))`
/// keeps it in the slot and returns its `Copy` [`State`] handle.
pub struct DerivedState<T: 'static> {
    inner: Rc<DerivedInner<T>>,
}

struct DerivedInner<T: 'static> {
    calculation: Box<dyn Fn() -> T>,
    result: OwnedState<T>,
    /// Hears writes to what the calculation read: marks the result dirty (in the write) and
    /// queues the flush.
    observer: ReadObserver,
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
        let weak: Weak<dyn Refresh> = weak.clone();
        let observer = ReadObserver::new(move || {
            // Computed later (by a read or the next frame), never inside the write.
            let weak = weak.clone();
            let _ = PENDING.try_with(|pending| pending.borrow_mut().push(weak));
            crate::executor::wake_host();
        });
        let value = observer.observe_exclusive(0, &calculation);
        DerivedInner {
            calculation: Box::new(calculation),
            result: crate::state::mutable_state_of_with(value, policy),
            observer,
        }
    });
    let id = inner.result.handle().id();
    inner.observer.set_dirty_cell(id);
    let refresh: Weak<dyn Refresh> = Rc::downgrade(&inner) as Weak<dyn Refresh>;
    crate::state::set_refresh(id, refresh);
    DerivedState { inner }
}

impl<T: 'static> Refresh for DerivedInner<T> {
    fn refresh(&self) {
        let id = self.result.handle().id();
        if !crate::state::take_dirty(id) {
            return;
        }
        // The keys invalidated since: this calculation records the reads anew.
        self.observer.drain_invalidated(|_| {});
        let value = self.observer.observe_exclusive(0, &self.calculation);
        self.result.handle().set_derived(value);
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
        self.inner.result.get()
    }

    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.inner.result.with(f)
    }

    /// The `Copy` read-only handle of the value: reads through it are fresh too.
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
