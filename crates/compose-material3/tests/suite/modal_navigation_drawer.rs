//! Compose's `ModalNavigationDrawerTest.kt`, adapted: the sheet's offsets and widths, its
//! insets, opening and closing, clicks, swipes (left to right and right to left),
//! `confirm_state_change`, the scrim, the focus on opening and back while sliding. Swipes are a
//! private helper over `send_pointer` (no gesture DSL in the harness yet).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    BackHandler, Color, Density, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier,
    Offset, PointerEvent, SemanticsActions, SemanticsProperties, UnitExt,
    platform::FixedTextMeasurer,
};

const DRAWER_TAG: &str = "drawer";

thread_local! {
    static STATE: Cell<Option<DrawerState>> = const { Cell::new(None) };
    static SCOPE: Cell<Option<CoroutineScope>> = const { Cell::new(None) };
}

fn state() -> DrawerState {
    STATE.with(Cell::get).expect("composed")
}

fn launch(task: impl std::future::Future<Output = ()> + 'static) {
    SCOPE.with(Cell::get).expect("composed").launch(task);
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

/// A drawer remembering its state from `initial`, its sheet `sheet_modifier` wide (360 dp by
/// default) holding a box tagged "content".
fn drawer(initial: DrawerValue, sheet_modifier: Modifier) -> impl Fn() {
    move || {
        SCOPE.with(|cell| cell.set(Some(remember_coroutine_scope())));
        let state = remember_drawer_state!(initial);
        STATE.with(|cell| cell.set(Some(state)));
        let sheet_modifier = sheet_modifier.clone();
        MaterialTheme!({
            let sheet_modifier = sheet_modifier.clone();
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(modifier = sheet_modifier.clone(), |_| {
                        Box!(modifier = Modifier.fill_max_size().test_tag("content"));
                    });
                },
                drawer_state = state,
                {}
            );
        });
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_testOffset_whenOpen
#[test]
fn an_open_drawers_sheet_is_at_the_start() {
    let mut ui = compose_test(drawer(DrawerValue::Open, Modifier));
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_testOffset_whenClosed
#[test]
fn a_closed_drawers_sheet_is_not_displayed() {
    let mut ui = compose_test(drawer(DrawerValue::Closed, Modifier));
    ui.on_node_with_tag("content").assert_is_not_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_testOffset_customWidthSmaller_whenOpen
#[test]
fn an_open_narrow_sheet_is_at_the_start() {
    let mut ui = compose_test(drawer(DrawerValue::Open, Modifier.width(200.dp())));
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_testOffset_customWidthSmaller_whenClosed
#[test]
fn a_closed_narrow_sheet_is_its_width_off_the_start() {
    let _ui = compose_test(drawer(DrawerValue::Closed, Modifier.width(200.dp())));
    assert!((state().current_offset() + 200.0).abs() <= 1.0, "{}", state().current_offset());
}

/// A rule at density 0.5, so a sheet wider than the default fits the 400 px window.
fn at_half_density(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test_with(
        IntSize::new(400, 800),
        Density { density: 0.5, font_scale: 1.0 },
        Box::new(FixedTextMeasurer::new()),
        content,
    )
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_testOffset_customWidthLarger_whenOpen
#[test]
fn an_open_wide_sheet_is_at_the_start() {
    let mut ui = at_half_density(drawer(DrawerValue::Open, Modifier.width(380.dp())));
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_testOffset_customWidthLarger_whenClosed
#[test]
fn a_closed_wide_sheet_is_its_width_off_the_start() {
    let _ui = at_half_density(drawer(DrawerValue::Closed, Modifier.width(380.dp())));
    assert!((state().current_offset() + 190.0).abs() <= 1.0, "{}", state().current_offset());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_testWidth_whenOpen
#[test]
fn an_open_sheet_is_360_dp_wide() {
    let mut ui = compose_test(drawer(DrawerValue::Open, Modifier));
    assert_eq!(bounds(&mut ui, "content").width(), 360.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_sheet_respectsContentPadding
#[test]
fn a_sheets_content_keeps_inside_its_insets() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(
                        window_insets = WindowInsets::new(10.dp(), 10.dp(), 10.dp(), 10.dp()),
                        |_| {
                            Box!(modifier = Modifier.fill_max_size().test_tag("content"));
                        }
                    );
                },
                drawer_state = remember_drawer_state!(DrawerValue::Open),
                {}
            );
        });
    });
    let content = bounds(&mut ui, "content");
    assert_eq!((content.left, content.top), (10.0, 10.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_openAndClose
#[test]
fn open_and_close_slide_the_sheet_in_and_out() {
    let mut ui = compose_test(drawer(DrawerValue::Closed, Modifier));
    ui.on_node_with_tag("content").assert_is_not_displayed();
    let drawer = state();
    launch(async move { drawer.open().await });
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
    launch(async move { drawer.close().await });
    ui.wait_for_idle();
    ui.on_node_with_tag("content").assert_is_not_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_snapTo
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_currentValue
#[test]
fn snap_to_opens_and_closes_at_once() {
    let mut ui = compose_test(drawer(DrawerValue::Closed, Modifier));
    assert_eq!(state().current_value(), DrawerValue::Closed);
    state().snap_to(DrawerValue::Open);
    ui.wait_for_idle();
    assert_eq!(state().current_value(), DrawerValue::Open);
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
    state().snap_to(DrawerValue::Closed);
    ui.wait_for_idle();
    assert_eq!(state().current_value(), DrawerValue::Closed);
    ui.on_node_with_tag("content").assert_is_not_displayed();
}

fn click_at(ui: &mut ComposeTestRule, point: Offset) {
    ui.host().send_pointer(PointerEvent::press(point));
    ui.host().send_pointer(PointerEvent::release(point));
    ui.wait_for_idle();
}

/// A drawer whose sheet and body count their clicks.
fn counting_drawer(drawer_clicks: Rc<Cell<u32>>, body_clicks: Rc<Cell<u32>>) -> impl Fn() {
    move || {
        SCOPE.with(|cell| cell.set(Some(remember_coroutine_scope())));
        let state = remember_drawer_state!(DrawerValue::Closed);
        STATE.with(|cell| cell.set(Some(state)));
        let (drawer_clicks, body_clicks) = (drawer_clicks.clone(), body_clicks.clone());
        MaterialTheme!({
            let (drawer_clicks, body_clicks) = (drawer_clicks.clone(), body_clicks.clone());
            ModalNavigationDrawer!(
                {
                    let drawer_clicks = drawer_clicks.clone();
                    ModalDrawerSheet!(|_| {
                        let drawer_clicks = drawer_clicks.clone();
                        Box!(
                            modifier = Modifier
                                .fill_max_size()
                                .test_tag(DRAWER_TAG)
                                .clickable(move || drawer_clicks.set(drawer_clicks.get() + 1))
                        );
                    });
                },
                drawer_state = state,
                {
                    let body_clicks = body_clicks.clone();
                    Box!(
                        modifier = Modifier
                            .fill_max_size()
                            .test_tag("body")
                            .clickable(move || body_clicks.set(body_clicks.get() + 1))
                    );
                }
            );
        });
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_bodyContent_clickable
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_drawerContent_doesntPropagateClicksWhenOpen
#[test]
fn clicks_reach_the_body_when_closed_and_the_sheet_when_open() {
    let (drawer_clicks, body_clicks) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let mut ui = compose_test(counting_drawer(drawer_clicks.clone(), body_clicks.clone()));
    let body = bounds(&mut ui, "body");
    click_at(&mut ui, Offset::new(body.width() / 2.0, body.height() / 2.0));
    assert_eq!((drawer_clicks.get(), body_clicks.get()), (0, 1));
    let drawer = state();
    launch(async move { drawer.open().await });
    ui.wait_for_idle();
    // The sheet's left-center pixel.
    click_at(&mut ui, Offset::new(1.0, body.height() / 2.0));
    assert_eq!((drawer_clicks.get(), body_clicks.get()), (1, 1));
}

/// Swipes across the window's middle, rightwards or leftwards, in 200 ms, then lets
/// everything settle.
fn swipe(ui: &mut ComposeTestRule, rightwards: bool) {
    let (from, to) = if rightwards { (1.0, 399.0) } else { (399.0, 1.0) };
    let y = 400.0;
    ui.host().send_pointer(PointerEvent::press(Offset::new(from, y)));
    for step in 1..=10 {
        ui.advance_time_by(Duration::from_millis(20));
        let x = from + (to - from) * step as f32 / 10.0;
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(to, y)));
    ui.wait_for_idle();
}

fn swipe_drawer(direction: LayoutDirection, confirm_closing: bool) -> impl Fn() {
    move || {
        SCOPE.with(|cell| cell.set(Some(remember_coroutine_scope())));
        let initial = if confirm_closing { DrawerValue::Closed } else { DrawerValue::Open };
        let state = remember_drawer_state!(
            initial,
            confirm_state_change = move |value| confirm_closing || value != DrawerValue::Closed
        );
        STATE.with(|cell| cell.set(Some(state)));
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            MaterialTheme!({
                ModalNavigationDrawer!(
                    {
                        ModalDrawerSheet!(|_| {
                            Box!(modifier = Modifier.fill_max_size());
                        });
                    },
                    drawer_state = state,
                    {
                        Box!(modifier = Modifier.fill_max_size());
                    }
                );
            });
        });
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_openBySwipe
#[test]
fn a_swipe_towards_the_end_opens_and_back_closes() {
    let mut ui = compose_test(swipe_drawer(LayoutDirection::Ltr, true));
    swipe(&mut ui, true);
    assert_eq!(state().current_value(), DrawerValue::Open);
    swipe(&mut ui, false);
    assert_eq!(state().current_value(), DrawerValue::Closed);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_openBySwipe_rtl
#[test]
fn right_to_left_a_swipe_leftwards_opens_and_back_closes() {
    let mut ui = compose_test(swipe_drawer(LayoutDirection::Rtl, true));
    swipe(&mut ui, false);
    assert_eq!(state().current_value(), DrawerValue::Open);
    swipe(&mut ui, true);
    assert_eq!(state().current_value(), DrawerValue::Closed);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_currentValueUpdatesOnRelease
#[test]
fn the_current_value_changes_only_once_released() {
    let mut ui = compose_test(swipe_drawer(LayoutDirection::Ltr, true));
    swipe(&mut ui, true);
    assert_eq!(state().current_value(), DrawerValue::Open);
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 400.0)));
    for x in [150.0, 100.0, 50.0, 0.0] {
        ui.advance_time_by(Duration::from_millis(16));
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, 400.0), true));
    }
    assert_eq!(state().current_value(), DrawerValue::Open);
    ui.host().send_pointer(PointerEvent::release(Offset::new(0.0, 400.0)));
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(state().current_value(), DrawerValue::Closed);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_confirmStateChangeRespect
#[test]
fn a_drawer_whose_confirmation_refuses_closing_stays_open() {
    let mut ui = compose_test(swipe_drawer(LayoutDirection::Ltr, false));
    swipe(&mut ui, false);
    assert_eq!(state().current_value(), DrawerValue::Open);
    // The scrim, beyond the 360 dp sheet.
    click_at(&mut ui, Offset::new(390.0, 400.0));
    assert_eq!(state().current_value(), DrawerValue::Open);
    // The pane's dismiss action.
    ui.on_node(expect_value(&SemanticsProperties::PANE_TITLE, "Navigation menu".into()))
        .perform_semantics_action(&SemanticsActions::DISMISS);
    ui.wait_for_idle();
    assert_eq!(state().current_value(), DrawerValue::Open);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_scrimNode_reportToSemanticsWhenOpen_notReportToSemanticsWhenClosed
#[test]
fn the_scrim_is_a_clickable_node_only_while_open() {
    let mut ui = compose_test(drawer(DrawerValue::Open, Modifier));
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
    let mut scrim = ui.on_node_with_content_description("Close navigation menu");
    scrim.assert_exists();
    scrim.perform_semantics_action(&SemanticsActions::ON_CLICK);
    ui.wait_for_idle();
    ui.on_node_with_tag("content").assert_is_not_displayed();
    ui.on_node_with_content_description("Close navigation menu").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_drawerIsFocused_whenOpened
#[test]
fn opening_the_drawer_focuses_its_first_focusable() {
    let mut ui = compose_test(|| {
        let scope = remember_coroutine_scope();
        let state = remember_drawer_state!(DrawerValue::Closed);
        STATE.with(|cell| cell.set(Some(state)));
        MaterialTheme!({
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(|_| {
                        Button!(
                            move || {
                                scope.launch(async move { state.close().await });
                            },
                            modifier = Modifier.test_tag(DRAWER_TAG),
                            {}
                        );
                    });
                },
                drawer_state = state,
                {
                    Button!(
                        move || {
                            scope.launch(async move { state.open().await });
                        },
                        modifier = Modifier.test_tag("button"),
                        {}
                    );
                }
            );
        });
    });
    ui.on_node_with_tag("button").request_focus();
    ui.on_node_with_tag("button").assert_is_focused();
    ui.on_node_with_tag("button").perform_click();
    ui.wait_for_idle();
    assert_eq!(state().current_value(), DrawerValue::Open);
    ui.on_node_with_tag(DRAWER_TAG).assert_is_focused();
}

/// A drawer starting `initial`, with a back handler of its own around it, whose sheet's button
/// closes it and whose body's button opens it.
fn back_drawer(initial: DrawerValue, back_pressed: Rc<Cell<bool>>) -> impl Fn() {
    move || {
        let scope = remember_coroutine_scope();
        let state = remember_drawer_state!(initial);
        STATE.with(|cell| cell.set(Some(state)));
        let back_pressed = back_pressed.clone();
        BackHandler(true, callback(move || back_pressed.set(true)));
        MaterialTheme!({
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(drawer_state = Some(state), |_| {
                        Button!(
                            move || {
                                scope.launch(async move { state.close().await });
                            },
                            modifier = Modifier.test_tag(DRAWER_TAG),
                            {}
                        );
                    });
                },
                drawer_state = state,
                {
                    Button!(
                        move || {
                            scope.launch(async move { state.open().await });
                        },
                        modifier = Modifier.test_tag("button"),
                        {}
                    );
                }
            );
        });
    }
}

fn assert_settled(value: DrawerValue) {
    assert_eq!(state().current_value(), value);
    assert_eq!(state().target_value(), value);
    assert!(!state().is_animation_running());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_drawerIsClosed_whenBackPressedDuringOpen
#[test]
fn back_while_opening_closes_the_drawer() {
    let back_pressed = Rc::new(Cell::new(false));
    let mut ui = compose_test(back_drawer(DrawerValue::Closed, back_pressed.clone()));
    assert_settled(DrawerValue::Closed);
    ui.set_auto_advance(false);
    ui.on_node_with_tag("button").perform_click();
    ui.advance_time_by_frame();
    assert_eq!(state().current_value(), DrawerValue::Closed);
    assert_eq!(state().target_value(), DrawerValue::Open);
    assert!(state().is_animation_running());
    ui.host().on_back_pressed();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_settled(DrawerValue::Closed);
    assert!(!back_pressed.get());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_drawerIsClosed_whenBackPressedDuringClose
#[test]
fn back_while_closing_goes_to_the_handler_around() {
    let back_pressed = Rc::new(Cell::new(false));
    let mut ui = compose_test(back_drawer(DrawerValue::Open, back_pressed.clone()));
    assert_settled(DrawerValue::Open);
    ui.set_auto_advance(false);
    ui.on_node_with_tag(DRAWER_TAG).perform_click();
    ui.advance_time_by_frame();
    assert_eq!(state().current_value(), DrawerValue::Open);
    assert_eq!(state().target_value(), DrawerValue::Closed);
    assert!(state().is_animation_running());
    ui.host().on_back_pressed();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_settled(DrawerValue::Closed);
    assert!(back_pressed.get());
}

/// A drawer in `state`, its sheet tagged.
fn drawer_of(state: DrawerState) -> impl Fn() + 'static {
    move || {
        MaterialTheme!({
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(modifier = Modifier.test_tag(DRAWER_TAG), |_| {
                        Box!(modifier = Modifier.fill_max_size());
                    });
                },
                drawer_state = state,
                {
                    Box!(modifier = Modifier.fill_max_size());
                }
            );
        });
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_hasPaneTitle
#[test]
fn a_drawers_pane_has_its_title() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = compose_test(drawer_of(*state));
    ui.on_node_unmerged(has_test_tag(DRAWER_TAG))
        .on_parent()
        .assert(expect_value(&SemanticsProperties::PANE_TITLE, "Navigation menu".into()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_noDismissActionWhenClosed_hasDismissActionWhenOpen
#[test]
fn a_drawer_has_a_dismiss_action_only_while_open() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = compose_test(drawer_of(handle));
    let pane = || expect_value(&SemanticsProperties::PANE_TITLE, "Navigation menu".into());
    ui.on_node_unmerged(pane()).assert(key_not_defined(&SemanticsActions::DISMISS));
    compose_runtime::platform::launch(async move { handle.open().await });
    ui.wait_for_idle();
    ui.on_node_unmerged(pane()).assert(key_is_defined(&SemanticsActions::DISMISS));
    ui.on_node_unmerged(pane()).perform_semantics_action(&SemanticsActions::DISMISS);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), DrawerValue::Closed);
    ui.on_node_unmerged(pane()).assert(key_not_defined(&SemanticsActions::DISMISS));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerTest.kt, navigationDrawer_drawerContainerIsOffScreen_whenClosed
#[test]
fn a_closed_drawers_pane_is_off_screen() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let mut ui = compose_test(drawer_of(*state));
    let bounds = ui
        .on_node_unmerged(expect_value(&SemanticsProperties::PANE_TITLE, "Navigation menu".into()))
        .fetch_semantics_node()
        .bounds;
    assert_eq!(bounds.left, -360.0);
}

/// A drawer with a focusable in its sheet and another in its content.
fn drawer_with_buttons(state: DrawerState) -> impl Fn() + 'static {
    move || {
        MaterialTheme!({
            ModalNavigationDrawer!(
                {
                    ModalDrawerSheet!(modifier = Modifier.test_tag(DRAWER_TAG), |_| {
                        Button!(on_click = || {}, modifier = Modifier.test_tag("in drawer"), {
                            Text!("Drawer");
                        });
                    });
                },
                drawer_state = state,
                {
                    Button!(on_click = || {}, modifier = Modifier.test_tag("in content"), {
                        Text!("Content");
                    });
                }
            );
        });
    }
}

// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, "Only place the drawer when it's visible so that keyboard focus doesn't navigate to an offscreen element"
#[test]
fn keyboard_focus_stays_out_of_a_closed_drawer() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let mut ui = compose_test(drawer_with_buttons(*state));
    ui.on_node_with_tag("in drawer").assert_does_not_exist();
    ui.press_key(compose_ui::Key::Tab);
    ui.on_node_with_tag("in content").assert_is_focused();
    ui.press_key(compose_ui::Key::Tab);
    ui.on_node_with_tag("in content").assert_is_focused();
}

