//! Snapshots, as Compose's `Snapshot`, `MutableSnapshot` and their nested kinds: isolated views of
//! the state, read-only or to apply. Compose's algorithms (`Snapshot.kt`), one thread's: no locks,
//! and the user code they run (observers, policies, values' `Drop`) outside the runtime's borrow
//! (docs/parity/design/snapshots.md §3.5, §3.6).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use super::SnapshotId;
use super::global::{
    Applied, ChangeSet, ObserverFrame, ObserverHandle, OuterPass, StateObserverFn, merged_with,
    send,
};
use super::id_set::IdSet;
use super::records::{ApplyViews, Merged, View};
use super::tooling;
use crate::observe::Observing;
use crate::runtime::{Runtime, try_with_runtime, with_runtime};
use crate::snapshots::{Snapshots, StateObjectId};
use crate::state::StateId;

pub(crate) type SnapshotRef = Rc<SnapshotInner>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    ReadOnly,
    Mutable,
    NestedReadOnly,
    NestedMutable,
}

impl Kind {
    fn read_only(self) -> bool {
        matches!(self, Kind::ReadOnly | Kind::NestedReadOnly)
    }
}

/// What records reads when a snapshot is taken besides its observers: composition, and an
/// observation in progress. Compose merges them as read observers; here they're the runtime's
/// (`read_cell_in`), so a snapshot entered where it was taken lets them go on recording, and one
/// entered elsewhere hides them, as its observers would be another's.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ReadContext {
    scope: bool,
    /// The observer (not its key: what's read is recorded under the scope in progress).
    observing: Option<u32>,
}

impl ReadContext {
    const NONE: ReadContext = ReadContext { scope: false, observing: None };

    fn of(runtime: &Runtime) -> ReadContext {
        let snapshots = &runtime.snapshots;
        let visible = |observing: &Observing| snapshots.observes_at(observing.depth);
        ReadContext {
            scope: !runtime.reading_scope().is_none() && snapshots.composition_reads(),
            observing: runtime.observing.filter(visible).map(|observing| observing.observer),
        }
    }
}

/// A snapshot other than the global one, as Compose's `Snapshot` subclasses.
pub(crate) struct SnapshotInner {
    id: Cell<SnapshotId>,
    invalid: RefCell<IdSet>,
    kind: Kind,
    parent: Option<SnapshotRef>,
    /// The observers given, merged with those of what it was taken from.
    read_observer: Option<StateObserverFn>,
    write_observer: Option<StateObserverFn>,
    context: ReadContext,
    /// The states it wrote, in write order (a state can appear twice if the snapshot advanced).
    modified: RefCell<Vec<StateId>>,
    /// The ids it had before, and those of nested snapshots applied to it.
    previous_ids: RefCell<IdSet>,
    pin: Cell<Option<SnapshotId>>,
    previous_pins: RefCell<Vec<SnapshotId>>,
    /// Itself and the nested snapshots still active, as `snapshots`.
    nested: Cell<u32>,
    /// How many read-only snapshots share its id: nested in a read-only one, it's the same view,
    /// and the id stays open until the last goes.
    sharers: Rc<Cell<u32>>,
    applied: Cell<bool>,
    disposed: Cell<bool>,
    deactivated: Cell<bool>,
    write_count: Cell<u32>,
}

impl SnapshotInner {
    /// Registers a new snapshot: its pin, as `trackPinning`.
    #[allow(clippy::too_many_arguments)]
    fn new(
        snapshots: &mut Snapshots,
        id: SnapshotId,
        invalid: IdSet,
        kind: Kind,
        parent: Option<SnapshotRef>,
        read_observer: Option<StateObserverFn>,
        write_observer: Option<StateObserverFn>,
        context: ReadContext,
        sharers: Rc<Cell<u32>>,
    ) -> SnapshotRef {
        let pin = invalid.lowest(id);
        snapshots.pin(pin);
        Rc::new(SnapshotInner {
            id: Cell::new(id),
            invalid: RefCell::new(invalid),
            kind,
            parent,
            read_observer,
            write_observer,
            context,
            modified: RefCell::new(Vec::new()),
            previous_ids: RefCell::new(IdSet::EMPTY),
            pin: Cell::new(Some(pin)),
            previous_pins: RefCell::new(Vec::new()),
            nested: Cell::new(1),
            sharers,
            applied: Cell::new(false),
            disposed: Cell::new(false),
            deactivated: Cell::new(false),
            write_count: Cell::new(0),
        })
    }

    #[inline]
    pub(crate) fn id(&self) -> SnapshotId {
        self.id.get()
    }

    #[inline]
    pub(crate) fn invalid(&self) -> IdSet {
        self.invalid.borrow().clone()
    }

    /// Whether this is `other` or nested in it.
    pub(crate) fn is_or_nested_in(self: &Rc<Self>, other: &SnapshotRef) -> bool {
        let mut snapshot = self;
        loop {
            if Rc::ptr_eq(snapshot, other) {
                return true;
            }
            match &snapshot.parent {
                Some(parent) => snapshot = parent,
                None => return false,
            }
        }
    }

    /// A write through this snapshot ended.
    pub(crate) fn record_write(&self, id: StateId, modified: bool) {
        self.write_count.set(self.write_count.get().wrapping_add(1));
        if modified {
            self.modified.borrow_mut().push(id);
        }
    }

    fn record_previous(&self, id: SnapshotId) {
        let previous = self.previous_ids.borrow().set(id);
        *self.previous_ids.borrow_mut() = previous;
    }

    fn add_invalid(&self, from: SnapshotId, until: SnapshotId) {
        let invalid = self.invalid.borrow().add_range(from, until);
        *self.invalid.borrow_mut() = invalid;
    }

    /// Takes the next id, the one it had staying readable, as `advance`'s tail.
    fn take_new_id(&self, snapshots: &mut Snapshots) -> SnapshotId {
        let previous = self.id.get();
        let id = snapshots.take_id();
        self.id.set(id);
        snapshots.open_id(id);
        previous
    }

    /// Makes this finished pass's snapshot ready for the next pass.
    fn reuse(&self, snapshots: &mut Snapshots, id: SnapshotId, invalid: IdSet) {
        let pin = invalid.lowest(id);
        snapshots.pin(pin);
        self.id.set(id);
        *self.invalid.borrow_mut() = invalid;
        *self.previous_ids.borrow_mut() = IdSet::EMPTY;
        self.pin.set(Some(pin));
        self.modified.borrow_mut().clear();
        self.applied.set(false);
        self.disposed.set(false);
        self.write_count.set(0);
    }

    fn is_active(&self) -> bool {
        !self.applied.get() && !self.disposed.get()
    }

