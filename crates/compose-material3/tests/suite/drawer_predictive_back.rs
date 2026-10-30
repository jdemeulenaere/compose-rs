//! The back-progress scenarios of Compose's `ModalNavigationDrawerScreenshotTest.kt`, with no
//! images to compare: the layers' scales in the display list, through a real gesture and so its
//! easing, where Compose's test feeds `DrawerPredictiveBackState` directly. Also how a gesture
//! ends and a drawer overshooting, as `NavigationDrawer.kt` has them.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::Easing;
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{
    BackEventCompat, Color, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, UnitExt,
};

const LEFT: i32 = BackEventCompat::EDGE_LEFT;
const RIGHT: i32 = BackEventCompat::EDGE_RIGHT;
const WINDOW: (i32, i32) = (400, 800);
const SHEET_WIDTH: f32 = 360.0;
const SCRIM: Color = Color::from_argb(0x80102030);

fn manual_clock(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(WINDOW.0, WINDOW.1), content);
    ui.set_auto_advance(false);
    ui
}

fn modal(state: DrawerState, direction: LayoutDirection) -> ComposeTestRule {
    let mut ui = manual_clock(move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
            MaterialTheme!({
                ModalNavigationDrawer!(
                    || {
                        ModalDrawerSheet!(drawer_state = Some(state), |_: ColumnScope| {
                            Box!(modifier = Modifier.fill_max_size().background(Color::RED));
                        });
                    },
                    drawer_state = state,
                    || {
                        Box!(modifier = Modifier.fill_max_size());
                    }
                );
            });
        });
    });
    settle(&mut ui);
    ui
}

fn dismissible(state: DrawerState) -> ComposeTestRule {
    let mut ui = manual_clock(move || {
        MaterialTheme!({
            DismissibleNavigationDrawer!(
                || {
                    DismissibleDrawerSheet!(drawer_state = Some(state), |_: ColumnScope| {
                        Box!(modifier = Modifier.fill_max_size().background(Color::RED));
                    });
                },
                drawer_state = state,
                || {
                    Box!(modifier = Modifier.fill_max_size());
                }
            );
        });
    });
    settle(&mut ui);
    ui
}

fn settle(ui: &mut ComposeTestRule) {
    ui.advance_time_by_frame();
    let mut left = 5_000;
    while ui.host().needs_frame() && left > 0 {
        ui.advance_time_by(Duration::from_millis(16));
        left -= 16;
    }
}

fn gesture(ui: &mut ComposeTestRule, edge: i32, progress: f32) {
    let event = |progress| BackEventCompat { swipe_edge: edge, progress, ..Default::default() };
    ui.host().on_back_started(event(0.0));
    ui.host().on_back_progressed(event(progress));
    settle(ui);
}

/// The scales of the layers that aren't identity, outermost first.
fn scales(ui: &mut ComposeTestRule) -> Vec<(f32, f32)> {
    let ops = ui.host().display_list().ops.clone();
    ops.iter()
        .filter_map(|op| match op {
            DrawOp::Transform { scale, .. } if *scale != (1.0, 1.0) => Some(*scale),
            _ => None,
        })
        .collect()
}

/// The container's scales and the content's, as `predictiveBackDrawerContainer` and
/// `predictiveBackDrawerChild`.
fn expected(progress: f32, grows: bool) -> Vec<(f32, f32)> {
    let eased = Easing::cubic_bezier(0.1, 0.1, 0.0, 1.0).transform(progress);
    let x = if grows { 1.0 + 12.0 * eased / SHEET_WIDTH } else { 1.0 - 24.0 * eased / SHEET_WIDTH };
    let y = 1.0 - 48.0 * eased / WINDOW.1 as f32;
    if progress == 0.0 { vec![] } else { vec![(x, y), (y / x, 1.0)] }
}

