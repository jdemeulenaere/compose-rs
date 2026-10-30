//! `derived_state_of`: a state computed from other state, as Compose's `derivedStateOf`
//! (docs/parity/design/snapshots.md §3.10).
//!
//! It's a state object of its own: its cell keeps a [`ResultRecord`] in records, the result with
//! the states the calculation read and the versions it saw of them. Nothing happens at a write:
//! a read validates the record, by the snapshot and the write counter when no write came since,
//! else by comparing what it read with the versions it saw (derived states among them first
//! brought up to date), and recomputes only if one differs, so no read is ever behind what it
//! derives from. A result found equivalent by a policy given keeps its version and stops the
//! chain; without one, as Compose's `derivedStateOf(calculation)`, every calculation writes a new
//! record, and readers compare structurally. It's lazy (D711): nothing computes it but a read, and a send telling of a change what
//! reads it: the composables that read it are invalidated on the condition that its value
//! changed, which the pass reaching them checks, so one that left isn't a reason to compute it;
//! one that layout or drawing reads is validated at the send. A recomputation that changes the
//! result invalidates what read the old one at once, which is every reader registered so far: a
//! read validates first and registers after.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::panic::Location;
use std::rc::Rc;

use crate::observe::ReadObserver;
use crate::remember::Remember;
use crate::snapshots::{Records, SnapshotId, StateObjectCell, View};
use crate::state::{MutationPolicy, OwnedState, State, StateId};

/// What a derived state's calculation gave, in a snapshot, as Compose's `ResultRecord`. The record
/// is borrowed only to look at or replace these, never across user code: a read hands out the
/// `Rc`s, so a send or a recomputation can come at any point of a calculation, a validation or a
/// `with` closure.
struct ResultRecord<T> {
    /// `None` until the first calculation.
    result: Option<Rc<T>>,
    /// The snapshot and the write counter it was last found valid at: no write since, it still
    /// is.
    valid_at: Cell<(SnapshotId, u32)>,
    /// What the calculation read directly, with the version of the record each had. Versions
    /// are distinct, but only their equality means anything.
    dependencies: Rc<[(StateId, u32)]>,
    /// The version of the record that first held this value (a recalculation finding it
    /// equivalent keeps this): what tells readers in the global snapshot that another snapshot's
    /// record, become theirs, holds another value.
    value_since: u32,
}

/// A derived state's cell in the arena, as Compose's `DerivedSnapshotState`.
pub(crate) struct DerivedCell<T: 'static> {
    calculation: Box<dyn Fn() -> T>,
    /// The mutation policy given, as Compose's `policy`: a new result it finds equivalent to the
    /// old one writes no new record, so what reads this state isn't recomputed. `None`, as
    /// `derivedStateOf(calculation)`: every calculation writes one.
    policy: Option<fn(&T, &T) -> bool>,
    /// What decides that readers saw another value, as `isInvalidFor`: the policy, or structural.
    equivalent: fn(&T, &T) -> bool,
    records: RefCell<Records<ResultRecord<T>>>,
    /// Its calculation is running: a read of it is an error.
    calculating: Cell<bool>,
    /// Calculations so far: a validation restarts if one came meanwhile (a send it caused).
    calculations: Cell<u32>,
    /// Records what the calculation read in those states' cells, by which a send finds the
    /// derived states reading what changed.
    observer: ReadObserver,
    /// Its id, once allocated.
    id: Cell<Option<StateId>>,
    /// The keys its observer read under in snapshots other than the global one, one per snapshot:
    /// what a calculation there read adds to what a send walks through (a record of that snapshot
    /// may become what the global snapshot reads), until a calculation with no snapshot open.
    side_keys: RefCell<Vec<u64>>,
    /// The `value_since` of the record the global snapshot last saw: another means a snapshot's
    /// calculation became what it reads (its apply), of which its readers must hear.
    global_saw: Cell<u32>,
    /// Calculated in a snapshot other than the global one since a calculation with none open.
    in_other_snapshots: Cell<bool>,
}

