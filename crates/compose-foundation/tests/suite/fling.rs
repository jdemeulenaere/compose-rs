//! Flings: a touch let go while moving scrolls on, slowing down, as on Android.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::*;
use compose_runtime::platform;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, PointerId, PointerType, UnitExt};

fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

/// A quick upward swipe: 300 px in 60 ms (5,000 px/s), every 10 ms, at their times.
fn swipe_up(ui: &mut ComposeTestRule, x: f32) {
    let start = platform::now_nanos();
    ui.host().send_pointer(touch(PointerEvent::press(Offset::new(x, 400.0))).at_time(start));
    for step in 1..=6u64 {
        let at = start + step * 10_000_000;
        let y = 400.0 - step as f32 * 50.0;
        ui.host().send_pointer(touch(PointerEvent::moved(Offset::new(x, y), true)).at_time(at));
    }
    ui.host().send_pointer(
        touch(PointerEvent::release(Offset::new(x, 100.0))).at_time(start + 60_000_000),
    );
}

fn list(state: LazyListState, clicks: Rc<Cell<u32>>) -> impl Fn() {
    move || {
        let clicks = clicks.clone();
        LazyColumn!(
            modifier = Modifier.fill_max_size(),
            state = state,
            move |scope: LazyListScope| {
                let clicks = clicks.clone();
                scope.items(1000, move |index| {
                    let clicks = clicks.clone();
                    BasicText!(
                        "Row {}",
                        index,
                        modifier = Modifier
                            .height(50.dp())
                            .fill_max_width()
                            .clickable(move || clicks.set(clicks.get() + 1))
                    );
                });
            }
        );
    }
}

#[test]
fn a_touch_let_go_while_moving_flings_the_list() {
    let state = LazyListState::new(0, 0);
    let clicks = Rc::new(Cell::new(0));
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 500), list(*state, clicks.clone()));
    ui.set_auto_advance(false);
    swipe_up(&mut ui, 100.0);
    ui.advance_time_by_frame();
    // The drag itself: 300 px less the slop, about 5 rows.
    let released = state.first_visible_item_index();
    ui.advance_time_by(Duration::from_millis(2000));
    let settled = state.first_visible_item_index();
    assert!(settled > released + 5, "flung from row {released} to {settled}");
    // It settles: no frame wanted any more.
    ui.advance_time_by(Duration::from_millis(3000));
    assert_eq!(state.first_visible_item_index(), settled);
}

#[test]
fn a_press_during_a_fling_stops_it_without_clicking() {
    let state = LazyListState::new(0, 0);
    let clicks = Rc::new(Cell::new(0));
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 500), list(*state, clicks.clone()));
    ui.set_auto_advance(false);
    swipe_up(&mut ui, 100.0);
    ui.advance_time_by(Duration::from_millis(100));
    let at = Offset::new(100.0, 200.0);
    ui.host().send_pointer(touch(PointerEvent::press(at)));
    ui.host().send_pointer(touch(PointerEvent::release(at)));
    ui.advance_time_by_frame();
    let stopped = state.first_visible_item_index();
    ui.advance_time_by(Duration::from_millis(1000));
    assert_eq!(state.first_visible_item_index(), stopped, "the fling stopped at the press");
    assert_eq!(clicks.get(), 0, "the press only stopped the fling");
    // A press when nothing moves clicks.
    ui.host().send_pointer(touch(PointerEvent::press(at)));
    ui.host().send_pointer(touch(PointerEvent::release(at)));
    ui.advance_time_by_frame();
    assert_eq!(clicks.get(), 1);
}
