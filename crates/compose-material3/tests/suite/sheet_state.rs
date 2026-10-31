//! Compose's `SheetStateTest.kt`, the scenarios through the public API (the others reach the
//! internal `anchoredDraggableState`): the constructor's contracts, `confirm_value_change`,
//! anchors changing under a state and reconciling with them. Compose's tests turning
//! `isBottomSheetPartiallyExpandedDeterministicEnabled` off hold with the default states,
//! but `state_missingAnchors_findsClosest` (a short sheet resting expanded): the legacy states
//! are gone (D1019).

#![allow(non_snake_case)]

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::lazy::{LazyColumn, LazyListScope};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Modifier, UnitExt};

thread_local! {
    static STATE: Cell<Option<SheetState>> = const { Cell::new(None) };
    static SCOPE: Cell<Option<CoroutineScope>> = const { Cell::new(None) };
}

fn state() -> SheetState {
    STATE.with(Cell::get).expect("composed")
}

fn launch(task: impl std::future::Future<Output = ()> + 'static) {
    SCOPE.with(Cell::get).expect("composed").launch(task);
}

/// Compose's `createSheetState(skipPartiallyExpanded, skipHiddenState, initialValue)`.
fn create_sheet_state(
    skip_partially_expanded: bool,
    skip_hidden_state: bool,
    initial_value: SheetValue,
) -> Owned<SheetState> {
    let mut values = vec![SheetValue::Expanded];
    if !skip_partially_expanded {
        values.push(SheetValue::PartiallyExpanded);
    }
    if !skip_hidden_state {
        values.push(SheetValue::Hidden);
    }
    SheetState::new(&values, || 56.0, || 125.0, initial_value, |_| true)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SheetStateTest.kt, state_constructor_initialValueContracts
#[test]
#[should_panic(expected = "The initial value must be one of the enabled values.")]
fn a_state_cant_start_partially_expanded_when_it_skips_it() {
    let _ = create_sheet_state(true, false, SheetValue::PartiallyExpanded);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SheetStateTest.kt, state_constructor_initialValueContracts
#[test]
#[should_panic(expected = "The initial value must be one of the enabled values.")]
fn a_state_cant_start_hidden_when_it_skips_it() {
    let _ = create_sheet_state(false, true, SheetValue::Hidden);
}

/// The values must hold `Expanded`, as Compose's `require`.
#[test]
#[should_panic(expected = "Expanded must be one of the enabled values.")]
fn a_state_must_be_able_to_expand() {
    let _ = SheetState::new(&[SheetValue::Hidden], || 56.0, || 125.0, SheetValue::Hidden, |_| true);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SheetStateTest.kt, state_respectsConfirmValueChange
#[test]
fn a_state_refusing_to_hide_keeps_the_sheet_on_swipes_and_hide() {
    let mut ui = compose_test(|| {
        let scope = remember_coroutine_scope();
        SCOPE.with(|cell| cell.set(Some(scope)));
        let state = remember_bottom_sheet_state!(
            SheetValue::Hidden,
            confirm_value_change = |value| value != SheetValue::Hidden
        );
        STATE.with(|cell| cell.set(Some(state)));
        MaterialTheme!({
            ModalBottomSheet!(
                || {},
                sheet_state = state,
                drag_handle =
                    Some(|| Box!(modifier = Modifier.test_tag("dragHandleTag").size(44.dp()))),
                {
                    Box!(modifier = Modifier.fill_max_size().test_tag("sheetTag"));
                }
            );
        });
    });
    let offset = state().require_offset();
    ui.run_on_idle(|_| assert_eq!(state().current_value(), SheetValue::PartiallyExpanded));
    // On a swipe.
    ui.on_node_with_tag("sheetTag").perform_touch_input(|touch| touch.swipe_down());
    ui.run_on_idle(|_| assert_eq!(state().current_value(), SheetValue::PartiallyExpanded));
    assert_eq!(state().require_offset(), offset);
    // On `hide`.
    let hiding = state();
    ui.run_on_idle(|_| launch(async move { hiding.hide().await }));
    ui.run_on_idle(|_| assert_eq!(state().current_value(), SheetValue::PartiallyExpanded));
    assert_eq!(state().require_offset(), offset);
    // The drag handle's dismiss action needs semantics actions (feature:semantics-gaps).
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SheetStateTest.kt, state_anchorsChange_retainsCurrentValue
#[test]
fn a_sheet_keeps_its_value_as_its_content_changes() {
    let items = mutable_state_of(0usize);
    let count = items.handle();
    let mut ui = compose_test(move || {
        let state = remember_bottom_sheet_state!(SheetValue::Hidden);
        STATE.with(|cell| cell.set(Some(state)));
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = state, drag_handle = None::<fn()>, {
                let scope = remember_coroutine_scope();
                SCOPE.with(|cell| cell.set(Some(scope)));
                let count = count.get();
                LazyColumn!(
                    modifier = Modifier.test_tag("sheetTag"),
                    move |scope: LazyListScope| {
                        scope.items(count, |index| {
                            ListItem!({
                                Text!("{}", index);
                            });
                        });
                    }
                );
            });
        });
    });
    assert_eq!(state().current_value(), SheetValue::Hidden);

    items.set(50);
    ui.wait_for_idle();
    let showing = state();
    launch(async move { showing.show().await });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::PartiallyExpanded);

    items.set(100);
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::PartiallyExpanded);
    assert!(state().has_partially_expanded_state() && state().has_expanded_state());

    let expanding = state();
    launch(async move { expanding.expand().await });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);

    items.set(50);
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    assert!(state().has_partially_expanded_state() && state().has_expanded_state());

    // No height: hidden, the only anchor.
    items.set(0);
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Hidden);
    assert!(!state().has_partially_expanded_state() && !state().has_expanded_state());
}