    /// As Compose's `advance()`: new writes get a new id, so what's written so far is older.
    fn advance(&self, snapshots: &mut Snapshots) {
        self.record_previous(self.id.get());
        if self.is_active() {
            let previous = self.take_new_id(snapshots);
            self.add_invalid(previous + 1, self.id.get());
        }
    }

    /// As `advanceParentsLocked`.
    fn advance_parents(&self, snapshots: &mut Snapshots) {
        let mut parent = self.parent.as_ref();
        while let Some(snapshot) = parent {
            if !snapshot.kind.read_only() && snapshot.is_active() {
                snapshot.record_previous(snapshot.id.get());
                let previous = snapshot.take_new_id(snapshots);
                snapshot.add_invalid(previous + 1, snapshot.id.get());
            }
            parent = snapshot.parent.as_ref();
        }
        snapshots.advance_global_silently();
    }

    /// As `closeLocked`.
    fn close(&self, snapshots: &mut Snapshots) {
        snapshots.close_ids(self.id.get(), &self.previous_ids.borrow());
    }

    /// As `releasePinnedSnapshotsForCloseLocked`.
    fn release_pins(&self, snapshots: &mut Snapshots) {
        if let Some(pin) = self.pin.take() {
            snapshots.unpin(pin);
        }
        for pin in self.previous_pins.borrow_mut().drain(..) {
            snapshots.unpin(pin);
        }
    }

    fn validate_not_disposed(&self) {
        assert!(!self.disposed.get(), "Cannot use a disposed snapshot");
    }

    fn validate_not_applied_or_pinned(&self) {
        assert!(
            !self.applied.get() || self.pin.get().is_some(),
            "Unsupported operation on a disposed or applied snapshot"
        );
    }

    /// As `validateOpen`.
    fn validate_open(&self, snapshots: &Snapshots) {
        if !snapshots.open().get(self.id.get()) {
            let applied = if self.kind.read_only() {
                "read-only".to_string()
            } else {
                self.applied.get().to_string()
            };
            panic!(
                "Snapshot is not open: snapshotId={}, disposed={}, applied={applied}, lowestPin={:?}",
                self.id.get(),
                self.disposed.get(),
                snapshots.lowest_pin()
            );
        }
    }
}

/// `nestedDeactivated`: a snapshot is abandoned (a mutable one not applied) or closed (a read-only
/// one) once it and its nested ones are gone.
fn nested_deactivated(inner: &SnapshotRef) {
    let left = inner.nested.get().saturating_sub(1);
    inner.nested.set(left);
    if left != 0 {
        return;
    }
    match inner.kind {
        Kind::ReadOnly => with_runtime(|runtime| {
            inner.close(&mut runtime.snapshots);
            inner.release_pins(&mut runtime.snapshots);
        }),
        Kind::Mutable | Kind::NestedMutable => {
            if !inner.applied.get() {
                abandon(inner);
            }
            if inner.kind == Kind::NestedMutable {
                deactivate(inner);
            }
        }
        Kind::NestedReadOnly => {}
    }
}

/// A nested mutable snapshot no longer counts for its parent.
fn deactivate(inner: &SnapshotRef) {
    if !inner.deactivated.replace(true)
        && let Some(parent) = &inner.parent
    {
        nested_deactivated(parent);
    }
}

/// As `abandon`: what it wrote is gone for everyone.
fn abandon(inner: &SnapshotRef) {
    let modified = std::mem::take(&mut *inner.modified.borrow_mut());
    let garbage = with_runtime(|runtime| {
        let mut garbage = Vec::new();
        let id = inner.id.get();
        let previous = inner.previous_ids.borrow();
        for &state in &modified {
            runtime.cells.abandon(state, id, &previous, &mut garbage);
        }
        inner.close(&mut runtime.snapshots);
        inner.release_pins(&mut runtime.snapshots);
        garbage
    });
    // The values' `Drop`: outside the borrow.
    drop(garbage);
}

fn dispose(inner: &SnapshotRef) {
    if inner.disposed.get() {
        return;
    }
    match inner.kind {
        Kind::ReadOnly => {
            inner.sharers.set(inner.sharers.get().saturating_sub(1));
            nested_deactivated(inner);
            inner.disposed.set(true);
            with_runtime(|runtime| inner.release_pins(&mut runtime.snapshots));
        }
        Kind::NestedReadOnly => {
            let parent = inner.parent.as_ref().expect("a nested snapshot's parent");
            inner.sharers.set(inner.sharers.get().saturating_sub(1));
            if inner.id.get() != parent.id.get() && inner.sharers.get() == 0 {
                with_runtime(|runtime| {
                    inner.close(&mut runtime.snapshots);
                    inner.release_pins(&mut runtime.snapshots);
                });
            }
            nested_deactivated(parent);
            inner.disposed.set(true);
            with_runtime(|runtime| inner.release_pins(&mut runtime.snapshots));
        }
        Kind::Mutable => {
            inner.disposed.set(true);
            nested_deactivated(inner);
            // An apply whose observer panicked didn't get to it.
            if inner.applied.get() {
                with_runtime(|runtime| inner.release_pins(&mut runtime.snapshots));
            }
        }
        Kind::NestedMutable => {
            inner.disposed.set(true);
            nested_deactivated(inner);
            deactivate(inner);
        }
    }
    super::global::reclaim_records();
    if with_runtime(|runtime| runtime.snapshots.has_snapshot_observers()) {
        tooling::pre_dispose(&Snapshot::handle_of(inner.clone()));
    }
}

/// As `innerApplyLocked`: decides each modified state against `current`, what the snapshot applies
/// to. False on a conflict (nothing has changed then, and the snapshot stays open).
fn inner_apply(inner: &SnapshotRef, current: View, modified: &mut Vec<StateId>) -> bool {
    let id = inner.id.get();
    let invalid = inner.invalid();
    let start = invalid.set(id).or(&inner.previous_ids.borrow());
    let views = ApplyViews { start: View::of(id, start), current, applied: View::of(id, invalid) };
    let mut merged = Vec::new();
    let mut stands = Vec::new();
    for &state in modified.iter() {
        let Some(cell) = crate::state::state_object(state) else { continue };
        // The policy: user code, outside the borrow.
        match cell.merge_at_apply(&views) {
            Merged::Nothing => {}
            Merged::Current => stands.push((state, cell)),
            Merged::New(record) => merged.push((state, cell, record)),
            Merged::Fail => return false,
        }
    }
    // Nothing failed: only now does anything change.
    let alone = inner.nested.get() == 1;
    let mut garbage = Vec::new();
    {
        let previous = inner.previous_ids.borrow();
        for (_, cell) in &stands {
            cell.current_stands(&views, alone, &previous, &mut garbage);
        }
    }
    // The values' `Drop`: user code, with nothing borrowed.
    drop(garbage);
    if !merged.is_empty() {
        with_runtime(|runtime| inner.advance(&mut runtime.snapshots));
        let id = inner.id.get();
        for (state, cell, record) in merged {
            let version = with_runtime(|runtime| runtime.snapshots.next_version());
            cell.prepend_merged(record, id, version);
            super::global::note_extra_record(state);
        }
    }
    if !stands.is_empty() {
        modified.retain(|state| !stands.iter().any(|(stood, _)| stood == state));
    }
    true
}

