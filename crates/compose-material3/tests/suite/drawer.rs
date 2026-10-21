//! `ModalNavigationDrawer`: opening and closing, the scrim, drags, and drawer items.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::{composable, remember_coroutine_scope};
use compose_test::*;
use compose_ui::{IntSize, Modifier, Offset, PointerEvent};

#[composable]
fn App(state: DrawerState) {
    let scope = remember_coroutine_scope();
    MaterialTheme!({
        ModalNavigationDrawer!(
            {
                ModalDrawerSheet!(modifier = Modifier.test_tag("sheet"), {
                    NavigationDrawerItem!(
                        { Text!("Inbox") },
                        true,
                        || {},
                        modifier = Modifier.test_tag("inbox")
                    );
                    NavigationDrawerItem!({ Text!("Outbox") }, false, || {});
                });
            },
            drawer_state = state.clone(),
            {
                Box!(modifier = Modifier.fill_max_size(), {
                    let (state, scope) = (state.clone(), scope.clone());
                    Button!(
                        move || {
                            let state = state.clone();
                            scope.launch(async move { state.open().await });
                        },
                        modifier = Modifier.test_tag("open"),
                        { Text!("Open") }
                    );
                });
            }
        );
    });
}

fn sheet_left(ui: &mut ComposeTestRule) -> f32 {
    ui.on_node_with_tag("sheet").fetch_semantics_node().bounds.left
}

#[test]
fn the_drawer_slides_in_and_the_scrim_closes_it() {
    let state = DrawerState::new(DrawerValue::Closed);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || App(given.clone()));
    ui.set_auto_advance(false);
    ui.on_node_with_tag("sheet").assert_does_not_exist();
    ui.on_node_with_tag("open").perform_click();
    ui.advance_time_by(Duration::from_millis(16 * 4));
    let sliding = sheet_left(&mut ui);
    assert!(sliding < 0.0, "{sliding}");
    ui.advance_time_by(Duration::from_millis(400));
    assert_eq!(sheet_left(&mut ui), 0.0);
    assert!(state.is_open());
    let sheet = ui.on_node_with_tag("sheet").fetch_semantics_node().bounds;
    assert_eq!(sheet.width(), 360.0);
    assert_eq!(ui.on_node_with_tag("inbox").fetch_semantics_node().config.selected, Some(true));
    // The scrim, beside the sheet, closes it.
    ui.host().send_pointer(PointerEvent::press(Offset::new(600.0, 300.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(600.0, 300.0)));
    ui.advance_time_by(Duration::from_millis(400));
    ui.on_node_with_tag("sheet").assert_does_not_exist();
    assert!(state.is_closed());
}

#[test]
fn dragging_the_sheet_to_the_start_closes_it() {
    let state = DrawerState::new(DrawerValue::Open);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || App(given.clone()));
    ui.set_auto_advance(false);
    assert_eq!(sheet_left(&mut ui), 0.0);
    let start = Offset::new(300.0, 500.0);
    ui.host().send_pointer(PointerEvent::press(start));
    for x in [280.0, 200.0, 100.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, 500.0), true));
        ui.advance_time_by_frame();
    }
    let dragged = sheet_left(&mut ui);
    assert!(dragged < -150.0, "{dragged}");
    ui.host().send_pointer(PointerEvent::release(Offset::new(100.0, 500.0)));
    ui.advance_time_by(Duration::from_millis(400));
    assert!(state.is_closed());
}
