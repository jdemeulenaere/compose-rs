//! A state object's records, as Compose's `StateRecord` chains: what each snapshot sees of the
//! object. A cell keeps one record inline, the head; more come only while snapshots other than
//! the global one are open and need an older value (docs/parity/design/snapshots.md §3.3).

use std::any::Any;
use std::cell::Cell;

/// A snapshot's id, as Compose's `SnapshotId`: records carry the id of the snapshot that wrote
/// them.
pub(crate) type SnapshotId = u64;

/// The id of the first global snapshot (Compose's ids start there: 0 is `INVALID`, 1
/// `PreexistingSnapshotId`).
pub(crate) const FIRST_GLOBAL: SnapshotId = 2;

/// A record: the snapshot that wrote it, the write that made it (a per-thread counter: what a
/// derived state validates its dependencies against), and the object's data as of that write.
pub(crate) struct Record<R> {
    pub(crate) id: SnapshotId,
    pub(crate) version: u32,
    pub(crate) data: R,
}

/// A state object's records, newest first: the head inline, older ones in `rest` (empty while
/// only the global snapshot exists, nearly always).
pub(crate) struct Records<R> {
    pub(crate) head: Record<R>,
    pub(crate) rest: Vec<Record<R>>,
}

impl<R> Records<R> {
    /// One record holding `data`, written now.
    pub(crate) fn new(data: R) -> Records<R> {
        let id = super::global::global_id();
        Records { head: Record { id, version: next_version(), data }, rest: Vec::new() }
    }

    /// The record a read sees.
    #[inline]
    pub(crate) fn readable(&self) -> &R {
        &self.head.data
    }

    /// Makes the head the record a write in snapshot `id` overwrites, with a new version:
    /// returns whether that's a new record for the snapshot (the head was written in an
    /// earlier one), as Compose's `overwritableRecord` records the state as modified then.
    /// While only the global snapshot exists nothing older can be read, so the head is
    /// overwritten in place (D752).
    #[inline]
    pub(crate) fn write_in(&mut self, id: SnapshotId) -> bool {
        self.head.version = next_version();
        let new_record = self.head.id != id;
        self.head.id = id;
        new_record
    }

    /// The version of the record a read sees.
    #[inline]
    pub(crate) fn version(&self) -> u32 {
        self.head.version
    }

    pub(crate) fn len(&self) -> usize {
        1 + self.rest.len()
    }
}

thread_local! {
    /// The last write's version on this thread.
    static VERSION: Cell<u32> = const { Cell::new(0) };
}

/// A new write's version: each write's is distinct (they wrap after 2³² writes, far beyond
/// what a derived state compares across).
#[inline]
pub(crate) fn next_version() -> u32 {
    VERSION.with(|version| {
        let next = version.get().wrapping_add(1);
        version.set(next);
        next
    })
}

/// A state object's cell in the thread's arena, as Compose's `StateObject`, erased: what the
/// runtime needs of it without knowing its record type. A `dyn Any` too (upcast), so a typed
/// handle reaches its records.
pub(crate) trait StateObjectCell: Any {
    /// The number of records the object holds.
    fn record_count(&self) -> usize;
}
