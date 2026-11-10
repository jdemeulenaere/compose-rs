//! Mouse injection, `perform_mouse_input`, as Compose's `injectionscope/mouse` tests check it
//! (`ClickTest`, `MoveTest`, `ScrollTest`): a recorder of the events a box gets, each with its
//! time since the block began, as their `Common.runMouseInputInjectionTest` and
//! `verifyMouseEvent`. Compose's sequences are those with
//! `isTriggerMoveEventsWhenLocationHasNotChangedEnabled` on (a release inside is followed by a
//! hover move), which compose-rs's injection sends.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::lazy::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{
    EventContext, IntSize, Modifier, ModifierNode, ModifierNodeElement, Offset, PointerButtons,
    PointerEventPass, PointerEventType, PointerEvents, PointerInputModifierNode, PointerType,
    UnitExt,
};

use PointerEventType::{Enter, Exit, Move, Press, Release, Scroll};

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
    pointer_type: PointerType,
    scroll: Offset,
}

/// What Compose's `verifyMouseEvent` checks (its scroll delta only in its overload that takes
/// one).
type Summary = (u64, PointerEventType, bool, Offset, PointerButtons);

impl Heard {
    fn summary(&self) -> Summary {
        (self.millis, self.kind, self.pressed, self.position, self.buttons)
    }
}

type Events = Rc<RefCell<Vec<Heard>>>;

/// Records every event its node gets, whole, as Compose's `SinglePointerInputRecorder`.
struct RecorderElement(Events, Rc<Cell<u64>>);

struct RecorderNode(Events, Rc<Cell<u64>>);

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
                pointer_type: change.pointer_type,
                scroll: change.scroll_delta,
            });
        }
    }
}

/// Runs `input` on a 100 dp recording box at the top left of a 1000 × 1000 root, with the test
/// view configuration of Compose's `Common` (double clicks 40 to 200 ms apart, a 300 ms long
/// press) and the clock not advancing by itself, as `runMouseInputInjectionTest`; returns what
/// the box heard, its times from the block's start.
fn run(input: impl FnOnce(&mut MouseInjectionScope<'_>)) -> Vec<Heard> {
    let events: Events = Rc::default();
    let start = Rc::new(Cell::new(0));
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
                .size(100.dp())
                .then_custom(RecorderElement(recorder.clone(), since.clone()))
                .test_tag("box")
        );
    });
    start.set(ui.current_time_millis());
    ui.on_node_with_tag("box").perform_mouse_input(input);
    ui.wait_for_idle();
    let heard = events.borrow().clone();
    for event in &heard {
        assert_eq!(event.pointer_type, PointerType::Mouse, "{event:?}");
    }
    heard
}

