//! Compose's `BottomSheetScaffoldTest.kt` and `BottomSheetTest.kt`, adapted: the standard sheet's
//! offsets, its actions by value, swipes, the top bar, body and snackbar's places, the max width,
//! nested scroll with gestures off, a peek height of 0, and the standalone sheet.
//!
//! Not ported: the two `*_bottomSheetOffsetTaggedAsMotionFrameOfReference` (Compose's
//! frame-of-reference tags); the two `bottomSheetScaffold_landscape_*` (`@Ignore`d in Compose);
//! `*_respectsMaterialThemeMotionScheme` (feature:motion-scheme); from `BottomSheetTest`, the
//! `@Ignore`d wide-screen tests and the window-insets ones (feature:material-window-insets);
//! `semanticsMatcher_hasActionLabel_findsNodeForAction` is a test of its own test helper
//! (compose-test's `perform_semantics_action` covers it).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Modifier, SemanticsActions, UnitExt};

const ROOT_HEIGHT: f32 = 800.0;
const SHEET_HEIGHT: f32 = 256.0;
const HANDLE: f32 = 44.0;
const PEEK: f32 = 75.0;
const SHEET_TAG: &str = "sheet";
const HANDLE_TAG: &str = "handle";

fn top(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds.top
}

fn new_state(values: &[SheetValue], initial: SheetValue) -> Owned<SheetState> {
    SheetState::new(values, || 56.0, || 125.0, initial, |_| true)
}

const STANDARD: [SheetValue; 2] = [SheetValue::PartiallyExpanded, SheetValue::Expanded];

/// A scaffold of `state`, with a 256 dp sheet and a 44 dp drag handle, peeking 75 dp.
fn scaffold(state: SheetState, host: SnackbarHostState) -> impl Fn() + 'static {
    move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .size_in(compose_ui::SizeSpec::exact(
                                compose_ui::Dp::UNSPECIFIED,
                                SHEET_HEIGHT.dp(),
                                true
                            ))
                            .test_tag(SHEET_TAG)
                    );
                },
                scaffold_state = BottomSheetScaffoldState {
                    bottom_sheet_state: state,
                    snackbar_host_state: host
                },
                sheet_drag_handle = Some(|| {
                    Box!(modifier = Modifier.test_tag(HANDLE_TAG).size(HANDLE.dp()));
                }),
                sheet_peek_height = PEEK.dp(),
                {
                    Text!("Content");
                }
            );
        });
    }
}

/// A rule of `state` with that scaffold.
fn rule_of(state: SheetState) -> (ComposeTestRule, Owned<SnackbarHostState>) {
    let host = SnackbarHostState::new();
    (compose_test(scaffold(state, *host)), host)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testOffset_whenCollapsed
#[test]
fn a_collapsed_sheet_peeks() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_size().test_tag(SHEET_TAG));
                },
                sheet_peek_height = PEEK.dp(),
                sheet_drag_handle = None::<fn()>,
                {
                    Text!("Content");
                }
            );
        });
    });
    assert_eq!(top(&mut ui, SHEET_TAG), ROOT_HEIGHT - PEEK);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testOffset_whenExpanded
#[test]
fn an_expanded_sheet_is_its_height_up() {
    let state = new_state(&STANDARD, SheetValue::Expanded);
    let (mut ui, _host) = rule_of(*state);
    assert_eq!(top(&mut ui, HANDLE_TAG), ROOT_HEIGHT - (SHEET_HEIGHT + HANDLE));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testExpandAction_whenCollapsed
#[test]
fn a_collapsed_sheets_expand_action_expands_it() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let (mut ui, _host) = rule_of(*state);
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG))
        .on_parent()
        .assert(key_not_defined(&SemanticsActions::COLLAPSE))
        .assert(key_is_defined(&SemanticsActions::EXPAND))
        .perform_semantics_action(&SemanticsActions::EXPAND);
    ui.wait_for_idle();
    assert_eq!(top(&mut ui, HANDLE_TAG), ROOT_HEIGHT - (SHEET_HEIGHT + HANDLE));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testDismissAction_whenEnabled
