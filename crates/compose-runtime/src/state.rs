//! Observable state: `Copy` handles into a per-thread arena of cells.
//!
//! A cell is an `Rc` in the arena: a state object keeping its value in records
//! ([`crate::snapshots`]), or a state holder's object. Reading or writing clones that `Rc` out
//! of the arena and lets the runtime's borrow go before touching the value, so `Clone`,
//! `PartialEq` and `Drop` of the value (user code) run outside the runtime, and a cell freed
//! during a `with` stays alive until the `with` ends.

use std::any::Any;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use std::rc::Rc;

use crate::runtime::{
    PassFrames, Runtime, invalidate_scope_parts, request_frames, try_with_runtime, with_runtime,
};
use crate::scope::{ScopeId, Scopes};
use crate::snapshots::{
    ApplyViews, Merged, Records, SnapshotId, StateObjectCell, StateObjectId, View, WriteSlot,
};

/// A generational handle to a cell in the thread's cell arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct StateId {
    index: u32,
    generation: u32,
}

/// What the arena holds for a cell.
#[derive(Clone)]
pub(crate) enum CellValue {
    /// A state object: a value in records, or a derived state's result before it's computed.
    State(Rc<dyn StateObjectCell>),
    /// A state holder's object (`Owned`): no records, observed by nothing.
    Object(Rc<dyn Any>),
}

impl CellValue {
    /// The state object, for a state handle.
    #[inline]
    fn state(&self) -> Rc<dyn StateObjectCell> {
        match self {
            CellValue::State(cell) => cell.clone(),
            CellValue::Object(_) => panic!("compose-runtime: state handle of an object"),
        }
    }
}

#[derive(Clone, Copy)]
struct Reader {
    scope: ScopeId,
    epoch: u32,
}

impl CellEntry {
    /// The state object, for a state handle.
    #[inline]
    fn state(&self) -> StateRc {
        self.cell.as_ref().expect("a live cell").state()
    }
}

struct CellEntry {
    /// Odd while the cell is live; bumped on allocation and on free.
    generation: u32,
    cell: Option<CellValue>,
    readers: Vec<Reader>,
    /// Reads observed outside composition (layout, drawing).
    observed: Vec<crate::observe::ObservedRead>,
    /// Written outside composition since the last send: its readers are invalidated then.
    pending: bool,
    /// In the global snapshot's modified list.
    in_global_modified: bool,
    /// In the list of cells holding more than one record.
    in_extra: bool,
    /// A derived state, queued for the next send to validate.
    in_derived_queue: bool,
    /// The walk from what changed through the derived states reading it that last reached this
    /// derived state: each is visited once.
    walked: u32,
    created_at: &'static Location<'static>,
    /// The generation and creation site of the cell last freed here, so a stale handle still
    /// names its creation site after the slot is reused (rule R3).
    freed: (u32, &'static Location<'static>),
}

pub(crate) struct Cells {
    entries: Vec<CellEntry>,
    free: Vec<u32>,
    /// Cells whose owner was dropped, freed at the end of the next pass (rule R2).
    retired: Vec<StateId>,
    /// Cells retired before the pass in progress, freed at its end.
    freeing: Vec<StateId>,
    /// Cells retired while a pass applies its end-of-pass work: they join `retired` when it
    /// ends, so a pass started meanwhile doesn't free them early.
    held_by_passes: Vec<StateId>,
    /// Cells retired while compositions are being disposed: freed when the disposals end.
    held_by_disposals: Vec<StateId>,
    /// The generation of new entries (odd): after a reset, above every earlier one.
    base: u32,
}

impl Cells {
    pub const fn new() -> Cells {
        Cells {
            entries: Vec::new(),
            free: Vec::new(),
            retired: Vec::new(),
            freeing: Vec::new(),
            held_by_passes: Vec::new(),
            held_by_disposals: Vec::new(),
            base: 1,
        }
    }

    fn alloc(&mut self, cell: CellValue, created_at: &'static Location<'static>) -> StateId {
        if let Some(index) = self.free.pop() {
            let entry = &mut self.entries[index as usize];
            entry.generation = entry.generation.wrapping_add(1);
            entry.cell = Some(cell);
            entry.created_at = created_at;
            StateId { index, generation: entry.generation }
        } else {
            let index = u32::try_from(self.entries.len()).expect("too many state cells");
            self.entries.push(CellEntry {
                generation: self.base,
                cell: Some(cell),
                readers: Vec::new(),
                observed: Vec::new(),
                pending: false,
                in_global_modified: false,
                in_extra: false,
                in_derived_queue: false,
                walked: 0,
                created_at,
                freed: (0, created_at),
            });
            StateId { index, generation: self.base }
        }
    }

    fn entry(&mut self, id: StateId) -> Result<&mut CellEntry, Stale> {
        match self.entries.get_mut(id.index as usize) {
            Some(entry) if entry.generation == id.generation => Ok(entry),
            Some(entry) if entry.freed.0 == id.generation => {
                Err(Stale { created_at: Some(entry.freed.1) })
            }
            _ => Err(Stale { created_at: None }),
        }
    }

    fn free(&mut self, id: StateId) -> Option<CellValue> {
        let entry = self.entries.get_mut(id.index as usize)?;
        if entry.generation != id.generation {
            return None;
        }
        entry.freed = (entry.generation, entry.created_at);
        entry.generation = entry.generation.wrapping_add(1);
        entry.readers.clear();
        entry.observed.clear();
        entry.pending = false;
        entry.in_global_modified = false;
        entry.in_extra = false;
        entry.in_derived_queue = false;
        self.free.push(id.index);
        entry.cell.take()
    }

    pub fn begin_pass(&mut self) {
        debug_assert!(self.freeing.is_empty());
        std::mem::swap(&mut self.retired, &mut self.freeing);
    }

    /// Frees the cells retired before the pass that just ended. Returns their values, to be
    /// dropped outside the borrow.
    /// Takes the cells retired before the pass that just ended: they are freed once its
    /// end-of-pass work has run (`free`), so cleanups still see them, and a pass started by
    /// that work doesn't see them.
    pub fn end_pass(&mut self) -> Vec<StateId> {
        std::mem::take(&mut self.freeing)
    }

    /// Frees cells taken by `end_pass`. Returns their values, to be dropped outside the borrow.
    pub fn free_all(&mut self, ids: Vec<StateId>) -> Vec<CellValue> {
        ids.into_iter().filter_map(|id| self.free(id)).collect()
    }

    /// After a panic in a pass: the cells it would have freed wait for the next one.
    pub fn abort_pass(&mut self) {
        let mut freeing = std::mem::take(&mut self.freeing);
        self.retired.append(&mut freeing);
        self.freeing = freeing;
    }

    /// A pass's end-of-pass work ended: the cells it retired wait for the next pass.
    pub fn release_held_by_passes(&mut self) {
        let mut held = std::mem::take(&mut self.held_by_passes);
        self.retired.append(&mut held);
        self.held_by_passes = held;
    }

    /// The disposals ended: the cells they retired wait for the next pass, as any retired cell
    /// (they may not belong to the disposed compositions). With `all` (no composition is left to
    /// run that pass), frees every retired cell.
    pub fn free_held_by_disposals(&mut self, all: bool) -> Vec<CellValue> {
        let mut ids = std::mem::take(&mut self.held_by_disposals);
        if !all {
            self.retired.append(&mut ids);
            return Vec::new();
        }
        {
            ids.append(&mut self.retired);
            ids.append(&mut self.freeing);
            ids.append(&mut self.held_by_passes);
        }
        ids.into_iter().filter_map(|id| self.free(id)).collect()
    }

    /// Cells taken by `end_pass` whose pass couldn't free them (a panic): they wait.
    pub fn retire_all(&mut self, mut ids: Vec<StateId>) {
        self.retired.append(&mut ids);
    }

    /// After a panic in a disposal: its cells wait with the others.
    pub fn abort_disposal(&mut self) {
        let mut held = std::mem::take(&mut self.held_by_disposals);
        self.retired.append(&mut held);
        self.held_by_disposals = held;
    }

    pub fn set_base(&mut self, base: u32) {
        self.base = base;
    }

    pub fn next_base(&self) -> u32 {
        crate::runtime::next_base(self.base, self.entries.iter().map(|entry| entry.generation))
    }

    pub fn live_count(&self) -> usize {
        self.entries.len() - self.free.len()
    }