/// Each state once, in the order of the first write.
fn dedupe(ids: &mut Vec<StateId>) {
    if ids.len() > 16 {
        let mut seen = std::collections::HashSet::with_capacity(ids.len());
        ids.retain(|&id| seen.insert(id));
    } else {
        let mut kept = 0;
        for index in 0..ids.len() {
            if !ids[..kept].contains(&ids[index]) {
                ids[kept] = ids[index];
                kept += 1;
            }
        }
        ids.truncate(kept);
    }
}

/// As `MutableSnapshot.apply`.
fn apply_root(inner: &SnapshotRef, handle: &Snapshot) -> SnapshotApplyResult {
    let current = with_runtime(|runtime| {
        let snapshots = &mut runtime.snapshots;
        inner.validate_open(snapshots);
        // In a pass another snapshot's records become readable: the pass must look at its apply.
        snapshots.note_not_alone();
        View::of(snapshots.next_id(), snapshots.open().clone())
    });
    // A copy: a policy that panics leaves the snapshot as it was, to abandon. Each state once: one
    // written on both sides of an advance is merged once.
    let mut modified = inner.modified.borrow().clone();
    dedupe(&mut modified);
    if !modified.is_empty() && !inner_apply(inner, current, &mut modified) {
        return SnapshotApplyResult::Failure(handle.handle());
    }
    inner.modified.borrow_mut().clear();
    let (observed, tell_tools) = with_runtime(|runtime| {
        inner.close(&mut runtime.snapshots);
        let tools = runtime.snapshots.has_snapshot_observers();
        (if tools { modified.clone() } else { Vec::new() }, tools)
    });
    inner.applied.set(true);
    // A new global snapshot, which sees this one, and what changed is told.
    super::global::send_with(true, Some(Applied { states: modified, snapshot: handle }));
    if tell_tools {
        tooling::applied(handle, &ChangeSet::of(observed));
    }
    // Pinned until the observers have run: they may take nested snapshots of it.
    with_runtime(|runtime| inner.release_pins(&mut runtime.snapshots));
    super::global::reclaim_records();
    SnapshotApplyResult::Success
}

/// As `NestedMutableSnapshot.apply`.
fn apply_nested(inner: &SnapshotRef, handle: &Snapshot) -> SnapshotApplyResult {
    let parent = inner.parent.as_ref().expect("a nested snapshot's parent");
    if !parent.is_active() {
        return SnapshotApplyResult::Failure(handle.handle());
    }
    with_runtime(|runtime| inner.validate_open(&runtime.snapshots));
    // A copy: a policy that panics leaves the snapshot as it was, to abandon. Each state once.
    let mut modified = inner.modified.borrow().clone();
    dedupe(&mut modified);
    if modified.is_empty() {
        with_runtime(|runtime| {
            inner.close(&mut runtime.snapshots);
            inner.release_pins(&mut runtime.snapshots);
        });
    } else {
        let current = View::of(parent.id.get(), parent.invalid());
        if !inner_apply(inner, current, &mut modified) {
            return SnapshotApplyResult::Failure(handle.handle());
        }
        inner.modified.borrow_mut().clear();
        parent.modified.borrow_mut().extend_from_slice(&modified);
    }
    with_runtime(|runtime| {
        let snapshots = &mut runtime.snapshots;
        // What the parent validated may read other records now, though nothing was written just
        // now: a derived state's mark must not stand (D782).
        snapshots.next_version();
        let id = inner.id.get();
        if parent.id.get() < id {
            parent.advance(snapshots);
        }
        let previous = inner.previous_ids.borrow();
        let invalid = parent.invalid.borrow().clear(id).and_not(&previous);
        *parent.invalid.borrow_mut() = invalid;
        parent.record_previous(id);
        if let Some(pin) = inner.pin.take() {
            parent.previous_pins.borrow_mut().push(pin);
        }
        let all_previous = parent.previous_ids.borrow().or(&previous);
        *parent.previous_ids.borrow_mut() = all_previous;
        parent.previous_pins.borrow_mut().append(&mut inner.previous_pins.borrow_mut());
    });
    inner.applied.set(true);
    deactivate(inner);
    if with_runtime(|runtime| runtime.snapshots.has_snapshot_observers()) {
        tooling::applied(handle, &ChangeSet::of(modified));
    }
    SnapshotApplyResult::Success
}

/// A snapshot, as Compose's `Snapshot`: a view of the state. What
/// [`take_snapshot`](Snapshot::take_snapshot) gives owns it, and dropping it disposes it; the
/// handles [`current`](Snapshot::current), [`handle`](Snapshot::handle) and an apply observer's
/// argument don't.
pub struct Snapshot {
    /// `None` is the global snapshot.
    inner: Option<SnapshotRef>,
    owner: bool,
    /// [`current`](Snapshot::current)'s: what it's asked goes by what's in effect now, the
    /// observers of `observe` too (Compose's transparent snapshots).
    transparent: bool,
    /// [`with_read_only`](Snapshot::with_read_only) was in effect.
    refuses_writes: bool,
}

impl Snapshot {
    fn owning(inner: SnapshotRef) -> Snapshot {
        Snapshot { inner: Some(inner), owner: true, transparent: false, refuses_writes: false }
    }

    /// The global snapshot, as the `GlobalSnapshot` an apply observer may be given.
    pub fn global_snapshot() -> Snapshot {
        Snapshot::global_handle()
    }

    fn handle_of(inner: SnapshotRef) -> Snapshot {
        Snapshot { inner: Some(inner), owner: false, transparent: false, refuses_writes: false }
    }

    pub(super) fn global_handle() -> Snapshot {
        Snapshot { inner: None, owner: false, transparent: false, refuses_writes: false }
    }

    /// The snapshot code runs in, as `Snapshot.current`.
    pub fn current() -> Snapshot {
        with_runtime(|runtime| {
            let snapshots = &runtime.snapshots;
            Snapshot {
                inner: snapshots.current().cloned(),
                owner: false,
                transparent: true,
                refuses_writes: snapshots.read_only(),
            }
        })
    }