#[track_caller]
fn assert_scales(actual: &[(f32, f32)], expected: &[(f32, f32)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} {expected:?}");
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual.0 - expected.0).abs() < 1e-4, "{actual:?} {expected:?}");
        assert!((actual.1 - expected.1).abs() < 1e-4, "{actual:?} {expected:?}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerScreenshotTest.kt, predictiveBack_navigationDrawer_progress0AndSwipeEdgeLeft, predictiveBack_navigationDrawer_progress25AndSwipeEdgeLeft, predictiveBack_navigationDrawer_progress50AndSwipeEdgeLeft, predictiveBack_navigationDrawer_progress75AndSwipeEdgeLeft, predictiveBack_navigationDrawer_progress100AndSwipeEdgeLeft
#[test]
fn the_back_progress_from_the_drawers_edge_stretches_it() {
    for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let state = DrawerState::new(DrawerValue::Open, |_| true);
        let mut ui = modal(*state, LayoutDirection::Ltr);
        gesture(&mut ui, LEFT, progress);
        assert_scales(&scales(&mut ui), &expected(progress, true));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalNavigationDrawerScreenshotTest.kt, predictiveBack_navigationDrawer_progress0AndSwipeEdgeRight, predictiveBack_navigationDrawer_progress25AndSwipeEdgeRight, predictiveBack_navigationDrawer_progress50AndSwipeEdgeRight, predictiveBack_navigationDrawer_progress75AndSwipeEdgeRight, predictiveBack_navigationDrawer_progress100AndSwipeEdgeRight
#[test]
fn the_back_progress_from_the_other_edge_shrinks_it() {
    for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let state = DrawerState::new(DrawerValue::Open, |_| true);
        let mut ui = modal(*state, LayoutDirection::Ltr);
        gesture(&mut ui, RIGHT, progress);
        assert_scales(&scales(&mut ui), &expected(progress, false));
    }
}

/// Right to left, the drawer's edge is the right one.
#[test]
fn right_to_left_the_drawers_edge_is_the_right() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = modal(*state, LayoutDirection::Rtl);
    gesture(&mut ui, RIGHT, 0.5);
    assert_scales(&scales(&mut ui), &expected(0.5, true));
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = modal(*state, LayoutDirection::Rtl);
    gesture(&mut ui, LEFT, 0.5);
    assert_scales(&scales(&mut ui), &expected(0.5, false));
}

/// The same in a dismissible drawer, its `BackHandler` replaced.
#[test]
fn a_dismissible_drawer_stretches_the_same() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = dismissible(*state);
    gesture(&mut ui, LEFT, 0.5);
    assert_scales(&scales(&mut ui), &expected(0.5, true));
    ui.host().on_back_pressed();
    settle(&mut ui);
    assert!(!state.is_open());
    assert_eq!(scales(&mut ui), []);
}

/// Cancelled, it goes back to how it was and stays open.
#[test]
fn a_cancelled_gesture_leaves_it_open_and_unstretched() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = modal(*state, LayoutDirection::Ltr);
    gesture(&mut ui, LEFT, 0.5);
    ui.host().on_back_cancelled();
    settle(&mut ui);
    assert!(state.is_open());
    assert_eq!(scales(&mut ui), []);
}

/// Completed from the drawer's edge, the stretch goes while it closes, so it can hide whole.
#[test]
fn completed_from_the_drawers_edge_it_unstretches_as_it_closes() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = modal(*state, LayoutDirection::Ltr);
    gesture(&mut ui, LEFT, 1.0);
    let before = scales(&mut ui)[0].0;
    ui.host().on_back_pressed();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(32));
    let during = scales(&mut ui);
    assert!(during[0].0 < before, "{during:?}, from {before}");
    // Height goes as the drawer closes, kept by the sheet until it's closed.
    assert!(during[0].1 < 1.0, "{during:?}");
    settle(&mut ui);
    assert!(!state.is_open());
    assert_eq!(state.current_offset(), -SHEET_WIDTH);
}