    /// Records a write in the global snapshot, as write `version` of snapshot `global`: a new
    /// record there is a change, in `modified` once; `pending`, made outside composition: the
    /// next send invalidates its readers.
    pub(crate) fn record_write(
        &mut self,
        id: StateId,
        global: crate::snapshots::SnapshotId,
        version: u32,
        modified: &mut Vec<StateId>,
    ) {
        // An `edit` may have freed its own state.
        let Ok(entry) = self.entry(id) else { return };
        let new_record = match &entry.cell {
            Some(CellValue::State(cell)) => cell.wrote_in(global, version),
            _ => false,
        };
        if new_record {
            if !entry.in_global_modified {
                entry.in_global_modified = true;
                modified.push(id);
            }
            entry.pending = true;
        }
    }

    /// Records a state as changed in the global snapshot, its record written already.
    pub(crate) fn record_modified(&mut self, id: StateId, modified: &mut Vec<StateId>) {
        let Ok(entry) = self.entry(id) else { return };
        if !entry.in_global_modified {
            entry.in_global_modified = true;
            modified.push(id);
        }
        entry.pending = true;
    }

    /// Whether the state, which now has several records, wasn't listed as such.
    pub(crate) fn mark_extra(&mut self, id: StateId) -> bool {
        match self.entry(id) {
            Ok(entry) => !std::mem::replace(&mut entry.in_extra, true),
            Err(_) => false,
        }
    }

    /// A send during a pass took this state off the global snapshot's list: its readers hear at the
    /// pass's apply.
    pub(crate) fn sent_for_later(&mut self, id: StateId) {
        if let Ok(entry) = self.entry(id) {
            entry.in_global_modified = false;
            entry.pending = false;
        }
    }

    /// See [`StateObjectCell::touch`].
    pub(crate) fn touch(&mut self, id: StateId, version: u32) {
        if let Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) = self.entry(id) {
            cell.touch(version);
        }
    }

    /// See [`StateObjectCell::relabel`].
    pub(crate) fn relabel(&mut self, id: StateId, from: SnapshotId, to: SnapshotId) {
        if let Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) = self.entry(id) {
            cell.relabel(from, to);
        }
    }

    /// See [`StateObjectCell::abandon`].
    pub(crate) fn abandon(
        &mut self,
        id: StateId,
        snapshot: SnapshotId,
        previous: &crate::snapshots::IdSet,
        garbage: &mut Vec<Box<dyn Any>>,
    ) {
        if let Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) = self.entry(id) {
            cell.abandon(snapshot, previous, garbage);
        }
    }

    /// The state no longer has several records, as far as anyone knows.
    pub(crate) fn unmark_extra(&mut self, id: StateId) {
        if let Ok(entry) = self.entry(id) {
            entry.in_extra = false;
        }
    }
}

struct Stale {
    created_at: Option<&'static Location<'static>>,
}

#[cold]
#[track_caller]
fn stale(stale: Stale, used_at: &'static Location<'static>) -> ! {
    match stale.created_at {
        Some(created_at) => panic!(
            "compose-runtime: state created at {created_at} was used at {used_at} after it was \
             freed; keep an Rc of its owner, or use try_get/try_set (rule R3)"
        ),
        None => panic!(
            "compose-runtime: state used at {used_at} was freed; keep an Rc of its owner, or use \
             try_get/try_set (rule R3)"
        ),
    }
}

/// How many of a cell's latest readers a read searches for the reading scope.
const RECENT_READERS: usize = 8;

/// Records a read by the scope composing now, if any.
///
/// A cell can be read by thousands of scopes (a composition local every layout node reads):
/// searching all its readers at each read would make composing a big screen quadratic. A read
/// searches the latest few only, where a scope's repeated reads usually are; a scope found
/// further back, or not at all, is recorded again. A copy is harmless: a write invalidates
/// every current one, and invalidating a scope twice does nothing more. Copies and stale
/// readers are dropped when the readers fill their room (`compact_readers`).
fn register_read(entry: &mut CellEntry, scopes: &mut Scopes, scope: ScopeId) {
    let Some(epoch) = scopes.get(scope).map(|scope| scope.epoch) else {
        return;
    };
    let readers = &mut entry.readers;
    if let Some(reader) =
        readers.iter_mut().rev().take(RECENT_READERS).find(|reader| reader.scope == scope)
    {
        reader.epoch = epoch;
        return;
    }
    if readers.len() == readers.capacity() {
        compact_readers(readers, scopes);
    }
    readers.push(Reader { scope, epoch });
}

/// Drops the readers that aren't current (their scope left, or ran again without reading the
/// cell) and the copies (the latest of each scope stays, keeping the order), then makes sure
/// that at least half the room is free.
///
/// So the room holds less than four records per scope reading the cell at once (four to start
/// with): it doubles only when, after a compaction, more than half of it holds distinct current
/// readers, and it never shrinks. That holds whatever the scopes do: read in a loop between
/// other readers, run every frame, come and go, or run without their epoch moving. And a
/// compaction, linear in the readers, comes only after at least half the room's worth of new
/// records (reads found among the latest readers record nothing): a constant cost per read.
fn compact_readers(readers: &mut Vec<Reader>, scopes: &mut Scopes) {
    scopes.begin_compaction();
    // From the latest, so a scope keeps its latest record, within reach of the next search.
    readers.reverse();
    readers.retain(|reader| scopes.keep_reader(reader.scope, reader.epoch));
    readers.reverse();
    if readers.len() > readers.capacity() / 2 {
        readers.reserve(readers.len());
    }
}

/// Records a read observed outside composition, under the observation in progress.
fn register_observed_read(
    entry: &mut CellEntry,
    observers: &crate::observe::Observers,
    observing: crate::observe::Observing,
) -> bool {
    if let Some(read) = entry
        .observed
        .iter_mut()
        .find(|read| read.observer == observing.observer && read.key == observing.key)
    {
        let first = read.epoch != observing.epoch;
        read.epoch = observing.epoch;
        return first;
    }
    if entry.observed.len() == entry.observed.capacity() {
        entry.observed.retain(|read| observers.is_current(read));
    }
    let read = crate::observe::ObservedRead {
        observer: observing.observer,
        key: observing.key,
        epoch: observing.epoch,
    };
    entry.observed.push(read);
    true
}

/// A state object's cell, out of the arena.
type StateRc = Rc<dyn StateObjectCell>;

/// A cell, and whether a write must look at snapshots.
type CellToWrite = (StateRc, bool);

/// `write_cell` or `try_write_cell`: the cell.
type CellLookup = fn(StateId, &'static Location<'static>) -> Result<CellToWrite, Stale>;

/// A read that must look further: read observers hear it (before it selects a record, as
/// Compose's `readable`), and the view to read through.
#[cold]
fn slow_read(id: StateId) -> Option<View> {
    crate::snapshots::notify_read_if_observed(id);
    crate::snapshots::view()
}

/// A state's cell, if it's alive.
pub(crate) fn state_object(id: StateId) -> Option<StateRc> {
    try_with_runtime(|runtime| match runtime.cells.entry(id) {
        Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) => Some(cell.clone()),
        _ => None,
    })
    .flatten()
}

/// A state object's cell, not registering a read. Panics if it's freed (rule R3).
#[track_caller]
pub(crate) fn object_cell(id: StateId, at: &'static Location<'static>) -> StateRc {
    with_runtime(|runtime| runtime.cells.entry(id).map(|entry| entry.state()))
        .unwrap_or_else(|error| stale(error, at))
}

/// [`object_cell`], and whether a read must look at snapshots.
#[track_caller]
#[inline]
fn object_cell_to_read(id: StateId, at: &'static Location<'static>) -> (StateRc, bool) {
    with_runtime(|runtime| {
        runtime.cells.entry(id).map(|entry| (entry.state(), runtime.snapshots.isolated()))
    })
    .unwrap_or_else(|error| stale(error, at))
}

/// If derived state `id` was found valid with no write since: registers the read, and gives its
/// cell and whether read observers heard it.
#[inline]
pub(crate) fn read_if_valid(
    id: StateId,
    at: &'static Location<'static>,
) -> Option<(StateRc, bool)> {
    let read = with_runtime(|runtime| {
        // In a snapshot, a read goes through the record it sees.
        if !runtime.snapshots.plain() {
            return None;
        }
        let mark = (runtime.snapshots.current_id(), runtime.snapshots.version());
        match runtime.cells.entry(id) {
            Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) if cell.valid_at(mark) => {
                Some(read_cell_in(runtime, id, at))
            }
            _ => None,
        }
    })?;
    match read {
        Read::Cell(cell) => Some((cell, false)),
        Read::Pass(cell) => Some((cell, false)),
        Read::Slow(cell) => {
            crate::snapshots::notify_read(id);
            Some((cell, true))
        }
        Read::Stale(_) => None,
    }
}

