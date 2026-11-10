//! Compose's `MouseEventTest` (ui, device tests): the `Enter` and `Exit` a mouse's node hears
//! around presses, touches and layouts moving under a resting cursor, and
//! `ClickNotPlacedChildTest`: a child measured but not placed takes no click.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, IntSize, Layout, Offset, PointerEventPass, PointerEventType, UnitExt, measure_policy,
};

const TAG: &str = "Tagged Layout";

type Events = Rc<RefCell<Vec<PointerEventType>>>;

/// A 50 dp box with `alignment` in the window, recording the kinds of the events it hears, as
/// the Kotlin tests' content (and `createRelayoutComposition`).
fn recording_box(alignment: impl Fn() -> Alignment + 'static) -> (ComposeTestRule, Events) {
    let events = Events::default();
    let seen = events.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let seen = seen.clone();
        let alignment = alignment();
        Box!(modifier = Modifier.fill_max_size(), move |scope: BoxScope| {
            let seen = seen.clone();
            Box!(
                modifier = Modifier
                    .align(scope, alignment)
                    .size(50.dp())
                    .test_tag(TAG)
                    .pointer_input((), move |pointer| {
                        let seen = seen.clone();
                        async move {
                            loop {
                                let event =
                                    pointer.await_pointer_event(PointerEventPass::Main).await;
                                seen.borrow_mut().push(event.kind);
                            }
                        }
                    })
            );
        });
    });
    ui.wait_for_idle();
    (ui, events)
}

fn centered() -> (ComposeTestRule, Events) {
    recording_box(|| Alignment::CENTER)
}

fn taken(events: &Events) -> Vec<PointerEventType> {
    std::mem::take(&mut *events.borrow_mut())
}

use PointerEventType::{Enter, Exit, Press, Release};

/// A mouse's first event a press: its node hears `Enter` before it.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MouseEventTest.kt, absentEnterSent
#[test]
fn absent_enter_sent() {
    let (mut ui, events) = centered();
    ui.on_node_with_tag(TAG).perform_mouse_input(|mouse| {
        mouse.move_to(Offset::ZERO);
        mouse.press(MouseButton::Primary);
    });
    assert_eq!(taken(&events), [Enter, Press]);
}

/// A touch after a mouse hovered: the mouse's node hears `Exit` before the touch's press.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MouseEventTest.kt, absentExitSent
#[test]
fn absent_exit_sent() {
    let (mut ui, events) = centered();
    ui.on_node_with_tag(TAG)
        .perform_mouse_input(|mouse| mouse.move_to(Offset::ZERO))
        .perform_touch_input(|touch| {
            touch.down(Offset::ZERO);
            touch.up();
        });
    assert_eq!(taken(&events), [Enter, Exit, Press, Release]);
}

/// Pressed, moved out, released: the node hears the exit and the release, and nothing of the
/// next press and release outside it. Compose's `HitPathTracker` makes the move out an `Exit`
/// for the node it left (each node of a hit path has its own `isIn`); compose-rs delivers the
/// move along the captured path and the `Exit` at the release: `[Enter, Press, Move, Exit,
/// Release]`.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MouseEventTest.kt, pressMoveRelease
#[test]
fn press_move_release() {
    let (mut ui, events) = centered();
    ui.on_node_with_tag(TAG).perform_mouse_input(|mouse| {
        mouse.move_to(Offset::ZERO);
        mouse.press(MouseButton::Primary);
        mouse.move_to(Offset::new(-10.0, -10.0));
        mouse.release(MouseButton::Primary);
        mouse.press(MouseButton::Primary);
        mouse.release(MouseButton::Primary);
    });
    assert_eq!(taken(&events), [Enter, Press, Exit, Release]);
}

/// The node moves away from under a resting cursor: it hears `Exit`.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MouseEventTest.kt, relayoutUnderCursor
#[test]
fn relayout_under_cursor() {
    let alignment = mutable_state_of(Alignment::BOTTOM_CENTER);
    let handle = alignment.handle();
    let (mut ui, events) = recording_box(move || handle.get());
    ui.on_node_with_tag(TAG).perform_mouse_input(|mouse| mouse.move_to(Offset::ZERO));
    assert_eq!(taken(&events), [Enter]);
    alignment.set(Alignment::TOP_CENTER);
    ui.wait_for_idle();
    assert_eq!(taken(&events), [Exit]);
}

/// After a touch's up, a layout moving the node sends it nothing.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MouseEventTest.kt, noSimulatedEventAfterUp
#[test]
fn no_simulated_event_after_up() {
    let alignment = mutable_state_of(Alignment::BOTTOM_CENTER);
    let handle = alignment.handle();
    let (mut ui, events) = recording_box(move || handle.get());
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.down(Offset::ZERO);
        touch.up();
    });
    assert_eq!(taken(&events).len(), 2);
    alignment.set(Alignment::TOP_CENTER);
    ui.wait_for_idle();
    assert_eq!(taken(&events), []);
}

/// After the mouse left the window, a layout moving the node sends it nothing.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/MouseEventTest.kt, noSimulatedEventAfterHoverExit
#[test]
fn no_simulated_event_after_hover_exit() {
    let alignment = mutable_state_of(Alignment::BOTTOM_CENTER);
    let handle = alignment.handle();
    let (mut ui, events) = recording_box(move || handle.get());
    ui.on_node_with_tag(TAG).perform_mouse_input(|mouse| {
        mouse.move_to(Offset::ZERO);
        let at = mouse.current_position();
        mouse.exit(at);
    });
    ui.wait_for_idle();
    assert_eq!(taken(&events), [Enter, Exit]);
    alignment.set(Alignment::TOP_CENTER);
    ui.wait_for_idle();
    assert_eq!(taken(&events), []);
}

/// Two full-size clickable children, the second measured but not placed: a click reaches the
/// first only.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/ClickNotPlacedChildTest.kt, childIsDisplayedWhenItWasNotPlacedOriginallyButPlacedLater
#[test]
fn child_is_displayed_when_it_was_not_placed_originally_but_placed_later() {
    let first_clicks = Rc::new(Cell::new(0));
    let second_clicks = Rc::new(Cell::new(0));
    let (first, second) = (first_clicks.clone(), second_clicks.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (first, second) = (first.clone(), second.clone());
        Layout!(
            modifier = Modifier.required_size(100.dp()).test_tag("parent"),
            measure_policy = measure_policy(|scope, measurables, constraints| {
                let first = scope.measure(measurables[0], constraints);
                scope.measure(measurables[1], constraints);
                scope.place(&first, 0, 0);
                scope.layout(first.width, first.height)
            }),
            {
                let (first, second) = (first.clone(), second.clone());
                Box!(
                    modifier =
                        Modifier.fill_max_size().clickable(move || first.set(first.get() + 1))
                );
                Box!(
                    modifier =
                        Modifier.fill_max_size().clickable(move || second.set(second.get() + 1))
                );
            }
        );
    });
    ui.on_node_with_tag("parent").perform_click();
    ui.wait_for_idle();
    assert_eq!(second_clicks.get(), 0);
    assert_eq!(first_clicks.get(), 1);
}
