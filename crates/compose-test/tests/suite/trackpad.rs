//! Trackpad injection, `perform_trackpad_input`, as Compose's `injectionscope/trackpad` tests
//! check it: a recorder of the events a box gets, each with its time since the block began.

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{
    EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, Offset, PointerButtons,
    PointerEventPass, PointerEventType, PointerEvents, PointerId, PointerInputModifierNode,
    PointerType, UnitExt, VelocityTracker,
};

use PointerEventType::{
    Enter, Exit, Move, PanEnd, PanMove, PanStart, Press, Release, ScaleChange, ScaleEnd, ScaleStart,
};

const T: u64 = 16;
const PRIMARY: PointerButtons = PointerButtons::PRIMARY;
const SECONDARY: PointerButtons = PointerButtons::SECONDARY;
const BOTH: PointerButtons =
    PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0);
const NONE: PointerButtons = PointerButtons::NONE;

/// One event as the box got it (the initial pass), as Compose's `DataPoint`.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Heard {
    millis: u64,
    kind: PointerEventType,
    pressed: bool,
    position: Offset,
    buttons: PointerButtons,
    pointer: PointerId,
    pointer_type: PointerType,
    pan_offset: Offset,
    scale_factor: f32,
}

impl Heard {
    /// What Compose's `verifyTrackpadEvent` checks.
    fn summary(&self) -> (u64, PointerEventType, bool, Offset, PointerButtons) {
        (self.millis, self.kind, self.pressed, self.position, self.buttons)
    }
}

type Events = Rc<RefCell<Vec<Heard>>>;

/// Records every event its node gets, whole, as Compose's `SinglePointerInputRecorder`.
struct RecorderElement(Events, Rc<std::cell::Cell<u64>>);

struct RecorderNode(Events, Rc<std::cell::Cell<u64>>);

impl ModifierNodeElement for RecorderElement {
    type Node = RecorderNode;
    fn create(&self) -> RecorderNode {
        RecorderNode(self.0.clone(), self.1.clone())
    }
    fn update(&self, _node: &mut RecorderNode) {}
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&other.0, &self.0)
    }
}

impl ModifierNode for RecorderNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for RecorderNode {
    fn takes_whole_events(&self) -> bool {
        true
    }

    fn on_pointer_events(
        &mut self,
        event: &PointerEvents,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Initial {
            return;
        }
        for change in &event.changes {
            self.0.borrow_mut().push(Heard {
                millis: change.uptime_millis - self.1.get(),
                kind: event.kind,
                pressed: change.pressed,
                position: change.position,
                buttons: event.buttons,
                pointer: change.id,
                pointer_type: change.pointer_type,
                pan_offset: change.pan_offset,
                scale_factor: change.scale_factor,
            });
        }
    }
}

/// A rule with a recording box of `size` dp at the top left of a 1000 × 1000 root, and Compose's
/// test view configuration of these tests (double taps 40 to 200 ms apart, a 300 ms long press).
fn recording(size: f32) -> (ComposeTestRule, Events, Rc<std::cell::Cell<u64>>) {
    let events: Events = Rc::default();
    let start = Rc::new(std::cell::Cell::new(0));
    let (recorder, since) = (events.clone(), start.clone());
    let mut host = compose_ui::platform::UiHost::headless(1000, 1000);
    let base = compose_ui::ViewConfiguration::at(compose_ui::Density::default());
    host.set_view_configuration(Some(compose_ui::ViewConfiguration {
        long_press_timeout_millis: 300,
        double_tap_min_time_millis: 40,
        double_tap_timeout_millis: 200,
        ..base
    }));
    let mut ui = ComposeTestRule::empty(host);
    ui.set_auto_advance(false);
    ui.set_content(move || {
        Box!(
            modifier = Modifier
                .size(size.dp())
                .then_custom(RecorderElement(recorder.clone(), since.clone()))
                .test_tag("box")
        );
    });
    (ui, events, start)
}