/// Registers a read of derived state `id`, brought up to date. Read observers heard it already.
pub(crate) fn register_read_of(id: StateId, at: &'static Location<'static>) {
    if let Read::Stale(error) = with_runtime(|runtime| read_cell_in(runtime, id, at)) {
        stale(error, at)
    }
}

/// A state object's current version: a derived state is brought up to date. `None` if it's
/// freed, or its records are being written.
pub(crate) fn current_version(id: StateId) -> Option<u32> {
    let (cell, plain) = try_with_runtime(|runtime| match runtime.cells.entry(id) {
        Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) => {
            Some((cell.clone(), runtime.snapshots.plain()))
        }
        _ => None,
    })??;
    version_of(&cell, plain)
}

/// `cell`'s current version, through the view unless reads are plain.
#[inline]
fn version_of(cell: &StateRc, plain: bool) -> Option<u32> {
    if plain {
        cell.current_version(None)
    } else {
        let view = crate::snapshots::view();
        cell.current_version(view.as_ref())
    }
}

/// Whether dependency `id` still has version `seen`: a freed one can't have changed.
pub(crate) fn dependency_unchanged(id: StateId, seen: u32) -> bool {
    let cell = try_with_runtime(|runtime| match runtime.cells.entry(id) {
        Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) => {
            Some((cell.clone(), runtime.snapshots.plain()))
        }
        _ => None,
    })
    .flatten();
    match cell {
        // Being written (an `edit` reading this derived state): not as it was seen.
        Some((cell, plain)) => version_of(&cell, plain) == Some(seen),
        None => true,
    }
}

/// Adds what state object `id` read, if it's a derived state, each once.
pub(crate) fn collect_dependencies(id: StateId, into: &mut Vec<StateId>) {
    let cell = try_with_runtime(|runtime| match runtime.cells.entry(id) {
        Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) => Some(cell.clone()),
        _ => None,
    })
    .flatten();
    if let Some(cell) = cell {
        cell.collect_dependencies(into);
    }
}

/// A derived state's cell, as its owner, as any state object's.
pub(crate) fn owned_object<T: 'static>(
    cell: StateRc,
    created_at: &'static Location<'static>,
) -> OwnedState<T> {
    let id = alloc_cell(CellValue::State(cell), created_at);
    OwnedState { handle: MutableState { id, _marker: PhantomData } }
}

/// What a read found.
enum Read {
    /// The cell.
    Cell(StateRc),
    /// The cell, and read observers ([`Snapshot::observe`]) must hear the read, or isolation is in
    /// use.
    ///
    /// [`Snapshot::observe`]: crate::snapshots::Snapshot::observe
    Slow(StateRc),
    /// The cell, read in a pass's snapshot: its only record is the one to read.
    Pass(StateRc),
    Stale(Stale),
}

fn read_cell_in(runtime: &mut Runtime, id: StateId, _at: &'static Location<'static>) -> Read {
    runtime.state_reads = runtime.state_reads.wrapping_add(1);
    let scope = runtime.reading_scope();
    let Runtime { cells, scopes, observers, observing, snapshots, .. } = runtime;
    let entry = match cells.entry(id) {
        Ok(entry) => entry,
        Err(stale) => return Read::Stale(stale),
    };
    // `without_read_observation` stops what started before it from recording reads.
    match *observing {
        Some(observing) => {
            // An observation started in a calculation adds to it, as Compose merges observers.
            let calculation =
                if observing.exclusive { None } else { snapshots.observing_calculation() };
            if observing.exclusive || scope.is_none() || calculation.is_some() {
                if snapshots.observes_at(observing.depth)
                    && register_observed_read(entry, observers, observing)
                    && observing.exclusive
                {
                    // A derived state's calculation: what it read, this once, is a dependency.
                    snapshots.record_dependency(id);
                }
            } else if snapshots.composition_reads() {
                register_read(entry, scopes, scope)
            }
            if let Some(calculation) = calculation
                && snapshots.observes_at(calculation.depth)
                && register_observed_read(entry, observers, calculation)
            {
                snapshots.record_dependency(id);
            }
        }
        None if !scope.is_none() && snapshots.composition_reads() => {
            register_read(entry, scopes, scope)
        }
        None => {}
    }
    let cell = entry.state();
    match snapshots.read_kind() {
        crate::snapshots::ReadKind::Head => Read::Cell(cell),
        crate::snapshots::ReadKind::InPass => Read::Pass(cell),
        crate::snapshots::ReadKind::Look => Read::Slow(cell),
    }
}

/// Looks a cell up for writing.
#[inline]
fn write_cell(id: StateId, at: &'static Location<'static>) -> Result<CellToWrite, Stale> {
    with_runtime(|runtime| write_cell_in(runtime, id, at))
}

/// Like [`write_cell`], for `try_set`: a stale cell once the thread's runtime is gone.
fn try_write_cell(id: StateId, at: &'static Location<'static>) -> Result<CellToWrite, Stale> {
    try_with_runtime(|runtime| write_cell_in(runtime, id, at))
        .unwrap_or(Err(Stale { created_at: None }))
}

/// A write during composition is allowed, as Compose's (D703): it invalidates the readers, the
/// ones the pass will still reach in this pass.
#[inline]
fn write_cell_in(
    runtime: &mut Runtime,
    id: StateId,
    _at: &'static Location<'static>,
) -> Result<CellToWrite, Stale> {
    Ok((runtime.cells.entry(id)?.state(), runtime.snapshots.isolated()))
}

/// The keys under which observer `observer`'s current observations read `id`.
pub(crate) fn observed_keys(runtime: &mut Runtime, id: StateId, observer: u32) -> Vec<u64> {
    let Runtime { cells, observers, .. } = runtime;
    let Ok(entry) = cells.entry(id) else { return Vec::new() };
    entry
        .observed
        .iter()
        .filter(|read| read.observer == observer && observers.is_current(read))
        .map(|read| read.key)
        .collect()
}

/// What's telling of a state's change.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Told {
    /// A send of what the global snapshot wrote: its flags say whether it's due.
    Sent,
    /// A write in a pass's snapshot, as `recordWriteOf`: that composition's readers, at once. One
    /// the pass will still reach reads the new value in this pass: the apply skips that pair, as
    /// Compose's `observationsProcessed` (D793).
    InPass,
    /// A snapshot's apply, as `addPendingInvalidationsLocked`: everything that read it, but those
    /// pairs. The composition's are for the next pass, what they read being old.
    Applied,
}