#[test]
fn a_sheet_that_can_hide_has_a_dismiss_action() {
    let state = new_state(&SheetValue::ALL, SheetValue::PartiallyExpanded);
    let (mut ui, _host) = rule_of(*state);
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG))
        .on_parent()
        .assert(key_not_defined(&SemanticsActions::COLLAPSE))
        .assert(key_is_defined(&SemanticsActions::DISMISS))
        .assert(key_is_defined(&SemanticsActions::EXPAND))
        .perform_semantics_action(&SemanticsActions::DISMISS);
    ui.wait_for_idle();
    assert_eq!(top(&mut ui, HANDLE_TAG), ROOT_HEIGHT);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testHideReturnsIllegalStateException
#[test]
#[should_panic(expected = "Attempted to animate to hidden when skipHiddenState was enabled")]
fn hiding_a_sheet_that_cant_hide_panics() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let handle = *state;
    let (mut ui, _host) = rule_of(handle);
    compose_runtime::platform::launch(async move { handle.hide().await });
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testCollapseAction_whenExpanded
#[test]
fn an_expanded_sheets_collapse_action_collapses_it() {
    let state = new_state(&STANDARD, SheetValue::Expanded);
    let (mut ui, _host) = rule_of(*state);
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG))
        .on_parent()
        .assert(key_not_defined(&SemanticsActions::EXPAND))
        .assert(key_is_defined(&SemanticsActions::COLLAPSE))
        .perform_semantics_action(&SemanticsActions::COLLAPSE);
    ui.wait_for_idle();
    assert_eq!(top(&mut ui, HANDLE_TAG), ROOT_HEIGHT - PEEK);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testNoCollapseExpandAction_whenPeekHeightIsSheetHeight
#[test]
fn a_sheet_as_high_as_its_peek_has_no_actions() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(PEEK.dp()));
                },
                scaffold_state = remember_bottom_sheet_scaffold_state!(Some(handle)),
                sheet_drag_handle = Some(|| {
                    Box!(modifier = Modifier.test_tag(HANDLE_TAG).size(0.dp()));
                }),
                sheet_peek_height = PEEK.dp(),
                {
                    Text!("Content");
                }
            );
        });
    });
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG))
        .on_parent()
        .assert(key_not_defined(&SemanticsActions::EXPAND))
        .assert(key_not_defined(&SemanticsActions::COLLAPSE));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_revealAndConceal_manually
#[test]
fn expand_and_partial_expand_move_the_sheet() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let handle = *state;
    let (mut ui, _host) = rule_of(handle);
    assert_eq!(top(&mut ui, HANDLE_TAG), ROOT_HEIGHT - PEEK);
    compose_runtime::platform::launch(async move { handle.expand().await });
    ui.wait_for_idle();
    assert_eq!(top(&mut ui, HANDLE_TAG), ROOT_HEIGHT - (SHEET_HEIGHT + HANDLE));
    compose_runtime::platform::launch(async move { handle.partial_expand().await });
    ui.wait_for_idle();
    assert_eq!(top(&mut ui, HANDLE_TAG), ROOT_HEIGHT - PEEK);
}

fn swipe_up(ui: &mut ComposeTestRule, tag: &str) {
    ui.on_node_unmerged(has_test_tag(tag)).perform_touch_input(|touch| touch.swipe_up());
    ui.wait_for_idle();
}

