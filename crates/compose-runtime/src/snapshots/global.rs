//! The thread's snapshot world: the global snapshot, what every write outside a snapshot goes to,
//! the open snapshots, and the observers of changes, as Compose's `GlobalSnapshot` and the
//! globals of `Snapshot.kt` (D750, D751). The other snapshots are in [`super::snapshot`].
//!
//! A write records its state in the global snapshot's modified list when it makes a new record
//! there (the state's readable record was written in an earlier global snapshot), as Compose's
//! `overwritableRecord` does; [`Snapshot::send_apply_notifications`] advances the global
//! snapshot, invalidates what read the modified states (composition scopes, observations,
//! derived states) and tells the apply observers. Turns send at their end: the executor's
//! `run_ready`, a host's event or frame, [`Composition::recompose`](crate::Composition) before
//! it recomposes. A pass composes in a snapshot of its own (`PassSnapshot`, D791): what it writes
//! invalidates that composition's readers at once, as Compose's composition snapshot's write
//! observer does (D703, D712), and everything else as it applies (D793).

use std::rc::Rc;

use super::id_set::IdSet;
use super::records::View;
use super::snapshot::{Snapshot, SnapshotRef};
use super::{SnapshotId, StateObjectId};
use crate::runtime::{Runtime, frames_requested, request_frames, try_with_runtime, with_runtime};
use crate::scope::ScopeId;
use crate::state::{StateId, Told as Telling};

/// An apply observer, as Compose's `(Set<Any>, Snapshot) -> Unit`.
pub(crate) type ApplyObserver = Rc<dyn Fn(&ChangeSet, &Snapshot)>;

/// A read or write observer, as Compose's `(Any) -> Unit`.
pub type StateObserverFn = Rc<dyn Fn(StateObjectId)>;

/// An observation frame of [`Snapshot::observe`], a barrier of
/// [`Snapshot::without_read_observation`], or a snapshot entered.
pub(crate) struct ObserverFrame {
    pub(super) read: Option<StateObserverFn>,
    pub(super) write: Option<StateObserverFn>,
    /// A barrier: the read observers below it (and composition's, and observations started
    /// before it) don't see the reads made above it.
    pub(super) barrier: bool,
    /// A snapshot entered: only this frame's observers and those above it count (what the
    /// snapshot merged when it was taken is in this frame's), and what was current comes back
    /// with its end.
    pub(super) entered: Option<Entered>,
}

/// What a snapshot's entering replaced.
pub(super) struct Entered {
    pub(super) current: Option<SnapshotRef>,
    pub(super) read_only: bool,
}

/// How a read finds its record: what it checks, in one load.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadKind {
    /// No snapshot to look at, no read observer: a state's only record.
    Head,
    /// In a pass's snapshot: a state's only record, or the walk if it wrote the state.
    InPass,
    /// Read observers to tell, or snapshots to look at.
    Look,
}

/// What a pass's snapshot replaced of an outer pass's, back at its end. Unreachable while `begin_as`
/// refuses a pass in a pass; composition-context's nested passes need it (D796).
pub(super) struct OuterPass {
    current: Option<SnapshotRef>,
    read_only: bool,
    pass: Option<SnapshotRef>,
    plain: bool,
    tainted: bool,
    not_alone: bool,
    births: u64,
    read_again: Vec<(StateId, ScopeId)>,
    told_during_pass: Vec<StateId>,
}

/// The thread's snapshot world, in its runtime.
pub(crate) struct Snapshots {
    /// The states the global snapshot modified since the last send, in write order.
    modified: Vec<StateId>,
    apply_observers: Vec<(u64, ApplyObserver)>,
    global_write_observers: Vec<(u64, StateObserverFn)>,
    next_handle: u64,
    /// [`Snapshot::observe`]'s and [`Snapshot::without_read_observation`]'s frames.
    frames: Vec<ObserverFrame>,
    /// The number of barriers in `frames`, and the position just above the latest one.
    barriers: u32,
    barrier_top: u32,
    /// Whether a frame above the latest barrier has a read observer.
    reads_observed: bool,
    /// What each read checks.
    read_kind: ReadKind,
    /// The frame from which read observers count: the latest barrier or snapshot entered.
    read_top: u32,
    /// The frame from which write observers count: the latest snapshot entered.
    write_top: u32,
    /// Apply notifications being sent (nested sends count).
    sending: u32,
    /// Turns in progress, which send at their end: a write in one doesn't wake the host.
    turns: u32,
    /// The global snapshot's id: what a write's record takes.
    global: SnapshotId,
    /// The next id a snapshot takes, as `nextSnapshotId`.
    next_id: SnapshotId,
    /// The open snapshots but the global one, as `openNonGlobalSnapshots`: the global snapshot's
    /// invalid set.
    open: IdSet,
    /// How many open snapshots read each lowest id, sorted, as `pinningTable`: what the lowest of
    /// them keeps of a state's records.
    pins: Vec<(SnapshotId, u32)>,
    /// The snapshot entered, as `threadSnapshot`: `None` is the global one.
    current: Option<SnapshotRef>,
    /// Writes are refused: a read-only snapshot is current, or `Snapshot::with_read_only` runs.
    read_only: bool,
    /// Whether a read or a write must look at snapshots: one is open or current, writes are
    /// refused, or a state still has several records. When not, a state has one record, the head,
    /// and a write overwrites it (D752), which is nearly always.
    isolated: bool,
    /// See [`Snapshots::plain`].
    plain: bool,
    /// The snapshot of the pass composing.
    pass: Option<SnapshotRef>,
    /// It's of the global snapshot, or of a pass that is: it applies as it ends, so it counts as the
    /// global one, and a state with one record has the record it reads (D791).
    plain_pass: bool,
    /// (State, scope) pairs a write in the pass invalidated for this pass, as
    /// `observationsProcessed`: its apply skips them (D793).
    read_again: Vec<(StateId, ScopeId)>,
    /// What a send during the pass told of: readers hear at its end, what the pass goes on reading
    /// being old until then.
    told_during_pass: Vec<StateId>,
    /// `Snapshot::global` ran in the pass: a state made there may be above the pass's snapshot.
    pass_tainted: bool,
    /// Another snapshot than the pass's or one nested in it was entered or applied in the pass, so
    /// something may have written what it wrote: its apply must look.
    pass_not_alone: bool,
    /// States made so far, and when the pass's snapshot last advanced.
    births: u64,
    pass_births: u64,
    /// The last pass's snapshot, reused: a pass allocates nothing for it.
    spare_pass: Option<SnapshotRef>,
    /// The cells holding more than one record, as `extraStateObjects`.
    extra_records: Vec<StateId>,
    /// Reclamations in progress, which hold that list aside.
    reclaiming: u32,
    /// Writes in the global snapshot since it last advanced, as its `writeCount`.
    global_writes: u32,
    /// The last write's version (a new write's is the next, each write's distinct; they wrap
    /// after 2³² writes, far beyond what a derived state compares across).
    version: u32,
    /// Whether a write must tell observers (a write observer frame, a global write observer).
    writes_observed: bool,
    /// A write outside a turn asked the host for a turn since the last send.
    woken: bool,
    /// Derived states an observation read, what they read having changed: the send validates
    /// them, outside the borrow.
    derived_queue: Vec<StateId>,
    /// The calculations of derived states in progress, innermost last.
    calculations: Vec<Calculation>,
    /// The current walk through derived states (`invalidate_cell`).
    walk: u32,
    /// A finished calculation's list of what it read, for its room.
    spare_reads: Vec<StateId>,
}

