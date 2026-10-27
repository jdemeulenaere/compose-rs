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
