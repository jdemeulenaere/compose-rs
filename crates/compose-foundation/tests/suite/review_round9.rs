//! Ninth review round: robustness of the task executor and lazy lists under inputs a real app
//! gives them, through the public API. Each test states the input, what Compose does (or what a
//! robust library should do), and what happens instead.

#![allow(non_snake_case)]

use compose_foundation::*;
use compose_runtime::composable;
use compose_test::*;
use compose_ui::{IntSize, UnitExt};

// ---- Tasks ---------------------------------------------------------------------------------

/// The future of an `async` yield, as `futures_lite::future::yield_now` or Kotlin's `yield()`:
/// pending once, waking itself, so other work runs before it goes on.
struct YieldNow(bool);

impl std::future::Future for YieldNow {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            return std::task::Poll::Ready(());
        }
        self.0 = true;
        context.waker().wake_by_ref();
        std::task::Poll::Pending
    }
}

/// A `LaunchedEffect` doing long work in chunks, yielding between them so the UI stays
/// responsive: `for (chunk in work) { process(chunk); yield() }`. In Compose, `yield()` on the
/// main dispatcher puts the coroutine back in the queue behind the frame, so frames (and input)
/// run between chunks. Here the executor's `run_ready` polls the tasks woken while it runs,
/// in the same loop: a task that wakes itself is polled again at once, and the frame that
/// started it doesn't end until all 100,000 chunks are done; a polling loop that yields forever
/// (`loop { check(); yield_now().await }`) hangs the UI thread.
#[test]
fn a_yielding_task_lets_the_frame_end() {
    let progress = compose_runtime::mutable_state_of(0u32);
    let handle = progress.handle();
    let mut host = compose_ui::UiHost::headless(200, 200);
    host.set_content(move || {
        compose_runtime::LaunchedEffect((), async move {
            for _ in 0..100_000 {
                handle.update(|done| done + 1);
                YieldNow(false).await;
            }
        });
        BasicText!("done {}", handle.get());
    });
    host.frame();
    assert!(progress.get() < 1000, "the first frame ran {} chunks before it ended", progress.get());
}

// ---- Lazy lists: offsets past the end ------------------------------------------------------

#[composable]
fn HundredRows(state: LazyListState) {
    LazyColumn!(modifier = Modifier.fill_max_size(), state = state, |scope: LazyListScope| {
        scope.items(100, |index| {
            BasicText!("Row {}", index, modifier = Modifier.height(20.dp()));
        });
    });
}

/// `scrollToItem(lastIndex, Int.MAX_VALUE)`, a way to say "to the very end of the last item":
/// Compose's measurement takes the offset in `Int`s and, finding room after the last item,
/// scrolls back so it ends the viewport: the list shows its last rows. Here the layout computes
/// the room left as `main_max - content_end` with `content_end` near `-i32::MAX`, which
/// overflows: a debug build panics ("attempt to subtract with overflow" in lazy.rs's measure),
/// and in a release build the gap wraps negative, so the list stays at item 99 scrolled
/// `i32::MAX` pixels into it, with nothing in view.
#[test]
fn scrolling_to_the_last_item_with_a_huge_offset_shows_the_end() {
    let state = LazyListState::new(0, 0);
    let list = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || HundredRows(list.clone()));
    state.scroll_to_item(99, i32::MAX);
    ui.wait_for_idle();
    ui.on_node_with_text("Row 99").assert_exists();
    assert_eq!(state.first_visible_item_index(), 90);
}

/// `scrollToItem(10)` then `scrollBy(Float.POSITIVE_INFINITY)` in one handler ("as far as it
/// goes"): Compose rounds the delta to `Int.MAX_VALUE` and its measurement stops at the end,
/// showing the last rows. Here a jump forgets how far the list can scroll (infinite until the
/// next layout), so the whole infinite delta becomes the offset, rounded to `i32::MAX` pixels:
/// the same overflow, a panic in debug builds.
#[test]
fn an_infinite_scroll_right_after_a_jump_stops_at_the_end() {
    let state = LazyListState::new(0, 0);
    let list = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || HundredRows(list.clone()));
    state.scroll_to_item(10, 0);
    state.scroll_by(f32::INFINITY);
    ui.wait_for_idle();
    ui.on_node_with_text("Row 99").assert_exists();
    assert_eq!(state.first_visible_item_index(), 90);
}