/// Invalidates what read a cell in its latest run, pruning the others, as `told` says. Composables
/// and observations are told; for the derived states that read it, and those reading them, nothing
/// is computed here: the composables that read one are invalidated on the condition that its value
/// changed, which the pass reaching them checks (Compose's conditional invalidation), and one that
/// an observation read is queued, for the send to validate outside the borrow. One nobody reads
/// isn't computed.
pub(crate) fn invalidate_cell(runtime: &mut Runtime, id: StateId, told: Told) {
    let Runtime {
        cells,
        scopes,
        compositions,
        frame_requests,
        observers,
        active,
        frames,
        snapshots,
        ..
    } = runtime;
    let Ok(entry) = cells.entry(id) else { return };
    if told == Told::Sent {
        entry.in_global_modified = false;
        if !std::mem::take(&mut entry.pending) {
            return;
        }
    }
    let composing = active.filter(|_| told == Told::InPass).map(|active| active.composition.index);
    entry.readers.retain(|reader| {
        if !scopes.is_current(reader.scope, reader.epoch) {
            return false;
        }
        if let Some(composition) = composing {
            if !scopes.get(reader.scope).is_some_and(|scope| scope.composition == composition) {
                return true;
            }
            if let Some(pass) = PassFrames::of(*active, frames)
                && crate::runtime::scope_is_ahead(scopes, compositions, &pass, reader.scope)
            {
                snapshots.note_read_again(id, reader.scope);
            }
        } else if told == Told::Applied && snapshots.take_read_again(id, reader.scope) {
            return true;
        }
        invalidate_scope_parts(
            scopes,
            compositions,
            frame_requests,
            PassFrames::of(*active, frames),
            reader.scope,
            None,
        );
        true
    });
    let mut derived: Vec<StateId> = Vec::new();
    if !entry.observed.is_empty() {
        entry.observed.retain(|read| {
            if !observers.is_current(read) {
                return false;
            }
            match observers.dirty_cell(read) {
                Some(cell) => derived.push(cell),
                // Observations hear at the apply, as `SnapshotStateObserver`.
                None if composing.is_some() => {}
                None => {
                    if let Some(notify) = observers.invalidate(read) {
                        frame_requests.push(notify);
                    }
                }
            }
            true
        });
    }
    let walk = snapshots.walk();
    let mut next = 0;
    while next < derived.len() {
        let cell = derived[next];
        next += 1;
        let Ok(entry) = cells.entry(cell) else { continue };
        if std::mem::replace(&mut entry.walked, walk) == walk {
            continue;
        }
        entry.readers.retain(|reader| {
            if !scopes.is_current(reader.scope, reader.epoch) {
                return false;
            }
            if let Some(composition) = composing
                && !scopes.get(reader.scope).is_some_and(|scope| scope.composition == composition)
            {
                return true;
            }
            invalidate_scope_parts(
                scopes,
                compositions,
                frame_requests,
                PassFrames::of(*active, frames),
                reader.scope,
                Some(cell),
            );
            true
        });
        let mut observed = false;
        for read in &entry.observed {
            if !observers.is_current(read) {
                continue;
            }
            match observers.dirty_cell(read) {
                Some(reading) => derived.push(reading),
                None => observed = true,
            }
        }
        if observed && composing.is_none() && !entry.in_derived_queue {
            entry.in_derived_queue = true;
            snapshots.queue_derived(cell);
        }
    }
}

/// A derived state's result changed: what read the old one is invalidated, outright. The derived
/// states among them validate themselves, and the send naming what changed reaches their readers.
pub(crate) fn invalidate_result_readers(id: StateId) {
    let requested = with_runtime(|runtime| {
        let Runtime {
            cells, scopes, compositions, frame_requests, observers, active, frames, ..
        } = runtime;
        if let Ok(entry) = cells.entry(id) {
            entry.readers.retain(|reader| {
                if !scopes.is_current(reader.scope, reader.epoch) {
                    return false;
                }
                invalidate_scope_parts(
                    scopes,
                    compositions,
                    frame_requests,
                    PassFrames::of(*active, frames),
                    reader.scope,
                    None,
                );
                true
            });
            entry.observed.retain(|read| {
                if !observers.is_current(read) {
                    return false;
                }
                if observers.dirty_cell(read).is_none()
                    && let Some(notify) = observers.invalidate(read)
                {
                    frame_requests.push(notify);
                }
                true
            });
        }
        crate::runtime::frames_requested(runtime)
    });
    if requested {
        request_frames();
    }
}

/// Clears the mark of a derived state a send took off its queue.
pub(crate) fn dequeue_derived(runtime: &mut Runtime, id: StateId) {
    if let Ok(entry) = runtime.cells.entry(id) {
        entry.in_derived_queue = false;
    }
}

/// A value state's record, as Compose's `StateStateRecord`: the value.
struct ValueRecord<T> {
    value: T,
}

/// One value state's cell, as Compose's `SnapshotMutableStateImpl`: its records behind a
/// run-time borrow flag that remembers who holds them, and its policy.
struct StateCell<T> {
    records: RefCell<Records<ValueRecord<T>>>,
    borrowed_at: Cell<Option<&'static Location<'static>>>,
    equivalent: fn(&T, &T) -> bool,
    /// What a conflict at an apply comes to, as `mergeRecords`.
    merge: MergeFn<T>,
    created_at: &'static Location<'static>,
}

/// `mergeRecords`: the value the snapshot started from, the current one and its own.
type MergeFn<T> = fn(&T, &T, &T) -> MergeResult<T>;

/// What a state's merge decides at an apply, as `StateObject.mergeRecords`'s result.
pub enum MergeResult<T> {
    /// The states conflict: the apply fails.
    Fail,
    /// The current value stands: the snapshot's write is dropped, and nobody is told of it.
    Current,
    /// The snapshot's value stands.
    Applied,
    /// A new value.
    New(T),
}

/// A write's outcome through a view: its record, and what's told of it.
struct WroteInView<T> {
    old: Option<T>,
    modified: bool,
    written_at: SnapshotId,
}

/// `with` when a read must look further: what read observers hear, then the record through the
/// view.
#[cold]
#[inline(never)]
fn with_in_view<T: 'static, R>(
    cell: StateRc,
    id: StateId,
    at: &'static Location<'static>,
    f: impl FnOnce(&T) -> R,
) -> R {
    let view = slow_read(id);
    let value = StateCell::<T>::of(&cell).read_in(view.as_ref(), at);
    f(&value)
}

impl<T: 'static> StateObjectCell for StateCell<T> {
    fn record_count(&self) -> usize {
        self.records.try_borrow().map_or(1, |records| records.len())
    }
    #[inline]
    fn wrote_in(&self, id: crate::snapshots::SnapshotId, version: u32) -> bool {
        // Nothing holds the records now: a write conflicting with a borrow panicked at its start.
        self.records.try_borrow_mut().map_or(true, |mut records| records.wrote_in(id, version))
    }
    #[inline]
    fn current_version(&self, view: Option<&View>) -> Option<u32> {
        let records = self.records.try_borrow().ok()?;
        if view.is_none() && !records.rest.is_empty() {
            let view = crate::snapshots::view();
            return records.version_in(view.as_ref());
        }
        records.version_in(view)
    }

    fn merge_at_apply(&self, views: &ApplyViews) -> Merged {
        let result = {
            let records = self.records.borrow();
            let Some(current) = records.index_in(&views.current) else { return Merged::Nothing };
            let Some(previous) = records.index_in(&views.start) else { return Merged::Nothing };
            // Nothing could have been read of it before the snapshot wrote it.
            if records.effective_id(previous, &views.start) == crate::snapshots::PREEXISTING
                || current == previous
            {
                return Merged::Nothing;
            }
            let Some(applied) = records.index_in(&views.applied) else {
                crate::snapshots::read_error()
            };
            let (previous, current, applied) =
                (records.at(previous), records.at(current), records.at(applied));
            if (self.equivalent)(&current.data.value, &applied.data.value) {
                MergeResult::Current
            } else {
                (self.merge)(&previous.data.value, &current.data.value, &applied.data.value)
            }
        };
        match result {
            MergeResult::Fail => Merged::Fail,
            MergeResult::Applied => Merged::Nothing,
            MergeResult::New(value) => Merged::New(Box::new(ValueRecord { value })),
            MergeResult::Current => Merged::Current,
        }
    }

    fn current_stands(
        &self,
        views: &ApplyViews,
        alone: bool,
        previous: &crate::snapshots::IdSet,
        garbage: &mut Vec<Box<dyn Any>>,
    ) {
        // Compose copies the current record under a new id, so it wins over the snapshot's own. A
        // value needn't be `Clone` here (D773): the snapshot's records go, or stay if a nested
        // snapshot may read them, equivalent by the policy. A `with` closure applying: they stay.
        let Ok(mut records) = self.records.try_borrow_mut() else { return };
        if alone {
            records.abandon(views.applied.id, previous, garbage);
            return;
        }
        let current = records.index_in(&views.current);
        let applied = records.index_in(&views.applied);
        if let (Some(current), Some(applied)) = (current, applied) {
            let version = records.at(current).version;
            records.at_mut(applied).version = version;
        }
    }

    fn prepend_merged(&self, record: Box<dyn Any>, id: SnapshotId, version: u32) {
        let record = record.downcast::<ValueRecord<T>>().expect("this state's record");
        self.records_mut(Location::caller()).push(id, version, *record);
    }

    fn abandon(
        &self,
        id: SnapshotId,
        previous: &crate::snapshots::IdSet,
        garbage: &mut Vec<Box<dyn Any>>,
    ) {
        match self.records.try_borrow_mut() {
            Ok(mut records) => records.abandon(id, previous, garbage),
            // No second panic while unwinding: what it wrote stays, the lesser evil.
            Err(_) if std::thread::panicking() => {}
            Err(_) => self.conflict(Location::caller()),
        }
    }

    fn reclaim(&self, reuse_limit: SnapshotId) -> bool {
        crate::snapshots::reclaim_cell(&self.records, reuse_limit)
    }

    fn touch(&self, version: u32) {
        if let Ok(mut records) = self.records.try_borrow_mut() {
            records.head.version = version;
        }
    }

    fn relabel(&self, from: SnapshotId, to: SnapshotId) {
        if let Ok(mut records) = self.records.try_borrow_mut() {
            let index = (0..records.len()).find(|&index| records.at(index).id == from);
            if let Some(index) = index {
                records.at_mut(index).id = to;
            }
        }
    }
}

