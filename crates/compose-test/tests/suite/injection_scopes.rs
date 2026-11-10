//! Smaller injection-scope tests of Compose's: `injectionscope/PositionsTest`, `touch/MoveTest`,
//! `mouse/CancelTest`, `touch/MoveByWithUnplacedUiTest`, and `inputdispatcher`'s
//! `IsGestureInProgressTest` and `AdvanceEventTimeTest` (through the touch scope: Compose's
//! dispatcher is internal).

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Density, IntSize, LOCAL_DENSITY, Modifier, Offset, PointerButtons, PointerEventPass,
    PointerEventType, UnitExt,
};

use super::dispatcher::*;
use super::input::expect_error;

const TAG: &str = "widget";

/// A box of `width` × `height` px (density 1), tagged [`TAG`], as Compose's `ClickableTestBox`.
fn clickable_test_box(width: f32, height: f32) {
    Box!(modifier = Modifier.size((width.dp(), height.dp())).test_tag(TAG));
}

// ---- PositionsTest -----------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/PositionsTest.kt, testCornersEdgesAndCenter
#[test]
fn test_corners_edges_and_center() {
    let mut ui = compose_test(|| clickable_test_box(3.0, 100.0));
    ui.on_node_with_tag(TAG).perform_touch_input(|scope| {
        assert_eq!((scope.width(), scope.height()), (3, 100));
        assert_eq!((scope.left(), scope.center_x(), scope.right()), (0.0, 1.5, 2.0));
        assert_eq!((scope.top(), scope.center_y(), scope.bottom()), (0.0, 50.0, 99.0));
        assert_eq!(scope.top_left(), Offset::new(0.0, 0.0));
        assert_eq!(scope.top_center(), Offset::new(1.5, 0.0));
        assert_eq!(scope.top_right(), Offset::new(2.0, 0.0));
        assert_eq!(scope.center_left(), Offset::new(0.0, 50.0));
        assert_eq!(scope.center(), Offset::new(1.5, 50.0));
        assert_eq!(scope.center_right(), Offset::new(2.0, 50.0));
        assert_eq!(scope.bottom_left(), Offset::new(0.0, 99.0));
        assert_eq!(scope.bottom_center(), Offset::new(1.5, 99.0));
        assert_eq!(scope.bottom_right(), Offset::new(2.0, 99.0));
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/PositionsTest.kt, testRelativeOffset
#[test]
fn test_relative_offset() {
    let mut ui = compose_test(|| clickable_test_box(100.0, 100.0));
    ui.on_node_with_tag(TAG).perform_touch_input(|scope| {
        assert_eq!(scope.percent_offset(0.1, 0.1), Offset::new(10.0, 10.0));
        assert_eq!(scope.percent_offset(-0.2, 0.0), Offset::new(-20.0, 0.0));
        assert_eq!(scope.percent_offset(0.25, -0.5), Offset::new(25.0, -50.0));
        assert_eq!(scope.percent_offset(0.0, 0.5), Offset::new(0.0, 50.0));
        assert_eq!(scope.percent_offset(2.0, -2.0), Offset::new(200.0, -200.0));
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/PositionsTest.kt, testDensity
#[test]
fn test_density() {
    let expected = Density { density: 0.8238974, font_scale: 0.923457 };
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_DENSITY.provides(expected), || {
            clickable_test_box(3.0, 100.0);
        });
    });
    ui.on_node_with_tag(TAG).perform_touch_input(|scope| {
        assert_eq!(scope.density().density, expected.density);
        assert_eq!(scope.density().font_scale, expected.font_scale);
    });
}

/// A 100 px viewport scrolling two 200 px boxes: the scope's size is the viewport's.
fn test_positions_in_viewport(is_vertical: bool, reverse_scroll_direction: bool) {
    let mut ui = compose_test(move || {
        let state = remember_scroll_state(0);
        let viewport = Modifier.required_size(100.dp()).test_tag("viewport");
        if is_vertical {
            Column!(
                modifier =
                    viewport.vertical_scroll_with(state, true, None, reverse_scroll_direction),
                {
                    clickable_test_box(200.0, 200.0);
                    clickable_test_box(200.0, 200.0);
                }
            );
        } else {
            Row!(
                modifier =
                    viewport.horizontal_scroll_with(state, true, None, reverse_scroll_direction),
                {
                    clickable_test_box(200.0, 200.0);
                    clickable_test_box(200.0, 200.0);
                }
            );
        }
    });
    ui.on_node_with_tag("viewport").perform_touch_input(|scope| {
        assert_eq!((scope.width(), scope.height()), (100, 100));
        assert_eq!(scope.center(), Offset::new(50.0, 50.0));
        assert_eq!(scope.top_left(), Offset::ZERO);
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/PositionsTest.kt, testSizeInViewport_column_startAtStart
#[test]
fn test_size_in_viewport_column_start_at_start() {
    test_positions_in_viewport(true, false);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/PositionsTest.kt, testSizeInViewport_column_startAtEnd
#[test]
fn test_size_in_viewport_column_start_at_end() {
    test_positions_in_viewport(true, true);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/PositionsTest.kt, testSizeInViewport_row_startAtStart
#[test]
fn test_size_in_viewport_row_start_at_start() {
    test_positions_in_viewport(false, false);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/PositionsTest.kt, testSizeInViewport_row_startAtEnd
#[test]
fn test_size_in_viewport_row_start_at_end() {
    test_positions_in_viewport(false, true);
}

// ---- touch/MoveTest ----------------------------------------------------------------------------

const DOWN_POSITION_1: Offset = Offset::new(10.0, 10.0);

fn move_error(before: impl Fn(&mut ComposeTestRule)) -> String {
    let mut ui = compose_test(|| clickable_test_box(100.0, 100.0));
    before(&mut ui);
    expect_error(|| {
        ui.on_node_with_tag(TAG).perform_touch_input(|scope| scope.move_pointers());
    })
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/MoveTest.kt, move_withoutDown
#[test]
fn move_without_down() {
    let message = move_error(|_| {});
    assert!(message.contains("Cannot send MOVE event, no gesture is in progress"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/MoveTest.kt, move_afterUp
#[test]
fn move_after_up() {
    let message = move_error(|ui| {
        ui.on_node_with_tag(TAG).perform_touch_input(|scope| scope.down(DOWN_POSITION_1));
        ui.on_node_with_tag(TAG).perform_touch_input(|scope| scope.up());
    });
    assert!(message.contains("Cannot send MOVE event, no gesture is in progress"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/MoveTest.kt, move_afterCancel
#[test]
fn move_after_cancel() {
    let message = move_error(|ui| {
        ui.on_node_with_tag(TAG).perform_touch_input(|scope| scope.down(DOWN_POSITION_1));
        ui.on_node_with_tag(TAG).perform_touch_input(|scope| scope.cancel());
    });
    assert!(message.contains("Cannot send MOVE event, no gesture is in progress"), "{message}");
}

// ---- mouse/CancelTest --------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/CancelTest.kt, cancel
/// The press comes over the box (its `Enter`, unpressed), then presses; the cancel sends no
/// event of its own (the gesture's cancel).
#[test]
fn cancel() {
    let (mut ui, log) = recording_root();
    ui.on_node_with_tag(ROOT).perform_mouse_input(|scope| {
        scope.press(MouseButton::Primary);
        scope.cancel();
    });
    let heard = heard(&log);
    let primary = PointerButtons::PRIMARY;
    assert_eq!(heard[0], mouse(PointerEventType::Enter, 0, Offset::ZERO, primary, false));
    assert_eq!(heard[1], mouse(PointerEventType::Press, 0, Offset::ZERO, primary, true));
    assert_eq!(heard[2..], [Heard::Cancel { millis: 16 }]);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/CancelTest.kt, cancel_withoutPress
#[test]
fn cancel_without_press() {
    let (mut ui, _) = recording_root();
    let message = expect_error(|| {
        ui.on_node_with_tag(ROOT).perform_mouse_input(|scope| scope.cancel());
    });
    assert!(
        message.contains("Cannot send mouse cancel event, no mouse buttons are pressed"),
        "{message}"
    );
}

// ---- touch/MoveByWithUnplacedUiTest ------------------------------------------------------------

/// The last event's kind and first position a pointer input kept.
type Kept = (Rc<Cell<Option<PointerEventType>>>, Rc<Cell<Offset>>);

/// A target filling the window whose layout places its content (a pointer input keeping the
/// last event's kind and first position) only while `placed`; returns the kept kind and position.
fn unplaced_target(placed: MutableState<bool>) -> (ComposeTestRule, Kept) {
    let kind: Rc<Cell<Option<PointerEventType>>> = Rc::default();
    let position = Rc::new(Cell::new(Offset::ZERO));
    let (kept_kind, kept_position) = (kind.clone(), position.clone());
    let ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (kind, position) = (kept_kind.clone(), kept_position.clone());
        Box!(modifier = Modifier.fill_max_size(), {
            let (kind, position) = (kind.clone(), position.clone());
            Box!(
                modifier = Modifier
                    .test_tag("TargetTag")
                    .layout(move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        let (width, height) = (placeable.width, placeable.height);
                        let place = placed.get();
                        scope.layout_with(width, height, move |placement| {
                            if place {
                                placement.place(&placeable, 0, 0);
                            }
                        })
                    })
                    .fill_max_size()
                    .pointer_input((), move |scope| {
                        let (kind, position) = (kind.clone(), position.clone());
                        async move {
                            loop {
                                let event = scope.await_pointer_event(PointerEventPass::Main).await;
                                kind.set(Some(event.kind));
                                position.set(event.changes[0].position);
                            }
                        }
                    })
            );
        });
    });
    (ui, (kind, position))
}

const MOVE_TO_POSITION_1: Offset = Offset::new(100.0, 100.0);
const MOVE_TO_POSITION_2: Offset = Offset::new(200.0, 200.0);
const MOVE_TO_POSITION_3: Offset = Offset::new(300.0, 300.0);

/// A down and a move are heard, then the content unplaced, a move isn't.
fn move_while_unplaced(
    placed: &MutableState<bool>,
    ui: &mut ComposeTestRule,
    kind: &Cell<Option<PointerEventType>>,
    position: &Cell<Offset>,
) {
    ui.run_on_idle(|_| assert_eq!((kind.get(), position.get()), (None, Offset::ZERO)));
    let down = Offset::new(5.0, 5.0);
    ui.on_node_with_tag("TargetTag").perform_touch_input(|scope| scope.down(down));
    ui.run_on_idle(|_| {
        assert_eq!((kind.get(), position.get()), (Some(PointerEventType::Press), down))
    });
    ui.on_node_with_tag("TargetTag").perform_touch_input(|scope| scope.move_to(MOVE_TO_POSITION_1));
    ui.run_on_idle(|_| {
        assert_eq!((kind.get(), position.get()), (Some(PointerEventType::Move), MOVE_TO_POSITION_1))
    });
    placed.set(false);
    ui.on_node_with_tag("TargetTag").perform_touch_input(|scope| scope.move_to(MOVE_TO_POSITION_2));
    // Not heard.
    ui.run_on_idle(|_| assert_eq!(position.get(), MOVE_TO_POSITION_1));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/MoveByWithUnplacedUiTest.kt, onePointer_moveByWithUnPlaceUi_stopsTrackingChanges
#[test]
fn one_pointer_move_by_with_un_place_ui_stops_tracking_changes() {
    let placed = mutable_state_of(true);
    let (mut ui, (kind, position)) = unplaced_target(placed.handle());
    move_while_unplaced(&placed, &mut ui, &kind, &position);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/MoveByWithUnplacedUiTest.kt, onePointer_moveByAndUnPlaceUiThenPlaceUi_temporarilyStopsTrackingChangesDuringUnPlace
#[test]
fn one_pointer_move_by_and_un_place_ui_then_place_ui_temporarily_stops_tracking_changes_during_un_place()
 {
    let placed = mutable_state_of(true);
    let (mut ui, (kind, position)) = unplaced_target(placed.handle());
    move_while_unplaced(&placed, &mut ui, &kind, &position);
    placed.set(true);
    ui.on_node_with_tag("TargetTag").perform_touch_input(|scope| scope.move_to(MOVE_TO_POSITION_3));
    // Heard again, placed again.
    ui.run_on_idle(|_| assert_eq!(position.get(), MOVE_TO_POSITION_3));
}

// ---- inputdispatcher/IsGestureInProgressTest, AdvanceEventTimeTest ------------------------------

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/IsGestureInProgressTest.kt, downUp
#[test]
fn down_up() {
    let (mut ui, _) = recording_root();
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
        assert_eq!(scope.current_position(1), None);
        scope.down_pointer(1, Offset::ZERO);
        assert_eq!(scope.current_position(1), Some(Offset::ZERO));
        scope.up_pointer(1);
        assert_eq!(scope.current_position(1), None);
    });
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/IsGestureInProgressTest.kt, downCancel
#[test]
fn down_cancel() {
    let (mut ui, _) = recording_root();
    ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
        assert_eq!(scope.current_position(1), None);
        scope.down_pointer(1, Offset::ZERO);
        assert_eq!(scope.current_position(1), Some(Offset::ZERO));
        scope.cancel();
        assert_eq!(scope.current_position(1), None);
    });
}

// Compose: compose/ui/ui-test/src/androidHostTest/kotlin/androidx/compose/ui/test/inputdispatcher/AdvanceEventTimeTest.kt, advanceEventTime
/// Three events with delays between: the times between them are the delays (Compose's
/// parameters: 0 or 23 ms, then 0 or 47 ms).
#[test]
fn advance_event_time() {
    for first_delay in [0, 23] {
        for second_delay in [0, 47] {
            let (mut ui, log) = recording_root();
            ui.on_node_with_tag(ROOT).perform_touch_input(|scope| {
                scope.down_pointer(0, Offset::ZERO);
                scope.advance_event_time(first_delay);
                scope.move_pointers_after(0);
                scope.advance_event_time(second_delay);
                scope.up_pointer(0);
            });
            let times: Vec<u64> = heard(&log).iter().map(Heard::millis).collect();
            assert_eq!(times.len(), 3);
            assert_eq!(
                [times[1] - times[0], times[2] - times[1]],
                [first_delay, second_delay],
                "delays {first_delay}, {second_delay}"
            );
        }
    }
}
