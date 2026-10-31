//! Observing state reads outside composition, as Compose's `SnapshotStateObserver`: a UI reads
//! state while measuring or drawing a node, under a key naming that node, and hears which keys a
//! later write invalidated, so it measures or draws them again without recomposing.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::hash::KeyHasher;
use crate::runtime::{try_with_runtime, with_runtime};

/// A registered observer.
pub(crate) struct ObserverEntry {
    serial: u64,
    notify: Rc<dyn Fn()>,
    /// The current observation of each key: reads from older ones are stale.
    // FNV, not SipHash: looked up for each observation (a node's measure, a node's draw), by
    // keys no one chooses.
    epochs: HashMap<u64, u32, BuildHasherDefault<KeyHasher>>,
    invalidated: Vec<u64>,
    /// `notify` is queued, and not called again until the invalidated keys are drained.
    pending: bool,
    /// A derived state's result cell, marked dirty when a read is invalidated.
    dirty_cell: Option<crate::state::StateId>,
}

/// The observation in progress: reads register under it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Observing {
    pub(crate) observer: u32,
    pub(crate) key: u64,
    pub(crate) epoch: u32,
    /// Reads register here even during composition (a derived state's calculation), not with
    /// the composing scope.
    pub(crate) exclusive: bool,
    /// The snapshot observer frames when it started: a `without_read_observation` started
    /// since stops it recording.
    pub(crate) depth: u32,
}

/// A state read observed under a key.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ObservedRead {
    pub(crate) observer: u32,
    pub(crate) key: u64,
    pub(crate) epoch: u32,
}

pub(crate) struct Observers {
    entries: Vec<Option<ObserverEntry>>,
    serial: u64,
}

impl Observers {
    pub(crate) const fn new() -> Observers {
        Observers { entries: Vec::new(), serial: 0 }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.iter().all(Option::is_none)
    }

    /// Whether a read is from the latest observation of its key.
    pub(crate) fn is_current(&self, read: &ObservedRead) -> bool {
        self.entries
            .get(read.observer as usize)
            .and_then(Option::as_ref)
            .is_some_and(|entry| entry.epochs.get(&read.key) == Some(&read.epoch))
    }

    /// The derived state result `read`'s observer computes, if it is a derived state's.
    pub(crate) fn dirty_cell(&self, read: &ObservedRead) -> Option<crate::state::StateId> {
        self.entries.get(read.observer as usize)?.as_ref()?.dirty_cell
    }

    /// Records that a write invalidated `read`; returns the notification to queue, if any.
    pub(crate) fn invalidate(&mut self, read: &ObservedRead) -> Option<Rc<dyn Fn()>> {
        let entry = self.entries.get_mut(read.observer as usize)?.as_mut()?;
        if !entry.invalidated.contains(&read.key) {
            entry.invalidated.push(read.key);
        }
        // A derived state's observer needs no notification: the write marks its result dirty.
        if entry.pending || entry.dirty_cell.is_some() {
            return None;
        }
        entry.pending = true;
        Some(entry.notify.clone())
    }

    fn entry(&mut self, index: u32, serial: u64) -> Option<&mut ObserverEntry> {
        self.entries.get_mut(index as usize)?.as_mut().filter(|entry| entry.serial == serial)
    }
}

/// Observes the state reads of code run under it, by key, as Compose's `SnapshotStateObserver`.
/// Thread-bound, as state is.
pub struct ReadObserver {
    index: u32,
    serial: u64,
    _not_send: PhantomData<*const ()>,
}

impl ReadObserver {
    /// An observer calling `notify` (once until drained) when a write invalidates a read it
    /// observed.
    pub fn new(notify: impl Fn() + 'static) -> ReadObserver {
        with_runtime(|runtime| {
            let observers = &mut runtime.observers;
            observers.serial += 1;
            let entry = ObserverEntry {
                serial: observers.serial,
                notify: Rc::new(notify),
                epochs: HashMap::default(),
                invalidated: Vec::new(),
                pending: false,
                dirty_cell: None,
            };
            let index = match observers.entries.iter().position(Option::is_none) {
                Some(index) => {
                    observers.entries[index] = Some(entry);
                    index
                }
                None => {
                    observers.entries.push(Some(entry));
                    observers.entries.len() - 1
                }
            };
            ReadObserver { index: index as u32, serial: observers.serial, _not_send: PhantomData }
        })
    }

    /// Runs `f`, recording the state it reads under `key` (replacing what an earlier run under
    /// `key` read). Observations nest: an inner one records its own reads, or adds them to the
    /// outer one's if it has the same key.
    pub fn observe<R>(&self, key: u64, f: impl FnOnce() -> R) -> R {
        self.observe_with(key, false, f)
    }

