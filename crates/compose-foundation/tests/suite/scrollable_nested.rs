//! `Modifier.scrollable` among others, and around flings: scenarios of Compose's `ScrollableTest`
//! not in `scrollable.rs`, `nested_fling.rs` or `review_scrolling.rs`, rewritten.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::gestures::LocalBoxFuture;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    IntSize, MutableInteractionSource, NestedScrollConnection, NestedScrollSource, Offset, UnitExt,
    ViewConfiguration,
};

use crate::interaction_helpers::{kinds, record};

const CENTER: Offset = Offset::new(150.0, 150.0);

/// A state consuming `fraction` of each delta, adding up what it consumed.
fn consuming(fraction: f32) -> (DefaultScrollableState, Rc<Cell<f32>>) {
    let total = Rc::new(Cell::new(0.0f32));
    let adding = total.clone();
    let state = scrollable_state(move |delta| {
        adding.set(adding.get() + delta * fraction);
        delta * fraction
    });
    (state, total)
}

fn window(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), content);
    ui.wait_for_idle();
    ui
}

fn sideways(state: &DefaultScrollableState) -> Modifier {
    Modifier.size(300.dp()).scrollable(state.clone(), Orientation::Horizontal)
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_startWithoutSlop_ifFlinging
#[test]
fn a_press_during_a_fling_drags_at_once() {
    let (state, total) = consuming(1.0);
    let given = state.clone();
    let mut ui = window(move || Box!(modifier = sideways(&given)));
    ui.set_auto_advance(false);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(CENTER, CENTER + Offset::new(200.0, 0.0), 4000.0, Some(100));
    });
    assert!(total.get() > 0.0);
    let before = total.get();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let flung = total.get();
    assert!(flung > before);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(115.0, 0.0));
        touch.up();
    });
    assert_eq!(total.get(), flung + 115.0, "no slop");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_blocksDownEvents_ifFlingingCaught
#[test]
fn a_press_catching_a_fling_clicks_nothing() {
    let (state, total) = consuming(1.0);
    let clicks = Rc::new(Cell::new(0));
    let (given, counted) = (state.clone(), clicks.clone());
    let mut ui = window(move || {
        let counted = counted.clone();
        Box!(modifier = sideways(&given), {
            let counted = counted.clone();
            Box!(
                modifier =
                    Modifier.size(300.dp()).clickable(move || counted.set(counted.get() + 1))
            );
        });
    });
    ui.set_auto_advance(false);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(CENTER, CENTER + Offset::new(200.0, 0.0), 4000.0, Some(100));
    });
    let before = total.get();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(total.get() > before);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.up();
    });
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(clicks.get(), 0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedFling
#[test]
fn half_consumed_by_the_inner_the_rest_goes_to_the_outer_through_a_fling() {
    let (outer, outer_total) = consuming(1.0);
    let (inner, inner_total) = consuming(0.5);
    let mut ui = window(move || {
        let inner = inner.clone();
        Box!(modifier = sideways(&outer), {
            Box!(modifier = sideways(&inner));
        });
    });
    ui.set_auto_advance(false);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(CENTER, CENTER + Offset::new(200.0, 0.0), 300);
    });
    assert!(inner_total.get() > 0.0);
    assert_eq!(outer_total.get(), inner_total.get(), "exactly half each");
    let equal = inner_total.get();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert!(inner_total.get() > equal);
    assert!(outer_total.get() > equal);
}

/// Takes `fraction` of what's offered before the scroll, and all the velocity.
struct PreConsuming {
    fraction: f32,
    last_available: Rc<Cell<f32>>,
}

impl NestedScrollConnection for PreConsuming {
    fn on_pre_scroll(&self, available: Offset, _: NestedScrollSource) -> Offset {
        self.last_available.set(available.x);
        Offset::new(available.x * self.fraction, available.y * self.fraction)
    }

