//! Helpers for the scrolling ports: compose-test's touch injection in the root's coordinates, a
//! finger across calls, and tasks run to their end.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use compose_runtime::{CoroutineScope, Job};
use compose_test::ComposeTestRule;
use compose_ui::Offset;

/// A finger across calls, between which a test reads or asserts, as a Compose test's several
/// `performTouchInput` blocks (the input's state lasts between them).
pub struct Touch;

impl Touch {
    /// A finger down at `position` (in the window), now.
    pub fn down(ui: &mut ComposeTestRule, position: Offset) -> Touch {
        ui.on_root().perform_touch_input(|touch| touch.down(position));
        Touch
    }

    /// Moves by `delta`, an event period later, as `moveBy`.
    pub fn move_by(&mut self, ui: &mut ComposeTestRule, delta: Offset) {
        ui.on_root().perform_touch_input(|touch| touch.move_by(delta));
    }

    /// Lets the event time run on by `millis` without an event, as `advanceEventTime`.
    pub fn advance_event_time(&mut self, ui: &mut ComposeTestRule, millis: u64) {
        ui.on_root().perform_touch_input(|touch| touch.advance_event_time(millis));
    }

    /// Lifts the finger where it is, as `up`.
    pub fn up(self, ui: &mut ComposeTestRule) {
        ui.on_root().perform_touch_input(|touch| touch.up());
    }
}

/// A straight swipe from `start` to `end` over `duration_millis`, as `swipe(start, end,
/// durationMillis)` in the root's coordinates.
pub fn swipe(ui: &mut ComposeTestRule, start: Offset, end: Offset, duration_millis: u64) {
    ui.on_root().perform_touch_input(|touch| touch.swipe(start, end, duration_millis));
}

/// Launches `future` as a task of its own, as `scope.launch { … }` in a test.
pub fn launch(future: impl Future<Output = ()> + 'static) -> Job {
    CoroutineScope::new().launch(future)
}

/// Runs `future` to its end, frames passing as it needs them, as `runBlocking` inside
/// `runOnIdle` (or `scope.launch` then `waitForIdle`); returns its output.
pub fn run<T: 'static>(ui: &mut ComposeTestRule, future: impl Future<Output = T> + 'static) -> T {
    let output: Rc<RefCell<Option<T>>> = Rc::default();
    let into = output.clone();
    launch(async move {
        *into.borrow_mut() = Some(future.await);
    });
    ui.wait_for_idle();
    output.borrow_mut().take().expect("the task ran to its end")
}