/// Closed after being open, it's out of the focus's way again, its pane still off screen.
#[test]
fn keyboard_focus_stays_out_of_a_drawer_closed_again() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let handle = *state;
    let mut ui = compose_test(drawer_with_buttons(handle));
    ui.on_node_with_tag("in drawer").assert_is_focused();
    compose_runtime::platform::launch(async move { handle.close().await });
    ui.wait_for_idle();
    ui.on_node_with_tag("in drawer").assert_does_not_exist();
    ui.press_key(compose_ui::Key::Tab);
    ui.on_node_with_tag("in content").assert_is_focused();
    let pane = expect_value(&SemanticsProperties::PANE_TITLE, "Navigation menu".into());
    assert_eq!(ui.on_node_unmerged(pane).fetch_semantics_node().bounds.left, -240.0);
}

/// While it's closed its content records nothing: a frame costs what the screen shows.
#[test]
fn a_closed_drawers_content_draws_nothing() {
    let marker = Color::from_rgb(10, 20, 30);
    let drawer = move |state: DrawerState| {
        move || {
            MaterialTheme!({
                ModalNavigationDrawer!(
                    {
                        ModalDrawerSheet!(
                            drawer_container_color = marker,
                            modifier = Modifier.test_tag(DRAWER_TAG),
                            |_| {}
                        );
                    },
                    drawer_state = state,
                    {
                        Box!(modifier = Modifier.fill_max_size());
                    }
                );
            });
        }
    };
    let draws = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().any(|op| match op {
            compose_ui::platform::DrawOp::DrawRect(_, paint)
            | compose_ui::platform::DrawOp::DrawRoundRect(_, _, paint)
            | compose_ui::platform::DrawOp::DrawPath { paint, .. } => paint.color == marker,
            _ => false,
        })
    };
    let closed = DrawerState::new(DrawerValue::Closed, |_| true);
    let mut ui = compose_test(drawer(*closed));
    assert!(!draws(&mut ui), "{:?}", ui.host().display_list().ops);
    let open = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = compose_test(drawer(*open));
    assert!(draws(&mut ui), "{:?}", ui.host().display_list().ops);
}

// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, ModalNavigationDrawer, Scrim(onClick = if (drawerState.isOpen) onDismissRequest else null)
#[test]
fn an_open_drawers_scrim_goes_after_the_content_in_the_traversal() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = compose_test(drawer_of(*state));
    ui.on_node_with_content_description("Close navigation menu")
        .assert(expect_value(&SemanticsProperties::TRAVERSAL_INDEX, 1.0));
}

/// With `gestures_enabled` false a drag moves nothing, opening or closing.
#[test]
fn with_its_gestures_disabled_a_swipe_on_a_modal_drawer_does_nothing() {
    for (value, from, by) in
        [(DrawerValue::Closed, 10.0, 500.0), (DrawerValue::Open, 200.0, -180.0)]
    {
        let state = DrawerState::new(value, |_| true);
        let handle = *state;
        let mut ui = ComposeTestRule::sized(compose_ui::IntSize::new(600, 600), move || {
            MaterialTheme!({
                ModalNavigationDrawer!(
                    || {
                        ModalDrawerSheet!(|_: ColumnScope| {
                            Box!(modifier = Modifier.fill_max_size());
                        });
                    },
                    drawer_state = handle,
                    gestures_enabled = false,
                    || {
                        Box!(modifier = Modifier.fill_max_size().test_tag("body"));
                    }
                );
            });
        });
        ui.on_root().perform_touch_input(|touch| {
            touch.down(compose_ui::Offset::new(from, 300.0));
            for _ in 0..10 {
                touch.move_by(compose_ui::Offset::new(by / 10.0, 0.0));
            }
            touch.up();
        });
        ui.wait_for_idle();
        assert_eq!(state.current_value(), value);
    }
}
