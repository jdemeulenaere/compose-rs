//! Observing snapshots themselves, as Compose's `snapshots.tooling` (experimental there): when one is
//! created, applied and disposed, and its reads and writes. For tools: with no observer, a snapshot
//! pays one check of a list's length at each of those, and reads and writes nothing (D1502).

use std::rc::Rc;

use super::global::{ChangeSet, ObserverHandle, StateObserverFn, merged_with};
use super::snapshot::Snapshot;
use crate::runtime::with_runtime;

/// An observer of the snapshot system, as Compose's `SnapshotObserver`. Using snapshots or reading
/// and writing state in its methods isn't supported, as Compose's says.
pub trait SnapshotObserver: 'static {
    /// Called before a snapshot is created, as `onPreCreate`, with the snapshot it'll be nested in,
    /// if any: what it returns observes the new snapshot's reads and writes.
    fn on_pre_create(
        &self,
        _parent: Option<&Snapshot>,
        _read_only: bool,
    ) -> Option<Rc<SnapshotInstanceObservers>> {
        None
    }

    /// Called after a snapshot is created, as `onCreated`, with what `on_pre_create` returned, to
    /// correlate the two.
    fn on_created(
        &self,
        _snapshot: &Snapshot,
        _parent: Option<&Snapshot>,
        _observers: Option<&Rc<SnapshotInstanceObservers>>,
    ) {
    }

    /// Called while a snapshot is disposed, as `onPreDispose`.
    fn on_pre_dispose(&self, _snapshot: &Snapshot) {}

    /// Called after a snapshot is applied, as `onApplied`: to its parent if it's nested, where only
    /// that parent sees them; a root's are what the apply observers hear of it.
    fn on_applied(&self, _snapshot: &Snapshot, _changed: &ChangeSet) {}
}

/// What [`SnapshotObserver::on_pre_create`] returns, as `SnapshotInstanceObservers`: called with
/// the new snapshot's reads and writes, before the observers given when taking it.
#[derive(Default)]
pub struct SnapshotInstanceObservers {
    pub read_observer: Option<StateObserverFn>,
    pub write_observer: Option<StateObserverFn>,
}

/// Observes when snapshots are created, applied and disposed, a pass's too, as
/// `Snapshot.observeSnapshots`: on this thread, as its snapshots are. Disposing or dropping the
/// handle unregisters it. A tool's API: it adds work to every snapshot while registered.
pub fn observe_snapshots(observer: impl SnapshotObserver) -> ObserverHandle {
    with_runtime(|runtime| runtime.snapshots.add_snapshot_observer(Rc::new(observer)))
}

/// The observers registered, and what each gave for a snapshot being created.
pub(super) type Creating = Vec<(Rc<dyn SnapshotObserver>, Option<Rc<SnapshotInstanceObservers>>)>;

/// The observers registered now, if any.
fn registered() -> Option<Vec<Rc<dyn SnapshotObserver>>> {
    with_runtime(|runtime| runtime.snapshots.snapshot_observers())
}

/// As `creatingSnapshot`'s start: each observer told, and its observers before those given.
pub(super) fn pre_create(
    parent: Option<&Snapshot>,
    read_only: bool,
    mut read: Option<StateObserverFn>,
    mut write: Option<StateObserverFn>,
) -> (Option<StateObserverFn>, Option<StateObserverFn>, Option<Creating>) {
    let Some(observers) = registered() else { return (read, write, None) };
    let mut creating = Vec::with_capacity(observers.len());
    for observer in observers {
        let instance = observer.on_pre_create(parent, read_only);
        if let Some(instance) = &instance {
            read = merged_with(instance.read_observer.clone(), read);
            write = merged_with(instance.write_observer.clone(), write);
        }
        creating.push((observer, instance));
    }
    (read, write, Some(creating))
}

/// As `creatingSnapshot`'s end.
pub(super) fn created(snapshot: &Snapshot, parent: Option<&Snapshot>, creating: Option<Creating>) {
    for (observer, instance) in creating.into_iter().flatten() {
        observer.on_created(snapshot, parent, instance.as_ref());
    }
}

/// As `dispatchObserverOnPreDispose`.
pub(super) fn pre_dispose(snapshot: &Snapshot) {
    for observer in registered().into_iter().flatten() {
        observer.on_pre_dispose(snapshot);
    }
}

/// As `dispatchObserverOnApplied`.
pub(super) fn applied(snapshot: &Snapshot, changed: &ChangeSet) {
    for observer in registered().into_iter().flatten() {
        observer.on_applied(snapshot, changed);
    }
}