impl<T: 'static> StateCell<T> {
    fn new(
        value: T,
        equivalent: fn(&T, &T) -> bool,
        merge: MergeFn<T>,
        created_at: &'static Location<'static>,
    ) -> Self {
        let (id, born, version) = crate::snapshots::birth();
        StateCell {
            records: RefCell::new(Records::new(ValueRecord { value }, id, born, version)),
            borrowed_at: Cell::new(None),
            equivalent,
            merge,
            created_at,
        }
    }

    #[inline]
    fn of(cell: &StateRc) -> &StateCell<T> {
        let any: &dyn Any = &**cell;
        any.downcast_ref::<StateCell<T>>().expect("compose-runtime: state handle of the wrong type")
    }

    /// The readable record's value in a pass's snapshot: the only record, unless it wrote the state.
    fn read_in_pass(&self, at: &'static Location<'static>) -> CellRef<'_, T> {
        if self.records.try_borrow().is_ok_and(|records| !records.rest.is_empty()) {
            return self.read_through_view(at);
        }
        self.read(at)
    }

    /// The readable record's value, no snapshot being open: its only record.
    #[inline]
    fn read(&self, at: &'static Location<'static>) -> CellRef<'_, T> {
        match self.records.try_borrow() {
            Ok(records) => {
                let outer = self.borrowed_at.get().is_none();
                if outer {
                    self.borrowed_at.set(Some(at));
                }
                CellRef {
                    value: Ref::map(records, |records| &records.readable().value),
                    cell: self,
                    outer,
                }
            }
            Err(_) => self.conflict(at),
        }
    }

    /// A plain read of a state with several records.
    #[cold]
    #[inline(never)]
    fn read_through_view(&self, at: &'static Location<'static>) -> CellRef<'_, T> {
        let view = crate::snapshots::view().expect("several records: isolation in use");
        self.read_in(Some(&view), at)
    }

    /// The readable record's value through `view`, if isolation is in use.
    fn read_in(&self, view: Option<&View>, at: &'static Location<'static>) -> CellRef<'_, T> {
        let Some(view) = view else { return self.read(at) };
        match self.records.try_borrow() {
            Ok(records) => {
                let outer = self.borrowed_at.get().is_none();
                if outer {
                    self.borrowed_at.set(Some(at));
                }
                CellRef {
                    value: Ref::map(records, |records| &records.readable_in(view).data.value),
                    cell: self,
                    outer,
                }
            }
            Err(_) => self.conflict(at),
        }
    }

    /// The head record, to overwrite (no snapshot is open).
    #[inline]
    fn write(&self, at: &'static Location<'static>) -> CellMut<'_, T> {
        self.write_at(0, at)
    }

    /// The record at `index`, to write.
    #[inline]
    fn write_at(&self, index: usize, at: &'static Location<'static>) -> CellMut<'_, T> {
        match self.records.try_borrow_mut() {
            Ok(records) => {
                self.borrowed_at.set(Some(at));
                let value = RefMut::map(records, |records| &mut records.at_mut(index).data.value);
                CellMut { value, cell: self }
            }
            Err(_) => self.conflict(at),
        }
    }

    /// The records, to write. Panics if a `with` or an `edit` holds them.
    fn records_mut(&self, at: &'static Location<'static>) -> RefMut<'_, Records<ValueRecord<T>>> {
        match self.records.try_borrow_mut() {
            Ok(records) => records,
            Err(_) => self.conflict(at),
        }
    }

    /// Overwrites what `view` writes with `value`, as Compose's `overwritable`.
    fn overwrite_in(
        &self,
        id: StateId,
        view: &View,
        value: T,
        at: &'static Location<'static>,
    ) -> WroteInView<T> {
        if view.read_only {
            crate::snapshots::report_read_only_write();
        }
        let version = with_runtime(|runtime| runtime.snapshots.next_version());
        let mut records = self.records_mut(at);
        let (slot, modified) = records.write_slot(view);
        let old = match slot {
            WriteSlot::Own(index) => {
                let record = records.at_mut(index);
                record.id = view.id;
                record.version = version;
                Some(std::mem::replace(&mut record.data.value, value))
            }
            WriteSlot::New(_) => {
                records.push(view.id, version, ValueRecord { value });
                crate::snapshots::note_extra_record(id);
                None
            }
        };
        WroteInView { old, modified, written_at: view.id }
    }

    /// The record `view` writes, a copy of what it reads if it must make one, as Compose's
    /// `writable`: its index, whether the state is modified, for an `edit`.
    fn writable_in(
        &self,
        id: StateId,
        view: &View,
        copy: impl FnOnce(&T) -> T,
        at: &'static Location<'static>,
    ) -> (usize, bool) {
        if view.read_only {
            crate::snapshots::report_read_only_write();
        }
        let version = with_runtime(|runtime| runtime.snapshots.next_version());
        let (slot, modified) = self.records_mut(at).write_slot(view);
        let index = match slot {
            WriteSlot::Own(index) => {
                let mut records = self.records_mut(at);
                let record = records.at_mut(index);
                record.id = view.id;
                record.version = version;
                index
            }
            WriteSlot::New(index) => {
                // User code: no write borrow.
                let value = copy(&self.records.borrow().at(index).data.value);
                let index = self.records_mut(at).push(view.id, version, ValueRecord { value });
                crate::snapshots::note_extra_record(id);
                index
            }
        };
        (index, modified)
    }

    #[cold]
    fn conflict(&self, at: &'static Location<'static>) -> ! {
        let holder = self.borrowed_at.get().map_or_else(|| "?".to_string(), |at| at.to_string());
        panic!(
            "compose-runtime: state created at {} is borrowed by `with` or `edit` at {holder}, \
             and was accessed again at {at}",
            self.created_at
        )
    }
}

struct CellRef<'a, T> {
    value: Ref<'a, T>,
    cell: &'a StateCell<T>,
    outer: bool,
}

impl<T> Deref for CellRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> Drop for CellRef<'_, T> {
    fn drop(&mut self) {
        if self.outer {
            self.cell.borrowed_at.set(None);
        }
    }
}

struct CellMut<'a, T> {
    value: RefMut<'a, T>,
    cell: &'a StateCell<T>,
}

impl<T> Deref for CellMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CellMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> Drop for CellMut<'_, T> {
    fn drop(&mut self) {
        self.cell.borrowed_at.set(None);
    }
}

/// Decides whether a write changes a state's value, as Compose's `SnapshotMutationPolicy`.
pub trait MutationPolicy<T> {
    /// Whether `a` and `b` are equivalent: writing `b` over `a` then invalidates nothing.
    fn equivalent(a: &T, b: &T) -> bool;

    /// What two snapshots' writes come to when the second applies, as `merge`: `previous` is what
    /// it started from, `current` what the other wrote, `applied` what it wrote. `None`, the
    /// default, fails the apply.
    fn merge(_previous: &T, _current: &T, _applied: &T) -> Option<T> {
        None
    }
}

/// A policy's `merge`, as the function a state's cell keeps.
fn merge_of<T, P: MutationPolicy<T>>(previous: &T, current: &T, applied: &T) -> MergeResult<T> {
    match P::merge(previous, current, applied) {
        Some(value) => MergeResult::New(value),
        None => MergeResult::Fail,
    }
}

/// The default policy's: no merge.
fn no_merge<T>(_: &T, _: &T, _: &T) -> MergeResult<T> {
    MergeResult::Fail
}

/// Writes of an equal value (`PartialEq`) invalidate nothing: the default policy.
#[derive(Clone, Copy, Debug, Default)]
pub struct StructuralEqualityPolicy;

impl<T: PartialEq> MutationPolicy<T> for StructuralEqualityPolicy {
    fn equivalent(a: &T, b: &T) -> bool {
        a == b
    }
}

/// Every write invalidates the readers.
#[derive(Clone, Copy, Debug, Default)]
pub struct NeverEqualPolicy;

impl<T> MutationPolicy<T> for NeverEqualPolicy {
    fn equivalent(_: &T, _: &T) -> bool {
        false
    }
}

