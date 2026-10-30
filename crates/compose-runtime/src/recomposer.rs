//! The `Recomposer`, as Compose's (`Recomposer.kt`): what a thread's compositions compose under. A
//! frame recomposes the invalid ones, those another's write invalidated too, then applies them all
//! (`perform_frame`), and `run_recompose_and_apply_changes` is that on the frame clock, with a
//! lifecycle that can be observed and shut down. Hosts drive frames themselves and call
//! `Composition::recompose` until the ui area moves them onto `perform_frame` (D1301).
//!
//! A thread's compositions are under its default recomposer unless they're given another
//! ([`Composition::with_parent`](crate::Composition::with_parent)).

use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;
use std::task::{Poll, Waker};

use crate::composer::{Mode, run_walk};
use crate::composition::{Composed, Pass, apply_work};
use crate::error::{ComposeError, misuse};
use crate::executor::{Job, launch, with_frame_nanos};
use crate::runtime::{CompositionId, with_runtime};
use crate::slot_table::SENTINEL;
use crate::snapshots::Snapshot;
use crate::state::{MutableState, ObjectHandle, Owned, OwnedState, State, mutable_state_of};

/// Where a recomposer is, as `Recomposer.State`, in the order Compose compares them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum RecomposerState {
    /// Cancelled or closed, its effects and its loop over: it does nothing any more.
    ShutDown,
    /// Cancelled or closed: its effects and its loop are ending.
    ShuttingDown,
    /// Not running, with nothing to do.
    Inactive,
    /// Not running, with something to do when it does.
    InactivePendingWork,
    /// Running, waiting for something to do.
    Idle,
    /// Running, with a frame to do.
    PendingWork,
}

impl crate::ComposeArg for RecomposerState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

type FrameEndCallback = Box<dyn FnOnce()>;

struct RecomposerInner {
    /// The thread's default: its compositions are those given none, and it's never shut down.
    is_default: bool,
    /// `cancel` or `close` was called.
    closed: Cell<bool>,
    cancelled: Cell<bool>,
    /// Over: stays so.
    shut_down: Cell<bool>,
    running: Cell<bool>,
    /// The state last derived, which `current_state`'s handle and the waiters follow.
    state: Cell<RecomposerState>,
    /// What `current_state` reads, made when it's first asked for.
    observed: RefCell<Option<OwnedState<RecomposerState>>>,
    /// What waits for its state to change (`await_idle`, `join`) and for work (its loop).
    waiters: RefCell<Vec<Waker>>,
    run_waker: RefCell<Option<Waker>>,
    change_count: Cell<u64>,
    frame_end_callbacks: RefCell<Vec<(u64, FrameEndCallback)>>,
    next_callback: Cell<u64>,
    /// Its compositions' effects and scopes' tasks not over yet.
    effects: Cell<usize>,
    jobs: RefCell<Vec<Job>>,
    paused_waiters: RefCell<Vec<Waker>>,
}

/// What a thread's compositions compose under, as `Recomposer`: a `Copy` handle, its owner the
/// [`Owned`] that [`new`](Recomposer::new) returns.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Recomposer {
    inner: ObjectHandle<RecomposerInner>,
}

impl std::fmt::Debug for Recomposer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Recomposer({:?})", self.state())
    }
}

/// A point compositions compose under, as `CompositionContext`: for now a recomposer (D1301).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CompositionContext {
    pub(crate) recomposer: Recomposer,
}

impl CompositionContext {
    /// Schedules `action` for the end of the current or next frame, as
    /// `scheduleFrameEndCallback`.
    pub fn schedule_frame_end_callback(
        &self,
        action: impl FnOnce() + 'static,
    ) -> CancellationHandle {
        self.recomposer.schedule_frame_end_callback(action)
    }
}

/// What cancels a frame-end callback, as `CancellationHandle`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CancellationHandle {
    recomposer: Recomposer,
    id: u64,
}

impl CancellationHandle {
    /// It won't run, if it hasn't.
    pub fn cancel(&self) {
        let dropped = self.recomposer.inner.try_get().and_then(|inner| {
            let mut callbacks = inner.frame_end_callbacks.borrow_mut();
            let at = callbacks.iter().position(|(id, _)| *id == self.id)?;
            Some(callbacks.remove(at))
        });
        drop(dropped);
        self.recomposer.update_state();
    }
}

