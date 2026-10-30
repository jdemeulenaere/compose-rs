//! `Modifier.draggable`: scenarios of Compose's `DraggableTest`, rewritten.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    IntSize, MutableInteractionSource, Offset, PointerEventPass, PointerEventType, UnitExt,
    ViewConfiguration,
};

use crate::interaction_helpers::{kinds, record};
use crate::scroll_support::run;

const TAG: &str = "drag";
const CENTER: Offset = Offset::new(50.0, 50.0);

fn counter() -> (Rc<Cell<f32>>, impl Fn(f32) + Clone + 'static) {
    let total = Rc::new(Cell::new(0.0f32));
    let adding = total.clone();
    (total, move |delta: f32| adding.set(adding.get() + delta))
}

/// A 100 x 100 box at the origin of a 300 x 300 window, given the modifier `factory` composes.
fn drag_box(factory: impl Fn() -> Modifier + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let draggable = factory();
        Box!(modifier = Modifier.size(100.dp()).test_tag(TAG).then(draggable));
    });
    ui.wait_for_idle();
    ui
}

/// A box with a state over `on_delta` and `options`, recomposed with each given one's reads.
fn dragging(
    orientation: Orientation,
    on_delta: impl Fn(f32) + Clone + 'static,
    options: impl Fn() -> DraggableOptions + 'static,
) -> ComposeTestRule {
    drag_box(move || {
        let on_delta = on_delta.clone();
        let state = remember_draggable_state!(move |delta| on_delta(delta));
        Modifier.draggable_with(state, orientation, options())
    })
}

fn swipe(ui: &mut ComposeTestRule, by: Offset, millis: u64) {
    ui.on_root().perform_touch_input(|touch| touch.swipe(CENTER, CENTER + by, millis));
    ui.wait_for_idle();
}

fn down_and_move(ui: &mut ComposeTestRule, by: Offset) {
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(by);
    });
    ui.wait_for_idle();
}

fn stops(count: &Rc<Cell<u32>>) -> Option<OnDragStopped> {
    let count = count.clone();
    Some(Rc::new(move |_, _| count.set(count.get() + 1)))
}

fn starts(count: &Rc<Cell<u32>>) -> Option<OnDragStarted> {
    let count = count.clone();
    Some(Rc::new(move |_, _| count.set(count.get() + 1)))
}

