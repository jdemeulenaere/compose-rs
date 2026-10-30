//! Compose's `ModalBottomSheetTest.kt`, in a 400 by 800 window at density 1: dismissal by the
//! scrim, a swipe and back (and the properties turning them off), where the sheet rests (half
//! its height partially expanded, or half the window; the legacy states'), size changes, the
//! drag handle's clicks, gestures turned off, the state's restoration and confirmation.
//! Compose's dispatcher-driven nested scrolls are real gestures here (compose-rs has no
//! `NestedScrollDispatcher` to call from a test).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers as _, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, Modifier, Offset, PointerEvent, UnitExt, platform::UiHost};

thread_local! {
    static STATE: Cell<Option<SheetState>> = const { Cell::new(None) };
    static SCOPE: Cell<Option<CoroutineScope>> = const { Cell::new(None) };
}

fn state() -> SheetState {
    STATE.with(Cell::get).expect("composed")
}

fn keep(state: SheetState) {
    STATE.with(|cell| cell.set(Some(state)));
}

fn launch(task: impl std::future::Future<Output = ()> + 'static) {
    SCOPE.with(Cell::get).expect("composed").launch(task);
}

/// Compose's `SheetState(enabledValues, positionalThreshold, velocityThreshold)` at density 1.
fn sheet_state(values: &[SheetValue]) -> Owned<SheetState> {
    SheetState::new(
        values,
        || BottomSheetDefaults::POSITIONAL_THRESHOLD.0,
        || BottomSheetDefaults::VELOCITY_THRESHOLD.0,
        SheetValue::Hidden,
        |_| true,
    )
}

const SHEET_HEIGHT: Dp = Dp(256.0);
const WINDOW_HEIGHT: f32 = 800.0;

/// A tap at `y` on the window's left edge.
fn tap(ui: &mut ComposeTestRule, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(0.0, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(0.0, y)));
    ui.wait_for_idle();
}