/// What can be read of a running recomposer, as `RecomposerInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RecomposerInfo {
    recomposer: Recomposer,
}

impl crate::ComposeArg for RecomposerInfo {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl RecomposerInfo {
    pub fn current_state(&self) -> State<RecomposerState> {
        self.recomposer.current_state()
    }

    pub fn has_pending_work(&self) -> bool {
        self.recomposer.has_pending_work()
    }

    pub fn change_count(&self) -> u64 {
        self.recomposer.change_count()
    }
}

thread_local! {
    static DEFAULT: RefCell<Option<Owned<Recomposer>>> = const { RefCell::new(None) };
    /// Those whose loop is running, for what an invalidation wakes.
    static RUNNING: RefCell<Vec<Recomposer>> = const { RefCell::new(Vec::new()) };
    static RUNNING_INFO: RefCell<Option<OwnedState<Vec<RecomposerInfo>>>> =
        const { RefCell::new(None) };
    /// Those whose compositions' frame clock is paused.
    static PAUSED: RefCell<Vec<Recomposer>> = const { RefCell::new(Vec::new()) };
    /// Whether `RUNNING` or `PAUSED` has any: the one thing asked at every frame awaited.
    static FRAMES_HOOKED: Cell<bool> = const { Cell::new(false) };
}

impl Recomposer {
    fn make(is_default: bool) -> Owned<Recomposer> {
        let inner = RecomposerInner {
            is_default,
            closed: Cell::new(false),
            cancelled: Cell::new(false),
            shut_down: Cell::new(false),
            running: Cell::new(false),
            state: Cell::new(RecomposerState::Inactive),
            observed: RefCell::new(None),
            waiters: RefCell::new(Vec::new()),
            run_waker: RefCell::new(None),
            change_count: Cell::new(0),
            frame_end_callbacks: RefCell::new(Vec::new()),
            next_callback: Cell::new(0),
            effects: Cell::new(0),
            jobs: RefCell::new(Vec::new()),
            paused_waiters: RefCell::new(Vec::new()),
        };
        Owned::new(inner, |inner| Recomposer { inner })
    }

    /// A recomposer of its own: compositions go under it with
    /// [`Composition::with_parent`](crate::Composition::with_parent).
    pub fn new() -> Owned<Recomposer> {
        Recomposer::make(false)
    }

    /// The thread's default recomposer, what a composition given no parent is under.
    pub fn current_thread() -> Recomposer {
        DEFAULT.with_borrow_mut(|default| {
            default.get_or_insert_with(|| Recomposer::make(true)).handle()
        })
    }

    /// A context of this recomposer, as a `Recomposer` is a `CompositionContext` there.
    pub fn as_context(&self) -> CompositionContext {
        CompositionContext { recomposer: *self }
    }

    pub fn as_recomposer_info(&self) -> RecomposerInfo {
        RecomposerInfo { recomposer: *self }
    }

    fn is_default(&self) -> bool {
        self.inner.try_get().is_some_and(|inner| inner.is_default)
    }

    /// Whether `id`'s composition is under this one.
    fn holds(&self, data: &crate::runtime::CompositionData, is_default: bool) -> bool {
        match data.recomposer {
            Some(recomposer) => recomposer == *self,
            None => is_default,
        }
    }

    /// Its compositions with work for a pass, in order of creation (of a slot, when slots are
    /// reused). Not those composed in this frame, which are still finishing: what's written since
    /// is for the next frame, as Compose's `alreadyComposed`.
    fn invalid_compositions(&self) -> Vec<CompositionId> {
        let is_default = self.is_default();
        with_runtime(|runtime| {
            let compositions = &runtime.compositions;
            (compositions.live_ids().into_iter())
                .filter(|id| {
                    compositions.get(*id).is_some_and(|data| {
                        self.holds(data, is_default)
                            && !data.poisoned
                            && !data.closed
                            && !data.finishing
                            && data.table[SENTINEL].dirty
                    })
                })
                .collect()
        })
    }

