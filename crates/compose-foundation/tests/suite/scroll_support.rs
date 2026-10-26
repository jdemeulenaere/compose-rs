//! Helpers for the scrolling ports: touch input with Compose's test spacing and tasks run to
//! their end. Private to this suite until the harness's own input DSL lands (the parity
//! ledger's "What the harness lacks").

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use compose_runtime::{CoroutineScope, Job, platform};
use compose_test::ComposeTestRule;
use compose_ui::{Offset, PointerEvent, PointerId, PointerType};

/// The time between two injected moves, as Compose's test `eventPeriodMillis`.
pub const EVENT_PERIOD_MILLIS: u64 = 16;

pub fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

/// A touch gesture's state, as Compose's `TouchInjectionScope`: where the finger is and the
/// event time, which moves on as Compose's does (an event period per move).
pub struct Touch {
    position: Offset,
    time: u64,
}

impl Touch {
    /// A finger down at `position` (in the window), now.
    pub fn down(ui: &mut ComposeTestRule, position: Offset) -> Touch {
        let time = platform::now_nanos();
        ui.host().send_pointer(touch(PointerEvent::press(position)).at_time(time));
        Touch { position, time }
    }

    /// Moves by `delta`, an event period later, as `moveBy`.
    pub fn move_by(&mut self, ui: &mut ComposeTestRule, delta: Offset) {
        self.move_to(ui, self.position + delta);
    }

    /// Moves to `position`, an event period later, as `moveTo`.
    pub fn move_to(&mut self, ui: &mut ComposeTestRule, position: Offset) {
        self.time += EVENT_PERIOD_MILLIS * 1_000_000;
        self.position = position;
        ui.host().send_pointer(touch(PointerEvent::moved(position, true)).at_time(self.time));
    }

    /// Lets the event time run on by `millis` without an event, as `advanceEventTime`.
    pub fn advance_event_time(&mut self, millis: u64) {
        self.time += millis * 1_000_000;
    }

    /// Lifts the finger where it is, as `up`.
    pub fn up(self, ui: &mut ComposeTestRule) {
        ui.host().send_pointer(touch(PointerEvent::release(self.position)).at_time(self.time));
    }
}

/// A straight swipe from `start` to `end` over `duration_millis`, with moves an event period
/// apart, as Compose's `swipe(start, end, durationMillis)`.
pub fn swipe(ui: &mut ComposeTestRule, start: Offset, end: Offset, duration_millis: u64) {
    let mut finger = Touch::down(ui, start);
    let steps = (duration_millis as f32 / EVENT_PERIOD_MILLIS as f32).round().max(1.0) as u64;
    let begin = finger.time;
    for step in 1..=steps {
        let at = step * duration_millis / steps;
        let fraction = at as f32 / duration_millis as f32;
        finger.time = begin + at * 1_000_000;
        finger.position = Offset::new(
            start.x + (end.x - start.x) * fraction,
            start.y + (end.y - start.y) * fraction,
        );
        ui.host()
            .send_pointer(touch(PointerEvent::moved(finger.position, true)).at_time(finger.time));
    }
    finger.up(ui);
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