/// The sheet, shown while `shown`, of a `SHEET_HEIGHT` box (or `modifier`'s), with `state`.
#[composable]
fn Dismissible(state: SheetState, shown: MutableState<bool>, sheet_modifier: Modifier) {
    MaterialTheme!({
        if shown.get() {
            ModalBottomSheet!(move || shown.set(false), sheet_state = state, {
                Box!(modifier = sheet_modifier.clone().test_tag("sheetContentTag"));
            });
        }
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_isDismissedOnTapOutside
#[test]
fn a_tap_on_the_scrim_dismisses_the_sheet() {
    let state = sheet_state(&SheetValue::ALL);
    let given = *state;
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = compose_test(move || Dismissible(given, handle, Modifier.size(SHEET_HEIGHT)));
    assert!(state.is_visible());
    tap(&mut ui, WINDOW_HEIGHT / 4.0);
    ui.on_node_with_tag("sheetContentTag").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_disabledClickOutside
#[test]
fn a_tap_on_the_scrim_keeps_the_sheet_without_dismissing_on_click_outside() {
    let mut ui = compose_test(|| {
        let state = remember_bottom_sheet_state!(SheetValue::Hidden);
        keep(state);
        MaterialTheme!({
            ModalBottomSheet!(
                || {},
                sheet_state = state,
                properties = ModalBottomSheetProperties {
                    should_dismiss_on_click_outside: false,
                    ..ModalBottomSheetProperties::DEFAULT
                },
                {
                    Box!(modifier = Modifier.size(SHEET_HEIGHT).test_tag("sheetContentTag"));
                }
            );
        });
    });
    assert!(state().is_visible());
    tap(&mut ui, WINDOW_HEIGHT / 4.0);
    assert!(state().is_visible());
    ui.on_node_with_tag("sheetContentTag").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_isDismissedOnTapOutsideWithPadding
#[test]
fn a_tap_on_the_scrim_above_a_padded_sheet_dismisses_it() {
    let state = sheet_state(&SheetValue::ALL);
    let given = *state;
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            if handle.get() {
                ModalBottomSheet!(
                    move || handle.set(false),
                    // The top half of the window as the sheet's padding.
                    modifier = Modifier.padding((0.dp(), Dp(WINDOW_HEIGHT / 2.0), 0.dp(), 0.dp())),
                    sheet_state = given,
                    {
                        Box!(modifier = Modifier.fill_max_size().test_tag("sheetContentTag"));
                    }
                );
            }
        });
    });
    assert!(state.is_visible());
    tap(&mut ui, WINDOW_HEIGHT / 2.0);
    ui.on_node_with_tag("sheetContentTag").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_isDismissedOnSwipeDown
#[test]
fn a_swipe_down_dismisses_the_sheet() {
    let state = sheet_state(&SheetValue::ALL);
    let given = *state;
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = compose_test(move || Dismissible(given, handle, Modifier.size(SHEET_HEIGHT)));
    assert!(state.is_visible());
    ui.on_node_with_tag("sheetContentTag").perform_touch_input(|touch| touch.swipe_down());
    ui.wait_for_idle();
    ui.on_node_with_tag("sheetContentTag").assert_does_not_exist();
}

/// A sheet without a drag handle or insets, in `state`, of a full-width box of `height`.
#[composable]
fn Bare(state: SheetState, height: Dp) {
    MaterialTheme!({
        ModalBottomSheet!(|| {}, sheet_state = state, drag_handle = None::<fn()>, {
            Box!(modifier = Modifier.fill_max_width().test_tag("sheetContentTag").height(height));
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_defaultStateForSmallContentIsFullExpanded
#[test]
fn a_short_sheet_that_cant_rest_half_way_shows_expanded() {
    // Compose's flag `isBottomSheetPartiallyExpandedDeterministicEnabled` off (the legacy
    // states, which rest short sheets expanded): here, a state without the half way (the
    // legacy states are gone, D1019).
    let mut ui = compose_test(|| {
        let state = remember_bottom_sheet_state!(
            SheetValue::Hidden,
            enabled_values = &[SheetValue::Hidden, SheetValue::Expanded]
        );
        keep(state);
        Bare(state, SHEET_HEIGHT);
    });
    let height = ui.on_node(is_popup()).get_unclipped_bounds_in_root().height();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    ui.on_node_with_tag("sheetContentTag")
        .assert_top_position_in_root_is_equal_to(height - SHEET_HEIGHT);
}

/// A sheet of a box `fraction` of the window high, shown while `shown`, with `state`; back
/// pressed from inside it (the host's back, as Compose's button calling the dispatcher).
fn back_press_dismisses(fraction: f32) {
    let state = sheet_state(&SheetValue::ALL);
    let given = *state;
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui =
        compose_test(move || Dismissible(given, handle, Modifier.fill_max_height_with(fraction)));
    assert!(state.is_visible());
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_tag("sheetContentTag").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_shortSheet_isDismissedOnBackPress
#[test]
fn back_dismisses_a_short_sheet() {
    back_press_dismisses(0.4);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_tallSheet_isDismissedOnBackPress
#[test]
fn back_dismisses_a_tall_sheet() {
    back_press_dismisses(0.6);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_doesNotDismissOnBack_whenPropertyFalse
#[test]
fn back_keeps_the_sheet_without_dismissing_on_back_press() {
    let dismissals = Rc::new(Cell::new(0));
    let counted = dismissals.clone();
    let mut ui = compose_test(move || {
        let state = remember_bottom_sheet_state!(SheetValue::Hidden);
        keep(state);
        let counted = counted.clone();
        MaterialTheme!({
            let counted = counted.clone();
            ModalBottomSheet!(
                move || counted.set(counted.get() + 1),
                sheet_state = state,
                properties = ModalBottomSheetProperties {
                    should_dismiss_on_back_press: false,
                    ..ModalBottomSheetProperties::DEFAULT
                },
                {
                    Box!(modifier = Modifier.fill_max_size());
                }
            );
        });
    });
    ui.wait_for_idle();
    assert!(state().is_visible());
    ui.host().on_back_pressed();
    ui.press_key(compose_ui::Key::Escape);
    ui.wait_for_idle();
    assert!(state().is_visible());
    assert_eq!(dismissals.get(), 0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_shortSheet_sizeChanges_snapsToNewTarget
#[test]
fn a_short_sheet_follows_its_size() {
    let size = mutable_state_of(56.dp());
    let given = size.handle();
    let mut ui = compose_test(move || {
        let state = remember_bottom_sheet_state!(
            SheetValue::Hidden,
            enabled_values = &[SheetValue::Hidden, SheetValue::Expanded]
        );
        keep(state);
        Bare(state, given.get());
    });
    let height = ui.on_node(is_popup()).get_unclipped_bounds_in_root().height().0;
    for dp in [56.0, 100.0, 30.0] {
        size.set(Dp(dp));
        ui.wait_for_idle();
        let expected = height - dp;
        assert!((state().require_offset() - expected).abs() <= 1.0, "{dp}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_sheetMaxWidth_sizeChanges_snapsToNewTarget
#[test]
fn a_sheet_follows_its_maximum_width() {
    let max_width = mutable_state_of(0.dp());
    let given = max_width.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_max_width = given.get(), {
                Box!(modifier = Modifier.fill_max_width().test_tag("sheetContentTag"));
            });
        });
    });
    for dp in [0.0, 200.0, 400.0] {
        max_width.set(Dp(dp));
        let width = ui.on_node_with_tag("sheetContentTag").get_unclipped_bounds_in_root().width();
        assert_eq!(width, Dp(dp.min(400.0)));
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_emptySheet_expandDoesNotAnimate
#[test]
fn an_empty_sheet_shows_without_moving() {
    let ui = compose_test(|| {
        let scope = remember_coroutine_scope();
        SCOPE.with(|cell| cell.set(Some(scope)));
        let state = remember_bottom_sheet_state!(
            SheetValue::Hidden,
            enabled_values = &[SheetValue::Hidden, SheetValue::Expanded]
        );
        keep(state);
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = state, drag_handle = None::<fn()>, |_| {});
        });
    });
    let mut ui = ui;
    // Nowhere to expand to: it stays hidden.
    assert!(!state().is_visible());
    let hidden = state().require_offset();
    let shown = state();
    launch(async move { shown.show().await });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    assert_eq!(state().require_offset(), hidden);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_callsOnDismissRequest_onNestedScrollFling
#[test]
fn a_fling_down_from_the_contents_top_dismisses_the_sheet() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let state = sheet_state(&[SheetValue::Expanded, SheetValue::Hidden]);
    let given = *state;
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        MaterialTheme!({
            let counted = counted.clone();
            ModalBottomSheet!(move || counted.set(counted.get() + 1), sheet_state = given, {
                Column!(
                    modifier = Modifier
                        .test_tag("sheetContentTag")
                        .vertical_scroll(remember_scroll_state(0)),
                    {
                        for index in 0..=50 {
                            Text!("{}", index);
                        }
                    }
                );
            });
        });
    });
    assert_eq!(state.current_value(), SheetValue::Expanded);
    // The content at its top: a fast drag down goes to the sheet, its fling dismisses it, as
    // Compose's dispatched post-scroll and post-fling.
    ui.on_node_with_tag("sheetContentTag").perform_touch_input(|touch| {
        let start = touch.top_center() + Offset::new(0.0, 10.0);
        touch.swipe(start, start + Offset::new(0.0, 300.0), 100);
    });
    ui.wait_for_idle();
    assert!(!state.is_visible());
    assert_eq!(calls.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_testDragHandleClick
#[test]
fn the_drag_handle_expands_then_dismisses_the_sheet() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = compose_test(move || {
        let state = remember_bottom_sheet_state!(SheetValue::Hidden);
        keep(state);
        MaterialTheme!({
            if handle.get() {
                ModalBottomSheet!(
                    move || handle.set(false),
                    sheet_state = state,
                    drag_handle =
                        Some(|| Box!(modifier = Modifier.test_tag("dragHandleTag").size(44.dp()))),
                    {
                        Box!(modifier = Modifier.fill_max_size().test_tag("sheetContentTag"));
                    }
                );
            }
        });
    });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::PartiallyExpanded);
    ui.on_node_with_tag_unmerged("dragHandleTag").perform_click();
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    ui.on_node_with_tag_unmerged("dragHandleTag").perform_click();
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Hidden);
    ui.on_node_with_tag("sheetContentTag").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_gesturesDisabled_doesNotParticipateInNestedScroll
#[test]
fn a_sheet_without_gestures_doesnt_follow_drags_or_scrolls() {
    let mut ui = compose_test(|| {
        let scope = remember_coroutine_scope();
        SCOPE.with(|cell| cell.set(Some(scope)));
        let state = remember_bottom_sheet_state!(SheetValue::Hidden);
        keep(state);
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = state, sheet_gestures_enabled = false, {
                Column!(
                    modifier = Modifier
                        .fill_max_width()
                        .height(300.dp())
                        .vertical_scroll(remember_scroll_state(0))
                        .test_tag("sheetContentTag"),
                    {
                        Box!(modifier = Modifier.fill_max_width().height(600.dp()));
                    }
                );
            });
        });
    });
    let expanding = state();
    launch(async move { expanding.expand().await });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    let before = state().require_offset();
    // A drag down on content at its top, and a fast one: the sheet stays.
    ui.on_node_with_tag("sheetContentTag").perform_touch_input(|touch| touch.swipe_down());
    ui.wait_for_idle();
    assert_eq!(state().require_offset(), before);
    assert_eq!(state().current_value(), SheetValue::Expanded);
}

fn restorable_sheet() {
    let scope = remember_coroutine_scope();
    SCOPE.with(|cell| cell.set(Some(scope)));
    let state = remember_bottom_sheet_state!(SheetValue::Hidden);
    keep(state);
    MaterialTheme!({
        ModalBottomSheet!(|| {}, sheet_state = state, {
            Box!(modifier = Modifier.fill_max_size().test_tag("sheet"));
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_stateRestoration_preservesExpandedState
#[test]
fn an_expanded_sheet_is_expanded_in_a_new_host() {
    let mut ui = compose_test(restorable_sheet);
    assert_eq!(state().current_value(), SheetValue::PartiallyExpanded);
    let expanding = state();
    launch(async move { expanding.expand().await });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    let saved = ui.host().save_state();
    drop(ui);
    let host = UiHost::headless(400, 800);
    host.restore_state(saved);
    let _ui = ComposeTestRule::with_host(host, restorable_sheet);
    assert_eq!(state().current_value(), SheetValue::Expanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_shortSheet_partiallyExpandedAtHalfContentHeight
#[test]
fn a_short_sheet_rests_half_its_height_up() {
    let mut ui = compose_test(|| {
        let scope = remember_coroutine_scope();
        SCOPE.with(|cell| cell.set(Some(scope)));
        let state = remember_bottom_sheet_state!(SheetValue::Hidden);
        keep(state);
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = state, drag_handle = None::<fn()>, {
                Box!(
                    modifier = Modifier
                        .fill_max_width()
                        .fill_max_height_with(0.4)
                        .test_tag("sheetContentTag")
                );
            });
        });
    });
    ui.wait_for_idle();
    let height = ui.on_node(is_popup()).get_unclipped_bounds_in_root().height().0;
    let sheet = height * 0.4;
    assert_eq!(state().current_value(), SheetValue::PartiallyExpanded);
    assert!((state().require_offset() - (height - sheet / 2.0)).abs() <= 1.0);
    let expanding = state();
    launch(async move { expanding.expand().await });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    assert!((state().require_offset() - (height - sheet)).abs() <= 1.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_tallSheet_partiallyExpandedAtHalfScreenHeight
#[test]
fn a_tall_sheet_rests_half_way_down_the_window() {
    let mut ui = compose_test(|| {
        let state = remember_bottom_sheet_state!(SheetValue::Hidden);
        keep(state);
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = state, drag_handle = None::<fn()>, {
                Box!(
                    modifier = Modifier
                        .fill_max_width()
                        .fill_max_height_with(2.0)
                        .test_tag("sheetContentTag")
                );
            });
        });
    });
    ui.wait_for_idle();
    let height = ui.on_node(is_popup()).get_unclipped_bounds_in_root().height().0;
    assert_eq!(state().current_value(), SheetValue::PartiallyExpanded);
    assert!((state().require_offset() - height / 2.0).abs() <= 1.0);
}

/// `remember_bottom_sheet_state`'s `confirm_value_change` can keep the sheet from hiding: the
/// scrim's click asks it first (Compose's `rememberBottomSheetState(confirmValueChange)`,
/// which `ModalBottomSheetTest`'s dismissal tests build on).
#[test]
fn a_remembered_states_confirmation_keeps_the_sheet_shown() {
    let mut ui = compose_test(|| {
        let state = remember_bottom_sheet_state!(
            SheetValue::Hidden,
            enabled_values = &[SheetValue::Hidden, SheetValue::Expanded],
            confirm_value_change = |value| value != SheetValue::Hidden
        );
        keep(state);
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = state, {
                Box!(modifier = Modifier.fill_max_width().height(Dp(100.0)));
            });
        });
    });
    assert_eq!(state().current_value(), SheetValue::Expanded);
    tap(&mut ui, 50.0);
    assert_eq!(state().current_value(), SheetValue::Expanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetDialogTest.kt, dialog_showsContent
#[test]
fn the_sheets_window_shows_its_content() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            ModalBottomSheet!(|| {}, {
                Box!(modifier = Modifier.fill_max_size(), {
                    Text!("BottomSheet Content");
                });
            });
        });
    });
    ui.on_node_with_text("BottomSheet Content").assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetDialogTest.kt, dialog_doesNotDismissOnBackPress_whenPropertyFalse
#[test]
fn back_doesnt_dismiss_a_small_sheet_without_dismissing_on_back_press() {
    let dismissals = Rc::new(Cell::new(0));
    let counted = dismissals.clone();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        MaterialTheme!({
            let counted = counted.clone();
            ModalBottomSheet!(
                move || counted.set(counted.get() + 1),
                properties = ModalBottomSheetProperties {
                    should_dismiss_on_back_press: false,
                    ..ModalBottomSheetProperties::DEFAULT
                },
                {
                    Box!(modifier = Modifier.size(100.dp()));
                }
            );
        });
    });
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    ui.run_on_idle(|_| assert_eq!(dismissals.get(), 0));
}

