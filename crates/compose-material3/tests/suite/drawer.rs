//! `ModalNavigationDrawer`: opening and closing, the scrim, drags, and drawer items.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::{MutableState, composable, mutable_state_of, remember_coroutine_scope};
use compose_test::*;
use compose_ui::{BackHandler, IntSize, Modifier, Offset, PointerEvent};

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
            drawer_state = state,
            {
                Box!(modifier = Modifier.fill_max_size(), {
                    Button!(
                        move || {
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
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || App(given));
    ui.set_auto_advance(false);
    ui.on_node_with_tag("sheet").assert_is_not_displayed();
    ui.on_node_with_tag("open").perform_click();
    ui.advance_time_by(Duration::from_millis(16 * 4));
    let sliding = sheet_left(&mut ui);
    assert!(sliding < 0.0, "{sliding}");
    ui.advance_time_by(Duration::from_millis(400));
    assert_eq!(sheet_left(&mut ui), 0.0);
    assert!(state.is_open());
    let sheet = ui.on_node_with_tag("sheet").fetch_semantics_node().bounds;
    assert_eq!(sheet.width(), 360.0);
    assert_eq!(
        ui.on_node_with_tag("inbox")
            .fetch_semantics_node()
            .config
            .get(&compose_ui::SemanticsProperties::SELECTED)
            .copied(),
        Some(true)
    );
    // The scrim, beside the sheet, closes it.
    ui.host().send_pointer(PointerEvent::press(Offset::new(600.0, 300.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(600.0, 300.0)));
    ui.advance_time_by(Duration::from_millis(400));
    ui.on_node_with_tag("sheet").assert_is_not_displayed();
    assert!(state.is_closed());
}

#[test]
fn dragging_the_sheet_to_the_start_closes_it() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || App(given));
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

#[test]
fn back_closes_an_open_drawer_and_passes_through_a_closed_one() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), move || App(given));
    ui.set_auto_advance(false);
    // Closed: back isn't taken (Android leaves the app).
    assert!(!ui.host().has_enabled_back_handlers());
    assert!(!ui.host().on_back_pressed());
    ui.on_node_with_tag("open").perform_click();
    ui.advance_time_by(Duration::from_millis(400));
    assert!(state.is_open());
    assert!(ui.host().has_enabled_back_handlers());
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(Duration::from_millis(400));
    assert!(state.is_closed());
    ui.on_node_with_tag("sheet").assert_is_not_displayed();
    assert!(!ui.host().has_enabled_back_handlers());
}

#[test]
fn an_open_drawer_stays_open_in_a_new_host() {
    // `remember_drawer_state` is saveable, as Compose's (D293).
    let content = || {
        let state = remember_drawer_state!(DrawerValue::Closed);
        App(state);
    };
    let mut ui = ComposeTestRule::sized(IntSize::new(800, 600), content);
    ui.on_node_with_tag("open").perform_click();
    ui.on_node_with_tag("sheet").assert_exists();
    let saved = ui.host().save_state();
    drop(ui);
    let host = compose_ui::platform::UiHost::headless(800, 600);
    host.restore_state(saved);
    let mut ui = ComposeTestRule::with_host(host, content);
    ui.on_node_with_tag("sheet").assert_exists();
    assert!(ui.host().has_enabled_back_handlers());
}

#[composable]
fn AppWithBackHandler(state: DrawerState, log: MutableState<u32>) {
    MaterialTheme!({
        ModalNavigationDrawer!(
            {
                ModalDrawerSheet!(modifier = Modifier.test_tag("sheet"), {
                    Text!("Inbox");
                });
            },
            drawer_state = state,
            {
                BackHandler!(on_back = move || log.update(|count| count + 1));
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
}

/// The content's own handler gets back while the drawer is closed; the open drawer's goes
/// first, as Compose's drawer sheet's handler (composed after the content).
#[test]
fn an_open_drawer_takes_back_before_the_content() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let owner = mutable_state_of(0u32);
    let log = owner.handle();
    let given = *state;
    let mut ui =
        ComposeTestRule::sized(IntSize::new(800, 600), move || AppWithBackHandler(given, log));
    ui.set_auto_advance(false);
    assert!(ui.host().on_back_pressed());
    assert_eq!(log.get(), 1);
    state.snap_to(DrawerValue::Open);
    ui.advance_time_by(Duration::from_millis(16));
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(Duration::from_millis(400));
    assert_eq!(log.get(), 1, "the content got back while the drawer was open");
    assert!(state.is_closed());
}

/// The scrim's semantics follow the drawer (kept per node, D1257): there while it's open, as
/// Compose's `Scrim(onClick = if (drawerState.isOpen) onDismissRequest else null)`; with gestures
/// off, a click that does nothing, its `onDismissRequest` asking them.
#[test]
fn the_scrims_semantics_follow_the_drawer() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let drawer = *state;
    let gestures = mutable_state_of(true);
    let handle = gestures.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        MaterialTheme!({
            ModalNavigationDrawer!(
                || {
                    Text!("The sheet");
                },
                drawer_state = drawer,
                gestures_enabled = handle.get(),
                {
                    Text!("The content");
                }
            );
        });
    });
    let scrim = "Close navigation menu";
    ui.on_node_with_content_description(scrim).assert_does_not_exist();
    compose_runtime::platform::launch(async move { drawer.open().await });
    ui.advance_time_by(std::time::Duration::from_millis(50));
    ui.on_node_with_text("The sheet").assert_exists();
    ui.on_node_with_content_description(scrim).assert_does_not_exist();
    ui.advance_time_by(std::time::Duration::from_secs(1));
    ui.on_node_with_content_description(scrim).assert_has_click_action();
    gestures.set(false);
    ui.advance_time_by_frame();
    ui.on_node_with_content_description(scrim)
        .assert_has_click_action()
        .perform_semantics_action(&compose_ui::SemanticsActions::ON_CLICK);
    ui.advance_time_by(std::time::Duration::from_secs(1));
    assert_eq!(state.current_value(), DrawerValue::Open);
    gestures.set(true);
    ui.advance_time_by_frame();
    ui.on_node_with_content_description(scrim)
        .perform_semantics_action(&compose_ui::SemanticsActions::ON_CLICK);
    ui.advance_time_by(std::time::Duration::from_secs(1));
    assert_eq!(state.current_value(), DrawerValue::Closed);
    ui.on_node_with_content_description(scrim).assert_does_not_exist();
}
