//! The global snapshot: what every write outside a snapshot goes to, and the observers of its
//! changes, as Compose's `GlobalSnapshot` and `Snapshot.Companion` (D750, D751).
//!
//! A write records its state in the global snapshot's modified list when it makes a new record
//! there (the state's readable record was written in an earlier global snapshot), as Compose's
//! `overwritableRecord` does; [`Snapshot::send_apply_notifications`] advances the global
//! snapshot, invalidates what read the modified states (composition scopes, observations,
//! derived states) and tells the apply observers. Turns send at their end: the executor's
//! `run_ready`, a host's event or frame, [`Composition::recompose`](crate::Composition) before
//! it recomposes. Writes during composition invalidate at once instead, as Compose's
//! composition snapshot's write observer does (D703, D712), and reach the apply observers at
//! the next send.

use std::rc::Rc;

use super::{SnapshotId, StateObjectId};
use crate::runtime::{Runtime, frames_requested, request_frames, try_with_runtime, with_runtime};
use crate::state::StateId;

/// An apply observer, as Compose's `(Set<Any>, Snapshot) -> Unit`.
pub(crate) type ApplyObserver = Rc<dyn Fn(&ChangeSet, &Snapshot)>;

/// A read or write observer, as Compose's `(Any) -> Unit`.
pub type StateObserverFn = Rc<dyn Fn(StateObjectId)>;

/// An observation frame of [`Snapshot::observe`], or a barrier of
/// [`Snapshot::without_read_observation`].
pub(crate) struct ObserverFrame {
    read: Option<StateObserverFn>,
    write: Option<StateObserverFn>,
    /// A barrier: the read observers below it (and composition's, and observations started
    /// before it) don't see the reads made above it.
    barrier: bool,
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
    /// Whether a frame above the latest barrier has a read observer: what each read checks.
    reads_observed: bool,
    /// Apply notifications being sent (nested sends count).
    sending: u32,
    /// Turns in progress, which send at their end: a write in one doesn't wake the host.
    turns: u32,
    /// The global snapshot's id: what a write's record takes.
    global: SnapshotId,
    /// The last write's version (a new write's is the next, each write's distinct; they wrap
    /// after 2³² writes, far beyond what a derived state compares across).
    version: u32,
    /// Whether a write must tell observers (a write observer frame, a global write observer).
    writes_observed: bool,
    /// A write outside a turn asked the host for a turn since the last send.
    woken: bool,
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
            sending: 0,
            turns: 0,
            global: super::records::FIRST_GLOBAL,
            version: 0,
            writes_observed: false,
            woken: false,
        }
    }

    /// The global snapshot's id.
    #[inline]
    pub(crate) fn global(&self) -> SnapshotId {
        self.global
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
        self.version = other.version;
    }

    /// Whether the global snapshot has changes to send.
    #[inline]
    pub(crate) fn has_changes(&self) -> bool {
        !self.modified.is_empty()
    }

    /// Whether nothing is registered: the runtime may be released at thread exit.
    pub(crate) fn is_empty(&self) -> bool {
        self.apply_observers.is_empty() && self.global_write_observers.is_empty() && self.turns == 0
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
            self.frames[self.barrier_top as usize..].iter().any(|frame| frame.read.is_some());
    }

    fn update_writes_observed(&mut self) {
        self.writes_observed = !self.global_write_observers.is_empty()
            || self.frames.iter().any(|frame| frame.write.is_some());
    }

    /// Advances the global snapshot; returns the id it had.
    fn advance(&mut self) -> SnapshotId {
        let previous = self.global;
        self.global = previous + 1;
        previous
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

/// A snapshot, as Compose's `Snapshot`: for now, the global snapshot whose changes an apply
/// observer hears (isolated snapshots come with step S5 of the design).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Snapshot {
    id: SnapshotId,
}

/// A registration of an observer, as Compose's `ObserverHandle`: [`dispose`](Self::dispose)
/// (or dropping it) unregisters it.
#[must_use = "dropping the handle unregisters the observer"]
pub struct ObserverHandle {
    handle: u64,
    kind: HandleKind,
}

#[derive(Clone, Copy)]
enum HandleKind {
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

impl Snapshot {
    /// The snapshot code runs in: the global one (until isolated snapshots, S5).
    pub fn current() -> Snapshot {
        Snapshot { id: with_runtime(|runtime| runtime.snapshots.global) }
    }

    /// The snapshot's id, as `snapshotId`: the global snapshot's grows as it advances.
    pub fn snapshot_id(&self) -> u64 {
        self.id
    }