fn swipe_down(ui: &mut ComposeTestRule, tag: &str) {
    ui.on_node_unmerged(has_test_tag(tag)).perform_touch_input(|touch| touch.swipe_down());
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_revealBySwiping
#[test]
fn swipes_reveal_and_collapse_the_sheet() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let (mut ui, _host) = rule_of(*state);
    swipe_up(&mut ui, HANDLE_TAG);
    assert_eq!(state.current_value(), SheetValue::Expanded);
    swipe_down(&mut ui, SHEET_TAG);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_respectsConfirmStateChange
#[test]
fn a_refused_value_isnt_swiped_to() {
    let state = SheetState::new(
        &STANDARD,
        || 56.0,
        || 125.0,
        SheetValue::PartiallyExpanded,
        |value| value != SheetValue::Expanded,
    );
    let (mut ui, _host) = rule_of(*state);
    swipe_up(&mut ui, HANDLE_TAG);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_revealBySwiping_gesturesDisabled
#[test]
fn with_swiping_off_a_swipe_does_nothing() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(300.dp()).test_tag(SHEET_TAG));
                },
                scaffold_state = remember_bottom_sheet_scaffold_state!(Some(handle)),
                sheet_swipe_enabled = false,
                sheet_peek_height = PEEK.dp(),
                {
                    Text!("Content");
                }
            );
        });
    });
    swipe_up(&mut ui, SHEET_TAG);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_AppbarAndContent_inColumn
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_slotsPositionedAppropriately
#[test]
fn the_slots_are_placed_around_the_sheet() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let host = SnackbarHostState::new();
    let (handle, host_handle) = (*state, *host);
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(SHEET_HEIGHT.dp()));
                },
                scaffold_state = BottomSheetScaffoldState {
                    bottom_sheet_state: handle,
                    snackbar_host_state: host_handle
                },
                sheet_peek_height = PEEK.dp(),
                top_bar = Some(|| {
                    Box!(modifier = Modifier.fill_max_width().height(56.dp()).test_tag("top"));
                }),
                snackbar_host = |state: SnackbarHostState| {
                    SnackbarHost!(state, modifier = Modifier.test_tag("snackbars"));
                },
                |padding: compose_ui::PaddingValues| {
                    Box!(modifier = Modifier.padding(padding).test_tag("body"), {
                        Text!("Scaffold Content");
                    });
                }
            );
        });
    });
    assert_eq!(top(&mut ui, "top"), 0.0);
    ui.on_node_with_tag("top")
        .assert_height_is_equal_to(56.dp())
        .assert_width_is_equal_to(400.dp());
    assert_eq!(top(&mut ui, "body"), 56.0);
    let shown = host_handle;
    compose_runtime::platform::launch(async move {
        let _ = shown.show_snackbar("Hello").await;
    });
    ui.advance_time_by(std::time::Duration::from_millis(500));
    let snackbars = ui.on_node_with_tag("snackbars").fetch_semantics_node().bounds;
    assert_eq!(snackbars.bottom, ROOT_HEIGHT - PEEK);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_innerPadding_lambdaParam
#[test]
fn the_body_is_given_the_peek_height_as_padding() {
    thread_local! { static BOTTOM: Cell<f32> = const { Cell::new(0.0) }; }
    let _ui = compose_test(|| {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(100.dp()));
                },
                sheet_peek_height = PEEK.dp(),
                |padding: compose_ui::PaddingValues| {
                    BOTTOM.with(|bottom| bottom.set(padding.bottom.0));
                    Text!("body");
                }
            );
        });
    });
    assert_eq!(BOTTOM.with(Cell::get), PEEK);
}

