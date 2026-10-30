//! `SnapshotStateObserver`, as Compose's: observes the state reads of blocks by scope, and
//! calls a scope's callback when an applied change touches what it read (D753).
//!
//! It's built on the runtime's keyed read observer (`platform::ReadObserver`, which compose-ui's
//! layout and drawing use): each callback gets one, and its scopes are interned to keys. Started,
//! it's an apply observer as others are, heard in registration order (D757): it maps the states
//! changed to the keys that read them, and those to scopes, and calls the callbacks through its
//! executor.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::{Rc, Weak};

use super::{ChangeSet, ObserverHandle, Snapshot};
use crate::observe::ReadObserver;

/// What a callback group does without knowing its scope type.
trait Group: Any {
    /// The callback's identity: groups are per callback, as Compose's scope maps.
    fn callback(&self) -> *const ();
    /// Forgets `scope`'s observations, if it's of this group's scope type.
    fn clear_scope(&self, scope: &dyn Any);
    fn clear_if(&self, predicate: &dyn Fn(&dyn Any) -> bool);
    fn clear_all(&self);
    /// Calls the callback for the scopes that read a state in `changes`.
    fn notify_changes(&self, changes: &ChangeSet);
    fn has_scopes(&self) -> bool;
}

/// The scopes of one callback, and its keyed observer.
struct ScopeGroup<S: 'static> {
    owner: Weak<Inner>,
    callback: Rc<dyn Fn(&S)>,
    observer: ReadObserver,
    keys: RefCell<HashMap<S, u64>>,
    scopes: RefCell<HashMap<u64, S>>,
    next_key: Cell<u64>,
}

/// What runs the callbacks: Compose's `onChangedExecutor`.
type Executor = Box<dyn Fn(Box<dyn FnOnce()>)>;

struct Inner {
    executor: Executor,
    groups: RefCell<Vec<Rc<dyn Group>>>,
    /// While started: its registration as an apply observer.
    registration: RefCell<Option<ObserverHandle>>,
}

impl Inner {
    fn notify_changes(&self, changes: &ChangeSet) {
        let groups: Vec<Rc<dyn Group>> = self.groups.borrow().clone();
        for group in groups {
            group.notify_changes(changes);
        }
    }
}

impl<S: Hash + Eq + Clone + 'static> ScopeGroup<S> {
    fn new(owner: &Rc<Inner>, callback: Rc<dyn Fn(&S)>) -> Rc<ScopeGroup<S>> {
        Rc::new_cyclic(|group: &Weak<ScopeGroup<S>>| {
            let group = group.clone();
            let observer = ReadObserver::new(move || {
                if let Some(group) = group.upgrade() {
                    group.invalidated();
                }
            });
            observer.set_apply_driven();
            ScopeGroup {
                owner: Rc::downgrade(owner),
                callback,
                observer,
                keys: RefCell::new(HashMap::new()),
                scopes: RefCell::new(HashMap::new()),
                next_key: Cell::new(1),
            }
        })
    }

    /// The key of `scope`, interned.
    fn key(&self, scope: &S) -> u64 {
        if let Some(&key) = self.keys.borrow().get(scope) {
            return key;
        }
        let key = self.next_key.get();
        self.next_key.set(key + 1);
        self.keys.borrow_mut().insert(scope.clone(), key);
        self.scopes.borrow_mut().insert(key, scope.clone());
        key
    }

    /// A derived state's result it read changed, which isn't a change sent: the scopes are
    /// called, once each, if the observer is started.
    fn invalidated(&self) {
        let mut keys = Vec::new();
        self.observer.drain_invalidated(|key| keys.push(key));
        if self.owner.upgrade().is_some_and(|owner| owner.registration.borrow().is_some()) {
            self.fire(keys);
        }
    }

    fn fire(&self, mut keys: Vec<u64>) {
        let Some(owner) = self.owner.upgrade() else { return };
        if keys.is_empty() {
            return;
        }
        keys.sort_unstable();
        keys.dedup();
        let scopes: Vec<S> = {
            let known = self.scopes.borrow();
            keys.iter().filter_map(|key| known.get(key).cloned()).collect()
        };
        if scopes.is_empty() {
            return;
        }
        let callback = self.callback.clone();
        (owner.executor)(Box::new(move || {
            for scope in &scopes {
                callback(scope);
            }
        }));
    }

    fn forget(&self, key: u64) {
        self.observer.forget(key);
        if let Some(scope) = self.scopes.borrow_mut().remove(&key) {
            self.keys.borrow_mut().remove(&scope);
        }
    }
}

