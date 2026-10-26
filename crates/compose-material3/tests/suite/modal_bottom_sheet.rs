//! Compose's `ModalBottomSheetTest.kt`, adapted (the scenarios `suite/sheets.rs` doesn't
//! already cover): the remembered state's restoration and `confirm_value_change`.

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Modifier, platform::UiHost};

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

fn sheet_screen() {
    let scope = remember_coroutine_scope();
    SCOPE.with(|cell| cell.set(Some(scope)));
    let state = remember_modal_bottom_sheet_state!();
    STATE.with(|cell| cell.set(Some(state)));
    MaterialTheme!({
        ModalBottomSheet!(|| {}, sheet_state = state, {
            Box!(modifier = Modifier.fill_max_size().test_tag("sheet"));
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ModalBottomSheetTest.kt, modalBottomSheet_stateRestoration_preservesExpandedState
#[test]
fn an_expanded_sheet_is_expanded_in_a_new_host() {
    let mut ui = compose_test(sheet_screen);
    assert_eq!(state().current_value(), SheetValue::PartiallyExpanded);
    let expanding = state();
    launch(async move { expanding.expand().await });
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
    let saved = ui.host().save_state();
    drop(ui);
    let host = UiHost::headless(400, 800);
    host.restore_state(saved);
    let _ui = ComposeTestRule::with_host(host, sheet_screen);
    assert_eq!(state().current_value(), SheetValue::Expanded);
}

/// `remember_modal_bottom_sheet_state`'s `confirm_value_change` can keep the sheet from
/// hiding: the scrim's click asks it first (Compose's `rememberModalBottomSheetState(
/// confirmValueChange)`, which `ModalBottomSheetTest`'s dismissal tests build on).
#[test]
fn a_remembered_states_confirmation_keeps_the_sheet_shown() {
    let mut ui = compose_test(|| {
        let state = remember_modal_bottom_sheet_state!(
            skip_partially_expanded = true,
            confirm_value_change = |value| value != SheetValue::Hidden
        );
        STATE.with(|cell| cell.set(Some(state)));
        MaterialTheme!({
            ModalBottomSheet!(|| {}, sheet_state = state, {
                Box!(modifier = Modifier.fill_max_width().height(compose_ui::Dp(100.0)));
            });
        });
    });
    assert_eq!(state().current_value(), SheetValue::Expanded);
    // A click on the scrim, above the sheet.
    ui.host().send_pointer(compose_ui::PointerEvent::press(compose_ui::Offset::new(200.0, 50.0)));
    ui.host().send_pointer(compose_ui::PointerEvent::release(compose_ui::Offset::new(200.0, 50.0)));
    ui.wait_for_idle();
    assert_eq!(state().current_value(), SheetValue::Expanded);
}
