//! Observable state: `Copy` handles into a per-thread arena of cells.
//!
//! A cell is an `Rc` in the arena. Reading or writing clones that `Rc` out of the arena and lets
//! the runtime's borrow go before touching the value, so `Clone`, `PartialEq` and `Drop` of the
//! value (user code) run outside the runtime, and a cell freed during a `with` stays alive until
//! the `with` ends.

use std::any::Any;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use std::rc::Rc;

use crate::runtime::{
    Runtime, invalidate_scope_parts, request_frames, try_with_runtime, with_runtime,
};
use crate::scope::{ScopeId, Scopes};

/// A generational handle to a cell in the thread's cell arena.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct StateId {
    index: u32,
    generation: u32,
}

/// The value of a freed cell, dropped outside the runtime's borrow.
pub(crate) type CellValue = Rc<dyn Any>;

#[derive(Clone, Copy)]
struct Reader {
    scope: ScopeId,
    epoch: u32,
}

struct CellEntry {
    /// Odd while the cell is live; bumped on allocation and on free.
    generation: u32,
    cell: Option<Rc<dyn Any>>,
    readers: Vec<Reader>,
    /// Reads observed outside composition (layout, drawing).
    observed: Vec<crate::observe::ObservedRead>,
    /// A derived state's result whose dependencies changed: a read recomputes it first.
    dirty: bool,
    /// The derived state computing this cell, if it is one's result.
    refresh: Option<std::rc::Weak<dyn crate::derived::Refresh>>,
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