/// A state resting anywhere, at density 1.
fn default_state() -> Owned<SheetState> {
    SheetState::new(
        &SheetValue::ALL,
        || BottomSheetDefaults::POSITIONAL_THRESHOLD.0,
        || BottomSheetDefaults::VELOCITY_THRESHOLD.0,
        SheetValue::Hidden,
        |_| true,
    )
}

/// An empty sheet shown (expanded, though nowhere to go), then given content of
/// `fraction` of the window: it reconciles with the content's expanded anchor.
fn reconciles(fraction: f32) {
    let content = mutable_state_of(false);
    let given = content.handle();
    let state = default_state();
    let sheet = *state;
    let mut ui = compose_test(move || {
        let scope = remember_coroutine_scope();
        SCOPE.with(|cell| cell.set(Some(scope)));
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = sheet, drag_handle = None::<fn()>, {
                if given.get() {
                    Box!(modifier = Modifier.fill_max_height_with(fraction).test_tag("sheetTag"));
                } else {
                    Box!(modifier = Modifier.size(0.dp()).test_tag("sheetTag"));
                }
            });
        });
    });
    assert_eq!(state.current_value(), SheetValue::Hidden);
    assert!(!state.has_partially_expanded_state() && !state.has_expanded_state());
    let showing = *state;
    launch(async move { showing.show().await });
    ui.wait_for_idle();
    // Nowhere to go: the value taken as is, the offset still at the window's bottom.
    assert!(state.is_visible());
    assert_eq!(state.current_value(), SheetValue::Expanded);
    assert_eq!(state.target_value(), SheetValue::Expanded);
    assert_eq!(state.require_offset(), 800.0);
    content.set(true);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Expanded);
    assert_eq!(state.target_value(), SheetValue::Expanded);
    assert!((state.require_offset() - 800.0 * (1.0 - fraction)).abs() <= 1.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SheetStateTest.kt, state_shortSheet_anchorChangeHandler_previousTargetNotInAnchors_reconciles
#[test]
fn a_shown_empty_short_sheet_reconciles_with_its_content() {
    reconciles(0.4);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SheetStateTest.kt, state_tallSheet_anchorChangeHandler_previousTargetNotInAnchors_reconciles
#[test]
fn a_shown_empty_tall_sheet_reconciles_with_its_content() {
    reconciles(0.6);
}

/// A modal sheet in `state`, its drag handle and content tagged.
fn modal_sheet_of(state: SheetState) -> impl Fn() + 'static {
    move || {
        MaterialTheme!({
            ModalBottomSheet!(
                || {},
                sheet_state = state,
                drag_handle = Some(|| {
                    compose_foundation::layout::Spacer(Modifier.test_tag("handle").size(44.dp()));
                }),
                |_| {
                    compose_foundation::layout::Spacer(Modifier.size(400.dp()));
                }
            );
        });
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SheetStateTest.kt, the semantic action of its confirmValueChange test
#[test]
fn a_refused_dismiss_action_leaves_the_sheet() {
    let state = SheetState::new(
        &[SheetValue::Hidden, SheetValue::PartiallyExpanded, SheetValue::Expanded],
        || 56.0,
        || 125.0,
        SheetValue::PartiallyExpanded,
        |value| value != SheetValue::Hidden,
    );
    let mut ui = compose_test(modal_sheet_of(*state));
    ui.on_node_unmerged(has_test_tag("handle"))
        .on_parent()
        .perform_semantics_action(&compose_ui::SemanticsActions::DISMISS);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
}

/// The drag handle's actions by the sheet's value, as `BottomSheet`'s semantics: half way,
/// expand and dismiss; expanded, collapse and dismiss; each moves it.
#[test]
fn a_drag_handles_actions_move_the_sheet() {
    let state = SheetState::new(
        &[SheetValue::Hidden, SheetValue::PartiallyExpanded, SheetValue::Expanded],
        || 56.0,
        || 125.0,
        SheetValue::PartiallyExpanded,
        |_| true,
    );
    let mut ui = compose_test(modal_sheet_of(*state));
    let handle = || has_test_tag("handle");
    ui.on_node_unmerged(handle())
        .on_parent()
        .assert(key_is_defined(&compose_ui::SemanticsActions::EXPAND))
        .assert(key_not_defined(&compose_ui::SemanticsActions::COLLAPSE))
        .assert(key_is_defined(&compose_ui::SemanticsActions::DISMISS));
    ui.on_node_unmerged(handle())
        .on_parent()
        .perform_semantics_action(&compose_ui::SemanticsActions::EXPAND);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Expanded);
    ui.on_node_unmerged(handle())
        .on_parent()
        .assert(key_not_defined(&compose_ui::SemanticsActions::EXPAND))
        .assert(key_is_defined(&compose_ui::SemanticsActions::COLLAPSE));
    ui.on_node_unmerged(handle())
        .on_parent()
        .perform_semantics_action(&compose_ui::SemanticsActions::COLLAPSE);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    ui.on_node_unmerged(handle())
        .on_parent()
        .perform_semantics_action(&compose_ui::SemanticsActions::DISMISS);
    ui.wait_for_idle();
    assert_eq!(state.current_value(), SheetValue::Hidden);
}

/// With its gestures off, a sheet's drag handle has no actions, as `if (gesturesEnabled)`; its
/// pane has its title and comes first.
#[test]
fn a_sheets_pane_has_its_title_and_gestures_off_no_actions() {
    let state = SheetState::new(
        &[SheetValue::Hidden, SheetValue::PartiallyExpanded, SheetValue::Expanded],
        || 56.0,
        || 125.0,
        SheetValue::PartiallyExpanded,
        |_| true,
    );
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            ModalBottomSheet!(
                || {},
                sheet_state = handle,
                sheet_gestures_enabled = false,
                drag_handle = Some(|| {
                    compose_foundation::layout::Spacer(Modifier.test_tag("handle").size(44.dp()));
                }),
                |_| {
                    compose_foundation::layout::Spacer(Modifier.size(400.dp()));
                }
            );
        });
    });
    ui.on_node_unmerged(has_test_tag("handle"))
        .on_parent()
        .assert(key_not_defined(&compose_ui::SemanticsActions::DISMISS));
    ui.on_node(expect_value(&compose_ui::SemanticsProperties::PANE_TITLE, "Bottom Sheet".into()))
        .assert(expect_value(&compose_ui::SemanticsProperties::TRAVERSAL_INDEX, 0.0));
}