    /// Whether code runs in a snapshot other than the plain global one, as `isInSnapshot`.
    pub fn is_in_snapshot() -> bool {
        with_runtime(|runtime| runtime.snapshots.is_in_snapshot())
    }

    /// A handle of this snapshot that doesn't own it, to keep.
    pub fn handle(&self) -> Snapshot {
        Snapshot {
            inner: self.inner.clone(),
            owner: false,
            transparent: self.transparent,
            refuses_writes: self.refuses_writes,
        }
    }

    /// The snapshot's id, as `snapshotId`: it changes as the snapshot advances.
    pub fn snapshot_id(&self) -> u64 {
        match &self.inner {
            Some(inner) => inner.id.get(),
            None => with_runtime(|runtime| runtime.snapshots.global()),
        }
    }

    /// Whether writes in it are refused, as `readOnly`.
    pub fn read_only(&self) -> bool {
        self.refuses_writes || self.inner.as_ref().is_some_and(|inner| inner.kind.read_only())
    }

    /// The snapshot this one was nested from, the first of them, as `root`.
    pub fn root(&self) -> Snapshot {
        let mut inner = self.inner.clone();
        while let Some(snapshot) = &inner {
            match (snapshot.kind, &snapshot.parent) {
                (Kind::NestedMutable | Kind::NestedReadOnly, Some(parent)) => {
                    inner = Some(parent.clone());
                }
                _ => break,
            }
        }
        Snapshot { inner, owner: false, transparent: false, refuses_writes: false }
    }

    /// Whether it wrote states it hasn't applied, as `hasPendingChanges`.
    pub fn has_pending_changes(&self) -> bool {
        match &self.inner {
            Some(inner) => !inner.modified.borrow().is_empty(),
            None => with_runtime(|runtime| runtime.snapshots.has_global_changes()),
        }
    }

    /// The writes made through it (the global snapshot's since it last advanced), as `writeCount`.
    #[doc(hidden)]
    pub fn write_count(&self) -> u32 {
        match &self.inner {
            Some(inner) => inner.write_count.get(),
            None => with_runtime(|runtime| runtime.snapshots.global_writes()),
        }
    }

    /// The states it wrote and hasn't applied, each once, as `modified`.
    pub fn modified(&self) -> Vec<StateObjectId> {
        let mut ids = match &self.inner {
            Some(inner) => inner.modified.borrow().clone(),
            None => with_runtime(|runtime| runtime.snapshots.global_modified()),
        };
        dedupe(&mut ids);
        ids.into_iter().map(StateObjectId).collect()
    }

    /// This snapshot as a mutable one, if it is one, as Kotlin's cast: a handle.
    pub fn as_mutable(&self) -> Option<MutableSnapshot> {
        (!self.read_only()).then(|| MutableSnapshot(self.handle()))
    }

    /// What a snapshot taken from this one starts from: its observers and context.
    fn base(
        &self,
        runtime: &Runtime,
    ) -> (Option<StateObserverFn>, Option<StateObserverFn>, ReadContext) {
        if self.transparent {
            let snapshots = &runtime.snapshots;
            return (
                snapshots.merged_read_observer(),
                snapshots.merged_write_observer(),
                ReadContext::of(runtime),
            );
        }
        match &self.inner {
            Some(inner) => {
                (inner.read_observer.clone(), inner.write_observer.clone(), inner.context)
            }
            None => (None, None, ReadContext::NONE),
        }
    }

    /// A read-only snapshot of the state as it is, as `Snapshot.takeSnapshot`: its
    /// reads call `read_observer` and it never sees later writes.
    pub fn take_snapshot(read_observer: Option<StateObserverFn>) -> Snapshot {
        Snapshot::current().take_nested_snapshot(read_observer)
    }

    /// A mutable snapshot, as `Snapshot.takeMutableSnapshot`: what's written in it stays in it
    /// until [`apply`](MutableSnapshot::apply). Panics in a read-only snapshot, as Compose's.
    pub fn take_mutable_snapshot(
        read_observer: Option<StateObserverFn>,
        write_observer: Option<StateObserverFn>,
    ) -> MutableSnapshot {
        let current = Snapshot::current();
        match current.as_mutable() {
            Some(current) => current.take_nested_mutable_snapshot(read_observer, write_observer),
            None => panic!("Cannot create a mutable snapshot of a read-only snapshot"),
        }
    }

    /// A read-only snapshot of what this one sees now, as `takeNestedSnapshot`. Its reads call
    /// `read_observer`, then this snapshot's read observer.
    pub fn take_nested_snapshot(&self, read_observer: Option<StateObserverFn>) -> Snapshot {
        // First, as Compose's: a snapshot refused isn't one tools hear of.
        if let Some(parent) = &self.inner {
            parent.validate_not_disposed();
            if !parent.kind.read_only() {
                parent.validate_not_applied_or_pinned();
            }
        }
        let parent = self.inner.is_some().then_some(self);
        let (read_observer, _, creating) = tooling::pre_create(parent, true, read_observer, None);
        let snapshot = self.nested_snapshot(read_observer);
        tooling::created(&snapshot, parent, creating);
        snapshot
    }

    fn nested_snapshot(&self, read_observer: Option<StateObserverFn>) -> Snapshot {
        let Some(parent) = &self.inner else {
            return take_root(self, Kind::ReadOnly, read_observer, None);
        };
        let inner = with_runtime(|runtime| {
            let (base_read, _, context) = self.base(runtime);
            let read = merged_with(read_observer, base_read);
            let snapshots = &mut runtime.snapshots;
            if parent.kind.read_only() {
                // The same view; a nested one's parent is its own.
                let owner = match parent.kind {
                    Kind::ReadOnly => parent,
                    _ => parent.parent.as_ref().expect("a nested snapshot's parent"),
                };
                owner.nested.set(owner.nested.get() + 1);
                parent.sharers.set(parent.sharers.get() + 1);
                return SnapshotInner::new(
                    snapshots,
                    parent.id.get(),
                    parent.invalid(),
                    Kind::NestedReadOnly,
                    Some(owner.clone()),
                    read,
                    None,
                    context,
                    parent.sharers.clone(),
                );
            }
            let previous = parent.id.get();
            parent.record_previous(previous);
            let id = snapshots.take_id();
            snapshots.open_id(id);
            let inner = SnapshotInner::new(
                snapshots,
                id,
                parent.invalid().add_range(previous + 1, id),
                Kind::NestedReadOnly,
                Some(parent.clone()),
                read,
                None,
                context,
                Rc::new(Cell::new(1)),
            );
            parent.nested.set(parent.nested.get() + 1);
            if parent.is_active() {
                parent.take_new_id(snapshots);
                parent.add_invalid(previous + 1, parent.id.get());
            }
            inner
        });
        Snapshot::owning(inner)
    }

