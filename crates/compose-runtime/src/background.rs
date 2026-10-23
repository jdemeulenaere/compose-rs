//! Blocking work off the UI thread, as Compose's `withContext(Dispatchers.IO) { … }`:
//! [`spawn_background`] runs a closure on a small pool of threads and gives a future the UI
//! thread's task awaits. The pool is std threads, created as work comes (at most
//! [`MAX_THREADS`], as `Dispatchers.IO`'s 64) and gone after a minute idle.
//!
//! The result comes back through the awaiting task's waker, which is `Send`
//! (docs/04-runtime.md, "Tasks"): the closure's thread stores it and wakes the task, the host
//! polls the task on the UI thread, and the task writes state there.
//!
//! # Awaiting another runtime's futures
//!
//! A tokio-based client (an HTTP client, a database driver) needs a tokio runtime: give it its
//! own thread and send each result back, awaited here as any background result:
//!
//! ```ignore
//! // Once, at startup: a runtime on its own thread.
//! let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
//! // In a task on the UI thread (a `LaunchedEffect`):
//! let handle = runtime.handle().clone();
//! let body = spawn_background(move || handle.block_on(client.get(url).send())).await;
//! ```
//!
//! `block_on` holds one of the pool's threads while the request runs. To hold none, spawn the
//! request on the runtime and await a one-shot channel its task sends the result on (tokio's
//! `sync::oneshot` works on any executor): the channel's receiver is a future the UI thread's
//! task awaits, woken from the runtime's thread.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

/// The most threads the pool runs at once, as `Dispatchers.IO`'s default limit; more work waits
/// for one to be free.
pub const MAX_THREADS: usize = 64;

/// How long an idle thread waits for work before it ends.
const KEEP_ALIVE: Duration = Duration::from_secs(60);

type Work = Box<dyn FnOnce() + Send>;

struct Pool {
    state: Mutex<PoolState>,
    available: Condvar,
}

#[derive(Default)]
struct PoolState {
    queue: VecDeque<Work>,
    threads: usize,
    idle: usize,
}

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| Pool { state: Mutex::default(), available: Condvar::new() })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs a job, its panics caught where panics unwind (desktop, tests): one escaping
/// `spawn_background`'s own catch (a result's or a capture's drop, a waker's `wake`) is reported
/// by the panic hook and goes no further, so the thread lives on and keeps its place in the
/// count. Under `panic = "abort"` (compose-rs's Android profiles, D361) nothing is caught: the
/// hook reports the panic and the process aborts on this thread.
fn run(work: Work) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
}

impl Pool {
    fn submit(&'static self, work: Work) {
        let mut state = lock(&self.state);
        if state.idle > state.queue.len() || state.threads >= MAX_THREADS {
            // An idle thread takes it, or the next one free.
            state.queue.push_back(work);
            drop(state);
            self.available.notify_one();
            return;
        }
        // A new thread, starting with this work.
        state.threads += 1;
        drop(state);
        let first = Arc::new(Mutex::new(Some(work)));
        let theirs = first.clone();
        let spawned =
            std::thread::Builder::new().name("compose-background".into()).spawn(move || {
                let work = lock(&theirs).take();
                drop(theirs);
                if let Some(work) = work {
                    run(work);
                }
                self.work();
            });
        if spawned.is_err() {
            // No thread to be had: the work waits for a running one or, with none, runs here
            // rather than never.
            let Some(work) = lock(&first).take() else { return };
            let mut state = lock(&self.state);
            state.threads -= 1;
            if state.threads == 0 {
                drop(state);
                run(work);
            } else {
                state.queue.push_back(work);
                drop(state);
                self.available.notify_one();
            }
        }
    }

    /// A thread's loop: work while there's some, ends after `KEEP_ALIVE` without.
    fn work(&self) {
        let mut state = lock(&self.state);
        loop {
            if let Some(work) = state.queue.pop_front() {
                drop(state);
                // Where panics unwind, a panicking closure's panic goes to its future (see
                // `spawn_background`) and the thread lives on; under abort, the process ends.
                run(work);
                state = lock(&self.state);
                continue;
            }
            state.idle += 1;
            let (next, timeout) = self
                .available
                .wait_timeout(state, KEEP_ALIVE)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
            state.idle -= 1;
            if timeout.timed_out() && state.queue.is_empty() {
                state.threads -= 1;
                return;
            }
        }
    }
}

/// What the closure's thread and the future share.
struct Shared<T> {
    /// The closure's result (or its panic), once it ran.
    result: Option<std::thread::Result<T>>,
    /// The task awaiting it, woken when it's there.
    waker: Option<Waker>,
    /// The future was dropped: the closure doesn't start, and a result is discarded.
    cancelled: bool,
}

