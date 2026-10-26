//! Frame clocks, as Compose's `MonotonicFrameClock`, `BroadcastFrameClock` and
//! `PausableMonotonicFrameClock`: a source of frames tasks await. The UI thread's executor has
//! one ([`ExecutorFrameClock`], what [`with_frame_nanos`](crate::with_frame_nanos) awaits, its
//! frames dispatched by the host); the others are clocks of one's own, for tests and for code
//! that sends its own frames.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::executor::{Job, current_job};

/// A source of frames, as Compose's `MonotonicFrameClock`: [`with_frame_nanos`] waits for the
/// next frame and calls `on_frame` with its time in nanoseconds.
///
/// [`with_frame_nanos`]: MonotonicFrameClock::with_frame_nanos
pub trait MonotonicFrameClock {
    /// Waits for the next frame, then runs `on_frame` with its time, as `withFrameNanos`.
    fn with_frame_nanos<R>(&self, on_frame: impl FnOnce(u64) -> R) -> impl Future<Output = R>;

    /// Like [`with_frame_nanos`](MonotonicFrameClock::with_frame_nanos), with the time in
    /// milliseconds, as `withFrameMillis`.
    fn with_frame_millis<R>(&self, on_frame: impl FnOnce(u64) -> R) -> impl Future<Output = R> {
        self.with_frame_nanos(move |nanos| on_frame(nanos / 1_000_000))
    }
}

/// The frame clock of this thread's executor: what [`with_frame_nanos`](crate::with_frame_nanos)
/// awaits, its frames dispatched by the host (`platform::dispatch_frame`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ExecutorFrameClock;

impl MonotonicFrameClock for ExecutorFrameClock {
    fn with_frame_nanos<R>(&self, on_frame: impl FnOnce(u64) -> R) -> impl Future<Output = R> {
        crate::executor::with_frame_nanos(on_frame)
    }
}

/// A frame clock whose frames are sent by its owner, as Compose's `BroadcastFrameClock`:
/// [`send_frame`](BroadcastFrameClock::send_frame) resumes every task waiting for a frame.
/// A clone shares the clock.
#[derive(Clone)]
pub struct BroadcastFrameClock {
    inner: Rc<BroadcastInner>,
}

struct BroadcastInner {
    awaiters: RefCell<Vec<Rc<Awaiter>>>,
    cancelled: Cell<bool>,
    on_new_awaiters: Option<Box<dyn Fn()>>,
}

/// A task waiting for a frame of a [`BroadcastFrameClock`].
struct Awaiter {
    /// The frame's time, once sent.
    time: Cell<Option<u64>>,
    waker: RefCell<Waker>,
    /// The task awaiting, cancelled with the clock.
    job: Option<Job>,
}

impl Default for BroadcastFrameClock {
    fn default() -> Self {
        BroadcastFrameClock::new()
    }
}

impl BroadcastFrameClock {
    pub fn new() -> BroadcastFrameClock {
        BroadcastFrameClock::with_inner(None)
    }

    /// A clock calling `on_new_awaiters` when a task starts waiting for a frame while none was,
    /// as `BroadcastFrameClock(onNewAwaiters)`: the owner's cue to schedule one.
    pub fn with_on_new_awaiters(on_new_awaiters: impl Fn() + 'static) -> BroadcastFrameClock {
        BroadcastFrameClock::with_inner(Some(Box::new(on_new_awaiters)))
    }

    fn with_inner(on_new_awaiters: Option<Box<dyn Fn()>>) -> BroadcastFrameClock {
        BroadcastFrameClock {
            inner: Rc::new(BroadcastInner {
                awaiters: RefCell::new(Vec::new()),
                cancelled: Cell::new(false),
                on_new_awaiters,
            }),
        }
    }

    /// Whether a task waits for a frame.
    pub fn has_awaiters(&self) -> bool {
        !self.inner.awaiters.borrow().is_empty()
    }

    /// Sends a frame at `time_nanos`: every task waiting for one resumes with it, as
    /// `sendFrame`.
    pub fn send_frame(&self, time_nanos: u64) {
        let awaiters = std::mem::take(&mut *self.inner.awaiters.borrow_mut());
        for awaiter in awaiters {
            awaiter.time.set(Some(time_nanos));
            let waker = awaiter.waker.borrow().clone();
            waker.wake();
        }
    }

