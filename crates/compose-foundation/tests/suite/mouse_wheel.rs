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
use compose_ui::{
    Dp, IntSize, Offset, PointerEvent, PointerEventPass, PointerEventType, UnitExt, WheelStep,
};

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

/// The totals after each frame of `frames`.
fn frames_of(ui: &mut ComposeTestRule, total: &Cell<f32>, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|_| {
            advance(ui, 16);
            total.get()
        })
        .collect()
}

const P: &str = "the path of its gesture";

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, a turn the other way mid-tween: the frame's delta, a new step back, then a tween from there on the same animation (sequential: its first frame at once)
#[test]
fn a_turn_back_during_the_tween_reverses_it() {
    let (state, total) = total_state();
    let mut ui = box_of(state);
    turn(&mut ui, -100.0);
    assert_eq!(frames_of(&mut ui, &total, 2), [6.0, 22.0], "{P}");
    turn(&mut ui, 100.0);
    // 38 at the frame, then 6 back at once; then down to the sum of the turns, 0.
    assert_eq!(frames_of(&mut ui, &total, 4), [32.0, 16.0, 0.0, 0.0], "{P}");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, waitNextScrollDelta(ScrollProgressTimeout - durationMillis): the hold counts the tween's time
#[test]
fn the_hold_counts_the_time_of_the_tween() {
    let (state, _) = total_state();
    let mut ui = box_of(state.clone());
    // 24 ms of tween, from its first frame at 16 ms: 26 ms of hold left at its end.
    turn(&mut ui, -30.0);
    advance(&mut ui, 64);
    assert!(state.is_scroll_in_progress(), "the hold goes on");
    advance(&mut ui, 32);
    assert!(!state.is_scroll_in_progress(), "over 50 ms from the tween's start, not its end");

    // Longer than the hold: none.
    let (state, _) = total_state();
    let mut ui = box_of(state.clone());
    turn(&mut ui, -100.0);
    advance(&mut ui, 96);
    assert!(state.is_scroll_in_progress(), "its last frame is the next one");
    advance(&mut ui, 16);
    assert!(!state.is_scroll_in_progress());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, isLowScrollingDelta on what a step left: under half a pixel unused, the tween goes on
#[test]
fn a_tween_goes_on_past_a_step_nearly_used() {
    let total = Rc::new(Cell::new(0.0f32));
    let into = total.clone();
    let state = scrollable_state(move |delta| {
        into.set(into.get() + delta);
        delta - 0.4f32.copysign(delta)
    });
    let mut ui = box_of(state);
    turn(&mut ui, -100.0);
    advance(&mut ui, 300);
    assert_eq!(total.get(), 100.0);
}

// Compose: the same, half a pixel or more unused: the tween stops
#[test]
fn a_tween_stops_at_a_step_not_used() {
    let total = Rc::new(Cell::new(0.0f32));
    let into = total.clone();
    let state = scrollable_state(move |delta| {
        into.set(into.get() + delta);
        delta - 0.6f32.copysign(delta)
    });
    let mut ui = box_of(state);
    turn(&mut ui, -100.0);
    advance(&mut ui, 300);
    assert_eq!(total.get(), 22.0, "the step, used or not; the tween's first delta, and no more");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, waitNextScrollDelta: a turn too small to see in the hold ends the gesture
#[test]
fn a_turn_too_small_to_see_in_the_hold_ends_the_gesture() {
    let (state, total) = total_state();
    let mut ui = box_of(state.clone());
    turn(&mut ui, -5.0);
    advance(&mut ui, 16);
    turn(&mut ui, -0.3);
    advance(&mut ui, 16);
    assert_eq!(total.get(), 5.0);
    assert!(!state.is_scroll_in_progress());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, shouldCancelAnimation: turns bringing the target back to where it is end the tween
#[test]
fn turns_back_to_where_the_tween_is_end_it() {
    let (state, total) = total_state();
    let mut ui = box_of(state);
    turn(&mut ui, -100.0);
    assert_eq!(frames_of(&mut ui, &total, 2), [6.0, 22.0], "{P}");
    // The next frame's value is 38, a fifth of a pixel from where they ask.
    turn(&mut ui, 61.8);
    assert_eq!(frames_of(&mut ui, &total, 3), [38.0, 38.0, 38.0], "{P}");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, animateMouseWheelScroll: frames whose delta is too small to see wait until it isn't (a low density)
#[test]
fn a_tweens_deltas_too_small_to_see_add_up() {
    let deltas: Rc<RefCell<Vec<f32>>> = Rc::default();
    let into = deltas.clone();
    let state = scrollable_state(move |delta| {
        into.borrow_mut().push(delta);
        delta
    });
    let given = state.clone();
    let mut ui = compose_test_with(
        IntSize::new(300, 300),
        compose_ui::Density::new(0.02),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
        move || {
            Box!(
                modifier =
                    Modifier.size(5000.dp()).scrollable(given.clone(), Orientation::Vertical)
            );
        },
    );
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    // 6 dp is 0.12 px; 2 px then take 94 ms, 0.32 px a frame.
    turn(&mut ui, -2.0);
    advance(&mut ui, 200);
    let deltas = deltas.borrow();
    assert!(deltas.len() > 1, "{deltas:?}");
    assert!(deltas[1..].iter().all(|delta| *delta >= 0.5), "{deltas:?}");
}

/// A custom behavior's velocities, in a box.
fn recording_box(state: DefaultScrollableState) -> (ComposeTestRule, Rc<RecordingFling>) {
    let fling = Rc::new(RecordingFling::default());
    let behavior = fling.clone();
    let ui = sized(move || {
        let behavior: Rc<dyn FlingBehavior> = behavior.clone();
        Box!(
            modifier = Modifier.size(100.dp()).scrollable_with(
                state.clone(),
                Orientation::Vertical,
                true,
                false,
                Some(behavior),
                None
            )
        );
    });
    (ui, fling)
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, onScrollStopped's velocity of a single data point: at most the animation's speed
#[test]
fn a_single_turns_velocity_is_at_most_the_animations_speed() {
    let (state, _) = total_state();
    let (mut ui, fling) = recording_box(state);
    turn(&mut ui, -200.0);
    advance(&mut ui, 500);
    assert_eq!(*fling.velocities.borrow(), vec![1000.0]);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, trackVelocity: several turns' speed, measured on the raw deltas
#[test]
fn several_turns_give_their_own_velocity() {
    let (state, _) = total_state();
    let (mut ui, fling) = recording_box(state);
    for _ in 0..3 {
        ui.host().send_pointer(
            PointerEvent::scroll(CENTER, Offset::new(0.0, -50.0))
                .at_time(compose_runtime::platform::now_nanos()),
        );
        advance(&mut ui, 16);
    }
    advance(&mut ui, 500);
    let velocities = fling.velocities.borrow();
    assert_eq!(velocities.len(), 1);
    assert!(velocities[0] > 1000.0, "beyond the cap of a single turn: {velocities:?}");
}

/// A precise turn: a trackpad's, or a high-resolution wheel's.
fn precise_turn(ui: &mut ComposeTestRule, pixels: f32) {
    ui.host().send_pointer(
        PointerEvent::scroll(CENTER, Offset::new(0.0, pixels)).with_precise_wheel(true),
    );
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, shouldApplyImmediately: a precise turn has no step and no tween, the first turn's flag holding for the gesture
#[test]
fn a_precise_turn_applies_at_once() {
    let (state, total) = total_state();
    let mut ui = box_of(state.clone());
    precise_turn(&mut ui, -100.0);
    assert_eq!(total.get(), 100.0, "all of it, in the event");
    assert!(state.is_scroll_in_progress(), "held as any");
    // Later turns join it as what it is, whatever they say.
    turn(&mut ui, -100.0);
    advance(&mut ui, 16);
    assert_eq!(total.get(), 200.0, "at the next frame, whole");
    advance(&mut ui, 200);
    assert!(!state.is_scroll_in_progress());
}

// Compose: the same, the other way: a precise turn joining an ordinary gesture is animated
#[test]
fn a_precise_turn_joining_an_ordinary_gesture_is_animated() {
    let (state, total) = total_state();
    let mut ui = box_of(state);
    turn(&mut ui, -5.0);
    advance(&mut ui, 16);
    precise_turn(&mut ui, -100.0);
    advance(&mut ui, 16);
    assert_eq!(total.get(), 11.0, "a step");
    advance(&mut ui, 300);
    assert_eq!(total.get(), 105.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, dispatchMouseWheelScroll: the 0.5 px early return holds for a precise turn too
#[test]
fn a_precise_turn_too_small_to_see_is_dropped() {
    let (state, total) = total_state();
    let mut ui = box_of(state.clone());
    precise_turn(&mut ui, -0.4);
    assert_eq!(total.get(), 0.0);
    assert!(!state.is_scroll_in_progress());
}

// A panic in a delta's consumer takes the wheel's task with it: nothing stays held, and the next
// turn scrolls.
#[test]
fn the_wheel_goes_on_after_a_panic_in_its_consumer() {
    let panics = Rc::new(Cell::new(true));
    let total = Rc::new(Cell::new(0.0f32));
    let (armed, into) = (panics.clone(), total.clone());
    let state = scrollable_state(move |delta| {
        assert!(!armed.get(), "the delta's consumer panics");
        into.set(into.get() + delta);
        delta
    });
    let mut ui = box_of(state.clone());
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| turn(&mut ui, -5.0)));
    assert!(unwound.is_err());
    panics.set(false);
    assert!(!state.is_scroll_in_progress(), "its scroll ended");
    turn(&mut ui, -5.0);
    assert_eq!(total.get(), 5.0);
}

/// A precise turn of `units` (so it applies at once) in `step`.
fn turn_in(ui: &mut ComposeTestRule, units: f32, step: WheelStep) {
    ui.host().send_pointer(
        PointerEvent::scroll(CENTER, Offset::new(0.0, units))
            .with_precise_wheel(true)
            .with_wheel_step(step),
    );
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, onScrollingEvent: `calculateMouseWheelScroll(pointerEvent, bounds)`, in the scrollable, with its own bounds; the desktop steps from the JetBrains fork's `DesktopScrollable.desktop.kt`, 10 dp, height / 20, sqrt(height), times `scrollAmount`
#[test]
fn a_units_worth_comes_from_the_step_and_the_scrollable() {
    let steps = [
        (WheelStep::Pixels, 1.0, 1.0),
        (WheelStep::Dp(10.0), 10.0, 10.0),
        (WheelStep::HeightOverTwenty { scroll_amount: 3.0 }, 15.0, 38.4),
        (WheelStep::SquareRootOfHeight { scroll_amount: 3.0 }, 30.0, 48.0),
    ];
    for (step, in_100_px, in_256_px) in steps {
        for (height, expected) in [(100.0, in_100_px), (256.0, in_256_px)] {
            let (state, total) = total_state();
            let mut ui = sized(move || {
                Box!(
                    modifier = Modifier
                        .size((100.dp(), Dp(height)))
                        .scrollable(state.clone(), Orientation::Vertical)
                );
            });
            turn_in(&mut ui, -2.0, step);
            assert!((total.get() - 2.0 * expected).abs() < 0.001, "{step:?} at {height}");
        }
    }
}

// Compose: the same, a turn's units are asked of each scrollable it reaches: the one inside uses it at its height
#[test]
fn a_step_is_the_scrollables_the_turn_reaches() {
    let (inner, inner_total) = total_state();
    let (outer, outer_total) = total_state();
    let mut ui = sized(move || {
        let inner = inner.clone();
        Box!(
            modifier = Modifier
                .size((100.dp(), 256.dp()))
                .scrollable(outer.clone(), Orientation::Vertical),
            {
                Box!(
                    modifier =
                        Modifier.size(100.dp()).scrollable(inner.clone(), Orientation::Vertical)
                );
            }
        );
    });
    turn_in(&mut ui, -1.0, WheelStep::SquareRootOfHeight { scroll_amount: 1.0 });
    assert_eq!((inner_total.get(), outer_total.get()), (10.0, 0.0));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, onScrollingEvent: `with(density) { calculateMouseWheelScroll(...) }`, a dp step at the host's density
#[test]
fn a_step_in_dp_counts_the_density() {
    let (state, total) = total_state();
    let mut ui = compose_test_with(
        IntSize::new(300, 300),
        compose_ui::Density::new(2.0),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
        move || {
            Box!(
                modifier = Modifier.size(100.dp()).scrollable(state.clone(), Orientation::Vertical)
            );
        },
    );
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    turn_in(&mut ui, -1.0, WheelStep::Dp(10.0));
    assert_eq!(total.get(), 20.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollingLogic.kt, onScrollingEvent: `canConsumeDelta(scrollDelta.toScrollValue())`, of the pixels worked out: a turn worth none starts no scroll
#[test]
fn a_turn_worth_nothing_here_starts_no_scroll() {
    let (state, total) = total_state();
    let mut ui = box_of(state.clone());
    turn_in(&mut ui, -1.0, WheelStep::Dp(0.0));
    assert!(!state.is_scroll_in_progress());
    assert_eq!(total.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/MouseWheelScrollable.kt: canConsumeDelta
#[test]
fn a_turn_worth_nothing_here_goes_on_to_the_parents() {
    let (state, _total) = total_state();
    let seen: Rc<Cell<Option<bool>>> = Rc::default();
    let noted = seen.clone();
    let mut ui = sized(move || {
        let noted = noted.clone();
        let state = state.clone();
        Box!(
            modifier = Modifier.pointer_input((), move |scope| {
                let noted = noted.clone();
                async move {
                    loop {
                        let event = scope.await_pointer_event(PointerEventPass::Main).await;
                        let change = &event.changes[0];
                        if event.kind == PointerEventType::Scroll {
                            noted.set(Some(change.is_consumed()));
                        }
                    }
                }
            }),
            {
                Box!(
                    modifier =
                        Modifier.size(100.dp()).scrollable(state.clone(), Orientation::Vertical)
                );
            }
        );
    });
    turn_in(&mut ui, -1.0, WheelStep::Dp(0.0));
    assert_eq!(seen.get(), Some(false), "not consumed");
    turn_in(&mut ui, -1.0, WheelStep::Dp(1.0));
    assert_eq!(seen.get(), Some(true), "consumed");
}