/// Whether the display list fills something with `color`.
fn fills_with(ui: &mut ComposeTestRule, color: compose_ui::Color) -> bool {
    ui.host().display_list().ops.iter().any(|op| match op {
        compose_ui::platform::DrawOp::DrawRect(_, paint)
        | compose_ui::platform::DrawOp::DrawRoundRect(_, _, paint)
        | compose_ui::platform::DrawOp::DrawPath { paint, .. } => paint.color == color,
        _ => false,
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testNestedScrollConnection
/// A scroll in the sheet moves the sheet before its content and stays out of the top app bar's
/// scrolled color; a scroll in the body goes to the bar and leaves the sheet where it is. The
/// bar's color read from the display list, Compose's `assertContainsColor`.
#[test]
fn the_sheet_body_and_top_bar_each_get_their_own_scrolls() {
    thread_local! {
        static SHEET_SCROLL: Cell<Option<compose_foundation::ScrollState>> = const { Cell::new(None) };
        static BODY_SCROLL: Cell<Option<compose_foundation::ScrollState>> = const { Cell::new(None) };
        static COLORS: Cell<(compose_ui::Color, compose_ui::Color)> = const {
            Cell::new((compose_ui::Color::UNSPECIFIED, compose_ui::Color::UNSPECIFIED))
        };
    }
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            let behavior = TopAppBarDefaults::pinned_scroll_behavior();
            let colors = TopAppBarDefaults::top_app_bar_colors();
            COLORS
                .with(|cell| cell.set((colors.container_color(0.0), colors.container_color(1.0))));
            BottomSheetScaffold!(
                |_| {
                    let scroll = remember_scroll_state!();
                    SHEET_SCROLL.with(|cell| cell.set(Some(scroll)));
                    Column!(modifier = Modifier.vertical_scroll(scroll).test_tag(SHEET_TAG), {
                        for index in 0..100 {
                            Text!("{}", index, modifier = Modifier.height(50.dp()));
                        }
                    });
                },
                modifier = Modifier.nested_scroll(behavior.nested_scroll_connection()),
                scaffold_state = remember_bottom_sheet_scaffold_state!(Some(handle)),
                sheet_peek_height = PEEK.dp(),
                top_bar = Some({
                    let behavior = behavior.clone();
                    move || {
                        TopAppBar!(
                            { Text!("Title") },
                            modifier = Modifier.test_tag("bar"),
                            scroll_behavior = Some(behavior.clone())
                        );
                    }
                }),
                {
                    let scroll = remember_scroll_state!();
                    BODY_SCROLL.with(|cell| cell.set(Some(scroll)));
                    Column!(modifier = Modifier.vertical_scroll(scroll).test_tag("body"), {
                        for index in 0..100 {
                            Text!("{}", index, modifier = Modifier.height(50.dp()));
                        }
                    });
                }
            );
        });
    });
    let sheet_scroll = || SHEET_SCROLL.with(Cell::get).expect("composed").value();
    let body_scroll = || BODY_SCROLL.with(Cell::get).expect("composed").value();
    let (before, after) = COLORS.with(Cell::get);
    assert_ne!(before, after);
    assert_eq!(sheet_scroll(), 0);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    assert!(fills_with(&mut ui, before));

    // A partial scroll in the sheet expands it, its content not moving.
    ui.on_node_unmerged(has_test_tag(SHEET_TAG)).perform_touch_input(|touch| {
        let bottom = touch.bottom();
        touch.swipe_up_with(bottom, bottom / 2.0, 200);
    });
    ui.wait_for_idle();
    assert_eq!(sheet_scroll(), 0);
    assert_eq!(state.current_value(), SheetValue::Expanded);
    assert!(fills_with(&mut ui, before), "the bar hasn't changed");

    ui.on_node_unmerged(has_test_tag(SHEET_TAG)).perform_touch_input(|touch| {
        let (top, bottom) = (touch.top(), touch.bottom());
        touch.swipe_down_with(top, bottom, 200);
    });
    ui.wait_for_idle();
    assert_eq!(sheet_scroll(), 0);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);

    // A scroll in the body: the bar's color, the sheet still.
    ui.on_node_unmerged(has_test_tag("body")).perform_touch_input(|touch| {
        let (top, bottom) = (touch.top(), touch.bottom());
        touch.swipe_up_with(bottom / 2.0, top, 200);
    });
    ui.wait_for_idle();
    assert_eq!(sheet_scroll(), 0);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    assert!(body_scroll() > 0);
    assert!(fills_with(&mut ui, after), "the bar shows it's scrolled");
}

