//! Tasks on the UI thread: what `LaunchedEffect` and a coroutine scope's `launch` run, as
//! Compose's coroutines on its main dispatcher, with `delay` and the frame clock
//! (`with_frame_nanos`).
//!
//! The executor is per thread. Its tasks are futures polled on that thread, by the host, between
//! events and frames: [`run_ready`] polls the tasks woken since, [`dispatch_frame`] resumes the
//! tasks waiting for a frame (before the frame recomposes, so an animation's state is that
//! frame's). Wakers are `Send`: a background thread finishing work wakes a task, and the host's
//! wake handler ([`set_wake_handler`]) schedules a frame.
//!
//! Time is the executor's clock: real (`Instant`), or manual for tests ([`set_manual_clock`],
//! [`advance_clock`]), so `delay` and frame times are deterministic under a test rule.

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::future::Future;
use std::hash::BuildHasherDefault;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use crate::hash::KeyHasher;
use crate::state::{ObjectHandle, Owned};

type BoxedTask = Pin<Box<dyn Future<Output = ()>>>;

/// A task's slot: generational, so a waker of a finished task can't poll its successor.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct TaskKey {
    index: u32,
    generation: u32,
}

struct TaskSlot {
    generation: u32,
    /// `None` while it is being polled, or once it is finished.
    future: Option<BoxedTask>,
    live: bool,
    /// Cancelled while it was being polled: dropped once the poll returns.
    cancelled: bool,
    /// The waker its polls get, made at its first: each poll clones it rather than allocating
    /// one (a frame polls a task per animated value).
    waker: Option<Waker>,
}

/// What wakers reach from any thread.
struct Shared {
    ready: Mutex<Vec<TaskKey>>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    /// [`dispatch_frame`] is resuming frame awaiters and polls the ready tasks right after:
    /// a task scheduled meanwhile, from any thread, needs no turn from the host. Set by
    /// [`QuietWakes`] only.
    quiet: AtomicBool,
}

impl Shared {
    fn schedule(&self, key: TaskKey) {
        self.ready.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(key);
        // Read after the push: a `true` here means the dispatching thread hasn't cleared it
        // yet, so its `run_ready` (which takes `ready`'s lock after clearing it) sees the key.
        if self.quiet.load(Ordering::SeqCst) {
            return;
        }
        self.wake_host();
    }

    /// Calls the host's wake handler, if it has one.
    fn wake_host(&self) {
        let wake = self.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        if let Some(wake) = wake {
            wake();
        }
    }
}

/// While it lives, wakes of one executor's tasks don't call its host's wake handler (see
/// [`Shared::quiet`]). Only that executor's: another thread's tasks woken meanwhile wake their
/// own host. Dropped in an unwind (a waker panicked), it wakes the host if tasks are ready, as
/// the `run_ready` that would have polled them won't run.
struct QuietWakes<'a> {
    shared: &'a Shared,
    /// The flag as it was (a nested dispatch leaves it set).
    previous: bool,
}

impl<'a> QuietWakes<'a> {
    fn new(shared: &'a Shared) -> QuietWakes<'a> {
        QuietWakes { shared, previous: shared.quiet.swap(true, Ordering::SeqCst) }
    }
}

impl Drop for QuietWakes<'_> {
    fn drop(&mut self) {
        self.shared.quiet.store(self.previous, Ordering::SeqCst);
        if !self.previous
            && std::thread::panicking()
            && !self.shared.ready.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_empty()
        {
            self.shared.wake_host();
        }
    }
}

struct TaskWaker {
    key: TaskKey,
    shared: Arc<Shared>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.shared.schedule(self.key);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.shared.schedule(self.key);
    }
}

enum Clock {
    Real(Instant),
    Manual(u64),
}

struct Timer {
    deadline: u64,
    serial: u64,
    waker: Waker,
}

impl PartialEq for Timer {
    fn eq(&self, other: &Timer) -> bool {
        (self.deadline, self.serial) == (other.deadline, other.serial)
    }
}

impl Eq for Timer {}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Timer) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timer {
    fn cmp(&self, other: &Timer) -> std::cmp::Ordering {
        (self.deadline, self.serial).cmp(&(other.deadline, other.serial))
    }
}

/// A task waiting for the next frame: the frame's time once dispatched.
type FrameAwaiter = (Rc<Cell<Option<u64>>>, Waker);

