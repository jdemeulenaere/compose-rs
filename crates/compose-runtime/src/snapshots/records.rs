//! A state object's records, as Compose's `StateRecord` chains: what each snapshot sees of the
//! object. A cell keeps one record inline, the head; more come only while snapshots other than
//! the global one are open and need an older value (docs/parity/design/snapshots.md §3.3).

use std::any::Any;

use super::id_set::IdSet;

/// A snapshot's id, as Compose's `SnapshotId`: records carry the id of the snapshot that wrote
/// them.
pub(crate) type SnapshotId = u64;

/// The id no snapshot has, as `INVALID_SNAPSHOT`.
pub(crate) const INVALID: SnapshotId = 0;

/// The id of a record every snapshot can read, as `PreexistingSnapshotId`: a state made in a
/// snapshot other than the global one is readable everywhere (`testSnapshotStateIsBornAccessible`).
pub(crate) const PREEXISTING: SnapshotId = 1;

/// The id of the first global snapshot (Compose's ids start there).
pub(crate) const FIRST_GLOBAL: SnapshotId = 2;

/// What a read or a write sees through, when isolation is in use: a snapshot's id and invalid set.
#[derive(Clone)]
pub(crate) struct View {
    pub(crate) id: SnapshotId,
    pub(crate) invalid: IdSet,
    /// Writes are refused.
    pub(crate) read_only: bool,
    /// The global snapshot's: a write may overwrite in place a record no open snapshot can read,
    /// one above `open_highest`.
    pub(crate) global: bool,
    pub(crate) open_highest: SnapshotId,
}

impl View {
    /// As Compose's `valid(currentSnapshot, candidateSnapshot, invalid)`.
    #[inline]
    pub(crate) fn valid(&self, id: SnapshotId) -> bool {
        id != INVALID && id <= self.id && !self.invalid.get(id)
    }

    /// A view through `id` and `invalid` alone, as the apply's readable calls.
    pub(crate) fn of(id: SnapshotId, invalid: IdSet) -> View {
        View { id, invalid, read_only: false, global: false, open_highest: 0 }
    }
}

/// A record: the snapshot that wrote it, the write that made it (a per-thread counter: what a
/// derived state validates its dependencies against), and the object's data as of that write.
pub(crate) struct Record<R> {
    pub(crate) id: SnapshotId,
    pub(crate) version: u32,
    pub(crate) data: R,
}

/// A state object's records, in no order: the head inline, others in `rest`. While no snapshot is
/// open `rest` is empty and the head is what a read sees, nearly always.
pub(crate) struct Records<R> {
    pub(crate) head: Record<R>,
    pub(crate) rest: Vec<Record<R>>,
    /// The snapshot the object was made in. Compose gives an object made outside the global
    /// snapshot two records with the same value, one `PREEXISTING` and one of that snapshot; here
    /// the first stands for both (a value needs no `Clone` to be born): `effective_id` tells which
    /// a view sees.
    pub(crate) born: SnapshotId,
}

/// Where a write goes (Compose's `overwritableRecord` and `writableRecord`).
pub(crate) enum WriteSlot {
    /// A record the write may overwrite, at this index.
    Own(usize),
    /// A new record, the readable one at this index staying.
    New(usize),
}

impl<R> Records<R> {
    /// One record holding `data`, written in snapshot `id` as write `version`; `born` tells of the
    /// snapshot it's made in.
    pub(crate) fn new(data: R, id: SnapshotId, born: SnapshotId, version: u32) -> Records<R> {
        Records { head: Record { id, version, data }, rest: Vec::new(), born }
    }

    /// The record a read sees, with no snapshot open.
    #[inline]
    pub(crate) fn readable(&self) -> &R {
        &self.head.data
    }

    /// Makes the head the record of write `version` in snapshot `id`, once it is written:
    /// returns whether that's a new record for the snapshot (the head was written in an
    /// earlier one), as Compose's `overwritableRecord` records the state as modified then.
    /// While only the global snapshot exists nothing older can be read, so the head is
    /// overwritten in place (D752).
    #[inline]
    pub(crate) fn wrote_in(&mut self, id: SnapshotId, version: u32) -> bool {
        self.head.version = version;
        let new_record = self.head.id != id;
        self.head.id = id;
        new_record
    }

    pub(crate) fn len(&self) -> usize {
        1 + self.rest.len()
    }

    /// The record at `index`: the head at 0.
    #[inline]
    pub(crate) fn at(&self, index: usize) -> &Record<R> {
        if index == 0 { &self.head } else { &self.rest[index - 1] }
    }

    /// The record at `index`, to write.
    #[inline]
    pub(crate) fn at_mut(&mut self, index: usize) -> &mut Record<R> {
        if index == 0 { &mut self.head } else { &mut self.rest[index - 1] }
    }