/// Writes of the same value by identity invalidate nothing, as Compose's
/// `referentialEqualityPolicy()` (`===`): identity as [`ComposeArg`](crate::ComposeArg)
/// defines it, so an `Rc` (a Kotlin object's analogue) by pointer, a state handle by handle,
/// and a small value (which Kotlin boxes) by value. Writing a new `Rc` of an equal value is a
/// change.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReferentialEqualityPolicy;

impl<T: crate::ComposeArg> MutationPolicy<T> for ReferentialEqualityPolicy {
    fn equivalent(a: &T, b: &T) -> bool {
        a.same(b)
    }
}

/// Shared values (`Rc<T>`) compared by value, as Kotlin's `==` on an object: the same `Rc`
/// first, then `T`'s `PartialEq`. For a composition local holding a shared object a provider
/// may make anew with an equal value (a theme's typography, color scheme or shapes): its readers
/// recompose only when the value changes (D1107). An `Rc`'s own [`ComposeArg`](crate::ComposeArg)
/// compares pointers.
#[derive(Clone, Copy, Debug, Default)]
pub struct SharedValueEqualityPolicy;

impl<T: PartialEq + ?Sized> MutationPolicy<std::rc::Rc<T>> for SharedValueEqualityPolicy {
    fn equivalent(a: &std::rc::Rc<T>, b: &std::rc::Rc<T>) -> bool {
        std::rc::Rc::ptr_eq(a, b) || **a == **b
    }
}

/// As Compose's `referentialEqualityPolicy()`.
pub fn referential_equality_policy() -> ReferentialEqualityPolicy {
    ReferentialEqualityPolicy
}

/// As Compose's `structuralEqualityPolicy()`.
pub fn structural_equality_policy() -> StructuralEqualityPolicy {
    StructuralEqualityPolicy
}

/// As Compose's `neverEqualPolicy()`.
pub fn never_equal_policy() -> NeverEqualPolicy {
    NeverEqualPolicy
}

/// A new state cell holding `value`, owned by the returned [`OwnedState`]. Writes of an equal
/// value invalidate nothing, as with Compose's `mutableStateOf`.
#[track_caller]
pub fn mutable_state_of<T: PartialEq + 'static>(value: T) -> OwnedState<T> {
    new_state(value, <T as PartialEq>::eq, no_merge, Location::caller())
}

/// A new state cell with a mutation policy, as `mutableStateOf(value, policy)`.
#[track_caller]
pub fn mutable_state_of_with<T: 'static, P: MutationPolicy<T>>(
    value: T,
    _policy: P,
) -> OwnedState<T> {
    new_state(value, P::equivalent, merge_of::<T, P>, Location::caller())
}

/// A new state cell whose policy is `equivalent`: a composition local's provider's.
#[track_caller]
pub(crate) fn mutable_state_with_equivalence<T: 'static>(
    value: T,
    equivalent: fn(&T, &T) -> bool,
) -> OwnedState<T> {
    new_state(value, equivalent, no_merge, Location::caller())
}

/// A new state of a state object: every write a change, and conflicts decided by `merge`.
pub(crate) fn state_with_merge<T: 'static>(
    value: T,
    merge: MergeFn<T>,
    created_at: &'static Location<'static>,
) -> OwnedState<T> {
    new_state(value, |_, _| false, merge, created_at)
}

fn new_state<T: 'static>(
    value: T,
    equivalent: fn(&T, &T) -> bool,
    merge: MergeFn<T>,
    created_at: &'static Location<'static>,
) -> OwnedState<T> {
    let cell: Rc<dyn StateObjectCell> =
        Rc::new(StateCell::new(value, equivalent, merge, created_at));
    let id = alloc_cell(CellValue::State(cell), created_at);
    OwnedState { handle: MutableState { id, _marker: PhantomData } }
}

fn alloc_cell(cell: CellValue, created_at: &'static Location<'static>) -> StateId {
    let id = with_runtime(|runtime| runtime.cells.alloc(cell, created_at));
    // A thread that only ever makes state still releases the runtime's storage at exit.
    crate::composition::register_exit_guard();
    id
}

/// Retires a cell whose owner was dropped (rule R2), or frees it at once if no composition
/// exists on the thread.
fn retire(id: StateId) {
    let freed = try_with_runtime(|runtime| {
        let cells = &mut runtime.cells;
        if runtime.disposing > 0 {
            cells.held_by_disposals.push(id);
        } else if runtime.finishing > 0 {
            cells.held_by_passes.push(id);
        } else if runtime.compositions.open() == 0 {
            // No pass will come: free it, and anything an interrupted disposal left retired.
            let mut ids = std::mem::take(&mut cells.retired);
            ids.push(id);
            return ids.into_iter().filter_map(|id| cells.free(id)).collect();
        } else {
            cells.retired.push(id);
        }
        Vec::new()
    });
    drop(freed);
    crate::runtime::release_if_empty();
}

/// A `Copy` handle to a state cell: what code and closures pass around.
///
/// It is thread-bound (`!Send`). If the cell was freed, using the handle panics naming where the
/// cell was created; [`try_get`](MutableState::try_get) and [`try_set`](MutableState::try_set)
/// return `None` / `false` instead.
pub struct MutableState<T: 'static> {
    id: StateId,
    _marker: PhantomData<*const T>,
}

impl<T: 'static> Clone for MutableState<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for MutableState<T> {}

impl<T: 'static> PartialEq for MutableState<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for MutableState<T> {}

impl<T: 'static> Hash for MutableState<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<T: 'static> fmt::Debug for MutableState<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "MutableState({}v{})", self.id.index, self.id.generation)
    }
}

/// `MutableState(value=…)@…`, as Compose's `toString`: the value, read without observing it.
impl<T: fmt::Debug + 'static> fmt::Display for MutableState<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cell = try_with_runtime(|runtime| {
            runtime.cells.entry(self.id).ok().and_then(|entry| entry.cell.clone())
        })
        .flatten();
        let (index, generation) = (self.id.index, self.id.generation);
        let Some(CellValue::State(cell)) = cell else {
            return write!(formatter, "MutableState(<freed>)@{index}v{generation}");
        };
        let view = crate::snapshots::view();
        let value = StateCell::<T>::of(&cell).read_in(view.as_ref(), Location::caller());
        write!(formatter, "MutableState(value={:?})@{index}v{generation}", *value)
    }
}

/// As `MutableState`'s, or `DerivedState(value=…)@…` for a derived state: `<Not calculated>`
/// while it isn't up to date.
impl<T: fmt::Debug + 'static> fmt::Display for State<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.derived {
            return fmt::Display::fmt(&self.handle, formatter);
        }
        let id = self.handle.id;
        let cell = try_with_runtime(|runtime| {
            runtime.cells.entry(id).ok().and_then(|entry| entry.cell.clone())
        })
        .flatten();
        match cell {
            Some(CellValue::State(cell)) => {
                crate::derived::display::<T>(&cell, formatter, (id.index, id.generation))
            }
            _ => write!(formatter, "DerivedState(<freed>)@{}v{}", id.index, id.generation),
        }
    }
}