/// An expressive spring goes past the top anchor: the sheet scales down from its top so no gap
/// shows below it, the content scaled back, as `verticalScaleUp` and `verticalScaleDown`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/SheetDefaults.kt, verticalScaleUp, verticalScaleDown
#[test]
fn an_overshooting_sheet_scales_up_and_its_content_back_down() {
    use compose_ui::platform::DrawOp;
    let owned = sheet_state(&[SheetValue::Hidden, SheetValue::Expanded]);
    let sheet = *owned;
    let showing = mutable_state_of(false);
    let shown = showing.handle();
    let mut ui = ComposeTestRule::sized(compose_ui::IntSize::new(400, 800), move || {
        let _ = &showing;
        MaterialTheme!(motion_scheme = MotionScheme::expressive(), {
            if !shown.get() {
                return;
            }
            ModalBottomSheet!(
                || {},
                sheet_state = sheet,
                drag_handle = None::<fn()>,
                |_: ColumnScope| {
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .height(500.dp())
                            .background(compose_ui::Color::RED)
                    );
                }
            );
        });
    });
    ui.set_auto_advance(false);
    shown.set(true);
    let mut overflowed = 0.0_f32;
    for _ in 0..200 {
        ui.advance_time_by(std::time::Duration::from_millis(16));
        let overflow = 300.0 - sheet.require_offset();
        if overflow > 1.0 {
            let up = (500.0 + overflow) / 500.0;
            let ops = ui.host().display_list().ops.clone();
            let found: Vec<(f32, f32)> = ops
                .iter()
                .filter_map(|op| match op {
                    DrawOp::Transform { scale, .. } if *scale != (1.0, 1.0) => Some(*scale),
                    _ => None,
                })
                .collect();
            assert_eq!(found.len(), 2, "{found:?}");
            assert!((found[0].1 - up).abs() < 1e-3, "{found:?}, {up}");
            assert!((found[1].1 - 1.0 / up).abs() < 1e-3, "{found:?}, {up}");
            overflowed = overflowed.max(overflow);
        }
    }
    assert!(overflowed > 0.0, "it never went past its top anchor");
    assert_eq!(sheet.require_offset(), 300.0);
}