struct Executor {
    tasks: Vec<TaskSlot>,
    free: Vec<u32>,
    shared: Arc<Shared>,
    clock: Clock,
    timers: BinaryHeap<Reverse<Timer>>,
    timer_serial: u64,
    frame_awaiters: Vec<FrameAwaiter>,
    /// Scratch for polling, kept for its capacity.
    polling: Vec<TaskKey>,
}

thread_local! {
    // Leaked at thread exit: dropping tasks then would run their drops (state owners among
    // them) after the runtime they use is gone.
    static EXECUTOR: RefCell<ManuallyDrop<Executor>> = RefCell::new(ManuallyDrop::new(Executor {
        tasks: Vec::new(),
        free: Vec::new(),
        shared: Arc::new(Shared {
            ready: Mutex::new(Vec::new()),
            wake: Mutex::new(None),
            quiet: AtomicBool::new(false),
        }),
        clock: Clock::Real(Instant::now()),
        timers: BinaryHeap::new(),
        timer_serial: 0,
        frame_awaiters: Vec::new(),
        polling: Vec::new(),
    }));
}

/// Runs `f` with the executor borrowed. `f` must not run user code (polls, drops of futures).
fn with_executor<R>(f: impl FnOnce(&mut Executor) -> R) -> R {
    EXECUTOR.with(|executor| f(&mut executor.borrow_mut()))
}

impl Executor {
    fn now(&self) -> u64 {
        match self.clock {
            Clock::Real(start) => start.elapsed().as_nanos() as u64,
            Clock::Manual(now) => now,
        }
    }

    fn slot(&mut self, key: TaskKey) -> Option<&mut TaskSlot> {
        self.tasks.get_mut(key.index as usize).filter(|slot| slot.generation == key.generation)
    }
}

/// Starts `future` as a task on this thread's executor; it first runs at the next
/// [`run_ready`]. Returns its key, to cancel it.
fn spawn(future: BoxedTask) -> TaskKey {
    let (key, shared) = with_executor(|executor| {
        let key = match executor.free.pop() {
            Some(index) => {
                let slot = &mut executor.tasks[index as usize];
                slot.generation = slot.generation.wrapping_add(1);
                slot.future = Some(future);
                slot.live = true;
                slot.cancelled = false;
                slot.waker = None;
                TaskKey { index, generation: slot.generation }
            }
            None => {
                let index = u32::try_from(executor.tasks.len()).expect("too many tasks");
                executor.tasks.push(TaskSlot {
                    generation: 0,
                    future: Some(future),
                    live: true,
                    cancelled: false,
                    waker: None,
                });
                TaskKey { index, generation: 0 }
            }
        };
        (key, executor.shared.clone())
    });
    shared.schedule(key);
    key
}

/// Cancels a task: its future is dropped (at once, or after its poll if it is running).
fn cancel(key: TaskKey) {
    let dropped = with_executor(|executor| {
        let slot = executor.slot(key)?;
        if !slot.live {
            return None;
        }
        match slot.future.take() {
            Some(future) => {
                slot.live = false;
                slot.waker = None;
                executor.free.push(key.index);
                Some(future)
            }
            None => {
                // Being polled: dropped when the poll returns.
                slot.cancelled = true;
                None
            }
        }
    });
    // Its drops (user code) run outside the borrow.
    drop(dropped);
}

fn is_live(key: TaskKey) -> bool {
    with_executor(|executor| executor.slot(key).is_some_and(|slot| slot.live && !slot.cancelled))
}