impl<T: 'static> MutableState<T> {
    /// The value. During composition, registers the composing scope as a reader.
    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Runs `f` with a borrow of the value. During composition, registers a read.
    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let at = Location::caller();
        match with_runtime(|runtime| read_cell_in(runtime, self.id, at)) {
            Read::Cell(cell) => {
                let value = StateCell::<T>::of(&cell).read(at);
                f(&value)
            }
            Read::Pass(cell) => {
                let value = StateCell::<T>::of(&cell).read_in_pass(at);
                f(&value)
            }
            Read::Slow(cell) => with_in_view(cell, self.id, at, f),
            Read::Stale(error) => stale(error, at),
        }
    }

    /// Runs `f` with the value, registering no read, as Compose's `withCurrent`: what a state
    /// collection's writes read.
    pub(crate) fn with_current<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let at = Location::caller();
        let (cell, isolated) = object_cell_to_read(self.id, at);
        if isolated {
            let view = crate::snapshots::view();
            let value = StateCell::<T>::of(&cell).read_in(view.as_ref(), at);
            return f(&value);
        }
        let value = StateCell::<T>::of(&cell).read(at);
        f(&value)
    }

    /// Writes the value, invalidating the readers unless the policy finds it equivalent. During
    /// composition too, as in Compose (D703): a reader the pass will still reach recomposes in
    /// it, one it has passed (a backward write) in the next pass.
    #[track_caller]
    pub fn set(&self, value: T) {
        let at = Location::caller();
        if let Some(old) = self.write(value, at).unwrap_or_else(|error| stale(error, at)) {
            crate::snapshots::wrote(self.id);
            // User code, and the write is told before it can panic.
            drop(old);
        }
    }

    pub(crate) fn id(&self) -> StateId {
        self.id
    }

    /// The state's identity, as observers are told it ([`StateObjectId`]).
    pub fn state_object_id(&self) -> StateObjectId {
        StateObjectId(self.id)
    }

    /// Writes `f(current)`.
    #[track_caller]
    pub fn update(&self, f: impl FnOnce(&T) -> T) {
        let value = self.with(f);
        self.set(value);
    }

    /// Mutates the value in place, then invalidates the readers: in-place edits can't tell what
    /// changed. Needs `Clone`: in a snapshot that mustn't see the write in place, it edits a copy.
    #[track_caller]
    pub fn edit<R>(&self, f: impl FnOnce(&mut T) -> R) -> R
    where
        T: Clone,
    {
        self.edit_with(T::clone, f)
    }

    /// [`edit`](Self::edit), with `copy` making a copy when it must.
    #[track_caller]
    pub(crate) fn edit_with<R>(
        &self,
        copy: impl FnOnce(&T) -> T,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        /// An `edit` that panics told of what it may have mutated.
        struct OnUnwind(StateId);
        impl Drop for OnUnwind {
            fn drop(&mut self) {
                crate::snapshots::wrote_unwinding(self.0);
            }
        }
        let at = Location::caller();
        let (cell, isolated) = write_cell(self.id, at).unwrap_or_else(|error| stale(error, at));
        if isolated {
            return self.edit_in_view(cell, copy, f, at);
        }
        let on_unwind = OnUnwind(self.id);
        let result = {
            let mut record = StateCell::<T>::of(&cell).write(at);
            f(&mut record)
        };
        std::mem::forget(on_unwind);
        drop(cell);
        crate::snapshots::wrote(self.id);
        result
    }

    /// `edit` through a view.
    #[cold]
    #[inline(never)]
    fn edit_in_view<R>(
        &self,
        cell: StateRc,
        copy: impl FnOnce(&T) -> T,
        f: impl FnOnce(&mut T) -> R,
        at: &'static Location<'static>,
    ) -> R {
        /// As `edit_with`'s, through a view.
        struct OnUnwind(StateId, bool, SnapshotId);
        impl Drop for OnUnwind {
            fn drop(&mut self) {
                crate::snapshots::wrote_in_view_unwinding(self.0, self.1, self.2);
            }
        }
        let view = crate::snapshots::view().expect("isolation in use");
        let state = StateCell::<T>::of(&cell);
        let (index, modified) = state.writable_in(self.id, &view, copy, at);
        let on_unwind = OnUnwind(self.id, modified, view.id);
        let result = {
            let mut record = state.write_at(index, at);
            f(&mut record)
        };
        std::mem::forget(on_unwind);
        drop(cell);
        crate::snapshots::wrote_in_view(self.id, modified, view.id);
        result
    }

    /// The value, or `None` if the cell was freed.
    #[track_caller]
    pub fn try_get(&self) -> Option<T>
    where
        T: Clone,
    {
        let at = Location::caller();
        match try_with_runtime(|runtime| read_cell_in(runtime, self.id, at))? {
            Read::Cell(cell) => Some(StateCell::<T>::of(&cell).read(at).clone()),
            Read::Pass(cell) => Some(StateCell::<T>::of(&cell).read_in_pass(at).clone()),
            Read::Slow(cell) => Some(with_in_view(cell, self.id, at, T::clone)),
            Read::Stale(_) => None,
        }
    }

    /// Writes the value; returns `false` (dropping `value`) if the cell was freed.
    #[track_caller]
    pub fn try_set(&self, value: T) -> bool {
        match self.write_with(value, Location::caller(), try_write_cell) {
            Ok(Some(old)) => {
                crate::snapshots::wrote(self.id);
                drop(old);
                true
            }
            Ok(None) => true,
            Err(_) => false,
        }
    }

    /// Whether the cell is still live.
    pub fn is_alive(&self) -> bool {
        try_with_runtime(|runtime| runtime.cells.entry(self.id).is_ok()).unwrap_or(false)
    }

    /// A read-only view of the same cell.
    pub fn as_state(&self) -> State<T> {
        State { handle: *self, derived: false }
    }

    /// The old value, for the caller to tell of the write and drop; `None` if there's nothing left
    /// to do: the policy found the value equivalent, or the write went through a view.
    #[inline]
    fn write(&self, value: T, at: &'static Location<'static>) -> Result<Option<T>, Stale> {
        self.write_with(value, at, write_cell)
    }

    #[inline]
    fn write_with(
        &self,
        value: T,
        at: &'static Location<'static>,
        lookup: CellLookup,
    ) -> Result<Option<T>, Stale> {
        let (cell, isolated) = lookup(self.id, at)?;
        if isolated {
            self.set_in_view(cell, value, at);
            return Ok(None);
        }
        let cell = StateCell::<T>::of(&cell);
        let equivalent = {
            let current = cell.read(at);
            (cell.equivalent)(&current, &value)
        };
        if equivalent {
            return Ok(None);
        }
        let mut record = cell.write(at);
        Ok(Some(std::mem::replace(&mut *record, value)))
    }

    /// `set` through a view.
    #[cold]
    #[inline(never)]
    fn set_in_view(&self, cell: StateRc, value: T, at: &'static Location<'static>) {
        let view = crate::snapshots::view().expect("isolation in use");
        let state = StateCell::<T>::of(&cell);
        let equivalent = {
            let current = state.read_in(Some(&view), at);
            (state.equivalent)(&current, &value)
        };
        if equivalent {
            return;
        }
        let wrote = state.overwrite_in(self.id, &view, value, at);
        crate::snapshots::wrote_in_view(self.id, wrote.modified, wrote.written_at);
        // User code, and the write is told before it can panic.
        drop(wrote.old);
    }
}

/// A read-only view of a state cell, as Compose's `State<T>`.
pub struct State<T: 'static> {
    handle: MutableState<T>,
    /// A derived state's: a read validates it first, in its own cell type.
    derived: bool,
}

impl<T: 'static> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for State<T> {}

impl<T: 'static> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
    }
}

impl<T: 'static> Eq for State<T> {}

impl<T: 'static> fmt::Debug for State<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "State({}v{})", self.handle.id.index, self.handle.id.generation)
    }
}

impl<T: 'static> State<T> {
    /// The state's identity, as observers are told it ([`StateObjectId`]).
    pub fn state_object_id(&self) -> StateObjectId {
        self.handle.state_object_id()
    }

    /// A derived state's handle.
    pub(crate) fn of_derived(handle: MutableState<T>) -> State<T> {
        State { handle, derived: true }
    }

    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        if self.derived { crate::derived::read(self.handle.id, f) } else { self.handle.with(f) }
    }

    #[track_caller]
    pub fn try_get(&self) -> Option<T>
    where
        T: Clone,
    {
        if !self.derived {
            return self.handle.try_get();
        }
        self.handle.is_alive().then(|| crate::derived::read(self.handle.id, T::clone))
    }
}

impl<T: 'static> From<MutableState<T>> for State<T> {
    fn from(handle: MutableState<T>) -> State<T> {
        handle.as_state()
    }
}

/// Owns one state cell. Not `Copy`; dropping it retires the cell, which is freed at the end of
/// the next pass (rule R2), or at once if no composition exists on the thread.
///
/// It derefs to its [`MutableState`] handle, so `todo.done.get()` works on the owner directly.
pub struct OwnedState<T: 'static> {
    handle: MutableState<T>,
}

impl<T: 'static> OwnedState<T> {
    /// The `Copy` handle, for closures.
    pub fn handle(&self) -> MutableState<T> {
        self.handle
    }
}

impl<T: 'static> Deref for OwnedState<T> {
    type Target = MutableState<T>;
    fn deref(&self) -> &MutableState<T> {
        &self.handle
    }
}

impl<T: 'static> fmt::Debug for OwnedState<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "OwnedState({}v{})", self.handle.id.index, self.handle.id.generation)
    }
}

impl<T: 'static> Drop for OwnedState<T> {
    fn drop(&mut self) {
        retire(self.handle.id);
    }
}

