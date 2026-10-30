//! Review of D241: the drawer's back handler against handlers in its sheet.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::{MutableState, composable, mutable_state_of};
use compose_test::*;
use compose_ui::{BackHandler, IntSize, Modifier, Offset, PointerEvent};

#[composable]
fn AppWithSheetHandler(state: DrawerState, log: MutableState<u32>) {
    MaterialTheme!({
        ModalNavigationDrawer!(
            {
                ModalDrawerSheet!(
                    drawer_state = Some(state),
                    modifier = Modifier.test_tag("sheet"),
                    {
                        // E.g. a search field or nested navigation inside the drawer.
                        BackHandler!(on_back = move || log.update(|count| count + 1));
                        Text!("Inbox");
                    }
                );
            },
            drawer_state = state,
            {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
}

/// Compose's `DrawerPredictiveBackHandler` is composed before the sheet's content, so a
/// handler inside the open drawer gets back before the drawer closes.
#[test]
fn a_handler_inside_the_open_drawer_goes_before_the_drawer() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let owner = mutable_state_of(0u32);
    let log = owner.handle();
    let given = *state;
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || AppWithSheetHandler(given, log));
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(16));
    assert!(state.is_open());
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(Duration::from_millis(400));
    assert_eq!(log.get(), 1, "the sheet's handler didn't get back");
    assert!(state.is_open(), "the drawer closed over its content's handler");
}

#[composable]
fn OwnDrawer() {
    let state = remember_drawer_state!(DrawerValue::Open);
    MaterialTheme!({
        ModalNavigationDrawer!(
            {
                ModalDrawerSheet!(
                    drawer_state = Some(state),
                    modifier = Modifier.test_tag("sheet"),
                    {
                        Text!("Inbox");
                    }
                );
            },
            drawer_state = state,
            {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
}

/// The host goes (an Android view destroyed) mid-drag: the drag's stop settles the drawer
/// through the scope; nothing may run on the freed drawer afterwards.
#[test]
fn a_host_disposed_mid_drag_leaves_no_task_on_a_freed_drawer() {
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), OwnDrawer);
    ui.set_auto_advance(false);
    let start = Offset::new(300.0, 500.0);
    ui.host().send_pointer(PointerEvent::press(start));
    for x in [280.0, 200.0, 100.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, 500.0), true));
        ui.advance_time_by_frame();
    }
    drop(ui);
    for _ in 0..10 {
        compose_runtime::platform::fire_timers();
        compose_runtime::platform::run_ready();
    }
}