/// Polls the tasks woken since the last call, and the ones they wake meanwhile, until none is
/// ready. For hosts: between events and frames. A task woken again after its poll here (one
/// that yields) waits for the next call, so it can't hold the thread: frames and input come
/// between its steps, as Compose's `yield()`. Returns whether a task ran.
pub fn run_ready() -> bool {
    // A turn, if any task is ready: what the tasks write is sent when it ends (D751).
    let mut turn = None;
    let mut ran = false;
    // A set: a frame resumes hundreds of tasks (an animated value each), and a scan per key
    // made that quadratic; the first few in an array, so a run polling a task or two (an
    // event's) allocates nothing.
    let mut polled = Polled::default();
    let mut deferred: Vec<TaskKey> = Vec::new();
    loop {
        let mut keys = with_executor(|executor| {
            let mut keys = std::mem::take(&mut executor.polling);
            keys.append(
                &mut executor.shared.ready.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
            keys
        });
        if keys.is_empty() {
            with_executor(|executor| executor.polling = keys);
            if !deferred.is_empty() {
                let shared = with_executor(|executor| executor.shared.clone());
                shared
                    .ready
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .append(&mut deferred);
            }
            // No task ran: a write outside any turn still has this one to send it (D755).
            if turn.is_none() {
                crate::snapshots::send_unsent_outside_turns();
            }
            drop(turn);
            crate::snapshots::flush_derived_outside_turns();
            return ran;
        }
        turn.get_or_insert_with(crate::snapshots::Turn::begin);
        for &key in &keys {
            if !polled.insert(key) {
                if !deferred.contains(&key) {
                    deferred.push(key);
                }
                continue;
            }
            ran |= poll_task(key);
        }
        keys.clear();
        with_executor(|executor| executor.polling = keys);
    }
}

/// The tasks a `run_ready` polled: an array for the first few, a set past them.
#[derive(Default)]
struct Polled {
    few: [Option<TaskKey>; 8],
    many: Option<HashSet<TaskKey, BuildHasherDefault<KeyHasher>>>,
}

impl Polled {
    /// Notes `key`; whether it wasn't yet.
    fn insert(&mut self, key: TaskKey) -> bool {
        if let Some(many) = &mut self.many {
            return many.insert(key);
        }
        for slot in &mut self.few {
            match slot {
                Some(kept) if *kept == key => return false,
                Some(_) => {}
                None => {
                    *slot = Some(key);
                    return true;
                }
            }
        }
        let mut many: HashSet<TaskKey, BuildHasherDefault<KeyHasher>> =
            self.few.iter().flatten().copied().collect();
        let inserted = many.insert(key);
        self.many = Some(many);
        inserted
    }
}

/// Polls task `key` once, if it's live and not being polled; returns whether it was.
fn poll_task(key: TaskKey) -> bool {
    let polled = with_executor(|executor| {
        let shared = &executor.shared;
        let slot = executor
            .tasks
            .get_mut(key.index as usize)
            .filter(|slot| slot.generation == key.generation && slot.live)?;
        let future = slot.future.take()?;
        let waker = slot.waker.get_or_insert_with(|| {
            Waker::from(Arc::new(TaskWaker { key, shared: shared.clone() }))
        });
        Some((future, waker.clone()))
    });
    let Some((mut future, waker)) = polled else { return false };
    let mut context = Context::from_waker(&waker);
    // The task being polled, for [`cancel_current_task`]; restored after (a nested `poll_now`).
    let previous = CURRENT_TASK.with(|current| current.replace(Some(key)));
    let poll = future.as_mut().poll(&mut context);
    CURRENT_TASK.with(|current| current.set(previous));
    let done = with_executor(move |executor| {
        let Some(slot) = executor.slot(key) else { return Some(future) };
        if poll.is_ready() || slot.cancelled {
            slot.live = false;
            slot.cancelled = false;
            slot.waker = None;
            executor.free.push(key.index);
            Some(future)
        } else {
            slot.future = Some(future);
            None
        }
    });
    // A finished or cancelled task's drops (user code) run outside the borrow.
    drop(done);
    true
}

thread_local! {
    /// The task [`poll_task`] is polling.
    static CURRENT_TASK: Cell<Option<TaskKey>> = const { Cell::new(None) };
    /// What the test environment says of infinite animations ([`set_infinite_animation_policy`]).
    static INFINITE_ANIMATION_POLICY: RefCell<Option<Rc<dyn Fn() -> bool>>> =
        const { RefCell::new(None) };
}

/// Cancels the task being polled, as throwing a `CancellationException` in a coroutine: its
/// future is dropped once this poll returns. Outside a task, nothing. For libraries whose
/// suspending calls end their caller as Compose's throw does (D901, D823).
pub fn cancel_current_task() {
    if let Some(key) = CURRENT_TASK.with(Cell::get) {
        cancel(key);
    }
}

/// Sets this thread's policy for infinite animations, as an `InfiniteAnimationPolicy` in the
/// coroutine context of Compose's test environment: before each frame an infinite animation
/// waits for ([`with_infinite_animation_frame_nanos`]), `allows()` says whether it may run; when
/// it says no, the task waiting is cancelled, as the test policy's `CancellationException`.
/// `None` (the default): they run. For test rules (D901).
pub fn set_infinite_animation_policy(allows: Option<Rc<dyn Fn() -> bool>>) {
    INFINITE_ANIMATION_POLICY.with(|policy| *policy.borrow_mut() = allows);
}

/// The policy [`set_infinite_animation_policy`] set last, to restore it.
pub fn infinite_animation_policy() -> Option<Rc<dyn Fn() -> bool>> {
    INFINITE_ANIMATION_POLICY.with(|policy| policy.borrow().clone())
}

/// Like [`with_frame_nanos`], for an animation that never ends (an infinite transition's, an
/// `infinite_repeatable`'s), as Compose's `withInfiniteAnimationFrameNanos`: under a policy that
/// stops infinite animations (a test rule advancing its clock by itself), the task is cancelled
/// instead and the frame never comes.
pub async fn with_infinite_animation_frame_nanos<R>(on_frame: impl FnOnce(u64) -> R) -> R {
    let allowed = INFINITE_ANIMATION_POLICY
        .with(|policy| policy.borrow().clone())
        .is_none_or(|allows| allows());
    if !allowed {
        cancel_current_task();
        return std::future::pending().await;
    }
    with_frame_nanos(on_frame).await
}

/// Like [`with_infinite_animation_frame_nanos`], with the time in milliseconds, as
/// `withInfiniteAnimationFrameMillis`.
pub async fn with_infinite_animation_frame_millis<R>(on_frame: impl FnOnce(u64) -> R) -> R {
    with_infinite_animation_frame_nanos(move |nanos| on_frame(nanos / 1_000_000)).await
}

/// The job of the task being polled now, if a task is: a future can cancel the task awaiting
/// it, as a coroutine's suspension resumed with a `CancellationException` cancels it.
pub fn current_job() -> Option<Job> {
    CURRENT_TASK.try_with(Cell::get).ok().flatten().map(|key| Job { key })
}

/// Polls `job` now, on this thread, if it's waiting (not finished, nor being polled): for
/// handlers whose task must see an event before the dispatch goes on (`pointer_input`, as
/// Compose resumes it undispatched). Returns whether it was polled.
pub fn poll_now(job: Job) -> bool {
    poll_task(job.key)
}

/// Wakes the tasks whose `delay` is over, by the executor's clock. For hosts, before
/// [`run_ready`]. Returns whether one was woken.
pub fn fire_timers() -> bool {
    let mut due = Vec::new();
    with_executor(|executor| {
        let now = executor.now();
        while executor.timers.peek().is_some_and(|Reverse(timer)| timer.deadline <= now) {
            if let Some(Reverse(timer)) = executor.timers.pop() {
                due.push(timer.waker);
            }
        }
    });
    let fired = !due.is_empty();
    for waker in due {
        waker.wake();
    }
    fired
}

/// Resumes the tasks waiting for a frame (`with_frame_nanos`) with this frame's time, and polls
/// them. For hosts: at the start of a frame, before it recomposes. Returns the frame's time.
///
/// The tasks it wakes don't ask the host for a turn (its wake handler): they're polled here.
pub fn dispatch_frame() -> u64 {
    let (now, awaiters, shared) = with_executor(|executor| {
        (executor.now(), std::mem::take(&mut executor.frame_awaiters), executor.shared.clone())
    });
    {
        let _quiet = QuietWakes::new(&shared);
        let mut awaiters = UnwokenAwaiters(awaiters.into_iter());
        for (time, waker) in awaiters.0.by_ref() {
            time.set(Some(now));
            waker.wake();
        }
    }
    run_ready();
    now
}

/// The frame awaiters [`dispatch_frame`] is waking. Dropped in an unwind (a waker panicked),
/// the ones not woken yet wait again, for the next frame, which the host is asked for.
struct UnwokenAwaiters(std::vec::IntoIter<FrameAwaiter>);

impl Drop for UnwokenAwaiters {
    fn drop(&mut self) {
        if !std::thread::panicking() || self.0.len() == 0 {
            return;
        }
        let rest: Vec<FrameAwaiter> = self.0.by_ref().collect();
        let shared = EXECUTOR.try_with(|executor| {
            let mut executor = executor.try_borrow_mut().ok()?;
            executor.frame_awaiters.extend(rest);
            Some(executor.shared.clone())
        });
        if let Ok(Some(shared)) = shared {
            shared.wake_host();
        }
    }
}

/// Whether a task waits for a frame: the host should run one.
pub fn has_frame_awaiters() -> bool {
    with_executor(|executor| !executor.frame_awaiters.is_empty())
}

/// Whether a task is ready to run.
pub fn has_ready_tasks() -> bool {
    with_executor(|executor| {
        !executor.shared.ready.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_empty()
    })
}

/// How long until the next `delay` ends, if one is pending: when the host should wake.
pub fn next_timer() -> Option<Duration> {
    with_executor(|executor| {
        let now = executor.now();
        executor
            .timers
            .peek()
            .map(|Reverse(timer)| Duration::from_nanos(timer.deadline.saturating_sub(now)))
    })
}

/// Called (from any thread) when a task becomes ready or waits for a frame: the host schedules
/// a frame or a turn of its loop.
pub fn set_wake_handler(wake: impl Fn() + Send + Sync + 'static) {
    let shared = with_executor(|executor| executor.shared.clone());
    *shared.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(wake));
}