    /// Runs `block` in this snapshot, as `enter`: its reads and writes go through it, and its
    /// observers hear them.
    pub fn enter<R>(&self, block: impl FnOnce() -> R) -> R {
        let _frame = FrameGuard(self.push_entered());
        block()
    }

    /// Enters it; returns the frame depth before.
    fn push_entered(&self) -> u32 {
        with_runtime(|runtime| {
            let (read, write, context) = self.base(runtime);
            let barrier = ReadContext::of(runtime) != context;
            let read_only = self.read_only();
            let depth = runtime.snapshots.depth();
            let frame = ObserverFrame { read, write, barrier, entered: None };
            runtime.snapshots.enter(self.inner.clone(), read_only, frame);
            depth
        })
    }

    /// Makes this snapshot the current one until [`unsafe_leave`](Snapshot::unsafe_leave), as
    /// `unsafeEnter`, for code that can't wrap what it runs in [`enter`](Snapshot::enter). What's
    /// entered must be left in order.
    pub fn unsafe_enter(&self) -> Option<SnapshotToken> {
        let previous = with_runtime(|runtime| runtime.snapshots.current().cloned());
        self.push_entered();
        previous.map(|snapshot| SnapshotToken { _previous: snapshot })
    }

    /// Ends [`unsafe_enter`](Snapshot::unsafe_enter), as `unsafeLeave`. Panics if this snapshot
    /// isn't the current one.
    pub fn unsafe_leave(&self, _token: Option<SnapshotToken>) {
        let current = with_runtime(|runtime| {
            let snapshots = &runtime.snapshots;
            snapshots.top_frame_entered()
                && match (snapshots.current(), &self.inner) {
                    (Some(current), Some(inner)) => Rc::ptr_eq(current, inner),
                    (None, None) => true,
                    _ => false,
                }
        });
        assert!(current, "Cannot leave snapshot; {self:?} is not the current snapshot");
        // The frame it entered: the one on top.
        drop(FrameGuard(with_runtime(|runtime| runtime.snapshots.depth()) - 1));
    }

    /// `future`, each of its polls in this snapshot, as `asContextElement`: compose-rs has no
    /// coroutine context. The snapshot must outlive it, as a context element's.
    pub fn enter_future<F: Future>(&self, future: F) -> SnapshotFuture<F> {
        SnapshotFuture { snapshot: self.handle(), future: Box::pin(future) }
    }

    /// Disposes it, as `dispose`: a mutable snapshot not applied is abandoned, what it wrote gone.
    /// Dropping the owner does it too.
    pub fn dispose(&self) {
        if let Some(inner) = &self.inner {
            dispose(inner);
        }
    }

    /// Runs `block` in the global snapshot, as `Snapshot.global`.
    pub fn global<R>(block: impl FnOnce() -> R) -> R {
        let depth = with_runtime(|runtime| {
            let depth = runtime.snapshots.depth();
            let frame = ObserverFrame { read: None, write: None, barrier: true, entered: None };
            runtime.snapshots.enter(None, false, frame);
            depth
        });
        let _frame = FrameGuard(depth);
        block()
    }

    /// Runs `block` in a new mutable snapshot and applies it, as `Snapshot.withMutableSnapshot`.
    /// Panics with [`SnapshotApplyConflict`] if the apply fails; if `block` panics nothing is
    /// applied.
    pub fn with_mutable_snapshot<R>(block: impl FnOnce() -> R) -> R {
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        let result = snapshot.enter(block);
        snapshot.apply().check();
        result
    }

    /// Runs `block` with writes refused, as `Snapshot.readOnly` (the method
    /// [`read_only`](Snapshot::read_only) has that name here): reading state is all it may do.
    pub fn with_read_only<R>(block: impl FnOnce() -> R) -> R {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = try_with_runtime(|runtime| runtime.snapshots.set_read_only(self.0));
            }
        }
        let was = with_runtime(|runtime| {
            let was = runtime.snapshots.read_only();
            runtime.snapshots.set_read_only(true);
            was
        });
        let _restore = Restore(was);
        block()
    }

    /// Sends the global snapshot's changes since the last send to the apply observers, as
    /// `Snapshot.sendApplyNotifications()`: the composables, observations and derived states
    /// that read the states written are invalidated first, then the observers registered with
    /// [`register_apply_observer`](Snapshot::register_apply_observer) hear the states, in
    /// registration order. Nothing if nothing changed. Turns send at their end; call it after
    /// writes made outside one to see their effect at once (a test's `set` then a check).
    pub fn send_apply_notifications() {
        send(false);
    }

    /// Registers `observer`, called with the states each send and each applied snapshot changed,
    /// as `Snapshot.registerApplyObserver`. It doesn't hear the changes made before: those are
    /// sent first, to the observers registered already.
    pub fn register_apply_observer(
        observer: impl Fn(&ChangeSet, &Snapshot) + 'static,
    ) -> ObserverHandle {
        // As `advanceGlobalSnapshot`: what's pending goes to the observers there are.
        send(true);
        with_runtime(|runtime| runtime.snapshots.add_apply_observer(Rc::new(observer)))
    }

    /// Registers `observer`, called with each state written in the global snapshot, at the
    /// write, as `Snapshot.registerGlobalWriteObserver`.
    pub fn register_global_write_observer(
        observer: impl Fn(StateObjectId) + 'static,
    ) -> ObserverHandle {
        with_runtime(|runtime| runtime.snapshots.add_global_write_observer(Rc::new(observer)))
    }

    /// Whether apply notifications are being sent, as `isApplyObserverNotificationPending`.
    pub fn is_apply_observer_notification_pending() -> bool {
        try_with_runtime(|runtime| runtime.snapshots.is_sending()).unwrap_or(false)
    }

    /// The snapshots open, the global one included, as `openSnapshotCount`.
    pub fn open_snapshot_count() -> usize {
        with_runtime(|runtime| runtime.snapshots.open().len() as usize + 1)
    }

    /// States made until now count as initialized: a later write to one is a change the apply
    /// observers hear, as `Snapshot.notifyObjectsInitialized()` (a state written in the global
    /// snapshot that made it isn't one, as in Compose). In the global snapshot it advances it, as
    /// `advanceGlobalSnapshot`, so what changed is sent. In a mutable snapshot, a pass's too, that one
    /// advances; in a read-only one nothing happens.
    pub fn notify_objects_initialized() {
        let current = with_runtime(|runtime| runtime.snapshots.current().cloned());
        match current {
            None => send(true),
            Some(inner) => {
                if !inner.kind.read_only() && inner.is_active() {
                    with_runtime(|runtime| inner.advance(&mut runtime.snapshots));
                }
                // Its observers (user code) are dropped outside the borrow, if this was the last.
                drop(inner);
            }
        }
    }

    /// Runs `block` with `read_observer` called for each state it reads and `write_observer`
    /// for each it writes, as `Snapshot.observe(readObserver, writeObserver, block)`: in
    /// addition to the observers already observing (the inner one first), isolating nothing.
    pub fn observe<R>(
        read_observer: Option<StateObserverFn>,
        write_observer: Option<StateObserverFn>,
        block: impl FnOnce() -> R,
    ) -> R {
        let depth = with_runtime(|runtime| {
            let depth = runtime.snapshots.depth();
            runtime.snapshots.push_frame(ObserverFrame {
                read: read_observer,
                write: write_observer,
                barrier: false,
                entered: None,
            });
            depth
        });
        let _frame = FrameGuard(depth);
        block()
    }

    /// Runs `block` without observing its reads, as `Snapshot.withoutReadObservation`: neither
    /// composition, nor an observation, nor a read observer started outside `block` records
    /// them (one started inside it does).
    pub fn without_read_observation<R>(block: impl FnOnce() -> R) -> R {
        let depth = with_runtime(|runtime| {
            let depth = runtime.snapshots.depth();
            runtime.snapshots.push_frame(ObserverFrame {
                read: None,
                write: None,
                barrier: true,
                entered: None,
            });
            depth
        });
        let _frame = FrameGuard(depth);
        block()
    }
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Snapshot) -> bool {
        match (&self.inner, &other.inner) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for Snapshot {}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Some(inner) => write!(formatter, "Snapshot({:?} {})", inner.kind, inner.id.get()),
            None => formatter.write_str("Snapshot(Global)"),
        }
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if self.owner
            && let Some(inner) = &self.inner
            && try_with_runtime(|_| ()).is_some()
        {
            dispose(inner);
        }
    }
}