    /// Cancels the clock, as `cancel()`: the tasks waiting for a frame are cancelled, and so is
    /// any task awaiting one later, as Kotlin's `CancellationException` cancels the coroutine.
    pub fn cancel(&self) {
        self.inner.cancelled.set(true);
        let awaiters = std::mem::take(&mut *self.inner.awaiters.borrow_mut());
        // Cancelling a task drops its future, which may use the clock: outside the borrow.
        for awaiter in awaiters {
            if let Some(job) = awaiter.job {
                job.cancel();
            }
        }
    }
}

impl MonotonicFrameClock for BroadcastFrameClock {
    fn with_frame_nanos<R>(&self, on_frame: impl FnOnce(u64) -> R) -> impl Future<Output = R> {
        FrameAwait { inner: self.inner.clone(), on_frame: Some(on_frame), awaiter: None }
    }
}

/// The future of [`BroadcastFrameClock::with_frame_nanos`].
struct FrameAwait<F> {
    inner: Rc<BroadcastInner>,
    on_frame: Option<F>,
    awaiter: Option<Rc<Awaiter>>,
}

impl<F> Unpin for FrameAwait<F> {}

impl<R, F: FnOnce(u64) -> R> Future for FrameAwait<F> {
    type Output = R;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<R> {
        if let Some(awaiter) = &self.awaiter {
            if let Some(time) = awaiter.time.get() {
                self.awaiter = None;
                let on_frame = self.on_frame.take().expect("polled after it completed");
                return Poll::Ready(on_frame(time));
            }
            *awaiter.waker.borrow_mut() = context.waker().clone();
            return Poll::Pending;
        }
        assert!(self.on_frame.is_some(), "polled after it completed");
        if self.inner.cancelled.get() {
            // A cancelled clock cancels its late awaiters' tasks.
            if let Some(job) = current_job() {
                job.cancel();
            }
            return Poll::Pending;
        }
        let awaiter = Rc::new(Awaiter {
            time: Cell::new(None),
            waker: RefCell::new(context.waker().clone()),
            job: current_job(),
        });
        let first = {
            let mut awaiters = self.inner.awaiters.borrow_mut();
            awaiters.push(awaiter.clone());
            awaiters.len() == 1
        };
        self.awaiter = Some(awaiter);
        if first && let Some(on_new_awaiters) = &self.inner.on_new_awaiters {
            on_new_awaiters();
        }
        Poll::Pending
    }
}

impl<F> Drop for FrameAwait<F> {
    /// An awaiter cancelled before its frame stops waiting.
    fn drop(&mut self) {
        if let Some(awaiter) = self.awaiter.take()
            && let Ok(mut awaiters) = self.inner.awaiters.try_borrow_mut()
        {
            awaiters.retain(|waiting| !Rc::ptr_eq(waiting, &awaiter));
        }
    }
}

/// A frame clock that can be paused, as Compose's `PausableMonotonicFrameClock`: while paused,
/// [`with_frame_nanos`](MonotonicFrameClock::with_frame_nanos) waits for
/// [`resume`](PausableMonotonicFrameClock::resume) before waiting for a frame of the clock it
/// wraps. A clone shares the pause.
#[derive(Clone)]
pub struct PausableMonotonicFrameClock<C: MonotonicFrameClock> {
    frame_clock: C,
    latch: Rc<Latch>,
}

/// Open or closed; tasks wait while it's closed.
#[derive(Default)]
struct Latch {
    closed: Cell<bool>,
    waiting: RefCell<Vec<Waker>>,
}

impl<C: MonotonicFrameClock> PausableMonotonicFrameClock<C> {
    pub fn new(frame_clock: C) -> PausableMonotonicFrameClock<C> {
        PausableMonotonicFrameClock { frame_clock, latch: Rc::default() }
    }

    pub fn is_paused(&self) -> bool {
        self.latch.closed.get()
    }

    /// Pauses: frames awaited from now on wait for [`resume`](Self::resume).
    pub fn pause(&self) {
        self.latch.closed.set(true);
    }

    /// Resumes: the tasks waiting go on to wait for a frame of the wrapped clock.
    pub fn resume(&self) {
        self.latch.closed.set(false);
        let waiting = std::mem::take(&mut *self.latch.waiting.borrow_mut());
        for waker in waiting {
            waker.wake();
        }
    }
}

impl<C: MonotonicFrameClock> MonotonicFrameClock for PausableMonotonicFrameClock<C> {
    fn with_frame_nanos<R>(&self, on_frame: impl FnOnce(u64) -> R) -> impl Future<Output = R> {
        let latch = self.latch.clone();
        async move {
            std::future::poll_fn(|context| {
                if latch.closed.get() {
                    latch.waiting.borrow_mut().push(context.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            self.frame_clock.with_frame_nanos(on_frame).await
        }
    }
}
