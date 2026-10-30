//! State objects an app builds itself, as Compose's `StateObject` and `StateRecord`: a record type,
//! which says how to copy and merge itself, kept by snapshot as a [`MutableState`] keeps a value
//! (docs/parity/design/snapshots.md §3.3). A state collection is one of these, more or less.

use std::panic::Location;

use super::StateObjectId;
use crate::state::{MergeResult, MutableState, OwnedState, state_with_merge};

/// What a state object keeps per snapshot, as Compose's `StateRecord`.
pub trait StateRecord: Sized + 'static {
    /// A copy, for a [`writable`](SnapshotStateObject::writable) write in a snapshot that can't
    /// write the record it reads, as `assign` onto `create`.
    fn copy(&self) -> Self;

    /// What a conflict at an apply comes to, as `StateObject.mergeRecords`: `previous` is what the
    /// applying snapshot started from, `current` what was written meanwhile, `applied` what it
    /// wrote. Failing the apply is the default.
    fn merge(_previous: &Self, _current: &Self, _applied: &Self) -> MergeResult<Self> {
        MergeResult::Fail
    }
}

/// A `Copy` handle to a state object holding records of type `R`; [`OwnedStateObject`] owns it. A
/// read is observed as any state's; a write is told to the snapshot and its observers.
pub struct SnapshotStateObject<R: StateRecord> {
    state: MutableState<R>,
}

impl<R: StateRecord> Clone for SnapshotStateObject<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: StateRecord> Copy for SnapshotStateObject<R> {}

impl<R: StateRecord> SnapshotStateObject<R> {
    /// `f` of the record this snapshot sees, an observed read, as `readable`.
    #[track_caller]
    pub fn readable<T>(&self, f: impl FnOnce(&R) -> T) -> T {
        self.state.with(f)
    }

    /// `f` of the record this snapshot sees, read by nothing, as `withCurrent`.
    #[track_caller]
    pub fn with_current<T>(&self, f: impl FnOnce(&R) -> T) -> T {
        self.state.with_current(f)
    }

    /// Changes the record this snapshot writes, a copy of the one it reads if it must make one, as
    /// `writable`.
    #[track_caller]
    pub fn writable<T>(&self, f: impl FnOnce(&mut R) -> T) -> T {
        self.state.edit_with(R::copy, f)
    }

    /// Replaces the record this snapshot writes by what `f` makes of the one it reads, as
    /// `overwritable`: no copy is made.
    #[track_caller]
    pub fn overwritable(&self, f: impl FnOnce(&R) -> R) {
        let record = self.state.with_current(f);
        self.state.set(record);
    }

    /// [`writable`](Self::writable) in `snapshot`, as `writable(state, snapshot)`.
    #[track_caller]
    pub fn writable_in<T>(&self, snapshot: &super::Snapshot, f: impl FnOnce(&mut R) -> T) -> T {
        snapshot.enter(|| self.writable(f))
    }

    /// [`readable`](Self::readable) in `snapshot`, as `readable(state, snapshot)`.
    #[track_caller]
    pub fn readable_in<T>(&self, snapshot: &super::Snapshot, f: impl FnOnce(&R) -> T) -> T {
        snapshot.enter(|| self.readable(f))
    }

    /// The id of the snapshot that wrote the record this snapshot reads, as the record's
    /// `snapshotId`. Records aren't otherwise reachable: no `firstStateRecord` or
    /// `prependStateRecord`, the chain being the runtime's (D784).
    pub fn record_snapshot_id(&self) -> u64 {
        crate::state::record_snapshot_id(self.state)
    }

    /// The object's identity, as observers are told it.
    pub fn state_object_id(&self) -> StateObjectId {
        self.state.state_object_id()
    }
}

/// Owns a state object: dropping it retires the object, as an [`OwnedState`]'s cell. It derefs to
/// its `Copy` handle.
pub struct OwnedStateObject<R: StateRecord> {
    handle: SnapshotStateObject<R>,
    _owner: OwnedState<R>,
}

impl<R: StateRecord> std::ops::Deref for OwnedStateObject<R> {
    type Target = SnapshotStateObject<R>;
    fn deref(&self) -> &SnapshotStateObject<R> {
        &self.handle
    }
}

impl<R: StateRecord> OwnedStateObject<R> {
    /// The `Copy` handle, for closures.
    pub fn handle(&self) -> SnapshotStateObject<R> {
        self.handle
    }
}

/// A new state object whose first record is `record`.
#[track_caller]
pub fn snapshot_state_object<R: StateRecord>(record: R) -> OwnedStateObject<R> {
    let owner = state_with_merge(record, R::merge, Location::caller());
    OwnedStateObject { handle: SnapshotStateObject { state: owner.handle() }, _owner: owner }
}
