//! Phase F review: failing tests for compose-desktop's input mapping (docs/07-platforms.md,
//! "Desktop"). The mapping lives in a private module, so this test compiles that same file as
//! a module of its own (`#[path]`) and feeds it winit events built by hand; no window is
//! opened. winit's `KeyEvent` can't be built outside winit (a private field), so keyboard
//! findings are written up in the review instead.

#![allow(dead_code)]

#[path = "../src/input.rs"]
mod input;

use std::cell::Cell;
use std::rc::Rc;

use compose_ui::{
    Density, IntSize, Modifier, UnitExt, empty_measure_policy,
    platform::{UiHost, emit_layout_node},
};
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceId, Touch, TouchPhase, WindowEvent};

/// A 100 x 100 px clickable at the origin, counting its clicks.
fn clickable_host() -> (UiHost, Rc<Cell<u32>>) {
    let clicks = Rc::new(Cell::new(0));
    let counter = clicks.clone();
    let mut host = UiHost::new(
        IntSize::new(200, 200),
        Density::default(),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
    );
    host.set_content(move || {
        let counter = counter.clone();
        emit_layout_node(
            Modifier.size(100.dp()).clickable(move || counter.set(counter.get() + 1)),
            empty_measure_policy(),
            || {},
        );
    });
    host.settle(10);
    (host, clicks)
}

fn touch(phase: TouchPhase, x: f64, y: f64, id: u64) -> WindowEvent {
    WindowEvent::Touch(Touch {
        device_id: DeviceId::dummy(),
        phase,
        location: PhysicalPosition::new(x, y),
        force: None,
        id,
    })
}

/// A touch the system cancels (a palm, an edge swipe the OS takes, the window losing the touch)
/// is not a tap: Compose's tap detection ends on cancellation without clicking
/// (`waitForUpOrCancellation` returns null, `PointerInputChange.isConsumed`/cancel). Here
/// `TouchPhase::Cancelled` is mapped to an ordinary release at the touch's position, so the
/// clickable under the finger clicks.
#[test]
fn a_cancelled_touch_does_not_click() {
    let (mut host, clicks) = clickable_host();
    let mut state = input::InputState::default();
    input::handle(&mut state, &mut host, &touch(TouchPhase::Started, 50.0, 50.0, 7));
    input::handle(&mut state, &mut host, &touch(TouchPhase::Cancelled, 50.0, 50.0, 7));
    host.settle(10);
    assert_eq!(clicks.get(), 0, "a cancelled touch clicked");
}

/// A finished touch on the same button still clicks (the control for the test above).
#[test]
fn an_ended_touch_clicks() {
    let (mut host, clicks) = clickable_host();
    let mut state = input::InputState::default();
    input::handle(&mut state, &mut host, &touch(TouchPhase::Started, 50.0, 50.0, 7));
    input::handle(&mut state, &mut host, &touch(TouchPhase::Ended, 50.0, 50.0, 7));
    host.settle(10);
    assert_eq!(clicks.get(), 1);
}