/// A derived state's calculation in progress.
struct Calculation {
    /// What it read.
    reads: Vec<StateId>,
    /// Its observation, which reads in observations started inside it add to.
    observing: Option<crate::observe::Observing>,
}

impl Snapshots {
    pub(crate) const fn new() -> Snapshots {
        Snapshots {
            modified: Vec::new(),
            apply_observers: Vec::new(),
            global_write_observers: Vec::new(),
            next_handle: 0,
            frames: Vec::new(),
            barriers: 0,
            barrier_top: 0,
            reads_observed: false,
            read_kind: ReadKind::Head,
            read_top: 0,
            write_top: 0,
            sending: 0,
            turns: 0,
            global: super::records::FIRST_GLOBAL,
            next_id: super::records::FIRST_GLOBAL + 1,
            open: IdSet::EMPTY,
            pins: Vec::new(),
            current: None,
            read_only: false,
            isolated: false,
            plain: true,
            pass: None,
            plain_pass: false,
            read_again: Vec::new(),
            told_during_pass: Vec::new(),
            pass_tainted: false,
            pass_not_alone: false,
            births: 0,
            pass_births: 0,
            spare_pass: None,
            reclaiming: 0,
            extra_records: Vec::new(),
            global_writes: 0,
            version: 0,
            writes_observed: false,
            woken: false,
            derived_queue: Vec::new(),
            calculations: Vec::new(),
            walk: 0,
            spare_reads: Vec::new(),
        }
    }

    /// The current walk through derived states: one a send, one a write in composition.
    #[inline]
    pub(crate) fn walk(&self) -> u32 {
        self.walk
    }

    /// Starts a walk through derived states.
    fn next_walk(&mut self) {
        self.walk = self.walk.wrapping_add(1);
    }

    /// The observation of the calculation in progress, if any.
    #[inline]
    pub(crate) fn observing_calculation(&self) -> Option<crate::observe::Observing> {
        self.calculations.last()?.observing
    }

    /// Queues a derived state for the next send to validate.
    pub(crate) fn queue_derived(&mut self, id: StateId) {
        self.derived_queue.push(id);
    }

    /// The calculation in progress read `id`.
    pub(crate) fn record_dependency(&mut self, id: StateId) {
        if let Some(calculation) = self.calculations.last_mut() {
            calculation.reads.push(id);
        }
    }

    /// The global snapshot's id.
    #[inline]
    pub(crate) fn global(&self) -> SnapshotId {
        self.global
    }

    /// Whether a read or a write must look at snapshots.
    #[inline]
    pub(crate) fn isolated(&self) -> bool {
        self.isolated
    }

    /// How a read finds its record.
    #[inline]
    pub(crate) fn read_kind(&self) -> ReadKind {
        self.read_kind
    }

    /// The view reads and writes go through, when isolation is in use.
    #[inline]
    pub(crate) fn view(&self) -> Option<View> {
        if self.isolated { Some(self.make_view()) } else { None }
    }

    #[cold]
    fn make_view(&self) -> View {
        match &self.current {
            Some(snapshot) => View {
                id: snapshot.id(),
                invalid: snapshot.invalid(),
                read_only: self.read_only,
                global: false,
                open_highest: 0,
                pass: self.is_plain_pass(snapshot),
            },
            None => View {
                id: self.global,
                invalid: self.open.clone(),
                read_only: self.read_only,
                global: true,
                open_highest: self.open.highest(0),
                pass: false,
            },
        }
    }

    /// The current snapshot's id.
    #[inline]
    pub(crate) fn current_id(&self) -> SnapshotId {
        match &self.current {
            Some(snapshot) => snapshot.id(),
            None => self.global,
        }
    }

    /// The id and birth snapshot of a state made now: the global snapshot's, or one every
    /// snapshot can read if another is current.
    #[inline]
    pub(crate) fn birth(&mut self) -> (SnapshotId, SnapshotId) {
        self.births += 1;
        match &self.current {
            Some(snapshot) => (super::records::PREEXISTING, snapshot.id()),
            None => (self.global, self.global),
        }
    }

    /// Takes the next id, as `nextSnapshotId`.
    pub(super) fn take_id(&mut self) -> SnapshotId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub(super) fn next_id(&self) -> SnapshotId {
        self.next_id
    }

    pub(super) fn open(&self) -> &IdSet {
        &self.open
    }

    pub(super) fn open_id(&mut self, id: SnapshotId) {
        self.open = self.open.set(id);
        self.update_isolated();
    }