    fn alloc(&mut self, cell: Rc<dyn Any>, created_at: &'static Location<'static>) -> StateId {
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
                dirty: false,
                refresh: None,
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
        entry.dirty = false;
        entry.refresh = None;
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

/// Records a read by the scope composing now, if any.
fn register_read(entry: &mut CellEntry, scopes: &Scopes, scope: ScopeId) {
    let Some(epoch) = scopes.get(scope).map(|scope| scope.epoch) else {
        return;
    };
    if let Some(reader) = entry.readers.iter_mut().rev().find(|reader| reader.scope == scope) {
        reader.epoch = epoch;
        return;
    }
    if entry.readers.len() == entry.readers.capacity() {
        // Prune stale readers before growing, so cells read by short-lived scopes stay small.
        entry.readers.retain(|reader| scopes.is_current(reader.scope, reader.epoch));
    }
    entry.readers.push(Reader { scope, epoch });
}

/// Records a read observed outside composition, under the observation in progress.
fn register_observed_read(
    entry: &mut CellEntry,
    observers: &crate::observe::Observers,
    observing: crate::observe::Observing,
) {
    if let Some(read) = entry
        .observed
        .iter_mut()
        .find(|read| read.observer == observing.observer && read.key == observing.key)
    {
        read.epoch = observing.epoch;
        return;
    }
    if entry.observed.len() == entry.observed.capacity() {
        entry.observed.retain(|read| observers.is_current(read));
    }
    entry.observed.push(crate::observe::ObservedRead {
        observer: observing.observer,
        key: observing.key,
        epoch: observing.epoch,
    });
}

/// `write_cell` or `try_write_cell`.
type CellLookup = fn(StateId, &'static Location<'static>) -> Result<Rc<dyn Any>, Stale>;

/// Looks a cell up for reading, registering the read.
fn read_cell(id: StateId, at: &'static Location<'static>) -> Result<Rc<dyn Any>, Stale> {
    refresh_if_dirty(id);
    with_runtime(|runtime| read_cell_in(runtime, id, at))
}

/// Like [`read_cell`], for `try_get`: a stale cell once the thread's runtime is gone.
fn try_read_cell(id: StateId, at: &'static Location<'static>) -> Result<Rc<dyn Any>, Stale> {
    refresh_if_dirty(id);
    try_with_runtime(|runtime| read_cell_in(runtime, id, at))
        .unwrap_or(Err(Stale { created_at: None }))
}

/// A derived state's result whose dependencies changed is recomputed before it is read, so a
/// read is never behind what it derives from.
fn refresh_if_dirty(id: StateId) {
    let refresh = try_with_runtime(|runtime| {
        let entry = runtime.cells.entry(id).ok()?;
        if !entry.dirty {
            return None;
        }
        entry.refresh.as_ref()?.upgrade()
    })
    .flatten();
    if let Some(refresh) = refresh {
        refresh.refresh();
    }
}

/// Makes `id` a derived state's result, recomputed by `refresh` when dirty.
pub(crate) fn set_refresh(id: StateId, refresh: std::rc::Weak<dyn crate::derived::Refresh>) {
    with_runtime(|runtime| {
        if let Ok(entry) = runtime.cells.entry(id) {
            entry.refresh = Some(refresh);
        }
    });
}

/// Clears `id`'s dirty mark; returns whether it was set.
pub(crate) fn take_dirty(id: StateId) -> bool {
    try_with_runtime(|runtime| {
        runtime.cells.entry(id).map(|entry| std::mem::take(&mut entry.dirty)).unwrap_or(false)
    })
    .unwrap_or(false)
}

fn read_cell_in(
    runtime: &mut Runtime,
    id: StateId,
    _at: &'static Location<'static>,
) -> Result<Rc<dyn Any>, Stale> {
    let scope = runtime.reading_scope();
    let Runtime { cells, scopes, observers, observing, .. } = runtime;
    let entry = cells.entry(id)?;
    match *observing {
        Some(observing) if observing.exclusive || scope.is_none() => {
            register_observed_read(entry, observers, observing)
        }
        _ if !scope.is_none() => register_read(entry, scopes, scope),
        _ => {}
    }
    Ok(entry.cell.clone().expect("a live cell"))
}

/// Looks a cell up for writing.
fn write_cell(id: StateId, at: &'static Location<'static>) -> Result<Rc<dyn Any>, Stale> {
    with_runtime(|runtime| write_cell_in(runtime, id, at))
}

/// Like [`write_cell`], for `try_set`: a stale cell once the thread's runtime is gone.
fn try_write_cell(id: StateId, at: &'static Location<'static>) -> Result<Rc<dyn Any>, Stale> {
    try_with_runtime(|runtime| write_cell_in(runtime, id, at))
        .unwrap_or(Err(Stale { created_at: None }))
}

fn write_cell_in(
    runtime: &mut Runtime,
    id: StateId,
    at: &'static Location<'static>,
) -> Result<Rc<dyn Any>, Stale> {
    if cfg!(debug_assertions) && runtime.is_composing() {
        panic!(
            "compose-runtime: state written at {at} during composition; composition must be a \
             pure function of state: write in a handler or an effect"
        );
    }
    let entry = runtime.cells.entry(id)?;
    Ok(entry.cell.clone().expect("a live cell"))
}

/// Invalidates the scopes that read a cell in their latest run, and prunes the others.
fn invalidate_readers(id: StateId) {
    invalidate_readers_except(id, ScopeId::NONE);
}

/// The same, but `skip`, a scope reading the cell right now (a derived state refreshed for it).
/// Derived states that read the cell are marked dirty, and those reading them, at once: a read
/// recomputes them, and a frame (`flush_derived_states`) tells their readers if they changed.
pub(crate) fn invalidate_readers_except(id: StateId, skip: ScopeId) {
    let requested = with_runtime(|runtime| {
        let Runtime { cells, scopes, compositions, frame_requests, observers, .. } = runtime;
        let Ok(entry) = cells.entry(id) else {
            return false;
        };
        let mut requested = false;
        entry.readers.retain(|reader| {
            if !scopes.is_current(reader.scope, reader.epoch) {
                return false;
            }
            if reader.scope != skip {
                requested |=
                    invalidate_scope_parts(scopes, compositions, frame_requests, reader.scope);
            }
            true
        });
        // Reads observed in layout and drawing: their observers hear the keys. A derived
        // state's observer marks its result dirty.
        let mut dirty = Vec::new();
        entry.observed.retain(|read| {
            if !observers.is_current(read) {
                return false;
            }
            if let Some(cell) = observers.dirty_cell(read) {
                dirty.push(cell);
            }
            if let Some(notify) = observers.invalidate(read) {
                frame_requests.push(notify);
                requested = true;
            }
            true
        });
        while let Some(cell) = dirty.pop() {
            let Ok(entry) = cells.entry(cell) else { continue };
            if entry.dirty {
                continue;
            }
            entry.dirty = true;
            for read in &entry.observed {
                if observers.is_current(read)
                    && let Some(cell) = observers.dirty_cell(read)
                {
                    dirty.push(cell);
                }
            }
        }
        requested || crate::runtime::frames_requested(runtime)
    });
    if requested {
        request_frames();
    }
}

/// One state cell: the value behind a run-time borrow flag that remembers who holds it.
struct StateCell<T> {
    value: RefCell<T>,
    borrowed_at: Cell<Option<&'static Location<'static>>>,
    equivalent: fn(&T, &T) -> bool,
    created_at: &'static Location<'static>,
}

impl<T: 'static> StateCell<T> {
    fn of(cell: &Rc<dyn Any>) -> &StateCell<T> {
        cell.downcast_ref::<StateCell<T>>()
            .expect("compose-runtime: state handle of the wrong type")
    }

    fn read(&self, at: &'static Location<'static>) -> CellRef<'_, T> {
        match self.value.try_borrow() {
            Ok(value) => {
                let outer = self.borrowed_at.get().is_none();
                if outer {
                    self.borrowed_at.set(Some(at));
                }
                CellRef { value, cell: self, outer }
            }
            Err(_) => self.conflict(at),
        }
    }

