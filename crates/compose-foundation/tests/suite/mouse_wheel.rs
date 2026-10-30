//! The mouse wheel, as Compose's `MouseWheelScrollingLogic`: a step at once, then a linear tween;
//! the scroll held for the next turn; turns joining it; nothing started at an edge; what runs
//! when it stops.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::gestures::{FlingBehavior, LocalBoxFuture, ScrollBlock, ScrollScope};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, UnitExt};

use crate::scroll_support::launch;

const CENTER: Offset = Offset::new(50.0, 50.0);

/// A state adding up its deltas, all of them consumed.
fn total_state() -> (DefaultScrollableState, Rc<Cell<f32>>) {
    let total = Rc::new(Cell::new(0.0f32));
    let into = total.clone();
    let state = scrollable_state(move |delta| {
        into.set(into.get() + delta);
        delta
    });
    (state, total)
}

fn sized(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), content);
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    ui
}

fn box_of(state: impl ScrollableState + ComposeArg + Clone + 'static) -> ComposeTestRule {
    sized(move || {
        Box!(modifier = Modifier.size(100.dp()).scrollable(state.clone(), Orientation::Vertical));
    })
}

/// A turn at the center of the box: towards the end of a `Modifier.scrollable`, whose deltas are
/// the pointer's, with a negative `pixels`.
fn turn(ui: &mut ComposeTestRule, pixels: f32) {
    ui.host().send_pointer(PointerEvent::scroll(CENTER, Offset::new(0.0, pixels)));
}

