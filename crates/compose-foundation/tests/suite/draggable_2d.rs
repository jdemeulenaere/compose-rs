//! `Modifier.draggable_2d`: scenarios of Compose's `Draggable2DTest`, rewritten. What it shares
//! with `draggable` (the node's detection and inputs) is tested in `draggable.rs`; these are what
//! two dimensions change, and the inputs' stops once more on its own element.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, MutableInteractionSource, Offset, UnitExt, Velocity, ViewConfiguration};

use crate::interaction_helpers::{kinds, record};
use crate::scroll_support::run;

const CENTER: Offset = Offset::new(50.0, 50.0);

fn counter() -> (Rc<Cell<Offset>>, impl Fn(Offset) + Clone + 'static) {
    let total = Rc::new(Cell::new(Offset::ZERO));
    let adding = total.clone();
    (total, move |delta: Offset| adding.set(adding.get() + delta))
}

fn drag_box(
    on_delta: impl Fn(Offset) + Clone + 'static,
    options: impl Fn() -> Draggable2DOptions + 'static,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let on_delta = on_delta.clone();
        let state = remember_draggable_2d_state!(on_delta);
        Box!(modifier = Modifier.size(100.dp()).draggable_2d(state, options()));
    });
    ui.wait_for_idle();
    ui
}

fn down_and_move(ui: &mut ComposeTestRule, by: Offset) {
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(by);
    });
    ui.wait_for_idle();
}

