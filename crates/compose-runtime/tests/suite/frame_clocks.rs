//! Frame clocks: Compose's `BroadcastFrameClockTest`, adapted, and `PausableMonotonicFrameClock`.
//! Tasks run on this thread's executor (`platform::run_ready`).

use std::cell::Cell;
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