    /// Closes `id` and the ids it had before, as `closeLocked`.
    pub(super) fn close_ids(&mut self, id: SnapshotId, previous: &IdSet) {
        self.open = self.open.clear(id).and_not(previous);
    }

    /// Pins `id`: what's readable at it stays.
    pub(super) fn pin(&mut self, id: SnapshotId) {
        match self.pins.binary_search_by_key(&id, |&(pinned, _)| pinned) {
            Ok(index) => self.pins[index].1 += 1,
            Err(index) => self.pins.insert(index, (id, 1)),
        }
    }

    pub(super) fn unpin(&mut self, id: SnapshotId) {
        if let Ok(index) = self.pins.binary_search_by_key(&id, |&(pinned, _)| pinned) {
            self.pins[index].1 -= 1;
            if self.pins[index].1 == 0 {
                self.pins.remove(index);
            }
        }
    }

    /// The limit of `overwriteUnusedRecordsLocked`: below it only the newest record of a state
    /// can be read. The global snapshot reads too: a snapshot that advanced past it and is gone
    /// has records it doesn't see yet (Compose reclaims only after advancing it).
    pub(super) fn reuse_limit(&self) -> SnapshotId {
        self.pins.first().map_or(self.next_id, |&(id, _)| id).min(self.global + 1)
    }

    pub(super) fn lowest_pin(&self) -> Option<SnapshotId> {
        self.pins.first().map(|&(id, _)| id)
    }

    pub(super) fn update_isolated(&mut self) {
        self.isolated = !self.open.is_empty()
            || self.current.is_some()
            || self.read_only
            || self.reclaiming > 0
            || !self.extra_records.is_empty();
        self.plain = !self.isolated || self.in_own_pass();
        self.update_read_kind();
    }

    fn update_read_kind(&mut self) {
        self.read_kind = if !self.plain || self.reads_observed {
            ReadKind::Look
        } else if self.isolated {
            ReadKind::InPass
        } else {
            ReadKind::Head
        };
    }

    /// Whether code runs in a pass's snapshot that's of the global one, nothing else since its start
    /// having been able to make a state it can't read.
    fn in_own_pass(&self) -> bool {
        self.plain_pass
            && !self.read_only
            && !self.pass_tainted
            && match (&self.pass, &self.current) {
                (Some(pass), Some(current)) => Rc::ptr_eq(pass, current),
                _ => false,
            }
    }

    /// Whether a read may take a state's only record without looking at snapshots: there are none
    /// to look at, or it's in a pass's snapshot (D791).
    #[inline]
    pub(crate) fn plain(&self) -> bool {
        self.plain
    }

    /// The pass's snapshot starts, replacing the current one with no frame: what observes the pass
    /// goes on observing, and reads go on registering with composition. What's kept of an outer
    /// pass comes back at its end (D796).
    pub(super) fn begin_pass(&mut self, pass: SnapshotRef) -> OuterPass {
        let current = self.current.replace(pass.clone());
        let read_only = std::mem::replace(&mut self.read_only, false);
        let plain = match (&current, &self.pass) {
            (None, _) => true,
            (Some(current), Some(outer)) => Rc::ptr_eq(current, outer) && self.plain_pass,
            _ => false,
        };
        let outer = OuterPass {
            current,
            read_only,
            pass: self.pass.replace(pass),
            plain: std::mem::replace(&mut self.plain_pass, plain),
            tainted: self.pass_tainted,
            not_alone: std::mem::replace(&mut self.pass_not_alone, false),
            births: std::mem::replace(&mut self.pass_births, self.births),
            read_again: std::mem::take(&mut self.read_again),
            told_during_pass: std::mem::take(&mut self.told_during_pass),
        };
        self.update_isolated();
        outer
    }

    /// Whether nothing else could have written during the pass.
    pub(super) fn pass_alone(&self) -> bool {
        !self.pass_not_alone
    }

    /// Something else may have written: its apply looks.
    pub(super) fn note_not_alone(&mut self) {
        self.pass_not_alone = true;
    }

    /// The pass's snapshot ends: what it replaced comes back. What an outer pass must know of it
    /// goes on: it wasn't alone, and what's to tell at its apply (D796).
    pub(super) fn end_pass(&mut self, outer: OuterPass) {
        self.current = outer.current;
        self.read_only = outer.read_only;
        self.pass = outer.pass;
        self.plain_pass = outer.plain;
        self.pass_tainted = outer.tainted;
        self.pass_not_alone |= outer.not_alone;
        self.pass_births = outer.births;
        if self.pass.is_none() {
            self.pass_not_alone = false;
        } else {
            let mut read_again = outer.read_again;
            read_again.append(&mut self.read_again);
            self.read_again = read_again;
            let mut told = outer.told_during_pass;
            told.append(&mut self.told_during_pass);
            self.told_during_pass = told;
        }
        self.update_isolated();
    }

    /// A write in the pass invalidated `scope` for this pass.
    pub(crate) fn note_read_again(&mut self, state: StateId, scope: ScopeId) {
        self.read_again.push((state, scope));
    }

    /// Whether a write in the pass invalidated `scope` for this pass, once.
    pub(crate) fn take_read_again(&mut self, state: StateId, scope: ScopeId) -> bool {
        match self.read_again.iter().position(|&pair| pair == (state, scope)) {
            Some(index) => {
                self.read_again.swap_remove(index);
                true
            }
            None => false,
        }
    }

    /// A pass's apply is over: pairs no walk took are no one's, unless an outer pass has them to
    /// come.
    pub(super) fn end_of_applies(&mut self) {
        if self.pass.is_none() {
            self.read_again.clear();
        }
    }

    /// Whether a send during the pass left something to tell.
    pub(super) fn has_told_during_pass(&self) -> bool {
        !self.told_during_pass.is_empty()
    }

    /// Whether the current snapshot is the pass's or nested in it: its writes are composition's.
    fn writing_in_pass(&self) -> bool {
        match (&self.pass, &self.current) {
            (Some(pass), Some(current)) => current.is_or_nested_in(pass),
            _ => false,
        }
    }

