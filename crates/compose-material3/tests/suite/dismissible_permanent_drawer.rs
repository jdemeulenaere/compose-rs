//! Compose's `DismissibleNavigationDrawerTest.kt` and `PermanentNavigationDrawerTest.kt`, adapted. A
//! closed dismissible drawer's sheet isn't placed, so isn't a node here (D1027): where Compose's
//! tests ask an unplaced node, these ask the state or that it isn't there.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Density, IntSize, Key, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Offset,
    SemanticsActions, SemanticsProperties, UnitExt, platform::FixedTextMeasurer,
};

const SHEET: &str = "sheet";
const CONTENT: &str = "content";
const BODY: &str = "body";

/// A rule of 600 by 600 with a dismissible drawer.
fn dismissible(state: DrawerState, width: Option<f32>) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(600, 600), move || {
        MaterialTheme!({
            DismissibleNavigationDrawer!(
                move || {
                    let modifier = match width {
                        Some(width) => Modifier.test_tag(SHEET).width(width.dp()),
                        None => Modifier.test_tag(SHEET),
                    };
                    DismissibleDrawerSheet!(modifier = modifier, |_: ColumnScope| {
                        Box!(modifier = Modifier.fill_max_size().test_tag(CONTENT));
                    });
                },
                drawer_state = state,
                || {
                    Box!(modifier = Modifier.fill_max_size().test_tag(BODY));
                }
            );
        });
    })
}

fn left(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds.left
}

fn run(future: impl Future<Output = ()> + 'static, ui: &mut ComposeTestRule) {
    compose_runtime::platform::launch(future);
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_testOffset_whenOpen
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_testOffset_customWidthSmaller_whenOpen
#[test]
fn open_its_sheet_is_at_the_start_and_the_content_after_it() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = dismissible(*state, None);
    assert_eq!(left(&mut ui, CONTENT), 0.0);
    assert_eq!(left(&mut ui, BODY), 360.0);
    ui.on_node_unmerged(has_test_tag(CONTENT)).assert_width_is_equal_to(360.dp());
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = dismissible(*state, Some(300.0));
    assert_eq!(left(&mut ui, CONTENT), 0.0);
    assert_eq!(left(&mut ui, BODY), 300.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_testOffset_whenClosed
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_testOffset_customWidthSmaller_whenClosed
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_testOffset_customWidthLarger_whenClosed
#[test]
fn closed_the_body_is_at_the_start_and_the_sheet_a_width_off() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let mut ui = dismissible(*state, None);
    assert_eq!(left(&mut ui, BODY), 0.0);
    assert_eq!(state.current_offset(), -360.0);
    // Not placed: nothing's there to take the focus or be read.
    ui.on_node_unmerged(has_test_tag(CONTENT)).assert_does_not_exist();
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let mut ui = dismissible(*state, Some(300.0));
    assert_eq!(left(&mut ui, BODY), 0.0);
    assert_eq!(state.current_offset(), -300.0);
}

/// Wider than the sheet's 360 dp, on a low density: its width's the maximum, as `sizeIn`'s.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_testOffset_customWidthLarger_whenOpen
#[test]
fn wider_than_its_most_its_sheet_is_still_at_the_start() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let handle = *state;
    let mut ui = compose_test_with(
        IntSize::new(600, 600),
        Density { density: 0.5, font_scale: 1.0 },
        Box::new(FixedTextMeasurer::new()),
        move || {
            MaterialTheme!({
                DismissibleNavigationDrawer!(
                    || {
                        DismissibleDrawerSheet!(
                            modifier = Modifier.width(380.dp()),
                            |_: ColumnScope| {
                                Box!(modifier = Modifier.fill_max_size().test_tag(CONTENT));
                            }
                        );
                    },
                    drawer_state = handle,
                    || {}
                );
            });
        },
    );
    assert_eq!(left(&mut ui, CONTENT), 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_sheet_respectsContentPadding
#[test]
fn a_dismissible_sheet_takes_its_insets() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        MaterialTheme!({
            DismissibleNavigationDrawer!(
                || {
                    DismissibleDrawerSheet!(
                        window_insets = WindowInsets::new(7.dp(), 7.dp(), 7.dp(), 7.dp()),
                        |_: ColumnScope| {
                            Box!(modifier = Modifier.fill_max_size().test_tag(CONTENT));
                        }
                    );
                },
                drawer_state = handle,
                || {}
            );
        });
    });
    let bounds = ui.on_node_unmerged(has_test_tag(CONTENT)).fetch_semantics_node().bounds;
    assert_eq!((bounds.left, bounds.top), (7.0, 7.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_testWidth_whenOpen
#[test]
fn its_sheet_is_360_dp_wide() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = dismissible(*state, None);
    ui.on_node_unmerged(has_test_tag(SHEET)).assert_width_is_equal_to(360.dp());
    assert_eq!(DrawerDefaults::MAXIMUM_DRAWER_WIDTH, 360.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_hasPaneTitle
#[test]
fn its_pane_has_a_title() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = dismissible(*state, None);
    let pane = ui.on_node_unmerged(has_test_tag(SHEET)).on_parent().fetch_semantics_node();
    assert_eq!(
        pane.config.get(&SemanticsProperties::PANE_TITLE).map(|title| title.to_string()),
        Some("Navigation menu".to_string())
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_openAndClose
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_currentValue
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_animateTo
#[test]
fn it_opens_and_closes() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = dismissible(handle, None);
    assert_eq!(state.current_value(), DrawerValue::Closed);
    run(async move { handle.open().await }, &mut ui);
    assert_eq!(left(&mut ui, CONTENT), 0.0);
    assert_eq!(left(&mut ui, BODY), 360.0);
    assert_eq!(state.current_value(), DrawerValue::Open);
    run(async move { handle.close().await }, &mut ui);
    assert_eq!(left(&mut ui, BODY), 0.0);
    assert_eq!(state.current_value(), DrawerValue::Closed);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_snapTo
#[test]
fn it_snaps() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = dismissible(handle, None);
    handle.snap_to(DrawerValue::Open);
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, BODY), 360.0);
    handle.snap_to(DrawerValue::Closed);
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, BODY), 0.0);
}