fn advance(ui: &mut ComposeTestRule, millis: u64) {
    ui.advance_time_by(Duration::from_millis(millis));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, dispatchMouseWheelScroll: a delta under the threshold applies at once
#[test]
fn a_small_turn_applies_at_once() {
    let (state, total) = total_state();
    let mut ui = box_of(state);
    turn(&mut ui, -5.0);
    assert_eq!(total.get(), 5.0);
    advance(&mut ui, 200);
    assert_eq!(total.get(), 5.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, dispatchMouseWheelScroll and animateMouseWheelScroll: a step of the threshold, 6 dp, then a linear tween at 1 dp/ms
#[test]
fn a_large_turn_steps_then_follows_a_linear_tween() {
    let (state, total) = total_state();
    let mut ui = box_of(state);
    turn(&mut ui, -100.0);
    assert_eq!(total.get(), 6.0, "the step, in the event");
    // The tween starts at the next frame, 94 ms for what's left.
    advance(&mut ui, 16);
    let start = total.get();
    assert_eq!(start, 6.0, "its first frame is its start");
    advance(&mut ui, 16);
    assert!((total.get() - 22.0).abs() < 0.01, "{}", total.get());
    advance(&mut ui, 16);
    assert!((total.get() - 38.0).abs() < 0.01, "{}", total.get());
    advance(&mut ui, 200);
    assert_eq!(total.get(), 100.0, "the whole turn");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, MaxAnimationDuration: a long turn takes no longer than 100 ms
#[test]
fn a_long_turn_takes_at_most_a_hundred_milliseconds() {
    let (state, total) = total_state();
    let mut ui = box_of(state);
    turn(&mut ui, -1000.0);
    advance(&mut ui, 96);
    assert!(total.get() < 1000.0, "{}", total.get());
    // Its first frame at the next one's time, 16 ms in; 100 ms from there, to a frame.
    advance(&mut ui, 32);
    assert_eq!(total.get(), 1000.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, ScrollProgressTimeout: the scroll stays held for 50 ms after a turn
#[test]
fn the_scroll_is_held_for_the_next_turn() {
    let (state, _) = total_state();
    let mut ui = box_of(state.clone());
    turn(&mut ui, -5.0);
    assert!(state.is_scroll_in_progress());
    advance(&mut ui, 32);
    assert!(state.is_scroll_in_progress(), "within the timeout");
    advance(&mut ui, 64);
    assert!(!state.is_scroll_in_progress(), "past it");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, isScrolling: what reads isScrollInProgress sees a gesture
#[test]
fn what_reads_is_scroll_in_progress_sees_a_wheels_gesture() {
    let (state, _) = total_state();
    let seen: Rc<RefCell<Vec<bool>>> = Rc::default();
    let (given, into) = (state.clone(), seen.clone());
    let mut ui = sized(move || {
        into.borrow_mut().push(given.is_scroll_in_progress());
        Box!(modifier = Modifier.size(100.dp()).scrollable(given.clone(), Orientation::Vertical));
    });
    turn(&mut ui, -5.0);
    advance(&mut ui, 200);
    assert_eq!(*seen.borrow(), vec![false, true, false]);
}

/// A state counting its scrolls.
#[derive(Clone)]
struct Counted {
    inner: DefaultScrollableState,
    scrolls: Rc<Cell<u32>>,
}

impl ScrollableState for Counted {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        self.scrolls.set(self.scrolls.get() + 1);
        self.inner.scroll_boxed(priority, block)
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.inner.dispatch_raw_delta(delta)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.inner.is_scroll_in_progress()
    }
}

impl ComposeArg for Counted {
    fn same(&self, other: &Self) -> bool {
        self.inner.same(&other.inner)
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, the channel: turns during the animation and within the timeout join the one scroll
#[test]
fn turns_join_the_scroll_going_on() {
    let (inner, total) = total_state();
    let scrolls = Rc::new(Cell::new(0));
    let mut ui = box_of(Counted { inner, scrolls: scrolls.clone() });
    turn(&mut ui, -100.0);
    advance(&mut ui, 32);
    turn(&mut ui, -100.0);
    advance(&mut ui, 300);
    assert_eq!(total.get(), 200.0, "both turns");
    assert_eq!(scrolls.get(), 1, "one scroll");
    // Within the timeout of the last one finishing: it joins too.
    turn(&mut ui, -5.0);
    advance(&mut ui, 32);
    turn(&mut ui, -5.0);
    advance(&mut ui, 300);
    assert_eq!(total.get(), 210.0);
    assert_eq!(scrolls.get(), 2, "the first one ended, past its timeout");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, isLowScrollingDelta: a turn too small to see is dropped
#[test]
fn a_turn_too_small_to_see_is_dropped() {
    let (state, total) = total_state();
    let mut ui = box_of(state.clone());
    turn(&mut ui, -0.4);
    assert_eq!(total.get(), 0.0);
    assert!(!state.is_scroll_in_progress(), "no scroll started");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/NonTouchScrollingLogic.kt, canConsumeDelta: a state that can't go that way starts nothing, stops nothing and leaves the event
#[test]
fn a_turn_at_an_edge_starts_nothing_and_goes_to_the_parent() {
    let owner = ScrollState::new(0);
    let inner = *owner;
    let (outer, total) = total_state();
    let (given, around) = (inner, outer.clone());
    let mut ui = sized(move || {
        Box!(
            modifier = Modifier.size(100.dp()).scrollable(around.clone(), Orientation::Vertical),
            {
                Column!(modifier = Modifier.size(100.dp()).vertical_scroll(given), {
                    Box!(modifier = Modifier.size((100.dp(), 1000.dp())));
                });
            }
        );
    });
    let running: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
    let into = running.clone();
    launch(async move {
        *into.borrow_mut() =
            Some(inner.scroll(MutatePriority::Default, |_| std::future::pending::<()>()).await);
    });
    advance(&mut ui, 16);
    // Back, at the start: the column can't.
    turn(&mut ui, -5.0);
    advance(&mut ui, 200);
    assert_eq!(*running.borrow(), None, "its scroll goes on");
    assert!(!inner.last_scrolled_backward() && !inner.last_scrolled_forward());
    assert_ne!(total.get(), 0.0, "the event went on to the parent");
}

/// Records the velocity it's called with; flings nothing.
#[derive(Default)]
struct RecordingFling {
    velocities: RefCell<Vec<f32>>,
}

impl FlingBehavior for RecordingFling {
    fn perform_fling<'a>(
        &'a self,
        _scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        self.velocities.borrow_mut().push(initial_velocity);
        Box::pin(async move { 0.0 })
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt, onScrollStopped: when a wheel's scroll stops, a custom fling behavior runs, at the speed of a single turn's animation
#[test]
fn a_custom_fling_behavior_runs_when_the_wheels_scroll_stops() {
    let (state, _) = total_state();
    let fling = Rc::new(RecordingFling::default());
    let (given, behavior) = (state.clone(), fling.clone());
    let mut ui = sized(move || {
        let behavior: Rc<dyn FlingBehavior> = behavior.clone();
        Box!(
            modifier = Modifier.size(100.dp()).scrollable_with(
                given.clone(),
                Orientation::Vertical,
                true,
                false,
                Some(behavior),
                None
            )
        );
    });
    turn(&mut ui, -50.0);
    advance(&mut ui, 32);
    assert!(fling.velocities.borrow().is_empty(), "not before it stops");
    advance(&mut ui, 300);
    // One turn: its size over 100 ms, at most 1 dp/ms, as a velocity. The deltas are the pointer's
    // here (not reversed): positive towards the state's end.
    assert_eq!(*fling.velocities.borrow(), vec![500.0]);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AbstractScrollableNode.kt, shouldBeTriggeredByMouseWheel: not the default inertia
#[test]
fn the_default_fling_doesnt_run_when_the_wheels_scroll_stops() {
    let (state, total) = total_state();
    let (given, behavior) = (state.clone(), Rc::new(DefaultFlingBehavior::new(1.0)));
    let mut ui = sized(move || {
        let behavior: Rc<dyn FlingBehavior> = behavior.clone();
        Box!(
            modifier = Modifier.size(100.dp()).scrollable_with(
                given.clone(),
                Orientation::Vertical,
                true,
                false,
                Some(behavior),
                None
            )
        );
    });
    turn(&mut ui, -50.0);
    advance(&mut ui, 1000);
    assert_eq!(total.get(), 50.0, "the turn and nothing more");
}

// A scroll of a higher priority refuses a turn, and the wheel isn't left scrolling.
#[test]
fn a_scroll_of_a_higher_priority_refuses_a_turn() {
    let (state, total) = total_state();
    let mut ui = box_of(state.clone());
    let held = state.clone();
    let result: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
    let into = result.clone();
    let job = launch(async move {
        *into.borrow_mut() = Some(
            held.scroll(MutatePriority::PreventUserInput, |_| std::future::pending::<()>()).await,
        );
    });
    advance(&mut ui, 16);
    turn(&mut ui, -5.0);
    advance(&mut ui, 100);
    assert_eq!(total.get(), 0.0);
    assert_eq!(*result.borrow(), None, "it goes on");
    job.cancel();
    advance(&mut ui, 16);
    // The next gesture scrolls, in a scroll of its own.
    turn(&mut ui, -5.0);
    assert_eq!(total.get(), 5.0);
}

// A press stops the wheel's animation, as any scroll (D883), and what's left of the turn is lost.
#[test]
fn a_press_stops_the_wheels_animation() {
    let (state, total) = total_state();
    let mut ui = box_of(state.clone());
    turn(&mut ui, -100.0);
    advance(&mut ui, 32);
    let at = total.get();
    assert!(at < 100.0);
    let finger = crate::scroll_support::Touch::down(&mut ui, CENTER);
    advance(&mut ui, 300);
    assert_eq!(total.get(), at);
    finger.up(&mut ui);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AbstractScrollableNode.kt, the wheel's coroutine is the node's: it ends with it
#[test]
fn the_wheels_scroll_ends_with_its_node() {
    let (state, total) = total_state();
    let shown = mutable_state_of(true);
    let (given, handle) = (state.clone(), shown.handle());
    let mut ui = sized(move || {
        if handle.get() {
            Box!(
                modifier = Modifier.size(100.dp()).scrollable(given.clone(), Orientation::Vertical)
            );
        }
    });
    turn(&mut ui, -100.0);
    advance(&mut ui, 32);
    assert!(state.is_scroll_in_progress());
    let at = total.get();
    shown.set(false);
    advance(&mut ui, 300);
    assert!(!state.is_scroll_in_progress());
    assert!(total.get() - at <= 16.0, "at most a frame on: {}", total.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedScroll_childPartialConsumptionForMouseWheel
#[test]
fn what_a_child_leaves_of_a_wheels_scroll_goes_to_its_parent() {
    let (inner_total, outer_total) = (Rc::new(Cell::new(0.0f32)), Rc::new(Cell::new(0.0f32)));
    let (into_inner, into_outer) = (inner_total.clone(), outer_total.clone());
    let inner = scrollable_state(move |delta| {
        into_inner.set(into_inner.get() + delta / 2.0);
        delta / 2.0
    });
    let outer = scrollable_state(move |delta| {
        into_outer.set(into_outer.get() + delta);
        delta
    });
    let mut ui = sized(move || {
        let inner = inner.clone();
        Box!(
            modifier = Modifier.size(100.dp()).scrollable(outer.clone(), Orientation::Vertical),
            {
                Box!(
                    modifier =
                        Modifier.size(100.dp()).scrollable(inner.clone(), Orientation::Vertical)
                );
            }
        );
    });
    turn(&mut ui, -200.0);
    advance(&mut ui, 300);
    assert!(inner_total.get() > 0.0);
    assert_eq!(inner_total.get(), outer_total.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedDiagonalScroll_mouseWheel_triggersOnAngle (its gestures after the first two)
#[test]
fn a_diagonal_turn_during_a_gesture_stays_with_the_scrollable_going() {
    let (vertical, down) = total_state();
    let (horizontal, across) = total_state();
    let mut ui = sized(move || {
        let horizontal = horizontal.clone();
        Box!(
            modifier = Modifier.size(100.dp()).scrollable(vertical.clone(), Orientation::Vertical),
            {
                Box!(
                    modifier = Modifier
                        .size(100.dp())
                        .scrollable(horizontal.clone(), Orientation::Horizontal)
                );
            }
        );
    });
    // Started vertical, then mostly horizontal: only the vertical one, which takes the event first.
    ui.host().send_pointer(PointerEvent::scroll(CENTER, Offset::new(-10.0, -50.0)));
    advance(&mut ui, 16);
    ui.host().send_pointer(PointerEvent::scroll(CENTER, Offset::new(-50.0, -10.0)));
    advance(&mut ui, 300);
    assert!(down.get() > 0.0);
    assert_eq!(across.get(), 0.0);
    // Past the gesture: a new direction.
    ui.host().send_pointer(PointerEvent::scroll(CENTER, Offset::new(-50.0, -10.0)));
    advance(&mut ui, 300);
    assert!(across.get() > 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_verticalScroll_mouseWheel_badMotionEvent
#[test]
fn a_turn_of_no_number_does_nothing() {
    let (state, total) = total_state();
    let mut ui = box_of(state);
    turn(&mut ui, f32::NAN);
    advance(&mut ui, 100);
    assert_eq!(total.get(), 0.0);
}
