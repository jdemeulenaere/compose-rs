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
use std::collections::BinaryHeap;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

type BoxedTask = Pin<Box<dyn Future<Output = ()>>>;

/// A task's slot: generational, so a waker of a finished task can't poll its successor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
}

/// What wakers reach from any thread.
struct Shared {
    ready: Mutex<Vec<TaskKey>>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Shared {
    fn schedule(&self, key: TaskKey) {
        self.ready.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(key);
        let wake = self.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        if let Some(wake) = wake {
            wake();
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
        shared: Arc::new(Shared { ready: Mutex::new(Vec::new()), wake: Mutex::new(None) }),
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
                TaskKey { index, generation: slot.generation }
            }
            None => {
                let index = u32::try_from(executor.tasks.len()).expect("too many tasks");
                executor.tasks.push(TaskSlot {
                    generation: 0,
                    future: Some(future),
                    live: true,
                    cancelled: false,
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
    let mut ran = false;
    let mut polled: Vec<TaskKey> = Vec::new();
    let mut deferred: Vec<TaskKey> = Vec::new();
    loop {
        let (mut keys, shared) = with_executor(|executor| {
            let mut keys = std::mem::take(&mut executor.polling);
            keys.append(
                &mut executor.shared.ready.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
            (keys, executor.shared.clone())
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
            return ran;
        }
        for &key in &keys {
            if polled.contains(&key) {
                if !deferred.contains(&key) {
                    deferred.push(key);
                }
                continue;
            }
            polled.push(key);
            ran |= poll_task(key, &shared);
        }
        keys.clear();
        with_executor(|executor| executor.polling = keys);
    }
}

/// Polls task `key` once, if it's live and not being polled; returns whether it was.
fn poll_task(key: TaskKey, shared: &Arc<Shared>) -> bool {
    let future = with_executor(|executor| {
        let slot = executor.slot(key)?;
        if !slot.live {
            return None;
        }
        slot.future.take()
    });
    let Some(mut future) = future else { return false };
    let waker = Waker::from(Arc::new(TaskWaker { key, shared: shared.clone() }));
    let mut context = Context::from_waker(&waker);
    let poll = future.as_mut().poll(&mut context);
    let done = with_executor(move |executor| {
        let Some(slot) = executor.slot(key) else { return Some(future) };
        if poll.is_ready() || slot.cancelled {
            slot.live = false;
            slot.cancelled = false;
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

/// Polls `job` now, on this thread, if it's waiting (not finished, nor being polled): for
/// handlers whose task must see an event before the dispatch goes on (`pointer_input`, as
/// Compose resumes it undispatched). Returns whether it was polled.
pub fn poll_now(job: Job) -> bool {
    let shared = with_executor(|executor| executor.shared.clone());
    poll_task(job.key, &shared)
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
pub fn dispatch_frame() -> u64 {
    let (now, awaiters) =
        with_executor(|executor| (executor.now(), std::mem::take(&mut executor.frame_awaiters)));
    for (time, waker) in awaiters {
        time.set(Some(now));
        waker.wake();
    }
    run_ready();
    now
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
        let wake = shared.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        if let Some(wake) = wake {
            wake();
        }
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
            let on_frame = self.on_frame.take().expect("polled after it completed");
            return Poll::Ready(on_frame(time));
        }
        let time = self.time.get_or_insert_with(|| Rc::new(Cell::new(None))).clone();
        let shared = with_executor(|executor| {
            executor.frame_awaiters.retain(|(awaiting, _)| !Rc::ptr_eq(awaiting, &time));
            executor.frame_awaiters.push((time, context.waker().clone()));
            executor.shared.clone()
        });
        // The host runs a frame.
        let wake = shared.wake.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        if let Some(wake) = wake {
            wake();
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

/// Where tasks are launched, as Compose's `CoroutineScope`: a scope from
/// [`remember_coroutine_scope`] cancels its tasks when it leaves the composition.
#[derive(Clone)]
pub struct CoroutineScope {
    inner: Rc<ScopeInner>,
}

struct ScopeInner {
    jobs: RefCell<Vec<Job>>,
    cancelled: Cell<bool>,
}

impl CoroutineScope {
    /// A scope of its own, cancelled only by [`cancel`](Self::cancel).
    pub fn new() -> CoroutineScope {
        CoroutineScope {
            inner: Rc::new(ScopeInner {
                jobs: RefCell::new(Vec::new()),
                cancelled: Cell::new(false),
            }),
        }
    }

    /// Starts `future` as a task of this scope, as `scope.launch { … }`. In a cancelled scope it
    /// never runs.
    pub fn launch(&self, future: impl Future<Output = ()> + 'static) -> Job {
        if self.inner.cancelled.get() {
            // A job that is already over.
            let key = spawn(Box::pin(async {}));
            cancel(key);
            return Job { key };
        }
        let job = Job { key: spawn(Box::pin(future)) };
        let mut jobs = self.inner.jobs.borrow_mut();
        jobs.retain(Job::is_active);
        jobs.push(job);
        job
    }

    /// Cancels the scope's tasks, and those it launches later.
    pub fn cancel(&self) {
        self.inner.cancelled.set(true);
        let jobs = std::mem::take(&mut *self.inner.jobs.borrow_mut());
        for job in jobs {
            job.cancel();
        }
    }

    /// Whether the scope is still active.
    pub fn is_active(&self) -> bool {
        !self.inner.cancelled.get()
    }
}

impl Default for CoroutineScope {
    fn default() -> CoroutineScope {
        CoroutineScope::new()
    }
}

impl std::fmt::Debug for CoroutineScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CoroutineScope")
    }
}

impl crate::ComposeArg for CoroutineScope {
    fn same(&self, other: &CoroutineScope) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}