impl<T: 'static> StateObjectCell for DerivedCell<T> {
    fn record_count(&self) -> usize {
        self.records.borrow().len()
    }

    fn wrote_in(&self, _id: SnapshotId, _version: u32) -> bool {
        false
    }

    fn current_version(&self, view: Option<&View>) -> Option<u32> {
        // A send a running calculation caused doesn't validate that calculation.
        if !self.calculating.get() {
            self.current();
        }
        if view.is_none() {
            return self.records.borrow().version_in(None);
        }
        // The snapshot may have advanced.
        let view = crate::snapshots::view();
        self.records.borrow().version_in(view.as_ref())
    }

    #[inline]
    fn valid_at(&self, mark: (SnapshotId, u32)) -> bool {
        let records = self.records.borrow();
        let record = &records.head.data;
        record.result.is_some() && record.valid_at.get() == mark
    }

    fn collect_dependencies(&self, into: &mut Vec<StateId>) {
        let view = crate::snapshots::view();
        let dependencies = {
            let records = self.records.borrow();
            match &view {
                None => records.head.data.dependencies.clone(),
                Some(view) => records.readable_in(view).data.dependencies.clone(),
            }
        };
        for &(dependency, _) in dependencies.iter() {
            // Each once, a diamond's state too.
            if !into.contains(&dependency) {
                into.push(dependency);
                crate::state::collect_dependencies(dependency, into);
            }
        }
    }

    fn abandon(
        &self,
        id: SnapshotId,
        previous: &crate::snapshots::IdSet,
        garbage: &mut Vec<Box<dyn Any>>,
    ) {
        if let Ok(mut records) = self.records.try_borrow_mut() {
            records.abandon(id, previous, garbage);
        }
    }

    fn reclaim(&self, reuse_limit: SnapshotId, garbage: &mut Vec<Box<dyn Any>>) -> bool {
        match self.records.try_borrow_mut() {
            Ok(mut records) => records.reclaim(reuse_limit, garbage),
            Err(_) => true,
        }
    }
}

/// The mark to keep after work that started at `mark`: a write since makes the next read compare
/// versions, so no read is behind what it derives from whatever that work ran (D765); only an
/// advance of the snapshot keeps the fast path.
fn mark_after(mark: (SnapshotId, u32)) -> (SnapshotId, u32) {
    let now = crate::snapshots::valid_mark();
    if now.1 == mark.1 { now } else { mark }
}