    /// Whether `snapshot` is a pass's that counts as the global one.
    pub(super) fn is_plain_pass(&self, snapshot: &SnapshotRef) -> bool {
        self.plain_pass && self.pass.as_ref().is_some_and(|pass| Rc::ptr_eq(pass, snapshot))
    }

    /// A pass's finished snapshot, to use for the next.
    pub(super) fn take_spare_pass(&mut self) -> Option<SnapshotRef> {
        self.spare_pass.take()
    }

    pub(super) fn keep_spare_pass(&mut self, snapshot: SnapshotRef) {
        self.spare_pass = Some(snapshot);
    }

    /// Whether nothing's been made in the pass since its snapshot last advanced.
    pub(super) fn no_births_in_pass(&self) -> bool {
        self.births == self.pass_births
    }

    pub(super) fn pass_advanced(&mut self) {
        self.pass_births = self.births;
    }

    /// The state has more than one record now.
    pub(crate) fn note_extra(&mut self, id: StateId) {
        self.extra_records.push(id);
        self.update_isolated();
    }

    pub(super) fn current(&self) -> Option<&SnapshotRef> {
        self.current.as_ref()
    }

    pub(super) fn read_only(&self) -> bool {
        self.read_only
    }

    pub(super) fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
        self.update_isolated();
    }

    pub(super) fn global_writes(&self) -> u32 {
        self.global_writes
    }

    /// The read observers visible now, merged, as a snapshot taken now merges them.
    pub(super) fn merged_read_observer(&self) -> Option<StateObserverFn> {
        let observers = self.frames[self.read_top as usize..].iter().filter_map(|f| f.read.clone());
        merged(observers.rev().collect())
    }

    /// The write observers visible now, merged.
    pub(super) fn merged_write_observer(&self) -> Option<StateObserverFn> {
        let observers =
            self.frames[self.write_top as usize..].iter().filter_map(|f| f.write.clone());
        merged(observers.rev().collect())
    }

    /// Enters a frame; the guard or `unsafe_leave` pops it.
    pub(super) fn push_frame(&mut self, frame: ObserverFrame) {
        let barrier = frame.barrier;
        let entered = frame.entered.is_some();
        let writes = frame.write.is_some();
        let position = self.frames.len() as u32;
        self.frames.push(frame);
        if barrier {
            self.barriers += 1;
            self.barrier_top = position + 1;
        }
        if barrier || entered {
            self.read_top = position;
        }
        if entered {
            self.write_top = position;
        }
        self.update_reads_observed();
        if writes {
            self.writes_observed = true;
        }
        if entered {
            self.update_writes_observed();
        }
    }

    /// Whether the top frame is an entered snapshot's.
    pub(super) fn top_frame_entered(&self) -> bool {
        self.frames.last().is_some_and(|frame| frame.entered.is_some())
    }

    /// Ends the top frame: what it held goes to the caller, to drop outside the borrow.
    pub(super) fn pop_frame(&mut self) -> Option<(ObserverFrame, Option<SnapshotRef>)> {
        let mut frame = self.frames.pop()?;
        if frame.barrier {
            self.barriers -= 1;
            self.barrier_top = self
                .frames
                .iter()
                .rposition(|frame| frame.barrier)
                .map_or(0, |position| position as u32 + 1);
        }
        if frame.barrier || frame.entered.is_some() {
            self.read_top = self
                .frames
                .iter()
                .rposition(|frame| frame.barrier || frame.entered.is_some())
                .map_or(0, |position| position as u32);
        }
        let mut left = None;
        if let Some(entered) = frame.entered.take() {
            self.write_top = self
                .frames
                .iter()
                .rposition(|frame| frame.entered.is_some())
                .map_or(0, |position| position as u32);
            left = std::mem::replace(&mut self.current, entered.current);
            self.read_only = entered.read_only;
            self.update_isolated();
        }
        self.update_reads_observed();
        self.update_writes_observed();
        Some((frame, left))
    }

    /// A snapshot enters: `frame`'s observers are its own.
    pub(super) fn enter(
        &mut self,
        snapshot: Option<SnapshotRef>,
        read_only: bool,
        mut frame: ObserverFrame,
    ) {
        self.pass_tainted |= self.pass.is_some() && snapshot.is_none();
        if let Some(pass) = &self.pass {
            let family = snapshot.as_ref().is_some_and(|snapshot| snapshot.is_or_nested_in(pass));
            self.pass_not_alone |= !family;
        }
        let entered = Entered {
            current: std::mem::replace(&mut self.current, snapshot),
            read_only: self.read_only,
        };
        frame.entered = Some(entered);
        self.read_only = read_only;
        self.push_frame(frame);
        self.update_isolated();
    }

    /// Advances the global snapshot, without telling of it, as
    /// `advanceGlobalSnapshotWithoutNotifications`.
    pub(super) fn advance_global_silently(&mut self) {
        self.advance();
    }

    /// The last write's version.
    #[inline]
    pub(crate) fn version(&self) -> u32 {
        self.version
    }

    /// A new write's version.
    #[inline]
    pub(crate) fn next_version(&mut self) -> u32 {
        self.version = self.version.wrapping_add(1);
        self.version
    }

    /// The ids and versions go on in a runtime replacing this one.
    pub(crate) fn continue_from(&mut self, other: &Snapshots) {
        self.global = other.global;
        self.next_id = other.next_id;
        self.version = other.version;
    }

    /// Whether the global snapshot has changes to send.
    #[inline]
    pub(crate) fn has_changes(&self) -> bool {
        !self.modified.is_empty() || !self.derived_queue.is_empty()
    }

    /// Whether nothing is registered: the runtime may be released at thread exit.
    pub(crate) fn is_empty(&self) -> bool {
        self.apply_observers.is_empty()
            && self.global_write_observers.is_empty()
            && self.turns == 0
            && !self.isolated
    }

    /// Whether a read registers with composition (no barrier is up).
    #[inline]
    pub(crate) fn composition_reads(&self) -> bool {
        self.barriers == 0
    }

    /// Whether a read registers with an observation started at frame depth `depth`.
    #[inline]
    pub(crate) fn observes_at(&self, depth: u32) -> bool {
        depth >= self.barrier_top
    }

    /// The frame depth an observation starting now is at.
    #[inline]
    pub(crate) fn depth(&self) -> u32 {
        self.frames.len() as u32
    }

    /// Whether a read must call read observers.
    #[inline]
    pub(crate) fn reads_observed(&self) -> bool {
        self.reads_observed
    }

    fn update_reads_observed(&mut self) {
        self.reads_observed =
            self.frames[self.read_top as usize..].iter().any(|frame| frame.read.is_some());
        self.update_read_kind();
    }

    fn update_writes_observed(&mut self) {
        self.writes_observed = !self.global_write_observers.is_empty()
            || self.frames[self.write_top as usize..].iter().any(|frame| frame.write.is_some());
    }

    /// Advances the global snapshot, as `resetGlobalSnapshotLocked`.
    fn advance(&mut self) {
        self.global = self.next_id;
        self.next_id += 1;
        self.global_writes = 0;
    }
}