    /// Whether it has anything to do for a frame, as `hasPendingWork`.
    pub fn has_pending_work(&self) -> bool {
        // The turn's changes invalidate their readers first (D751).
        let busy = with_runtime(|runtime| runtime.is_busy());
        if !busy {
            Snapshot::send_apply_notifications();
        }
        let has_frame_work = self.inner.try_get().is_some_and(|inner| {
            !inner.frame_end_callbacks.borrow().is_empty() || self.effects_await_frame(&inner)
        });
        has_frame_work || !self.invalid_compositions().is_empty()
    }

    /// Whether its compositions' effects wait for a frame, as Compose's
    /// `hasBroadcastFrameClockAwaiters`: its frame is theirs, what they write composed in it. The
    /// default recomposer's tasks aren't told apart: any.
    fn effects_await_frame(&self, inner: &RecomposerInner) -> bool {
        if inner.is_default {
            return crate::executor::has_frame_awaiters();
        }
        if inner.effects.get() == 0 {
            return false;
        }
        let jobs = inner.jobs.borrow();
        crate::executor::frame_awaiting_jobs().iter().any(|job| jobs.contains(job))
    }

    fn derive_state(&self) -> RecomposerState {
        let Some(inner) = self.inner.try_get() else { return RecomposerState::ShutDown };
        if inner.shut_down.get() {
            return RecomposerState::ShutDown;
        }
        if inner.closed.get() {
            if inner.effects.get() == 0 && !inner.running.get() {
                inner.shut_down.set(true);
                return RecomposerState::ShutDown;
            }
            return RecomposerState::ShuttingDown;
        }
        let pending = !inner.frame_end_callbacks.borrow().is_empty()
            || self.effects_await_frame(&inner)
            || !self.invalid_compositions().is_empty();
        match (inner.running.get(), pending) {
            (false, false) => RecomposerState::Inactive,
            (false, true) => RecomposerState::InactivePendingWork,
            (true, false) => RecomposerState::Idle,
            (true, true) => RecomposerState::PendingWork,
        }
    }

    /// Derives its state, tells who follows it, and wakes its loop if there's work.
    pub(crate) fn update_state(&self) {
        let Some(inner) = self.inner.try_get() else { return };
        let state = self.derive_state();
        let changed = inner.state.replace(state) != state;
        if state != RecomposerState::Idle {
            let waker = inner.run_waker.borrow_mut().take();
            if let Some(waker) = waker {
                waker.wake();
            }
        }
        if !changed {
            return;
        }
        let observed = inner.observed.borrow().as_ref().map(|state| state.handle());
        if let Some(observed) = observed {
            observed.set(state);
        }
        let waiters = std::mem::take(&mut *inner.waiters.borrow_mut());
        for waker in waiters {
            waker.wake();
        }
    }

    /// Its state as a state that can be observed, as `currentState`.
    pub fn current_state(&self) -> State<RecomposerState> {
        self.update_state();
        let Some(inner) = self.inner.try_get() else {
            panic!("a recomposer's owner was dropped: its state is gone");
        };
        let mut observed = inner.observed.borrow_mut();
        let state = observed.get_or_insert_with(|| mutable_state_of(inner.state.get()));
        let handle: MutableState<RecomposerState> = state.handle();
        handle.as_state()
    }

    fn state(&self) -> RecomposerState {
        self.inner.try_get().map_or(RecomposerState::ShutDown, |inner| inner.state.get())
    }

    /// How many frames changed something, as `changeCount`.
    pub fn change_count(&self) -> u64 {
        self.inner.try_get().map_or(0, |inner| inner.change_count.get())
    }

    /// Waits until its state is what `until` says.
    async fn wait_for(&self, until: impl Fn(RecomposerState) -> bool) {
        poll_fn(|context| {
            self.update_state();
            let Some(inner) = self.inner.try_get() else { return Poll::Ready(()) };
            if until(inner.state.get()) {
                return Poll::Ready(());
            }
            let mut waiters = inner.waiters.borrow_mut();
            if !waiters.iter().any(|waker| waker.will_wake(context.waker())) {
                waiters.push(context.waker().clone());
            }
            Poll::Pending
        })
        .await;
    }