/// Open, a click on the sheet stays on it; closed, on the body.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_drawerContent_doesntPropagateClicksWhenOpen
#[test]
fn a_click_on_an_open_sheet_doesnt_reach_the_body() {
    let clicks = Rc::new(Cell::new(0));
    let heard = clicks.clone();
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let heard = heard.clone();
        MaterialTheme!({
            let heard = heard.clone();
            DismissibleNavigationDrawer!(
                || {
                    DismissibleDrawerSheet!(|_: ColumnScope| {
                        Box!(modifier = Modifier.fill_max_size().test_tag(CONTENT));
                    });
                },
                drawer_state = handle,
                move || {
                    let heard = heard.clone();
                    Box!(
                        modifier = Modifier
                            .fill_max_size()
                            .test_tag(BODY)
                            .clickable(move || heard.set(heard.get() + 1))
                    );
                }
            );
        });
    });
    ui.on_node_with_tag(BODY).perform_click();
    assert_eq!(clicks.get(), 1);
    run(async move { handle.open().await }, &mut ui);
    ui.on_node_unmerged(has_test_tag(CONTENT)).perform_touch_input(|touch| {
        let center = touch.center();
        touch.click_at(Offset::new(1.0, center.y));
    });
    ui.wait_for_idle();
    assert_eq!(clicks.get(), 1);
}

fn swipe(ui: &mut ComposeTestRule, from: f32, by: f32) {
    let tag = BODY;
    ui.on_node_unmerged(has_test_tag(tag)).perform_touch_input(|touch| {
        let y = touch.center().y;
        touch.down(Offset::new(from, y));
        for _ in 0..10 {
            touch.move_by(Offset::new(by / 10.0, 0.0));
        }
        touch.up();
    });
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_openBySwipe
#[test]
fn a_swipe_opens_and_closes_it() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let mut ui = dismissible(*state, None);
    swipe(&mut ui, 10.0, 500.0);
    assert_eq!(state.current_value(), DrawerValue::Open);
    swipe(&mut ui, 200.0, -180.0);
    assert_eq!(state.current_value(), DrawerValue::Closed);
}