impl<T: 'static> DerivedCell<T> {
    fn of(cell: &Rc<dyn StateObjectCell>) -> &DerivedCell<T> {
        let any: &dyn Any = &**cell;
        any.downcast_ref().expect("compose-runtime: state handle of the wrong type")
    }

    /// `f` of the record a read through `view` sees.
    #[inline]
    fn with_record<R>(&self, view: Option<&View>, f: impl FnOnce(&ResultRecord<T>) -> R) -> R {
        let records = self.records.borrow();
        match view {
            None => f(&records.head.data),
            Some(view) => f(&records.readable_in(view).data),
        }
    }

    /// Whether the record is up to date: what it read is as it saw it, without computing this
    /// state (the derived states it read are brought up to date, which may send, and so may
    /// recompute this one: then it looks again).
    fn is_valid(&self) -> bool {
        loop {
            let (mark, isolated) = crate::snapshots::valid_mark_and_isolated();
            let view = if isolated { crate::snapshots::view() } else { None };
            let dependencies = self.with_record(view.as_ref(), |record| {
                if record.result.is_none() {
                    return Err(false);
                }
                if record.valid_at.get() == mark {
                    return Err(true);
                }
                Ok(record.dependencies.clone())
            });
            let dependencies = match dependencies {
                Ok(dependencies) => dependencies,
                Err(valid) => return valid,
            };
            let calculations = self.calculations.get();
            let same = dependencies.iter().all(|&(dependency, seen)| {
                crate::state::dependency_unchanged(dependency, seen)
                    && self.calculations.get() == calculations
            });
            if self.calculations.get() != calculations {
                continue;
            }
            if same {
                self.with_record(view.as_ref(), |record| record.valid_at.set(mark_after(mark)));
            }
            return same;
        }
    }

    /// Brings the record up to date; returns whether that recomputed it.
    fn current(&self) -> bool {
        let valid = self.is_valid();
        if !valid {
            self.recompute();
        }
        if self.in_other_snapshots.get() {
            self.tell_of_another_record(valid);
        }
        !valid
    }

    /// In the global snapshot: its readers may have read another record (no recomputation tells
    /// them of a record an applied snapshot made).
    #[cold]
    fn tell_of_another_record(&self, valid: bool) {
        let view = crate::snapshots::view();
        if view.as_ref().is_none_or(|view| view.global) {
            let since = self.with_record(view.as_ref(), |record| record.value_since);
            if self.global_saw.replace(since) != since && valid {
                let id = self.id.get().expect("an allocated derived state");
                crate::state::invalidate_result_readers(id);
            }
        }
    }

    fn recompute(&self) {
        /// Ends a calculation, on an unwind too.
        struct Calculation<'a>(&'a Cell<bool>);
        impl Drop for Calculation<'_> {
            fn drop(&mut self) {
                self.0.set(false);
                crate::snapshots::end_calculation();
            }
        }
        if self.calculating.replace(true) {
            panic!("A derived state calculation cannot read itself");
        }
        let id = self.id.get().expect("an allocated derived state");
        let view = crate::snapshots::view();
        // What the global snapshot's calculation read stays what a send walks through, whatever
        // another snapshot's read (S5's addition to D759).
        let key = match &view {
            Some(view) if !view.global => view.id,
            _ => 0,
        };
        crate::snapshots::begin_calculation();
        let calculation = Calculation(&self.calculating);
        let value = self.observer.observe_exclusive(key, || {
            crate::snapshots::observe_calculation();
            (self.calculation)()
        });
        if key != 0 {
            self.side_keys.borrow_mut().push(key);
            self.in_other_snapshots.set(true);
        } else if view.is_none() && self.in_other_snapshots.replace(false) {
            // One record now, this calculation's.
            for key in self.side_keys.borrow_mut().drain(..) {
                self.observer.forget(key);
            }
        }
        let read = crate::snapshots::calculation_reads();
        drop(calculation);
        let mut seen: Vec<(StateId, u32)> = Vec::with_capacity(read.len());
        for &dependency in &read {
            if let Some(version) = crate::state::current_version(dependency) {
                seen.push((dependency, version));
            }
        }
        crate::snapshots::return_calculation_reads(read);
        self.calculations.set(self.calculations.get().wrapping_add(1));
        // As Compose's: a policy's `equivalent(new, old)` keeps the record; else a new one, and
        // readers are told if they'd see another value (`isInvalidFor`). User code: no borrow.
        // Isolation in use before stays so; a snapshot taken by the calculation itself is gone.
        let view = if view.is_some() { crate::snapshots::view() } else { None };
        let old = self.with_record(view.as_ref(), |record| record.result.clone());
        let kept = match (&old, self.policy) {
            (Some(old), Some(policy)) => policy(&value, old),
            _ => false,
        };
        let differs = !kept && old.as_ref().is_some_and(|old| !(self.equivalent)(&value, old));
        let first = old.is_none();
        // Only the global snapshot's readers are registered; a snapshot's own record tells none.
        let told = differs && view.as_ref().is_none_or(|view| view.global);
        // The record's `Rc` alone holds it now, to write in place.
        drop(old);
        let mut replaced = None;
        let mut pushed = false;
        {
            let mut records = self.records.borrow_mut();
            // A record of another snapshot (or one seen by others) stays, as Compose's
            // `newWritableRecord`; one of this snapshot is its own.
            let value_since = match &view {
                None => records.head.data.value_since,
                Some(view) => records.readable_in(view).data.value_since,
            };
            let index = match &view {
                None => 0,
                Some(view) => {
                    let Some(index) = records.index_in(view) else {
                        crate::snapshots::read_error()
                    };
                    if kept || records.at(index).id == view.id {
                        index
                    } else {
                        pushed = true;
                        let (id, version) = crate::snapshots::next_record();
                        records.push(
                            id,
                            version,
                            ResultRecord {
                                result: None,
                                valid_at: Cell::new((0, 0)),
                                dependencies: Rc::from([]),
                                value_since,
                            },
                        )
                    }
                }
            };
            let record = records.at_mut(index);
            // In place unless a validation is going through the list.
            let list = &mut record.data.dependencies;
            match Rc::get_mut(list) {
                Some(list) if list.len() == seen.len() => list.copy_from_slice(&seen),
                _ => *list = Rc::from(seen),
            }
            if !kept {
                let (current, version) = crate::snapshots::next_record();
                record.id = current;
                record.version = version;
                record.data.value_since = if differs || first { version } else { value_since };
                // In place unless a `with` closure is looking at it.
                match record.data.result.as_mut().and_then(Rc::get_mut) {
                    Some(slot) => replaced = Some(std::mem::replace(slot, value)),
                    None => record.data.result = Some(Rc::new(value)),
                }
            }
        }
        if pushed {
            crate::snapshots::note_extra_record(id);
        }
        // What readers in the global snapshot go by, this one's now.
        if view.as_ref().is_none_or(|view| view.global) {
            let since = self.with_record(view.as_ref(), |record| record.value_since);
            self.global_saw.set(since);
        }
        let mark = crate::snapshots::valid_mark();
        // The new one in place: readers of the old one are told before any send below.
        if told {
            crate::state::invalidate_result_readers(id);
        }
        drop(replaced);
        // The outermost calculation: as Compose's, what it made is initialized, and what's
        // written so far is sent.
        if crate::snapshots::calculation_depth() == 0 {
            crate::snapshots::Snapshot::notify_objects_initialized();
        }
        // A notify_objects_initialized in a mutable snapshot gave it a new id: this view still reads
        // this record.
        self.with_record(view.as_ref(), |record| record.valid_at.set(mark_after(mark)));
    }
}

