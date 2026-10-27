//! Input injection: `perform_touch_input`, `perform_mouse_input` and `perform_key_input`, as
//! Compose's `injectionscope` tests check them, with a recorder of the events a node gets.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{
    EventContext, IntSize, Key, KeyEvent, KeyEventType, Modifier, ModifierNode,
    ModifierNodeElement, Offset, PointerEvent, PointerEventPass, PointerEventType, PointerId,
    PointerInputModifierNode, PointerType, TapGestures, UnitExt,
};

/// One pointer event as a node got it: what, which pointer, where, when (ms).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Recorded {
    kind: PointerEventType,
    pointer: PointerId,
    pointer_type: PointerType,
    position: Offset,
    pressed: bool,
    millis: u64,
    scroll: Offset,
}

type Events = Rc<RefCell<Vec<Recorded>>>;

/// Records the pointer events its node gets (the main pass), as Compose's
/// `SinglePointerInputRecorder`/`MultiPointerInputRecorder`; a cancel shows as `Exit`.
struct RecorderElement(Events);

struct RecorderNode(Events);

impl ModifierNodeElement for RecorderElement {
    type Node = RecorderNode;
    fn create(&self) -> RecorderNode {
        RecorderNode(self.0.clone())
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
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _bounds: IntSize,
        _context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main {
            self.0.borrow_mut().push(Recorded {
                kind: event.kind,
                pointer: event.pointer,
                pointer_type: event.pointer_type,
                position: event.position,
                pressed: event.pressed,
                millis: event.time_nanos / 1_000_000,
                scroll: event.scroll_delta,
            });
        }
    }
}

const SIZE: f32 = 100.0;

/// A 100 dp box recording its events, tagged "box", as Compose's `ClickableTestBox`.
fn recording_box(at: Offset) -> (ComposeTestRule, Events) {
    let events: Events = Rc::default();
    let recorder = events.clone();
    let ui = compose_test(move || {
        let recorder = recorder.clone();
        Box!(
            modifier = Modifier
                .offset(at.x.dp(), at.y.dp())
                .size(SIZE.dp())
                .then_custom(RecorderElement(recorder))
                .test_tag("box")
        );
    });
    (ui, events)
}