fn summaries(heard: &[Heard]) -> Vec<Summary> {
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

// ---- Click -------------------------------------------------------------------------------------

/// `ClickDuration`: how long a click holds its button.
const CLICK: u64 = 60;
/// `DefaultDoubleClickTimeMillis` with the test configuration: (40 + 200) / 2.
const DOUBLE_CLICK_DELAY: u64 = 120;

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, click_pressIn_releaseIn
/// A release inside hovers again: a move where the cursor is, at the release's time.
#[test]
fn click_press_in_release_in() {
    let heard = run(|mouse| {
        mouse.move_to(IN);
        mouse.press(MouseButton::Primary);
        mouse.move_to(MOVE1);
        mouse.release(MouseButton::Primary);
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, click_pressIn_moveOutIn_releaseIn
/// A held mouse dragged out of the box and back: the box hears it exit and enter, pressed.
#[test]
fn click_press_in_move_out_in_release_in() {
    let heard = run(|mouse| {
        mouse.move_to(IN);
        mouse.press(MouseButton::Primary);
        mouse.move_to(OUT);
        mouse.move_to(MOVE1);
        mouse.release(MouseButton::Primary);
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, click_pressIn_releaseOut
/// Released out of the box, the box heard its pressed exit as the drag left it, then the release.
#[test]
fn click_press_in_release_out() {
    let heard = run(|mouse| {
        mouse.move_to(IN);
        mouse.press(MouseButton::Primary);
        mouse.move_to(OUT);
        mouse.release(MouseButton::Primary);
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, click_twoButtons_symmetric
/// A second button pressed and released keeps the pointer down: a `Press` and a `Release` of
/// their own, where Compose's box hears moves telling the buttons (as the trackpad's test).
#[test]
fn click_two_buttons_symmetric() {
    let heard = run(|mouse| {
        mouse.move_to(IN);
        mouse.press(MouseButton::Primary);
        mouse.move_to(MOVE1);
        mouse.press(MouseButton::Secondary);
        mouse.move_to(MOVE2);
        mouse.release(MouseButton::Secondary);
        mouse.release(MouseButton::Primary);
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, click_twoButtons_staggered
#[test]
fn click_two_buttons_staggered() {
    let heard = run(|mouse| {
        mouse.move_to(IN);
        mouse.press(MouseButton::Primary);
        mouse.move_to(MOVE1);
        mouse.press(MouseButton::Secondary);
        mouse.move_to(MOVE2);
        mouse.release(MouseButton::Primary);
        mouse.release(MouseButton::Secondary);
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, press_alreadyPressed
#[test]
fn press_already_pressed() {
    let heard = run(|mouse| {
        mouse.move_to(IN);
        mouse.press(MouseButton::Primary);
        let message = expect_error(|| mouse.press(MouseButton::Primary));
        assert!(
            message.contains("Cannot send mouse button down event, button 1 is already pressed"),
            "{message}"
        );
    });
    assert_eq!(summaries(&heard), [(T, Enter, false, IN, NONE), (T, Press, true, IN, PRIMARY)]);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, clickTest
/// A click at the center, the button held 60 ms. The cursor wasn't over the box, so it comes
/// over it with the press, its `Enter` telling the press's buttons (Compose's tells none).
#[test]
fn click() {
    let heard = run(|mouse| mouse.click());
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, CENTER, PRIMARY),
            (0, Press, true, CENTER, PRIMARY),
            (CLICK, Release, false, CENTER, NONE),
            (CLICK, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, rightClickTest
#[test]
fn right_click() {
    let heard = run(|mouse| {
        let center = mouse.center();
        mouse.right_click(center);
    });
    assert_eq!(
        summaries(&heard),
        [
            (0, Enter, false, CENTER, SECONDARY),
            (0, Press, true, CENTER, SECONDARY),
            (CLICK, Release, false, CENTER, NONE),
            (CLICK, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, doubleClickTest
/// Time starts at 0: the double click presses at once.
#[test]
fn double_click() {
    let heard = run(|mouse| {
        let center = mouse.center();
        mouse.double_click(center);
    });
    let (press1, release1) = (0, CLICK);
    let press2 = release1 + DOUBLE_CLICK_DELAY;
    let release2 = press2 + CLICK;
    assert_eq!(
        summaries(&heard),
        [
            (press1, Enter, false, CENTER, PRIMARY),
            (press1, Press, true, CENTER, PRIMARY),
            (release1, Release, false, CENTER, NONE),
            (release1, Move, false, CENTER, NONE),
            (press2, Press, true, CENTER, PRIMARY),
            (release2, Release, false, CENTER, NONE),
            (release2, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, tripleClickTest
#[test]
fn triple_click() {
    let heard = run(|mouse| {
        let center = mouse.center();
        mouse.triple_click(center);
    });
    let (press1, release1) = (0, CLICK);
    let press2 = release1 + DOUBLE_CLICK_DELAY;
    let release2 = press2 + CLICK;
    let press3 = release2 + DOUBLE_CLICK_DELAY;
    let release3 = press3 + CLICK;
    assert_eq!(
        summaries(&heard),
        [
            (press1, Enter, false, CENTER, PRIMARY),
            (press1, Press, true, CENTER, PRIMARY),
            (release1, Release, false, CENTER, NONE),
            (release1, Move, false, CENTER, NONE),
            (press2, Press, true, CENTER, PRIMARY),
            (release2, Release, false, CENTER, NONE),
            (release2, Move, false, CENTER, NONE),
            (press3, Press, true, CENTER, PRIMARY),
            (release3, Release, false, CENTER, NONE),
            (release3, Move, false, CENTER, NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, longClickTest
/// A long click holds the button 100 ms past the long-press timeout (300 ms here).
#[test]
fn long_click() {
    let heard = run(|mouse| {
        let center = mouse.center();
        mouse.long_click(center);
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, dragAndDropTest
/// Rather than the events, the outcome: a drag and drop moves a `draggable_2d` box by the drag.
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
    ui.on_node_with_tag("draggable-box").perform_mouse_input(|mouse| {
        let (center, width, height) = (mouse.center(), mouse.width() as f32, mouse.height() as f32);
        let duration = mouse.default_gesture_millis();
        mouse.drag_and_drop(center, center + Offset::new(2.0 * width, 4.0 * height), duration);
    });
    ui.wait_for_idle();
    let moved = offset.get();
    assert!((moved.x - 48.0).abs() <= 0.5, "{moved:?}");
    assert!((moved.y - 96.0).abs() <= 0.5, "{moved:?}");
}

// ---- Move --------------------------------------------------------------------------------------

/// The moves of `moveTo` and `moveBy`: in at once, around, around after two periods, out, in.
const MOVES: [(Offset, u64); 5] = [(IN, 0), (MOVE1, T), (MOVE2, 2 * T), (OUT, T), (IN, T)];

/// What the box hears of [`MOVES`]: the move that brings the cursor in is its `Enter`, out of
/// it its `Exit`.
const MOVED: [Summary; 5] = [
    (0, Enter, false, IN, NONE),
    (T, Move, false, MOVE1, NONE),
    (3 * T, Move, false, MOVE2, NONE),
    (4 * T, Exit, false, OUT, NONE),
    (5 * T, Enter, false, IN, NONE),
];

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, moveTo
#[test]
fn move_to() {
    let heard = run(|mouse| {
        for (position, delay) in MOVES {
            mouse.move_to_after(position, delay);
            assert_eq!(mouse.current_position(), position);
        }
    });
    assert_eq!(summaries(&heard), MOVED);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, moveBy
#[test]
fn move_by() {
    let heard = run(|mouse| {
        let mut at = Offset::ZERO;
        for (position, delay) in MOVES {
            mouse.move_by_after(position - at, delay);
            assert_eq!(mouse.current_position(), position);
            at = position;
        }
    });
    assert_eq!(summaries(&heard), MOVED);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, updatePointerTo
/// Moving the cursor without events sends nothing: the press is where it ended. The cursor
/// wasn't over the box, so it comes over it with the press, its `Enter` telling the press's
/// buttons (Compose's tells none).
#[test]
fn update_pointer_to() {
    let heard = run(|mouse| {
        for position in [IN, MOVE1, MOVE2] {
            mouse.update_pointer_to(position);
            assert_eq!(mouse.current_position(), position);
        }
        mouse.press(MouseButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [(0, Enter, false, MOVE2, PRIMARY), (0, Press, true, MOVE2, PRIMARY)]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, updatePointerBy
#[test]
fn update_pointer_by() {
    let heard = run(|mouse| {
        let mut at = Offset::ZERO;
        for position in [IN, MOVE1, MOVE2] {
            mouse.update_pointer_by(position - at);
            assert_eq!(mouse.current_position(), position);
            at = position;
        }
        mouse.press(MouseButton::Primary);
    });
    assert_eq!(
        summaries(&heard),
        [(0, Enter, false, MOVE2, PRIMARY), (0, Press, true, MOVE2, PRIMARY)]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, enter_exit
#[test]
fn enter_exit() {
    let heard = run(|mouse| {
        mouse.enter(IN);
        mouse.move_to(MOVE1);
        mouse.exit(OUT);
    });
    let summaries = summaries(&heard);
    assert_eq!(summaries[..2], [(T, Enter, false, IN, NONE), (2 * T, Move, false, MOVE1, NONE)]);
    // The window's exit carries no position (the host's `send_pointer_left`): where the cursor
    // leaves isn't the one Compose's box hears (`OUT`).
    assert_eq!((summaries[2].0, summaries[2].1, summaries.len()), (3 * T, Exit, 3));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, enter_alreadyEntered
#[test]
fn enter_already_entered() {
    let heard = run(|mouse| {
        mouse.enter(IN);
        let message = expect_error(|| mouse.enter(MOVE1));
        assert!(
            message.contains("Cannot send mouse hover enter event, mouse is already hovering"),
            "{message}"
        );
    });
    assert_eq!(summaries(&heard), [(T, Enter, false, IN, NONE)]);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, exit_notEntered
#[test]
fn exit_not_entered() {
    let heard = run(|mouse| {
        let message = expect_error(|| mouse.exit(OUT));
        assert!(
            message.contains("Cannot send mouse hover exit event, mouse is not hovering"),
            "{message}"
        );
    });
    assert!(heard.is_empty(), "{heard:?}");
}

const STEP: Offset = Offset::new(5.0, 5.0);

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, animatePointerTo
#[test]
fn animate_pointer_to() {
    let heard = run(|mouse| mouse.animate_move_to(STEP * 4.0, 4 * T));
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, animatePointerBy
#[test]
fn animate_pointer_by() {
    let start = Offset::new(1.0, 1.0);
    let heard = run(|mouse| {
        mouse.move_to(start);
        mouse.animate_move_by(STEP * 4.0, 4 * T);
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, animateAlong_fromCurrentPosition
/// A curve starting where the cursor is sends no first move.
#[test]
fn animate_along_from_current_position() {
    let curve = |t: u64| Offset::new(t as f32, t as f32);
    let heard = run(|mouse| mouse.animate_move_along(&curve, 4 * T));
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

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt, animateAlong_fromOtherPosition
/// A curve starting elsewhere moves there first, at once.
#[test]
fn animate_along_from_other_position() {
    let curve = |t: u64| Offset::new(1.0 + t as f32, 1.0 + t as f32);
    let heard = run(|mouse| mouse.animate_move_along(&curve, 4 * T));
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

// ---- Scroll ------------------------------------------------------------------------------------
//
// Two differences from Compose's sequences, as `mouse_events.rs`' scroll tests: compose-rs's
// wheel delta is in pixels, forward positive, 64 dp a unit (Compose's change carries the wheel's
// units); and a scroll comes after a move to where the cursor is, which the box hears (its
// `Enter` the first time, a `Move` after), where Android's scroll ends the hover around it (the
// box hears `Exit`, the scroll, `Enter`).

/// A wheel's unit, in pixels at density 1.
const UNIT: f32 = 64.0;

/// The events with their scroll deltas, as `verifyMouseEvent`'s overload taking one.
fn scrolls(heard: &[Heard]) -> Vec<(u64, PointerEventType, bool, Offset, Offset, PointerButtons)> {
    heard
        .iter()
        .map(|event| {
            (event.millis, event.kind, event.pressed, event.position, event.scroll, event.buttons)
        })
        .collect()
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, scrollVertically
#[test]
fn scroll_vertically() {
    let heard = run(|mouse| mouse.scroll(10.0, ScrollWheel::Vertical));
    assert_eq!(
        scrolls(&heard),
        [
            (0, Enter, false, Offset::ZERO, Offset::ZERO, NONE),
            (0, Scroll, false, Offset::ZERO, Offset::new(0.0, 10.0 * UNIT), NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, scrollHorizontally
#[test]
fn scroll_horizontally() {
    let heard = run(|mouse| mouse.scroll(10.0, ScrollWheel::Horizontal));
    assert_eq!(
        scrolls(&heard),
        [
            (0, Enter, false, Offset::ZERO, Offset::ZERO, NONE),
            (0, Scroll, false, Offset::ZERO, Offset::new(10.0 * UNIT, 0.0), NONE),
        ]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, scrollWithPrimaryDown
/// A scroll with a button held: the scroll tells it (the `Enter` with the press tells it too, as
/// the click tests'.)
#[test]
fn scroll_with_primary_down() {
    let heard = run(|mouse| {
        mouse.press(MouseButton::Primary);
        mouse.scroll(10.0, ScrollWheel::Vertical);
    });
    assert_eq!(
        scrolls(&heard),
        [
            (0, Enter, false, Offset::ZERO, Offset::ZERO, PRIMARY),
            (0, Press, true, Offset::ZERO, Offset::ZERO, PRIMARY),
            (0, Move, true, Offset::ZERO, Offset::ZERO, PRIMARY),
            (0, Scroll, true, Offset::ZERO, Offset::new(0.0, 10.0 * UNIT), PRIMARY),
        ]
    );
}

/// What a smooth scroll of 40 in 4 steps an event period apart sends, each step 10 along
/// `step`.
fn smoothly_scrolled(
    step: Offset,
) -> Vec<(u64, PointerEventType, bool, Offset, Offset, PointerButtons)> {
    let mut expected = Vec::new();
    for i in 1..=4 {
        let came = if i == 1 { Enter } else { Move };
        expected.extend([
            (i * T, came, false, Offset::ZERO, Offset::ZERO, NONE),
            (i * T, Scroll, false, Offset::ZERO, step * UNIT, NONE),
        ]);
    }
    expected
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, smoothScrollVertically
#[test]
fn smooth_scroll_vertically() {
    let heard = run(|mouse| mouse.smooth_scroll(40.0, 4 * T, ScrollWheel::Vertical));
    assert_eq!(scrolls(&heard), smoothly_scrolled(Offset::new(0.0, 10.0)));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, smoothScrollHorizontally
#[test]
fn smooth_scroll_horizontally() {
    let heard = run(|mouse| mouse.smooth_scroll(40.0, 4 * T, ScrollWheel::Horizontal));
    assert_eq!(scrolls(&heard), smoothly_scrolled(Offset::new(10.0, 0.0)));
}

const ITEMS: usize = 200;

/// Compose's `TestItem`: tagged `item-{i}`, at least 10 dp each way.
fn test_item(i: usize, modifier: Modifier) {
    Box!(modifier = modifier.test_tag(format!("item-{i}")).default_min_size(10.dp(), 10.dp()));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, smoothScrollLazyColumn
/// A lazy column scrolls by the wheel to its last item.
#[test]
fn smooth_scroll_lazy_column() {
    let mut ui = compose_test(|| {
        LazyColumn!(modifier = Modifier.test_tag("list").width(50.dp()), |scope: LazyListScope| {
            scope.items_list((0..ITEMS).collect::<Vec<usize>>(), |item_scope, i: &usize| {
                test_item(*i, Modifier.fill_parent_max_width(&item_scope));
            });
        });
    });
    ui.on_node_with_tag("list")
        .perform_mouse_input(|mouse| mouse.smooth_scroll(100.0, 500, ScrollWheel::Vertical));
    ui.on_node_with_tag(&format!("item-{}", ITEMS - 1)).assert_is_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, smoothScrollColumn
#[test]
fn smooth_scroll_column() {
    let mut ui = compose_test(|| {
        let state = remember_scroll_state!();
        Column!(modifier = Modifier.test_tag("list").width(50.dp()).vertical_scroll(state), {
            for i in 0..ITEMS {
                test_item(i, Modifier.fill_max_width());
            }
        });
    });
    ui.on_node_with_tag("list")
        .perform_mouse_input(|mouse| mouse.smooth_scroll(100.0, 500, ScrollWheel::Vertical));
    ui.on_node_with_tag(&format!("item-{}", ITEMS - 1)).assert_is_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, smoothScrollLazyRow
#[test]
fn smooth_scroll_lazy_row() {
    let mut ui = compose_test(|| {
        LazyRow!(modifier = Modifier.test_tag("list").height(50.dp()), |scope: LazyListScope| {
            scope.items_list((0..ITEMS).collect::<Vec<usize>>(), |item_scope, i: &usize| {
                test_item(*i, Modifier.fill_parent_max_height(&item_scope));
            });
        });
    });
    ui.on_node_with_tag("list")
        .perform_mouse_input(|mouse| mouse.smooth_scroll(100.0, 500, ScrollWheel::Horizontal));
    ui.on_node_with_tag(&format!("item-{}", ITEMS - 1)).assert_is_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt, smoothScrollRow
#[test]
fn smooth_scroll_row() {
    let mut ui = compose_test(|| {
        let state = remember_scroll_state!();
        Row!(modifier = Modifier.test_tag("list").height(50.dp()).horizontal_scroll(state), {
            for i in 0..ITEMS {
                test_item(i, Modifier.fill_max_height());
            }
        });
    });
    ui.on_node_with_tag("list")
        .perform_mouse_input(|mouse| mouse.smooth_scroll(100.0, 500, ScrollWheel::Horizontal));
    ui.on_node_with_tag(&format!("item-{}", ITEMS - 1)).assert_is_displayed();
}