/// From the other edge the shrink stays until it's closed, then it's cleared for the next time.
#[test]
fn completed_from_the_other_edge_the_shrink_stays_until_it_is_closed() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let mut ui = modal(*state, LayoutDirection::Ltr);
    gesture(&mut ui, RIGHT, 1.0);
    let before = scales(&mut ui)[0];
    ui.host().on_back_pressed();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(32));
    assert_eq!(scales(&mut ui)[0], before);
    settle(&mut ui);
    assert!(!state.is_open());
    compose_runtime::platform::launch({
        let state = *state;
        async move { state.open().await }
    });
    settle(&mut ui);
    assert!(state.is_open());
    assert_eq!(scales(&mut ui), []);
}

/// An expressive spring goes past open: the sheet's start edge stays covered by scaling up from
/// its other edge, the content scaled back, as `horizontalScaleUp` and `horizontalScaleDown`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/NavigationDrawer.kt, horizontalScaleUp, horizontalScaleDown
#[test]
fn an_overshooting_drawer_scales_up_and_its_content_back_down() {
    let state = DrawerState::new(DrawerValue::Closed, |_| true);
    let handle = *state;
    let mut ui = manual_clock(move || {
        MaterialTheme!(motion_scheme = MotionScheme::expressive(), {
            ModalNavigationDrawer!(
                || {
                    ModalDrawerSheet!(
                        drawer_state = Some(handle),
                        modifier = Modifier.test_tag("sheet"),
                        |_: ColumnScope| {
                            Box!(modifier = Modifier.fill_max_size().background(Color::RED));
                        }
                    );
                },
                drawer_state = handle,
                scrim_color = SCRIM,
                || {
                    Box!(modifier = Modifier.fill_max_size());
                }
            );
        });
    });
    settle(&mut ui);
    compose_runtime::platform::launch(async move { handle.open().await });
    let mut most = 0.0_f32;
    let mut left = 2_000;
    while left > 0 {
        ui.advance_time_by(Duration::from_millis(16));
        left -= 16;
        let offset = state.current_offset();
        if offset > 0.0 {
            // It's placed there, and the scrim's no darker for that.
            let left =
                ui.on_node_unmerged(has_test_tag("sheet")).fetch_semantics_snapshot().bounds.left;
            assert!((left - offset).abs() <= 1.0, "{left}, {offset}");
            let ops = ui.host().display_list().ops.clone();
            let scrim = ops.iter().find_map(|op| match op {
                DrawOp::DrawRect(_, paint)
                    if paint.color.with_alpha(1.0) == SCRIM.with_alpha(1.0) =>
                {
                    Some(paint.color)
                }
                _ => None,
            });
            assert!(scrim.unwrap().alpha() <= SCRIM.alpha(), "{scrim:?}");
            let found = scales(&mut ui);
            assert_scales(
                &found,
                &[(1.0 + offset / SHEET_WIDTH, 1.0), (1.0 / (1.0 + offset / SHEET_WIDTH), 1.0)],
            );
            most = most.max(offset);
        }
    }
    assert!(most > 0.0, "it never went past open");
    assert_eq!(state.current_offset(), 0.0);
    assert_eq!(scales(&mut ui), []);
}

/// What the scrim shows is still of a whole, whatever the sheet does.
#[test]
fn a_plain_sheet_has_no_layers_of_its_own() {
    let state = DrawerState::new(DrawerValue::Open, |_| true);
    let handle = *state;
    let mut ui = manual_clock(move || {
        MaterialTheme!({
            ModalNavigationDrawer!(
                || {
                    ModalDrawerSheet!(|_: ColumnScope| {
                        Box!(modifier = Modifier.fill_max_size());
                    });
                },
                drawer_state = handle,
                || {
                    Box!(modifier = Modifier.fill_max_size().size(10.dp()));
                }
            );
        });
    });
    settle(&mut ui);
    assert_eq!(scales(&mut ui), []);
}