/// Observers merged into one, the first called first, as `mergedReadObserver`.
fn merged(mut observers: Vec<StateObserverFn>) -> Option<StateObserverFn> {
    if observers.len() <= 1 {
        return observers.pop();
    }
    Some(Rc::new(move |state| {
        for observer in &observers {
            observer(state);
        }
    }))
}

/// `specified` then `parent`, as `mergedReadObserver`.
pub(super) fn merged_with(
    specified: Option<StateObserverFn>,
    parent: Option<StateObserverFn>,
) -> Option<StateObserverFn> {
    match (specified, parent) {
        (Some(specified), Some(parent)) if !Rc::ptr_eq(&specified, &parent) => {
            merged(vec![specified, parent])
        }
        (specified, parent) => specified.or(parent),
    }
}

/// A state's changes, as the set of objects Compose's apply observers receive.
pub struct ChangeSet {
    ids: Vec<StateId>,
}

impl ChangeSet {
    /// Whether `state` changed.
    pub fn contains(&self, state: impl Into<StateObjectId>) -> bool {
        let id = state.into().0;
        self.ids.contains(&id)
    }

    /// The states that changed, in the order they were first written.
    pub fn iter(&self) -> impl Iterator<Item = StateObjectId> + '_ {
        self.ids.iter().map(|&id| StateObjectId(id))
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

impl std::fmt::Debug for ChangeSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_set().entries(self.iter()).finish()
    }
}

/// A registration of an observer, as Compose's `ObserverHandle`: [`dispose`](Self::dispose)
/// (or dropping it) unregisters it.
#[must_use = "dropping the handle unregisters the observer"]
pub struct ObserverHandle {
    pub(super) handle: u64,
    pub(super) kind: HandleKind,
}

#[derive(Clone, Copy)]
pub(super) enum HandleKind {
    Apply,
    GlobalWrite,
}

impl ObserverHandle {
    /// Unregisters the observer.
    pub fn dispose(self) {}
}

impl Drop for ObserverHandle {
    fn drop(&mut self) {
        let removed = try_with_runtime(|runtime| {
            let snapshots = &mut runtime.snapshots;
            match self.kind {
                HandleKind::Apply => take_observer(&mut snapshots.apply_observers, self.handle)
                    .map(|observer| Box::new(observer) as Box<dyn std::any::Any>),
                HandleKind::GlobalWrite => {
                    let removed = take_observer(&mut snapshots.global_write_observers, self.handle)
                        .map(|observer| Box::new(observer) as Box<dyn std::any::Any>);
                    snapshots.update_writes_observed();
                    removed
                }
            }
        })
        .flatten();
        // The observer (user code) drops here, outside the borrow.
        drop(removed);
    }
}

fn take_observer<F>(observers: &mut Vec<(u64, F)>, handle: u64) -> Option<F> {
    let index = observers.iter().position(|(id, _)| *id == handle)?;
    Some(observers.remove(index).1)
}

impl Snapshots {
    /// Registers an apply observer.
    pub(super) fn add_apply_observer(&mut self, observer: ApplyObserver) -> ObserverHandle {
        self.next_handle += 1;
        let handle = self.next_handle;
        self.apply_observers.push((handle, observer));
        ObserverHandle { handle, kind: HandleKind::Apply }
    }

    /// Registers a global write observer.
    pub(super) fn add_global_write_observer(
        &mut self,
        observer: StateObserverFn,
    ) -> ObserverHandle {
        self.next_handle += 1;
        let handle = self.next_handle;
        self.global_write_observers.push((handle, observer));
        self.update_writes_observed();
        ObserverHandle { handle, kind: HandleKind::GlobalWrite }
    }

    /// Whether code runs in a snapshot other than the plain global one, as `isInSnapshot`.
    pub(super) fn is_in_snapshot(&self) -> bool {
        if self.current.is_some() || self.read_only {
            return true;
        }
        // `Snapshot::global` leaves the frames below it behind.
        let from = match self.frames.get(self.write_top as usize) {
            Some(frame) if frame.entered.is_some() => self.write_top as usize + 1,
            _ => 0,
        };
        self.frames.len() > from
    }

    pub(super) fn is_sending(&self) -> bool {
        self.sending > 0
    }

    pub(super) fn global_modified(&self) -> Vec<StateId> {
        self.modified.clone()
    }

    pub(super) fn has_global_changes(&self) -> bool {
        !self.modified.is_empty()
    }
}

/// Tells the read observers of `id`, if there are any; returns whether there were.
pub(crate) fn notify_read_if_observed(id: StateId) -> bool {
    let observed = with_runtime(|runtime| runtime.snapshots.reads_observed());
    if observed {
        notify_read(id);
    }
    observed
}