/// Runs `input` on a 100 dp recording box, as Compose's `runTrackpadInputInjectionTest`, and
/// returns what it heard, its times from the block's start.
fn run(input: impl FnOnce(&mut TrackpadInjectionScope<'_>)) -> Vec<Heard> {
    let (mut ui, events, start) = recording(100.0);
    start.set(ui.current_time_millis());
    ui.on_node_with_tag("box").perform_trackpad_input(input);
    let heard = events.borrow().clone();
    for event in &heard {
        assert_eq!(event.pointer_type, PointerType::Mouse, "{event:?}");
    }
    heard
}

fn summaries(heard: &[Heard]) -> Vec<(u64, PointerEventType, bool, Offset, PointerButtons)> {
    heard.iter().map(Heard::summary).collect()
}

#[track_caller]
fn expect_error(action: impl FnOnce()) -> String {
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
    let panic = failed.expect_err("an error");
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

const IN: Offset = Offset::new(1.0, 1.0);
const MOVE1: Offset = Offset::new(2.0, 2.0);
const MOVE2: Offset = Offset::new(3.0, 3.0);
const OUT: Offset = Offset::new(101.0, 101.0);
const CENTER: Offset = Offset::new(50.0, 50.0);

// ---- Cancel ------------------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/CancelTest.kt, cancel
/// A cancel ends the press without an event the box hears (it comes over the box with the
/// press, where Compose's hears the press alone).
#[test]
fn cancel() {
    let heard = run(|trackpad| {
        trackpad.press(TrackpadButton::Primary);
        trackpad.cancel();
    });
    assert_eq!(
        summaries(&heard),
        [(0, Enter, false, Offset::ZERO, PRIMARY), (0, Press, true, Offset::ZERO, PRIMARY)]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/CancelTest.kt, cancel_withoutPress
#[test]
fn cancel_without_press() {
    let message = expect_error(|| {
        run(|trackpad| trackpad.cancel());
    });
    assert!(
        message.contains("Cannot send trackpad cancel event, no trackpad buttons are pressed"),
        "{message}"
    );
}

// ---- Move --------------------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, moveTo
/// The move that brings the cursor in is the box's `Enter`; out of it, its `Exit`.
#[test]
fn move_to() {
    let heard = run(|trackpad| {
        for (position, delay) in [(IN, 0), (MOVE1, T), (MOVE2, 2 * T), (OUT, T), (IN, T)] {
            trackpad.move_to_after(position, delay);
            assert_eq!(trackpad.current_position(), position);
        }
    });
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, IN, NONE),
            (T, Move, false, MOVE1, NONE),
            (3 * T, Move, false, MOVE2, NONE),
            (4 * T, Exit, false, OUT, NONE),
            (5 * T, Enter, false, IN, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, moveBy
#[test]
fn move_by() {
    let heard = run(|trackpad| {
        let mut at = Offset::ZERO;
        for (position, delay) in [(IN, 0), (MOVE1, T), (MOVE2, 2 * T), (OUT, T), (IN, T)] {
            trackpad.move_by_after(position - at, delay);
            assert_eq!(trackpad.current_position(), position);
            at = position;
        }
    });
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, IN, NONE),
            (T, Move, false, MOVE1, NONE),
            (3 * T, Move, false, MOVE2, NONE),
            (4 * T, Exit, false, OUT, NONE),
            (5 * T, Enter, false, IN, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, updatePointerTo
/// Moving the cursor without events sends nothing: the press is where it ended. The cursor
/// wasn't over the box, so it comes over it with the press, its `Enter` telling the press's
/// buttons (Compose's box hears the press alone, a difference its test notes from the mouse's).
#[test]
fn update_pointer_to() {
    let heard = run(|trackpad| {
        for position in [IN, MOVE1, MOVE2] {
            trackpad.update_pointer_to(position);
            assert_eq!(trackpad.current_position(), position);
        }
        trackpad.press(TrackpadButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [(0, Enter, false, MOVE2, PRIMARY), (0, Press, true, MOVE2, PRIMARY)]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, updatePointerBy
#[test]
fn update_pointer_by() {
    let heard = run(|trackpad| {
        let mut at = Offset::ZERO;
        for position in [IN, MOVE1, MOVE2] {
            trackpad.update_pointer_by(position - at);
            assert_eq!(trackpad.current_position(), position);
            at = position;
        }
        trackpad.press(TrackpadButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [(0, Enter, false, MOVE2, PRIMARY), (0, Press, true, MOVE2, PRIMARY)]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, enter_exit
#[test]
fn enter_exit() {
    let heard = run(|trackpad| {
        trackpad.enter(IN);
        trackpad.move_to(MOVE1);
        trackpad.exit(OUT);
    });
    let summaries = summaries(&heard);
    assert_eq!(summaries[..2], [(T, Enter, false, IN, NONE), (2 * T, Move, false, MOVE1, NONE)]);
    // The window's exit: where the cursor leaves isn't a move the box hears.
    assert_eq!((summaries[2].0, summaries[2].1, summaries.len()), (3 * T, Exit, 3));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, enter_alreadyEntered
#[test]
fn enter_already_entered() {
    let heard = run(|trackpad| {
        trackpad.enter(IN);
        let message = expect_error(|| trackpad.enter(MOVE1));
        assert!(
            message
                .contains("Cannot send trackpad hover enter event, trackpad is already hovering"),
            "{message}"
        );
    });
    assert_eq!(summaries(&heard), [(T, Enter, false, IN, NONE)]);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, exit_notEntered
#[test]
fn exit_not_entered() {
    let heard = run(|trackpad| {
        let message = expect_error(|| trackpad.exit(OUT));
        assert!(
            message.contains("Cannot send trackpad hover exit event, trackpad is not hovering"),
            "{message}"
        );
    });
    assert!(heard.is_empty(), "{heard:?}");
}

const STEP: Offset = Offset::new(5.0, 5.0);

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, animatePointerTo
#[test]
fn animate_pointer_to() {
    let heard = run(|trackpad| trackpad.animate_move_to(STEP * 4.0, 4 * T));
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, STEP, NONE),
            (2 * T, Move, false, STEP * 2.0, NONE),
            (3 * T, Move, false, STEP * 3.0, NONE),
            (4 * T, Move, false, STEP * 4.0, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, animatePointerBy
#[test]
fn animate_pointer_by() {
    let start = Offset::new(1.0, 1.0);
    let heard = run(|trackpad| {
        trackpad.move_to(start);
        trackpad.animate_move_by(STEP * 4.0, 4 * T);
    });
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, start, NONE),
            (2 * T, Move, false, start + STEP, NONE),
            (3 * T, Move, false, start + STEP * 2.0, NONE),
            (4 * T, Move, false, start + STEP * 3.0, NONE),
            (5 * T, Move, false, start + STEP * 4.0, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, animateAlong_fromCurrentPosition
/// A curve starting where the cursor is sends no first move.
#[test]
fn animate_along_from_current_position() {
    let curve = |t: u64| Offset::new(t as f32, t as f32);
    let heard = run(|trackpad| trackpad.animate_move_along(&curve, 4 * T));
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, curve(T), NONE),
            (2 * T, Move, false, curve(2 * T), NONE),
            (3 * T, Move, false, curve(3 * T), NONE),
            (4 * T, Move, false, curve(4 * T), NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/MoveTest.kt, animateAlong_fromOtherPosition
/// A curve starting elsewhere moves there first, at once.
#[test]
fn animate_along_from_other_position() {
    let curve = |t: u64| Offset::new(1.0 + t as f32, 1.0 + t as f32);
    let heard = run(|trackpad| trackpad.animate_move_along(&curve, 4 * T));
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, curve(0), NONE),
            (T, Move, false, curve(T), NONE),
            (2 * T, Move, false, curve(2 * T), NONE),
            (3 * T, Move, false, curve(3 * T), NONE),
            (4 * T, Move, false, curve(4 * T), NONE),
        ]
    );
}

// ---- Click -------------------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, click_pressIn_releaseIn
/// A release inside hovers again: a move where the cursor is, at the release's time.
#[test]
fn click_press_in_release_in() {
    let heard = run(|trackpad| {
        trackpad.move_to(IN);
        trackpad.press(TrackpadButton::Primary);
        trackpad.move_to(MOVE1);
        trackpad.release(TrackpadButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, IN, NONE),
            (T, Press, true, IN, PRIMARY),
            (2 * T, Move, true, MOVE1, PRIMARY),
            (2 * T, Release, false, MOVE1, NONE),
            (2 * T, Move, false, MOVE1, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, click_pressIn_moveOutIn_releaseIn
/// A held trackpad button dragged out of the box and back: the box hears it exit and enter,
/// pressed.
#[test]
fn click_press_in_move_out_in_release_in() {
    let heard = run(|trackpad| {
        trackpad.move_to(IN);
        trackpad.press(TrackpadButton::Primary);
        trackpad.move_to(OUT);
        trackpad.move_to(MOVE1);
        trackpad.release(TrackpadButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, IN, NONE),
            (T, Press, true, IN, PRIMARY),
            (2 * T, Exit, true, OUT, PRIMARY),
            (3 * T, Enter, true, MOVE1, PRIMARY),
            (3 * T, Release, false, MOVE1, NONE),
            (3 * T, Move, false, MOVE1, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, click_pressIn_releaseOut
/// Released out of the box, the box heard its pressed exit as the drag left it, then the release.
#[test]
fn click_press_in_release_out() {
    let heard = run(|trackpad| {
        trackpad.move_to(IN);
        trackpad.press(TrackpadButton::Primary);
        trackpad.move_to(OUT);
        trackpad.release(TrackpadButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, IN, NONE),
            (T, Press, true, IN, PRIMARY),
            (2 * T, Exit, true, OUT, PRIMARY),
            (2 * T, Release, false, OUT, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, click_twoButtons_symmetric
/// A second button pressed and released keeps the pointer down (an event of its own, where
/// Compose's is a move).
#[test]
fn click_two_buttons_symmetric() {
    let heard = run(|trackpad| {
        trackpad.move_to(IN);
        trackpad.press(TrackpadButton::Primary);
        trackpad.move_to(MOVE1);
        trackpad.press(TrackpadButton::Secondary);
        trackpad.move_to(MOVE2);
        trackpad.release(TrackpadButton::Secondary);
        trackpad.release(TrackpadButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, IN, NONE),
            (T, Press, true, IN, PRIMARY),
            (2 * T, Move, true, MOVE1, PRIMARY),
            (2 * T, Press, true, MOVE1, BOTH),
            (3 * T, Move, true, MOVE2, BOTH),
            (3 * T, Release, true, MOVE2, PRIMARY),
            (3 * T, Release, false, MOVE2, NONE),
            (3 * T, Move, false, MOVE2, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, click_twoButtons_staggered
#[test]
fn click_two_buttons_staggered() {
    let heard = run(|trackpad| {
        trackpad.move_to(IN);
        trackpad.press(TrackpadButton::Primary);
        trackpad.move_to(MOVE1);
        trackpad.press(TrackpadButton::Secondary);
        trackpad.move_to(MOVE2);
        trackpad.release(TrackpadButton::Primary);
        trackpad.release(TrackpadButton::Secondary);
    });
    assert_eq!(
        summaries(&heard),
        [
            (T, Enter, false, IN, NONE),
            (T, Press, true, IN, PRIMARY),
            (2 * T, Move, true, MOVE1, PRIMARY),
            (2 * T, Press, true, MOVE1, BOTH),
            (3 * T, Move, true, MOVE2, BOTH),
            (3 * T, Release, true, MOVE2, SECONDARY),
            (3 * T, Release, false, MOVE2, NONE),
            (3 * T, Move, false, MOVE2, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, press_alreadyPressed
#[test]
fn press_already_pressed() {
    let heard = run(|trackpad| {
        trackpad.move_to(IN);
        trackpad.press(TrackpadButton::Primary);
        let message = expect_error(|| trackpad.press(TrackpadButton::Primary));
        let id = TrackpadButton::Primary.button_id();
        assert!(
            message.contains(&format!(
                "Cannot send trackpad button down event, button {id} is already pressed"
            )),
            "{message}"
        );
    });
    assert_eq!(summaries(&heard), [(T, Enter, false, IN, NONE), (T, Press, true, IN, PRIMARY)]);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, clickTest
/// A click is a press and a release at once at the center; the cursor comes over the box with
/// the press (Compose's box hears it come in after the release).
#[test]
fn click() {
    let heard = run(|trackpad| trackpad.click());
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, CENTER, PRIMARY),
            (0, Press, true, CENTER, PRIMARY),
            (0, Release, false, CENTER, NONE),
            (0, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, rightClickTest
#[test]
fn right_click() {
    let heard = run(|trackpad| {
        let center = trackpad.center();
        trackpad.right_click(center);
    });
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, CENTER, SECONDARY),
            (0, Press, true, CENTER, SECONDARY),
            (0, Release, false, CENTER, NONE),
            (0, Move, false, CENTER, NONE),
        ]
    );
}

/// The default double click's delay with the test configuration: (40 + 200) / 2.
const DOUBLE_CLICK_DELAY: u64 = 120;

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, doubleClickTest
#[test]
fn double_click() {
    let heard = run(|trackpad| {
        let center = trackpad.center();
        trackpad.double_click(center, TrackpadButton::Primary);
    });
    let second = DOUBLE_CLICK_DELAY;
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, CENTER, PRIMARY),
            (0, Press, true, CENTER, PRIMARY),
            (0, Release, false, CENTER, NONE),
            (0, Move, false, CENTER, NONE),
            (second, Press, true, CENTER, PRIMARY),
            (second, Release, false, CENTER, NONE),
            (second, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, tripleClickTest
#[test]
fn triple_click() {
    let heard = run(|trackpad| {
        let center = trackpad.center();
        trackpad.triple_click(center, TrackpadButton::Primary);
    });
    let (second, third) = (DOUBLE_CLICK_DELAY, 2 * DOUBLE_CLICK_DELAY);
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, CENTER, PRIMARY),
            (0, Press, true, CENTER, PRIMARY),
            (0, Release, false, CENTER, NONE),
            (0, Move, false, CENTER, NONE),
            (second, Press, true, CENTER, PRIMARY),
            (second, Release, false, CENTER, NONE),
            (second, Move, false, CENTER, NONE),
            (third, Press, true, CENTER, PRIMARY),
            (third, Release, false, CENTER, NONE),
            (third, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, longClickTest
/// A long click holds the button 100 ms past the long-press timeout (300 ms here).
#[test]
fn long_click() {
    let heard = run(|trackpad| {
        let center = trackpad.center();
        trackpad.long_click(center, TrackpadButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, CENTER, PRIMARY),
            (0, Press, true, CENTER, PRIMARY),
            (400, Release, false, CENTER, NONE),
            (400, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ClickTest.kt, dragAndDropTest
/// A drag and drop moves a `draggable_2d` box by the drag.
#[test]
fn drag_and_drop() {
    let offset = compose_runtime::mutable_state_of(Offset::ZERO);
    let handle = offset.handle();
    let mut ui = compose_test(move || {
        let position = handle.get();
        Box!(modifier = Modifier.padding(16.dp()).fill_max_size(), {
            let state = remember_draggable_2d_state!(move |delta: Offset| {
                handle.set(handle.get() + delta)
            });
            Box!(
                modifier = Modifier
                    .test_tag("draggable-box")
                    .offset(position.x.dp(), position.y.dp())
                    .size(24.dp())
                    .draggable_2d(state, Draggable2DOptions::default())
            );
        });
    });
    ui.on_node_with_tag("draggable-box").perform_trackpad_input(|trackpad| {
        let (center, width, height) =
            (trackpad.center(), trackpad.width() as f32, trackpad.height() as f32);
        let duration = trackpad.default_gesture_millis();
        trackpad.drag_and_drop(
            center,
            center + Offset::new(2.0 * width, 4.0 * height),
            TrackpadButton::Primary,
            duration,
        );
    });
    ui.wait_for_idle();
    let moved = offset.get();
    assert!((moved.x - 48.0).abs() <= 0.5, "{moved:?}");
    assert!((moved.y - 96.0).abs() <= 0.5, "{moved:?}");
}

// ---- Pan ---------------------------------------------------------------------------------------

/// A pan of `delta` after the cursor entered at the box's top left.
fn panned(press_first: bool, delta: Offset) -> Vec<Heard> {
    run(|trackpad| {
        trackpad.enter(Offset::ZERO);
        if press_first {
            trackpad.press(TrackpadButton::Primary);
        }
        trackpad.pan(delta);
    })
}

/// A pan is `PanStart`, `PanMove` and `PanEnd` at the cursor, which keeps hovering, then a
/// move as it hovers again; the move's offset is the scroll's, the fingers' turned around.
#[track_caller]
fn assert_pan(heard: &[Heard], scroll: Offset) {
    let pans: Vec<(u64, PointerEventType, bool, Offset, Offset)> = heard
        .iter()
        .map(|event| (event.millis, event.kind, event.pressed, event.position, event.pan_offset))
        .collect();
    assert_eq!(
        pans[pans.len() - 4..],
        [
            (T, PanStart, false, Offset::ZERO, Offset::ZERO),
            (2 * T, PanMove, false, Offset::ZERO, scroll),
            (3 * T, PanEnd, false, Offset::ZERO, Offset::ZERO),
            (3 * T, Move, false, Offset::ZERO, Offset::ZERO),
        ]
    );
    // One pointer throughout, as Compose's `assertSinglePointer`.
    assert!(heard.iter().all(|event| event.pointer == heard[0].pointer), "{heard:?}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/PanTest.kt, scrollVertically
#[test]
fn pan_scrolls_vertically() {
    let heard = panned(false, Offset::new(0.0, 10.0));
    assert_eq!(heard.len(), 5, "{heard:?}");
    assert_eq!(heard[0].summary(), (T, Enter, false, Offset::ZERO, NONE));
    assert_pan(&heard, Offset::new(0.0, -10.0));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/PanTest.kt, scrollHorizontally
#[test]
fn pan_scrolls_horizontally() {
    let heard = panned(false, Offset::new(10.0, 0.0));
    assert_eq!(heard.len(), 5, "{heard:?}");
    assert_eq!(heard[0].summary(), (T, Enter, false, Offset::ZERO, NONE));
    assert_pan(&heard, Offset::new(-10.0, 0.0));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/PanTest.kt, scrollWithPrimaryDownReleasesButton
/// A pan with a button held releases it first, and the cursor hovers again.
#[test]
fn pan_with_primary_down_releases_the_button() {
    let heard = panned(true, Offset::new(10.0, 0.0));
    assert_eq!(heard.len(), 8, "{heard:?}");
    assert_eq!(
        summaries(&heard[..4]),
        [
            (T, Enter, false, Offset::ZERO, NONE),
            (T, Press, true, Offset::ZERO, PRIMARY),
            (T, Release, false, Offset::ZERO, NONE),
            (T, Move, false, Offset::ZERO, NONE),
        ]
    );
    assert_pan(&heard, Offset::new(-10.0, 0.0));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/PanWithVelocityTest.kt, panWithVelocity
/// A pan with velocity ends at that velocity, its moves at the centre where the cursor is, over
/// the duration asked for.
#[test]
fn pan_with_velocity() {
    for duration in [100, 500, 1000] {
        for velocity in [100.0, 999.0, 2000.0] {
            let (mut ui, events, start) = recording(800.0);
            start.set(ui.current_time_millis());
            ui.on_node_with_tag("box").perform_trackpad_input(|trackpad| {
                let center = trackpad.center();
                trackpad.move_to(center);
                trackpad.pan_with_velocity(Offset::new(200.0, 0.0), velocity, Some(duration));
            });
            let heard = events.borrow().clone();
            let case = format!("{duration} ms, {velocity} px/s");
            assert!(heard.len() >= (100 / T) as usize, "{case}: {heard:?}");
            assert!(heard.iter().all(|event| event.pointer == heard[0].pointer), "{case}");
            assert!(heard.windows(2).all(|pair| pair[0].millis <= pair[1].millis), "{case}");
            assert_eq!(heard.last().unwrap().millis - heard[0].millis, duration, "{case}");
            let center = Offset::new(400.0, 400.0);
            assert!(heard.iter().all(|event| event.position == center), "{case}: {heard:?}");
            let kinds: Vec<PointerEventType> = heard.iter().map(|event| event.kind).collect();
            let last = kinds.len() - 1;
            assert_eq!((kinds[0], kinds[1]), (Enter, PanStart), "{case}");
            assert!(kinds[2..last - 1].iter().all(|kind| *kind == PanMove), "{case}: {kinds:?}");
            assert_eq!((kinds[last - 1], kinds[last]), (PanEnd, Move), "{case}");
            let mut tracker = VelocityTracker::new();
            let mut panned = Offset::ZERO;
            for event in heard.iter().filter(|event| event.kind == PanMove) {
                panned = panned + event.pan_offset;
                tracker.add_position(event.millis as i64, panned);
            }
            let velocity_now = tracker.calculate_velocity();
            assert!((-velocity_now.x - velocity).abs() < 0.1, "{case}: {velocity_now:?}");
            assert!(velocity_now.y.abs() < 0.1, "{case}: {velocity_now:?}");
        }
    }
}

// ---- Scale -------------------------------------------------------------------------------------

/// A pinch at the box's centre scaling by `factor`.
fn scaled(factor: f32) -> Vec<Heard> {
    let (mut ui, events, start) = recording(300.0);
    start.set(ui.current_time_millis());
    ui.on_node_with_tag("box").perform_trackpad_input(|trackpad| {
        let center = trackpad.center();
        trackpad.move_to(center);
        trackpad.scale(factor);
    });
    events.take()
}

/// A pinch is `ScaleStart`, a change for its second finger coming down, the scale's change, one
/// for that finger lifting, and `ScaleEnd`, all at the cursor; then it hovers again (Compose's
/// box hears an `Enter` there, its exit lost on the way, b/461873914; compose-rs's cursor never
/// stopped hovering, so it's a move).
#[track_caller]
fn assert_scale(heard: &[Heard], factor: f32) {
    let center = Offset::new(150.0, 150.0);
    let scales: Vec<(u64, PointerEventType, bool, Offset, f32)> = heard
        .iter()
        .map(|event| (event.millis, event.kind, event.pressed, event.position, event.scale_factor))
        .collect();
    assert_eq!(
        scales,
        [
            (T, Enter, false, center, 1.0),
            (T, ScaleStart, false, center, 1.0),
            (T, ScaleChange, false, center, 1.0),
            (2 * T, ScaleChange, false, center, factor),
            (3 * T, ScaleChange, false, center, 1.0),
            (3 * T, ScaleEnd, false, center, 1.0),
            (3 * T, Move, false, center, 1.0),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ScaleTest.kt, pinchTogether
#[test]
fn pinch_together() {
    assert_scale(&scaled(0.9), 0.9);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/trackpad/ScaleTest.kt, pinchAway
#[test]
fn pinch_away() {
    assert_scale(&scaled(1.1), 1.1);
}

// ---- What the dispatcher sends -----------------------------------------------------------------

/// The box at the root's top left: a position outside the root is one left of it.
const OUTSIDE: Offset = Offset::new(-1.0, -1.0);

type Input = dyn Fn(&mut TrackpadInjectionScope<'_>);

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/TrackpadEventsTest.kt, enqueueTrackpadDown_alreadyDown, enqueueTrackpadDown_outOfBounds, enqueueTrackpadUp_withoutDown, enqueueTrackpadEnter_alreadyEntered, enqueueTrackpadEnter_buttonsDown, enqueueTrackpadEnter_outOfBounds, enqueueTrackpadExit_notEntered, enqueueTrackpadCancel_withoutDown
/// What can't be sent says why.
#[test]
fn what_cannot_be_sent_says_why() {
    let cases: [(&str, &Input); 8] = [
        ("Cannot send trackpad button down event, button 1 is already pressed", &|trackpad| {
            trackpad.press(TrackpadButton::Primary);
            trackpad.press(TrackpadButton::Primary);
        }),
        ("Cannot start a trackpad gesture outside the Compose root bounds", &|trackpad| {
            trackpad.update_pointer_to(OUTSIDE);
            trackpad.press(TrackpadButton::Primary);
        }),
        ("Cannot send trackpad button up event, button 1 is not pressed", &|trackpad| {
            trackpad.release(TrackpadButton::Primary)
        }),
        ("Cannot send trackpad hover enter event, trackpad is already hovering", &|trackpad| {
            trackpad.enter(IN);
            trackpad.enter(IN);
        }),
        ("Cannot send trackpad hover enter event, trackpad buttons are down", &|trackpad| {
            trackpad.press(TrackpadButton::Primary);
            trackpad.enter(IN);
        }),
        (
            "Cannot send trackpad hover enter event, Offset { x: -1.0, y: -1.0 } is out of bounds",
            &|trackpad| trackpad.enter(OUTSIDE),
        ),
        ("Cannot send trackpad hover exit event, trackpad is not hovering", &|trackpad| {
            trackpad.exit(IN)
        }),
        ("Cannot send trackpad cancel event, no trackpad buttons are pressed", &|trackpad| {
            trackpad.cancel()
        }),
    ];
    for (expected, input) in cases {
        let message = expect_error(|| {
            run(input);
        });
        assert!(message.contains(expected), "{expected}: {message}");
    }
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/TrackpadEventsTest.kt, enqueueTrackpadPress_interruptsTouch, enqueueTrackpadMove_interruptsTouch
/// The trackpad's press or move cancels a touch gesture in progress: the touch's next move
/// says no gesture is in progress.
#[test]
fn the_trackpad_interrupts_touch() {
    for press in [true, false] {
        let (mut ui, events, _) = recording(100.0);
        ui.on_node_with_tag("box").perform_touch_input(|touch| touch.down(IN));
        ui.on_node_with_tag("box").perform_trackpad_input(|trackpad| {
            if press {
                trackpad.press(TrackpadButton::Primary);
            } else {
                trackpad.move_to(MOVE1);
            }
        });
        let message = expect_error(|| {
            ui.on_node_with_tag("box").perform_touch_input(|touch| touch.move_to(MOVE2));
        });
        assert!(message.contains("Cannot move pointers, no gesture is in progress"), "{message}");
        let kinds: Vec<PointerEventType> = events.borrow().iter().map(|event| event.kind).collect();
        let expected = if press { [Press, Enter, Press] } else { [Press, Enter, Enter] };
        assert_eq!(kinds, expected[..kinds.len()], "press: {press}");
    }
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/TrackpadEventsTest.kt, hoverOutOfRootBounds
/// A cursor leaving the window exits it; moves out there say nothing; coming back enters.
#[test]
fn hover_out_of_root_bounds() {
    let heard = run(|trackpad| {
        trackpad.move_to(IN);
        trackpad.move_to(OUTSIDE);
        trackpad.move_to(Offset::new(-2.0, -2.0));
        trackpad.move_to(MOVE1);
    });
    let kinds: Vec<(u64, PointerEventType)> =
        heard.iter().map(|event| (event.millis, event.kind)).collect();
    assert_eq!(kinds, [(T, Enter), (2 * T, Exit), (4 * T, Enter)]);
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/TrackpadEventsTest.kt, pan, pinch
/// Outside the window a pan or a pinch sends nothing, though it's in progress: its end is
/// still needed.
#[test]
fn a_pan_or_pinch_outside_the_window_sends_nothing() {
    let heard = run(|trackpad| {
        trackpad.update_pointer_to(OUTSIDE);
        trackpad.pan(Offset::new(10.0, 0.0));
        trackpad.scale(2.0);
        trackpad.pan_start();
        let message = expect_error(|| trackpad.pan_start());
        assert!(message.contains("a pan gesture is already in progress"), "{message}");
        trackpad.pan_end();
        let message = expect_error(|| trackpad.pan_end());
        assert!(message.contains("no pan gesture is in progress"), "{message}");
        let message = expect_error(|| trackpad.scale_change_by(2.0));
        assert!(message.contains("no scale gesture is in progress"), "{message}");
        trackpad.scale_start();
        let message = expect_error(|| trackpad.scale_start());
        assert!(message.contains("a scale gesture is already in progress"), "{message}");
    });
    assert!(heard.is_empty(), "{heard:?}");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, trackpadPan_hoverExitAndPruned_childScrolledOffScreen
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, trackpadPan_preservesPanEvents_whenNodeInBounds
/// A pan stays with what it started on, and comes over nothing new: what moves away from under
/// the still cursor exits (and still hears the pan); what comes under it hears nothing until the
/// cursor hovers again after the pan.
#[test]
fn a_pan_comes_over_nothing_new() {
    let (moved, under) = (Events::default(), Events::default());
    let start = Rc::new(std::cell::Cell::new(0));
    let away = compose_runtime::mutable_state_of(false);
    let handle = away.handle();
    let (top, bottom, since) = (moved.clone(), under.clone(), start.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let at = if handle.get() { 60.0 } else { 0.0 };
        let (top, bottom, since) = (top.clone(), bottom.clone(), since.clone());
        Box!(modifier = Modifier.size(200.dp()).test_tag("root"), {
            Box!(
                modifier = Modifier
                    .size(50.dp())
                    .then_custom(RecorderElement(bottom.clone(), since.clone()))
            );
            Box!(
                modifier = Modifier
                    .offset(at.dp(), at.dp())
                    .size(50.dp())
                    .then_custom(RecorderElement(top.clone(), since.clone()))
            );
        });
    });
    let cursor = Offset::new(10.0, 10.0);
    ui.on_node_with_tag("root").perform_trackpad_input(|trackpad| {
        trackpad.enter(cursor);
        trackpad.pan_start();
    });
    ui.run_on_idle(|_| away.set(true));
    ui.on_node_with_tag("root").perform_trackpad_input(|trackpad| {
        trackpad.pan_move_by(Offset::new(0.0, 5.0));
        trackpad.pan_end();
    });
    let kinds = |events: &Events| -> Vec<PointerEventType> {
        events.borrow().iter().map(|event| event.kind).collect()
    };
    assert_eq!(kinds(&moved), [Enter, PanStart, Exit, PanMove, PanEnd]);
    assert_eq!(kinds(&under), [Enter], "only once the pan ended");
    let positions: Vec<Offset> = moved.borrow().iter().map(|event| event.position).collect();
    assert_eq!(positions[3], cursor - Offset::new(60.0, 60.0), "the move where it's now");
}

/// Hears changes one at a time and its cancels.
struct PressElement(Rc<RefCell<Vec<String>>>);

struct PressNode(Rc<RefCell<Vec<String>>>);

impl ModifierNodeElement for PressElement {
    type Node = PressNode;
    fn create(&self) -> PressNode {
        PressNode(self.0.clone())
    }
    fn update(&self, _node: &mut PressNode) {}
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for PressNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for PressNode {
    fn on_pointer_event(
        &mut self,
        event: &mut compose_ui::PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main {
            self.0.borrow_mut().push(format!("{:?} {}", event.kind, event.pressed));
        }
    }
    fn on_cancel_pointer_input(&mut self) {
        self.0.borrow_mut().push("cancel".to_string());
    }
}

// Compose: compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/input/pointer/MotionEventAdapter.android.kt, the trackpad's fake fingers taking the pointer over (no test of its own)
/// A pan or a pinch starting while a button is held (from a host that didn't release it first)
/// ends that press: what it pressed is cancelled, not released, even with the cursor gone from it.
#[test]
fn a_pan_or_pinch_cancels_a_held_press() {
    for start in [PanStart, ScaleStart] {
        let heard: Rc<RefCell<Vec<String>>> = Rc::default();
        let recorder = heard.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            Box!(modifier = Modifier.size(50.dp()).then_custom(PressElement(recorder.clone())));
        });
        let mouse =
            |event: compose_ui::PointerEvent| event.with_pointer(PointerId(0), PointerType::Mouse);
        let (inside, outside) = (Offset::new(10.0, 10.0), Offset::new(150.0, 150.0));
        ui.host().send_pointer(mouse(compose_ui::PointerEvent::press(inside)));
        ui.host().send_pointer(mouse(compose_ui::PointerEvent::moved(outside, true)));
        let gesture = if start == PanStart {
            compose_ui::PointerEvent::pan(PanStart, outside, Offset::ZERO)
        } else {
            compose_ui::PointerEvent::scale(ScaleStart, outside, 1.0)
        };
        ui.host().send_pointer(mouse(gesture));
        ui.wait_for_idle();
        let heard = heard.borrow().clone();
        assert!(heard.contains(&"cancel".to_string()), "{start:?}: {heard:?}");
        assert!(!heard.iter().any(|it| it.starts_with("Release")), "{start:?}: {heard:?}");
    }
}