/// `rememberBottomSheetScaffoldState`'s default sheet can't hide: its `skipHiddenState`.
#[test]
#[should_panic(expected = "Attempted to animate to hidden when skipHiddenState was enabled")]
fn a_default_scaffold_states_sheet_cant_hide() {
    thread_local! { static SHEET: Cell<Option<SheetState>> = const { Cell::new(None) }; }
    let mut ui = compose_test(move || {
        MaterialTheme!({
            let state = remember_bottom_sheet_scaffold_state!();
            SHEET.with(|cell| cell.set(Some(state.bottom_sheet_state)));
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.size(300.dp()));
                },
                scaffold_state = state,
                { Text!("Content") }
            );
        });
    });
    let sheet = SHEET.with(Cell::get).expect("composed");
    assert_eq!(sheet.current_value(), SheetValue::PartiallyExpanded);
    compose_runtime::platform::launch(async move { sheet.hide().await });
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_gesturesDisabled_doesNotParticipateInNestedScroll
#[test]
fn with_swiping_off_the_sheet_isnt_in_its_contents_scrolls() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let handle = *state;
    thread_local! { static SCROLL: Cell<Option<compose_foundation::ScrollState>> = const { Cell::new(None) }; }
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    let scroll = remember_scroll_state!();
                    SCROLL.with(|cell| cell.set(Some(scroll)));
                    Column!(modifier = Modifier.vertical_scroll(scroll).test_tag(SHEET_TAG), {
                        for index in 0..100 {
                            Text!("{}", index, modifier = Modifier.height(50.dp()));
                        }
                    });
                },
                scaffold_state = remember_bottom_sheet_scaffold_state!(Some(handle)),
                sheet_swipe_enabled = false,
                sheet_peek_height = PEEK.dp(),
                {
                    Text!("Content");
                }
            );
        });
    });
    let scroll = || SCROLL.with(Cell::get).expect("composed");
    assert_eq!(scroll().value(), 0);
    swipe_up(&mut ui, SHEET_TAG);
    assert!(scroll().value() > 0);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    state.snap_to(SheetValue::Expanded);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Expanded);
    swipe_down(&mut ui, SHEET_TAG);
    assert_eq!(scroll().value(), 0);
    assert_eq!(state.current_value(), SheetValue::Expanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_sheetMaxWidth_sizeChanges_snapsToNewTarget
#[test]
fn a_new_max_width_changes_the_sheets_width() {
    let max_width = mutable_state_of(0.0f32);
    let handle = max_width.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_size().test_tag(SHEET_TAG));
                },
                sheet_peek_height = PEEK.dp(),
                sheet_max_width = handle.get().dp(),
                sheet_drag_handle = None::<fn()>,
                {
                    Text!("Content");
                }
            );
        });
    });
    for width in [0.0f32, 200.0, 400.0] {
        max_width.set(width);
        ui.wait_for_idle();
        let bounds = ui.on_node_unmerged(has_test_tag(SHEET_TAG)).fetch_semantics_node().bounds;
        assert_eq!(bounds.width(), width.min(400.0));
        assert_eq!(bounds.left, (400.0 - width.min(400.0)) / 2.0);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testDragHandleClick
#[test]
fn a_click_on_the_handle_expands_and_collapses() {
    let state = new_state(&STANDARD, SheetValue::PartiallyExpanded);
    let (mut ui, _host) = rule_of(*state);
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG)).perform_click();
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Expanded);
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG)).perform_click();
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_testDragHandleClick_hiddenStateAllowed
#[test]
fn a_click_on_the_handle_of_an_expanded_sheet_hides_it_if_it_can() {
    let state = new_state(&SheetValue::ALL, SheetValue::PartiallyExpanded);
    let (mut ui, _host) = rule_of(*state);
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG)).perform_click();
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Expanded);
    ui.on_node_unmerged(has_test_tag(HANDLE_TAG)).perform_click();
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Hidden);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_peekHeightMatchesContentHeight_containsExpandedAnchor
#[test]
fn a_peek_as_high_as_the_sheet_has_the_expanded_anchor() {
    let state = new_state(&[SheetValue::Expanded], SheetValue::Expanded);
    let handle = *state;
    let _ui = compose_test(move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(SHEET_HEIGHT.dp()));
                },
                scaffold_state = remember_bottom_sheet_scaffold_state!(Some(handle)),
                sheet_drag_handle = None::<fn()>,
                sheet_peek_height = SHEET_HEIGHT.dp(),
                {
                    Text!("Content");
                }
            );
        });
    });
    assert!(state.has_expanded_state());
    assert!(!state.has_partially_expanded_state());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_withDragHandle_confirmValueChange_invokedForSemanticsAction
#[test]
fn the_actions_ask_confirm_value_change() {
    let asked: Rc<RefCell<Vec<SheetValue>>> = Rc::default();
    let seen = asked.clone();
    let state = SheetState::new(
        &SheetValue::ALL,
        || 56.0,
        || 125.0,
        SheetValue::PartiallyExpanded,
        move |value| {
            seen.borrow_mut().push(value);
            true
        },
    );
    let (mut ui, _host) = rule_of(*state);
    assert!(asked.borrow().is_empty());
    let parent = |ui: &mut ComposeTestRule, action| {
        ui.on_node_unmerged(has_test_tag(HANDLE_TAG)).on_parent().perform_semantics_action(action);
        ui.wait_for_idle();
    };
    parent(&mut ui, &SemanticsActions::EXPAND);
    assert!(asked.borrow().contains(&SheetValue::Expanded));
    asked.borrow_mut().clear();
    parent(&mut ui, &SemanticsActions::COLLAPSE);
    assert!(asked.borrow().contains(&SheetValue::PartiallyExpanded));
    asked.borrow_mut().clear();
    parent(&mut ui, &SemanticsActions::DISMISS);
    assert!(asked.borrow().contains(&SheetValue::Hidden));
}