/// A modal sheet's scrim and sheet are read as one group, the sheet first, as Compose's
/// `ModalBottomSheetContent`'s `isTraversalGroup`: its `traversalIndex`es count within it.
#[test]
fn a_modal_sheets_pane_is_in_a_traversal_group() {
    let state = SheetState::new(
        &[SheetValue::PartiallyExpanded, SheetValue::Expanded, SheetValue::Hidden],
        || 56.0,
        || 125.0,
        SheetValue::Expanded,
        |_| true,
    );
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = handle, |_| {
                compose_foundation::layout::Spacer(Modifier.size(100.dp()));
            });
        });
    });
    ui.on_node_unmerged(expect_value(
        &compose_ui::SemanticsProperties::PANE_TITLE,
        "Bottom Sheet".into(),
    ))
    .on_parent()
    .assert(expect_value(&compose_ui::SemanticsProperties::IS_TRAVERSAL_GROUP, true));
}

// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/ModalBottomSheet.kt, Scrim(onClick = if (properties.shouldDismissOnClickOutside) animateToDismiss else null)
/// A scrim's semantics come with its click: a sheet not dismissed by one has none.
#[test]
fn a_scrim_that_doesnt_dismiss_has_no_semantics() {
    let sheet = |dismisses: bool| {
        let state = SheetState::new(
            &[SheetValue::PartiallyExpanded, SheetValue::Expanded, SheetValue::Hidden],
            || 56.0,
            || 125.0,
            SheetValue::Expanded,
            |_| true,
        );
        compose_test(move || {
            let handle = *state;
            MaterialTheme!({
                ModalBottomSheet!(
                    || {},
                    sheet_state = handle,
                    properties = ModalBottomSheetProperties {
                        should_dismiss_on_click_outside: dismisses,
                        ..ModalBottomSheetProperties::default()
                    },
                    |_| {
                        compose_foundation::layout::Spacer(Modifier.size(100.dp()));
                    }
                );
            });
        })
    };
    sheet(true).on_node_with_content_description("Close sheet").assert_exists();
    sheet(false).on_node_with_content_description("Close sheet").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/SheetDefaults.kt rememberSheetState
#[test]
fn other_enabled_values_make_another_state() {
    let skip = mutable_state_of(false);
    let read = skip.handle();
    let noted: std::rc::Rc<Cell<Option<SheetState>>> = Default::default();
    let keep = noted.clone();
    let mut ui = compose_test(move || {
        let values: &[SheetValue] =
            if read.get() { &[SheetValue::Hidden, SheetValue::Expanded] } else { &SheetValue::ALL };
        keep.set(Some(remember_bottom_sheet_state!(SheetValue::Hidden, enabled_values = values)));
    });
    ui.wait_for_idle();
    let first = noted.get().expect("composed");
    skip.set(true);
    ui.wait_for_idle();
    assert!(noted.get().expect("composed") != first);
}

#[test]
fn the_latest_confirmation_is_the_one_asked() {
    let refuse = mutable_state_of(false);
    let read = refuse.handle();
    let noted: std::rc::Rc<Cell<Option<SheetState>>> = Default::default();
    let keep = noted.clone();
    let mut ui = compose_test(move || {
        let refused = read.get();
        keep.set(Some(remember_bottom_sheet_state!(
            SheetValue::Expanded,
            confirm_value_change = move |_| !refused
        )));
    });
    ui.wait_for_idle();
    let state = noted.get().expect("composed");
    assert!(state.confirms(SheetValue::Hidden));
    refuse.set(true);
    ui.wait_for_idle();
    assert!(noted.get().expect("composed") == state);
    assert!(!state.confirms(SheetValue::Hidden));
}