/// Takes a snapshot of the global one, as `takeNewSnapshot`: it sees what was written so far, the
/// global snapshot advances so what's written next is above it, and what's pending is told.
fn take_root(
    from: &Snapshot,
    kind: Kind,
    read_observer: Option<StateObserverFn>,
    write_observer: Option<StateObserverFn>,
) -> Snapshot {
    let inner = with_runtime(|runtime| {
        let (base_read, base_write, context) = from.base(runtime);
        let snapshots = &mut runtime.snapshots;
        let id = snapshots.take_id();
        let invalid = snapshots.open().clone();
        let inner = SnapshotInner::new(
            snapshots,
            id,
            invalid,
            kind,
            None,
            merged_with(read_observer, base_read),
            if kind.read_only() { None } else { merged_with(write_observer, base_write) },
            context,
            Rc::new(Cell::new(1)),
        );
        snapshots.open_id(id);
        inner
    });
    let snapshot = Snapshot::owning(inner);
    send(true);
    snapshot
}

/// Pops the frames down to its depth when dropped (an unwind too), leaving what they entered: an
/// `unsafe_enter` not left inside an `enter` ends with it, as `enter`'s `restoreCurrent`.
struct FrameGuard(u32);

impl Drop for FrameGuard {
    fn drop(&mut self) {
        while let Some(popped) = try_with_runtime(|runtime| {
            if runtime.snapshots.depth() > self.0 { runtime.snapshots.pop_frame() } else { None }
        })
        .flatten()
        {
            // The observers, the last owner of what was entered: user code, outside the borrow.
            drop(popped);
        }
    }
}

/// What [`Snapshot::unsafe_enter`] replaced, if it wasn't the global snapshot.
pub struct SnapshotToken {
    _previous: SnapshotRef,
}

/// A mutable snapshot, as `MutableSnapshot`: it derefs to its [`Snapshot`].
pub struct MutableSnapshot(Snapshot);

impl std::ops::Deref for MutableSnapshot {
    type Target = Snapshot;
    fn deref(&self) -> &Snapshot {
        &self.0
    }
}

impl std::fmt::Debug for MutableSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl PartialEq<Snapshot> for MutableSnapshot {
    fn eq(&self, other: &Snapshot) -> bool {
        self.0 == *other
    }
}

impl PartialEq<MutableSnapshot> for Snapshot {
    fn eq(&self, other: &MutableSnapshot) -> bool {
        *self == other.0
    }
}

impl MutableSnapshot {
    /// A mutable snapshot nested in this one, as `takeNestedMutableSnapshot`: it sees what this
    /// one wrote, and what it writes stays in it until it's applied, to this one. Its reads and
    /// writes call the observers given, then this snapshot's.
    pub fn take_nested_mutable_snapshot(
        &self,
        read_observer: Option<StateObserverFn>,
        write_observer: Option<StateObserverFn>,
    ) -> MutableSnapshot {
        if let Some(parent) = &self.0.inner {
            parent.validate_not_disposed();
            parent.validate_not_applied_or_pinned();
        }
        let parent = self.0.inner.is_some().then_some(&self.0);
        let (read_observer, write_observer, creating) =
            tooling::pre_create(parent, false, read_observer, write_observer);
        let snapshot = self.nested_mutable_snapshot(read_observer, write_observer);
        tooling::created(&snapshot.0, parent, creating);
        snapshot
    }

    fn nested_mutable_snapshot(
        &self,
        read_observer: Option<StateObserverFn>,
        write_observer: Option<StateObserverFn>,
    ) -> MutableSnapshot {
        let Some(parent) = &self.0.inner else {
            return MutableSnapshot(take_root(
                &self.0,
                Kind::Mutable,
                read_observer,
                write_observer,
            ));
        };
        let inner = with_runtime(|runtime| {
            let (base_read, base_write, context) = self.0.base(runtime);
            nested_mutable(
                &mut runtime.snapshots,
                parent,
                merged_with(read_observer, base_read),
                merged_with(write_observer, base_write),
                context,
            )
        });
        MutableSnapshot(Snapshot::owning(inner))
    }

    /// Applies what it wrote, as `apply`: to the global snapshot, whose observers hear it (to the
    /// parent for a nested one, which applies it in turn). A state written meanwhile that the
    /// policy can't merge fails it, nothing applied and the snapshot still open.
    pub fn apply(&self) -> SnapshotApplyResult {
        let Some(inner) = &self.0.inner else {
            panic!("Cannot apply the global snapshot directly. Call Snapshot.advanceGlobalSnapshot")
        };
        if inner.kind == Kind::NestedMutable {
            apply_nested(inner, &self.0)
        } else {
            apply_root(inner, &self.0)
        }
    }
}

