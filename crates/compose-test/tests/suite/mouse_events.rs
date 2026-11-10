//! Compose's `inputdispatcher/MouseEventsTest`, through `MouseInjectionScope`: Compose's tests
//! read the `MotionEvent`s its internal `InputDispatcher` makes; these read what a node filling
//! the root hears of them (`dispatcher.rs`), in compose-rs's model of a node's events (D1657,
//! D1658): Android's hover enter and move into the window are the node's `Enter`; its hover exit,
//! down and button press its `Press`; a last button's release, up, hover enter and move its
//! `Release` and a hover `Move` (as `input.rs`'s `a_release_inside_the_window_hovers_again`);
//! a press with no hover first comes with an `Enter` (as Compose's `mouse/CancelTest` hears
//! it); a button pressed or released with another held is a `Press` or `Release` of its own,
//! without Android's move first.

use compose_test::*;
use compose_ui::{Offset, PointerButtons, PointerEventType};

use super::dispatcher::*;
use PointerEventType::{Enter, Exit, Move, Press, Release, Scroll};

const T: u64 = 16;

const POSITION_1: Offset = Offset::new(1.0, 1.0);
const POSITION_2: Offset = Offset::new(2.0, 2.0);
const POSITION_3: Offset = Offset::new(3.0, 3.0);
const POSITION_4: Offset = Offset::new(4.0, 4.0);
const POSITION_MIN_1: Offset = Offset::new(-1.0, -1.0);
const POSITION_MIN_2: Offset = Offset::new(-2.0, -2.0);

const NONE: PointerButtons = PointerButtons::NONE;
const PRIMARY: PointerButtons = PointerButtons::PRIMARY;
const SECONDARY: PointerButtons = PointerButtons::SECONDARY;
const TERTIARY: PointerButtons = PointerButtons::TERTIARY;

fn both(a: PointerButtons, b: PointerButtons) -> PointerButtons {
    PointerButtons(a.0 | b.0)
}

