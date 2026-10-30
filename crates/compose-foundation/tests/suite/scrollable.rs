//! `Modifier.scrollable` and `ScrollableState`: drags, the wheel and flings give a state its
//! deltas; scrolls run one at a time; the state says whether one goes on and which way the
//! last went.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::{keyframes, tween};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, MutableInteractionSource, Offset, PointerEvent, UnitExt};

use crate::scroll_support::{Touch, launch, swipe};

const TAG: &str = "scrollable";
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

/// A 100 dp box (density 1) with the modifier `scrollable` gives, as Compose's
/// `setScrollableContent`.
fn scrollable_content(scrollable: impl Fn() -> Modifier + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let modifier = Modifier.test_tag(TAG).size(100.dp()).then(scrollable());
        Box!({
            Box!(modifier = modifier.clone());
        });
    })
}

fn wheel(ui: &mut ComposeTestRule, delta: Offset) {
    ui.host().send_pointer(PointerEvent::scroll(CENTER, delta));
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_horizontalScroll
#[test]
fn a_horizontal_scrollable_takes_horizontal_drags() {
    let (state, total) = total_state();
    let mut ui =
        scrollable_content(move || Modifier.scrollable(state.clone(), Orientation::Horizontal));
    swipe(&mut ui, CENTER, CENTER + Offset::new(100.0, 0.0), 100);
    ui.wait_for_idle();
    let last = total.get();
    assert!(last > 0.0, "a drag to the right gives positive deltas: {last}");
    swipe(&mut ui, CENTER, CENTER + Offset::new(0.0, 100.0), 100);
    ui.wait_for_idle();
    assert_eq!(total.get(), last, "a vertical drag does nothing");
    swipe(&mut ui, CENTER, CENTER - Offset::new(100.0, 0.0), 100);
    ui.wait_for_idle();
    assert!(total.get() < 0.01, "back: {}", total.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_verticalScroll
#[test]
fn a_vertical_scrollable_takes_vertical_drags() {
    let (state, total) = total_state();
    let mut ui =
        scrollable_content(move || Modifier.scrollable(state.clone(), Orientation::Vertical));
    swipe(&mut ui, CENTER, CENTER + Offset::new(0.0, 100.0), 100);
    ui.wait_for_idle();
    let last = total.get();
    assert!(last > 0.0);
    swipe(&mut ui, CENTER, CENTER + Offset::new(100.0, 0.0), 100);
    ui.wait_for_idle();
    assert_eq!(total.get(), last);
    swipe(&mut ui, CENTER, CENTER - Offset::new(0.0, 100.0), 100);
    ui.wait_for_idle();
    assert!(total.get() < 0.01);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_horizontalScroll_reverse
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_verticalScroll_reversed
#[test]
fn a_reversed_scrollable_negates_the_drags() {
    for orientation in [Orientation::Horizontal, Orientation::Vertical] {
        let (state, total) = total_state();
        let mut ui = scrollable_content(move || {
            Modifier.scrollable_with(state.clone(), orientation, true, true, None, None)
        });
        let along = if orientation == Orientation::Horizontal {
            Offset::new(100.0, 0.0)
        } else {
            Offset::new(0.0, 100.0)
        };
        swipe(&mut ui, CENTER, CENTER + along, 100);
        ui.wait_for_idle();
        let last = total.get();
        assert!(last < 0.0, "{orientation:?}: {last}");
        swipe(&mut ui, CENTER, CENTER - along, 100);
        ui.wait_for_idle();
        assert!(total.get() > -0.01, "{orientation:?}: {}", total.get());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_verticalScroll_mouseWheel
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_horizontalScroll_mouseWheel
#[test]
fn the_wheel_scrolls_along_the_axis_only() {
    for orientation in [Orientation::Horizontal, Orientation::Vertical] {
        let (state, total) = total_state();
        let mut ui = scrollable_content(move || Modifier.scrollable(state.clone(), orientation));
        let (along, across) = if orientation == Orientation::Horizontal {
            (Offset::new(-100.0, 0.0), Offset::new(0.0, -100.0))
        } else {
            (Offset::new(0.0, -100.0), Offset::new(-100.0, 0.0))
        };
        // A wheel turned towards the start gives positive deltas, as a drag towards the end.
        wheel(&mut ui, along);
        let last = total.get();
        assert!(last > 0.0, "{orientation:?}: {last}");
        wheel(&mut ui, across);
        assert_eq!(total.get(), last, "{orientation:?}: the other wheel does nothing");
        wheel(&mut ui, Offset::ZERO - along);
        assert!(total.get() < 0.01, "{orientation:?}: {}", total.get());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_horizontalScroll_reverse_mouseWheel
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_verticalScroll_reversed_mouseWheel
#[test]
fn a_reversed_scrollable_negates_the_wheel() {
    for orientation in [Orientation::Horizontal, Orientation::Vertical] {
        let (state, total) = total_state();
        let mut ui = scrollable_content(move || {
            Modifier.scrollable_with(state.clone(), orientation, true, true, None, None)
        });
        let along = if orientation == Orientation::Horizontal {
            Offset::new(-100.0, 0.0)
        } else {
            Offset::new(0.0, -100.0)
        };
        wheel(&mut ui, along);
        assert!(total.get() < 0.0, "{orientation:?}: {}", total.get());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_disabledWontCallLambda
#[test]
fn a_disabled_scrollable_takes_no_drags() {
    let (state, total) = total_state();
    let enabled = Rc::new(RefCell::new(None::<MutableState<bool>>));
    let into = enabled.clone();
    let mut ui = scrollable_content(move || {
        let enabled = remember(|| mutable_state_of(true));
        *into.borrow_mut() = Some(enabled);
        Modifier.scrollable_with(
            state.clone(),
            Orientation::Horizontal,
            enabled.get(),
            false,
            None,
            None,
        )
    });
    swipe(&mut ui, CENTER, CENTER + Offset::new(100.0, 0.0), 100);
    ui.wait_for_idle();
    let previous = total.get();
    assert!(previous > 0.0);
    enabled.borrow().unwrap().set(false);
    ui.wait_for_idle();
    swipe(&mut ui, CENTER, CENTER + Offset::new(100.0, 0.0), 100);
    ui.wait_for_idle();
    assert_eq!(total.get(), previous);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_explicitDisposal
#[test]
fn a_fling_stops_when_the_modifier_goes() {
    let total = Rc::new(Cell::new(0.0f32));
    let expect_deltas = Rc::new(Cell::new(true));
    let (into, expected) = (total.clone(), expect_deltas.clone());
    let state = scrollable_state(move |delta| {
        assert!(expected.get(), "scrolled after the modifier went");
        into.set(into.get() + delta);
        delta
    });
    let emit = Rc::new(RefCell::new(None::<MutableState<bool>>));
    let into = emit.clone();
    let mut ui = scrollable_content(move || {
        let emit = remember(|| mutable_state_of(true));
        *into.borrow_mut() = Some(emit);
        if emit.get() {
            Modifier.scrollable(state.clone(), Orientation::Horizontal)
        } else {
            Modifier
        }
    });
    ui.set_auto_advance(false);
    // Fast: a fling follows.
    swipe(&mut ui, CENTER, CENTER + Offset::new(200.0, 0.0), 50);
    assert!(total.get() > 0.0);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    emit.borrow().unwrap().set(false);
    ui.advance_time_by_frame();
    let previous = total.get();
    expect_deltas.set(false);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(total.get(), previous);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_interactionSource
#[test]
fn a_drag_is_told_to_the_interaction_source() {
    let (state, _) = total_state();
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = scrollable_content(move || {
        Modifier.scrollable_with(
            state.clone(),
            Orientation::Horizontal,
            true,
            false,
            None,
            Some(given.clone()),
        )
    });
    assert!(!source.is_dragged());
    let mut finger = Touch::down(&mut ui, Offset::new(25.0, 50.0));
    finger.move_by(&mut ui, Offset::new(50.0, 0.0));
    ui.wait_for_idle();
    assert!(source.is_dragged(), "dragged");
    finger.up(&mut ui);
    ui.wait_for_idle();
    assert!(!source.is_dragged(), "the drag stopped");
}

/// A fling behavior recording its calls and returning `0`.
struct RecordingFling {
    calls: Cell<u32>,
    velocity: Cell<f32>,
}

impl FlingBehavior for RecordingFling {
    fn perform_fling<'a>(
        &'a self,
        _scope: ScrollScope,
        initial_velocity: f32,
    ) -> compose_foundation::gestures::LocalBoxFuture<'a, f32> {
        self.calls.set(self.calls.get() + 1);
        self.velocity.set(initial_velocity);
        Box::pin(async { 0.0 })
    }
}

fn recording_fling() -> Rc<RecordingFling> {
    Rc::new(RecordingFling { calls: Cell::new(0), velocity: Cell::new(f32::MAX) })
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_flingBehaviourCalled_whenVelocity0
#[test]
fn the_fling_behavior_is_called_with_no_velocity_after_a_pause() {
    let (state, _) = total_state();
    let fling = recording_fling();
    let behavior: Rc<dyn FlingBehavior> = fling.clone();
    let mut ui = scrollable_content(move || {
        Modifier.scrollable_with(
            state.clone(),
            Orientation::Horizontal,
            true,
            false,
            Some(behavior.clone()),
            None,
        )
    });
    let mut finger = Touch::down(&mut ui, CENTER);
    finger.move_by(&mut ui, Offset::new(115.0, 0.0));
    finger.advance_event_time(&mut ui, 3000);
    finger.up(&mut ui);
    ui.wait_for_idle();
    assert_eq!(fling.calls.get(), 1);
    assert!(fling.velocity.get().abs() < 0.01, "{}", fling.velocity.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_flingBehaviourCalled
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_flingBehaviourCalled_reversed
#[test]
fn the_fling_behavior_gets_the_release_velocity() {
    for reverse in [false, true] {
        let (state, _) = total_state();
        let fling = recording_fling();
        let behavior: Rc<dyn FlingBehavior> = fling.clone();
        let mut ui = scrollable_content(move || {
            Modifier.scrollable_with(
                state.clone(),
                Orientation::Horizontal,
                true,
                reverse,
                Some(behavior.clone()),
                None,
            )
        });
        // 1,000 px/s all along (Compose's `swipeWithVelocity` ends at it).
        swipe(&mut ui, CENTER, CENTER + Offset::new(160.0, 0.0), 160);
        ui.wait_for_idle();
        assert_eq!(fling.calls.get(), 1);
        let expected = if reverse { -1000.0 } else { 1000.0 };
        assert!((fling.velocity.get() - expected).abs() < 5.0, "{}", fling.velocity.get());
    }
}

/// A fling behavior scrolling 123 px with its scope.
struct ScrollingFling(Rc<Cell<f32>>);

impl FlingBehavior for ScrollingFling {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        _initial_velocity: f32,
    ) -> compose_foundation::gestures::LocalBoxFuture<'a, f32> {
        Box::pin(async move {
            self.0.set(scope.scroll_by(123.0));
            0.0
        })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_flingBehaviourCalled_correctScope
#[test]
fn the_fling_behavior_scrolls_the_state() {
    let (state, total) = total_state();
    let returned = Rc::new(Cell::new(0.0f32));
    let behavior: Rc<dyn FlingBehavior> = Rc::new(ScrollingFling(returned.clone()));
    let mut ui = scrollable_content(move || {
        Modifier.scrollable_with(
            state.clone(),
            Orientation::Horizontal,
            true,
            false,
            Some(behavior.clone()),
            None,
        )
    });
    let mut finger = Touch::down(&mut ui, CENTER);
    finger.move_by(&mut ui, Offset::new(100.0, 0.0));
    ui.wait_for_idle();
    let previous = total.get();
    assert!(previous > 0.0);
    finger.up(&mut ui);
    ui.wait_for_idle();
    assert_eq!(total.get(), previous + 123.0);
    assert_eq!(returned.get(), 123.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_scrollByWorksWithRepeatableAnimations
#[test]
fn animate_scroll_by_follows_a_keyframed_animation_back_and_forth() {
    let (state, total) = total_state();
    let scrolled = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Box!(modifier = Modifier.size(100.dp()).scrollable(state.clone(), Orientation::Horizontal));
    });
    ui.set_auto_advance(false);
    launch(async move {
        let spec = keyframes::<f32>(|frames| {
            frames.duration_millis = 2500;
            frames.at(0, 0.0);
            frames.at(500, 100.0);
            frames.at(1000, 100.0);
            frames.at(1500, 0.0);
            frames.at(2000, 0.0);
            frames.at(2500, 100.0);
        });
        let _ = scrolled.animate_scroll_by(100.0, spec).await;
    });
    // At the times Compose's test names (250, 750, 1250, … ms): each in the middle of one of
    // the animation's stretches or after one.
    let check = |ui: &mut ComposeTestRule, millis: u64, check: &dyn Fn(f32) -> bool| {
        ui.advance_time_by(Duration::from_millis(millis));
        assert!(check(total.get()), "{} at +{millis}", total.get());
    };
    check(&mut ui, 250, &|total| total > 0.0 && total < 100.0);
    check(&mut ui, 500, &|total| total == 100.0);
    check(&mut ui, 500, &|total| total > 0.0 && total < 100.0);
    check(&mut ui, 500, &|total| total == 0.0);
    check(&mut ui, 500, &|total| total > 0.0 && total < 100.0);
    check(&mut ui, 500, &|total| total == 100.0);
}

/// A 100 dp scrollable of `state` on a manual clock.
fn manual(state: DefaultScrollableState) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Box!(modifier = Modifier.size(100.dp()).scrollable(state.clone(), Orientation::Horizontal));
    });
    ui.set_auto_advance(false);
    ui
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_cancellingAnimateScrollUpdatesIsScrollInProgress
#[test]
fn cancelling_an_animated_scroll_ends_the_scroll() {
    let (state, _) = total_state();
    let mut ui = manual(state.clone());
    let scrolled = state.clone();
    let job = launch(async move {
        let _ = scrolled.animate_scroll_by(100.0, tween(1000)).await;
    });
    ui.advance_time_by(Duration::from_millis(500));
    assert!(state.is_scroll_in_progress());
    job.cancel();
    ui.wait_for_idle();
    assert!(!state.is_scroll_in_progress());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_preemptingAnimateScrollUpdatesIsScrollInProgress
#[test]
fn an_animated_scroll_interrupting_another_keeps_the_scroll_going() {
    let (state, total) = total_state();
    let mut ui = manual(state.clone());
    let scrolled = state.clone();
    launch(async move {
        let _ = scrolled.animate_scroll_by(100.0, tween(1000)).await;
    });
    ui.advance_time_by(Duration::from_millis(500));
    assert!(total.get() > 0.0 && total.get() < 100.0, "{}", total.get());
    assert!(state.is_scroll_in_progress());
    let scrolled = state.clone();
    launch(async move {
        let _ = scrolled.animate_scroll_by(-100.0, tween(1000)).await;
    });
    ui.wait_for_idle();
    assert!(state.is_scroll_in_progress(), "the second scroll goes on");
    ui.advance_time_by(Duration::from_millis(1000));
    ui.advance_time_by_frame();
    assert!(total.get() > -75.0 && total.get() < 0.0, "{}", total.get());
    assert!(!state.is_scroll_in_progress());
}

/// A fling behavior scrolling by NaN.
struct NanFling;

impl FlingBehavior for NanFling {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        _initial_velocity: f32,
    ) -> compose_foundation::gestures::LocalBoxFuture<'a, f32> {
        Box::pin(async move { scope.scroll_by(f32::NAN) })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, defaultScrollableState_scrollByWithNan_shouldFilterOutNan
#[test]
fn a_scroll_scope_drops_a_nan_delta() {
    let state = scrollable_state(|delta| {
        assert!(!delta.is_nan(), "a NaN reached the state");
        0.0
    });
    let behavior: Rc<dyn FlingBehavior> = Rc::new(NanFling);
    let mut ui = scrollable_content(move || {
        Modifier.scrollable_with(
            state.clone(),
            Orientation::Horizontal,
            true,
            false,
            Some(behavior.clone()),
            None,
        )
    });
    swipe(&mut ui, Offset::new(90.0, 50.0), Offset::new(10.0, 50.0), 200);
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollableState_checkLastScrollDirection
#[test]
fn the_state_knows_which_way_the_last_scroll_went() {
    let state = scrollable_state(|delta| delta);
    let mut ui = manual(state.clone());
    assert!(!state.last_scrolled_forward() && !state.last_scrolled_backward());
    let scrolled = state.clone();
    let job = launch(async move {
        let _ = scrolled.animate_scroll_by(100.0, tween(1000)).await;
    });
    ui.advance_time_by(Duration::from_millis(500));
    assert!(state.last_scrolled_forward() && !state.last_scrolled_backward());
    job.cancel();
    ui.wait_for_idle();
    assert!(state.last_scrolled_forward() && !state.last_scrolled_backward());
    let scrolled = state.clone();
    let job = launch(async move {
        let _ = scrolled.animate_scroll_by(-100.0, tween(1000)).await;
    });
    ui.advance_time_by(Duration::from_millis(500));
    assert!(!state.last_scrolled_forward() && state.last_scrolled_backward());
    job.cancel();
    ui.wait_for_idle();
    assert!(!state.last_scrolled_forward() && state.last_scrolled_backward());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedDrag
#[test]
fn what_an_inner_scrollable_leaves_of_a_drag_goes_to_the_outer_one() {
    let (outer, outer_total) = total_state();
    let inner_total = Rc::new(Cell::new(0.0f32));
    let into = inner_total.clone();
    let inner = scrollable_state(move |delta| {
        into.set(into.get() + delta / 2.0);
        delta / 2.0
    });
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let inner = Modifier
            .test_tag(TAG)
            .size(300.dp())
            .scrollable(inner.clone(), Orientation::Horizontal);
        Box!(
            modifier = Modifier.size(300.dp()).scrollable(outer.clone(), Orientation::Horizontal),
            content_alignment = Alignment::CENTER,
            {
                Box!(modifier = inner.clone());
            }
        );
    });
    // Slowly, with a pause at the end: no fling.
    let mut finger = Touch::down(&mut ui, Offset::new(150.0, 150.0));
    for _ in 0..20 {
        finger.move_by(&mut ui, Offset::new(10.0, 0.0));
    }
    finger.advance_event_time(&mut ui, 1000);
    finger.up(&mut ui);
    ui.wait_for_idle();
    assert!(inner_total.get() > 0.0 && outer_total.get() > 0.0);
    assert_eq!(outer_total.get(), inner_total.get(), "half to each");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_noMomentum_shouldChangeScrollStateAfterRelease
#[test]
fn a_drag_without_momentum_scrolls_by_its_distance_less_the_slop() {
    let scroll = ScrollState::new(0);
    let state = *scroll;
    let mut ui = scrollable_content(move || Modifier.scrollable(state, Orientation::Vertical));
    let mut finger = Touch::down(&mut ui, CENTER);
    for _ in 0..30 {
        finger.move_by(&mut ui, Offset::new(0.0, 10.0));
    }
    finger.advance_event_time(&mut ui, 3000);
    finger.up(&mut ui);
    ui.wait_for_idle();
    // 300 px less the touch slop (8 dp at density 1).
    assert_eq!(state.value(), 300 - 8);
    drop(scroll);
}

/// A mouse drag and drop from `start` to `end` in 100 ms, as Compose's `performMouseInput {
/// dragAndDrop(…) }`: the primary button pressed, moves an event period apart, released.
fn mouse_drag(ui: &mut ComposeTestRule, start: Offset, end: Offset) {
    let host = ui.host();
    host.send_pointer(PointerEvent::press(start));
    for step in 1..=6 {
        let fraction = step as f32 / 6.0;
        let at = Offset::new(
            start.x + (end.x - start.x) * fraction,
            start.y + (end.y - start.y) * fraction,
        );
        host.send_pointer(PointerEvent::moved(at, true));
    }
    host.send_pointer(PointerEvent::release(end));
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_mouseHorizontalDragDoesNotScroll
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_mouseVerticalDragDoesNotScroll
#[test]
fn a_mouse_drag_does_not_scroll() {
    for (orientation, along) in [
        (Orientation::Horizontal, Offset::new(100.0, 0.0)),
        (Orientation::Vertical, Offset::new(0.0, 100.0)),
    ] {
        let (state, total) = total_state();
        let mut ui = scrollable_content(move || Modifier.scrollable(state.clone(), orientation));
        mouse_drag(&mut ui, CENTER, CENTER + along);
        assert_eq!(total.get(), 0.0, "{orientation:?}");
    }
}

// Review of D852: a wheel turn scrolls in a scroll of the user-input priority, as Compose's
// `NonTouchScrollingLogic.userScroll` (`scrollLogic.scroll(MutatePriority.UserInput)`): the
// state says it's scrolling while it applies, and it stops an animated scroll going on.
#[test]
fn a_wheel_turn_is_a_user_scroll_that_stops_an_animation() {
    let seen: Rc<RefCell<Option<DefaultScrollableState>>> = Rc::default();
    let in_progress = Rc::new(Cell::new(None::<bool>));
    let (reader, flag) = (seen.clone(), in_progress.clone());
    let state = scrollable_state(move |delta| {
        if let Some(state) = reader.borrow().as_ref() {
            flag.set(Some(state.is_scroll_in_progress()));
        }
        delta
    });
    *seen.borrow_mut() = Some(state.clone());
    let scrolled = state.clone();
    let mut ui =
        scrollable_content(move || Modifier.scrollable(scrolled.clone(), Orientation::Vertical));
    wheel(&mut ui, Offset::new(0.0, 10.0));
    assert_eq!(in_progress.get(), Some(true), "scrolling while the wheel's delta applies");
    assert!(!state.is_scroll_in_progress(), "and not after");

    ui.set_auto_advance(false);
    let result: Rc<RefCell<Option<Result<f32, MutationInterrupted>>>> = Rc::default();
    let (animated, into) = (state.clone(), result.clone());
    launch(async move {
        *into.borrow_mut() = Some(animated.animate_scroll_by(1000.0, tween(1000)).await);
    });
    ui.advance_time_by(Duration::from_millis(100));
    assert!(state.is_scroll_in_progress(), "animating");
    wheel(&mut ui, Offset::new(0.0, 10.0));
    ui.advance_time_by(Duration::from_millis(16));
    assert!(matches!(*result.borrow(), Some(Err(MutationInterrupted))), "the wheel stopped it");
    ui.advance_time_by(Duration::from_millis(200));
    assert!(!state.is_scroll_in_progress(), "and ended");
}

// D888: a wheel's gesture is a scroll of the user-input priority, so a scroll of a higher priority
// going on refuses it, as `MutatorMutex.mutate` refuses, and goes on.
#[test]
fn a_scroll_of_a_higher_priority_refuses_a_wheel_turn() {
    let (state, total) = total_state();
    let scrolled = state.clone();
    let mut ui =
        scrollable_content(move || Modifier.scrollable(scrolled.clone(), Orientation::Vertical));
    let result: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
    let (held, into) = (state.clone(), result.clone());
    launch(async move {
        *into.borrow_mut() = Some(
            held.scroll(MutatePriority::PreventUserInput, |_| std::future::pending::<()>()).await,
        );
    });
    ui.wait_for_idle();
    wheel(&mut ui, Offset::new(0.0, 10.0));
    assert_eq!(total.get(), 0.0, "refused");
    assert!(state.is_scroll_in_progress(), "the other scroll goes on");
    assert_eq!(*result.borrow(), None);
}

// D888: a wheel's gesture holds the state's mutex: what its deltas start at a lower priority is
// refused, as a mutation under a running one is.
#[test]
fn a_wheel_turn_refuses_a_lower_scroll_started_from_its_deltas() {
    let refused: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
    let seen: Rc<RefCell<Option<DefaultScrollableState>>> = Rc::default();
    let (reader, into) = (seen.clone(), refused.clone());
    let state = scrollable_state(move |delta| {
        let Some(state) = reader.borrow().clone() else { return delta };
        let into = into.clone();
        let job = launch(async move {
            *into.borrow_mut() = Some(state.stop_scroll(MutatePriority::Default).await);
        });
        compose_runtime::platform::poll_now(job);
        delta
    });
    *seen.borrow_mut() = Some(state.clone());
    let scrolled = state.clone();
    let mut ui =
        scrollable_content(move || Modifier.scrollable(scrolled.clone(), Orientation::Vertical));
    wheel(&mut ui, Offset::new(0.0, 10.0));
    assert_eq!(*refused.borrow(), Some(Err(MutationInterrupted)));
    assert!(!state.is_scroll_in_progress());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/ScrollableState.kt, DefaultScrollableState's scope notes the direction of each scroll, a wheel turn's too
#[test]
fn a_wheel_turn_notes_which_way_it_went() {
    let (state, _) = total_state();
    let scrolled = state.clone();
    let mut ui =
        scrollable_content(move || Modifier.scrollable(scrolled.clone(), Orientation::Vertical));
    // A wheel turned towards the end gives positive deltas here, as a drag towards it.
    wheel(&mut ui, Offset::new(0.0, -10.0));
    assert!(state.last_scrolled_forward() && !state.last_scrolled_backward());
    wheel(&mut ui, Offset::new(0.0, 10.0));
    assert!(state.last_scrolled_backward() && !state.last_scrolled_forward());
}

/// An app's own state: the required functions only, handed to an inner state.
#[derive(Clone)]
struct OwnState(DefaultScrollableState);

impl ScrollableState for OwnState {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: compose_foundation::gestures::ScrollBlock<'a>,
    ) -> compose_foundation::gestures::LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        self.0.scroll_boxed(priority, block)
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.0.dispatch_raw_delta(delta)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.0.is_scroll_in_progress()
    }
}

impl ComposeArg for OwnState {
    fn same(&self, other: &Self) -> bool {
        self.0.same(&other.0)
    }
}

// D888: a state implementing the trait itself gets the wheel through its `scroll` as any: it says
// it's scrolling while the delta applies, and the wheel stops an animated scroll.
#[test]
fn a_wheel_turn_scrolls_an_apps_own_state() {
    let seen: Rc<RefCell<Option<DefaultScrollableState>>> = Rc::default();
    let in_progress = Rc::new(Cell::new(None::<bool>));
    let total = Rc::new(Cell::new(0.0f32));
    let (reader, flag, into) = (seen.clone(), in_progress.clone(), total.clone());
    let inner = scrollable_state(move |delta| {
        if let Some(state) = reader.borrow().as_ref() {
            flag.set(Some(state.is_scroll_in_progress()));
        }
        into.set(into.get() + delta);
        delta
    });
    *seen.borrow_mut() = Some(inner.clone());
    let state = OwnState(inner);
    let scrolled = state.clone();
    let mut ui =
        scrollable_content(move || Modifier.scrollable(scrolled.clone(), Orientation::Vertical));
    wheel(&mut ui, Offset::new(0.0, 10.0));
    assert_eq!(total.get().abs(), 10.0);
    assert_eq!(in_progress.get(), Some(true));
    assert!(!state.is_scroll_in_progress());

    ui.set_auto_advance(false);
    let result: Rc<RefCell<Option<Result<f32, MutationInterrupted>>>> = Rc::default();
    let (animated, into) = (state.clone(), result.clone());
    launch(async move {
        *into.borrow_mut() = Some(animated.animate_scroll_by(1000.0, tween(1000)).await);
    });
    ui.advance_time_by(Duration::from_millis(100));
    assert!(state.is_scroll_in_progress(), "animating");
    wheel(&mut ui, Offset::new(0.0, 10.0));
    ui.advance_time_by(Duration::from_millis(16));
    assert!(matches!(*result.borrow(), Some(Err(MutationInterrupted))), "the wheel stopped it");
    ui.advance_time_by(Duration::from_millis(200));
    assert!(!state.is_scroll_in_progress(), "and ended");
}

// Review of D855: `Modifier.scrollable` without an interaction source reports its drags to none
// (Compose's `scrollable(interactionSource = null)` emits nothing), not to its state's own
// source, which only `vertical_scroll` and the lazy layouts give it.
#[test]
fn a_scrollable_without_a_source_reports_its_drags_nowhere() {
    let owner = ScrollState::new(0);
    let state = *owner;
    let mut ui = scrollable_content(move || {
        Modifier.scrollable_with(state, Orientation::Vertical, true, false, None, None)
    });
    let mut finger = Touch::down(&mut ui, CENTER);
    finger.move_by(&mut ui, Offset::new(0.0, -20.0));
    assert!(!state.interaction_source().is_dragged());
    finger.up(&mut ui);
}

// Review of D855: `is_dragged` counts the drags going on, as Compose's `collectIsDraggedAsState`
// counts the starts not stopped: two scrollables sharing a source, one released while the
// other drags, still dragged.
#[test]
fn a_shared_source_is_dragged_while_any_drag_goes_on() {
    let source = MutableInteractionSource::new();
    let shared = source.clone();
    let (first, second) = (total_state().0, total_state().0);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (first, second, shared) = (first.clone(), second.clone(), shared.clone());
        Column!(modifier = Modifier.test_tag("column"), {
            for state in [first.clone(), second.clone()] {
                let source = Some(shared.clone());
                Box!(
                    modifier = Modifier.size(100.dp()).scrollable_with(
                        state,
                        Orientation::Horizontal,
                        true,
                        false,
                        None,
                        source
                    )
                );
            }
        });
    });
    let seen = source.clone();
    ui.on_node_with_tag("column").perform_touch_input(move |touch| {
        let (top, bottom) = (Offset::new(50.0, 50.0), Offset::new(50.0, 150.0));
        touch.down_pointer(0, top);
        touch.move_pointer_by(0, Offset::new(20.0, 0.0), 16);
        touch.down_pointer(1, bottom);
        touch.move_pointer_by(1, Offset::new(20.0, 0.0), 16);
        assert!(seen.is_dragged());
        touch.up_pointer(0);
        assert!(seen.is_dragged(), "the other still drags");
        touch.up_pointer(1);
        assert!(!seen.is_dragged());
    });
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt, DefaultFlingBehavior.performFling (no test of Compose's: each frame's distance is the decay's at the play time Compose's `animateDecay` computes)
#[test]
fn a_default_fling_is_the_spline_based_decay_frame_by_frame() {
    use compose_animation::{FloatDecayAnimationSpec, SplineBasedFloatDecayAnimationSpec};
    let density = 2.625;
    for velocity in [12_000.0f32, -5000.0, 800.0] {
        let spec = SplineBasedFloatDecayAnimationSpec::new(compose_ui::Density::new(density));
        let duration = spec.duration_nanos(0.0, velocity);
        let mut ui = ComposeTestRule::with_manual_clock(|| {});
        let distances = Rc::new(RefCell::new(Vec::new()));
        let seen = distances.clone();
        let covered = Rc::new(Cell::new(0.0f32));
        let done = Rc::new(Cell::new(false));
        let finished = done.clone();
        let scope = compose_runtime::CoroutineScope::new();
        scope.launch(async move {
            let scroll = ScrollScope::new(move |delta| {
                covered.set(covered.get() + delta);
                seen.borrow_mut().push(covered.get());
                delta
            });
            let behavior = DefaultFlingBehavior::new(density);
            behavior.perform_fling(scroll, velocity).await;
            finished.set(true);
        });
        // The first frame is its start.
        ui.advance_time_by_frame();
        let (mut expected, mut frame, mut last) = (0.0f32, 0u64, 0.0f32);
        while !done.get() {
            frame += 1;
            ui.advance_time_by_frame();
            // Kotlin's `(frameTime - startTime) / durationScale`, a `Long` over a `Float` of 1.
            let elapsed = frame * 16_000_000;
            let play_time = (elapsed as f32 / 1.0) as i64;
            let target = if play_time >= duration {
                spec.target_value(0.0, velocity)
            } else {
                spec.value_from_nanos(play_time, 0.0, velocity)
            };
            let delta = target - last;
            last = target;
            expected += delta;
            assert_eq!(
                distances.borrow().last().copied().unwrap_or(0.0),
                expected,
                "{velocity} px/s at frame {frame}"
            );
            assert!(frame < 1_000, "the fling never ended");
        }
        assert!(duration as u64 <= frame * 16_000_000, "it ends at its duration");
        assert!(duration as u64 > (frame - 1) * 16_000_000, "not later: {frame} frames");
    }
}
