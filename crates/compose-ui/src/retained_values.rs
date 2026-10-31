//! What a host's content retains across hosts on one view tree, as Compose's
//! `LifecycleRetainedValuesStoreOwner` (D1306): a list of stores per view id, so two views with one
//! id each have their own, retaining only from a host's stop until a frame after its resume.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::platform::CancellationHandle;
use compose_runtime::retain::{
    ManagedRetainedValuesStore, RetainKey, RetainedValue, RetainedValuesStore,
};

/// A store driven by a lifecycle rather than by its content, as `LifecycleRetainedValuesStore`:
/// resumed, what leaves is retired at once.
struct LifecycleRetainedValuesStore {
    delegate: Rc<ManagedRetainedValuesStore>,
    retaining: Cell<bool>,
}

impl RetainedValuesStore for LifecycleRetainedValuesStore {
    fn consume_exited_value(&self, key: &RetainKey) -> Option<RetainedValue> {
        self.delegate.consume_exited_value(key)
    }

    fn save_exiting_value(&self, key: RetainKey, value: RetainedValue) {
        self.delegate.save_exiting_value(key, value);
    }

    // Its content's coming and going says nothing: its lifecycle does.
    fn on_content_entered_composition(&self) {}

    fn on_content_exit_composition(&self) {}
}

/// One view's store in a [`LifecycleRetainedValuesStoreOwner`], as `RetainedValuesStoreEntry`.
pub struct RetainedValuesStoreEntry {
    store: Rc<LifecycleRetainedValuesStore>,
    in_use: Cell<bool>,
    ending: Cell<Option<CancellationHandle>>,
}

impl RetainedValuesStoreEntry {
    fn new() -> RetainedValuesStoreEntry {
        let delegate = ManagedRetainedValuesStore::new();
        // Resumed, until it's told otherwise.
        delegate.on_content_entered_composition();
        RetainedValuesStoreEntry {
            store: Rc::new(LifecycleRetainedValuesStore { delegate, retaining: Cell::new(false) }),
            in_use: Cell::new(false),
            ending: Cell::new(None),
        }
    }

    /// The store the content goes under.
    pub fn store(&self) -> Rc<dyn RetainedValuesStore> {
        self.store.clone()
    }

    fn cancel_ending(&self) {
        if let Some(ending) = self.ending.take() {
            ending.cancel();
        }
    }

    /// The lifecycle stopped: what leaves from now on stays.
    pub fn start_retaining_exited_values(&self) {
        self.cancel_ending();
        if !self.store.retaining.replace(true) {
            self.store.delegate.on_content_exit_composition();
        }
    }

    /// The lifecycle resumed: `schedule` runs what it's given at the end of the host's frame, when
    /// what's kept and not taken back is retired; given nothing to schedule on, at once.
    pub fn stop_retaining_exited_values(
        self: &Rc<Self>,
        schedule: impl FnOnce(Box<dyn FnOnce()>) -> Option<CancellationHandle>,
    ) {
        if !self.store.retaining.get() {
            return;
        }
        self.cancel_ending();
        let scheduled = Rc::downgrade(self);
        let handle = schedule(Box::new(move || {
            if let Some(entry) = scheduled.upgrade() {
                entry.end_retaining();
            }
        }));
        match handle {
            Some(handle) => self.ending.set(Some(handle)),
            None => self.end_retaining(),
        }
    }

    fn end_retaining(&self) {
        self.ending.set(None);
        if self.store.retaining.replace(false) {
            self.store.delegate.on_content_entered_composition();
        }
    }

    /// Its host went: another view of the id may take it.
    pub fn release(&self) {
        self.in_use.set(false);
    }

    fn clear(&self) {
        self.cancel_ending();
        self.store.delegate.dispose();
    }
}

/// The stores of the views under one holder, as `LifecycleRetainedValuesStoreOwner`.
#[derive(Default)]
pub struct LifecycleRetainedValuesStoreOwner {
    scopes: RefCell<Vec<(i32, Vec<Rc<RetainedValuesStoreEntry>>)>>,
}

impl LifecycleRetainedValuesStoreOwner {
    pub fn new() -> Rc<LifecycleRetainedValuesStoreOwner> {
        Rc::default()
    }

    /// A store of `view_id` no host's using, made if there's none: views with one id each have
    /// their own, in the order they come.
    pub fn get_or_create_entry(&self, view_id: i32) -> Rc<RetainedValuesStoreEntry> {
        let mut scopes = self.scopes.borrow_mut();
        let at = match scopes.iter().position(|(id, _)| *id == view_id) {
            Some(at) => at,
            None => {
                scopes.push((view_id, Vec::new()));
                scopes.len() - 1
            }
        };
        let entries = &mut scopes[at].1;
        let entry = match entries.iter().find(|entry| !entry.in_use.get()) {
            Some(entry) => entry.clone(),
            None => {
                let entry = Rc::new(RetainedValuesStoreEntry::new());
                entries.push(entry.clone());
                entry
            }
        };
        entry.in_use.set(true);
        entry
    }

    /// The holder's cleared for good: what's kept is retired, and what leaves from now on. A host
    /// still on one of its stores goes on composing, keeping nothing.
    pub fn clear(&self) {
        let scopes = std::mem::take(&mut *self.scopes.borrow_mut());
        for (_, entries) in scopes {
            for entry in entries {
                entry.clear();
            }
        }
    }
}
