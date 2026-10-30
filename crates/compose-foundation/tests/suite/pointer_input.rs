//! `Modifier.pointer_input`: coroutines handling pointer events, and the gesture detectors.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{
    DragGestures, IntSize, Offset, PointerEvent, PointerEventPass, PointerEventType, TapGestures,
    UnitExt,
};

type Log = Rc<RefCell<Vec<String>>>;

/// The same event from a finger: a mouse's slop is next to nothing, as `pointerSlop`.
fn finger(ui: &mut ComposeTestRule, event: PointerEvent) {
    ui.host()
        .send_pointer(event.with_pointer(compose_ui::PointerId(1), compose_ui::PointerType::Touch));
    ui.wait_for_idle();
}

fn press(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.wait_for_idle();
}

fn release(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
    ui.wait_for_idle();
}

fn drag_to(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
    ui.wait_for_idle();
}

#[test]
fn a_block_awaits_the_events_reaching_its_node() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let seen = seen.clone();
                async move {
                    loop {
                        let change = scope.await_pointer_event(PointerEventPass::Main).await;
                        if change.kind != PointerEventType::Enter {
                            seen.borrow_mut()
                                .push(format!("{:?} {}", change.kind, change.position.x));
                        }
                    }
                }
            })
        );
    });
    press(&mut ui, 10.0, 10.0);
    drag_to(&mut ui, 20.0, 10.0);
    release(&mut ui, 20.0, 10.0);
    assert_eq!(*log.borrow(), vec!["Press 10", "Move 20", "Release 20"]);
}

#[test]
fn taps_double_taps_and_long_presses_are_told_apart() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let (tap, double, long) = (seen.clone(), seen.clone(), seen.clone());
                async move {
                    scope
                        .detect_tap_gestures(
                            TapGestures::default()
                                .on_tap(move |at| tap.borrow_mut().push(format!("tap {}", at.x)))
                                .on_double_tap(move |_| double.borrow_mut().push("double".into()))
                                .on_long_press(move |_| long.borrow_mut().push("long".into())),
                        )
                        .await
                }
            })
        );
    });
    // A tap: told once the double-tap timeout is over.
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    assert!(log.borrow().is_empty());
    ui.advance_time_by(Duration::from_millis(350));
    assert_eq!(*log.borrow(), vec!["tap 10"]);
    // Two quick taps: a double tap.
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(100));
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    assert_eq!(log.borrow().last().map(String::as_str), Some("double"));
    // Held: a long press, and no tap at the release.
    ui.advance_time_by(Duration::from_millis(500));
    press(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(450));
    assert_eq!(log.borrow().last().map(String::as_str), Some("long"));
    release(&mut ui, 10.0, 10.0);
    ui.advance_time_by(Duration::from_millis(400));
    assert_eq!(*log.borrow(), vec!["tap 10", "double", "long"]);
}

#[test]
fn drags_start_past_the_slop_and_report_their_amounts() {
    let log: Log = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(100.dp()).pointer_input((), move |scope| {
                let (start, drag, end) = (seen.clone(), seen.clone(), seen.clone());
                async move {
                    scope
                        .detect_drag_gestures(
                            DragGestures::default()
                                .on_drag_start(move |at| {
                                    start.borrow_mut().push(format!("start {}", at.x))
                                })
                                .on_drag(move |_, amount| {
                                    drag.borrow_mut().push(format!("drag {}", amount.x))
                                })
                                .on_drag_end(move || end.borrow_mut().push("end".into())),
                        )
                        .await
                }
            })
        );
    });
    finger(&mut ui, PointerEvent::press(Offset::new(10.0, 10.0)));
    finger(&mut ui, PointerEvent::moved(Offset::new(15.0, 10.0), true));
    assert!(log.borrow().is_empty(), "within the slop");
    finger(&mut ui, PointerEvent::moved(Offset::new(30.0, 10.0), true));
    finger(&mut ui, PointerEvent::moved(Offset::new(40.0, 10.0), true));
    finger(&mut ui, PointerEvent::release(Offset::new(40.0, 10.0)));
    assert_eq!(*log.borrow(), vec!["start 30", "drag 12", "drag 10", "end"]);
}

#[test]
fn what_a_block_consumes_an_outer_clickable_does_not_click() {
    let clicks = Rc::new(std::cell::Cell::new(0));
    let counted = clicks.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let counted = counted.clone();
        Box!(
            modifier = Modifier.size(100.dp()).clickable(move || counted.set(counted.get() + 1)),
            {
                Box!(
                    modifier = Modifier.size(50.dp()).pointer_input((), |scope| async move {
                        scope.detect_tap_gestures(TapGestures::default().on_tap(|_| {})).await
                    })
                );
            }
        );
    });
    press(&mut ui, 10.0, 10.0);
    release(&mut ui, 10.0, 10.0);
    assert_eq!(clicks.get(), 0, "the inner block consumed the press");
    press(&mut ui, 80.0, 80.0);
    release(&mut ui, 80.0, 80.0);
    assert_eq!(clicks.get(), 1);
}

#[test]
fn draggable_reports_deltas_along_its_axis_past_the_slop() {
    let offset = Rc::new(std::cell::Cell::new(0.0f32));
    let moved = offset.clone();
    let stopped = Rc::new(std::cell::Cell::new(false));
    let stop = stopped.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let (moved, stop) = (moved.clone(), stop.clone());
        let state = remember_draggable_state!(move |delta| moved.set(moved.get() + delta));
        Box!(
            modifier = Modifier.size(100.dp()).draggable(
                state,
                Orientation::Horizontal,
                true,
                false,
                |_| {},
                move |_| stop.set(true),
            )
        );
    });
    press(&mut ui, 10.0, 10.0);
    // Vertical moves aren't this drag.
    drag_to(&mut ui, 10.0, 15.0);
    assert_eq!(offset.get(), 0.0);
    drag_to(&mut ui, 30.0, 15.0);
    // 20 px past the press, less a mouse's slop (8 px / 144, as `pointerSlop`).
    let slop = 8.0 / 144.0;
    assert_eq!(offset.get(), 20.0 - slop);
    drag_to(&mut ui, 50.0, 15.0);
    assert_eq!(offset.get(), 40.0 - slop);
    release(&mut ui, 50.0, 15.0);
    assert!(stopped.get());
}