    /// Like [`observe`](Self::observe), keeping what earlier runs under `key` read: code that runs
    /// a part of what's observed there, ahead of the rest.
    pub fn observe_adding<R>(&self, key: u64, f: impl FnOnce() -> R) -> R {
        self.observe_inner(key, false, true, f)
    }

    /// Like [`observe`](Self::observe), recording reads made during composition too, instead of
    /// registering them with the composing scope: a derived state's calculation.
    pub(crate) fn observe_exclusive<R>(&self, key: u64, f: impl FnOnce() -> R) -> R {
        self.observe_with(key, true, f)
    }

    fn observe_with<R>(&self, key: u64, exclusive: bool, f: impl FnOnce() -> R) -> R {
        self.observe_inner(key, exclusive, false, f)
    }

    fn observe_inner<R>(
        &self,
        key: u64,
        exclusive: bool,
        adding: bool,
        f: impl FnOnce() -> R,
    ) -> R {
        let previous = with_runtime(|runtime| {
            // Inside an observation of the same key (a node's layout modifiers around its
            // policy), the reads add to it, seen from here: a `without_read_observation` around
            // this one hides what's read outside it, not in it.
            let previous = runtime.observing;
            if let Some(observing) = runtime
                .observing
                .as_mut()
                .filter(|observing| observing.observer == self.index && observing.key == key)
            {
                observing.depth = runtime.snapshots.depth();
                return previous;
            }
            let epoch = match runtime.observers.entry(self.index, self.serial) {
                Some(entry) => {
                    let epoch = entry.epochs.entry(key).or_insert(0);
                    if !adding {
                        *epoch = epoch.wrapping_add(1);
                    }
                    *epoch
                }
                None => return runtime.observing,
            };
            let depth = runtime.snapshots.depth();
            runtime.observing.replace(Observing {
                observer: self.index,
                key,
                epoch,
                exclusive,
                depth,
            })
        });
        struct Restore(Option<Observing>);
        impl Drop for Restore {
            fn drop(&mut self) {
                let previous = self.0;
                let _ = try_with_runtime(|runtime| runtime.observing = previous);
            }
        }
        let _restore = Restore(previous);
        f()
    }

    /// Makes this observer a derived state's: an invalidated read marks `cell` dirty.
    pub(crate) fn set_dirty_cell(&self, cell: crate::state::StateId) {
        with_runtime(|runtime| {
            if let Some(entry) = runtime.observers.entry(self.index, self.serial) {
                entry.dirty_cell = Some(cell);
            }
        });
    }

    /// Calls `f` with each key a write invalidated since the last drain.
    pub fn drain_invalidated(&self, mut f: impl FnMut(u64)) {
        let keys = with_runtime(|runtime| {
            runtime.observers.entry(self.index, self.serial).map(|entry| {
                entry.pending = false;
                std::mem::take(&mut entry.invalidated)
            })
        });
        let Some(mut keys) = keys else { return };
        for &key in &keys {
            f(key);
        }
        keys.clear();
        // Back, keeping its capacity.
        with_runtime(|runtime| {
            if let Some(entry) = runtime.observers.entry(self.index, self.serial)
                && entry.invalidated.is_empty()
            {
                entry.invalidated = keys;
            }
        });
    }

    /// What forgets `key`'s reads when dropped.
    pub(crate) fn forgetting(&self, key: u64) -> ForgetKey {
        ForgetKey { index: self.index, serial: self.serial, key }
    }

    /// The keys whose current observation read `state`.
    pub(crate) fn keys_reading(&self, state: crate::state::StateId) -> Vec<u64> {
        with_runtime(|runtime| crate::state::observed_keys(runtime, state, self.index))
    }

    /// Forgets `key`'s reads (its node is gone).
    pub fn forget(&self, key: u64) {
        with_runtime(|runtime| {
            if let Some(entry) = runtime.observers.entry(self.index, self.serial) {
                entry.epochs.remove(&key);
            }
        });
    }
}

/// Forgets a key's reads when dropped, as [`ReadObserver::forget`]: what ties a derived state's
/// reads in a snapshot to the record they made (D781).
pub(crate) struct ForgetKey {
    index: u32,
    serial: u64,
    pub(crate) key: u64,
}

impl Drop for ForgetKey {
    fn drop(&mut self) {
        let _ = try_with_runtime(|runtime| {
            if let Some(entry) = runtime.observers.entry(self.index, self.serial) {
                entry.epochs.remove(&self.key);
            }
        });
    }
}

impl Drop for ReadObserver {
    fn drop(&mut self) {
        let removed = try_with_runtime(|runtime| {
            let entry = runtime.observers.entries.get_mut(self.index as usize)?;
            if entry.as_ref().is_some_and(|entry| entry.serial == self.serial) {
                entry.take()
            } else {
                None
            }
        })
        .flatten();
        // Its `notify` drops here, outside the borrow.
        drop(removed);
    }
}