/// A mutable snapshot nested in `parent`, as `takeNestedMutableSnapshot`.
fn nested_mutable(
    snapshots: &mut Snapshots,
    parent: &SnapshotRef,
    read_observer: Option<StateObserverFn>,
    write_observer: Option<StateObserverFn>,
    context: ReadContext,
) -> SnapshotRef {
    let previous = parent.id.get();
    parent.record_previous(previous);
    let id = snapshots.take_id();
    snapshots.open_id(id);
    let invalid = parent.invalid();
    *parent.invalid.borrow_mut() = invalid.set(id);
    let inner = SnapshotInner::new(
        snapshots,
        id,
        invalid.add_range(previous + 1, id),
        Kind::NestedMutable,
        Some(parent.clone()),
        read_observer,
        write_observer,
        context,
        Rc::new(Cell::new(1)),
    );
    parent.nested.set(parent.nested.get() + 1);
    if parent.is_active() {
        parent.take_new_id(snapshots);
        parent.advance_parents(snapshots);
        parent.add_invalid(previous + 1, parent.id.get());
    }
    inner
}

/// The snapshot a pass composes in, as `Recomposer.composing` takes (§3.13): of the current one, the
/// global one nearly always. It has no observers and no frame: composition's reads register in the
/// cells, and what observes the pass goes on observing. A tool observing snapshots hears of it as
/// of any, and what it gives is a frame (D1503).
pub(crate) struct PassSnapshot {
    snapshot: SnapshotRef,
    outer: Option<OuterPass>,
    /// Nothing else could have written during the pass.
    alone: bool,
    /// Tools observe snapshots, as of its begin and then of its leaving: it goes through `apply`,
    /// as the shortcut dispatches nothing.
    observed: bool,
    /// What the tools gave it is a frame on the stack, till `leave`.
    framed: bool,
    /// That frame, to drop outside the runtime's borrow: closures have drops of their own.
    _left: Option<Box<ObserverFrame>>,
}

/// [`PassSnapshot::commit`] when there may be something to decide, as any snapshot's apply.
fn commit_by_apply(inner: SnapshotRef) -> Committed {
    let snapshot = MutableSnapshot(Snapshot::owning(inner));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| snapshot.apply()));
    let committed = match result {
        Ok(SnapshotApplyResult::Success) => Committed::Applied,
        Ok(SnapshotApplyResult::Failure(_)) => Committed::Conflict,
        // Nothing applied: a policy's merge panicked. Otherwise an apply observer.
        Err(panic) if snapshot.0.inner.as_ref().is_some_and(|inner| !inner.applied.get()) => {
            Committed::MergePanicked(panic)
        }
        Err(panic) => Committed::ObserverPanicked(panic),
    };
    // A tool's `on_pre_dispose` panicking is an observer's, too.
    let disposed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(snapshot)));
    match (committed, disposed) {
        (Committed::Applied, Err(panic)) => Committed::ObserverPanicked(panic),
        (committed, _) => committed,
    }
}

/// What ending a pass's snapshot came to.
pub(crate) enum Committed {
    Applied,
    /// Something outside it wrote what it wrote: composition failed, as `applyAndCheck`.
    Conflict,
    /// A policy's `merge` panicked: nothing applied, as a conflict, the panic going on.
    MergePanicked(Box<dyn std::any::Any + Send>),
    /// An apply observer panicked, the snapshot applied: to go on once the pass's work has run.
    ObserverPanicked(Box<dyn std::any::Any + Send>),
}

impl PassSnapshot {
    /// Panics if a pass can't begin now, as `takeMutableSnapshot`: before it changes anything.
    pub(crate) fn check_can_begin(snapshots: &Snapshots) {
        if snapshots.read_only()
            || snapshots.current().is_some_and(|current| current.kind.read_only())
        {
            panic!("Cannot create a mutable snapshot of a read-only snapshot");
        }
    }

    /// Takes it and makes it current, in the runtime's borrow (no user code): no apply notifications
    /// here, as the pass's entry points sent.
    pub(crate) fn begin(snapshots: &mut Snapshots) -> PassSnapshot {
        let snapshot = match snapshots.current().cloned() {
            Some(parent) => nested_mutable(snapshots, &parent, None, None, ReadContext::NONE),
            None => {
                let id = snapshots.take_id();
                let invalid = snapshots.open().clone();
                let snapshot = match snapshots.take_spare_pass() {
                    Some(spare) => {
                        spare.reuse(snapshots, id, invalid);
                        spare
                    }
                    None => SnapshotInner::new(
                        snapshots,
                        id,
                        invalid,
                        Kind::Mutable,
                        None,
                        None,
                        None,
                        ReadContext::NONE,
                        Rc::new(Cell::new(1)),
                    ),
                };
                snapshots.open_id(id);
                snapshots.advance_global_silently();
                snapshot
            }
        };
        let outer = snapshots.begin_pass(snapshot.clone());
        let observed = snapshots.has_snapshot_observers();
        PassSnapshot {
            snapshot,
            outer: Some(outer),
            alone: false,
            observed,
            framed: false,
            _left: None,
        }
    }

    /// Tells the tools observing snapshots of it, as `creatingSnapshot`, once it's begun: it's made
    /// in the runtime's borrow, where their user code can't run. What they give hears the pass's
    /// reads and writes.
    pub(crate) fn tell_tools(&mut self) {
        if !self.observed {
            return;
        }
        let parent = self
            .outer
            .as_ref()
            .and_then(OuterPass::current)
            .map(|outer| Snapshot::handle_of(outer.clone()));
        let (read, write, creating) = tooling::pre_create(parent.as_ref(), false, None, None);
        if read.is_some() || write.is_some() {
            self.framed = true;
            with_runtime(|runtime| {
                runtime.snapshots.push_frame(ObserverFrame {
                    read,
                    write,
                    barrier: false,
                    entered: None,
                })
            });
        }
        let handle = Snapshot::handle_of(self.snapshot.clone());
        tooling::created(&handle, parent.as_ref(), creating);
    }

    /// What it replaced is current again; composing is over.
    pub(crate) fn leave(&mut self, snapshots: &mut Snapshots) {
        if self.framed {
            self.framed = false;
            self._left = snapshots.pop_frame().map(|(frame, _)| Box::new(frame));
        }
        self.alone = snapshots.pass_alone();
        self.observed = snapshots.has_snapshot_observers();
        if let Some(outer) = self.outer.take() {
            snapshots.end_pass(outer);
        }
    }