fn kinds(events: &Events) -> Vec<PointerEventType> {
    events.borrow().iter().map(|event| event.kind).collect()
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

// ---- Touch ---------------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/ClickTest.kt, click
/// A click is a touch down, a move one event period (16 ms) later, and an up, at the
/// position in the node's coordinates (its center by default).
#[test]
fn a_touch_click_is_a_down_a_move_and_an_up() {
    for position in [
        Some(Offset::new(0.0, 0.0)),
        Some(Offset::new(SIZE - 1.0, 0.0)),
        Some(Offset::new(0.0, SIZE - 1.0)),
        Some(Offset::new(SIZE - 1.0, SIZE - 1.0)),
        None,
    ] {
        let (mut ui, events) = recording_box(Offset::new(20.0, 30.0));
        ui.on_node_with_tag("box").perform_touch_input(|touch| match position {
            Some(position) => touch.click_at(position),
            None => touch.click(),
        });
        let expected = position.unwrap_or(Offset::new(SIZE / 2.0, SIZE / 2.0));
        let recorded = events.borrow().clone();
        assert_eq!(
            kinds(&events),
            [PointerEventType::Press, PointerEventType::Move, PointerEventType::Release],
            "{position:?}"
        );
        let t0 = recorded[0].millis;
        for (event, (millis, pressed)) in
            recorded.iter().zip([(t0, true), (t0 + 16, true), (t0 + 16, false)])
        {
            assert_eq!(event.millis, millis, "{position:?}: {event:?}");
            assert_eq!(event.pressed, pressed, "{position:?}: {event:?}");
            assert_eq!(event.position, expected, "{position:?}: {event:?}");
            assert_eq!(event.pointer_type, PointerType::Touch);
            assert_eq!(event.pointer, recorded[0].pointer);
        }
    }
}

/// `perform_click` is a touch click, as Compose's `performClick`.
#[test]
fn perform_click_is_a_touch_click() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_click();
    assert_eq!(
        kinds(&events),
        [PointerEventType::Press, PointerEventType::Move, PointerEventType::Release]
    );
    assert!(events.borrow().iter().all(|event| event.pointer_type == PointerType::Touch));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/DownTest.kt, onePointer, twoPointers, duplicatePointers
/// Fingers go down by id, each its own pointer, at the event time; a finger down twice fails.
/// compose-rs's events carry one change each: the second finger's down doesn't list the first.
#[test]
fn fingers_go_down_by_pointer_id() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box")
        .perform_touch_input(|touch| touch.down_pointer(1, Offset::new(5.0, 5.0)));
    ui.advance_time_by(Duration::from_millis(20));
    ui.on_node_with_tag("box")
        .perform_touch_input(|touch| touch.down_pointer(2, Offset::new(7.0, 7.0)));
    let recorded = events.borrow().clone();
    assert_eq!(recorded.len(), 2);
    assert!(recorded.iter().all(|event| event.kind == PointerEventType::Press && event.pressed));
    assert_eq!(
        (recorded[0].position, recorded[1].position),
        (Offset::new(5.0, 5.0), Offset::new(7.0, 7.0))
    );
    assert_ne!(recorded[0].pointer, recorded[1].pointer);
    assert!(recorded[1].millis > recorded[0].millis);
    let message = expect_error(|| {
        ui.on_node_with_tag("box")
            .perform_touch_input(|touch| touch.down_pointer(1, Offset::new(5.0, 5.0)));
    });
    assert!(message.contains("a gesture is already in progress for pointer 1"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/MoveToTest.kt, MoveByTest.kt, UpTest.kt, CurrentPositionTest.kt
/// Moves go to or by a position an event period on (or after a delay given); `current_position`
/// follows the finger; lifting an unknown finger fails.
#[test]
fn fingers_move_and_lift() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_touch_input(|touch| {
        touch.down(Offset::new(10.0, 10.0));
        touch.move_to(Offset::new(20.0, 10.0));
        touch.move_by(Offset::new(0.0, 15.0));
        assert_eq!(touch.current_position(0), Some(Offset::new(20.0, 25.0)));
        touch.move_pointer_to(0, Offset::new(30.0, 30.0), 100);
        touch.up();
        assert_eq!(touch.current_position(0), None);
    });
    let recorded = events.borrow().clone();
    let t0 = recorded[0].millis;
    let summary: Vec<(PointerEventType, u64, Offset)> =
        recorded.iter().map(|event| (event.kind, event.millis - t0, event.position)).collect();
    assert_eq!(
        summary,
        [
            (PointerEventType::Press, 0, Offset::new(10.0, 10.0)),
            (PointerEventType::Move, 16, Offset::new(20.0, 10.0)),
            (PointerEventType::Move, 32, Offset::new(20.0, 25.0)),
            (PointerEventType::Move, 132, Offset::new(30.0, 30.0)),
            (PointerEventType::Release, 132, Offset::new(30.0, 30.0)),
        ]
    );
    let message = expect_error(|| {
        ui.on_node_with_tag("box").perform_touch_input(|touch| touch.up());
    });
    assert!(message.contains("Cannot send UP event for pointer 0"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/CancelTest.kt
/// A cancel ends the gesture without an up: the node hears no release, and a new down works.
#[test]
fn a_cancelled_gesture_ends_without_an_up() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_touch_input(|touch| {
        touch.down(Offset::new(10.0, 10.0));
        touch.cancel();
        touch.down(Offset::new(12.0, 12.0));
        touch.up();
    });
    assert_eq!(
        kinds(&events),
        [PointerEventType::Press, PointerEventType::Press, PointerEventType::Release]
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/DoubleClickTest.kt, doubleClick
/// Two clicks halfway through the double-tap window apart ((40 + 300) / 2 = 170 ms), heard
/// as a double tap.
#[test]
fn a_double_click_is_two_clicks_heard_as_a_double_tap() {
    let doubles: Rc<RefCell<Vec<Offset>>> = Rc::default();
    let events: Events = Rc::default();
    let (seen, recorder) = (doubles.clone(), events.clone());
    let mut ui = compose_test(move || {
        let (seen, recorder) = (seen.clone(), recorder.clone());
        Box!(
            modifier = Modifier
                .size(SIZE.dp())
                .pointer_input((), move |scope| {
                    let seen = seen.clone();
                    async move {
                        scope
                            .detect_tap_gestures(
                                TapGestures::default()
                                    .on_double_tap(move |at| seen.borrow_mut().push(at)),
                            )
                            .await
                    }
                })
                .then_custom(RecorderElement(recorder))
                .test_tag("box")
        );
    });
    ui.on_node_with_tag("box").perform_touch_input(|touch| touch.double_click());
    assert_eq!(*doubles.borrow(), [Offset::new(50.0, 50.0)]);
    let recorded = events.borrow().clone();
    let t0 = recorded[0].millis;
    let times: Vec<u64> = recorded.iter().map(|event| event.millis - t0).collect();
    assert_eq!(times, [0, 16, 16, 186, 202, 202]);
    let message = expect_error(|| {
        ui.on_node_with_tag("box")
            .perform_touch_input(|touch| touch.double_click_at(Offset::ZERO, 10));
    });
    assert!(message.contains("must be at least 40ms"), "{message}");
}

/// A box detecting taps, recording long presses and double taps.
fn tapping_box(
    configuration: Option<compose_ui::ViewConfiguration>,
) -> (ComposeTestRule, Rc<RefCell<Vec<&'static str>>>) {
    let heard: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let seen = heard.clone();
    let mut host = compose_test::default_host();
    host.set_view_configuration(configuration);
    let ui = ComposeTestRule::with_host(host, move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier
                .size(SIZE.dp())
                .pointer_input((), move |scope| {
                    let (long, double) = (seen.clone(), seen.clone());
                    async move {
                        scope
                            .detect_tap_gestures(
                                TapGestures::default()
                                    .on_long_press(move |_| long.borrow_mut().push("long"))
                                    .on_double_tap(move |_| double.borrow_mut().push("double")),
                            )
                            .await
                    }
                })
                .test_tag("box")
        );
    });
    (ui, heard)
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/LongClickTest.kt, longClick (with its `TestViewConfiguration(longPressTimeoutMillis = 300)`)
// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/DoubleClickTest.kt, doubleClick (with its `TestViewConfiguration(doubleTapMinTimeMillis = 40, doubleTapTimeoutMillis = 200)`)
/// The host's view configuration sets the detectors' timeouts and the gestures' defaults
/// alike (D907): a 300 ms long press is held 400 ms; double taps 40 to 200 ms apart are
/// 120 ms apart by default, and one 250 ms apart is two taps.
#[test]
fn the_view_configuration_times_the_detectors_and_the_gestures() {
    let base = compose_ui::ViewConfiguration::at(compose_ui::Density::default());
    let configuration = compose_ui::ViewConfiguration {
        long_press_timeout_millis: 300,
        double_tap_min_time_millis: 40,
        double_tap_timeout_millis: 200,
        ..base
    };
    let (mut ui, heard) = tapping_box(Some(configuration));
    let mut timeouts = (0, 0);
    ui.on_node_with_tag("box").perform_touch_input(|touch| {
        timeouts = (
            touch.view_configuration().long_press_timeout_millis,
            touch.view_configuration().double_tap_timeout_millis,
        );
        let start = touch.current_position(0);
        assert_eq!(start, None);
        touch.long_click();
    });
    assert_eq!(timeouts, (300, 200));
    assert_eq!(*heard.borrow(), ["long"]);
    heard.borrow_mut().clear();
    ui.on_node_with_tag("box").perform_touch_input(|touch| touch.double_click());
    assert_eq!(*heard.borrow(), ["double"]);
    // Past the configured timeout: not a double tap (the default's 300 ms would make one).
    heard.borrow_mut().clear();
    ui.advance_time_by(std::time::Duration::from_secs(1));
    ui.on_node_with_tag("box").perform_touch_input(|touch| {
        touch.click();
        touch.advance_event_time(250);
        touch.click();
    });
    ui.advance_time_by(std::time::Duration::from_secs(1));
    assert!(heard.borrow().is_empty(), "{:?}", heard.borrow());
    let (mut default, heard) = tapping_box(None);
    default.on_node_with_tag("box").perform_touch_input(|touch| {
        touch.click();
        touch.advance_event_time(250);
        touch.click();
    });
    assert_eq!(*heard.borrow(), ["double"], "Android's 300 ms make it one");
}

/// Android's view configuration scales its distances by the density: at 2x, an 8 dp touch slop
/// is 16 px, the 2 dp handwriting slop 4 px, the 16 dp handwriting line margin 32 px, flings
/// 100 to 16,000 px a second.
#[test]
fn the_default_view_configuration_scales_with_the_density() {
    let configuration = compose_ui::ViewConfiguration::at(compose_ui::Density::new(2.0));
    assert_eq!(configuration.touch_slop, 16.0);
    assert_eq!(configuration.handwriting_slop, 4.0);
    assert_eq!(configuration.handwriting_gesture_line_margin, 32.0);
    assert_eq!(configuration.minimum_fling_velocity, 100.0);
    assert_eq!(configuration.maximum_fling_velocity, 16_000.0);
    assert_eq!(configuration.minimum_touch_target_size, compose_ui::DpSize::new(48.dp(), 48.dp()));
}

/// How far a vertical scroll goes after a swipe up ending at `velocity` px/s, under a view
/// configuration with flings at most `maximum` px/s (`None`: Android's 8,000 dp/s).
fn scrolled_after_a_fling(maximum: Option<f32>, velocity: f32) -> i32 {
    let state = ScrollState::new(0);
    let given = *state;
    let mut host = compose_test::default_host();
    host.set_view_configuration(maximum.map(|maximum| compose_ui::ViewConfiguration {
        maximum_fling_velocity: maximum,
        ..compose_ui::ViewConfiguration::default()
    }));
    let mut ui = ComposeTestRule::with_host(host, move || {
        Column!(
            modifier = Modifier.size((100.dp(), 700.dp())).vertical_scroll(given).test_tag("list"),
            {
                Box!(modifier = Modifier.size((100.dp(), 1_000_000.dp())));
            }
        );
    });
    ui.on_node_with_tag("list").perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(50.0, 650.0),
            Offset::new(50.0, 50.0),
            velocity,
            Some(400),
        );
    });
    ui.wait_for_idle();
    state.value()
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt, the drag's end velocity bounded by `LocalViewConfiguration.maximumFlingVelocity`
/// The view configuration's maximum fling velocity bounds a fling: past it, a faster swipe
/// goes no further (D907), while under Android's bound it does.
#[test]
fn the_maximum_fling_velocity_bounds_a_fling() {
    let (slower, faster) = (
        scrolled_after_a_fling(Some(1000.0), 3000.0),
        scrolled_after_a_fling(Some(1000.0), 6000.0),
    );
    assert_eq!(slower, faster, "both bounded to 1,000 px/s");
    let (slower, faster) =
        (scrolled_after_a_fling(None, 3000.0), scrolled_after_a_fling(None, 6000.0));
    assert!(faster > slower, "under the 8,000 dp/s bound: {slower} then {faster}");
    assert!(slower > scrolled_after_a_fling(Some(1000.0), 3000.0), "the bound slows it");
}

/// Content reads the host's view configuration as `LOCAL_VIEW_CONFIGURATION`, Android's at the
/// density by default (an 8 dp touch slop).
#[test]
fn content_reads_the_view_configuration() {
    let seen = Rc::new(std::cell::Cell::new(0.0f32));
    let read = seen.clone();
    let mut ui = compose_test_with(
        IntSize::new(100, 100),
        compose_ui::Density::new(2.0),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
        move || read.set(compose_ui::LOCAL_VIEW_CONFIGURATION.current().touch_slop),
    );
    assert_eq!(seen.get(), 16.0);
    let configuration = compose_ui::ViewConfiguration {
        touch_slop: 3.0,
        ..compose_ui::ViewConfiguration::at(compose_ui::Density::new(2.0))
    };
    ui.host().set_view_configuration(Some(configuration));
    ui.wait_for_idle();
    assert_eq!(seen.get(), 3.0);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/LongClickTest.kt, longClick
/// A long click holds 100 ms past the long-press timeout (400 + 100 ms), moving every event
/// period, heard as a long press.
#[test]
fn a_long_click_is_heard_as_a_long_press() {
    let longs: Rc<RefCell<Vec<Offset>>> = Rc::default();
    let events: Events = Rc::default();
    let (seen, recorder) = (longs.clone(), events.clone());
    let mut ui = compose_test(move || {
        let (seen, recorder) = (seen.clone(), recorder.clone());
        Box!(
            modifier = Modifier
                .size(SIZE.dp())
                .pointer_input((), move |scope| {
                    let seen = seen.clone();
                    async move {
                        scope
                            .detect_tap_gestures(
                                TapGestures::default()
                                    .on_long_press(move |at| seen.borrow_mut().push(at)),
                            )
                            .await
                    }
                })
                .then_custom(RecorderElement(recorder))
                .test_tag("box")
        );
    });
    ui.on_node_with_tag("box")
        .perform_touch_input(|touch| touch.long_click_at(Offset::new(10.0, 10.0), 700));
    assert_eq!(*longs.borrow(), [Offset::new(10.0, 10.0)]);
    let recorded = events.borrow().clone();
    let (first, last) = (recorded[0], recorded[recorded.len() - 1]);
    assert_eq!((first.kind, last.kind), (PointerEventType::Press, PointerEventType::Release));
    assert_eq!(last.millis - first.millis, 700);
    assert!(recorded.iter().all(|event| event.position == Offset::new(10.0, 10.0)));
    expect_error(|| {
        ui.on_node_with_tag("box")
            .perform_touch_input(|touch| touch.long_click_at(Offset::ZERO, 300));
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/SwipeDirectionTest.kt, swipeUp, swipeDown, swipeLeft, swipeRight
/// The swipes cross the node's middle edge to edge in 200 ms, a move about every 16 ms.
#[test]
fn swipes_cross_the_node_edge_to_edge() {
    type Swipe = fn(&mut TouchInjectionScope<'_>);
    let cases: [(&str, Swipe, Offset, Offset); 4] = [
        ("up", |touch| touch.swipe_up(), Offset::new(50.0, 99.0), Offset::new(50.0, 0.0)),
        ("down", |touch| touch.swipe_down(), Offset::new(50.0, 0.0), Offset::new(50.0, 99.0)),
        ("left", |touch| touch.swipe_left(), Offset::new(99.0, 50.0), Offset::new(0.0, 50.0)),
        ("right", |touch| touch.swipe_right(), Offset::new(0.0, 50.0), Offset::new(99.0, 50.0)),
    ];
    for (name, swipe, start, end) in cases {
        let (mut ui, events) = recording_box(Offset::ZERO);
        ui.on_node_with_tag("box").perform_touch_input(swipe);
        let recorded = events.borrow().clone();
        let (first, last) = (recorded[0], recorded[recorded.len() - 1]);
        assert_eq!((first.kind, first.position), (PointerEventType::Press, start), "{name}");
        assert_eq!((last.kind, last.position), (PointerEventType::Release, end), "{name}");
        assert_eq!(last.millis - first.millis, 200, "{name}");
        // 200 ms in steps as near 16 ms as can be: 13 moves.
        assert_eq!(recorded.len(), 15, "{name}: {recorded:?}");
        let distances: Vec<f32> = recorded
            .iter()
            .map(|event| (event.position.x - start.x).abs() + (event.position.y - start.y).abs())
            .collect();
        assert!(distances.windows(2).all(|pair| pair[0] <= pair[1]), "{name}: {distances:?}");
    }
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/SwipeWithVelocityTest.kt (its path: still, then the last stretch)
/// A swipe ending at a velocity waits still at the start, then covers the distance in the
/// last stretch, ending at the end at its duration.
#[test]
fn a_swipe_with_velocity_ends_at_its_end_at_its_duration() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_touch_input(|touch| {
        touch.swipe_with_velocity(
            Offset::new(10.0, 50.0),
            Offset::new(90.0, 50.0),
            1000.0,
            Some(300),
        );
    });
    let recorded = events.borrow().clone();
    let (first, last) = (recorded[0], recorded[recorded.len() - 1]);
    assert_eq!(last.millis - first.millis, 300);
    assert_eq!(last.position, Offset::new(90.0, 50.0));
    // The last two moves 16 ms apart cover about 16 px (1000 px/s).
    let moves: Vec<&Recorded> =
        recorded.iter().filter(|e| e.kind == PointerEventType::Move).collect();
    let (a, b) = (moves[moves.len() - 2], moves[moves.len() - 1]);
    let speed = (b.position.x - a.position.x) / (b.millis - a.millis) as f32 * 1000.0;
    assert!((speed - 1000.0).abs() < 150.0, "{speed} px/s");
    assert_eq!(moves[0].position, Offset::new(10.0, 50.0), "still at first");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/PinchTest.kt
/// A pinch moves two fingers together, from their starts to their ends.
#[test]
fn a_pinch_moves_two_fingers() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_touch_input(|touch| {
        touch.pinch(
            Offset::new(40.0, 50.0),
            Offset::new(10.0, 50.0),
            Offset::new(60.0, 50.0),
            Offset::new(90.0, 50.0),
            400,
        );
    });
    let recorded = events.borrow().clone();
    let pointers: Vec<PointerId> = {
        let mut seen = Vec::new();
        for event in &recorded {
            if !seen.contains(&event.pointer) {
                seen.push(event.pointer);
            }
        }
        seen
    };
    assert_eq!(pointers.len(), 2);
    let last =
        |pointer: PointerId| *recorded.iter().rev().find(|event| event.pointer == pointer).unwrap();
    assert_eq!(last(pointers[0]).position, Offset::new(10.0, 50.0));
    assert_eq!(last(pointers[1]).position, Offset::new(90.0, 50.0));
    assert!(recorded.iter().filter(|e| e.kind == PointerEventType::Release).count() == 2);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/touch/SynchronizedWithMainClockTest.kt, zeroTimeBetween_performTouchInput, someTimeBetween_performTouchInput
/// Events are on the main clock: two gestures in a row are as far apart as the clock moved
/// between them.
#[test]
fn events_follow_the_main_clock() {
    for (between, expected) in [(None, 0), (Some(1273), 1273)] {
        let (mut ui, events) = recording_box(Offset::ZERO);
        ui.on_node_with_tag("box").perform_touch_input(|touch| touch.click());
        if let Some(millis) = between {
            ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(millis));
        }
        ui.on_node_with_tag("box").perform_touch_input(|touch| touch.click());
        let recorded = events.borrow().clone();
        assert_eq!(recorded.len(), 6);
        assert_eq!(recorded[3].millis - recorded[2].millis, expected);
    }
}

/// Positions are in the node's coordinates: its visible bounds' top left is (0, 0); the
/// geometry helpers follow its size.
#[test]
fn positions_are_relative_to_the_node() {
    let (mut ui, events) = recording_box(Offset::new(30.0, 40.0));
    let mut geometry = (0, 0, Offset::ZERO, Offset::ZERO, Offset::ZERO);
    ui.on_node_with_tag("box").perform_touch_input(|touch| {
        geometry = (
            touch.width(),
            touch.height(),
            touch.center(),
            touch.bottom_right(),
            touch.percent_offset(0.25, 0.5),
        );
        touch.click_at(touch.top_left());
    });
    assert_eq!(
        geometry,
        (100, 100, Offset::new(50.0, 50.0), Offset::new(99.0, 99.0), Offset::new(25.0, 50.0))
    );
    assert_eq!(events.borrow()[0].position, Offset::ZERO, "the node's own top left");
}

// ---- Mouse -------------------------------------------------------------------------------

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt
/// A mouse click presses and releases the primary button 60 ms apart, as a mouse pointer.
#[test]
fn a_mouse_click_presses_and_releases() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box")
        .perform_mouse_input(|mouse| mouse.click_at(Offset::new(10.0, 20.0), MouseButton::Primary));
    let recorded = events.borrow().clone();
    let press = recorded.iter().find(|e| e.kind == PointerEventType::Press).unwrap();
    let release = recorded.iter().find(|e| e.kind == PointerEventType::Release).unwrap();
    assert_eq!(press.position, Offset::new(10.0, 20.0));
    assert_eq!(release.millis - press.millis, 60);
    assert!(recorded.iter().all(|event| event.pointer_type == PointerType::Mouse));
    let message = expect_error(|| {
        ui.on_node_with_tag("box").perform_mouse_input(|mouse| mouse.right_click(Offset::ZERO));
    });
    assert!(message.contains("primary button only"), "{message}");
}

// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/InputDispatcher.kt, enqueueMouseMove (an exit, then the move)
/// A move out of the window sends the exit, then the move: a drag pressed inside keeps
/// hearing its moves out there.
#[test]
fn a_mouse_moving_out_of_the_window_exits_then_moves() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_mouse_input(|mouse| {
        mouse.move_to(Offset::new(10.0, 10.0));
        mouse.press(MouseButton::Primary);
        mouse.move_to(Offset::new(-50.0, -50.0));
        mouse.release(MouseButton::Primary);
    });
    let recorded = events.borrow().clone();
    let moved_out = recorded
        .iter()
        .find(|event| event.kind == PointerEventType::Move && event.position.x < 0.0)
        .unwrap_or_else(|| panic!("the drag's move out of the window: {recorded:?}"));
    assert!(moved_out.pressed);
    assert_eq!(recorded.last().unwrap().kind, PointerEventType::Release);
}

// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/InputDispatcher.kt, enqueueMousePress and enqueueMouseMove (exitHoverOnPress)
/// A press ends the hover: dragged out of the window and back while pressed, the cursor
/// neither exits nor enters it (the window's exit would be an unpressed `Exit` among the
/// moves), and the release out there leaves it not hovering, so a later exit is refused. The
/// box's own exit comes with the release out of it, the tree's (Compose's box hears a pressed
/// `Exit` and `Enter` as the drag crosses its edge).
#[test]
fn a_press_ends_the_hover() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_mouse_input(|mouse| {
        mouse.move_to(Offset::new(10.0, 10.0));
        mouse.press(MouseButton::Primary);
        mouse.move_to(Offset::new(-50.0, -50.0));
        mouse.move_to(Offset::new(20.0, 20.0));
        mouse.move_to(Offset::new(-50.0, -50.0));
        mouse.release(MouseButton::Primary);
    });
    let recorded = events.borrow().clone();
    let press = recorded.iter().position(|e| e.kind == PointerEventType::Press).unwrap();
    let after: Vec<(PointerEventType, bool)> =
        recorded[press + 1..].iter().map(|event| (event.kind, event.pressed)).collect();
    assert_eq!(
        after,
        [
            (PointerEventType::Move, true),
            (PointerEventType::Move, true),
            (PointerEventType::Move, true),
            (PointerEventType::Exit, false),
            (PointerEventType::Release, false),
        ],
        "only pressed moves while pressed: {recorded:?}"
    );
    let message = expect_error(|| {
        ui.on_node_with_tag("box").perform_mouse_input(|mouse| mouse.exit(Offset::ZERO));
    });
    assert!(message.contains("mouse is not hovering"), "{message}");
}

// Compose: compose/ui/ui-test/src/commonMain/kotlin/androidx/compose/ui/test/InputDispatcher.kt, enqueueMouseRelease (the hover enter and move after the last button's release)
// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ClickTest.kt, click_pressIn_moveInside_releaseIn
/// A release inside the window hovers again at once: the node hears an unpressed move where
/// the cursor is after the release, and the cursor can exit without moving first.
#[test]
fn a_release_inside_the_window_hovers_again() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_mouse_input(|mouse| {
        mouse.move_to(Offset::new(10.0, 10.0));
        mouse.press(MouseButton::Primary);
        mouse.move_to(Offset::new(30.0, 30.0));
        mouse.release(MouseButton::Primary);
    });
    let recorded = events.borrow().clone();
    let press = recorded.iter().position(|e| e.kind == PointerEventType::Press).unwrap();
    let after: Vec<(PointerEventType, bool)> =
        recorded[press..].iter().map(|event| (event.kind, event.pressed)).collect();
    assert_eq!(
        after,
        [
            (PointerEventType::Press, true),
            (PointerEventType::Move, true),
            (PointerEventType::Release, false),
            (PointerEventType::Move, false),
        ]
    );
    let (release, hover) = (&recorded[press + 2], &recorded[press + 3]);
    assert_eq!(hover.position, Offset::new(30.0, 30.0));
    assert_eq!(hover.millis, release.millis, "at the release's time");
    ui.on_node_with_tag("box").perform_mouse_input(|mouse| mouse.exit(Offset::new(30.0, 30.0)));
    assert_eq!(events.borrow().last().unwrap().kind, PointerEventType::Exit);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/MoveTest.kt
/// Moves hover: the node hears the mouse come in and move; an exit sends it away.
#[test]
fn a_mouse_moves_in_and_out() {
    let (mut ui, events) = recording_box(Offset::ZERO);
    ui.on_node_with_tag("box").perform_mouse_input(|mouse| {
        mouse.move_to(Offset::new(10.0, 10.0));
        mouse.move_by(Offset::new(5.0, 0.0));
        assert_eq!(mouse.current_position(), Offset::new(15.0, 10.0));
        mouse.exit(Offset::new(15.0, 10.0));
    });
    let recorded = events.borrow().clone();
    let summary: Vec<PointerEventType> = recorded.iter().map(|e| e.kind).collect();
    assert_eq!(
        summary,
        [
            PointerEventType::Enter,
            PointerEventType::Move,
            PointerEventType::Move,
            PointerEventType::Exit
        ]
    );
    assert_eq!(recorded[2].millis - recorded[1].millis, 16);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/mouse/ScrollTest.kt
/// A wheel's unit scrolls 64 dp, forward positive.
#[test]
fn a_mouse_wheel_scrolls_64_dp_a_unit() {
    let state = ScrollState::new(0);
    let given = *state;
    let mut ui = compose_test(move || {
        Column!(modifier = Modifier.size(100.dp()).vertical_scroll(given).test_tag("list"), {
            Box!(modifier = Modifier.size((100.dp(), 1000.dp())));
        });
    });
    ui.on_node_with_tag("list").perform_mouse_input(|mouse| {
        mouse.update_pointer_to(Offset::new(50.0, 50.0));
        mouse.scroll(1.0, ScrollWheel::Vertical);
    });
    assert_eq!(state.value(), 64);
    ui.on_node_with_tag("list")
        .perform_mouse_input(|mouse| mouse.smooth_scroll(-0.5, 160, ScrollWheel::Vertical));
    assert_eq!(state.value(), 32);
}

// ---- Keys --------------------------------------------------------------------------------

/// A focused box recording its key events.
type Keys = Rc<RefCell<Vec<(KeyEvent, u64)>>>;

fn recording_keys() -> (ComposeTestRule, Keys) {
    let keys: Keys = Rc::default();
    let recorder = keys.clone();
    let mut ui = compose_test(move || {
        let recorder = recorder.clone();
        Box!(
            modifier = Modifier
                .size(10.dp())
                .on_key_event(move |event: KeyEvent| {
                    let millis = compose_runtime::platform::now_nanos() / 1_000_000;
                    recorder.borrow_mut().push((event, millis));
                    false
                })
                .focusable()
                .test_tag("keys")
        );
    });
    ui.on_node_with_tag("keys").request_focus();
    (ui, keys)
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/key/KeyPressTest.kt, KeyDownTest.kt, KeyUpTest.kt
/// A key press is a down and an up 50 ms later; a key down twice, or up while up, fails.
#[test]
fn a_key_press_is_a_down_and_an_up() {
    let (mut ui, keys) = recording_keys();
    ui.on_node_with_tag("keys").perform_key_input(|keys| keys.press_key(Key::Enter));
    let recorded = keys.borrow().clone();
    assert_eq!(recorded.len(), 2);
    assert_eq!(
        (recorded[0].0.kind, recorded[1].0.kind),
        (KeyEventType::KeyDown, KeyEventType::KeyUp)
    );
    assert_eq!(recorded[1].1 - recorded[0].1, 50);
    let message = expect_error(|| {
        ui.on_node_with_tag("keys").perform_key_input(|keys| keys.key_up(Key::Enter));
    });
    assert!(message.contains("is not pressed down"), "{message}");
    let message = expect_error(|| {
        ui.on_node_with_tag("keys").perform_key_input(|keys| {
            keys.key_down(Key::Tab);
            keys.key_down(Key::Tab);
        });
    });
    assert!(message.contains("is already pressed down"), "{message}");
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/key/MetaKeysTest.kt
/// A modifier key held marks the events sent meanwhile.
#[test]
fn modifier_keys_held_mark_the_events() {
    let (mut ui, keys) = recording_keys();
    ui.on_node_with_tag("keys").perform_key_input(|keys| {
        keys.with_key_down(Key::ShiftLeft, |keys| {
            assert!(keys.is_shift_down());
            keys.with_key_down(Key::CtrlRight, |keys| keys.press_key(Key::Character('a')));
        });
        assert!(!keys.is_shift_down() && !keys.is_ctrl_down());
        keys.press_key(Key::Character('b'));
    });
    let recorded = keys.borrow().clone();
    let a = recorded.iter().find(|(event, _)| event.key == Key::Character('a')).unwrap().0;
    assert!(a.modifiers.shift && a.modifiers.ctrl && !a.modifiers.alt, "{a:?}");
    let b = recorded.iter().find(|(event, _)| event.key == Key::Character('b')).unwrap().0;
    assert!(!b.modifiers.shift && !b.modifiers.ctrl, "{b:?}");
}

/// A key held past 500 ms repeats, then every 50 ms, as Compose's key injection.
#[test]
fn a_held_key_repeats() {
    // Held 620 ms; then exactly as long as a repeat's time (600, 650 ms): the repeat due at
    // the release still comes, before it, as Compose's `sendRepeatKeysIfNeeded`.
    for (held, expected) in [
        (620, &[0, 500, 550, 600][..]),
        (600, &[0, 500, 550, 600]),
        (650, &[0, 500, 550, 600, 650]),
    ] {
        let (mut ui, keys) = recording_keys();
        ui.on_node_with_tag("keys")
            .perform_key_input(|keys| keys.press_key_for(Key::Backspace, held));
        let recorded = keys.borrow().clone();
        let t0 = recorded[0].1;
        let downs: Vec<u64> = recorded
            .iter()
            .filter(|(event, _)| event.kind == KeyEventType::KeyDown)
            .map(|(_, millis)| millis - t0)
            .collect();
        assert_eq!(downs, expected, "held {held} ms");
        let last = recorded.last().unwrap();
        assert_eq!((last.0.kind, last.1 - t0), (KeyEventType::KeyUp, held), "held {held} ms");
    }
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/injectionscope/key/LockKeysTest.kt
/// A lock key is on from its first press to its second press's release.
#[test]
fn lock_keys_toggle() {
    let (mut ui, _) = recording_keys();
    ui.on_node_with_tag("keys").perform_key_input(|keys| {
        assert!(!keys.is_caps_lock_on());
        keys.press_key(Key::CapsLock);
        assert!(keys.is_caps_lock_on());
        keys.key_down(Key::CapsLock);
        assert!(keys.is_caps_lock_on());
        keys.key_up(Key::CapsLock);
        assert!(!keys.is_caps_lock_on());
        keys.with_key_toggled(Key::NumLock, |keys| assert!(keys.is_num_lock_on()));
        assert!(!keys.is_num_lock_on());
    });
}
