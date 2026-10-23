//! Review of D479 (d301767): a lazy list's rest past an estimated end, offered to an app bar's
//! nested scrolling after the layout.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::{LazyColumn, LazyListScope, LazyListState};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, Offset, PointerEvent, PointerId, PointerType, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

fn height(ui: &mut ComposeTestRule) -> f32 {
    bounds(ui, "bar").height()
}

/// A scaffold whose content is a `LazyColumn` of 40 px rows under a medium bar of `kind` (1
/// enter-always, 2 exit-until-collapsed).
#[composable]
fn Screen(kind: u8, state: LazyListState) {
    MaterialTheme!({
        let behavior = match kind {
            1 => TopAppBarDefaults::enter_always_scroll_behavior(),
            _ => TopAppBarDefaults::exit_until_collapsed_scroll_behavior(),
        };
        Scaffold!(
            modifier = Modifier.nested_scroll(behavior.nested_scroll_connection()),
            top_bar = {
                MediumTopAppBar!(
                    { Text!("Title") },
                    modifier = Modifier.test_tag("bar"),
                    scroll_behavior = Some(behavior.clone())
                );
            },
            move |padding: PaddingValues| {
                LazyColumn!(
                    modifier = Modifier.padding(padding).fill_max_size(),
                    state = state,
                    |scope: LazyListScope| {
                        scope.items(200, |index| {
                            Text!("Item {}", index, modifier = Modifier.height(40.dp()));
                        });
                    }
                );
            }
        );
    });
}

fn wheel_one_frame(ui: &mut ComposeTestRule, delta: f32) {
    ui.host()
        .send_pointer(PointerEvent::scroll(Offset::new(200.0, 400.0), Offset::new(0.0, delta)));
    ui.host().frame();
}

/// Input: an exit-until-collapsed bar, collapsed (64 px), over a list at row 10 (400 px down,
/// the start not laid out); a wheel turn of 430 px back.
///
/// Compose: the list moves 400 px to its top; its post-scroll's 30 px expand the bar to 94 px,
/// in the frame.
#[test]
fn a_bar_expands_by_what_the_list_left_in_the_frame() {
    let state = LazyListState::new(10, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || Screen(2, given));
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    wheel_one_frame(&mut ui, 100.0);
    assert_eq!(height(&mut ui), 64.0);
    assert_eq!(state.first_visible_item_index(), 10);
    wheel_one_frame(&mut ui, -430.0);
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            height(&mut ui)
        ),
        (0, 0, 94.0)
    );
    // Nothing more comes later (no double consumption).
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(height(&mut ui), 94.0);
}

/// Input: as above with a touch: a drag down by 430 px in one move.
///
/// Compose: as above: the list to its top, the bar 94 px, in the frame.
#[test]
fn a_drag_expands_the_bar_by_what_the_list_left_in_the_frame() {
    let state = LazyListState::new(10, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || Screen(2, given));
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    wheel_one_frame(&mut ui, 100.0);
    let finger = |event: PointerEvent| event.with_pointer(PointerId(1), PointerType::Touch);
    ui.host().send_pointer(finger(PointerEvent::press(Offset::new(200.0, 100.0))));
    ui.host().send_pointer(finger(PointerEvent::moved(Offset::new(200.0, 108.0 + 430.0), true)));
    ui.host().frame();
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            height(&mut ui)
        ),
        (0, 0, 94.0)
    );
    // Then on down by 10 px: the bar expands on, in the event.
    ui.host().send_pointer(finger(PointerEvent::moved(Offset::new(200.0, 548.0), true)));
    ui.host().frame();
    assert_eq!(height(&mut ui), 104.0);
}

/// Input: a collapsed exit-until-collapsed bar, a list at row 30 (1,200 px down); a fast fling
/// back to the top.
///
/// Compose: the list reaches its top, the fling's rest expands the bar, which ends expanded.
#[test]
fn a_fling_back_to_the_top_expands_the_bar() {
    let state = LazyListState::new(30, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || Screen(2, given));
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    wheel_one_frame(&mut ui, 100.0);
    assert_eq!(height(&mut ui), 64.0);
    let finger = |event: PointerEvent, time: u64| {
        let mut event = event.with_pointer(PointerId(1), PointerType::Touch);
        event.time_nanos = time;
        event
    };
    let start = 1_000_000_000u64;
    ui.host().send_pointer(finger(PointerEvent::press(Offset::new(200.0, 100.0)), start));
    for step in 1..=6u64 {
        let y = 100.0 + 8.0 + 40.0 * step as f32;
        ui.host().send_pointer(finger(
            PointerEvent::moved(Offset::new(200.0, y), true),
            start + step * 8_000_000,
        ));
    }
    ui.host().send_pointer(finger(
        PointerEvent::release(Offset::new(200.0, 108.0 + 240.0)),
        start + 7 * 8_000_000,
    ));
    for _ in 0..200 {
        ui.advance_time_by(Duration::from_millis(16));
    }
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            height(&mut ui)
        ),
        (0, 0, 112.0)
    );
}