/// Runs `work` on a background thread, as Compose's `withContext(Dispatchers.IO) { work() }`:
/// for blocking work (files, a blocking network call, decoding) a UI thread's task awaits
/// without holding the thread. The future completes on the thread polling it (the UI thread, in
/// a `LaunchedEffect` or a scope's `launch`), where its task goes on to write state.
///
/// Dropping the future (its task cancelled: the effect left the composition) doesn't block: work
/// not started yet never starts, and the result of work already running is dropped on its
/// thread when it ends (blocking work can't be interrupted, as in Compose, where `withContext`
/// can't cancel a blocking call either).
///
/// A panic in `work`, where panics unwind (desktop, tests), panics the awaiting task, or is
/// dropped with the result if the future was dropped. Under `panic = "abort"` (compose-rs's
/// Android profiles, D361), it aborts the process on the background thread, awaited or not,
/// after the panic hook reports it.
///
/// `work` and its result must be `Send`; state handles aren't, so read what it needs before and
/// write what it returns after:
///
/// ```ignore
/// LaunchedEffect(path.clone(), async move {
///     let text = spawn_background(move || std::fs::read_to_string(path)).await;
///     contents.set(text.unwrap_or_default());
/// });
/// ```
pub fn spawn_background<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Background<T> {
    let shared = Arc::new(Mutex::new(Shared { result: None, waker: None, cancelled: false }));
    let theirs = shared.clone();
    pool().submit(Box::new(move || {
        if lock(&theirs).cancelled {
            return;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        let mut state = lock(&theirs);
        if state.cancelled {
            // Nobody awaits it: dropped here, outside the lock.
            drop(state);
            drop(result);
            return;
        }
        state.result = Some(result);
        let waker = state.waker.take();
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
    }));
    Background { shared, done: false }
}

/// The future of [`spawn_background`]: the closure's result. Polling it again once it gave
/// its result panics, as std's futures do.
#[must_use = "the work's result is discarded unless awaited"]
pub struct Background<T> {
    shared: Arc<Mutex<Shared<T>>>,
    /// It gave its result (or its panic).
    done: bool,
}

impl<T> Future for Background<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<T> {
        assert!(!self.done, "compose-runtime: a `Background` polled after it completed");
        let mut state = lock(&self.shared);
        let Some(result) = state.result.take() else {
            // Under the lock, so the work storing its result sees this waker.
            if !state.waker.as_ref().is_some_and(|waker| waker.will_wake(context.waker())) {
                state.waker = Some(context.waker().clone());
            }
            return Poll::Pending;
        };
        drop(state);
        self.done = true;
        match result {
            Ok(value) => Poll::Ready(value),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

impl<T> Drop for Background<T> {
    fn drop(&mut self) {
        let mut state = lock(&self.shared);
        state.cancelled = true;
        state.waker = None;
        // A result not taken is dropped here, outside the lock.
        let result = state.result.take();
        drop(state);
        drop(result);
    }
}

impl<T> std::fmt::Debug for Background<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Background")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::task::Wake;

    struct CountingWaker(std::sync::atomic::AtomicUsize);

    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn poll<T>(future: &mut Background<T>, waker: &Waker) -> Poll<T> {
        Pin::new(future).poll(&mut Context::from_waker(waker))
    }

    #[test]
    fn the_result_comes_back_through_the_waker() {
        let (release, released) = mpsc::channel::<()>();
        let mut future = spawn_background(move || {
            released.recv().unwrap();
            std::thread::current().id()
        });
        let counter = Arc::new(CountingWaker(0.into()));
        let waker = Waker::from(counter.clone());
        assert!(poll(&mut future, &waker).is_pending());
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while counter.0.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(std::time::Instant::now() < deadline, "never woken");
            std::thread::sleep(Duration::from_millis(1));
        }
        match poll(&mut future, &waker) {
            Poll::Ready(thread) => assert_ne!(thread, std::thread::current().id()),
            Poll::Pending => panic!("woken without a result"),
        }
    }

    /// Tells where it was dropped.
    struct DropProbe(mpsc::Sender<std::thread::ThreadId>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            let _ = self.0.send(std::thread::current().id());
        }
    }

    #[test]
    fn dropping_the_future_does_not_block_and_discards_the_result_on_its_thread() {
        let (release, released) = mpsc::channel::<()>();
        let (started_sender, started) = mpsc::channel::<()>();
        let (dropped_sender, dropped) = mpsc::channel();
        let future = spawn_background(move || {
            started_sender.send(()).unwrap();
            released.recv().unwrap();
            DropProbe(dropped_sender)
        });
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        // The work is running, blocked: dropping its future returns at once.
        drop(future);
        release.send(()).unwrap();
        let thread = dropped.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_ne!(thread, std::thread::current().id());
    }

    #[test]
    fn work_whose_future_is_gone_before_it_starts_never_runs() {
        // Every thread busy, so the next work waits in the queue.
        let (release, released) = mpsc::channel::<()>();
        let released = Arc::new(Mutex::new(released));
        let (started_sender, started) = mpsc::channel::<()>();
        let blockers: Vec<_> = (0..MAX_THREADS)
            .map(|_| {
                let (released, started) = (released.clone(), started_sender.clone());
                spawn_background(move || {
                    started.send(()).unwrap();
                    let _ = lock(&released).recv();
                })
            })
            .collect();
        for _ in 0..MAX_THREADS {
            started.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        let (ran_sender, ran) = mpsc::channel::<()>();
        drop(spawn_background(move || ran_sender.send(()).unwrap()));
        for _ in 0..MAX_THREADS {
            release.send(()).unwrap();
        }
        drop(blockers);
        // The queued work was skipped: its sender dropped unsent.
        assert_eq!(
            ran.recv_timeout(Duration::from_secs(10)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
    }

    #[test]
    fn a_panic_in_the_work_panics_the_awaiting_task() {
        let mut future = spawn_background(|| -> u32 { panic!("work failed") });
        let waker = Waker::from(Arc::new(CountingWaker(0.into())));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let result = loop {
            let polled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                poll(&mut future, &waker)
            }));
            match polled {
                Ok(Poll::Pending) => {
                    assert!(std::time::Instant::now() < deadline, "never done");
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => break other,
            }
        };
        assert!(result.is_err());
    }
}