    fn on_pre_fling(&self, available: Offset) -> LocalBoxFuture<'_, Offset> {
        Box::pin(async move { available })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedScrollAbove_respectsPreConsumption
#[test]
fn what_a_parent_takes_first_and_all_the_velocity_leave_no_fling() {
    let last_available = Rc::new(Cell::new(0.0f32));
    let seen = last_available.clone();
    let value = Rc::new(Cell::new(0.0f32));
    let adding = value.clone();
    let fraction = 0.7;
    let state = scrollable_state(move |delta| {
        let expected = seen.get() * (1.0 - fraction);
        assert!((delta - expected).abs() < 0.01, "{delta} of {}", seen.get());
        adding.set(adding.get() + delta);
        delta
    });
    let connection: Rc<dyn NestedScrollConnection> =
        Rc::new(PreConsuming { fraction, last_available });
    let mut ui = window(move || {
        let state = state.clone();
        Box!(modifier = Modifier.size(300.dp()).nested_scroll(connection.clone()), {
            Box!(modifier = sideways(&state));
        });
    });
    ui.set_auto_advance(false);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(CENTER, CENTER + Offset::new(200.0, 0.0), 300);
    });
    let at_release = value.get();
    assert!(at_release > 0.0);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(value.get(), at_release);
}

/// Told what's left of each scroll and of the fling.
struct PostCycles {
    expected_left: Rc<Cell<f32>>,
    velocity: f32,
    post_flings: Rc<Cell<u32>>,
}

impl NestedScrollConnection for PostCycles {
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        // The content scrolls the pointer's way here: a fraction of what's asked is left.
        assert!(
            (available.x.abs() - self.expected_left.get().abs()).abs() < 0.001,
            "{} left, {} expected",
            available.x,
            self.expected_left.get()
        );
        if source == NestedScrollSource::SideEffect { Offset::ZERO } else { available }
    }

    fn on_post_fling(&self, consumed: Offset, available: Offset) -> LocalBoxFuture<'_, Offset> {
        Box::pin(async move {
            let expected = self.velocity - consumed.x.abs();
            assert!(consumed.x.abs() < self.velocity);
            assert!((available.x.abs() - expected).abs() < 0.05 * expected, "{available:?}");
            self.post_flings.set(self.post_flings.get() + 1);
            available
        })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedScrollAbove_proxiesPostCycles, scrollable_nestedScrollAbove_reversed_proxiesPostCycles
#[test]
fn a_parent_is_told_what_is_left_of_scrolls_and_of_the_fling() {
    for reverse in [false, true] {
        let expected_left = Rc::new(Cell::new(0.0f32));
        let post_flings = Rc::new(Cell::new(0));
        let left = expected_left.clone();
        let state = scrollable_state(move |delta| {
            let used = delta * 0.345;
            left.set(delta - used);
            used
        });
        let connection: Rc<dyn NestedScrollConnection> = Rc::new(PostCycles {
            expected_left,
            velocity: 5000.0,
            post_flings: post_flings.clone(),
        });
        let mut ui = window(move || {
            let state = state.clone();
            Box!(modifier = Modifier.size(300.dp()).nested_scroll(connection.clone()), {
                Box!(
                    modifier = Modifier.size(300.dp()).scrollable_with(
                        state.clone(),
                        Orientation::Horizontal,
                        true,
                        reverse,
                        None,
                        None
                    )
                );
            });
        });
        ui.on_root().perform_touch_input(|touch| {
            touch.swipe_with_velocity(CENTER, CENTER + Offset::new(400.0, 0.0), 5000.0, Some(300));
        });
        ui.wait_for_idle();
        assert_eq!(post_flings.get(), 1);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedScroll_allowParentWhenDisabled
#[test]
fn a_disabled_child_leaves_the_drag_to_its_parent() {
    let (parent, parent_total) = consuming(1.0);
    let (child, child_total) = consuming(1.0);
    let mut ui = window(move || {
        let child = child.clone();
        Box!(modifier = sideways(&parent), {
            Box!(
                modifier = Modifier.size(200.dp()).scrollable_with(
                    child.clone(),
                    Orientation::Horizontal,
                    false,
                    false,
                    None,
                    None
                )
            );
        });
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 100.0), Offset::new(200.0, 100.0), 200);
    });
    ui.wait_for_idle();
    assert_eq!(child_total.get(), 0.0);
    assert!(parent_total.get() > 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedScroll_disabledConnectionNoOp
#[test]
fn a_disabled_scrollable_between_two_passes_what_is_left_through() {
    let (parent, parent_total) = consuming(0.5);
    let (middle, middle_total) = consuming(0.5);
    let (child, child_total) = consuming(0.5);
    let mut ui = window(move || {
        let (middle, child) = (middle.clone(), child.clone());
        Box!(modifier = sideways(&parent), {
            let child = child.clone();
            Box!(
                modifier = Modifier.size(200.dp()).scrollable_with(
                    middle.clone(),
                    Orientation::Horizontal,
                    false,
                    false,
                    None,
                    None
                ),
                {
                    Box!(
                        modifier = Modifier
                            .size(200.dp())
                            .scrollable(child.clone(), Orientation::Horizontal)
                    );
                }
            );
        });
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 100.0), Offset::new(200.0, 100.0), 200);
    });
    ui.wait_for_idle();
    assert!(child_total.get() > 0.0);
    assert_eq!(middle_total.get(), 0.0);
    assert!(parent_total.get() > 0.0);
}