/// Calls the read observers of the frames above the latest barrier, innermost first.
pub(crate) fn notify_read(id: StateId) {
    let observers: Vec<StateObserverFn> = with_runtime(|runtime| {
        let snapshots = &runtime.snapshots;
        snapshots.frames[snapshots.read_top as usize..]
            .iter()
            .rev()
            .filter_map(|frame| frame.read.clone())
            .collect()
    });
    for observer in observers {
        observer(StateObjectId(id));
    }
}

/// What a write's record asks of the code after the borrow.
struct Told {
    wake: bool,
    requested: bool,
    observed: bool,
    composing: bool,
}

/// Tells of a write to `id`, whose user code is over (an `edit`'s closure may have sent), in the
/// runtime's borrow: marks its record with the global snapshot as it is now, records the change,
/// and invalidates what's due now.
#[inline]
fn tell(runtime: &mut Runtime, id: StateId) -> Told {
    if runtime.snapshots.isolated {
        return tell_after_a_snapshot_opened(runtime, id);
    }
    let snapshots = &mut runtime.snapshots;
    let version = snapshots.next_version();
    snapshots.global_writes = snapshots.global_writes.wrapping_add(1);
    runtime.cells.record_write(id, snapshots.global, version, &mut snapshots.modified);
    // The first change since a send, outside any turn, asks the host for one.
    let wake = !snapshots.modified.is_empty() && snapshots.turns == 0 && !snapshots.woken;
    snapshots.woken |= wake;
    let observed = snapshots.writes_observed;
    // Its readers hear at the send (D751); a pass, in a snapshot, doesn't come here.
    Told { wake, requested: frames_requested(runtime), observed, composing: false }
}

/// A write of the head whose user code (an `edit`'s closure, a policy's `equivalent`) opened a
/// snapshot: that snapshot can read this record, which keeps its id.
#[cold]
fn tell_after_a_snapshot_opened(runtime: &mut Runtime, id: StateId) -> Told {
    let version = runtime.snapshots.next_version();
    runtime.cells.touch(id, version);
    let global = runtime.snapshots.global;
    tell_in_view(runtime, id, true, global)
}

/// A write to `id` ended: told, with what the host and the observers should hear of it.
#[inline]
pub(crate) fn wrote(id: StateId) {
    let Told { wake, requested, observed, composing } = with_runtime(|runtime| tell(runtime, id));
    if wake {
        crate::executor::wake_host();
    }
    if requested {
        request_frames();
    }
    if observed {
        notify_write(id, composing);
    }
}

/// An `edit` panicked: its write is recorded for the next send, with no user code while unwinding
/// (no wake, frame request or write observer: those come with the next write or send).
pub(crate) fn wrote_unwinding(id: StateId) {
    let _ = try_with_runtime(|runtime| tell(runtime, id));
}

/// Tells of a write through a view, whose record is written already: in the current snapshot, or
/// the global one with snapshots open. `modified`: it's a change for the snapshot (the record it
/// read wasn't written there).
fn tell_in_view(
    runtime: &mut Runtime,
    id: StateId,
    modified: bool,
    written_at: SnapshotId,
) -> Told {
    if let Some(current) = runtime.snapshots.current() {
        current.record_write(id, modified);
        // Composition's write: what read the state is invalidated at once, as Compose's composition
        // snapshot's write observer does (D703, D712); the rest of the world hears at the apply.
        let composing = runtime.is_composing() && runtime.snapshots.writing_in_pass();
        if composing {
            runtime.snapshots.next_walk();
            crate::state::invalidate_cell(runtime, id, Telling::InPass);
        }
        let requested = composing && frames_requested(runtime);
        let observed = runtime.snapshots.writes_observed;
        return Told { wake: false, requested, observed, composing };
    }
    let snapshots = &mut runtime.snapshots;
    snapshots.global_writes = snapshots.global_writes.wrapping_add(1);
    // An `edit`'s closure sent: what it did after is the next send's.
    let modified = modified || written_at != snapshots.global;
    if written_at != snapshots.global {
        runtime.cells.relabel(id, written_at, snapshots.global);
    }
    if modified {
        runtime.cells.record_modified(id, &mut snapshots.modified);
    }
    let wake = !snapshots.modified.is_empty() && snapshots.turns == 0 && !snapshots.woken;
    snapshots.woken |= wake;
    let observed = snapshots.writes_observed;
    // Even from a pass (`Snapshot::global`): not composition's write, its readers hear at the send.
    Told { wake, requested: frames_requested(runtime), observed, composing: false }
}

/// A write through a view ended, as [`wrote`].
pub(crate) fn wrote_in_view(id: StateId, modified: bool, written_at: SnapshotId) {
    let Told { wake, requested, observed, composing } =
        with_runtime(|runtime| tell_in_view(runtime, id, modified, written_at));
    if wake {
        crate::executor::wake_host();
    }
    if requested {
        request_frames();
    }
    if observed {
        notify_write(id, composing);
    }
}

/// An `edit` through a view panicked, as [`wrote_unwinding`].
pub(crate) fn wrote_in_view_unwinding(id: StateId, modified: bool, written_at: SnapshotId) {
    let _ = try_with_runtime(|runtime| tell_in_view(runtime, id, modified, written_at));
}

/// The view of a read or write, when isolation is in use.
#[inline]
pub(crate) fn view() -> Option<View> {
    with_runtime(|runtime| runtime.snapshots.view())
}

/// A state has a new record beside its others.
pub(crate) fn note_extra_record(id: StateId) {
    with_runtime(|runtime| {
        let Runtime { cells, snapshots, .. } = runtime;
        if cells.mark_extra(id) {
            snapshots.note_extra(id);
        }
    });
}

