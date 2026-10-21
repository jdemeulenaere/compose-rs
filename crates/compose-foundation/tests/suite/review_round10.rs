//! Tenth review round: `draggable`, against Jetpack Compose's `DragGestureNode`. Each test
//! states the input, what Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, UnitExt};

fn send(ui: &mut ComposeTestRule, event: PointerEvent) {
    ui.host().send_pointer(event);
    ui.wait_for_idle();
}

/// A vertical `draggable` pressed at y = 10 and moved to y = 40 in one move (the touch slop is
/// 8 px at density 1). Compose's `DragGestureNode` sends
/// `DragStarted(slopTriggerChange.position - postSlopOffset)`, "the down event + touch slop
/// value": `onDragStarted` gets y = 18, and the
/// first delta is the 22 px past the slop. Here `on_drag_started` gets the press's position,
/// y = 10, so the start and the deltas don't add up to where the pointer is (10 + 22 != 40).
#[test]
fn drag_started_is_where_the_slop_was_crossed() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let (started, deltas) = (seen.clone(), seen.clone());
        let state = remember_draggable_state!(move |delta| {
            deltas.borrow_mut().push(format!("delta {delta}"));
        });
        Box!(
            modifier = Modifier.size(100.dp()).draggable(
                state,
                Orientation::Vertical,
                true,
                false,
                move |at: Offset| started.borrow_mut().push(format!("started {}", at.y)),
                |_| {}
            )
        );
    });
    send(&mut ui, PointerEvent::press(Offset::new(10.0, 10.0)));
    send(&mut ui, PointerEvent::moved(Offset::new(10.0, 40.0), true));
    assert_eq!(*log.borrow(), vec!["started 18", "delta 22"]);
}