/// Midway through a drag, the sheet's partly out, the body right after it.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, DismissibleNavigationDrawer
#[test]
fn midway_the_body_follows_the_sheet() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let mut ui = dismissible(*state, None);
    ui.on_node_unmerged(has_test_tag(BODY)).perform_touch_input(|touch| {
        let y = touch.center().y;
        touch.down(Offset::new(10.0, y));
        touch.move_by(Offset::new(50.0, 0.0));
        touch.move_by(Offset::new(100.0, 0.0));
    });
    ui.wait_for_idle();
    let sheet = left(&mut ui, SHEET);
    assert!(sheet < 0.0 && sheet > -360.0, "{sheet}");
    assert_eq!(left(&mut ui, BODY), sheet + 360.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_confirmStateChangeRespect
#[test]
fn a_swipe_goes_by_confirm_state_change() {
    let state = DrawerState::new(DrawerValue::Open, |value| value != DrawerValue::Closed);
    let mut ui = dismissible(*state, None);
    swipe(&mut ui, 200.0, -180.0);
    assert_eq!(state.current_value(), DrawerValue::Open);
    assert_eq!(left(&mut ui, BODY), 360.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_openBySwipe_rtl
#[test]
fn right_to_left_a_swipe_goes_the_other_way() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
            MaterialTheme!({
                DismissibleNavigationDrawer!(
                    || {
                        DismissibleDrawerSheet!(|_: ColumnScope| {
                            Box!(modifier = Modifier.fill_max_size());
                        });
                    },
                    drawer_state = handle,
                    || {
                        Box!(modifier = Modifier.fill_max_size().test_tag(BODY));
                    }
                );
            });
        });
    });
    swipe(&mut ui, 590.0, -500.0);
    assert_eq!(state.current_value(), DrawerValue::Open);
    // Pushed to the left, off the sheet at the right.
    let bounds = ui.on_node_unmerged(has_test_tag(BODY)).fetch_semantics_node().bounds;
    assert_eq!(bounds.right, 600.0 - 360.0);
    // Its coordinates: from 100 in the window, the body starting 360 before.
    swipe(&mut ui, 460.0, 180.0);
    assert_eq!(state.current_value(), DrawerValue::Closed);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_noDismissActionWhenClosed_hasDismissActionWhenOpen
#[test]
fn it_can_be_dismissed_while_open() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = dismissible(handle, None);
    // Closed, the pane isn't placed: nothing to dismiss.
    ui.on_node_unmerged(has_test_tag(SHEET)).assert_does_not_exist();
    run(async move { handle.open().await }, &mut ui);
    let pane = ui.on_node_unmerged(has_test_tag(SHEET)).on_parent().fetch_semantics_node();
    assert!(pane.config.contains(&SemanticsActions::DISMISS));
    ui.on_node_unmerged(has_test_tag(SHEET))
        .on_parent()
        .perform_semantics_action(&SemanticsActions::DISMISS);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), DrawerValue::Closed);
    // And it asks first.
    let state = DrawerState::new(DrawerValue::Open, |value| value != DrawerValue::Closed);
    let mut ui = dismissible(*state, None);
    ui.on_node_unmerged(has_test_tag(SHEET))
        .on_parent()
        .perform_semantics_action(&SemanticsActions::DISMISS);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), DrawerValue::Open);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DismissibleNavigationDrawerTest.kt, dismissibleNavigationDrawer_drawerIsFocused_whenOpened
#[test]
fn opened_the_focus_goes_in_and_escape_closes_it() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        MaterialTheme!({
            DismissibleNavigationDrawer!(
                || {
                    DismissibleDrawerSheet!(|_: ColumnScope| {
                        Button!(|| {}, modifier = Modifier.test_tag(CONTENT).focusable(), {});
                    });
                },
                drawer_state = handle,
                || {
                    Button!(|| {}, modifier = Modifier.test_tag(BODY).focusable(), {});
                }
            );
        });
    });
    ui.on_node_with_tag(BODY).request_focus();
    ui.wait_for_idle();
    ui.on_node_with_tag(BODY).assert_is_focused();
    run(async move { handle.open().await }, &mut ui);
    ui.on_node_with_tag(CONTENT).assert_is_focused();
    ui.press_key(Key::Escape);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), DrawerValue::Closed);
}

/// A sheet given the state takes back while it's open; one that isn't, not.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, DrawerPredictiveBackHandler
#[test]
fn a_sheet_with_its_state_takes_back() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        MaterialTheme!({
            DismissibleNavigationDrawer!(
                move || {
                    DismissibleDrawerSheet!(drawer_state = Some(handle), |_: ColumnScope| {});
                },
                drawer_state = handle,
                || {}
            );
        });
    });
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert_eq!(state.current_value(), DrawerValue::Closed);
    assert!(!ui.host().on_back_pressed());
}

// ---- Permanent ---------------------------------------------------------------------------------