/// Asks the host for a turn of its loop (its wake handler, if set): state changed that only a
/// frame tells readers about (a derived state's dependency).
pub(crate) fn wake_host() {
    let shared = EXECUTOR
        .try_with(|executor| executor.try_borrow().ok().map(|executor| executor.shared.clone()));
    if let Ok(Some(shared)) = shared {
        shared.wake_host();
    }
}

/// The executor's time in nanoseconds: since the thread first used it, or the manual clock's.
pub fn now_nanos() -> u64 {
    with_executor(|executor| executor.now())
}

/// Switches this thread's clock to a manual one at `nanos`, as a test's main clock: time moves
/// only with [`advance_clock`].
pub fn set_manual_clock(nanos: u64) {
    with_executor(|executor| executor.clock = Clock::Manual(nanos));
}

/// Moves the manual clock forward (nothing with a real clock).
pub fn advance_clock(by: Duration) {
    with_executor(|executor| {
        if let Clock::Manual(now) = &mut executor.clock {
            *now = now.saturating_add(by.as_nanos() as u64);
        }
    });
}

/// When the next `delay` ends, by the manual clock: what a test's clock advances to when only
/// timers are left.
pub fn next_timer_deadline() -> Option<u64> {
    with_executor(|executor| executor.timers.peek().map(|Reverse(timer)| timer.deadline))
}