/// Whether no snapshot is about: reads and writes on their plain paths. For tests.
#[doc(hidden)]
pub fn snapshot_paths_are_plain() -> bool {
    with_runtime(|runtime| !runtime.snapshots.isolated())
}

/// The number of records a state holds (one, unless snapshots need older values), and the
/// version of the one a read sees (each write makes a new one). For tests.
#[doc(hidden)]
pub fn state_records<T: 'static>(state: MutableState<T>) -> (usize, u32) {
    let cell = with_runtime(|runtime| {
        runtime.cells.entry(state.id).ok().and_then(|entry| entry.cell.clone())
    });
    match cell {
        Some(CellValue::State(cell)) => {
            let any: &dyn Any = &*cell;
            match any.downcast_ref::<StateCell<T>>() {
                Some(value) => {
                    let view = crate::snapshots::view();
                    let records = value.records.borrow();
                    (records.len(), records.version_in(view.as_ref()).unwrap_or(0))
                }
                None => (cell.record_count(), 0),
            }
        }
        _ => (0, 0),
    }
}

/// The id of the snapshot that wrote the record this snapshot reads of `state`.
pub(crate) fn record_snapshot_id<T: 'static>(state: MutableState<T>) -> u64 {
    let cell = object_cell(state.id, Location::caller());
    let view = crate::snapshots::view();
    let records = StateCell::<T>::of(&cell).records.borrow();
    match view {
        Some(view) => {
            let index = records.index_in(&view).unwrap_or_else(|| crate::snapshots::read_error());
            records.effective_id(index, &view)
        }
        None => {
            let id = records.head.id;
            if id == crate::snapshots::PREEXISTING { records.born } else { id }
        }
    }
}

/// The number of observed reads a state keeps (current and stale ones). For tests.
#[doc(hidden)]
pub fn state_observed_read_count<T: 'static>(state: MutableState<T>) -> usize {
    with_runtime(|runtime| runtime.cells.entry(state.id).map_or(0, |entry| entry.observed.len()))
}

/// The number of live state cells on this thread. For tests.
#[doc(hidden)]
pub fn live_state_count() -> usize {
    with_runtime(|runtime| runtime.cells.live_count())
}

/// The number of composition reads a state cell keeps (current, stale and repeated ones). For
/// tests.
#[doc(hidden)]
pub fn state_reader_count<T: 'static>(state: MutableState<T>) -> usize {
    with_runtime(|runtime| runtime.cells.entry(state.id).map_or(0, |entry| entry.readers.len()))
}

/// The number of composition reads a state cell has room for before it grows. For tests.
#[doc(hidden)]
pub fn state_reader_capacity<T: 'static>(state: MutableState<T>) -> usize {
    with_runtime(|runtime| {
        runtime.cells.entry(state.id).map_or(0, |entry| entry.readers.capacity())
    })
}

// ---- Objects: what state holders keep ----------------------------------------------------------

/// A `Copy` handle to an object in the thread's cell arena: what a state holder (`DrawerState`,
/// `LazyListState`, `CoroutineScope`, …) wraps, so closures capture it without clones, as they
/// capture a [`MutableState`]. An [`Owned`] owns the object (in a `remember` slot, or in data);
/// once that is dropped, the object is freed at the end of the next pass (rule R2). Unlike a
/// state handle's, using the handle then doesn't panic: [`get`](Self::get) gives `None` and the
/// holder's methods do nothing (D240). Reading it observes nothing (the object's own states
/// do).
pub struct ObjectHandle<T: 'static> {
    id: StateId,
    _marker: PhantomData<*const T>,
}

impl<T: 'static> Clone for ObjectHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for ObjectHandle<T> {}

impl<T: 'static> PartialEq for ObjectHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for ObjectHandle<T> {}

impl<T: 'static> Hash for ObjectHandle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<T: 'static> fmt::Debug for ObjectHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ObjectHandle({}v{})", self.id.index, self.id.generation)
    }
}

impl<T: 'static> ObjectHandle<T> {
    /// The object (an `Rc`, so an async method keeps it across its `.await`s), or `None` once
    /// it was freed (its owner dropped before the last pass ended): a holder's method then does
    /// nothing, as a Kotlin holder nobody shows any more would (D240). Debug builds say so once
    /// per call site, naming the object's type and where it was made.
    #[track_caller]
    pub fn get(&self) -> Option<Rc<T>> {
        match self.lookup() {
            Ok(object) => Some(object),
            Err(stale) => {
                warn_stale(stale, std::any::type_name::<T>(), Location::caller());
                None
            }
        }
    }

    /// The object, or `None` if it was freed, saying nothing.
    pub fn try_get(&self) -> Option<Rc<T>> {
        self.lookup().ok()
    }

    /// Whether the object is still live.
    pub fn is_alive(&self) -> bool {
        self.lookup().is_ok()
    }

    /// Says (in debug builds, once per call site) that the object was used after it was
    /// freed, for a holder that found it out another way (its own state handles).
    #[track_caller]
    pub fn warn_if_freed(&self) {
        if let Err(stale) = self.lookup() {
            warn_stale(stale, std::any::type_name::<T>(), Location::caller());
        }
    }

    fn lookup(&self) -> Result<Rc<T>, Stale> {
        let cell = try_with_runtime(|runtime| {
            runtime.cells.entry(self.id).map(|entry| entry.cell.clone().expect("a live cell"))
        })
        .unwrap_or(Err(Stale { created_at: None }))?;
        let CellValue::Object(object) = cell else {
            panic!("compose-runtime: object handle of a state");
        };
        Ok(object.downcast::<T>().unwrap_or_else(|_| {
            panic!("compose-runtime: object handle of the wrong type");
        }))
    }
}

/// A freed object used at `used_at`: debug builds log a warning (`log::warn!`) once per call site.
#[cold]
fn warn_stale(stale: Stale, type_name: &str, used_at: &'static Location<'static>) {
    if !cfg!(debug_assertions) {
        return;
    }
    thread_local! {
        static WARNED: RefCell<Vec<&'static Location<'static>>> = const { RefCell::new(Vec::new()) };
    }
    let first = WARNED
        .try_with(|warned| {
            let mut warned = warned.borrow_mut();
            let first = !warned.contains(&used_at);
            if first {
                warned.push(used_at);
            }
            first
        })
        .unwrap_or(false);
    if !first {
        return;
    }
    let made = stale.created_at.map_or_else(String::new, |at| format!(" made at {at}"));
    log::warn!(
        "{type_name}{made} was used at {used_at} after its owner (a `remember` slot, or an \
         `Owned`) was dropped; ignored (D240)"
    );
}

/// Owns an object in the thread's cell arena, as [`OwnedState`] owns a state cell, and derefs
/// to its `Copy` handle `H`: `DrawerState::new(…)` is an `Owned<DrawerState>`. Not `Copy`;
/// dropping it retires the object, which is freed at the end of the next pass (rule R2), or at
/// once if no composition exists on the thread. `remember` keeps it and returns the handle.
///
/// **Keep the owner.** `let state = *DrawerState::new(…);` compiles, but drops the owner at
/// the end of the statement: the handle is stale from the start (its methods then do nothing,
/// D240). Write `let state = DrawerState::new(…);` (the owner, which derefs to the handle) and
/// copy the handle out (`*state`) for closures while it lives, or `remember` it.
pub struct Owned<H: Copy + 'static> {
    handle: H,
    object: StateId,
}

impl<H: Copy + 'static> Owned<H> {
    /// Moves `object` into the arena, owned by the result, whose handle `handle` makes from the
    /// object's (a state holder wrapping it).
    #[track_caller]
    pub fn new<T: 'static>(object: T, handle: impl FnOnce(ObjectHandle<T>) -> H) -> Owned<H> {
        let id = alloc_cell(CellValue::Object(Rc::new(object)), Location::caller());
        Owned { handle: handle(ObjectHandle { id, _marker: PhantomData }), object: id }
    }

    /// The `Copy` handle, for closures.
    pub fn handle(&self) -> H {
        self.handle
    }
}

impl<H: Copy + 'static> Deref for Owned<H> {
    type Target = H;
    fn deref(&self) -> &H {
        &self.handle
    }
}

impl<H: Copy + fmt::Debug + 'static> fmt::Debug for Owned<H> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Owned({:?})", self.handle)
    }
}

impl<H: Copy + 'static> Drop for Owned<H> {
    fn drop(&mut self) {
        retire(self.object);
    }
}
