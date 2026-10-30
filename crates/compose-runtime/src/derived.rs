//! `derived_state_of`: a state computed from other state, as Compose's `derivedStateOf`
//! (docs/parity/design/snapshots.md §3.10).
//!
//! It's a state object of its own: its cell keeps a [`ResultRecord`] in records, the result with
//! the states the calculation read and the versions it saw of them. Nothing happens at a write:
//! a read validates the record, by the snapshot and the write counter when no write came since,
//! else by comparing what it read with the versions it saw (derived states among them first
//! brought up to date), and recomputes only if one differs, so no read is ever behind what it
//! derives from, and a result found equivalent by its policy keeps its version and stops the
//! chain. It's lazy (D711): nothing computes it but a read, and a send telling of a change what
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
use crate::snapshots::{Records, SnapshotId, StateObjectCell};
use crate::state::{MutationPolicy, OwnedState, State, StateId, StructuralEqualityPolicy};

/// What a derived state's calculation gave, in a snapshot, as Compose's `ResultRecord`.
struct ResultRecord<T> {
    /// `None` until the first calculation.
    result: Option<T>,
    /// The global snapshot and the write counter it was last found valid at: no write since, it
    /// still is.
    valid_at: Cell<(SnapshotId, u32)>,
    /// What the calculation read directly, with the version of the record each had. Versions
    /// are distinct, but only their equality means anything.
    dependencies: Vec<(StateId, u32)>,
}

/// A derived state's cell in the arena, as Compose's `DerivedSnapshotState`.
pub(crate) struct DerivedCell<T: 'static> {
    calculation: Box<dyn Fn() -> T>,
    equivalent: fn(&T, &T) -> bool,
    records: RefCell<Records<ResultRecord<T>>>,
    /// Its calculation is running: a read of it is an error.
    calculating: Cell<bool>,
    /// Records what the calculation read in those states' cells, by which a send finds the
    /// derived states reading what changed.
    observer: ReadObserver,
    /// Its id, once allocated.
    id: Cell<Option<StateId>>,
}

impl<T: 'static> StateObjectCell for DerivedCell<T> {
    fn record_count(&self) -> usize {
        self.records.try_borrow().map_or(1, |records| records.len())
    }

    fn wrote_in(&self, _id: SnapshotId, _version: u32) -> bool {
        false
    }

    fn current_version(&self) -> Option<u32> {
        self.current();
        self.records.try_borrow().ok().map(|records| records.version())
    }

    #[inline]
    fn valid_at(&self, mark: (SnapshotId, u32)) -> bool {
        self.records.try_borrow().is_ok_and(|records| {
            let record = &records.head.data;
            record.result.is_some() && record.valid_at.get() == mark
        })
    }

    fn notify_dependencies(&self) {
        let Ok(records) = self.records.try_borrow() else { return };
        for &(dependency, _) in &records.head.data.dependencies {
            crate::snapshots::notify_read(dependency);
            crate::state::notify_dependencies(dependency);
        }
    }
}