    /// The index of the record `view` reads, as Compose's `readable`: the highest valid id, the
    /// first record ranking as the snapshot it was born in if `view` can see that one (Compose's
    /// record of that id); a later record of that id is written over it.
    pub(crate) fn index_in(&self, view: &View) -> Option<usize> {
        let mut best = None;
        let mut best_id = INVALID;
        for index in 0..self.len() {
            let id = self.at(index).id;
            if !view.valid(id) {
                continue;
            }
            let rank = self.effective_id(index, view);
            if rank > best_id || (rank == best_id && id != PREEXISTING) {
                best = Some(index);
                best_id = rank;
            }
        }
        best
    }

    /// The record `view` reads. Panics as Compose's `readError` if there's none.
    #[inline]
    pub(crate) fn readable_in(&self, view: &View) -> &Record<R> {
        match self.index_in(view) {
            Some(index) => self.at(index),
            None => read_error(),
        }
    }

    /// The version of the record `view` reads; with no view, of the head.
    #[inline]
    pub(crate) fn version_in(&self, view: Option<&View>) -> Option<u32> {
        match view {
            None => Some(self.head.version),
            Some(view) => Some(self.at(self.index_in(view)?).version),
        }
    }

    /// The id of the record at `index`, as `view` sees it: a state's first record is the
    /// `PREEXISTING` one, or the record of the snapshot it was born in if `view` can see that one.
    pub(crate) fn effective_id(&self, index: usize, view: &View) -> SnapshotId {
        let id = self.at(index).id;
        if id == PREEXISTING && view.valid(self.born) { self.born } else { id }
    }

    /// Where a write in `view` goes, and whether it records the state as modified there (the
    /// record it reads wasn't written in the snapshot, as Compose's two functions decide).
    pub(crate) fn write_slot(&self, view: &View) -> (WriteSlot, bool) {
        let Some(index) = self.index_in(view) else { read_error() };
        let id = self.at(index).id;
        if id == view.id {
            return (WriteSlot::Own(index), false);
        }
        if id == PREEXISTING {
            // Written in the snapshot it was born in: the record is its own, as Compose's.
            let born_here = self.effective_id(index, view) == view.id;
            return (WriteSlot::New(index), !born_here);
        }
        // Nothing but later global snapshots can read it (D752).
        if view.global && id > view.open_highest {
            return (WriteSlot::Own(index), true);
        }
        (WriteSlot::New(index), true)
    }

    /// Adds a record: its index.
    pub(crate) fn push(&mut self, id: SnapshotId, version: u32, data: R) -> usize {
        self.rest.push(Record { id, version, data });
        self.rest.len()
    }

    /// Takes out the record at `index`; `rest` must not be empty. Its value goes to the caller,
    /// which drops it outside the runtime's borrow.
    pub(crate) fn remove(&mut self, index: usize) -> Record<R> {
        if index == 0 {
            let replacement = self.rest.pop().expect("another record");
            std::mem::replace(&mut self.head, replacement)
        } else {
            self.rest.swap_remove(index - 1)
        }
    }

    /// Drops the records of an abandoned snapshot, as Compose's `abandon` makes them invalid: its
    /// id, and those it had before.
    pub(crate) fn abandon(
        &mut self,
        id: SnapshotId,
        previous: &IdSet,
        garbage: &mut Vec<Box<dyn Any>>,
    ) where
        R: 'static,
    {
        let mut index = self.len();
        while index > 0 {
            index -= 1;
            let record_id = self.at(index).id;
            if record_id == id || previous.get(record_id) {
                if self.rest.is_empty() {
                    // The state has no other record: nothing can read it.
                    self.head.id = INVALID;
                } else {
                    garbage.push(Box::new(self.remove(index)));
                }
            }
        }
    }

    /// Drops the records no open snapshot can read, as Compose's `overwriteUnusedRecordsLocked`
    /// with `reuse_limit`: of those below it only the newest stays. Returns whether more than one
    /// record remains. With every record below the limit (no snapshot open), the head is then
    /// what any read sees.
    pub(crate) fn reclaim(
        &mut self,
        reuse_limit: SnapshotId,
        garbage: &mut Vec<Box<dyn Any>>,
    ) -> bool
    where
        R: 'static,
    {
        if self.rest.is_empty() {
            return false;
        }
        // The first record stands for two of Compose's (`born`): while the one of the snapshot it
        // was born in can be read, it stays.
        let rank = |id: SnapshotId| if id == PREEXISTING { self.born } else { id };
        let mut newest = INVALID;
        let mut newest_rank = INVALID;
        for index in 0..self.len() {
            let id = self.at(index).id;
            let rank = rank(id);
            if rank < reuse_limit
                && (rank > newest_rank || (rank == newest_rank && id != PREEXISTING))
            {
                newest = id;
                newest_rank = rank;
            }
        }
        let born = self.born;
        let mut index = self.len();
        while index > 0 {
            index -= 1;
            let id = self.at(index).id;
            let kept_for_birth = id == PREEXISTING && born >= reuse_limit;
            if id != newest && id < reuse_limit && !kept_for_birth {
                garbage.push(Box::new(self.remove(index)));
            }
        }
        !self.rest.is_empty()
    }
}