/// Reads derived state `id`: brought up to date, then the read registered, as any state's.
#[track_caller]
pub(crate) fn read<T: 'static, R>(id: StateId, f: impl FnOnce(&T) -> R) -> R {
    let at = Location::caller();
    // No write since it was last found valid: nothing to validate.
    let mut found_valid = true;
    let (cell, cached_observed) = match crate::state::read_if_valid(id, at) {
        Some(valid) => valid,
        None => {
            found_valid = false;
            let cell = crate::state::object_cell(id, at);
            let derived = DerivedCell::<T>::of(&cell);
            if derived.calculating.get() {
                panic!("A derived state calculation cannot read itself");
            }
            // Read observers hear it first, as Compose's value getter tells them.
            let observed = crate::snapshots::notify_read_if_observed(id);
            let recomputed = derived.current();
            // Registered after: what the recomputation invalidates read the old value.
            crate::state::register_read_of(id, at);
            (cell, observed && !recomputed)
        }
    };
    let derived = DerivedCell::<T>::of(&cell);
    // A cached result's reads are heard as its calculation's were (`forceDependencyReads`).
    if cached_observed {
        let mut dependencies = Vec::new();
        derived.collect_dependencies(&mut dependencies);
        for dependency in dependencies {
            crate::snapshots::notify_read(dependency);
        }
    }
    // Found valid with no snapshot to look at, and no user code since: the head.
    let view = if found_valid && !cached_observed { None } else { crate::snapshots::view() };
    let value = derived.with_record(view.as_ref(), |record| record.result.clone());
    f(&value.expect("a calculated derived state"))
}

/// For hosts, until they all rely on turns: what's written so far is sent, which validates the
/// derived states layout or drawing read.
pub fn flush_derived_states() {
    crate::snapshots::Snapshot::send_apply_notifications();
}