/// Drops the records no open snapshot can read (§3.9); with none open, each state is back to one
/// record and reads and writes to the fast path.
pub(crate) fn reclaim_records() {
    let taken = try_with_runtime(|runtime| {
        let Runtime { cells, snapshots, .. } = runtime;
        if snapshots.extra_records.is_empty() {
            snapshots.update_isolated();
            return None;
        }
        snapshots.reclaiming += 1;
        let ids = std::mem::take(&mut snapshots.extra_records);
        for &id in &ids {
            cells.unmark_extra(id);
        }
        Some((ids, snapshots.reuse_limit()))
    })
    .flatten();
    let Some((ids, limit)) = taken else { return };
    /// Lists again what wasn't reclaimed, a value's `Drop` panicking too.
    struct Reclaiming(Vec<StateId>);
    impl Drop for Reclaiming {
        fn drop(&mut self) {
            let _ = try_with_runtime(|runtime| {
                let Runtime { cells, snapshots, .. } = runtime;
                snapshots.reclaiming -= 1;
                for &id in &self.0 {
                    if cells.mark_extra(id) {
                        snapshots.extra_records.push(id);
                    }
                }
                if snapshots.extra_records.is_empty() {
                    self.0.clear();
                    snapshots.extra_records = std::mem::take(&mut self.0);
                }
                snapshots.update_isolated();
            });
        }
    }
    let mut reclaiming = Reclaiming(ids);
    // Values drop here: user code, which may take snapshots and write, with no borrow held.
    reclaiming
        .0
        .retain(|&id| crate::state::state_object(id).is_some_and(|cell| cell.reclaim(limit)));
}

/// Calls the write observers of the frames, innermost first, and the global ones for a write
/// outside composition.
fn notify_write(id: StateId, composing: bool) {
    let observers: Vec<StateObserverFn> = with_runtime(|runtime| {
        let snapshots = &runtime.snapshots;
        let frames = snapshots.frames[snapshots.write_top as usize..]
            .iter()
            .rev()
            .filter_map(|frame| frame.write.clone());
        let global = snapshots
            .global_write_observers
            .iter()
            .filter(|_| !composing && snapshots.current.is_none())
            .map(|(_, observer)| observer.clone());
        frames.chain(global).collect()
    });
    for observer in observers {
        observer(StateObjectId(id));
    }
}

/// Ends the global snapshot's changes: advances it, invalidates the readers of what changed and
/// calls the apply observers. With `always`, it advances even with nothing to send.
pub(super) fn send(always: bool) {
    send_with(always, None);
}

/// What a snapshot applied changed.
pub(super) struct Applied<'a> {
    pub(super) states: Vec<StateId>,
    pub(super) snapshot: &'a Snapshot,
}

/// [`send`], with what an applied snapshot changed: its readers are invalidated too, and the apply
/// observers hear the global snapshot's changes first, then the snapshot's, as `apply` tells them.
pub(super) fn send_with(always: bool, applied: Option<Applied<'_>>) {
    let (own, applied_snapshot) = match applied {
        Some(Applied { states, snapshot }) => (states, Some(snapshot)),
        None => (Vec::new(), None),
    };
    let mut reclaim = false;
    let sent = with_runtime(|runtime| {
        let later = runtime.is_composing();
        if !later && runtime.snapshots.has_told_during_pass() {
            // The pass's over: its readers of what was told during it hear, what they read being
            // old (D793).
            runtime.snapshots.next_walk();
            for id in std::mem::take(&mut runtime.snapshots.told_during_pass) {
                crate::state::invalidate_cell(runtime, id, Telling::Applied);
            }
        }
        if !runtime.snapshots.has_changes() && own.is_empty() {
            if always {
                runtime.snapshots.advance();
                reclaim = runtime.snapshots.isolated;
            }
            return None;
        }
        let modified = std::mem::take(&mut runtime.snapshots.modified);
        runtime.snapshots.advance();
        runtime.snapshots.sending += 1;
        runtime.snapshots.woken = false;
        runtime.snapshots.next_walk();
        if later {
            // During a pass nothing is invalidated: what reads in it goes on reading what it reads,
            // and all readers hear at its end, as the `Recomposer` takes what it's told after
            // composing (`recordComposerModifications`).
            for &id in &modified {
                runtime.cells.sent_for_later(id);
            }
            runtime.snapshots.told_during_pass.extend_from_slice(&modified);
            runtime.snapshots.told_during_pass.extend_from_slice(&own);
        } else {
            for &id in &modified {
                crate::state::invalidate_cell(runtime, id, Telling::Sent);
            }
            for &id in &own {
                crate::state::invalidate_cell(runtime, id, Telling::Applied);
            }
        }
        let derived = std::mem::take(&mut runtime.snapshots.derived_queue);
        for &id in &derived {
            crate::state::dequeue_derived(runtime, id);
        }
        let requested = frames_requested(runtime);
        let observers: Option<Vec<ApplyObserver>> = ((!modified.is_empty() || !own.is_empty())
            && !runtime.snapshots.apply_observers.is_empty())
        .then(|| runtime.snapshots.apply_observers.iter().map(|(_, o)| o.clone()).collect());
        Some((modified, observers, requested, derived))
    });
    let Some((modified, observers, requested, derived)) = sent else {
        if reclaim {
            reclaim_records();
        }
        return;
    };
    struct Sending;
    impl Drop for Sending {
        fn drop(&mut self) {
            let _ = try_with_runtime(|runtime| runtime.snapshots.sending -= 1);
        }
    }
    let _sending = Sending;
    if requested {
        request_frames();
    }
    // What an observation read of a derived state: told now if its value changed.
    for id in derived {
        crate::state::current_version(id);
    }
    let changes = ChangeSet { ids: modified };
    let own = ChangeSet { ids: own };
    if let Some(observers) = observers {
        let global = Snapshot::global_handle();
        let snapshot = applied_snapshot.unwrap_or(&global);
        if !changes.is_empty() {
            for observer in &observers {
                observer(&changes, snapshot);
            }
        }
        if !own.is_empty() {
            for observer in &observers {
                observer(&own, snapshot);
            }
        }
    }
    // Back, for its capacity: a click allocates nothing.
    let mut ids = changes.ids;
    ids.clear();
    let isolated = try_with_runtime(|runtime| {
        if runtime.snapshots.modified.is_empty() {
            runtime.snapshots.modified = ids;
        }
        runtime.snapshots.isolated
    });
    // A snapshot's apply reclaims once its pins are released, after this.
    if isolated == Some(true) && applied_snapshot.is_none() {
        reclaim_records();
    }
}