    /// Waits until it has nothing left to do, as `awaitIdle`.
    pub async fn await_idle(&self) {
        self.wait_for(|state| state <= RecomposerState::Idle).await;
    }

    /// Waits until it's shut down, as `join`.
    pub async fn join(&self) {
        self.wait_for(|state| state == RecomposerState::ShutDown).await;
    }

    fn refuse_default(&self) {
        if self.is_default() {
            misuse(ComposeError::DefaultRecomposerShutDown)
        }
    }

    /// Shuts it down once what's running ends, as `close`: its loop goes on while its effects do,
    /// and no effect starts now.
    ///
    /// # Panics
    ///
    /// On the thread's default recomposer, which has what no one else is told of.
    pub fn close(&self) {
        self.refuse_default();
        if let Some(inner) = self.inner.try_get() {
            inner.closed.set(true);
        }
        self.update_state();
    }

    /// Shuts it down now, as `cancel`: its effects are cancelled and its loop returns.
    ///
    /// # Panics
    ///
    /// On the thread's default recomposer.
    pub fn cancel(&self) {
        self.refuse_default();
        let Some(inner) = self.inner.try_get() else { return };
        inner.closed.set(true);
        inner.cancelled.set(true);
        let jobs = std::mem::take(&mut *inner.jobs.borrow_mut());
        for job in jobs {
            job.cancel();
        }
        self.update_state();
    }

    /// Effects' `with_frame_nanos` wait from now on, as `pauseCompositionFrameClock`: composition
    /// goes on.
    pub fn pause_composition_frame_clock(&self) {
        let recomposer = *self;
        PAUSED.with_borrow_mut(|paused| {
            if !paused.contains(&recomposer) {
                paused.push(recomposer);
            }
        });
        update_frames_hooked();
    }

    /// Effects' frames go on, as `resumeCompositionFrameClock`.
    pub fn resume_composition_frame_clock(&self) {
        let recomposer = *self;
        PAUSED.with_borrow_mut(|paused| paused.retain(|other| *other != recomposer));
        update_frames_hooked();
        let Some(inner) = self.inner.try_get() else { return };
        let waiters = std::mem::take(&mut *inner.paused_waiters.borrow_mut());
        for waker in waiters {
            waker.wake();
        }
    }

    /// Schedules `action` for the end of the current or next frame, asking for one when idle, as
    /// `scheduleFrameEndCallback`. Run by [`perform_frame`].
    pub fn schedule_frame_end_callback(
        &self,
        action: impl FnOnce() + 'static,
    ) -> CancellationHandle {
        let inner = self.inner.try_get().expect("a recomposer whose owner is there");
        let id = inner.next_callback.get();
        inner.next_callback.set(id + 1);
        inner.frame_end_callbacks.borrow_mut().push((id, Box::new(action)));
        drop(inner);
        self.update_state();
        CancellationHandle { recomposer: *self, id }
    }

    /// Whether its loop goes on: while it's open, and closed while its effects run.
    fn keeps_going(&self) -> bool {
        self.inner.try_get().is_some_and(|inner| {
            !inner.cancelled.get() && (!inner.closed.get() || inner.effects.get() > 0)
        })
    }

    /// Recomposes its compositions on the frames of the thread's clock while it's open, as
    /// `runRecomposeAndApplyChanges`. Returns once it's cancelled, or closed with its effects over.
    ///
    /// # Panics
    ///
    /// If it's already running, or shut down.
    pub async fn run_recompose_and_apply_changes(&self) {
        let recomposer = *self;
        let _running = Running::begin(recomposer);
        while recomposer.keeps_going() {
            // Until there's something to do.
            poll_fn(|context| {
                Snapshot::send_apply_notifications();
                recomposer.update_state();
                if !recomposer.keeps_going() || recomposer.state() == RecomposerState::PendingWork {
                    return Poll::Ready(());
                }
                if let Some(inner) = recomposer.inner.try_get() {
                    *inner.run_waker.borrow_mut() = Some(context.waker().clone());
                }
                Poll::Pending
            })
            .await;
            if !recomposer.keeps_going() {
                break;
            }
            // Its frame, or being stopped while it waits for it.
            let mut frame = std::pin::pin!(with_frame_nanos(|time| time));
            let time = poll_fn(|context| {
                if !recomposer.keeps_going() {
                    return Poll::Ready(None);
                }
                if let Some(inner) = recomposer.inner.try_get() {
                    *inner.run_waker.borrow_mut() = Some(context.waker().clone());
                }
                frame.as_mut().poll(context).map(Some)
            })
            .await;
            let Some(time) = time else { break };
            // After the others waiting for this frame, as the clock's awaiters go first: what they
            // write is composed in it.
            let mut yielded = false;
            poll_fn(|context| {
                if std::mem::replace(&mut yielded, true) {
                    return Poll::Ready(());
                }
                context.waker().wake_by_ref();
                Poll::Pending
            })
            .await;
            perform_frame(recomposer, time);
        }
    }