/// A scaffold of a 100 dp sheet peeking 0.
fn peeking_nothing(state: SheetState) -> impl Fn() + 'static {
    move || {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(100.dp()));
                },
                scaffold_state = remember_bottom_sheet_scaffold_state!(Some(state)),
                sheet_peek_height = 0.dp(),
                {
                    Box!(modifier = Modifier.fill_max_size());
                }
            );
        });
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_peekHeightZero_initialStatePartiallyExpanded
#[test]
fn a_peek_of_zero_starts_partially_expanded_at_the_bottom() {
    let state = new_state(&SheetValue::ALL, SheetValue::PartiallyExpanded);
    let _ui = compose_test(peeking_nothing(*state));
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    assert_eq!(state.require_offset(), ROOT_HEIGHT);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_peekHeightZero_explicitHide
#[test]
fn a_peek_of_zero_can_still_hide() {
    let state = new_state(&SheetValue::ALL, SheetValue::PartiallyExpanded);
    let handle = *state;
    let mut ui = compose_test(peeking_nothing(handle));
    compose_runtime::platform::launch(async move { handle.hide().await });
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Hidden);
    assert!(!state.is_visible());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_peekHeightZero_ambiguousAnchorRemovedAfterExpansion
#[test]
fn a_peek_of_zeros_partial_anchor_goes_once_expanded() {
    let state = new_state(&SheetValue::ALL, SheetValue::PartiallyExpanded);
    let handle = *state;
    let mut ui = compose_test(peeking_nothing(handle));
    assert!(state.has_partially_expanded_state());
    compose_runtime::platform::launch(async move { handle.expand().await });
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Expanded);
    assert!(!state.has_partially_expanded_state());
    compose_runtime::platform::launch(async move { handle.hide().await });
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Hidden);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_peekHeightZero_animateToPartiallyExpanded
#[test]
fn a_peek_of_zero_animates_to_partially_expanded() {
    let state = new_state(&SheetValue::ALL, SheetValue::Expanded);
    let handle = *state;
    let mut ui = compose_test(peeking_nothing(handle));
    assert_eq!(state.current_value(), SheetValue::Expanded);
    compose_runtime::platform::launch(async move { handle.partial_expand().await });
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    assert_eq!(state.target_value(), SheetValue::PartiallyExpanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, bottomSheetScaffold_topAppBarIsDrawnOnTopOfContent
#[test]
fn the_top_bar_is_drawn_over_the_content() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            BottomSheetScaffold!(
                |_| {
                    Box!(modifier = Modifier.size(0.dp()));
                },
                top_bar = Some(|| {
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .height(10.dp())
                            .background(compose_ui::Color::RED)
                    );
                }),
                {
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .height(10.dp())
                            .background(compose_ui::Color::BLUE)
                    );
                }
            );
        });
    });
    let ops = ui.host().display_list().ops;
    let at = |color: compose_ui::Color| {
        ops.iter()
            .position(|op| matches!(op, compose_ui::platform::DrawOp::DrawRect(_, paint) if paint.color == color))
            .expect("drawn")
    };
    assert!(at(compose_ui::Color::BLUE) < at(compose_ui::Color::RED));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetScaffoldTest.kt, test_stateSavedAndRestored
#[test]
fn a_sheet_state_is_saved_and_restored() {
    thread_local! { static VALUE: Cell<Option<SheetValue>> = const { Cell::new(None) }; }
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        let state = remember_bottom_sheet_state!(SheetValue::Expanded);
        VALUE.with(|cell| cell.set(Some(state.current_value())));
    });
    assert_eq!(VALUE.with(Cell::get), Some(SheetValue::Expanded));
    tester.emulate_save_and_restore(&mut rule);
    assert_eq!(VALUE.with(Cell::get), Some(SheetValue::Expanded));
}