// ---- Futures ---------------------------------------------------------------------------------

/// Suspends the task for `duration` by the executor's clock, as Compose's `delay`. Dropping it
/// (the task cancelled) removes its timer.
pub fn delay(duration: Duration) -> Delay {
    Delay { duration, deadline: None, timer: None }
}

/// The future of [`delay`].
pub struct Delay {
    duration: Duration,
    deadline: Option<u64>,
    /// The serial of the timer it waits on, if any.
    timer: Option<u64>,
}

impl Future for Delay {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let duration = self.duration;
        let deadline = self.deadline;
        let timer = self.timer;
        let (ready, deadline, timer, wake) = with_executor(|executor| {
            let now = executor.now();
            let deadline =
                deadline.unwrap_or_else(|| now.saturating_add(duration.as_nanos() as u64));
            // Polled again: its timer is replaced (with this poll's waker), never doubled.
            if let Some(serial) = timer {
                executor.timers.retain(|Reverse(timer)| timer.serial != serial);
            }
            if now >= deadline {
                return (true, deadline, None, None);
            }
            // A new timer ending first: the host hears it (it may sleep until the one it knew).
            let earliest = timer.is_none()
                && executor.timers.peek().is_none_or(|Reverse(next)| deadline < next.deadline);
            executor.timer_serial += 1;
            let serial = executor.timer_serial;
            executor.timers.push(Reverse(Timer {
                deadline,
                serial,
                waker: context.waker().clone(),
            }));
            let wake = if earliest {
                executor.shared.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
            } else {
                None
            };
            (false, deadline, Some(serial), wake)
        });
        self.deadline = Some(deadline);
        self.timer = timer;
        if let Some(wake) = wake {
            wake();
        }
        if ready { Poll::Ready(()) } else { Poll::Pending }
    }
}

impl Drop for Delay {
    fn drop(&mut self) {
        if let Some(serial) = self.timer {
            let _ = EXECUTOR.try_with(|executor| {
                if let Ok(mut executor) = executor.try_borrow_mut() {
                    executor.timers.retain(|Reverse(timer)| timer.serial != serial);
                }
            });
        }
    }
}

/// Suspends the task until the next frame, then calls `on_frame` with the frame's time in
/// nanoseconds, as Compose's `withFrameNanos`. Awaiting it asks the host for a frame.
pub fn with_frame_nanos<R>(on_frame: impl FnOnce(u64) -> R) -> WithFrame<impl FnOnce(u64) -> R> {
    WithFrame { on_frame: Some(on_frame), time: None }
}