/// Sends, and again while what the send ran (apply observers, observations' callbacks) wrote
/// state, a few times at most: a turn ends with its changes sent, as Compose's next-turn send
/// would. What's still pending then waits for the next turn, which the host is asked for.
fn send_settled() {
    for _ in 0..16 {
        // What the turn made is initialized too: its next write is a change, whatever read it
        // meanwhile, as `Recomposer`'s `notifyObjectsInitialized` closes a frame (D754).
        let settled = try_with_runtime(|runtime| {
            let snapshots = &mut runtime.snapshots;
            let settled = !snapshots.has_changes();
            if settled {
                snapshots.advance();
            }
            settled
        });
        if settled == Some(true) {
            if try_with_runtime(|runtime| runtime.snapshots.isolated) == Some(true) {
                reclaim_records();
            }
            return;
        }
        if settled.is_none() {
            return;
        }
        send(false);
    }
    crate::executor::wake_host();
}

/// What a host's tick does with no task to run: if writes made outside any turn wait, a turn, so
/// they're sent and what apply observers write in turn settles too (D755). Those writes asked
/// for this tick, and nothing else would send them until an unrelated frame.
pub(crate) fn send_unsent_outside_turns() {
    if with_runtime(|runtime| runtime.snapshots.has_changes() && runtime.snapshots.turns == 0) {
        drop(Turn::begin());
    }
}

/// A turn of a host's loop (a run of the executor's tasks, an event, a frame): writes made in it
/// don't wake the host, and the outermost turn sends the changes when it ends and advances the
/// global snapshot.
pub struct Turn {
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Turn {
    /// Starts a turn.
    pub fn begin() -> Turn {
        with_runtime(|runtime| runtime.snapshots.turns += 1);
        Turn { _not_send: std::marker::PhantomData }
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        let outermost = try_with_runtime(|runtime| {
            runtime.snapshots.turns -= 1;
            runtime.snapshots.turns == 0
        });
        if outermost == Some(true) && !std::thread::panicking() {
            send_settled();
        }
    }
}

/// The current snapshot and the write counter: while they're as they were, no state changed in
/// what this snapshot sees.
pub(crate) fn valid_mark() -> (SnapshotId, u32) {
    with_runtime(|runtime| (runtime.snapshots.current_id(), runtime.snapshots.version))
}

/// [`valid_mark`], and whether reads are plain.
#[inline]
pub(crate) fn valid_mark_and_plain() -> ((SnapshotId, u32), bool) {
    with_runtime(|runtime| {
        let snapshots = &runtime.snapshots;
        ((snapshots.current_id(), snapshots.version), snapshots.plain)
    })
}

/// Whether a pass's snapshot is what code runs in.
pub(crate) fn in_pass() -> bool {
    with_runtime(|runtime| runtime.snapshots.in_own_pass())
}

/// Whether reads are plain.
#[inline]
pub(crate) fn plain_reads() -> bool {
    with_runtime(|runtime| runtime.snapshots.plain)
}

/// The current snapshot and a new write's version, for a record a derived state writes.
pub(crate) fn next_record() -> (SnapshotId, u32) {
    with_runtime(|runtime| {
        let version = runtime.snapshots.next_version();
        (runtime.snapshots.current_id(), version)
    })
}

/// The birth of a state made now: its record's id, the snapshot it's made in, and a write's
/// version.
pub(crate) fn birth() -> (SnapshotId, SnapshotId, u32) {
    with_runtime(|runtime| {
        let (id, born) = runtime.snapshots.birth();
        (id, born, runtime.snapshots.next_version())
    })
}

/// A derived state's calculation starts: its reads are recorded (`record_dependency`).
pub(crate) fn begin_calculation() {
    with_runtime(|runtime| {
        let reads = std::mem::take(&mut runtime.snapshots.spare_reads);
        runtime.snapshots.calculations.push(Calculation { reads, observing: None })
    });
}

/// The calculation in progress has its observation.
pub(crate) fn observe_calculation() {
    with_runtime(|runtime| {
        let observing = runtime.observing;
        if let Some(calculation) = runtime.snapshots.calculations.last_mut() {
            calculation.observing = observing;
        }
    });
}

/// What the calculation in progress read.
pub(crate) fn calculation_reads() -> Vec<StateId> {
    with_runtime(|runtime| {
        runtime
            .snapshots
            .calculations
            .last_mut()
            .map(|calculation| std::mem::take(&mut calculation.reads))
            .unwrap_or_default()
    })
}

/// A list `calculation_reads` gave, for its room.
pub(crate) fn return_calculation_reads(mut reads: Vec<StateId>) {
    reads.clear();
    with_runtime(|runtime| runtime.snapshots.spare_reads = reads);
}

/// The calculation in progress ended.
pub(crate) fn end_calculation() {
    let _ = try_with_runtime(|runtime| runtime.snapshots.calculations.pop());
}

/// How many derived states' calculations are in progress.
pub(crate) fn calculation_depth() -> usize {
    with_runtime(|runtime| runtime.snapshots.calculations.len())
}

/// Advances the global snapshot, without sending, after a pass's end-of-pass work, as
/// `Recomposer`'s `notifyObjectsInitialized` (D754): what its effects made is initialized and their
/// next writes are changes.
pub(crate) fn advance_after_pass(runtime: &mut Runtime) {
    runtime.snapshots.advance();
}

/// A derived state's outermost calculation ended: as Compose's, what it made is initialized, and
/// what's written so far is sent. In a pass's snapshot, advancing for nothing made would give it an
/// id per calculation to keep, a screen's worth (D792).
pub(crate) fn calculation_ended() {
    let in_pass_with_nothing_made = with_runtime(|runtime| {
        let snapshots = &mut runtime.snapshots;
        if snapshots.in_own_pass() && !snapshots.no_births_in_pass() {
            snapshots.pass_advanced();
            return false;
        }
        snapshots.in_own_pass()
    });
    if !in_pass_with_nothing_made {
        Snapshot::notify_objects_initialized();
    }
}