// ---- BottomSheet ----------------------------------------------------------------------------

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetTest.kt, bottomSheet_fillsScreenWidth
#[test]
fn a_standalone_sheet_fills_the_width() {
    let state = new_state(&SheetValue::ALL, SheetValue::Expanded);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheet!(state = handle, |_| {
                Box!(
                    modifier =
                        Modifier.fill_max_width().height(SHEET_HEIGHT.dp()).test_tag(SHEET_TAG)
                );
            });
        });
    });
    let bounds = ui.on_node_unmerged(has_test_tag(SHEET_TAG)).fetch_semantics_node().bounds;
    assert_eq!(bounds.width(), 400.0);
}

/// `maxWidth`, as `bottomSheet_wideScreen_fixedMaxWidth_sheetRespectsMaxWidthAndIsCentered`
/// (`@Ignore`d in Compose for its rotation).
#[test]
fn a_standalone_sheet_is_centered_at_its_max_width() {
    let state = new_state(&SheetValue::ALL, SheetValue::Expanded);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheet!(state = handle, max_width = 200.dp(), |_| {
                Box!(
                    modifier =
                        Modifier.fill_max_width().height(SHEET_HEIGHT.dp()).test_tag(SHEET_TAG)
                );
            });
        });
    });
    let bounds = ui.on_node_unmerged(has_test_tag(SHEET_TAG)).fetch_semantics_node().bounds;
    assert_eq!((bounds.left, bounds.width()), (100.0, 200.0));
}

/// It doesn't show itself: hidden, it stays below; `show` shows it; its pane's there, and back
/// hides it and tells the caller.
#[test]
fn a_standalone_sheet_moves_with_its_state_and_back_dismisses_it() {
    let state = new_state(&SheetValue::ALL, SheetValue::Hidden);
    let handle = *state;
    let dismissed = Rc::new(Cell::new(0));
    let seen = dismissed.clone();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        MaterialTheme!({
            let seen = seen.clone();
            BottomSheet!(
                state = handle,
                on_dismiss_request = move || seen.set(seen.get() + 1),
                |_| {
                    Box!(modifier = Modifier.fill_max_width().height(400.dp()).test_tag(SHEET_TAG));
                }
            );
        });
    });
    assert_eq!(state.current_value(), SheetValue::Hidden);
    assert!(!state.is_visible());
    compose_runtime::platform::launch(async move { handle.show().await });
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    ui.on_node(expect_value(&compose_ui::SemanticsProperties::PANE_TITLE, "Bottom Sheet".into()))
        .assert_exists();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Hidden);
    assert_eq!(dismissed.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/BottomSheetTest.kt, bottomSheet_smallSheet_escapesDampeningAndDismisses
#[test]
fn a_tiny_sheet_still_dismisses_on_a_swipe() {
    let state = new_state(&SheetValue::ALL, SheetValue::PartiallyExpanded);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Box!(modifier = Modifier.required_size(100.dp()), {
                BottomSheet!(state = handle, drag_handle = None::<fn()>, |_| {
                    Box!(modifier = Modifier.fill_max_size().test_tag(SHEET_TAG), {
                        Text!("Extremely small");
                    });
                });
            });
        });
    });
    assert!(state.is_visible());
    swipe_down(&mut ui, SHEET_TAG);
    assert!(!state.is_visible());
}

/// `BottomSheet`'s default tonal elevation, `BottomSheetDefaults.Elevation`: on the surface, its
/// tint.
#[test]
fn a_standalone_sheets_default_tonal_elevation_tints_the_surface() {
    let state = new_state(&STANDARD, SheetValue::Expanded);
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            BottomSheet!(
                state = handle,
                container_color = MaterialTheme::color_scheme().surface,
                |_| {
                    Box!(modifier = Modifier.size(100.dp()));
                }
            );
        });
    });
    let tinted = light_color_scheme().surface_color_at_elevation(BottomSheetDefaults::ELEVATION);
    assert_ne!(tinted, light_color_scheme().surface);
    assert!(fills_with(&mut ui, tinted), "{:?}", ui.host().display_list().ops);
}