/// Stops a fling by cancelling it from a post scroll with something left.
struct CancellingFling(Rc<Cell<u32>>);

impl NestedScrollConnection for CancellingFling {
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        if source == NestedScrollSource::SideEffect && available != Offset::ZERO {
            self.0.set(self.0.get() + 1);
            compose_runtime::platform::cancel_current_task();
        }
        Offset::ZERO
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedFlingCancellation_shouldPreventDeltasFromPropagating
#[test]
fn a_fling_cancelled_by_a_parent_gives_the_child_no_more_deltas() {
    let child_total = Rc::new(Cell::new(0.0f32));
    let adding = child_total.clone();
    let state = scrollable_state(move |delta| {
        adding.set(adding.get() + delta);
        delta / 2.0
    });
    let cancelled = Rc::new(Cell::new(0));
    let connection: Rc<dyn NestedScrollConnection> = Rc::new(CancellingFling(cancelled.clone()));
    let mut ui = window(move || {
        let state = state.clone();
        Box!(modifier = Modifier.nested_scroll(connection.clone()), {
            Box!(modifier = sideways(&state));
        });
    });
    let slop = ViewConfiguration::default().touch_slop;
    let (left, right) = (Offset::new(1.0, 150.0), Offset::new(299.0, 150.0));
    ui.on_root().perform_touch_input(|touch| {
        touch.down(left);
        touch.move_by(Offset::new(100.0, 0.0));
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(child_total.get(), 100.0 - slop);
    child_total.set(0.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(left, right, 2000.0, None);
    });
    ui.wait_for_idle();
    // Its first delta reached the child, whose rest cancelled it: there's no second.
    assert_eq!(cancelled.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_multiDirectionsShouldPropagateOrthogonalAxisToNextParentWithSameDirection
#[test]
fn what_is_left_goes_past_a_scrollable_across_to_the_next_one_along() {
    let (outer, outer_total) = consuming(1.0);
    let middle_total = Rc::new(Cell::new(0.0f32));
    let adding = middle_total.clone();
    let middle = scrollable_state(move |delta| {
        adding.set(adding.get() + delta);
        delta / 2.0
    });
    let inner_total = Rc::new(Cell::new(0.0f32));
    let adding = inner_total.clone();
    let inner = scrollable_state(move |delta| {
        adding.set(adding.get() + delta);
        delta / 2.0
    });
    let mut ui = window(move || {
        let (middle, inner) = (middle.clone(), inner.clone());
        Box!(modifier = sideways(&outer), {
            let inner = inner.clone();
            Box!(
                modifier =
                    Modifier.size(300.dp()).scrollable(middle.clone(), Orientation::Vertical),
                {
                    Box!(modifier = sideways(&inner));
                }
            );
        });
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 0.0));
        touch.up();
    });
    ui.set_auto_advance(false);
    assert!(inner_total.get() > 0.0);
    assert_eq!(middle_total.get(), 0.0);
    assert_eq!(outer_total.get(), inner_total.get() / 2.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, nestedScrollable_noFlingContinuationInCrossAxis_shouldAllowClicksOnCrossAxis_scrollable
#[test]
fn a_fling_of_a_row_lets_the_column_around_it_click() {
    let clicked = Rc::new(Cell::new(0));
    let counted = clicked.clone();
    let mut ui = window(move || {
        let counted = counted.clone();
        LazyColumn!(move |scope: LazyListScope| {
            let counted = counted.clone();
            scope.item(move || {
                let counted = counted.clone();
                Box!(
                    modifier =
                        Modifier.size(20.dp()).clickable(move || counted.set(counted.get() + 1))
                );
            });
            scope.item(|| {
                LazyRow!(|scope: LazyListScope| {
                    scope.items(100, |_| Box!(modifier = Modifier.size(20.dp())));
                });
            });
        });
    });
    ui.set_auto_advance(false);
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(200.0, 30.0), Offset::new(20.0, 30.0), 100);
    });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.on_root().perform_touch_input(|touch| touch.click_at(Offset::new(10.0, 10.0)));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(clicked.get(), 1);
}

/// Adds up what it's told.
#[derive(Default)]
struct Tally {
    pre_available: Cell<f32>,
    post_consumed: Cell<f32>,
    post_available: Cell<f32>,
}

impl NestedScrollConnection for Tally {
    fn on_pre_scroll(&self, available: Offset, _: NestedScrollSource) -> Offset {
        self.pre_available.set(self.pre_available.get() + available.x);
        Offset::ZERO
    }