    /// `future` as a task of its compositions' effects: over when it ends, cancelled with it, and
    /// never started once it's shut down.
    pub(crate) fn launch_effect(&self, future: impl Future<Output = ()> + 'static) -> Job {
        let Some(inner) = self.inner.try_get().filter(|inner| !inner.closed.get()) else {
            let job = launch(async {});
            job.cancel();
            return job;
        };
        inner.effects.set(inner.effects.get() + 1);
        let guard = EffectGuard(*self);
        let job = launch(async move {
            let _guard = guard;
            future.await;
        });
        let mut jobs = inner.jobs.borrow_mut();
        jobs.retain(Job::is_active);
        jobs.push(job);
        job
    }

    /// Whether `job` is one of its compositions' effects.
    fn is_effect(&self, job: Job) -> bool {
        self.inner.try_get().is_some_and(|inner| inner.jobs.borrow().contains(&job))
    }
}

/// An effect of a recomposer: what ends it tells it.
struct EffectGuard(Recomposer);

impl Drop for EffectGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.0.inner.try_get() {
            inner.effects.set(inner.effects.get() - 1);
        }
        self.0.update_state();
    }
}

/// A recomposer's loop: its end, cancelled or not, tells it.
struct Running(Recomposer);

impl Running {
    fn begin(recomposer: Recomposer) -> Running {
        let inner = recomposer.inner.try_get().expect("a recomposer whose owner is there");
        if inner.running.get() {
            misuse(ComposeError::RecomposerRunning)
        }
        if inner.closed.get() {
            misuse(ComposeError::RecomposerShutDown)
        }
        inner.running.set(true);
        drop(inner);
        let first = RUNNING.with_borrow_mut(|running| {
            running.push(recomposer);
            running.len() == 1
        });
        if first {
            with_runtime(|runtime| {
                runtime.compositions.recomposer_wake = Some(Rc::new(wake_running));
            });
        }
        update_frames_hooked();
        update_running_info();
        recomposer.update_state();
        Running(recomposer)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let Running(recomposer) = *self;
        if let Some(inner) = recomposer.inner.try_get() {
            inner.running.set(false);
            inner.run_waker.borrow_mut().take();
        }
        let empty = RUNNING.with_borrow_mut(|running| {
            running.retain(|other| *other != recomposer);
            running.is_empty()
        });
        if empty {
            crate::runtime::try_with_runtime(|runtime| {
                runtime.compositions.recomposer_wake = None;
            });
        }
        update_frames_hooked();
        update_running_info();
        recomposer.update_state();
    }
}

fn update_frames_hooked() {
    let running = RUNNING.try_with(|running| !running.borrow().is_empty()).unwrap_or(false);
    let paused = PAUSED.try_with(|paused| !paused.borrow().is_empty()).unwrap_or(false);
    let _ = FRAMES_HOOKED.try_with(|hooked| hooked.set(running || paused));
}

/// Whether frames awaited are of interest to a recomposer: none running or paused, none are.
#[inline]
pub(crate) fn frames_hooked() -> bool {
    FRAMES_HOOKED.with(Cell::get)
}

/// A task started waiting for a frame.
pub(crate) fn frame_awaited() {
    wake_running();
}

/// What an invalidation asks, while a loop is running.
fn wake_running() {
    let running = RUNNING.with_borrow(Vec::clone);
    for recomposer in running {
        recomposer.update_state();
    }
}