/// `<Not calculated>`-or-the-value for a derived state's `Display`, read without observing.
pub(crate) fn display<T: std::fmt::Debug + 'static>(
    cell: &Rc<dyn StateObjectCell>,
    formatter: &mut std::fmt::Formatter<'_>,
    (index, generation): (u32, u32),
) -> std::fmt::Result {
    let derived = DerivedCell::<T>::of(cell);
    let valid = derived.is_valid();
    let view = crate::snapshots::view();
    let result = derived.with_record(view.as_ref(), |record| record.result.clone());
    match result.as_ref().filter(|_| valid) {
        Some(value) => write!(formatter, "DerivedState(value={value:?})@{index}v{generation}"),
        None => write!(formatter, "DerivedState(value=<Not calculated>)@{index}v{generation}"),
    }
}

/// Owns a derived state: dropping it retires its cell, as an [`OwnedState`]'s.
/// `remember(|| derived_state_of(…))` keeps it in the slot and returns its `Copy` [`State`]
/// handle.
pub struct DerivedState<T: 'static> {
    owner: OwnedState<T>,
}

/// The equivalence a policy decides with, as a function pointer the cell keeps.
fn equivalent_of<T, P: MutationPolicy<T>>(_: &P) -> fn(&T, &T) -> bool {
    P::equivalent
}

#[track_caller]
fn new_derived<T: 'static>(
    policy: Option<fn(&T, &T) -> bool>,
    equivalent: fn(&T, &T) -> bool,
    calculation: Box<dyn Fn() -> T>,
) -> DerivedState<T> {
    let created_at = Location::caller();
    let (snapshot, born, version) = crate::snapshots::birth();
    // Nothing is computed until a read.
    let cell = Rc::new(DerivedCell {
        calculation,
        policy,
        equivalent,
        records: RefCell::new(Records::new(
            ResultRecord {
                result: None,
                valid_at: Cell::new((0, 0)),
                dependencies: Rc::from([]),
                value_since: version,
            },
            snapshot,
            born,
            version,
        )),
        calculating: Cell::new(false),
        calculations: Cell::new(0),
        observer: ReadObserver::new(|| {}),
        id: Cell::new(None),
        side_keys: RefCell::new(Vec::new()),
        global_saw: Cell::new(version),
        in_other_snapshots: Cell::new(false),
    });
    let erased: Rc<dyn StateObjectCell> = cell.clone();
    let owner = crate::state::owned_object(erased, created_at);
    let id = owner.handle().id();
    cell.id.set(Some(id));
    cell.observer.set_dirty_cell(id);
    DerivedState { owner }
}

/// A state whose value is `calculation`'s, recomputed when the state it read changes, as
/// Compose's `derivedStateOf`. Readers are invalidated only when the value changes (`==`).
#[track_caller]
pub fn derived_state_of<T: PartialEq + 'static>(
    calculation: impl Fn() -> T + 'static,
) -> DerivedState<T> {
    new_derived(None, <T as PartialEq>::eq, Box::new(calculation))
}

/// Like [`derived_state_of`], with a mutation policy deciding when the value changed, as
/// `derivedStateOf(policy, calculation)`: a result it finds equivalent also leaves what reads this
/// state uncomputed.
#[track_caller]
pub fn derived_state_of_with<T: 'static, P: MutationPolicy<T>>(
    policy: P,
    calculation: impl Fn() -> T + 'static,
) -> DerivedState<T> {
    let equivalent = equivalent_of(&policy);
    new_derived(Some(equivalent), equivalent, Box::new(calculation))
}

impl<T: 'static> DerivedState<T> {
    /// The value, recomputed first if a dependency changed. Reading it registers the reader, as
    /// reading any state does.
    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.as_state().get()
    }

    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.as_state().with(f)
    }

    /// The `Copy` read-only handle of the value: reads through it are fresh too.
    pub fn as_state(&self) -> State<T> {
        State::of_derived(self.owner.handle())
    }
}

impl<T: 'static> Remember for DerivedState<T> {
    type Stored = DerivedState<T>;
    type Output = State<T>;

    fn into_stored(self) -> DerivedState<T> {
        self
    }

    fn output(stored: &DerivedState<T>) -> State<T> {
        stored.as_state()
    }
}

impl<T: 'static> std::fmt::Debug for DerivedState<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "DerivedState({:?})", self.as_state())
    }
}
