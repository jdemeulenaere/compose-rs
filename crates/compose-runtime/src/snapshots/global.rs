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

use std::cell::Cell;
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
    /// Apply notifications being sent (nested sends count).
    sending: u32,
    /// Turns in progress, which send at their end: a write in one doesn't wake the host.
    turns: u32,
}

thread_local! {
    /// The global snapshot's id, read by every write (const-initialized: no lazy check).
    static GLOBAL: Cell<SnapshotId> = const { Cell::new(super::records::FIRST_GLOBAL) };
    /// Whether a write must tell observers (a write observer frame, a global write observer).
    static WRITES_OBSERVED: Cell<bool> = const { Cell::new(false) };
}

/// The global snapshot's id.
#[inline]
pub(crate) fn global_id() -> SnapshotId {
    GLOBAL.with(Cell::get)
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
            sending: 0,
            turns: 0,
        }
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
        (self.frames.len() as u32) > self.barrier_top
            && self.frames[self.barrier_top as usize..].iter().any(|frame| frame.read.is_some())
    }

    fn update_writes_observed(&self) {
        let observed = !self.global_write_observers.is_empty()
            || self.frames.iter().any(|frame| frame.write.is_some());
        WRITES_OBSERVED.with(|flag| flag.set(observed));
    }

    /// Advances the global snapshot; returns the id it had.
    fn advance(&mut self) -> SnapshotId {
        GLOBAL.with(|global| {
            let previous = global.get();
            global.set(previous + 1);
            previous
        })
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
        Snapshot { id: global_id() }
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
    /// snapshot that made it isn't one, as in Compose). It advances the global snapshot.
    pub fn notify_objects_initialized() {
        with_runtime(|runtime| runtime.snapshots.advance());
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
            if writes {
                WRITES_OBSERVED.with(|flag| flag.set(true));
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

/// Records a write to `id`, made outside composition (`composing` false) or in it; `new_record`
/// says it made a new record in the global snapshot (its readable one was older).
#[inline]
pub(crate) fn wrote(id: StateId, new_record: bool, composing: bool) {
    let (wake, requested) = with_runtime(|runtime| {
        let mut wake = new_record && record_modified(runtime, id, !composing);
        // In composition the write invalidated everything at once.
        if composing {
            return (wake, false);
        }
        // Derived states reading it are marked now: no read is behind what it derives from.
        wake |= crate::state::invalidate_cell(
            runtime,
            id,
            crate::scope::ScopeId::NONE,
            crate::state::Invalidation::DERIVED,
        );
        (wake, frames_requested(runtime))
    });
    if wake {
        crate::executor::wake_host();
    }
    if requested {
        request_frames();
    }
    if WRITES_OBSERVED.with(Cell::get) {
        notify_write(id, composing);
    }
}

/// Adds `id` to the global snapshot's modified list; `pending`: its readers are invalidated at
/// the send (a write in composition invalidated them already). Returns whether the host should
/// be woken for a send (the first change since one, outside any turn).
fn record_modified(runtime: &mut Runtime, id: StateId, pending: bool) -> bool {
    let first = runtime.snapshots.modified.is_empty();
    runtime.snapshots.modified.push(id);
    if pending {
        runtime.cells.set_pending(id);
    }
    first && runtime.snapshots.turns == 0
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
/// calls the apply observers. With `always`, it advances even with nothing to send. Nothing
/// while a pass composes (its writes invalidate at once; the next turn sends).
fn send(always: bool) {
    let sent = with_runtime(|runtime| {
        if runtime.is_composing() {
            return None;
        }
        if runtime.snapshots.modified.is_empty() {
            if always {
                runtime.snapshots.advance();
            }
            return None;
        }
        let modified = std::mem::take(&mut runtime.snapshots.modified);
        let previous = runtime.snapshots.advance();
        runtime.snapshots.sending += 1;
        let mut flush = false;
        for &id in &modified {
            if runtime.cells.take_pending(id) {
                flush |= crate::state::invalidate_cell(
                    runtime,
                    id,
                    crate::scope::ScopeId::NONE,
                    crate::state::Invalidation::READERS,
                );
            }
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

/// Whether the global snapshot has changes to send.
fn has_pending_changes() -> bool {
    try_with_runtime(|runtime| !runtime.snapshots.modified.is_empty()).unwrap_or(false)
}

/// Sends, and again while what the send ran (apply observers, observations' callbacks) wrote
/// state, a few times at most: a turn ends with its changes sent, as Compose's next-turn send
/// would. What's still pending then waits for the next turn, which the host is asked for.
fn send_settled() {
    for _ in 0..16 {
        send(false);
        if !has_pending_changes() {
            return;
        }
    }
    crate::executor::wake_host();
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
            // What the turn made is initialized: its next write is a change, whatever read it
            // meanwhile, as `Recomposer`'s `notifyObjectsInitialized` closes a frame (D754).
            let _ = try_with_runtime(|runtime| runtime.snapshots.advance());
        }
    }
}

/// Advances the global snapshot after a pass composed, as applying Compose's composition
/// snapshot does, and again after its end-of-pass work, as `Recomposer`'s
/// `notifyObjectsInitialized` (D754): states made in the pass or by its effects are initialized,
/// so their next writes are changes.
pub(crate) fn pass_composed(runtime: &mut Runtime) {
    runtime.snapshots.advance();
}