/// A sheet of a button alone rests with half of it showing, its handle and not its button, as the
/// tip's default state does (`isBottomSheetPartiallyExpandedDeterministicEnabled`, D1019): to show
/// a short sheet whole, its state has no half way. It's nothing to do with its window.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/BottomSheet.kt, calculatePartiallyExpandedOffset
#[test]
fn a_short_sheet_rests_half_way_unless_its_state_has_none() {
    let run = |values: &'static [SheetValue]| {
        let owned = sheet_state(values);
        let sheet = *owned;
        let mut ui = ComposeTestRule::sized(compose_ui::IntSize::new(400, 800), move || {
            MaterialTheme!({
                ModalBottomSheet!(
                    || {},
                    sheet_state = sheet,
                    modifier = Modifier.test_tag("sheet"),
                    |_: ColumnScope| {
                        Button!(|| {}, modifier = Modifier.test_tag("button"), {
                            Text!("Close");
                        });
                    }
                );
            });
        });
        ui.wait_for_idle();
        let bounds = ui.on_node_with_tag_unmerged("sheet").fetch_semantics_snapshot().bounds;
        (sheet.current_value(), bounds.top, bounds.height())
    };
    assert_eq!(run(&SheetValue::ALL), (SheetValue::PartiallyExpanded, 752.0, 96.0));
    assert_eq!(
        run(&[SheetValue::Hidden, SheetValue::Expanded]),
        (SheetValue::Expanded, 704.0, 96.0)
    );
}