/// Runs `block` in one mouse input on the root, returning what the root heard.
fn mice(block: impl FnOnce(&mut MouseInjectionScope<'_>)) -> Vec<Heard> {
    let (mut ui, log) = recording_root();
    ui.on_node_with_tag(ROOT).perform_mouse_input(block);
    assert_has_valid_event_times(&log);
    heard(&log)
}

/// Moves the cursor to `position` now (as `enqueueMouseMove`) and checks it's there.
#[track_caller]
fn move_and_check(scope: &mut MouseInjectionScope<'_>, position: Offset) {
    scope.move_to_after(position, 0);
    verify_mouse_position(scope, position);
}

#[track_caller]
fn verify_mouse_position(scope: &MouseInjectionScope<'_>, expected: Offset) {
    assert_eq!(scope.current_position(), expected, "currentMousePosition");
}

/// The kind and time of what was heard, for the exits (whose position is the tree's).
fn kind_and_time(heard: &Heard) -> (Option<PointerEventType>, u64) {
    (heard.kind(), heard.millis())
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, oneButton_primary
#[test]
fn one_button_primary() {
    one_button(MouseButton::Primary, PRIMARY);
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, oneButton_secondary
#[test]
fn one_button_secondary() {
    one_button(MouseButton::Secondary, SECONDARY);
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, oneButton_tertiary
#[test]
fn one_button_tertiary() {
    one_button(MouseButton::Tertiary, TERTIARY);
}

/// Move, press, move, release, move.
fn one_button(button: MouseButton, buttons: PointerButtons) {
    let heard = mice(|scope| {
        verify_mouse_position(scope, Offset::ZERO);
        move_and_check(scope, POSITION_1);
        scope.advance_event_time(T);
        scope.press(button);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_2);
        scope.advance_event_time(T);
        scope.release(button);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_3);
    });
    assert_eq!(
        heard,
        [
            mouse(Enter, 0, POSITION_1, NONE, false),
            mouse(Press, T, POSITION_1, buttons, true),
            mouse(Move, 2 * T, POSITION_2, buttons, true),
            mouse(Release, 3 * T, POSITION_2, NONE, false),
            mouse(Move, 3 * T, POSITION_2, NONE, false),
            mouse(Move, 4 * T, POSITION_3, NONE, false),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, oneButton_cancel
#[test]
fn one_button_cancel() {
    let heard = mice(|scope| {
        scope.press(MouseButton::Primary);
        scope.cancel();
    });
    assert_eq!(
        heard,
        [
            mouse(Enter, 0, Offset::ZERO, PRIMARY, false),
            mouse(Press, 0, Offset::ZERO, PRIMARY, true),
            Heard::Cancel { millis: T },
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, hoverOutOfRootBounds
/// Out of the window the cursor exits, and its moves there are suppressed; back in, it enters.
#[test]
fn hover_out_of_root_bounds() {
    let heard = mice(|scope| {
        verify_mouse_position(scope, Offset::ZERO);
        move_and_check(scope, POSITION_1);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_MIN_1);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_MIN_2);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_2);
    });
    assert_eq!(heard.len(), 3, "{heard:?}");
    assert_eq!(heard[0], mouse(Enter, 0, POSITION_1, NONE, false));
    assert_eq!(kind_and_time(&heard[1]), (Some(Exit), T));
    assert_eq!(heard[2], mouse(Enter, 3 * T, POSITION_2, NONE, false));
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, moveOutOfRootBounds
/// Pressed, the cursor out of the window exits the root it left as a pressed `Exit`, still
/// moves what it pressed, and the buttons pressed and released out there are heard (Compose's
/// turns them into moves), the last release an up.
#[test]
fn move_out_of_root_bounds() {
    let heard = mice(|scope| {
        verify_mouse_position(scope, Offset::ZERO);
        scope.press(MouseButton::Primary);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_MIN_1);
        scope.advance_event_time(T);
        scope.press(MouseButton::Secondary);
        scope.advance_event_time(T);
        scope.release(MouseButton::Secondary);
        scope.advance_event_time(T);
        scope.release(MouseButton::Primary);
    });
    assert_eq!(
        heard,
        [
            mouse(Enter, 0, Offset::ZERO, PRIMARY, false),
            mouse(Press, 0, Offset::ZERO, PRIMARY, true),
            mouse(Exit, T, POSITION_MIN_1, PRIMARY, true),
            mouse(Press, 2 * T, POSITION_MIN_1, both(PRIMARY, SECONDARY), true),
            mouse(Release, 3 * T, POSITION_MIN_1, PRIMARY, true),
            mouse(Release, 4 * T, POSITION_MIN_1, NONE, false),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, twoButtons
#[test]
fn two_buttons() {
    let primary_and_secondary = both(PRIMARY, SECONDARY);
    let heard = mice(|scope| {
        verify_mouse_position(scope, Offset::ZERO);
        scope.press(MouseButton::Primary);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_1);
        scope.advance_event_time(T);
        scope.press(MouseButton::Secondary);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_2);
        scope.advance_event_time(T);
        scope.release(MouseButton::Primary);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_3);
        scope.advance_event_time(T);
        scope.release(MouseButton::Secondary);
        scope.advance_event_time(T);
        move_and_check(scope, POSITION_4);
        scope.advance_event_time(T);
        scope.press(MouseButton::Tertiary);
    });
    assert_eq!(
        heard,
        [
            mouse(Enter, 0, Offset::ZERO, PRIMARY, false),
            mouse(Press, 0, Offset::ZERO, PRIMARY, true),
            mouse(Move, T, POSITION_1, PRIMARY, true),
            mouse(Press, 2 * T, POSITION_1, primary_and_secondary, true),
            mouse(Move, 3 * T, POSITION_2, primary_and_secondary, true),
            mouse(Release, 4 * T, POSITION_2, SECONDARY, true),
            mouse(Move, 5 * T, POSITION_3, SECONDARY, true),
            mouse(Release, 6 * T, POSITION_3, NONE, false),
            mouse(Move, 6 * T, POSITION_3, NONE, false),
            mouse(Move, 7 * T, POSITION_4, NONE, false),
            mouse(Press, 8 * T, POSITION_4, TERTIARY, true),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, manualEnterExit
#[test]
fn manual_enter_exit() {
    let heard = mice(|scope| {
        verify_mouse_position(scope, Offset::ZERO);
        scope.enter(POSITION_1);
        verify_mouse_position(scope, POSITION_1);
        scope.move_to(POSITION_2);
        verify_mouse_position(scope, POSITION_2);
        scope.exit(POSITION_3);
        verify_mouse_position(scope, POSITION_3);
    });
    assert_eq!(heard.len(), 3, "{heard:?}");
    assert_eq!(heard[0], mouse(Enter, 0, POSITION_1, NONE, false));
    assert_eq!(heard[1], mouse(Move, T, POSITION_2, NONE, false));
    assert_eq!(kind_and_time(&heard[2]), (Some(Exit), 2 * T));
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, scroll_vertically
/// compose-rs's wheel is forward positive in pixels, 64 a unit (Android's axis is inverted).
#[test]
fn scroll_vertically() {
    scroll_test(ScrollWheel::Vertical, |units| Offset::new(0.0, 64.0 * units));
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, scroll_horizontally
#[test]
fn scroll_horizontally() {
    scroll_test(ScrollWheel::Horizontal, |units| Offset::new(64.0 * units, 0.0));
}

/// Move, scroll by 1, press, scroll by 2, release, scroll by 3: each scroll after a move where
/// the cursor is.
fn scroll_test(wheel: ScrollWheel, scrolled: fn(f32) -> Offset) {
    let heard = mice(|scope| {
        verify_mouse_position(scope, Offset::ZERO);
        move_and_check(scope, POSITION_1);
        scope.advance_event_time(T);
        scope.scroll(1.0, wheel);
        scope.advance_event_time(T);
        scope.press(MouseButton::Primary);
        scope.advance_event_time(T);
        scope.scroll(2.0, wheel);
        scope.advance_event_time(T);
        scope.release(MouseButton::Primary);
        scope.advance_event_time(T);
        scope.scroll(3.0, wheel);
    });
    let p = POSITION_1;
    assert_eq!(
        heard,
        [
            mouse(Enter, 0, p, NONE, false),
            mouse(Move, T, p, NONE, false),
            mouse_scroll(Scroll, T, p, NONE, false, scrolled(1.0)),
            mouse(Press, 2 * T, p, PRIMARY, true),
            mouse(Move, 3 * T, p, PRIMARY, true),
            mouse_scroll(Scroll, 3 * T, p, PRIMARY, true, scrolled(2.0)),
            mouse(Release, 4 * T, p, NONE, false),
            mouse(Move, 4 * T, p, NONE, false),
            mouse(Move, 5 * T, p, NONE, false),
            mouse_scroll(Scroll, 5 * T, p, NONE, false, scrolled(3.0)),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, twoButtons_cancel
#[test]
fn two_buttons_cancel() {
    let heard = mice(|scope| {
        scope.press(MouseButton::Primary);
        scope.advance_event_time(T);
        scope.press(MouseButton::Secondary);
        scope.cancel();
    });
    assert_eq!(
        heard,
        [
            mouse(Enter, 0, Offset::ZERO, PRIMARY, false),
            mouse(Press, 0, Offset::ZERO, PRIMARY, true),
            mouse(Press, T, Offset::ZERO, both(PRIMARY, SECONDARY), true),
            Heard::Cancel { millis: 2 * T },
        ]
    );
}

// ---- The mouse interrupting a touch ------------------------------------------------------------

/// The root's events: a finger down at `POSITION_1`, then what `block` does with the mouse an
/// event period on.
fn touch_then_mouse(block: impl FnOnce(&mut MouseInjectionScope<'_>)) -> Vec<Heard> {
    let (mut ui, log) = recording_root();
    ui.on_node_with_tag(ROOT).perform_touch_input(|touch| touch.down_pointer(1, POSITION_1));
    ui.on_node_with_tag(ROOT).perform_mouse_input(|scope| {
        scope.advance_event_time(T);
        block(scope);
    });
    assert_has_valid_event_times(&log);
    heard(&log)
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMousePress_interruptsTouch
#[test]
fn enqueue_mouse_press_interrupts_touch() {
    let heard = touch_then_mouse(|scope| scope.press(MouseButton::Primary));
    assert_eq!(
        heard,
        [
            touch(Press, 0, &[(finger(1), POSITION_1, true)]),
            Heard::Cancel { millis: T },
            mouse(Enter, T, Offset::ZERO, PRIMARY, false),
            mouse(Press, T, Offset::ZERO, PRIMARY, true),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseMove_interruptsTouch
#[test]
fn enqueue_mouse_move_interrupts_touch() {
    let heard = touch_then_mouse(|scope| scope.move_to_after(POSITION_2, 0));
    assert_eq!(
        heard,
        [
            touch(Press, 0, &[(finger(1), POSITION_1, true)]),
            Heard::Cancel { millis: T },
            mouse(Enter, T, POSITION_2, NONE, false),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseScroll_interruptsTouch
#[test]
fn enqueue_mouse_scroll_interrupts_touch() {
    let heard = touch_then_mouse(|scope| scope.scroll(1.0, ScrollWheel::Vertical));
    assert_eq!(
        heard,
        [
            touch(Press, 0, &[(finger(1), POSITION_1, true)]),
            Heard::Cancel { millis: T },
            mouse(Enter, T, Offset::ZERO, NONE, false),
            mouse_scroll(Scroll, T, Offset::ZERO, NONE, false, Offset::new(0.0, 64.0)),
        ]
    );
}

// ---- Errors ------------------------------------------------------------------------------------

/// What `block` panics with, in a mouse input on a recording root.
fn mouse_error(block: impl FnOnce(&mut MouseInjectionScope<'_>)) -> String {
    let (mut ui, _) = recording_root();
    super::input::expect_error(|| {
        ui.on_node_with_tag(ROOT).perform_mouse_input(block);
    })
}

#[track_caller]
fn assert_says(message: &str, expected: &str) {
    assert!(message.contains(expected), "expected {expected:?}, got {message:?}");
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseDown_alreadyDown
#[test]
fn enqueue_mouse_down_already_down() {
    let message = mouse_error(|scope| {
        scope.press(MouseButton::Primary);
        scope.press(MouseButton::Primary);
    });
    assert_says(&message, "Cannot send mouse button down event, button 1 is already pressed");
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseDown_outOfBounds
#[test]
fn enqueue_mouse_down_out_of_bounds() {
    let message = mouse_error(|scope| {
        scope.update_pointer_to(POSITION_MIN_1);
        scope.press(MouseButton::Primary);
    });
    assert_says(
        &message,
        "Cannot start a mouse gesture outside the Compose root bounds, mouse position is",
    );
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseUp_withoutDown
#[test]
fn enqueue_mouse_up_without_down() {
    let message = mouse_error(|scope| scope.release(MouseButton::Primary));
    assert_says(&message, "Cannot send mouse button up event, button 1 is not pressed");
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseEnter_alreadyEntered
#[test]
fn enqueue_mouse_enter_already_entered() {
    let message = mouse_error(|scope| {
        scope.enter(POSITION_1);
        scope.enter(POSITION_1);
    });
    assert_says(&message, "Cannot send mouse hover enter event, mouse is already hovering");
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseEnter_buttonsDown
#[test]
fn enqueue_mouse_enter_buttons_down() {
    let message = mouse_error(|scope| {
        scope.press(MouseButton::Primary);
        scope.enter(POSITION_1);
    });
    assert_says(&message, "Cannot send mouse hover enter event, mouse buttons are down");
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseEnter_outOfBounds
#[test]
fn enqueue_mouse_enter_out_of_bounds() {
    let message = mouse_error(|scope| scope.enter(POSITION_MIN_1));
    assert_says(&message, "Cannot send mouse hover enter event, Offset { x: -1.0, y: -1.0 }");
    assert_says(&message, "is out of bounds");
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseExit_notEntered
#[test]
fn enqueue_mouse_exit_not_entered() {
    let message = mouse_error(|scope| scope.exit(POSITION_1));
    assert_says(&message, "Cannot send mouse hover exit event, mouse is not hovering");
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/MouseEventsTest.kt, enqueueMouseCancel_withoutDown
#[test]
fn enqueue_mouse_cancel_without_down() {
    let message = mouse_error(|scope| scope.cancel());
    assert_says(&message, "Cannot send mouse cancel event, no mouse buttons are pressed");
}