fn stops(count: &Rc<Cell<u32>>) -> Option<OnDrag2DStopped> {
    let count = count.clone();
    Some(Rc::new(move |_, _| count.set(count.get() + 1)))
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_2d_drag
#[test]
fn a_drag_any_way_moves_it_on_both_axes() {
    let (total, add) = counter();
    let mut ui = drag_box(add, Draggable2DOptions::default);
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert!(total.get().x > 0.0 && total.get().y > 0.0);
    ui.on_root().perform_touch_input(|touch| touch.move_by(Offset::new(-100.0, -100.0)));
    ui.wait_for_idle();
    assert!(total.get().x < 0.01 && total.get().y < 0.01);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_startStop
#[test]
fn a_drag_starts_and_stops_once() {
    let (started, stopped) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let told = started.clone();
    let on_start: Option<OnDragStarted> = Some(Rc::new(move |_, _| told.set(told.get() + 1)));
    let on_stop = stops(&stopped);
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            on_drag_started: on_start.clone(),
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(100.0, 0.0));
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_eq!((started.get(), stopped.get()), (1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_disableWontCallLambda, draggable2D_callsDragStop_whenDisabled
#[test]
fn disabling_it_stops_the_drag_and_moves_nothing_after() {
    let (total, add) = counter();
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let enabled = mutable_state_of(true);
    let read = enabled.handle();
    let mut ui = drag_box(add, move || Draggable2DOptions {
        enabled: read.get(),
        on_drag_stopped: on_stop.clone(),
        ..Draggable2DOptions::default()
    });
    down_and_move(&mut ui, Offset::new(100.0, 0.0));
    let before = total.get();
    assert!(before.x > 0.0);
    enabled.set(false);
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
    ui.on_root().perform_touch_input(|touch| {
        touch.up();
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), before);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_velocityProxy
#[test]
fn a_release_tells_the_velocity_on_both_axes() {
    let velocity = Rc::new(Cell::new(Velocity::ZERO));
    let told = velocity.clone();
    let on_stop: Option<OnDrag2DStopped> = Some(Rc::new(move |_, at| told.set(at)));
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
        },
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        for _ in 0..10 {
            touch.move_pointer_by(0, Offset::new(5.0, -10.0), 10);
        }
        touch.up();
    });
    ui.wait_for_idle();
    assert!((velocity.get().x - 500.0).abs() < 1.0, "{:?}", velocity.get());
    assert!((velocity.get().y + 1000.0).abs() < 1.0, "{:?}", velocity.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_startWithoutSlop_ifAnimating
#[test]
fn starting_immediately_there_is_no_slop() {
    let (total, add) = counter();
    let mut ui = drag_box(add, || Draggable2DOptions {
        start_drag_immediately: true,
        ..Draggable2DOptions::default()
    });
    down_and_move(&mut ui, Offset::new(100.0, 40.0));
    assert_eq!(total.get(), Offset::new(100.0, 40.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_cancel_callsDragStop
#[test]
fn a_cancelled_touch_stops_the_drag() {
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
        },
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 100.0));
        touch.cancel();
    });
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_callsDragStop_whenNewState
#[test]
fn a_new_state_stops_the_drag() {
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let states = [draggable_2d_state(|_| {}), draggable_2d_state(|_| {})];
    let state = mutable_state_of(0usize);
    let read = state.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let options = Draggable2DOptions {
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
        };
        Box!(modifier = Modifier.size(100.dp()).draggable_2d(states[read.get()].clone(), options));
    });
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(stopped.get(), 0);
    state.set(1);
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_callsDragStop_whenNewReverseDirection
#[test]
fn reversing_it_stops_the_drag_and_negates_what_follows() {
    let (total, add) = counter();
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let reverse = mutable_state_of(false);
    let read = reverse.handle();
    let mut ui = drag_box(add, move || Draggable2DOptions {
        reverse_direction: read.get(),
        on_drag_stopped: on_stop.clone(),
        ..Draggable2DOptions::default()
    });
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert_eq!(stopped.get(), 0);
    reverse.set(true);
    ui.wait_for_idle();
    assert_eq!(stopped.get(), 1);
    total.set(Offset::ZERO);
    ui.on_root().perform_touch_input(|touch| {
        touch.up();
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 100.0));
    });
    ui.wait_for_idle();
    assert!(total.get().x < 0.0 && total.get().y < 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_resumesNormally_whenInterruptedWithHigherPriority
#[test]
fn a_drag_of_a_higher_priority_stops_it_and_a_later_one_works() {
    let (total, add) = counter();
    let stopped = Rc::new(Cell::new(0));
    let on_stop = stops(&stopped);
    let state = draggable_2d_state(add);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let options = Draggable2DOptions {
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
        };
        Box!(modifier = Modifier.size(100.dp()).draggable_2d(given.clone(), options));
    });
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    let before = total.get();
    let taking = state.clone();
    run(&mut ui, async move {
        taking
            .drag(MutatePriority::PreventUserInput, |scope| async move {
                scope.drag_by(Offset::new(123.0, 123.0));
            })
            .await
            .expect("the highest priority");
    });
    assert_eq!(total.get(), before + Offset::new(123.0, 123.0));
    assert_eq!(stopped.get(), 1);
    ui.on_root().perform_touch_input(|touch| {
        touch.up();
        touch.down(CENTER);
        touch.move_by(Offset::new(100.0, 100.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert!(total.get().x > before.x + 123.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_noNestedDrag, draggable2D_noNestedDragWithDraggable
#[test]
fn the_inner_takes_the_drag_from_a_2d_or_a_1d_outer() {
    for outer_2d in [true, false] {
        let (inner, add_inner) = counter();
        let (outer, add_outer) = counter();
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            let (add_outer, add_inner) = (add_outer.clone(), add_inner.clone());
            let around = if outer_2d {
                let state = remember_draggable_2d_state!(add_outer);
                Modifier.size(300.dp()).draggable_2d(state, Draggable2DOptions::default())
            } else {
                let state = remember_draggable_state!(move |x| add_outer(Offset::new(x, 0.0)));
                Modifier.size(300.dp()).draggable_with(
                    state,
                    Orientation::Horizontal,
                    DraggableOptions::default(),
                )
            };
            Box!(modifier = around, {
                let add_inner = add_inner.clone();
                let state = remember_draggable_2d_state!(add_inner);
                Box!(
                    modifier =
                        Modifier.size(300.dp()).draggable_2d(state, Draggable2DOptions::default())
                );
            });
        });
        ui.wait_for_idle();
        down_and_move(&mut ui, Offset::new(100.0, 100.0));
        assert!(inner.get().x > 0.0 && inner.get().y > 0.0);
        assert_eq!(outer.get(), Offset::ZERO);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_interactionSource, _resetWhenEnabledChanged, _resetWhenInteractionSourceChanged
#[test]
fn drags_show_on_the_source_and_cancel_when_it_changes() {
    let (one, two) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let (recorded_one, recorded_two) = (record(one.as_source()), record(two.as_source()));
    let current = mutable_state_of(one);
    let read = current.handle();
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            interaction_source: Some(read.get()),
            ..Draggable2DOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(50.0, 50.0));
    assert_eq!(kinds(&recorded_one), ["Start"]);
    current.set(two);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded_one), ["Start", "DragCancel"]);
    assert!(recorded_two.borrow().is_empty());
    ui.on_root().perform_touch_input(|touch| {
        touch.up();
        touch.down(CENTER);
        touch.move_by(Offset::new(50.0, 50.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded_two), ["Start", "Stop"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_velocityIsLimitedByViewConfiguration
#[test]
fn the_velocity_is_limited_on_each_axis() {
    let velocity = Rc::new(Cell::new(Velocity::ZERO));
    let told = velocity.clone();
    let on_stop: Option<OnDrag2DStopped> = Some(Rc::new(move |_, at| told.set(at)));
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
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
    assert_eq!(velocity.get().x, 1000.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, onDragStarted_startDragImmediatelyFalse_offsetShouldBePostSlopPosition
#[test]
fn the_start_is_where_the_slop_was_crossed_along_the_gesture() {
    let at = Rc::new(Cell::new(Offset::ZERO));
    let told = at.clone();
    let mut ui = drag_box(
        |_| {},
        move || {
            let told = told.clone();
            Draggable2DOptions {
                on_drag_started: Some(Rc::new(move |_, position| told.set(position))),
                ..Draggable2DOptions::default()
            }
        },
    );
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    let slop = ViewConfiguration::default().touch_slop;
    let along = slop * std::f32::consts::FRAC_1_SQRT_2;
    assert!((at.get().x - (50.0 + along)).abs() < 0.5, "{:?}", at.get());
    assert!((at.get().y - (50.0 + along)).abs() < 0.5, "{:?}", at.get());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable2d.kt: reverseIfNeeded
#[test]
fn reversed_the_velocity_is_negated() {
    let velocity = Rc::new(Cell::new(Velocity::ZERO));
    let told = velocity.clone();
    let on_stop: Option<OnDrag2DStopped> = Some(Rc::new(move |_, at| told.set(at)));
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            reverse_direction: true,
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
        },
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(CENTER, CENTER + Offset::new(100.0, 0.0), 500.0, Some(100));
    });
    ui.wait_for_idle();
    assert!((velocity.get().x + 500.0).abs() < 1.0, "{:?}", velocity.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_immediateStart_callsStopWithoutSlop
#[test]
fn a_mouse_press_starts_it_at_once_and_its_release_stops_it() {
    let (started, stopped) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let told = started.clone();
    let on_start: Option<OnDragStarted> = Some(Rc::new(move |_, _| told.set(told.get() + 1)));
    let on_stop = stops(&stopped);
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            start_drag_immediately: true,
            on_drag_started: on_start.clone(),
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
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

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_updates_startDragImmediately
#[test]
fn starting_immediately_can_change() {
    let (total, add) = counter();
    let immediately = mutable_state_of(false);
    let read = immediately.handle();
    let mut ui = drag_box(add, move || Draggable2DOptions {
        start_drag_immediately: read.get(),
        ..Draggable2DOptions::default()
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
    assert_eq!(total.get(), Offset::ZERO);
    immediately.set(true);
    ui.wait_for_idle();
    move_a_bit(&mut ui);
    assert_eq!(total.get(), Offset::new(half_a_slop, 0.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_updates_onDragStarted
#[test]
fn the_newest_start_callback_is_called() {
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (told_first, told_second) = (first.clone(), second.clone());
    let on_first: Option<OnDragStarted> =
        Some(Rc::new(move |_, _| told_first.set(told_first.get() + 1)));
    let on_second: Option<OnDragStarted> =
        Some(Rc::new(move |_, _| told_second.set(told_second.get() + 1)));
    let callback = mutable_state_of(0usize);
    let read = callback.handle();
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            on_drag_started: if read.get() == 0 { on_first.clone() } else { on_second.clone() },
            ..Draggable2DOptions::default()
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

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_updates_onDragStopped
#[test]
fn the_newest_stop_callback_is_called_even_for_a_drag_going_on() {
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (on_first, on_second) = (stops(&first), stops(&second));
    let callback = mutable_state_of(0usize);
    let read = callback.handle();
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            on_drag_stopped: if read.get() == 0 { on_first.clone() } else { on_second.clone() },
            ..Draggable2DOptions::default()
        },
    );
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    callback.set(1);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get()), (0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_interactionSource_resetWhenDisposed
#[test]
fn a_drag_cancels_on_the_source_when_disposed() {
    let source = MutableInteractionSource::new();
    let recorded = record(source.as_source());
    let shown = mutable_state_of(true);
    let read = shown.handle();
    let given = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        if read.get() {
            let state = remember_draggable_2d_state!(|_| {});
            Box!(
                modifier = Modifier.size(100.dp()).draggable_2d(
                    state,
                    Draggable2DOptions {
                        interaction_source: Some(given.clone()),
                        ..Draggable2DOptions::default()
                    }
                )
            );
        }
    });
    down_and_move(&mut ui, Offset::new(50.0, 50.0));
    assert_eq!(kinds(&recorded), ["Start"]);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Start", "DragCancel"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_cancelMidDown_shouldContinueWithNextDown
#[test]
fn a_touch_cancelled_at_its_press_leaves_the_next_to_drag() {
    let (total, add) = counter();
    let mut ui = drag_box(add, Draggable2DOptions::default);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.cancel();
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), Offset::ZERO);
    down_and_move(&mut ui, Offset::new(100.0, 100.0));
    assert!(total.get().x > 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/Draggable2DTest.kt, draggable2D_noMomentumDragging_onDragStopped_shouldGenerateZeroVelocity
#[test]
fn a_pointer_that_stopped_before_its_release_has_no_velocity() {
    let velocity = Rc::new(Cell::new(Velocity::new(1.0, 1.0)));
    let told = velocity.clone();
    let on_stop: Option<OnDrag2DStopped> = Some(Rc::new(move |_, at| told.set(at)));
    let mut ui = drag_box(
        |_| {},
        move || Draggable2DOptions {
            on_drag_stopped: on_stop.clone(),
            ..Draggable2DOptions::default()
        },
    );
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        for _ in 0..30 {
            touch.move_by(Offset::new(5.0, -10.0));
        }
        touch.advance_event_time(3000);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(velocity.get(), Velocity::ZERO);
}
