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
use crate::snapshots::{Records, StateObjectCell, StateObjectId};

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
    /// A derived state, queued for the next send to validate.
    in_derived_queue: bool,
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
                in_derived_queue: false,
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
        pending: bool,
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
            entry.pending |= pending;
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

/// `write_cell` or `try_write_cell`: the cell.
type CellLookup = fn(StateId, &'static Location<'static>) -> Result<StateRc, Stale>;

/// Looks a cell up for reading, registering the read.
fn read_cell(id: StateId, at: &'static Location<'static>) -> Result<StateRc, Stale> {
    match with_runtime(|runtime| read_cell_in(runtime, id, at)) {
        Read::Cell(cell) => Ok(cell),
        Read::Observed(cell) => {
            crate::snapshots::notify_read(id);
            Ok(cell)
        }
        Read::Stale(stale) => Err(stale),
    }
}

/// A state object's cell, not registering a read. Panics if it's freed (rule R3).
#[track_caller]
pub(crate) fn object_cell(id: StateId, at: &'static Location<'static>) -> StateRc {
    with_runtime(|runtime| runtime.cells.entry(id).map(|entry| entry.state()))
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
        let mark = (runtime.snapshots.global(), runtime.snapshots.version());
        match runtime.cells.entry(id) {
            Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) if cell.valid_at(mark) => {
                Some(read_cell_in(runtime, id, at))
            }
            _ => None,
        }
    })?;
    match read {
        Read::Cell(cell) => Some((cell, false)),
        Read::Observed(cell) => {
            crate::snapshots::notify_read(id);
            Some((cell, true))
        }
        Read::Stale(_) => None,
    }
}

/// Registers a read of derived state `id`, brought up to date; returns whether read observers
/// heard it.
pub(crate) fn register_read_of(id: StateId, at: &'static Location<'static>) -> bool {
    match with_runtime(|runtime| read_cell_in(runtime, id, at)) {
        Read::Cell(_) => false,
        Read::Observed(_) => {
            crate::snapshots::notify_read(id);
            true
        }
        Read::Stale(error) => stale(error, at),
    }
}

/// A state object's current version: a derived state is brought up to date. `None` if it's
/// freed, or its records are being written.
pub(crate) fn current_version(id: StateId) -> Option<u32> {
    let cell = try_with_runtime(|runtime| match runtime.cells.entry(id) {
        Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) => Some(cell.clone()),
        _ => None,
    })??;
    cell.current_version()
}

/// Tells the read observers what state object `id` read, if it's a derived state.
pub(crate) fn notify_dependencies(id: StateId) {
    let cell = try_with_runtime(|runtime| match runtime.cells.entry(id) {
        Ok(CellEntry { cell: Some(CellValue::State(cell)), .. }) => Some(cell.clone()),
        _ => None,
    })
    .flatten();
    if let Some(cell) = cell {
        cell.notify_dependencies();
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

/// Like [`read_cell`], for `try_get`: a stale cell once the thread's runtime is gone.
fn try_read_cell(id: StateId, at: &'static Location<'static>) -> Result<StateRc, Stale> {
    match try_with_runtime(|runtime| read_cell_in(runtime, id, at)) {
        Some(Read::Cell(cell)) => Ok(cell),
        Some(Read::Observed(cell)) => {
            crate::snapshots::notify_read(id);
            Ok(cell)
        }
        Some(Read::Stale(stale)) => Err(stale),
        None => Err(Stale { created_at: None }),
    }
}

/// What a read found.
enum Read {
    /// The cell.
    Cell(StateRc),
    /// The cell, and read observers ([`Snapshot::observe`]) must hear the read.
    ///
    /// [`Snapshot::observe`]: crate::snapshots::Snapshot::observe
    Observed(StateRc),
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
        Some(observing) if observing.exclusive || scope.is_none() => {
            if snapshots.observes_at(observing.depth)
                && register_observed_read(entry, observers, observing)
                && observing.exclusive
            {
                // A derived state's calculation: what it read, this once, is a dependency.
                snapshots.record_dependency(id);
            }
        }
        _ if !scope.is_none() && snapshots.composition_reads() => {
            register_read(entry, scopes, scope)
        }
        _ => {}
    }
    let cell = entry.state();
    if snapshots.reads_observed() { Read::Observed(cell) } else { Read::Cell(cell) }
}

/// Looks a cell up for writing.
fn write_cell(id: StateId, at: &'static Location<'static>) -> Result<StateRc, Stale> {
    with_runtime(|runtime| write_cell_in(runtime, id, at))
}

/// Like [`write_cell`], for `try_set`: a stale cell once the thread's runtime is gone.
fn try_write_cell(id: StateId, at: &'static Location<'static>) -> Result<StateRc, Stale> {
    try_with_runtime(|runtime| write_cell_in(runtime, id, at))
        .unwrap_or(Err(Stale { created_at: None }))
}

/// A write during composition is allowed, as Compose's (D703): it invalidates the readers, the
/// ones the pass will still reach in this pass.
fn write_cell_in(
    runtime: &mut Runtime,
    id: StateId,
    _at: &'static Location<'static>,
) -> Result<StateRc, Stale> {
    Ok(runtime.cells.entry(id)?.state())
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

/// Invalidates what read a cell in its latest run, pruning the others: a write in composition
/// (D703) or, `sent`, the send of one outside it (D751). Composables and observations are told;
/// for the derived states that read it, and those reading them, nothing is computed here: the
/// composables that read one are invalidated on the condition that its value changed, which the
/// pass reaching them checks (Compose's conditional invalidation), and one that an observation
/// read is queued, for the send to validate outside the borrow. One nobody reads isn't computed.
pub(crate) fn invalidate_cell(runtime: &mut Runtime, id: StateId, sent: bool) {
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
    if sent {
        entry.in_global_modified = false;
        if !std::mem::take(&mut entry.pending) {
            return;
        }
    }
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
    let mut derived: Vec<StateId> = Vec::new();
    if !entry.observed.is_empty() {
        entry.observed.retain(|read| {
            if !observers.is_current(read) {
                return false;
            }
            match observers.dirty_cell(read) {
                Some(cell) => {
                    if !derived.contains(&cell) {
                        derived.push(cell);
                    }
                }
                None => {
                    if let Some(notify) = observers.invalidate(read) {
                        frame_requests.push(notify);
                    }
                }
            }
            true
        });
    }
    let mut next = 0;
    while next < derived.len() {
        let cell = derived[next];
        next += 1;
        let Ok(entry) = cells.entry(cell) else { continue };
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
                Some(reading) => {
                    if !derived.contains(&reading) {
                        derived.push(reading);
                    }
                }
                None => observed = true,
            }
        }
        if observed && !entry.in_derived_queue {
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
    created_at: &'static Location<'static>,
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
    fn current_version(&self) -> Option<u32> {
        self.records.try_borrow().ok().map(|records| records.version())
    }
}

impl<T: 'static> StateCell<T> {
    fn new(
        value: T,
        equivalent: fn(&T, &T) -> bool,
        created_at: &'static Location<'static>,
    ) -> Self {
        let (snapshot, version) =
            with_runtime(|runtime| (runtime.snapshots.global(), runtime.snapshots.next_version()));
        StateCell {
            records: RefCell::new(Records::new(ValueRecord { value }, snapshot, version)),
            borrowed_at: Cell::new(None),
            equivalent,
            created_at,
        }
    }

    #[inline]
    fn of(cell: &StateRc) -> &StateCell<T> {
        let any: &dyn Any = &**cell;
        any.downcast_ref::<StateCell<T>>().expect("compose-runtime: state handle of the wrong type")
    }

    /// The readable record's value.
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

    /// The writable record, to overwrite ([`CellMut::wrote_in`] says the write is done).
    fn write(&self, at: &'static Location<'static>) -> CellMut<'_, T> {
        match self.records.try_borrow_mut() {
            Ok(records) => {
                self.borrowed_at.set(Some(at));
                CellMut { records, cell: self }
            }
            Err(_) => self.conflict(at),
        }
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
    records: RefMut<'a, Records<ValueRecord<T>>>,
    cell: &'a StateCell<T>,
}

impl<T> Deref for CellMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.records.head.data.value
    }
}