impl<T: 'static> DerivedCell<T> {
    fn of(cell: &Rc<dyn StateObjectCell>) -> &DerivedCell<T> {
        let any: &dyn Any = &**cell;
        any.downcast_ref().expect("compose-runtime: state handle of the wrong type")
    }

    /// Whether the record is up to date: what it read is as it saw it, without computing this
    /// state (the derived states it read are brought up to date).
    fn is_valid(&self) -> bool {
        let Ok(records) = self.records.try_borrow() else { return true };
        let record = &records.head.data;
        if record.result.is_none() {
            return false;
        }
        if record.valid_at.get() == crate::snapshots::valid_mark() {
            return true;
        }
        let same = record
            .dependencies
            .iter()
            .all(|&(dependency, seen)| crate::state::current_version(dependency) == Some(seen));
        if same {
            // The derived states it read may have moved the mark.
            record.valid_at.set(crate::snapshots::valid_mark());
        }
        same
    }

    /// Brings the record up to date; returns whether that recomputed it.
    fn current(&self) -> bool {
        if self.is_valid() {
            return false;
        }
        self.recompute();
        true
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
        crate::snapshots::begin_calculation();
        let calculation = Calculation(&self.calculating);
        let value = self.observer.observe_exclusive(0, &self.calculation);
        let read = crate::snapshots::calculation_reads();
        drop(calculation);
        let dependencies: Vec<(StateId, u32)> = read
            .into_iter()
            .filter_map(|dependency| {
                crate::state::current_version(dependency).map(|version| (dependency, version))
            })
            .collect();
        let (changed, old) = {
            let mut records = self.records.try_borrow_mut().unwrap_or_else(|_| {
                panic!(
                    "compose-runtime: a derived state's result is borrowed by `with` while what \
                     it reads changed"
                )
            });
            let (global, version) = crate::snapshots::next_record();
            let head = &mut records.head;
            head.data.dependencies = dependencies;
            match &head.data.result {
                Some(old) if (self.equivalent)(old, &value) => (false, Some(value)),
                previous => {
                    let first = previous.is_none();
                    head.id = global;
                    head.version = version;
                    (!first, head.data.result.replace(value))
                }
            }
        };
        // The new one in place: readers of the old one are told before any send below.
        if changed {
            crate::state::invalidate_result_readers(id);
        }
        drop(old);
        // The outermost calculation: as Compose's, what it made is initialized, and what's
        // written so far is sent.
        if crate::snapshots::calculation_depth() == 0 {
            crate::snapshots::Snapshot::notify_objects_initialized();
        }
        if let Ok(records) = self.records.try_borrow() {
            records.head.data.valid_at.set(crate::snapshots::valid_mark());
        }
    }
}

/// Reads derived state `id`: brought up to date, then the read registered, as any state's.
#[track_caller]
pub(crate) fn read<T: 'static, R>(id: StateId, f: impl FnOnce(&T) -> R) -> R {
    let at = Location::caller();
    // No write since it was last found valid: nothing to validate.
    let (cell, observed) = match crate::state::read_if_valid(id, at) {
        Some(valid) => valid,
        None => {
            let cell = crate::state::object_cell(id, at);
            let derived = DerivedCell::<T>::of(&cell);
            if derived.calculating.get() {
                panic!("A derived state calculation cannot read itself");
            }
            let recomputed = derived.current();
            let observed = crate::state::register_read_of(id, at);
            (cell, observed && !recomputed)
        }
    };
    let derived = DerivedCell::<T>::of(&cell);
    // A cached result's reads are heard as its calculation's were (`forceDependencyReads`).
    if observed {
        derived.notify_dependencies();
    }
    let records = derived.records.borrow();
    f(records.head.data.result.as_ref().expect("a calculated derived state"))
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
    let records = derived.records.borrow();
    match records.head.data.result.as_ref().filter(|_| valid) {
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

/// A state whose value is `calculation`'s, recomputed when the state it read changes, as
/// Compose's `derivedStateOf`. Readers are invalidated only when the value changes (`==`).
#[track_caller]
pub fn derived_state_of<T: PartialEq + 'static>(
    calculation: impl Fn() -> T + 'static,
) -> DerivedState<T> {
    derived_state_of_with(StructuralEqualityPolicy, calculation)
}

/// Like [`derived_state_of`], with a mutation policy deciding when the value changed, as
/// `derivedStateOf(policy, calculation)`.
#[track_caller]
pub fn derived_state_of_with<T: 'static, P: MutationPolicy<T>>(
    policy: P,
    calculation: impl Fn() -> T + 'static,
) -> DerivedState<T> {
    let created_at = Location::caller();
    let (snapshot, version) = crate::snapshots::next_record();
    // Nothing is computed until a read.
    let cell = Rc::new(DerivedCell {
        calculation: Box::new(calculation),
        equivalent: equivalent_of(&policy),
        records: RefCell::new(Records::new(
            ResultRecord { result: None, valid_at: Cell::new((0, 0)), dependencies: Vec::new() },
            snapshot,
            version,
        )),
        calculating: Cell::new(false),
        observer: ReadObserver::new(|| {}),
        id: Cell::new(None),
    });
    let erased: Rc<dyn StateObjectCell> = cell.clone();
    let owner = crate::state::owned_object(erased, created_at);
    let id = owner.handle().id();
    cell.id.set(Some(id));
    cell.observer.set_dirty_cell(id);
    DerivedState { owner }
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
