//! Frame clocks: Compose's `BroadcastFrameClockTest`, adapted, and `PausableMonotonicFrameClock`.
//! Tasks run on this thread's executor (`platform::run_ready`).

use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;

use compose_runtime::platform::run_ready;
use compose_runtime::*;

/// Launches `future` as a task of a scope of its own, and runs what's ready.
fn launch(scope: &CoroutineScope, future: impl std::future::Future<Output = ()> + 'static) -> Job {
    let job = scope.launch(future);
    run_ready();
    job
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/BroadcastFrameClockTest.kt, sendAndReceiveFrames
#[test]
fn a_sent_frame_reaches_its_awaiter() {
    let scope = CoroutineScope::new();
    let clock = BroadcastFrameClock::new();
    let received = Rc::new(Cell::new(None));
    let (c, r) = (clock.clone(), received.clone());
    launch(&scope, async move { r.set(Some(c.with_frame_nanos(|time| time).await)) });
    assert!(clock.has_awaiters());
    clock.send_frame(1);
    run_ready();
    assert_eq!(received.get(), Some(1), "the awaiter's frame time");
    assert!(!clock.has_awaiters());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/BroadcastFrameClockTest.kt, cancelClock
#[test]
fn cancelling_the_clock_cancels_its_awaiters_and_later_ones() {
    let scope = CoroutineScope::new();
    let clock = BroadcastFrameClock::new();
    let c = clock.clone();
    let awaiter = launch(&scope, async move {
        c.with_frame_nanos(|time| time).await;
    });
    assert!(awaiter.is_active());
    clock.cancel();
    assert!(!awaiter.is_active(), "the awaiter was cancelled");
    let c = clock.clone();
    let late = launch(&scope, async move {
        c.with_frame_nanos(|time| time).await;
    });
    assert!(!late.is_active(), "a late awaiter is cancelled");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/BroadcastFrameClockTest.kt, locklessCancellation
#[test]
fn a_cancelled_awaiter_stops_waiting_and_new_ones_wait_for_the_next_frame() {
    // Compose cancels the awaiter from another thread while a frame is being sent; state here
    // is thread-bound, so this keeps the single-threaded part: cancelling an awaiting task
    // removes it, and the clock takes new awaiters after.
    let scope = CoroutineScope::new();
    let clock = BroadcastFrameClock::new();
    let c = clock.clone();
    let cancelling = launch(&scope, async move { c.with_frame_nanos(|_| ()).await });
    assert!(clock.has_awaiters());
    cancelling.cancel();
    assert!(!clock.has_awaiters());
    let c = clock.clone();
    launch(&scope, async move { c.with_frame_nanos(|_| ()).await });
    assert!(clock.has_awaiters());
    clock.cancel();
}

/// `on_new_awaiters` hears the first awaiter since a frame, as `BroadcastFrameClock`'s
/// `onNewAwaiters` (Compose's `failClockWhenNewAwaitersNotified` throws from it: a Kotlin
/// exception from a callback has no analogue here).
#[test]
fn new_awaiters_are_announced_once_per_frame() {
    let scope = CoroutineScope::new();
    let announced = Rc::new(Cell::new(0));
    let counted = announced.clone();
    let clock = BroadcastFrameClock::with_on_new_awaiters(move || counted.set(counted.get() + 1));
    for _ in 0..3 {
        let c = clock.clone();
        launch(&scope, async move { c.with_frame_nanos(|_| ()).await });
    }
    assert_eq!(announced.get(), 1, "the first awaiter since the last frame");
    clock.send_frame(1);
    run_ready();
    let c = clock.clone();
    launch(&scope, async move { c.with_frame_nanos(|_| ()).await });
    assert_eq!(announced.get(), 2);
}

/// A paused clock holds its awaiters until resumed, as `PausableMonotonicFrameClock` (the
/// recomposer's, which Compose's `RecomposerTests.pausingTheFrameClockStopShouldBlockWithFrameNanos`
/// pauses).
#[test]
fn a_paused_clock_holds_its_awaiters_until_resumed() {
    let scope = CoroutineScope::new();
    let broadcast = BroadcastFrameClock::new();
    let clock = PausableMonotonicFrameClock::new(broadcast.clone());
    assert!(!clock.is_paused());
    clock.pause();
    assert!(clock.is_paused());
    let received = Rc::new(Cell::new(None));
    let (c, r) = (clock.clone(), received.clone());
    launch(&scope, async move { r.set(Some(c.with_frame_millis(|time| time).await)) });
    assert!(!broadcast.has_awaiters(), "paused: not waiting for a frame yet");
    broadcast.send_frame(5_000_000);
    run_ready();
    assert_eq!(received.get(), None);
    clock.resume();
    run_ready();
    assert!(broadcast.has_awaiters());
    broadcast.send_frame(7_000_000);
    run_ready();
    assert_eq!(received.get(), Some(7), "resumed: the next frame, in milliseconds");
}

/// The executor's clock is a `MonotonicFrameClock` too: what `with_frame_nanos` awaits.
#[test]
fn the_executor_clock_waits_for_the_hosts_frames() {
    let scope = CoroutineScope::new();
    compose_runtime::platform::set_manual_clock(3_000);
    let received = Rc::new(Cell::new(None));
    let r = received.clone();
    launch(&scope, async move { r.set(Some(ExecutorFrameClock.with_frame_nanos(|t| t).await)) });
    assert!(compose_runtime::platform::has_frame_awaiters());
    compose_runtime::platform::dispatch_frame();
    assert_eq!(received.get(), Some(3_000));
}

/// `send_frame` runs each awaiter's `on_frame` itself, before returning, as Compose's: the
/// task resumes later, with the result.
#[test]
fn a_sent_frame_runs_on_frame_at_once() {
    let scope = CoroutineScope::new();
    let clock = BroadcastFrameClock::new();
    let (ran, resumed) = (Rc::new(Cell::new(None)), Rc::new(Cell::new(false)));
    let (c, r, s) = (clock.clone(), ran.clone(), resumed.clone());
    launch(&scope, async move {
        let r = r.clone();
        c.with_frame_nanos(move |time| r.set(Some(time))).await;
        s.set(true);
    });
    clock.send_frame(5);
    assert_eq!(ran.get(), Some(5), "on_frame ran in send_frame");
    assert!(!resumed.get(), "the task resumes on the executor");
    run_ready();
    assert!(resumed.get());
}

/// A waker counting its wakes.
struct CountingWaker(std::sync::atomic::AtomicUsize);

impl std::task::Wake for CountingWaker {
    fn wake(self: std::sync::Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A paused clock's awaiter polled again keeps one waker: resuming wakes it once.
#[test]
fn a_paused_awaiter_polled_again_is_woken_once() {
    let clock = PausableMonotonicFrameClock::new(BroadcastFrameClock::new());
    clock.pause();
    let counter = std::sync::Arc::new(CountingWaker(std::sync::atomic::AtomicUsize::new(0)));
    let waker = std::task::Waker::from(counter.clone());
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(clock.with_frame_nanos(|time| time));
    for _ in 0..3 {
        assert!(future.as_mut().poll(&mut context).is_pending());
    }
    clock.resume();
    assert_eq!(counter.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}