impl<T> DerefMut for CellMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.records.head.data.value
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
    new_state(value, <T as PartialEq>::eq, Location::caller())
}

/// A new state cell with a mutation policy, as `mutableStateOf(value, policy)`.
#[track_caller]
pub fn mutable_state_of_with<T: 'static, P: MutationPolicy<T>>(
    value: T,
    _policy: P,
) -> OwnedState<T> {
    new_state(value, P::equivalent, Location::caller())
}

/// A new state cell whose policy is `equivalent`: a composition local's provider's.
#[track_caller]
pub(crate) fn mutable_state_with_equivalence<T: 'static>(
    value: T,
    equivalent: fn(&T, &T) -> bool,
) -> OwnedState<T> {
    new_state(value, equivalent, Location::caller())
}

fn new_state<T: 'static>(
    value: T,
    equivalent: fn(&T, &T) -> bool,
    created_at: &'static Location<'static>,
) -> OwnedState<T> {
    let cell: Rc<dyn StateObjectCell> = Rc::new(StateCell::new(value, equivalent, created_at));
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
        let value = StateCell::<T>::of(&cell).read(Location::caller());
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
        let cell = read_cell(self.id, at).unwrap_or_else(|error| stale(error, at));
        let value = StateCell::<T>::of(&cell).read(at);
        f(&value)
    }

    /// Runs `f` with the value, registering no read, as Compose's `withCurrent`: what a state
    /// collection's writes read.
    pub(crate) fn with_current<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let at = Location::caller();
        let cell = object_cell(self.id, at);
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
    /// changed.
    #[track_caller]
    pub fn edit<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        /// An `edit` that panics told of what it may have mutated.
        struct OnUnwind(StateId);
        impl Drop for OnUnwind {
            fn drop(&mut self) {
                crate::snapshots::wrote_unwinding(self.0);
            }
        }
        let at = Location::caller();
        let cell = write_cell(self.id, at).unwrap_or_else(|error| stale(error, at));
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

    /// The value, or `None` if the cell was freed.
    #[track_caller]
    pub fn try_get(&self) -> Option<T>
    where
        T: Clone,
    {
        let at = Location::caller();
        let cell = try_read_cell(self.id, at).ok()?;
        let value = StateCell::<T>::of(&cell).read(at).clone();
        Some(value)
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

    /// The old value, or `None` if the policy found the value equivalent.
    fn write(&self, value: T, at: &'static Location<'static>) -> Result<Option<T>, Stale> {
        self.write_with(value, at, write_cell)
    }

    fn write_with(
        &self,
        value: T,
        at: &'static Location<'static>,
        lookup: CellLookup,
    ) -> Result<Option<T>, Stale> {
        let cell = lookup(self.id, at)?;
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
                    let records = value.records.borrow();
                    (records.len(), records.version())
                }
                None => (cell.record_count(), 0),
            }
        }
        _ => (0, 0),
    }
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