    fn on_post_scroll(&self, consumed: Offset, available: Offset, _: NestedScrollSource) -> Offset {
        self.post_consumed.set(self.post_consumed.get() + consumed.x);
        self.post_available.set(self.post_available.get() + available.x);
        Offset::ZERO
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, dispatchScroll_shouldReturnConsumedDeltaInNestedScrollChain
#[test]
fn a_connection_around_two_is_told_what_both_consumed() {
    let (outer, outer_total) = consuming(1.0);
    let (inner, inner_total) = consuming(0.5);
    let tally = Rc::new(Tally::default());
    let connection: Rc<dyn NestedScrollConnection> = tally.clone();
    let mut ui = window(move || {
        let (outer, inner) = (outer.clone(), inner.clone());
        Box!(modifier = Modifier.nested_scroll(connection.clone()), {
            let inner = inner.clone();
            Box!(modifier = sideways(&outer), {
                Box!(modifier = sideways(&inner));
            });
        });
    });
    let slop = ViewConfiguration::default().touch_slop;
    let delta = 200.0;
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(50.0, 150.0));
        touch.move_by(Offset::new(delta, 0.0));
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
    assert!(inner_total.get() > 0.0);
    assert!(outer_total.get() > 0.0);
    assert_eq!(tally.post_available.get(), 0.0);
    assert_eq!(tally.post_consumed.get().abs(), delta - slop);
    assert_eq!(tally.pre_available.get().abs(), delta - slop);
    assert_eq!(inner_total.get().abs() + outer_total.get().abs() + slop, delta);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_interactionSource_resetWhenDisposed
#[test]
fn a_drag_cancels_on_the_source_when_disposed() {
    let source = MutableInteractionSource::new();
    let recorded = record(source.as_source());
    let (state, _) = consuming(1.0);
    let shown = mutable_state_of(true);
    let read = shown.handle();
    let given = source.clone();
    let mut ui = window(move || {
        if read.get() {
            Box!(
                modifier = Modifier.size(300.dp()).scrollable_with(
                    state.clone(),
                    Orientation::Horizontal,
                    true,
                    false,
                    None,
                    Some(given.clone())
                )
            );
        }
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(50.0, 150.0));
        touch.move_by(Offset::new(100.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Start"]);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Start", "DragCancel"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, onDensityChange_shouldUpdateFlingBehavior
#[test]
fn a_new_density_changes_the_default_fling() {
    let fling_of = |density: f32| {
        let (state, total) = consuming(1.0);
        let mut ui = compose_test_with(
            IntSize::new(600, 600),
            compose_ui::Density::new(density),
            Box::new(compose_ui::platform::FixedTextMeasurer::new()),
            move || {
                Box!(
                    modifier =
                        Modifier.size(100.dp()).scrollable(state.clone(), Orientation::Vertical)
                );
            },
        );
        ui.wait_for_idle();
        ui.on_root().perform_touch_input(|touch| {
            touch.swipe(Offset::new(50.0, 90.0), Offset::new(50.0, 10.0), 100);
        });
        ui.wait_for_idle();
        total.get()
    };
    assert_ne!(fling_of(1.0), fling_of(2.0));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: a scrollable is a DragGestureNode, its slop crossed at a gesture's angle
#[test]
fn a_gesture_mostly_vertical_goes_past_a_horizontal_scrollable_to_a_vertical_one() {
    let (outer, outer_total) = consuming(1.0);
    let (inner, inner_total) = consuming(1.0);
    let mut ui = window(move || {
        let inner = inner.clone();
        Box!(
            modifier = Modifier.size(300.dp()).scrollable(outer.clone(), Orientation::Vertical),
            {
                Box!(modifier = sideways(&inner));
            }
        );
    });
    let slop = ViewConfiguration::default().touch_slop;
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        // Past the slop on both axes, at about 80 degrees.
        touch.move_by(Offset::new(slop, slop * 5.0));
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(inner_total.get(), 0.0);
    assert_eq!(outer_total.get(), slop * 4.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: only a gesture waiting around goes first
#[test]
fn past_a_disabled_scrollable_around_a_gesture_at_an_angle_stays_inside() {
    let (outer, outer_total) = consuming(1.0);
    let (inner, inner_total) = consuming(1.0);
    let dragged = inner.clone();
    let mut ui = window(move || {
        let inner = inner.clone();
        Box!(
            modifier = Modifier.size(300.dp()).scrollable_with(
                outer.clone(),
                Orientation::Vertical,
                false,
                false,
                None,
                None
            ),
            {
                Box!(modifier = sideways(&inner));
            }
        );
    });
    let slop = ViewConfiguration::default().touch_slop;
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(slop, slop * 5.0));
    });
    ui.wait_for_idle();
    assert!(dragged.is_scroll_in_progress(), "the inner took it, at exactly its slop");
    assert_eq!(outer_total.get(), 0.0);
    assert_eq!(inner_total.get(), 0.0);
}

fn wheel_at(ui: &mut ComposeTestRule, by: Offset) {
    ui.host().send_pointer(compose_ui::PointerEvent::scroll(CENTER, by).with_precise_wheel(true));
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_verticalScroll_2d_mouseWheel, scrollable_horizontalScroll_2d_mouseWheel, scrollableVertical_diagonalScroll_2d_mouseWheel, scrollableHorizontal_diagonalScroll_2d_mouseWheel
#[test]
fn a_wheel_on_both_axes_scrolls_only_if_mostly_along() {
    let cases = [
        (Orientation::Vertical, Offset::new(0.0, -100.0), true),
        (Orientation::Vertical, Offset::new(-100.0, 0.0), false),
        (Orientation::Vertical, Offset::new(50.0, 100.0), true),
        (Orientation::Vertical, Offset::new(-50.0, -100.0), true),
        // Exactly 45 degrees is vertical.
        (Orientation::Vertical, Offset::new(100.0, 100.0), true),
        (Orientation::Vertical, Offset::new(-100.0, -50.0), false),
        (Orientation::Horizontal, Offset::new(-100.0, 0.0), true),
        (Orientation::Horizontal, Offset::new(0.0, -100.0), false),
        (Orientation::Horizontal, Offset::new(100.0, 50.0), true),
        (Orientation::Horizontal, Offset::new(100.0, 100.0), false),
    ];
    for (orientation, by, moves) in cases {
        let (state, total) = consuming(1.0);
        let mut ui = window(move || {
            Box!(modifier = Modifier.size(300.dp()).scrollable(state.clone(), orientation));
        });
        wheel_at(&mut ui, by);
        assert_eq!(total.get() != 0.0, moves, "{orientation:?} by {by:?}");
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_horizontalScroll_mouseWheel_badMotionEvent, scrollable_verticalScroll_mouseWheel_badMotionEvent
#[test]
fn a_wheel_turning_by_nan_scrolls_nothing() {
    for (orientation, by) in [
        (Orientation::Horizontal, Offset::new(f32::NAN, 0.0)),
        (Orientation::Vertical, Offset::new(0.0, f32::NAN)),
    ] {
        let (state, total) = consuming(1.0);
        let mut ui = window(move || {
            Box!(modifier = Modifier.size(300.dp()).scrollable(state.clone(), orientation));
        });
        wheel_at(&mut ui, by);
        assert_eq!(total.get(), 0.0);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_snappingScrolling
#[test]
fn an_animated_scroll_lands_where_asked_each_way() {
    let (state, total) = consuming(1.0);
    let given = state.clone();
    let mut ui = window(move || {
        Box!(modifier = Modifier.size(300.dp()).scrollable(given.clone(), Orientation::Vertical));
    });
    let animating = state.clone();
    crate::scroll_support::run(&mut ui, async move {
        let _ = animating.animate_scroll_by(1000.0, compose_animation::spring()).await;
    });
    assert!((total.get() - 1000.0).abs() < 0.001, "{}", total.get());
    let animating = state.clone();
    crate::scroll_support::run(&mut ui, async move {
        let _ = animating.animate_scroll_by(-200.0, compose_animation::spring()).await;
    });
    assert!((total.get() - 800.0).abs() < 0.001, "{}", total.get());
}

/// Notes the velocity a fling starts with, before anyone takes part of it.
struct PreFlingVelocity(Rc<Cell<f32>>);

impl NestedScrollConnection for PreFlingVelocity {
    fn on_pre_fling(&self, available: Offset) -> LocalBoxFuture<'_, Offset> {
        Box::pin(async move {
            self.0.set(available.y);
            Offset::ZERO
        })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, verticalScrollable_assertVelocityCalculationIsSimilarInsideOutsideVelocityTracker
#[test]
fn a_flings_velocity_is_what_a_tracker_outside_finds() {
    let velocity = Rc::new(Cell::new(0.0f32));
    let tracker = Rc::new(std::cell::RefCell::new(compose_ui::VelocityTracker::new()));
    let (noted, connection): (_, Rc<dyn NestedScrollConnection>) =
        (tracker.clone(), Rc::new(PreFlingVelocity(velocity.clone())));
    let (state, _) = consuming(0.0);
    let mut ui = window(move || {
        let noted = noted.clone();
        Box!(
            modifier = Modifier
                .size(300.dp())
                .pointer_input((), move |scope| {
                    let noted = noted.clone();
                    async move {
                        let mut at = Offset::ZERO;
                        loop {
                            let change = scope
                                .await_pointer_event(compose_ui::PointerEventPass::Initial)
                                .await;
                            let down = change.kind == compose_ui::PointerEventType::Press;
                            let up = change.kind == compose_ui::PointerEventType::Release;
                            if down {
                                at = change.position;
                            } else {
                                at = at + change.position_change();
                            }
                            noted.borrow_mut().add_pointer_input_change(
                                change.uptime_millis as i64,
                                at,
                                down,
                                up,
                            );
                        }
                    }
                })
                .nested_scroll(connection.clone())
                .scrollable(state.clone(), Orientation::Vertical)
        );
    });
    for (from, to) in [(280.0, 20.0), (20.0, 280.0)] {
        tracker.borrow_mut().reset_tracking();
        ui.on_root().perform_touch_input(|touch| {
            touch.swipe(Offset::new(150.0, from), Offset::new(150.0, to), 200);
        });
        ui.wait_for_idle();
        let outside = tracker.borrow().calculate_velocity().y;
        assert!(outside != 0.0);
        assert!((velocity.get().abs() - outside.abs()).abs() < 1.0, "{} {outside}", velocity.get());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_verticalScroll_keyboardPageUpAndDown, scrollable_horizontalScroll_keyboardPageUpAndDown
#[test]
fn page_keys_scroll_by_the_viewport_each_way() {
    for (orientation, page) in [(Orientation::Vertical, 300.0), (Orientation::Horizontal, 200.0)] {
        let (state, total) = consuming(1.0);
        let mut ui = window(move || {
            Box!(
                modifier =
                    Modifier.size((200.dp(), 300.dp())).scrollable(state.clone(), orientation),
                {
                    Box!(modifier = Modifier.size(50.dp()).test_tag("focus").focusable());
                }
            );
        });
        ui.on_node_with_tag("focus").request_focus();
        ui.wait_for_idle();
        ui.press_key(compose_ui::Key::PageDown);
        ui.wait_for_idle();
        assert_eq!(total.get(), -page, "{orientation:?}");
        ui.press_key(compose_ui::Key::PageUp);
        ui.wait_for_idle();
        assert_eq!(total.get(), 0.0, "{orientation:?}");
    }
}

// Compose: the same on what scrolls its content: Page Down goes down
#[test]
fn page_down_scrolls_a_list_and_a_column_forward() {
    let list = LazyListState::new(0, 0);
    let handle = *list;
    let mut ui = window(move || {
        LazyColumn!(modifier = Modifier.size(300.dp()), state = handle, |scope: LazyListScope| {
            scope.item(|| {
                Box!(modifier = Modifier.size(100.dp()).test_tag("focus").focusable());
            });
            scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
        });
    });
    ui.on_node_with_tag("focus").request_focus();
    ui.wait_for_idle();
    ui.press_key(compose_ui::Key::PageDown);
    ui.wait_for_idle();
    assert_eq!(handle.first_visible_item_index(), 3);

    let scroll = ScrollState::new(0);
    let given = *scroll;
    let mut ui = window(move || {
        Column!(modifier = Modifier.size(300.dp()).vertical_scroll(given), {
            Box!(modifier = Modifier.size(100.dp()).test_tag("focus").focusable());
            Box!(modifier = Modifier.size(1000.dp()));
        });
    });
    ui.on_node_with_tag("focus").request_focus();
    ui.wait_for_idle();
    ui.press_key(compose_ui::Key::PageDown);
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 300);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt: onKeyEvent ignores Ctrl and a key's release
#[test]
fn only_a_page_key_alone_going_down_scrolls() {
    let (state, total) = consuming(1.0);
    let mut ui = window(move || {
        Box!(
            modifier = Modifier.size(300.dp()).scrollable(state.clone(), Orientation::Vertical),
            {
                Box!(modifier = Modifier.size(50.dp()).test_tag("focus").focusable());
            }
        );
    });
    ui.on_node_with_tag("focus").request_focus();
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent::up(compose_ui::Key::PageDown));
    ui.wait_for_idle();
    assert_eq!(total.get(), 0.0);
    let mut with_ctrl = compose_ui::KeyEvent::down(compose_ui::Key::PageDown);
    with_ctrl.modifiers.ctrl = true;
    ui.host().send_key(with_ctrl);
    ui.wait_for_idle();
    assert_eq!(total.get(), 0.0);
}