/// Like [`with_frame_nanos`], with the time in milliseconds.
pub fn with_frame_millis<R>(on_frame: impl FnOnce(u64) -> R) -> WithFrame<impl FnOnce(u64) -> R> {
    with_frame_nanos(move |nanos| on_frame(nanos / 1_000_000))
}

/// The future of [`with_frame_nanos`].
pub struct WithFrame<F> {
    on_frame: Option<F>,
    time: Option<Rc<Cell<Option<u64>>>>,
}

impl<F> Unpin for WithFrame<F> {}

impl<R, F: FnOnce(u64) -> R> Future for WithFrame<F> {
    type Output = R;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<R> {
        if let Some(time) = self.time.as_ref().and_then(|time| time.get()) {
            // Resumed: no longer an awaiter, nothing for its drop to remove.
            self.time = None;
            let on_frame = self.on_frame.take().expect("polled after it completed");
            return Poll::Ready(on_frame(time));
        }
        assert!(self.on_frame.is_some(), "polled after it completed");
        // Polled again while waiting: its registration is replaced (with this poll's waker).
        let registered = self.time.is_some();
        let time = self.time.get_or_insert_with(|| Rc::new(Cell::new(None))).clone();
        let shared = with_executor(|executor| {
            if registered {
                executor.frame_awaiters.retain(|(awaiting, _)| !Rc::ptr_eq(awaiting, &time));
            }
            // The first awaiter since the last frame asks the host for the next; the host
            // already runs it for the others (it runs frames while `has_frame_awaiters`).
            let first = executor.frame_awaiters.is_empty();
            executor.frame_awaiters.push((time, context.waker().clone()));
            first.then(|| executor.shared.clone())
        });
        if let Some(shared) = shared {
            shared.wake_host();
        }
        Poll::Pending
    }
}

impl<F> Drop for WithFrame<F> {
    /// A frame no longer awaited (the task cancelled) isn't asked for.
    fn drop(&mut self) {
        if let Some(time) = &self.time {
            let _ = EXECUTOR.try_with(|executor| {
                if let Ok(mut executor) = executor.try_borrow_mut() {
                    executor.frame_awaiters.retain(|(awaiting, _)| !Rc::ptr_eq(awaiting, time));
                }
            });
        }
    }
}

// ---- Jobs and scopes -------------------------------------------------------------------------

/// A launched task, as Compose's `Job`: cancel it, or ask whether it is still running.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Job {
    key: TaskKey,
}

impl Job {
    /// Cancels the task: its future is dropped, as a coroutine cancelled at its suspension.
    pub fn cancel(&self) {
        cancel(self.key);
    }

    /// Whether the task hasn't finished nor been cancelled.
    pub fn is_active(&self) -> bool {
        is_live(self.key)
    }
}

/// Starts `future` as a task of no scope (an effect's, a modifier node's), as `launch` on a scope of
/// its own, without the arena object a [`CoroutineScope`] is: cancel it through its [`Job`].
pub fn launch(future: impl Future<Output = ()> + 'static) -> Job {
    Job { key: spawn(Box::pin(future)) }
}

/// Where tasks are launched, as Compose's `CoroutineScope`: a scope from
/// [`remember_coroutine_scope`] cancels its tasks when it leaves the composition.
///
/// A `Copy` handle (D240), as a state handle: its owner is the `remember` slot (or the
/// [`Owned`] that [`new`](Self::new) returns). A forgotten scope is cancelled, and once freed
/// it stays a cancelled scope: launching in it starts nothing, as in Kotlin.
///
/// [`remember_coroutine_scope`]: crate::remember_coroutine_scope
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CoroutineScope {
    inner: ObjectHandle<ScopeInner>,
}

struct ScopeInner {
    jobs: RefCell<Vec<Job>>,
    cancelled: Cell<bool>,
}

impl CoroutineScope {
    /// A scope of its own, cancelled only by [`cancel`](Self::cancel), owned by the result:
    /// launch through it, or copy its handle out (`*scope`) while it lives. Dropping it doesn't
    /// cancel the tasks it launched.
    #[track_caller]
    pub fn new() -> Owned<CoroutineScope> {
        let inner = ScopeInner { jobs: RefCell::new(Vec::new()), cancelled: Cell::new(false) };
        Owned::new(inner, |inner| CoroutineScope { inner })
    }