/// As Compose's `readError`.
#[cold]
pub(crate) fn read_error() -> ! {
    panic!(
        "Reading a state that was created after the snapshot was taken or in a snapshot that has \
         not yet been applied"
    )
}

/// As Compose's `reportReadonlySnapshotWrite`.
#[cold]
pub(crate) fn report_read_only_write() -> ! {
    panic!("Cannot modify a state object in a read-only snapshot")
}

/// What a state object's merge decides at an apply, as `StateObject.mergeRecords`'s result.
pub(crate) enum Merged {
    /// No conflict, or the snapshot's record stands.
    Nothing,
    /// The state as it is now stands: the state leaves the snapshot's modified list.
    Current,
    /// A new record, the merge's: goes in under a new id.
    New(Box<dyn Any>),
    /// A conflict: the apply fails.
    Fail,
}

/// The views of an apply, as `innerApplyLocked`'s `readable` calls.
pub(crate) struct ApplyViews {
    /// What the snapshot started from: not its own writes.
    pub(crate) start: View,
    /// The state as it is now where the snapshot applies to.
    pub(crate) current: View,
    /// The snapshot's own view: its writes.
    pub(crate) applied: View,
}

/// A state object's cell in the thread's arena, as Compose's `StateObject`, erased: what the
/// runtime needs of it without knowing its record type. A `dyn Any` too (upcast), so a typed
/// handle reaches its records.
pub(crate) trait StateObjectCell: Any {
    /// The number of records the object holds.
    fn record_count(&self) -> usize;
    /// The write that just ended was write `version` in snapshot `id`: whether it made a new
    /// record there. Runs in the runtime's borrow, once the write's user code is over.
    fn wrote_in(&self, id: SnapshotId, version: u32) -> bool;
    /// The version of the record a read through `view` would see, a derived state brought up to
    /// date first: `None` if the records are borrowed for writing, or `view` sees none. May run a
    /// calculation: outside the runtime's borrow.
    fn current_version(&self, view: Option<&View>) -> Option<u32>;
    /// Whether a derived state was found valid at `mark` (the current snapshot and the write
    /// counter), so it still is; a value state needs no validating.
    #[inline]
    fn valid_at(&self, _mark: (SnapshotId, u32)) -> bool {
        true
    }
    /// Adds what this object read, a derived state's dependencies, each once.
    fn collect_dependencies(&self, _into: &mut Vec<crate::state::StateId>) {}
    /// Decides this state's part of an apply, as `innerApplyLocked`'s loop and `mergeRecords`. User
    /// code (the policy): outside the runtime's borrow.
    fn merge_at_apply(&self, _views: &ApplyViews) -> Merged {
        Merged::Nothing
    }
    /// The apply stands, and for this state the current value did (`Merged::Current`): the
    /// snapshot's records have no more say. With `alone` (no nested snapshot of it is active) they
    /// go, as `abandon`; otherwise they stay for what reads them, with the current version.
    fn current_stands(
        &self,
        _views: &ApplyViews,
        _alone: bool,
        _previous: &IdSet,
        _garbage: &mut Vec<Box<dyn Any>>,
    ) {
    }
    /// Adds a record `merge_at_apply` made, under the apply's new id.
    fn prepend_merged(&self, _record: Box<dyn Any>, _id: SnapshotId, _version: u32) {}
    /// Drops the records of an abandoned snapshot.
    fn abandon(&self, id: SnapshotId, previous: &IdSet, garbage: &mut Vec<Box<dyn Any>>);
    /// Drops the records no open snapshot can read: whether more than one remains (a record
    /// borrowed now counts as more: it's tried again).
    fn reclaim(&self, reuse_limit: SnapshotId, garbage: &mut Vec<Box<dyn Any>>) -> bool;
    /// An `edit`'s closure sent, advancing the global snapshot: its record goes on under the new
    /// id.
    fn relabel(&self, _from: SnapshotId, _to: SnapshotId) {}
    /// A write of the head ended with a snapshot opened meanwhile: only its version changes, the
    /// record staying as readable as it was.
    fn touch(&self, _version: u32) {}
}