fn permanent(width: Option<f32>, direction: LayoutDirection) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(600, 600), move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            MaterialTheme!({
                PermanentNavigationDrawer!(
                    move || {
                        let modifier = match width {
                            Some(width) => Modifier.test_tag(SHEET).width(width.dp()),
                            None => Modifier.test_tag(SHEET),
                        };
                        PermanentDrawerSheet!(modifier = modifier, |_: ColumnScope| {
                            Box!(modifier = Modifier.fill_max_size().test_tag(CONTENT));
                        });
                    },
                    || {
                        Box!(modifier = Modifier.fill_max_size().test_tag(BODY));
                    }
                );
            });
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/PermanentNavigationDrawerTest.kt, permanentNavigationDrawer_testOffset
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/PermanentNavigationDrawerTest.kt, permanentNavigationDrawer_testWidth
#[test]
fn a_permanent_drawer_is_360_dp_at_the_start() {
    let mut ui = permanent(None, LayoutDirection::Ltr);
    assert_eq!(left(&mut ui, CONTENT), 0.0);
    assert_eq!(left(&mut ui, BODY), 360.0);
    ui.on_node_unmerged(has_test_tag(CONTENT)).assert_width_is_equal_to(360.dp());
}

/// It fills what it's given, whatever its content wants.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, PermanentNavigationDrawer
#[test]
fn a_permanent_drawer_fills_its_room() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            Box!({
                PermanentNavigationDrawer!(
                    || {
                        Box!(modifier = Modifier.size(10.dp()));
                    },
                    modifier = Modifier.test_tag(SHEET),
                    || {}
                );
            });
        });
    });
    let bounds = ui.on_node_unmerged(has_test_tag(SHEET)).fetch_semantics_node().bounds;
    assert_eq!((bounds.width(), bounds.height()), (600.0, 600.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/PermanentNavigationDrawerTest.kt, permanentNavigationDrawer_testOffset_rtl
#[test]
fn right_to_left_a_permanent_drawer_is_at_the_right() {
    let mut ui = permanent(None, LayoutDirection::Rtl);
    assert_eq!(left(&mut ui, CONTENT), 600.0 - 360.0);
    assert_eq!(left(&mut ui, BODY), 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/PermanentNavigationDrawerTest.kt, permanentNavigationDrawer_testCustomWidth
#[test]
fn a_permanent_drawer_takes_its_width() {
    let mut ui = permanent(Some(260.0), LayoutDirection::Ltr);
    ui.on_node_unmerged(has_test_tag(CONTENT)).assert_width_is_equal_to(260.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/PermanentNavigationDrawerTest.kt, permanentNavigationDrawer_sheet_respectsContentPadding
#[test]
fn a_permanent_sheet_takes_its_insets() {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), || {
        MaterialTheme!({
            PermanentNavigationDrawer!(
                || {
                    PermanentDrawerSheet!(
                        window_insets = WindowInsets::new(13.dp(), 13.dp(), 13.dp(), 13.dp()),
                        |_: ColumnScope| {
                            Box!(modifier = Modifier.fill_max_size().test_tag(CONTENT));
                        }
                    );
                },
                || {}
            );
        });
    });
    let bounds = ui.on_node_unmerged(has_test_tag(CONTENT)).fetch_semantics_node().bounds;
    assert_eq!((bounds.left, bounds.top), (13.0, 13.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/PermanentNavigationDrawerTest.kt, permanentNavigationDrawer_hasPaneTitle
#[test]
fn a_permanent_sheet_has_a_pane_title() {
    let mut ui = permanent(None, LayoutDirection::Ltr);
    let sheet = ui.on_node_unmerged(has_test_tag(SHEET)).fetch_semantics_node();
    assert_eq!(
        sheet.config.get(&SemanticsProperties::PANE_TITLE).map(|title| title.to_string()),
        Some("Navigation menu".to_string())
    );
}

/// The sheets' tokens: square on the surface color, no elevation.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, DrawerDefaults
#[test]
fn the_defaults_are_the_tokens() {
    assert_eq!(DrawerDefaults::MODAL_DRAWER_ELEVATION, 0.dp());
    assert_eq!(DrawerDefaults::DISMISSIBLE_DRAWER_ELEVATION, 0.dp());
    assert_eq!(DrawerDefaults::PERMANENT_DRAWER_ELEVATION, 0.dp());
    let _ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        MaterialTheme!({
            assert_eq!(
                DrawerDefaults::standard_container_color(),
                MaterialTheme::color_scheme().surface
            );
        });
    });
}