fn with_slop(ui: &mut ComposeTestRule, touch_slop: f32) {
    ui.host().set_view_configuration(Some(ViewConfiguration {
        touch_slop,
        ..ViewConfiguration::default()
    }));
    ui.advance_time_by_frame();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_horizontalDrag, draggable_verticalDrag
#[test]
fn a_drag_along_the_axis_moves_it_and_across_it_does_not() {
    for (orientation, along, across) in [
        (Orientation::Horizontal, Offset::new(100.0, 0.0), Offset::new(0.0, 100.0)),
        (Orientation::Vertical, Offset::new(0.0, 100.0), Offset::new(100.0, 0.0)),
    ] {
        let (total, add) = counter();
        let mut ui = dragging(orientation, add, DraggableOptions::default);
        swipe(&mut ui, along, 100);
        let after_along = total.get();
        assert!(after_along > 0.0);
        swipe(&mut ui, across, 100);
        assert_eq!(total.get(), after_along);
        swipe(&mut ui, Offset::ZERO - along, 100);
        assert!(total.get() < 0.01);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_startStop
#[test]
fn a_drag_starts_and_stops_once() {
    let (started, stopped) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (on_start, on_stop) = (starts(&started), stops(&stopped));
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            on_drag_started: on_start.clone(),
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    assert_eq!((started.get(), stopped.get()), (0, 0));
    swipe(&mut ui, Offset::new(100.0, 0.0), 100);
    assert_eq!((started.get(), stopped.get()), (1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_disabledWontCallLambda
#[test]
fn disabled_it_moves_nothing() {
    let (total, add) = counter();
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let mut ui = dragging(Orientation::Horizontal, add, move || DraggableOptions {
        enabled: read.get(),
        ..DraggableOptions::default()
    });
    swipe(&mut ui, Offset::new(100.0, 0.0), 100);
    let before = total.get();
    assert!(before > 0.0);
    enabled.set(false);
    ui.wait_for_idle();
    swipe(&mut ui, Offset::new(100.0, 0.0), 100);
    assert_eq!(total.get(), before);
}

fn stopped_velocity() -> (Rc<Cell<f32>>, Option<OnDragStopped>) {
    let velocity = Rc::new(Cell::new(f32::NAN));
    let told = velocity.clone();
    (velocity, Some(Rc::new(move |_, at| told.set(at))))
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_velocityProxy
#[test]
fn a_release_tells_the_velocity() {
    let (velocity, on_stop) = stopped_velocity();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(CENTER, CENTER + Offset::new(100.0, 0.0), 112.0, Some(100));
    });
    ui.wait_for_idle();
    assert!((velocity.get() - 112.0).abs() < 0.1, "{}", velocity.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_velocityProxy_multiTouch
#[test]
fn the_last_pointer_up_tells_its_own_velocity() {
    let (velocity, on_stop) = stopped_velocity();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    // Each at its own speed; the second lifts last.
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(0, CENTER);
        touch.down_pointer(1, CENTER);
        for step in 1..=20 {
            let at = step as f32;
            touch.update_pointer_to(0, CENTER + Offset::new(at, 0.0));
            touch.update_pointer_to(1, CENTER + Offset::new(at * 3.0, 0.0));
            touch.move_pointers_after(10);
        }
        touch.up_pointer(0);
        touch.up_pointer(1);
    });
    ui.wait_for_idle();
    // 3 px per 10 ms.
    assert!((velocity.get() - 300.0).abs() < 1.0, "{}", velocity.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_startWithoutSlop_ifAnimating
#[test]
fn starting_immediately_there_is_no_slop() {
    let (total, add) = counter();
    let mut ui = dragging(Orientation::Horizontal, add, || DraggableOptions {
        start_drag_immediately: true,
        ..DraggableOptions::default()
    });
    swipe(&mut ui, Offset::new(100.0, 0.0), 100);
    assert_eq!(total.get(), 100.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_cancel_callsDragStop
#[test]
fn a_cancelled_touch_stops_the_drag() {
    let (total, add) = counter();
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let mut ui = dragging(Orientation::Horizontal, add, move || DraggableOptions {
        start_drag_immediately: true,
        on_drag_stopped: on_stop.clone(),
        ..DraggableOptions::default()
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 100.0));
        touch.cancel();
    });
    ui.wait_for_idle();
    assert!(total.get() > 0.0);
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_immediateStart_callsStopWithoutSlop
#[test]
fn a_mouse_press_starts_it_at_once_and_its_release_stops_it() {
    let (started, stopped) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (on_start, on_stop) = (starts(&started), stops(&stopped));
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            start_drag_immediately: true,
            on_drag_started: on_start.clone(),
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    ui.on_root().perform_mouse_input(|mouse| {
        mouse.move_to(CENTER);
        mouse.press(MouseButton::Primary);
    });
    ui.wait_for_idle();
    assert_eq!(started.get(), 1);
    ui.on_root().perform_mouse_input(|mouse| mouse.release(MouseButton::Primary));
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_callsDragStop_whenNewState
#[test]
fn a_new_state_stops_the_drag() {
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let states = [DraggableState::new(|_| {}), DraggableState::new(|_| {})];
    let state = mutable_state_of(0usize);
    let read = state.handle();
    let mut ui = drag_box(move || {
        Modifier.draggable_with(
            states[read.get()].clone(),
            Orientation::Horizontal,
            DraggableOptions { on_drag_stopped: on_stop.clone(), ..DraggableOptions::default() },
        )
    });
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(stopped.get(), 0);
    state.set(1);
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_callsDragStop_whenNewOrientation
#[test]
fn a_new_orientation_stops_the_drag() {
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let orientation = mutable_state_of(Orientation::Horizontal);
    let read = orientation.handle();
    let mut ui = drag_box(move || {
        let state = remember_draggable_state!(|_| {});
        Modifier.draggable_with(
            state,
            read.get(),
            DraggableOptions { on_drag_stopped: on_stop.clone(), ..DraggableOptions::default() },
        )
    });
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(stopped.get(), 0);
    orientation.set(Orientation::Vertical);
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_callsDragStop_whenDisabled
#[test]
fn disabling_it_stops_the_drag() {
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            enabled: read.get(),
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(stopped.get(), 0);
    enabled.set(false);
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_callsDragStop_whenNewReverseDirection
#[test]
fn reversing_it_stops_the_drag() {
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let reverse = mutable_state_of(false);
    let read = reverse.handle();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            reverse_direction: read.get(),
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(stopped.get(), 0);
    reverse.set(true);
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_updates_startDragImmediately
#[test]
fn starting_immediately_can_change() {
    let (total, add) = counter();
    let immediately = mutable_state_of(false);
    let read = immediately.handle();
    let mut ui = dragging(Orientation::Horizontal, add, move || DraggableOptions {
        start_drag_immediately: read.get(),
        ..DraggableOptions::default()
    });
    let half_a_slop = ViewConfiguration::default().touch_slop / 2.0;
    let move_a_bit = |ui: &mut ComposeTestRule| {
        ui.on_root().perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(Offset::new(half_a_slop, 0.0));
            touch.up();
        });
        ui.wait_for_idle();
    };
    move_a_bit(&mut ui);
    assert_eq!(total.get(), 0.0);
    immediately.set(true);
    ui.wait_for_idle();
    move_a_bit(&mut ui);
    assert_eq!(total.get(), half_a_slop);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_updates_onDragStarted
#[test]
fn the_newest_start_callback_is_called() {
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let callback = mutable_state_of(0usize);
    let read = callback.handle();
    let (on_first, on_second) = (starts(&first), starts(&second));
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            on_drag_started: if read.get() == 0 { on_first.clone() } else { on_second.clone() },
            ..DraggableOptions::default()
        },
    );
    let drag = |ui: &mut ComposeTestRule| {
        down_and_move(ui, Offset::new(100.0, 100.0));
        ui.on_root().perform_touch_input(|touch| touch.up());
        ui.wait_for_idle();
    };
    drag(&mut ui);
    assert_eq!((first.get(), second.get()), (1, 0));
    callback.set(1);
    ui.wait_for_idle();
    drag(&mut ui);
    assert_eq!((first.get(), second.get()), (1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_updates_onDragStopped
#[test]
fn the_newest_stop_callback_is_called_even_for_a_drag_going_on() {
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let callback = mutable_state_of(0usize);
    let read = callback.handle();
    let (on_first, on_second) = (stops(&first), stops(&second));
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            on_drag_stopped: if read.get() == 0 { on_first.clone() } else { on_second.clone() },
            ..DraggableOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    callback.set(1);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get()), (0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_resumesNormally_whenInterruptedWithHigherPriority
#[test]
fn a_drag_of_a_higher_priority_stops_it_and_a_later_one_works() {
    let (total, add) = counter();
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let state = DraggableState::new(add);
    let given = state.clone();
    let mut ui = drag_box(move || {
        Modifier.draggable_with(
            given.clone(),
            Orientation::Horizontal,
            DraggableOptions { on_drag_stopped: on_stop.clone(), ..DraggableOptions::default() },
        )
    });
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(stopped.get(), 0);
    let before = total.get();
    assert!(before > 0.0);
    let taking = state.clone();
    run(&mut ui, async move {
        taking
            .drag(MutatePriority::PreventUserInput, |scope| async move { scope.drag_by(123.0) })
            .await
            .expect("the highest priority");
    });
    assert_eq!(total.get(), before + 123.0);
    assert_eq!(stopped.get(), 1);
    ui.on_root().perform_touch_input(|touch| {
        touch.up();
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 100.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert!(total.get() > before + 123.0);
}

/// An outer box of `outer` around an inner one of `inner`, both filling 300 x 300.
fn nested(
    outer: impl Fn() -> Modifier + 'static,
    inner: impl Fn() -> Modifier + Clone + 'static,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let inner = inner.clone();
        Box!(modifier = Modifier.size(300.dp()).then(outer()), {
            Box!(modifier = Modifier.size(300.dp()).then(inner()));
        });
    });
    ui.wait_for_idle();
    ui
}

fn drag_of(
    orientation: Orientation,
    on_delta: impl Fn(f32) + Clone + 'static,
    immediately: bool,
) -> impl Fn() -> Modifier + Clone + 'static {
    move || {
        let on_delta = on_delta.clone();
        let state = remember_draggable_state!(move |delta| on_delta(delta));
        Modifier.draggable_with(
            state,
            orientation,
            DraggableOptions { start_drag_immediately: immediately, ..DraggableOptions::default() },
        )
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_noNestedDrag
#[test]
fn the_inner_of_two_takes_the_drag_and_the_outer_gets_nothing() {
    let (outer, add_outer) = counter();
    let (inner, add_inner) = counter();
    let mut ui = nested(
        drag_of(Orientation::Horizontal, add_outer, false),
        drag_of(Orientation::Horizontal, add_inner, false),
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(50.0, 150.0), Offset::new(250.0, 150.0), 300);
    });
    ui.wait_for_idle();
    assert!(inner.get() > 0.0);
    assert_eq!(outer.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_interactionSource
#[test]
fn a_drag_starts_and_stops_on_the_source() {
    let source = MutableInteractionSource::new();
    let recorded = record(&source.as_source());
    let given = source.clone();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            interaction_source: Some(given.clone()),
            ..DraggableOptions::default()
        },
    );
    assert!(recorded.borrow().is_empty());
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(25.0, 50.0));
        touch.move_by(Offset::new(50.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Start"]);
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Start", "Stop"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_interactionSource_resetWhenDisposed
#[test]
fn a_drag_cancels_on_the_source_when_disposed() {
    let source = MutableInteractionSource::new();
    let recorded = record(&source.as_source());
    let shown = mutable_state_of(true);
    let read = shown.handle();
    let given = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        if read.get() {
            let state = remember_draggable_state!(|_| {});
            Box!(
                modifier = Modifier.size(100.dp()).draggable_with(
                    state,
                    Orientation::Horizontal,
                    DraggableOptions {
                        interaction_source: Some(given.clone()),
                        ..DraggableOptions::default()
                    }
                )
            );
        }
    });
    down_and_move(&mut ui, Offset::new(50.0, 0.0));
    assert_eq!(kinds(&recorded), ["Start"]);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Start", "DragCancel"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_interactionSource_resetWhenEnabledChanged
#[test]
fn a_drag_cancels_on_the_source_when_disabled() {
    let source = MutableInteractionSource::new();
    let recorded = record(&source.as_source());
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let given = source.clone();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            enabled: read.get(),
            interaction_source: Some(given.clone()),
            ..DraggableOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(50.0, 0.0));
    assert_eq!(kinds(&recorded), ["Start"]);
    enabled.set(false);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Start", "DragCancel"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_velocityIsLimitedByViewConfiguration
#[test]
fn the_velocity_is_limited_by_the_view_configuration() {
    let (velocity, on_stop) = stopped_velocity();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    ui.host().set_view_configuration(Some(ViewConfiguration {
        maximum_fling_velocity: 1000.0,
        ..ViewConfiguration::default()
    }));
    ui.advance_time_by_frame();
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(Offset::new(0.0, 50.0), Offset::new(100.0, 50.0), 2000.0, None);
    });
    ui.wait_for_idle();
    assert_eq!(velocity.get(), 1000.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_interactionSource_resetWhenInteractionSourceChanged
#[test]
fn a_drag_cancels_on_the_old_source_and_starts_nothing_on_the_new() {
    let (one, two) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let (recorded_one, recorded_two) = (record(&one.as_source()), record(&two.as_source()));
    let current = mutable_state_of(one);
    let read = current.handle();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            interaction_source: Some(read.get()),
            ..DraggableOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(50.0, 0.0));
    assert_eq!(kinds(&recorded_one), ["Start"]);
    current.set(two);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded_one), ["Start", "DragCancel"]);
    assert!(recorded_two.borrow().is_empty());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_cancelMidDown_shouldContinueWithNextDown
#[test]
fn a_touch_cancelled_at_its_press_leaves_the_next_to_drag() {
    let (total, add) = counter();
    let mut ui = dragging(Orientation::Horizontal, add, DraggableOptions::default);
    ui.on_root().perform_mouse_input(|mouse| {
        mouse.enter(CENTER);
        mouse.exit(CENTER);
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.cancel();
    });
    assert_eq!(total.get(), 0.0);
    ui.on_root().perform_mouse_input(|mouse| {
        mouse.enter(CENTER);
        mouse.exit(CENTER);
    });
    assert_eq!(total.get(), 0.0);
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert!(total.get() > 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_noMomentumDragging_onDragStopped_shouldGenerateZeroVelocity
#[test]
fn a_pointer_that_stopped_before_its_release_has_no_velocity() {
    let (velocity, on_stop) = stopped_velocity();
    let mut ui = dragging(
        Orientation::Vertical,
        |_| {},
        move || DraggableOptions {
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        for _ in 0..30 {
            touch.move_by(Offset::new(0.0, -10.0));
        }
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(velocity.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, onDragStopped_inputChanged_shouldNotCancelScope
#[test]
fn what_the_stop_callback_launched_outlives_new_inputs() {
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let job: Rc<Cell<Option<Job>>> = Rc::default();
    let kept = job.clone();
    let on_stop: Option<OnDragStopped> = Some(Rc::new(move |scope: CoroutineScope, _| {
        kept.set(Some(scope.launch(async {
            compose_runtime::delay(Duration::from_secs(10)).await;
        })));
    }));
    let mut ui = dragging(
        Orientation::Vertical,
        |_| {},
        move || DraggableOptions {
            enabled: read.get(),
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    ui.set_auto_advance(false);
    swipe(&mut ui, Offset::new(0.0, 40.0), 100);
    enabled.set(false);
    ui.advance_time_by_frame();
    assert!(job.get().expect("the stop ran").is_active());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, onDragStarted_startDragImmediately_offsetShouldBePositionOfDownEvent
#[test]
fn starting_immediately_the_start_is_the_press() {
    let at = Rc::new(Cell::new(Offset::ZERO));
    let told = at.clone();
    let mut ui = dragging(
        Orientation::Vertical,
        |_| {},
        move || {
            let told = told.clone();
            DraggableOptions {
                start_drag_immediately: true,
                on_drag_started: Some(Rc::new(move |_, position| told.set(position))),
                ..DraggableOptions::default()
            }
        },
    );
    swipe(&mut ui, Offset::new(40.0, 40.0), 100);
    assert_eq!(at.get(), CENTER);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, onDragStarted_startDragImmediatelyFalse_offsetShouldBePostSlopPosition_vertical, _horizontal
#[test]
fn otherwise_the_start_is_where_the_slop_was_crossed() {
    let slop = ViewConfiguration::default().touch_slop;
    for (orientation, by) in [
        (Orientation::Vertical, Offset::new(0.0, 40.0)),
        (Orientation::Horizontal, Offset::new(40.0, 0.0)),
    ] {
        let at = Rc::new(Cell::new(Offset::ZERO));
        let told = at.clone();
        let mut ui = dragging(
            orientation,
            |_| {},
            move || {
                let told = told.clone();
                DraggableOptions {
                    on_drag_started: Some(Rc::new(move |_, position| told.set(position))),
                    ..DraggableOptions::default()
                }
            },
        );
        down_and_move(&mut ui, by);
        ui.on_root().perform_touch_input(|touch| touch.up());
        ui.wait_for_idle();
        let along = |offset: Offset| if by.x != 0.0 { offset.x } else { offset.y };
        assert_eq!(along(at.get()), 50.0 + slop);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, onDragStopped_withSameTimeStamps_shouldNotPropagateNanVelocity
#[test]
fn moves_at_the_same_time_tell_no_nan() {
    let (velocity, on_stop) = stopped_velocity();
    let mut ui = dragging(
        Orientation::Horizontal,
        |_| {},
        move || DraggableOptions {
            on_drag_stopped: on_stop.clone(),
            ..DraggableOptions::default()
        },
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        for _ in 0..3 {
            touch.move_pointer_by(0, Offset::new(100.0, 0.0), 0);
        }
        touch.up();
    });
    ui.wait_for_idle();
    assert!(!velocity.get().is_nan());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, nestedDraggable_childStopsConsuming_shouldAllowParentToConsume
#[test]
fn a_parent_takes_over_once_the_child_is_gone() {
    let (parent, add_parent) = counter();
    let (child, add_child) = counter();
    let child_total = child.clone();
    let shown = mutable_state_of(true);
    let read = shown.handle();
    let hide = shown.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (add_parent, add_child) = (add_parent.clone(), add_child.clone());
        let child_total = child_total.clone();
        let outer = remember_draggable_state!(move |delta| add_parent(delta));
        Box!(
            modifier = Modifier.size(400.dp()).draggable_with(
                outer,
                Orientation::Vertical,
                DraggableOptions::default()
            ),
            {
                if read.get() {
                    let add_child = add_child.clone();
                    let child_total = child_total.clone();
                    let inner = remember_draggable_state!(move |delta| {
                        add_child(delta);
                        if child_total.get().abs() >= 80.0 {
                            hide.set(false);
                        }
                    });
                    Box!(
                        modifier = Modifier.size(400.dp()).draggable_with(
                            inner,
                            Orientation::Vertical,
                            DraggableOptions::default()
                        )
                    );
                }
            }
        );
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(200.0, 390.0), Offset::new(200.0, 10.0), 200);
    });
    ui.wait_for_idle();
    assert!(!shown.get());
    assert!(child.get() != 0.0);
    assert!(parent.get() != 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, nestedDraggable_childStopsConsumingMidway_shouldAllowParentToConsume
#[test]
fn a_parent_picking_up_midway_is_told_the_whole_drag() {
    let (parent, add_parent) = counter();
    let parent_start = Rc::new(Cell::new(0.0f32));
    let told = parent_start.clone();
    let keep = mutable_state_of(true);
    let (read, drop_child) = (keep.handle(), keep.handle());
    let distance = 400.0f32;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (add_parent, told) = (add_parent.clone(), told.clone());
        let outer = remember_draggable_state!(move |delta| add_parent(delta));
        Box!(
            modifier = Modifier.size(400.dp()).draggable_with(
                outer,
                Orientation::Vertical,
                DraggableOptions {
                    on_drag_started: Some(Rc::new(move |_, at: Offset| told.set(at.y))),
                    ..DraggableOptions::default()
                }
            ),
            {
                if read.get() {
                    // Takes the moves to a fifth of the drag, leaves one, takes them again to
                    // two fifths and goes.
                    Box!(
                        modifier =
                            Modifier.size(400.dp()).pointer_input((), move |scope| async move {
                                let mut child = 0.0f32;
                                let mut dropped = false;
                                loop {
                                    let change =
                                        scope.await_pointer_event(PointerEventPass::Main).await;
                                    if change.kind != PointerEventType::Move {
                                        continue;
                                    }
                                    if child.abs() < distance * 0.2 || dropped {
                                        child += change.position_change().y;
                                        change.consume();
                                    } else {
                                        dropped = true;
                                    }
                                    if child.abs() > distance * 0.4 {
                                        drop_child.set(false);
                                    }
                                }
                            })
                    );
                }
            }
        );
    });
    ui.wait_for_idle();
    let bottom = 399.0;
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(200.0, bottom), Offset::new(200.0, bottom - distance), 400);
    });
    ui.wait_for_idle();
    assert!(!keep.get());
    let told_in_all = (parent_start.get() - bottom).abs() + parent.get().abs();
    assert!((told_in_all - distance).abs() <= 2.0, "{told_in_all}");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, onGesturePickUp_doesNotNeedToWaitForCompleteTouchSlop
#[test]
fn picking_up_a_gesture_needs_only_what_is_left_of_the_slop() {
    let started = Rc::new(Cell::new(false));
    let told = started.clone();
    let nested = mutable_state_of(true);
    let read = nested.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let told = told.clone();
        let state = remember_draggable_state!(|_| {});
        let scroll = remember_scroll_state(0);
        let drag = Modifier.size(300.dp()).draggable_with(state, Orientation::Vertical, {
            DraggableOptions {
                on_drag_started: Some(Rc::new(move |_, _| told.set(true))),
                ..DraggableOptions::default()
            }
        });
        let drag = if read.get() { drag.vertical_scroll(scroll) } else { drag };
        Box!(modifier = drag);
    });
    ui.wait_for_idle();
    let slop = ViewConfiguration::default().touch_slop;
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(150.0, 150.0));
        touch.move_by(Offset::new(0.0, slop + 10.0));
    });
    ui.wait_for_idle();
    assert!(!started.get());
    nested.set(false);
    ui.wait_for_idle();
    // One event to enter the pick up, one to cross the slop.
    ui.on_root().perform_touch_input(|touch| touch.move_by(Offset::new(0.0, slop / 2.0)));
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| touch.move_by(Offset::new(0.0, slop / 2.0)));
    ui.wait_for_idle();
    assert!(started.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, assertDraggableCallbackOrder
#[test]
fn a_drag_starts_before_its_deltas_and_stops_after_them() {
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let (on_delta, on_start, on_stop) = (log.clone(), log.clone(), log.clone());
    let mut ui = dragging(Orientation::Vertical, move |_| on_delta.borrow_mut().push("delta"), {
        let on_drag_started: Option<OnDragStarted> =
            Some(Rc::new(move |_, _| on_start.borrow_mut().push("start")));
        let on_drag_stopped: Option<OnDragStopped> =
            Some(Rc::new(move |_, _| on_stop.borrow_mut().push("stop")));
        move || DraggableOptions {
            on_drag_started: on_drag_started.clone(),
            on_drag_stopped: on_drag_stopped.clone(),
            ..DraggableOptions::default()
        }
    });
    swipe(&mut ui, Offset::new(0.0, -40.0), 100);
    let log = log.borrow();
    assert_eq!(log.first(), Some(&"start"));
    assert_eq!(log.last(), Some(&"stop"));
    assert_eq!(log.iter().filter(|entry| **entry == "start").count(), 1);
    assert_eq!(log.iter().filter(|entry| **entry == "stop").count(), 1);
    assert!(log.len() > 2);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, gesturePickUp_doesNotStealFromOngoingGesture
#[test]
fn a_drag_going_on_is_not_taken_by_a_gesture_across_it() {
    let (inner, add_inner) = counter();
    let outer_started = Rc::new(Cell::new(false));
    let told = outer_started.clone();
    let mut ui = nested(
        move || {
            let told = told.clone();
            let state = remember_draggable_state!(|_| {});
            Modifier.draggable_with(
                state,
                Orientation::Horizontal,
                DraggableOptions {
                    on_drag_started: Some(Rc::new(move |_, _| told.set(true))),
                    ..DraggableOptions::default()
                },
            )
        },
        drag_of(Orientation::Vertical, add_inner, false),
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(150.0, 100.0));
        touch.move_by(Offset::new(0.0, 100.0));
    });
    ui.wait_for_idle();
    let before = inner.get();
    assert!(before != 0.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.move_by(Offset::new(100.0, 0.0));
        touch.move_by(Offset::new(100.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(inner.get(), before);
    ui.on_root().perform_touch_input(|touch| {
        touch.move_by(Offset::new(0.0, 100.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert!(inner.get() > before);
    assert!(!outer_started.get());
}

/// A box consuming moves in `pass`, around a vertical draggable, by finger.
fn consumed_around(pass: PointerEventPass) {
    let (total, add) = counter();
    let consumed = Rc::new(Cell::new(0.0f32));
    let noted = consumed.clone();
    let state = DraggableState::new(add);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let noted = noted.clone();
        Box!(
            modifier = Modifier
                .size(100.dp())
                .pointer_input((), move |scope| {
                    let noted = noted.clone();
                    async move {
                        loop {
                            let change = scope.await_pointer_event(pass).await;
                            if change.kind == PointerEventType::Move {
                                noted.set(noted.get() + change.position_change().y);
                                change.consume();
                            }
                        }
                    }
                })
                .draggable_with(state.clone(), Orientation::Vertical, DraggableOptions::default())
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        // Short of the slop, then past it.
        touch.move_by(Offset::new(0.0, 5.0));
        touch.move_by(Offset::new(0.0, 60.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert!(consumed.get() != 0.0);
    assert_eq!(total.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, parentConsumedDuringTheMainPass_shouldGiveItUp
#[test]
fn a_parent_consuming_in_the_main_pass_keeps_it() {
    consumed_around(PointerEventPass::Main);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, parentConsumedDuringTheFinalPass_shouldGiveItUp
#[test]
fn a_parent_consuming_in_the_final_pass_keeps_it() {
    consumed_around(PointerEventPass::Final);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, multiPointer_shouldNotLock
#[test]
fn two_fingers_do_not_lock_the_next_gesture() {
    let (outer, add_outer) = counter();
    let (inner, add_inner) = counter();
    let mut ui = nested(
        drag_of(Orientation::Vertical, add_outer, false),
        drag_of(Orientation::Horizontal, add_inner, false),
    );
    let middle = Offset::new(100.0, 100.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(0, middle);
        touch.down_pointer(1, middle + Offset::new(50.0, 0.0));
        for _ in 0..5 {
            touch.move_pointer_by(0, Offset::new(0.0, 25.0), 10);
            touch.move_pointer_by(1, Offset::new(0.0, 25.0), 10);
        }
        touch.up_pointer(0);
        touch.up_pointer(1);
    });
    ui.wait_for_idle();
    assert!(outer.get() != 0.0);
    assert_eq!(inner.get(), 0.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(middle);
        touch.move_by(Offset::new(100.0, 0.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert!(inner.get() != 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, multiPointer_secondPointerTakesOver
#[test]
fn a_second_finger_takes_over_when_the_first_lifts() {
    let (total, add) = counter();
    let mut ui = dragging(Orientation::Vertical, add, DraggableOptions::default);
    let half_a_slop = ViewConfiguration::default().touch_slop / 2.0;
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(0, CENTER);
        touch.down_pointer(1, CENTER + Offset::new(10.0, 0.0));
        touch.move_pointer_by(0, Offset::new(0.0, half_a_slop), 10);
        touch.move_pointer_by(1, Offset::new(0.0, half_a_slop), 10);
        touch.up_pointer(0);
        for _ in 0..5 {
            touch.move_pointer_by(1, Offset::new(0.0, 10.0), 10);
        }
        touch.up_pointer(1);
    });
    ui.wait_for_idle();
    assert!(total.get() != 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, multiPointer_shouldNotMoveIfOnlySecondaryPointersMoves, multiPointer_shouldMoveIfOnlyMainPointersMoves
#[test]
fn only_the_first_of_two_fingers_drags() {
    for (mover, moves) in [(1, false), (0, true)] {
        let (total, add) = counter();
        let mut ui = dragging(Orientation::Vertical, add, DraggableOptions::default);
        ui.on_root().perform_touch_input(|touch| {
            touch.down_pointer(0, CENTER);
            touch.down_pointer(1, CENTER + Offset::new(10.0, 0.0));
            for _ in 0..5 {
                touch.move_pointer_by(mover, Offset::new(0.0, 10.0), 10);
            }
            touch.up_pointer(0);
            touch.up_pointer(1);
        });
        ui.wait_for_idle();
        assert_eq!(total.get() != 0.0, moves, "pointer {mover}");
    }
}

/// A vertical draggable around a horizontal one, by a slop of 5.
fn competing() -> (ComposeTestRule, Rc<Cell<f32>>, Rc<Cell<f32>>) {
    let (outer, add_outer) = counter();
    let (inner, add_inner) = counter();
    let mut ui = nested(
        drag_of(Orientation::Vertical, add_outer, false),
        drag_of(Orientation::Horizontal, add_inner, false),
    );
    with_slop(&mut ui, 5.0);
    let middle = Offset::new(150.0, 150.0);
    let straight = |ui: &mut ComposeTestRule, by: Offset| {
        ui.on_root().perform_touch_input(|touch| {
            touch.down(middle);
            touch.move_by(by);
            touch.up();
        });
        ui.wait_for_idle();
    };
    straight(&mut ui, Offset::new(0.0, 100.0));
    assert_eq!((outer.get(), inner.get()), (95.0, 0.0));
    straight(&mut ui, Offset::new(100.0, 0.0));
    assert_eq!((outer.get(), inner.get()), (95.0, 95.0));
    outer.set(0.0);
    inner.set(0.0);
    with_slop(&mut ui, 20.0);
    (ui, outer, inner)
}

fn moves(ui: &mut ComposeTestRule, steps: &[Offset]) {
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(150.0, 150.0));
        for step in steps {
            touch.move_by(*step);
        }
        touch.up();
    });
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, competingNestedDrags_shouldBenefitVerticalIfEventIsMostlyVertical
#[test]
fn a_gesture_mostly_vertical_goes_to_the_vertical_around_a_horizontal() {
    let (mut ui, outer, inner) = competing();
    moves(&mut ui, &[Offset::new(5.0, 5.0), Offset::new(10.0, 0.0), Offset::new(0.0, 30.0)]);
    assert_eq!((outer.get(), inner.get()), (15.0, 0.0), "35 less the slop");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, competingNestedDrags_shouldBenefitHorizontalIfEventIsMostlyHorizontal
#[test]
fn a_gesture_mostly_horizontal_goes_to_the_horizontal_inside_a_vertical() {
    let (mut ui, outer, inner) = competing();
    moves(&mut ui, &[Offset::new(5.0, 5.0), Offset::new(0.0, 10.0), Offset::new(30.0, 0.0)]);
    assert_eq!((outer.get(), inner.get()), (0.0, 15.0), "35 less the slop");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, competingNestedDrags_shouldBenefitHorizontalOnlyForEventsBelowAngle
#[test]
fn a_gesture_within_30_degrees_goes_to_the_horizontal() {
    let (mut ui, outer, inner) = competing();
    moves(&mut ui, &[Offset::new(5.0, 5.0), Offset::new(0.0, 10.0), Offset::new(40.0, 10.0)]);
    assert_eq!((outer.get(), inner.get()), (0.0, 25.0), "45 less the slop");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, competingNestedDrags_shouldBenefitVerticalOnlyForEventsAboveAngle
#[test]
fn a_gesture_past_30_degrees_goes_to_the_vertical() {
    let (mut ui, outer, inner) = competing();
    moves(&mut ui, &[Offset::new(5.0, 5.0), Offset::new(0.0, 10.0), Offset::new(40.0, 20.0)]);
    assert_eq!((outer.get(), inner.get()), (15.0, 0.0), "35 less the slop");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, competingNestedDrags_threeLevels_shouldBenefitVerticalIfEventIsMostlyVertical
#[test]
fn of_three_levels_a_vertical_gesture_goes_past_the_horizontal_inside() {
    let (outer, add_outer) = counter();
    let (middle, add_middle) = counter();
    let (inner, add_inner) = counter();
    let outer_drag = drag_of(Orientation::Horizontal, add_outer, false);
    let middle_drag = drag_of(Orientation::Vertical, add_middle, false);
    let inner_drag = drag_of(Orientation::Horizontal, add_inner, false);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (middle_drag, inner_drag) = (middle_drag.clone(), inner_drag.clone());
        Box!(modifier = Modifier.size(300.dp()).then(outer_drag()), {
            let inner_drag = inner_drag.clone();
            Box!(modifier = Modifier.size(300.dp()).then(middle_drag()), {
                Box!(modifier = Modifier.size(300.dp()).then(inner_drag()));
            });
        });
    });
    ui.wait_for_idle();
    with_slop(&mut ui, 20.0);
    moves(&mut ui, &[Offset::new(5.0, 5.0), Offset::new(10.0, 0.0), Offset::new(0.0, 30.0)]);
    assert_eq!((outer.get(), middle.get(), inner.get()), (0.0, 15.0, 0.0));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: only a parent waiting takes a gesture at its angle
#[test]
fn a_gesture_at_its_angle_does_not_go_to_a_disabled_one_around() {
    let (outer, add_outer) = counter();
    let (inner, add_inner) = counter();
    let mut ui = nested(
        move || {
            let add_outer = add_outer.clone();
            let state = remember_draggable_state!(move |delta| add_outer(delta));
            Modifier.draggable_with(
                state,
                Orientation::Vertical,
                DraggableOptions { enabled: false, ..DraggableOptions::default() },
            )
        },
        drag_of(Orientation::Horizontal, add_inner, false),
    );
    with_slop(&mut ui, 20.0);
    moves(&mut ui, &[Offset::new(30.0, 60.0)]);
    assert_eq!((outer.get(), inner.get()), (0.0, 10.0), "30 less the slop");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, competingNestedDrags_threeLevels_shouldBenefitInnerHorizontalIfEventIsMostlyHorizontal
#[test]
fn of_three_levels_a_horizontal_gesture_stays_with_the_horizontal_inside() {
    let (outer, add_outer) = counter();
    let (middle, add_middle) = counter();
    let (inner, add_inner) = counter();
    let outer_drag = drag_of(Orientation::Horizontal, add_outer, false);
    let middle_drag = drag_of(Orientation::Vertical, add_middle, false);
    let inner_drag = drag_of(Orientation::Horizontal, add_inner, false);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (middle_drag, inner_drag) = (middle_drag.clone(), inner_drag.clone());
        Box!(modifier = Modifier.size(300.dp()).then(outer_drag()), {
            let inner_drag = inner_drag.clone();
            Box!(modifier = Modifier.size(300.dp()).then(middle_drag()), {
                Box!(modifier = Modifier.size(300.dp()).then(inner_drag()));
            });
        });
    });
    ui.wait_for_idle();
    with_slop(&mut ui, 20.0);
    moves(&mut ui, &[Offset::new(5.0, 5.0), Offset::new(0.0, 10.0), Offset::new(30.0, 0.0)]);
    assert_eq!((outer.get(), middle.get(), inner.get()), (0.0, 0.0, 15.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_nestedDraggable_parentConsumesInitially_childDoesNotSteal
#[test]
fn a_parent_starting_immediately_keeps_it_from_the_child() {
    let (parent, add_parent) = counter();
    let (child, add_child) = counter();
    let mut ui = nested(
        drag_of(Orientation::Horizontal, add_parent, true),
        drag_of(Orientation::Horizontal, add_child, false),
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 100.0), Offset::new(200.0, 100.0), 100);
    });
    ui.wait_for_idle();
    assert!(parent.get() > 0.0);
    assert_eq!(child.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_nestedDraggable_parentConsumesInitially_childDoesNotSteal_crossAxis
#[test]
fn a_parent_starting_immediately_keeps_it_from_a_child_across() {
    let (parent, add_parent) = counter();
    let (child, add_child) = counter();
    let mut ui = nested(
        drag_of(Orientation::Horizontal, add_parent, true),
        drag_of(Orientation::Vertical, add_child, false),
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(100.0, 100.0));
        touch.move_by(Offset::new(0.0, 100.0));
        touch.move_by(Offset::new(100.0, 0.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert!(parent.get() > 0.0);
    assert_eq!(child.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, draggable_nestedDraggable_childConsumesInitially_parentDoesNotSteal
#[test]
fn a_child_starting_immediately_keeps_it_from_the_parent() {
    let (parent, add_parent) = counter();
    let (child, add_child) = counter();
    let mut ui = nested(
        drag_of(Orientation::Horizontal, add_parent, false),
        drag_of(Orientation::Horizontal, add_child, true),
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe(Offset::new(100.0, 100.0), Offset::new(200.0, 100.0), 100);
    });
    ui.wait_for_idle();
    assert!(child.get() > 0.0);
    assert_eq!(parent.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/DraggableTest.kt, drag_zeroDeltas_shouldConsumeEvents
#[test]
fn moves_of_nothing_are_consumed_once_dragging() {
    let (outer, add_outer) = counter();
    let (inner, add_inner) = counter();
    let unconsumed = Rc::new(Cell::new(0));
    let noted = unconsumed.clone();
    let outer_drag = drag_of(Orientation::Vertical, add_outer, false);
    let inner_drag = drag_of(Orientation::Vertical, add_inner, false);
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let noted = noted.clone();
        let inner_drag = inner_drag.clone();
        Box!(
            modifier = Modifier
                .size(300.dp())
                .pointer_input((), move |scope| {
                    let noted = noted.clone();
                    async move {
                        loop {
                            let change = scope.await_pointer_event(PointerEventPass::Final).await;
                            if change.kind == PointerEventType::Move && !change.is_consumed() {
                                noted.set(noted.get() + 1);
                            }
                        }
                    }
                })
                .then(outer_drag()),
            {
                Box!(modifier = Modifier.size(300.dp()).then(inner_drag()));
            }
        );
    });
    ui.wait_for_idle();
    let slop = ViewConfiguration::default().touch_slop;
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(150.0, 100.0));
        touch.move_by(Offset::new(0.0, 2.0 * slop));
    });
    ui.wait_for_idle();
    assert!(inner.get() != 0.0);
    assert_eq!(outer.get(), 0.0);
    inner.set(0.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.move_by(Offset::ZERO);
        touch.move_by(Offset::ZERO);
        touch.move_by(Offset::new(0.0, slop));
    });
    ui.wait_for_idle();
    assert!(inner.get() != 0.0);
    assert_eq!(outer.get(), 0.0);
    assert_eq!(unconsumed.get(), 0, "every move seen consumed");
}

/// The old form of the modifier still drags, and a disabled one still composes.
#[test]
fn the_short_form_is_the_full_one() {
    let (total, add) = counter();
    let mut ui = drag_box(move || {
        let add = add.clone();
        let state = remember_draggable_state!(move |delta| add(delta));
        Modifier.draggable(state, Orientation::Vertical, true, true, |_| {}, |_| {})
    });
    swipe(&mut ui, Offset::new(0.0, 40.0), 100);
    assert!(total.get() < 0.0, "reversed");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: processDraggingState, isConsumedByOther
#[test]
fn a_move_taken_by_another_stops_the_drag_without_velocity() {
    let (total, add) = counter();
    let (velocity, on_stop) = stopped_velocity();
    let take = Rc::new(Cell::new(false));
    let taking = take.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (add, on_stop, taking) = (add.clone(), on_stop.clone(), taking.clone());
        let state = remember_draggable_state!(move |delta| add(delta));
        Box!(
            modifier = Modifier
                .size(100.dp())
                .pointer_input((), move |scope| {
                    let taking = taking.clone();
                    async move {
                        loop {
                            let change = scope.await_pointer_event(PointerEventPass::Initial).await;
                            if taking.get() && change.kind == PointerEventType::Move {
                                change.consume();
                            }
                        }
                    }
                })
                .draggable_with(
                    state,
                    Orientation::Horizontal,
                    DraggableOptions { on_drag_stopped: on_stop, ..DraggableOptions::default() }
                )
        );
    });
    ui.wait_for_idle();
    down_and_move(&mut ui, Offset::new(40.0, 0.0));
    let before = total.get();
    assert!(before > 0.0);
    assert!(velocity.get().is_nan(), "going on");
    take.set(true);
    ui.on_root().perform_touch_input(|touch| touch.move_by(Offset::new(40.0, 0.0)));
    ui.wait_for_idle();
    assert_eq!(total.get(), before);
    assert_eq!(velocity.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isAnyChangedToDown
#[test]
fn a_press_with_another_finger_down_starts_nothing() {
    let (total, add) = counter();
    let reverse = mutable_state_of(false);
    let read = reverse.handle();
    let mut ui = dragging(Orientation::Horizontal, add, move || DraggableOptions {
        reverse_direction: read.get(),
        ..DraggableOptions::default()
    });
    down_and_move(&mut ui, Offset::new(40.0, 0.0));
    // The detection starts over with the finger still down.
    reverse.set(true);
    ui.wait_for_idle();
    total.set(0.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(1, Offset::new(10.0, 10.0));
        touch.move_pointer_by(1, Offset::new(50.0, 0.0), 10);
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: processDraggingState follows its pointer
#[test]
fn once_dragging_a_second_finger_moves_nothing() {
    let (total, add) = counter();
    let mut ui = dragging(Orientation::Horizontal, add, DraggableOptions::default);
    down_and_move(&mut ui, Offset::new(40.0, 0.0));
    let before = total.get();
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(1, Offset::new(10.0, 10.0));
        touch.move_pointer_by(1, Offset::new(30.0, 0.0), 10);
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), before);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: reverseIfNeeded
#[test]
fn reversed_the_deltas_and_the_velocity_are_negated() {
    let (total, add) = counter();
    let (velocity, on_stop) = stopped_velocity();
    let mut ui = dragging(Orientation::Horizontal, add, move || DraggableOptions {
        reverse_direction: true,
        on_drag_stopped: on_stop.clone(),
        ..DraggableOptions::default()
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(CENTER, CENTER + Offset::new(100.0, 0.0), 500.0, Some(100));
    });
    ui.wait_for_idle();
    assert!(total.get() < 0.0);
    assert!((velocity.get() + 500.0).abs() < 1.0, "{}", velocity.get());
}