    /// Starts `future` as a task of this scope, as `scope.launch { … }`. In a cancelled (or
    /// freed) scope it never runs: the job is already over.
    pub fn launch(&self, future: impl Future<Output = ()> + 'static) -> Job {
        let inner = self.inner.try_get().filter(|inner| !inner.cancelled.get());
        let Some(inner) = inner else {
            // A job that is already over.
            let key = spawn(Box::pin(async {}));
            cancel(key);
            return Job { key };
        };
        let job = Job { key: spawn(Box::pin(future)) };
        let mut jobs = inner.jobs.borrow_mut();
        jobs.retain(Job::is_active);
        jobs.push(job);
        job
    }

    /// Cancels the scope's tasks, and those it launches later. A freed scope has nothing left
    /// to cancel.
    pub fn cancel(&self) {
        let Some(inner) = self.inner.try_get() else { return };
        inner.cancelled.set(true);
        let jobs = std::mem::take(&mut *inner.jobs.borrow_mut());
        for job in jobs {
            job.cancel();
        }
    }

    /// Whether the scope is still active (not cancelled, nor freed).
    pub fn is_active(&self) -> bool {
        self.inner.try_get().is_some_and(|inner| !inner.cancelled.get())
    }
}

impl std::fmt::Debug for CoroutineScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CoroutineScope")
    }
}