    fn write(&self, at: &'static Location<'static>) -> CellMut<'_, T> {
        match self.value.try_borrow_mut() {
            Ok(value) => {
                self.borrowed_at.set(Some(at));
                CellMut { value, cell: self }
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

/// A new state holding an empty list, as Compose's `mutableStateListOf()`: `edit` it to change
/// the list; every edit invalidates the readers (the list's elements need no `PartialEq`).
#[track_caller]
pub fn mutable_state_list_of<T: 'static>() -> OwnedState<Vec<T>> {
    new_state(Vec::new(), NeverEqualPolicy::equivalent, Location::caller())
}

/// A new list state holding `items`, as Compose's `toMutableStateList()`.
#[track_caller]
pub fn mutable_state_list_from<T: 'static>(
    items: impl IntoIterator<Item = T>,
) -> OwnedState<Vec<T>> {
    new_state(items.into_iter().collect(), NeverEqualPolicy::equivalent, Location::caller())
}

fn new_state<T: 'static>(
    value: T,
    equivalent: fn(&T, &T) -> bool,
    created_at: &'static Location<'static>,
) -> OwnedState<T> {
    let cell: Rc<dyn Any> = Rc::new(StateCell {
        value: RefCell::new(value),
        borrowed_at: Cell::new(None),
        equivalent,
        created_at,
    });
    OwnedState { handle: MutableState { id: alloc_cell(cell, created_at), _marker: PhantomData } }
}

fn alloc_cell(cell: Rc<dyn Any>, created_at: &'static Location<'static>) -> StateId {
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

    /// Writes the value, invalidating the readers unless the policy finds it equivalent.
    ///
    /// # Panics
    ///
    /// In debug builds, during composition (composition must be a pure function of state).
    #[track_caller]
    pub fn set(&self, value: T) {
        let at = Location::caller();
        if !self.write(value, at).unwrap_or_else(|error| stale(error, at)) {
            return;
        }
        invalidate_readers(self.id);
    }

    /// A derived state's new result: written even during composition (a read refreshes it),
    /// invalidating its readers if it changed, but the scope reading it now.
    pub(crate) fn set_derived(&self, value: T) {
        let at = Location::caller();
        let lookup: CellLookup = |id, _| {
            with_runtime(|runtime| {
                runtime.cells.entry(id).map(|entry| entry.cell.clone().expect("a live cell"))
            })
        };
        if self.write_with(value, at, lookup).unwrap_or(false) {
            let skip = with_runtime(|runtime| runtime.reading_scope());
            invalidate_readers_except(self.id, skip);
        }
    }

    pub(crate) fn id(&self) -> StateId {
        self.id
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
        let at = Location::caller();
        let cell = write_cell(self.id, at).unwrap_or_else(|error| stale(error, at));
        let result = {
            let mut value = StateCell::<T>::of(&cell).write(at);
            f(&mut value)
        };
        drop(cell);
        invalidate_readers(self.id);
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
            Ok(true) => {
                invalidate_readers(self.id);
                true
            }
            Ok(false) => true,
            Err(_) => false,
        }
    }

    /// Whether the cell is still live.
    pub fn is_alive(&self) -> bool {
        try_with_runtime(|runtime| runtime.cells.entry(self.id).is_ok()).unwrap_or(false)
    }

    /// A read-only view of the same cell.
    pub fn as_state(&self) -> State<T> {
        State { handle: *self }
    }

    /// Writes the value during composition, invalidating its readers: for a composition local's
    /// provider, whose readers are its descendants, which recompose later in the same pass.
    #[track_caller]
    pub(crate) fn set_while_composing(&self, value: T) {
        let at = Location::caller();
        let lookup: CellLookup = |id, _| {
            with_runtime(|runtime| {
                let entry = runtime.cells.entry(id)?;
                Ok(entry.cell.clone().expect("a live cell"))
            })
        };
        if self.write_with(value, at, lookup).unwrap_or_else(|error| stale(error, at)) {
            invalidate_readers(self.id);
        }
    }

    /// Returns whether the value changed.
    fn write(&self, value: T, at: &'static Location<'static>) -> Result<bool, Stale> {
        self.write_with(value, at, write_cell)
    }

    fn write_with(
        &self,
        value: T,
        at: &'static Location<'static>,
        lookup: CellLookup,
    ) -> Result<bool, Stale> {
        let cell = lookup(self.id, at)?;
        let cell = StateCell::<T>::of(&cell);
        let equivalent = {
            let current = cell.read(at);
            (cell.equivalent)(&current, &value)
        };
        if equivalent {
            return Ok(false);
        }
        let old = std::mem::replace(&mut *cell.write(at), value);
        drop(old);
        Ok(true)
    }
}

/// A read-only view of a state cell, as Compose's `State<T>`.
pub struct State<T: 'static> {
    handle: MutableState<T>,
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
    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.handle.get()
    }

    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.handle.with(f)
    }

    #[track_caller]
    pub fn try_get(&self) -> Option<T>
    where
        T: Clone,
    {
        self.handle.try_get()
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

/// The number of live state cells on this thread. For tests.
#[doc(hidden)]
pub fn live_state_count() -> usize {
    with_runtime(|runtime| runtime.cells.live_count())
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
        Ok(cell.downcast::<T>().unwrap_or_else(|_| {
            panic!("compose-runtime: object handle of the wrong type");
        }))
    }
}

/// A freed object used at `used_at`: debug builds print it once per call site.
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
    eprintln!(
        "compose-runtime: {type_name}{made} was used at {used_at} after its owner (a `remember` \
         slot, or an `Owned`) was dropped; ignored (D240)"
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
        let id = alloc_cell(Rc::new(object), Location::caller());
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