    /// Whether writes in it are refused, as `readOnly`.
    pub fn read_only(&self) -> bool {
        false
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

    /// Registers `observer`, called with the states each send (and each applied snapshot)
    /// changed, as `Snapshot.registerApplyObserver`. It doesn't hear the changes made before:
    /// those are sent first, to the observers registered already.
    pub fn register_apply_observer(
        observer: impl Fn(&ChangeSet, &Snapshot) + 'static,
    ) -> ObserverHandle {
        // As `advanceGlobalSnapshot`: what's pending goes to the observers there are.
        send(true);
        with_runtime(|runtime| {
            let snapshots = &mut runtime.snapshots;
            snapshots.next_handle += 1;
            let handle = snapshots.next_handle;
            snapshots.apply_observers.push((handle, Rc::new(observer)));
            ObserverHandle { handle, kind: HandleKind::Apply }
        })
    }

    /// Registers `observer`, called with each state written in the global snapshot, at the
    /// write, as `Snapshot.registerGlobalWriteObserver`.
    pub fn register_global_write_observer(
        observer: impl Fn(StateObjectId) + 'static,
    ) -> ObserverHandle {
        with_runtime(|runtime| {
            let snapshots = &mut runtime.snapshots;
            snapshots.next_handle += 1;
            let handle = snapshots.next_handle;
            snapshots.global_write_observers.push((handle, Rc::new(observer)));
            snapshots.update_writes_observed();
            ObserverHandle { handle, kind: HandleKind::GlobalWrite }
        })
    }

    /// Whether apply notifications are being sent, as `isApplyObserverNotificationPending`.
    pub fn is_apply_observer_notification_pending() -> bool {
        try_with_runtime(|runtime| runtime.snapshots.sending > 0).unwrap_or(false)
    }

    /// States made until now count as initialized: a later write to one is a change the apply
    /// observers hear, as `Snapshot.notifyObjectsInitialized()` (a state written in the global
    /// snapshot that made it isn't one, as in Compose). It advances the global snapshot, as
    /// `advanceGlobalSnapshot`, so what changed is sent; during a pass only the id advances.
    pub fn notify_objects_initialized() {
        send(true);
    }

    /// Runs `block` with `read_observer` called for each state it reads and `write_observer`
    /// for each it writes, as `Snapshot.observe(readObserver, writeObserver, block)`: in
    /// addition to the observers already observing (the inner one first), isolating nothing.
    pub fn observe<R>(
        read_observer: Option<StateObserverFn>,
        write_observer: Option<StateObserverFn>,
        block: impl FnOnce() -> R,
    ) -> R {
        let _frame = FrameGuard::push(ObserverFrame {
            read: read_observer,
            write: write_observer,
            barrier: false,
        });
        block()
    }

    /// Runs `block` without observing its reads, as `Snapshot.withoutReadObservation`: neither
    /// composition, nor an observation, nor a read observer started outside `block` records
    /// them (one started inside it does).
    pub fn without_read_observation<R>(block: impl FnOnce() -> R) -> R {
        let _frame = FrameGuard::push(ObserverFrame { read: None, write: None, barrier: true });
        block()
    }
}

/// Pops its frame when dropped (an unwind too).
struct FrameGuard;

impl FrameGuard {
    fn push(frame: ObserverFrame) -> FrameGuard {
        with_runtime(|runtime| {
            let snapshots = &mut runtime.snapshots;
            let barrier = frame.barrier;
            let writes = frame.write.is_some();
            snapshots.frames.push(frame);
            if barrier {
                snapshots.barriers += 1;
                snapshots.barrier_top = snapshots.frames.len() as u32;
            }
            snapshots.update_reads_observed();
            if writes {
                snapshots.writes_observed = true;
            }
        });
        FrameGuard
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        let frame = try_with_runtime(|runtime| {
            let snapshots = &mut runtime.snapshots;
            let frame = snapshots.frames.pop()?;
            if frame.barrier {
                snapshots.barriers -= 1;
                snapshots.barrier_top = snapshots
                    .frames
                    .iter()
                    .rposition(|frame| frame.barrier)
                    .map_or(0, |position| position as u32 + 1);
            }
            snapshots.update_reads_observed();
            snapshots.update_writes_observed();
            Some(frame)
        })
        .flatten();
        drop(frame);
    }
}

/// Calls the read observers of the frames above the latest barrier, innermost first.
pub(crate) fn notify_read(id: StateId) {
    let observers: Vec<StateObserverFn> = with_runtime(|runtime| {
        let snapshots = &runtime.snapshots;
        snapshots.frames[snapshots.barrier_top as usize..]
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
    let composing = runtime.is_composing();
    let snapshots = &mut runtime.snapshots;
    let version = snapshots.next_version();
    let read_by_derived = runtime.cells.record_write(
        id,
        snapshots.global,
        version,
        !composing,
        &mut snapshots.modified,
    );
    // The first change since a send, outside any turn, asks the host for one.
    let mut wake = !snapshots.modified.is_empty() && snapshots.turns == 0 && !snapshots.woken;
    snapshots.woken |= wake;
    let observed = snapshots.writes_observed;
    let scope = crate::scope::ScopeId::NONE;
    if composing {
        // In composition everything is invalidated at once (D703, D712), reader by reader; a
        // frame request waits for the pass's end.
        wake |= crate::state::invalidate_cell(runtime, id, scope, crate::state::Invalidation::ALL);
    } else if read_by_derived {
        // Derived states reading it are marked now: no read is behind what it derives from. The
        // readers wait for the send (D751).
        wake |=
            crate::state::invalidate_cell(runtime, id, scope, crate::state::Invalidation::DERIVED);
    }
    Told { wake, requested: frames_requested(runtime), observed, composing }
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

/// Calls the write observers of the frames, innermost first, and the global ones for a write
/// outside composition.
fn notify_write(id: StateId, composing: bool) {
    let observers: Vec<StateObserverFn> = with_runtime(|runtime| {
        let snapshots = &runtime.snapshots;
        let frames = snapshots.frames.iter().rev().filter_map(|frame| frame.write.clone());
        let global = snapshots
            .global_write_observers
            .iter()
            .filter(|_| !composing)
            .map(|(_, observer)| observer.clone());
        frames.chain(global).collect()
    });
    for observer in observers {
        observer(StateObjectId(id));
    }
}

/// Ends the global snapshot's changes: advances it, invalidates the readers of what changed and
/// calls the apply observers. With `always`, it advances even with nothing to send. It sends
/// nothing while a pass composes (its writes invalidate at once; the next turn sends).
fn send(always: bool) {
    let sent = with_runtime(|runtime| {
        if runtime.is_composing() || runtime.snapshots.modified.is_empty() {
            if always {
                runtime.snapshots.advance();
            }
            return None;
        }
        let modified = std::mem::take(&mut runtime.snapshots.modified);
        let previous = runtime.snapshots.advance();
        runtime.snapshots.sending += 1;
        runtime.snapshots.woken = false;
        let mut flush = false;
        for &id in &modified {
            flush |= crate::state::invalidate_cell(
                runtime,
                id,
                crate::scope::ScopeId::NONE,
                crate::state::Invalidation::SENT,
            );
        }
        let requested = frames_requested(runtime);
        let observers: Option<Vec<ApplyObserver>> = (!runtime.snapshots.apply_observers.is_empty())
            .then(|| runtime.snapshots.apply_observers.iter().map(|(_, o)| o.clone()).collect());
        Some((modified, previous, observers, requested, flush))
    });
    let Some((modified, previous, observers, requested, flush)) = sent else { return };
    struct Sending;
    impl Drop for Sending {
        fn drop(&mut self) {
            let _ = try_with_runtime(|runtime| runtime.snapshots.sending -= 1);
        }
    }
    let _sending = Sending;
    if flush {
        crate::executor::wake_host();
    }
    if requested {
        request_frames();
    }
    let changes = ChangeSet { ids: modified };
    if let Some(observers) = observers {
        let snapshot = Snapshot { id: previous };
        for observer in observers {
            observer(&changes, &snapshot);
        }
    }
    // Back, for its capacity: a click allocates nothing.
    let mut ids = changes.ids;
    ids.clear();
    let _ = try_with_runtime(|runtime| {
        if runtime.snapshots.modified.is_empty() {
            runtime.snapshots.modified = ids;
        }
    });
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
        if settled != Some(false) {
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

/// What a host's tick does with no turn left open: recomputes the derived states read by layout
/// or drawing that a write marked, whose readers nothing else would tell: the send doesn't, and
/// no frame comes without them (D758). Step S2 replaces this, as derived states validate
/// themselves.
pub(crate) fn flush_derived_outside_turns() {
    if crate::derived::has_pending_derived_states()
        && with_runtime(|runtime| runtime.snapshots.turns == 0 && !runtime.is_busy())
    {
        crate::derived::flush_derived_states();
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

/// Advances the global snapshot, without sending, around a pass: when it starts, as Compose's
/// composition takes a snapshot of its own, so what it writes of what was made before is a
/// change (D756); when it composed, as applying that snapshot does; after its end-of-pass work,
/// as `Recomposer`'s `notifyObjectsInitialized` (D754), so what it and its effects made is
/// initialized and their next writes are changes.
pub(crate) fn advance_around_pass(runtime: &mut Runtime) {
    runtime.snapshots.advance();
}