    /// Applies it, once left. With nothing else having written in the pass, there's nothing to
    /// decide (D794): what it wrote stands, what's to tell is told, and its object is the next
    /// pass's.
    pub(crate) fn commit(self) -> Committed {
        let committed = self.apply();
        // A pair no walk took (a write found equivalent to what's there now) isn't the next's.
        let _ = try_with_runtime(|runtime| runtime.snapshots.end_of_applies());
        committed
    }

    fn apply(self) -> Committed {
        let inner = self.snapshot;
        // A nested snapshot still open holds the object, so it's not reused; its pins are its own.
        if inner.kind != Kind::Mutable || !self.alone || self.observed {
            return commit_by_apply(inner);
        }
        let mut modified = std::mem::take(&mut *inner.modified.borrow_mut());
        dedupe(&mut modified);
        let quiet = with_runtime(|runtime| {
            let snapshots = &mut runtime.snapshots;
            inner.close(snapshots);
            let quiet = modified.is_empty()
                && !snapshots.has_changes()
                && !snapshots.has_told_during_pass();
            if quiet {
                snapshots.advance_global_silently();
                inner.release_pins(snapshots);
                snapshots.update_isolated();
            }
            quiet
        });
        inner.applied.set(true);
        let mut panic = None;
        if !quiet {
            let handle = Snapshot::handle_of(inner.clone());
            let told = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::global::send_with(
                    true,
                    Some(Applied { states: modified, snapshot: &handle }),
                );
            }));
            panic = told.err();
            with_runtime(|runtime| inner.release_pins(&mut runtime.snapshots));
        }
        inner.disposed.set(true);
        if with_runtime(|runtime| runtime.snapshots.isolated()) {
            super::global::reclaim_records();
        }
        if Rc::strong_count(&inner) == 1 {
            with_runtime(|runtime| runtime.snapshots.keep_spare_pass(inner));
        }
        match panic {
            Some(panic) => Committed::ObserverPanicked(panic),
            None => Committed::Applied,
        }
    }
}

/// What [`MutableSnapshot::apply`] came to, as `SnapshotApplyResult`.
#[must_use = "an apply can fail: check it"]
#[derive(PartialEq, Eq, Debug)]
pub enum SnapshotApplyResult {
    Success,
    /// The snapshot that failed to apply, still open, as `Failure.snapshot`.
    Failure(Snapshot),
}

impl SnapshotApplyResult {
    /// Panics with [`SnapshotApplyConflict`] if it failed, as `check`.
    pub fn check(self) {
        if let SnapshotApplyResult::Failure(snapshot) = self {
            let conflict = SnapshotApplyConflict { snapshot_id: snapshot.snapshot_id() };
            drop(snapshot);
            std::panic::panic_any(conflict);
        }
    }

    /// Whether it succeeded, as `succeeded`.
    pub fn succeeded(&self) -> bool {
        matches!(self, SnapshotApplyResult::Success)
    }
}

/// An apply that failed, as `SnapshotApplyConflictException`, the panic payload of
/// [`SnapshotApplyResult::check`] (`Send`, so it names the snapshot by its id).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SnapshotApplyConflict {
    pub snapshot_id: u64,
}

impl std::fmt::Display for SnapshotApplyConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Snapshot {} can't apply: a state it wrote conflicts", self.snapshot_id)
    }
}

impl std::error::Error for SnapshotApplyConflict {}

/// A future entering a snapshot around each poll: [`Snapshot::enter_future`].
pub struct SnapshotFuture<F> {
    snapshot: Snapshot,
    future: Pin<Box<F>>,
}

impl<F: Future> Future for SnapshotFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<F::Output> {
        let this = Pin::into_inner(self);
        let future = this.future.as_mut();
        this.snapshot.enter(|| future.poll(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::mutable_state_of;

    fn begin() -> PassSnapshot {
        with_runtime(|runtime| PassSnapshot::begin(&mut runtime.snapshots))
    }

    fn leave(pass: &mut PassSnapshot) {
        with_runtime(|runtime| pass.leave(&mut runtime.snapshots));
    }

    fn plain_in_a_pass() -> bool {
        with_runtime(|runtime| runtime.snapshots.plain() && runtime.snapshots.isolated())
    }

    /// `begin_as` refuses a pass in a pass today; composition-context's will nest (D796): what a
    /// child's snapshot changes comes back at its end, and what the outer must know goes on.
    #[test]
    fn a_pass_nested_in_a_pass_restores_the_outers_state() {
        let state = mutable_state_of(0);
        let handle = state.handle();
        let mut outer = begin();
        assert!(plain_in_a_pass());
        let outer_snapshot = Snapshot::current();

        let mut child = begin();
        assert!(plain_in_a_pass(), "of a plain pass, it's plain, and key 0");
        assert_ne!(Snapshot::current(), outer_snapshot);
        handle.set(1);
        leave(&mut child);
        assert_eq!(Snapshot::current(), outer_snapshot);
        assert!(plain_in_a_pass());
        assert_eq!(handle.get(), 0, "not applied yet");
        assert!(matches!(child.commit(), Committed::Applied));
        assert_eq!(handle.get(), 1, "applied to the outer");

        // A child that wasn't alone: so isn't the outer, and its taint is the outer's no more.
        let mut child = begin();
        Snapshot::global(|| {});
        assert!(!plain_in_a_pass(), "`Snapshot::global` ran");
        leave(&mut child);
        assert!(plain_in_a_pass(), "the outer's taint is its own");
        assert!(matches!(child.commit(), Committed::Applied));
        with_runtime(|runtime| assert!(!runtime.snapshots.pass_alone()));

        // A taint is inherited.
        Snapshot::global(|| {});
        assert!(!plain_in_a_pass());
        let mut child = begin();
        assert!(!plain_in_a_pass(), "inherited");
        leave(&mut child);
        assert!(matches!(child.commit(), Committed::Applied));

        leave(&mut outer);
        assert!(matches!(outer.commit(), Committed::Applied));
        assert_eq!(state.get(), 1);
        assert!(with_runtime(|runtime| !runtime.snapshots.isolated()));
    }

    /// A pass nested in a user's snapshot isn't plain, nor what nests in it.
    #[test]
    fn a_pass_nested_in_a_users_snapshot_isnt_plain() {
        let snapshot = Snapshot::take_mutable_snapshot(None, None);
        snapshot.enter(|| {
            let mut pass = begin();
            assert!(!with_runtime(|runtime| runtime.snapshots.plain()));
            let mut child = begin();
            assert!(!with_runtime(|runtime| runtime.snapshots.plain()));
            leave(&mut child);
            assert!(matches!(child.commit(), Committed::Applied));
            leave(&mut pass);
            assert!(matches!(pass.commit(), Committed::Applied));
        });
    }
}
