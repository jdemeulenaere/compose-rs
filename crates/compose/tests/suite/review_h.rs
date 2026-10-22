//! Phase H review: failing tests for the runtime's tasks (`compose_runtime::executor`) and
//! `derived_state_of` (docs/04-runtime.md, "Effects" and "Tasks and the frame clock";
//! docs/05-state.md, "Derived state"). Each test states the input, what Jetpack Compose does,
//! and what happens instead. Tests that need the UI test rule are in
//! compose-foundation/tests/review_h.rs.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose::runtime::platform;
use compose::runtime::platform::*;
use compose::runtime::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

// ---- Derived state -----------------------------------------------------------------------------

/// Input: `current = derived_state_of(|| items.with(|items| items[selected.get()]))` over
/// `items = [10, 20, 30]` and `selected = 2`; a handler removes the selected (last) item, then
/// selects the first one: `items.edit(Vec::pop); selected.set(0)`. Every consistent state has
/// `selected < items.len()`.
///
/// Compose: `derivedStateOf` is lazy. Nothing reads it between the two writes, and its next
/// read (the next recomposition, or `current.value`) computes `items[0]` = 10.
///
/// Instead: the derived state recomputes eagerly inside the first write (D109), on the
/// half-done update `items = [10, 20]`, `selected = 2`, and the calculation panics with an
/// index out of bounds inside `items.edit`. Any calculation that relies on an invariant two
/// writes maintain crashes this way. D109's reasoning weighs only the cost of a check before
/// each pass, not that eager recomputation runs the calculation on intermediate states.
#[test]
fn a_derived_state_is_not_computed_from_a_half_done_update() {
    let items = mutable_state_of(vec![10, 20, 30]);
    let selected = mutable_state_of(2usize);
    let (items_handle, selected_handle) = (items.handle(), selected.handle());
    let current = derived_state_of(move || items_handle.with(|items| items[selected_handle.get()]));
    assert_eq!(current.get(), 30);
    items.edit(|items| {
        items.pop();
    });
    selected.set(0);
    assert_eq!(current.get(), 10);
}

/// Input: `summary = derived_state_of(|| items.with(Vec::len) + count.get())`; a handler edits
/// `items` in place and records the new length in `count` from inside the edit:
/// `items.edit(|items| { items.push(3); count.set(items.len()) })`. Without the derived state
/// this is fine: the code never accesses `items` twice.
///
/// Compose: writes don't run derived calculations; `summary` is 3 + 3 = 6 when next read.
///
/// Instead: `count.set` recomputes `summary` at once, inside the edit, and its calculation's
/// `items.with` hits the edit's borrow: "state … is borrowed by `with` or `edit` … and was
/// accessed again". The user's code has no nested access; the runtime adds one, so whether a
/// handler panics depends on which derived states exist anywhere on the thread.
#[test]
fn writing_state_inside_an_edit_does_not_run_a_derived_calculation_on_the_edited_state() {
    let items = mutable_state_of(vec![1, 2]);
    let count = mutable_state_of(0usize);
    let (items_handle, count_handle) = (items.handle(), count.handle());
    let summary = derived_state_of(move || items_handle.with(Vec::len) + count_handle.get());
    assert_eq!(summary.get(), 2);
    items.edit(|items| {
        items.push(3);
        count_handle.set(items.len());
    });
    assert_eq!(summary.get(), 6);
}

#[composable]
fn Doubled(count: MutableState<i32>) {
    let doubled = remember(move || derived_state_of(move || count.get() * 2));
    log(format!("count={} doubled={}", count.get(), doubled.get()));
}

/// Input: a host whose frame requester recomposes at once (docs/04, D49), and a composable
/// reading both `count` and `doubled = derived_state_of(|| count.get() * 2)`; a handler sets
/// `count` to 2.
///
/// Compose: a derived state read is always consistent with the state it reads: the pass sees
/// `count=2 doubled=4`.
///
/// Instead: the write queues the composition's requester (the scope reads `count`) before the
/// derived state's recomputation (an observed read, queued after the scope readers in
/// `invalidate_readers`). The requester recomposes first, and the body sees `count=2
/// doubled=2`: an inconsistent pass whose side effects run on the glitch, followed by a second
/// pass once the derived state catches up.
#[test]
fn a_synchronous_host_never_sees_a_derived_state_behind_its_dependency() {
    let composition = Rc::new(Composition::new(NodeTree::<()>::new()));
    let weak = Rc::downgrade(&composition);
    composition.set_frame_requester(move || {
        if let Some(composition) = weak.upgrade() {
            composition.recompose();
        }
    });
    let count = mutable_state_of(1);
    let handle = count.handle();
    composition.set_content(move || Doubled(handle));
    assert_eq!(take_log(), ["count=1 doubled=2"]);
    count.set(2);
    assert_eq!(take_log(), ["count=2 doubled=4"]);
}

// ---- Tasks -------------------------------------------------------------------------------------

/// Input: a task waits on `delay(1 hour)`, then is cancelled (`job.cancel()`, as a
/// `LaunchedEffect` whose keys changed or that left the composition).
///
/// Compose: cancelling a coroutine suspended in `delay` disposes its scheduled resume; the
/// dispatcher has no pending timer.
///
/// Instead: `cancel` drops the future, but `Delay` has no `Drop` and its timer (holding the
/// task's waker) stays in the executor's heap until its deadline. `next_timer()` still reports
/// it, so the desktop loop wakes for nothing at that time, and the test rule's `wait_for_idle`
/// moves the clock to it (see compose-foundation/tests/review_h.rs). A `LaunchedEffect(key) {
/// delay(…) }` restarted on every keystroke piles up one timer per restart. (A `Delay` polled
/// again before its deadline, as in a `select`, also pushes a duplicate timer per poll.)
#[test]
fn a_cancelled_delay_leaves_no_timer() {
    let job = CoroutineScope::new().launch(async {
        delay(Duration::from_secs(3600)).await;
    });
    platform::run_ready();
    assert!(platform::next_timer().is_some(), "the task waits on its delay");
    job.cancel();
    assert_eq!(platform::next_timer(), None);
}

/// Input: an animation task (`loop { with_frame_nanos(…).await }`) is cancelled between frames.
///
/// Compose: cancelling a coroutine suspended in `withFrameNanos` removes its awaiter from the
/// frame clock (`BroadcastFrameClock` handles the cancellation); no frame is asked for.
///
/// Instead: the awaiter stays in `frame_awaiters` until the next frame, so
/// `has_frame_awaiters()` (and `UiHost::needs_frame`) is still true: the desktop paces and runs
/// one more frame for nothing, and the test rule's `wait_for_idle` moves its clock one frame on
/// (16 ms) after a cancelled animation.
#[test]
fn a_cancelled_animation_asks_for_no_frame() {
    let job = CoroutineScope::new().launch(async {
        loop {
            with_frame_nanos(|_| ()).await;
        }
    });
    platform::run_ready();
    assert!(platform::has_frame_awaiters(), "the task waits for a frame");
    job.cancel();
    assert!(!platform::has_frame_awaiters());
}