impl crate::ComposeArg for CoroutineScope {
    fn same(&self, other: &CoroutineScope) -> bool {
        self == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Counts the host's wakes on this thread's executor.
    fn count_wakes() -> Arc<AtomicUsize> {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = wakes.clone();
        set_wake_handler(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        wakes
    }

    /// A waker that runs `on_wake`.
    struct CallbackWaker<F: Fn() + Send + Sync + 'static>(F);

    impl<F: Fn() + Send + Sync + 'static> Wake for CallbackWaker<F> {
        fn wake(self: Arc<Self>) {
            (self.0)();
        }
    }

    /// A frame awaiter registered with `waker` rather than a task's: the future, kept pending.
    fn await_frame_with(waker: Waker) -> Pin<Box<WithFrame<impl FnOnce(u64)>>> {
        let mut future = Box::pin(with_frame_nanos(|_| {}));
        let poll = future.as_mut().poll(&mut Context::from_waker(&waker));
        assert!(poll.is_pending());
        future
    }

    /// `dispatch_frame` polls the tasks it resumes right away: their wakes don't ask the host
    /// for a turn (on Android, a JNI upcall, a posted message and a `tick` per frame).
    #[test]
    fn tasks_resumed_by_a_frame_dont_wake_the_host() {
        let wakes = count_wakes();
        let frames = Rc::new(Cell::new(0));
        let counted = frames.clone();
        let _job = launch(async move {
            loop {
                with_frame_nanos(|_| ()).await;
                counted.set(counted.get() + 1);
            }
        });
        run_ready();
        wakes.store(0, Ordering::SeqCst);
        dispatch_frame();
        assert_eq!(frames.get(), 1);
        // Only the task's next await asks for a frame.
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        // Wakes outside a frame's dispatch still reach the host.
        let _other = launch(async {});
        assert_eq!(wakes.load(Ordering::SeqCst), 2);
    }

    /// Many tasks waiting for a frame ask the host for it once: on Android each wake is a JNI
    /// upcall, and an animation frame's tasks all wait for the next frame.
    #[test]
    fn many_frame_awaiters_ask_the_host_once() {
        let wakes = count_wakes();
        let jobs: Vec<Job> = (0..5)
            .map(|_| {
                launch(async {
                    loop {
                        with_frame_nanos(|_| ()).await;
                    }
                })
            })
            .collect();
        run_ready();
        assert_eq!(wakes.load(Ordering::SeqCst), 6, "5 launches, then 1 frame asked for");
        wakes.store(0, Ordering::SeqCst);
        dispatch_frame();
        assert_eq!(wakes.load(Ordering::SeqCst), 1, "the next frame, asked for once");
        // An awaiter gone (cancelled) and back: the host still hears of the first one after the
        // list was empty.
        for job in &jobs {
            job.cancel();
        }
        assert!(!has_frame_awaiters());
        wakes.store(0, Ordering::SeqCst);
        let _job = launch(async {
            with_frame_nanos(|_| ()).await;
        });
        run_ready();
        assert_eq!(wakes.load(Ordering::SeqCst), 2, "the launch, then its frame");
    }

    /// A task waking itself while it waits for a frame (polled again, still waiting) is resumed
    /// once by the frame, and polled once per `run_ready`.
    #[test]
    fn a_frame_awaiter_polled_again_waits_once() {
        let polls = Rc::new(Cell::new(0));
        let frames = Rc::new(Cell::new(0));
        let (counted_polls, counted_frames) = (polls.clone(), frames.clone());
        let _job = launch(async move {
            let mut frame = std::pin::pin!(with_frame_nanos(|_| ()));
            std::future::poll_fn(|context| {
                counted_polls.set(counted_polls.get() + 1);
                let poll = frame.as_mut().poll(context);
                if poll.is_pending() && counted_polls.get() == 1 {
                    context.waker().wake_by_ref();
                }
                poll
            })
            .await;
            counted_frames.set(counted_frames.get() + 1);
        });
        run_ready();
        assert_eq!(polls.get(), 1, "a task woken during its poll runs at the next run_ready");
        run_ready();
        assert_eq!(polls.get(), 2);
        assert!(has_frame_awaiters());
        dispatch_frame();
        assert_eq!((polls.get(), frames.get()), (3, 1));
        assert!(!has_frame_awaiters());
    }

    /// Polling a completed `with_frame_nanos` again panics there, not at the next frame.
    #[test]
    #[should_panic(expected = "polled after it completed")]
    fn a_completed_frame_wait_polled_again_panics_at_once() {
        let mut future = Box::pin(with_frame_nanos(|_| ()));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(future.as_mut().poll(&mut context).is_pending());
        dispatch_frame();
        assert!(future.as_mut().poll(&mut context).is_ready());
        let _ = future.as_mut().poll(&mut context);
    }

    /// A frame awaiter's waker that panics leaves the executor waking the host as before: the
    /// tasks resumed before it get a turn from the host (the frame's `run_ready` never ran), and
    /// those after it wait for the next frame.
    #[test]
    fn a_panicking_frame_awaiter_leaves_wakes_reaching_the_host() {
        let wakes = count_wakes();
        let awaiting = |resumed: Rc<Cell<bool>>| {
            launch(async move {
                with_frame_nanos(|_| ()).await;
                resumed.set(true);
            })
        };
        let (before, after) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let _first = awaiting(before.clone());
        run_ready();
        let _panicking =
            await_frame_with(Waker::from(Arc::new(CallbackWaker(|| panic!("a waker panics")))));
        let _last = awaiting(after.clone());
        run_ready();
        wakes.store(0, Ordering::SeqCst);
        let result = std::panic::catch_unwind(dispatch_frame);
        assert!(result.is_err());
        assert!(wakes.load(Ordering::SeqCst) >= 1, "the resumed task's wake reached the host");
        assert!(has_ready_tasks());
        run_ready();
        assert!(before.get());
        assert!(!after.get());
        assert!(has_frame_awaiters(), "the awaiter after the panicking one waits again");
        dispatch_frame();
        assert!(after.get());
        wakes.store(0, Ordering::SeqCst);
        let _other = launch(async {});
        assert_eq!(wakes.load(Ordering::SeqCst), 1, "wakes reach the host again");
    }

    /// Only the dispatching executor's wakes are quiet: a frame awaiter on another thread that
    /// wakes this thread's task wakes this thread's host.
    #[test]
    fn another_threads_frame_wakes_this_threads_host() {
        let wakes = count_wakes();
        let parked: Arc<Mutex<Option<Waker>>> = Arc::new(Mutex::new(None));
        let parking = parked.clone();
        let _job = launch(std::future::poll_fn(move |context| {
            *parking.lock().unwrap() = Some(context.waker().clone());
            Poll::<()>::Pending
        }));
        run_ready();
        let waker = parked.lock().unwrap().take().expect("the task waits");
        wakes.store(0, Ordering::SeqCst);
        std::thread::spawn(move || {
            let waker = std::sync::Mutex::new(Some(waker));
            let _awaiter = await_frame_with(Waker::from(Arc::new(CallbackWaker(move || {
                if let Some(waker) = waker.lock().unwrap().take() {
                    waker.wake();
                }
            }))));
            dispatch_frame();
        })
        .join()
        .unwrap();
        assert_eq!(wakes.load(Ordering::SeqCst), 1, "this thread's host heard its task's wake");
        assert!(has_ready_tasks());
    }
}