impl<S: Hash + Eq + Clone + 'static> Group for ScopeGroup<S> {
    fn callback(&self) -> *const () {
        Rc::as_ptr(&self.callback).cast()
    }

    fn clear_scope(&self, scope: &dyn Any) {
        let Some(scope) = scope.downcast_ref::<S>() else { return };
        let key = self.keys.borrow().get(scope).copied();
        if let Some(key) = key {
            self.forget(key);
        }
    }

    fn clear_if(&self, predicate: &dyn Fn(&dyn Any) -> bool) {
        let keys: Vec<u64> = self
            .scopes
            .borrow()
            .iter()
            .filter(|(_, scope)| predicate(*scope))
            .map(|(&key, _)| key)
            .collect();
        for key in keys {
            self.forget(key);
        }
    }

    fn clear_all(&self) {
        let keys: Vec<u64> = self.scopes.borrow().keys().copied().collect();
        for key in keys {
            self.forget(key);
        }
    }

    fn notify_changes(&self, changes: &ChangeSet) {
        let mut keys = Vec::new();
        for state in changes.iter() {
            keys.extend(self.observer.keys_reading(state.0));
        }
        self.fire(keys);
    }

    fn has_scopes(&self) -> bool {
        !self.scopes.borrow().is_empty()
    }
}

/// Observes the state reads of blocks under scopes, and calls a scope's callback when a change
/// applied to the global snapshot touches what it read, as Compose's `SnapshotStateObserver`.
/// Callbacks run through `on_changed_executor` (Compose's `onChangedExecutor`; `|run| run()`
/// runs them at the send). It hears changes only while [started](Self::start).
///
/// Thread-bound, as state is.
pub struct SnapshotStateObserver {
    inner: Rc<Inner>,
}

impl SnapshotStateObserver {
    pub fn new(on_changed_executor: impl Fn(Box<dyn FnOnce()>) + 'static) -> SnapshotStateObserver {
        SnapshotStateObserver {
            inner: Rc::new(Inner {
                executor: Box::new(on_changed_executor),
                groups: RefCell::new(Vec::new()),
                registration: RefCell::new(None),
            }),
        }
    }

    /// Runs `block`, recording the states it reads under `scope` (replacing what an earlier
    /// `observe_reads` of `scope` with this callback recorded): when one changes,
    /// `on_value_changed_for_scope` is called with `scope`, as `observeReads`. Callbacks are
    /// grouped by identity: pass the same `Rc` each time. Observation pauses inside
    /// [`Snapshot::without_read_observation`]; an `observe_reads` nested in `block` records its
    /// own reads.
    pub fn observe_reads<S: Hash + Eq + Clone + 'static, R>(
        &self,
        scope: S,
        on_value_changed_for_scope: &Rc<dyn Fn(&S)>,
        block: impl FnOnce() -> R,
    ) -> R {
        let group = self.group(on_value_changed_for_scope);
        let key = group.key(&scope);
        group.observer.observe(key, block)
    }

    /// The group of `callback`, made on first use.
    fn group<S: Hash + Eq + Clone + 'static>(
        &self,
        callback: &Rc<dyn Fn(&S)>,
    ) -> Rc<ScopeGroup<S>> {
        let pointer: *const () = Rc::as_ptr(callback).cast();
        let existing =
            self.inner.groups.borrow().iter().find(|group| group.callback() == pointer).cloned();
        if let Some(group) = existing {
            let group: Rc<dyn Any> = group;
            if let Ok(group) = group.downcast::<ScopeGroup<S>>() {
                return group;
            }
        }
        let group = ScopeGroup::new(&self.inner, callback.clone());
        self.inner.groups.borrow_mut().push(group.clone() as Rc<dyn Group>);
        group
    }

    /// Forgets `scope`'s observations, for every callback, as `clear(scope)`.
    pub fn clear<S: 'static>(&self, scope: &S) {
        for group in self.groups() {
            group.clear_scope(scope);
        }
        self.drop_empty_groups();
    }

    /// Forgets the observations of the scopes `predicate` accepts, as `clearIf`.
    pub fn clear_if(&self, predicate: impl Fn(&dyn Any) -> bool) {
        for group in self.groups() {
            group.clear_if(&predicate);
        }
        self.drop_empty_groups();
    }

    /// Starts hearing changes, as `start()`: an apply observer, registered now, so what's
    /// pending is sent first and not heard.
    pub fn start(&self) {
        if self.inner.registration.borrow().is_some() {
            return;
        }
        let inner = Rc::downgrade(&self.inner);
        let registration = Snapshot::register_apply_observer(move |changes, _| {
            if let Some(inner) = inner.upgrade() {
                inner.notify_changes(changes);
            }
        });
        *self.inner.registration.borrow_mut() = Some(registration);
    }

    /// Stops hearing changes, as `stop()`.
    pub fn stop(&self) {
        let registration = self.inner.registration.borrow_mut().take();
        drop(registration);
    }

    /// Tells the observer `changes` were applied, as `notifyChanges`, started or not.
    pub fn notify_changes(&self, changes: &ChangeSet, _snapshot: &Snapshot) {
        self.inner.notify_changes(changes);
    }

    /// Forgets every observation, as `clear()`.
    pub fn clear_all(&self) {
        for group in self.groups() {
            group.clear_all();
        }
    }

    fn groups(&self) -> Vec<Rc<dyn Group>> {
        self.inner.groups.borrow().clone()
    }

    fn drop_empty_groups(&self) {
        let removed: Vec<Rc<dyn Group>> = {
            let mut groups = self.inner.groups.borrow_mut();
            let (kept, removed) = groups.drain(..).partition(|group| group.has_scopes());
            *groups = kept;
            removed
        };
        drop(removed);
    }
}
