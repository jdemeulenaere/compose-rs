//! Phase H review: failing tests for tasks and the frame clock through a UI host and the test
//! rule's main clock (docs/04-runtime.md, "Tasks and the frame clock"; docs/09-testing.md, "The
//! main clock"; D112, D113). Each test states the input, what Jetpack Compose does, and what
//! happens instead. Runtime-only findings are in compose/tests/review_h.rs.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::UiHost;

thread_local! {
    static LOG: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    static COUNT: Cell<u32> = const { Cell::new(0) };
}

fn log(entry: u64) {
    LOG.with(|log| log.borrow_mut().push(entry));
}

fn take_log() -> Vec<u64> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Input: an animation task, `loop { with_frame_nanos(|_| ()).await; ticks += 1 }`, with
/// `ticks` shown by a `BasicText`, in a `UiHost`; after its first frames, one more frame runs.
///
/// Compose: the frame resumes the animation, which writes `ticks`, then recomposes in the same
/// frame; the next frame is asked for only by the animation's next `withFrameNanos`, which the
/// frame clock paces at vsync.
///
/// Instead: the write in `dispatch_frame` happens after `frame()` cleared `frame_requested`, so
/// it sets it again (and calls the platform's frame waker) even though this frame recomposes
/// it. After the frame, `has_pending_work()` is true and the waker was called: the desktop
/// loop's `redraw` and `about_to_wait` request a redraw at once instead of pacing the next
/// animation frame at the refresh interval, and the waker's `request_redraw` does the same. Any
/// animation that writes state the UI reads (all of them) runs frames back to back, a busy
/// loop, which D113 meant to prevent.
#[test]
fn an_animation_frame_leaves_no_pending_work() {
    let mut host = UiHost::headless(200, 100);
    let wakes = Rc::new(Cell::new(0));
    let counter = wakes.clone();
    host.set_frame_waker(move || counter.set(counter.get() + 1));
    host.set_content(|| {
        let ticks = remember(|| mutable_state_of(0u32));
        LaunchedEffect((), async move {
            loop {
                with_frame_nanos(|_| ()).await;
                ticks.update(|ticks| ticks + 1);
            }
        });
        BasicText!("{}", ticks.get());
    });
    // The first pass, then the animation's first frame.
    host.frame();
    host.frame();
    wakes.set(0);
    host.frame();
    assert!(host.needs_frame(), "the animation waits for its next frame");
    assert_eq!(wakes.get(), 0, "the frame asked the platform for another frame at once");
    assert!(!host.has_pending_work(), "a frame is due at once, not at the next refresh");
}

/// Input: a test rule with `mainClock.autoAdvance` off (`with_manual_clock`) and a five-frame
/// animation whose frames write state the UI shows; the test advances the clock one frame.
///
/// Compose: with the clock still, the animation waits for it (docs/09: "animations and delays
/// wait for it"); `mainClock.advanceTimeByFrame()` then runs one frame: the animation logs one
/// frame time, 16 ms after the start.
///
/// Instead: the animation runs all five frames at the start time while the rule is being
/// created, before the test advances anything (the log is `[0, 0, 0, 0, 0]`). Its first frame
/// comes with the first `frame()` (`set_content` composed and launched it, so `run_ready`
/// then `dispatch_frame` resume it at once), and each frame's write leaves `frame_requested`
/// set (as above), so `wait_for_idle`, even with the clock off, runs another frame without
/// moving the clock, and `dispatch_frame` resumes the animation again with the same time: a
/// zero frame-time delta (a division by zero for a velocity). The same happens in the
/// `wait_for_idle` that ends every `advance_time_by`; an endless animation never lets the rule
/// idle and fails after 100,000 frames at one timestamp.
#[test]
fn advancing_the_clock_one_frame_runs_one_animation_frame() {
    take_log();
    #[composable]
    fn Animated() {
        let frames = remember(|| mutable_state_of(0u32));
        LaunchedEffect((), async move {
            for _ in 0..5 {
                let time = with_frame_millis(|millis| millis).await;
                log(time);
                frames.update(|frames| frames + 1);
            }
        });
        BasicText!("{} frames", frames.get());
    }
    let mut ui = ComposeTestRule::with_manual_clock(Animated);
    let start = ui.current_time_millis();
    assert_eq!(take_log(), Vec::<u64>::new());
    ui.advance_time_by_frame();
    assert_eq!(take_log(), [start + 16]);
}

/// Input: a test rule with `mainClock.autoAdvance` off and a `LaunchedEffect` counting
/// `loop { delay(10 ms).await; count += 1 }`; the test advances the clock 100 ms.
///
/// Compose: `mainClock.advanceTimeBy(100)` advances the test scheduler through every task due
/// on the way, each at its own time: the delays end at 10, 20, … 100 ms, and `count` is 10.
///
/// Instead: `advance_time_by` moves the clock in 16 ms steps and fires timers only at the end
/// of each step, so each `delay` ends up to 16 ms late and the next one starts from there: the
/// delays end at 16, 32, … 96 ms and `count` is 6. Tasks see a clock that drifts from their
/// deadlines, and a test asserting a timeout's or a debounce's effect at its exact time fails.
#[test]
fn advancing_the_clock_ends_each_delay_at_its_deadline() {
    COUNT.with(|count| count.set(0));
    #[composable]
    fn Ticker() {
        LaunchedEffect((), async move {
            loop {
                delay(Duration::from_millis(10)).await;
                COUNT.with(|count| count.set(count.get() + 1));
            }
        });
    }
    let mut ui = ComposeTestRule::with_manual_clock(Ticker);
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(COUNT.with(Cell::get), 10);
}

/// Input: a `LaunchedEffect` waiting on `delay(2 minutes)` leaves the composition (its caller's
/// condition turns false); then the test waits for idle with `mainClock.autoAdvance` on.
///
/// Compose: the cancelled coroutine's delay is gone; nothing is pending, `waitForIdle` returns
/// at once and the clock doesn't move.
///
/// Instead: the cancelled `Delay`'s timer stays in the executor's heap (see
/// compose/tests/review_h.rs, `a_cancelled_delay_leaves_no_timer`), so `wait_for_idle` takes it
/// for pending work and jumps the clock two minutes, to its deadline, then fails: "the UI never
/// became idle (…, 120s of test time)". With a shorter delay it silently moves the test's clock
/// to a cancelled task's deadline.
#[test]
fn a_cancelled_delay_does_not_move_the_test_clock() {
    #[composable]
    fn Screen(waiting: MutableState<bool>) {
        if waiting.get() {
            LaunchedEffect((), async move {
                delay(Duration::from_secs(120)).await;
            });
        }
    }
    let waiting = mutable_state_of(true);
    let handle = waiting.handle();
    // Started with the clock still, so the delay doesn't end on its own.
    let mut ui = ComposeTestRule::with_manual_clock(move || Screen(handle));
    waiting.set(false);
    ui.wait_for_idle();
    let start = ui.current_time_millis();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(ui.current_time_millis(), start);
}
