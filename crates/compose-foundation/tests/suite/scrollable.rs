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
    finger.advance_event_time(3000);
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
    finger.advance_event_time(1000);
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
    finger.advance_event_time(3000);
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