fn update_running_info() {
    let state = RUNNING_INFO.with_borrow(|info| info.as_ref().map(|state| state.handle()));
    if let Some(state) = state {
        state.set(running_list());
    }
}

fn running_list() -> Vec<RecomposerInfo> {
    RUNNING.with_borrow(|running| running.iter().map(Recomposer::as_recomposer_info).collect())
}

/// The running recomposers of this thread, as `Recomposer.runningRecomposers`.
pub fn running_recomposers() -> State<Vec<RecomposerInfo>> {
    let handle = RUNNING_INFO.with_borrow_mut(|info| {
        info.get_or_insert_with(|| mutable_state_of(running_list())).handle()
    });
    handle.as_state()
}

/// Runs `block` with a recomposer running, shutting it down after, as `withRunningRecomposer`.
pub async fn with_running_recomposer<R>(block: impl AsyncFnOnce(Recomposer) -> R) -> R {
    /// Gone with the task, cancelled or not.
    struct Shutdown(Owned<Recomposer>, Job);
    impl Drop for Shutdown {
        fn drop(&mut self) {
            self.0.cancel();
            self.1.cancel();
        }
    }
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let runner = launch(async move { recomposer.run_recompose_and_apply_changes().await });
    let shutdown = Shutdown(owner, runner);
    // Running by the time `block` starts, as it's a task of the same thread's next poll.
    crate::executor::poll_now(runner);
    let result = block(recomposer).await;
    recomposer.cancel();
    recomposer.join().await;
    drop(shutdown);
    result
}

/// Whether the task being polled must wait for a frame: an effect of a recomposer whose
/// compositions' clock is paused. It's woken when that's over.
pub(crate) fn frame_held_back(waker: &Waker) -> bool {
    let paused = PAUSED.with_borrow(|paused| (!paused.is_empty()).then(|| paused.clone()));
    let Some(paused) = paused else { return false };
    let Some(job) = crate::executor::current_job() else { return false };
    let holding = paused.iter().find(|recomposer| recomposer.is_effect(job));
    let Some(inner) = holding.and_then(|recomposer| recomposer.inner.try_get()) else {
        return false;
    };
    inner.paused_waiters.borrow_mut().push(waker.clone());
    true
}

/// The frame of `recomposer` at `frame_time_nanos`, as the body of Compose's `withFrameNanos`:
/// recomposes its invalid compositions, and those invalid by what they wrote until none is left,
/// each once, then applies them all, then runs the frame-end callbacks. Returns whether a
/// composition was composed.
pub fn perform_frame(recomposer: Recomposer, frame_time_nanos: u64) -> bool {
    let _ = frame_time_nanos;
    Snapshot::send_apply_notifications();
    let mut to_apply: Vec<Composed> = Vec::new();
    let mut panic = None;
    loop {
        let invalid = recomposer.invalid_compositions();
        if invalid.is_empty() {
            break;
        }
        for id in invalid {
            crate::pausable::run_requests(id);
            let Some(pass) = Pass::begin(id, Mode::Walk) else { continue };
            match catch_unwind(AssertUnwindSafe(|| run_walk(0))) {
                Ok(()) => to_apply.push(pass.finish_composing()),
                Err(caught) => {
                    // The pass's drop poisons it. What's composed is applied before it goes on.
                    drop(pass);
                    panic = Some(caught);
                    break;
                }
            }
        }
        if panic.is_some() {
            break;
        }
        Snapshot::send_apply_notifications();
    }
    let changed = !to_apply.is_empty();
    for pass in to_apply {
        if let Err(caught) = catch_unwind(AssertUnwindSafe(|| apply_work(pass))) {
            panic.get_or_insert(caught);
        }
    }
    let callbacks = recomposer
        .inner
        .try_get()
        .map(|inner| std::mem::take(&mut *inner.frame_end_callbacks.borrow_mut()))
        .unwrap_or_default();
    for (_, callback) in callbacks {
        callback();
    }
    if changed && let Some(inner) = recomposer.inner.try_get() {
        inner.change_count.set(inner.change_count.get() + 1);
    }
    Snapshot::send_apply_notifications();
    recomposer.update_state();
    if let Some(panic) = panic {
        resume_unwind(panic);
    }
    changed
}
