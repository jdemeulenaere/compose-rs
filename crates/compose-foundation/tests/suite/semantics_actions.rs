//! Semantics actions: what an accessibility service asks of a node instead of sending input.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::ClickableOptions;
use compose_ui::{SemanticsActions, UnitExt};

#[test]
fn on_click_clicks_and_toggles_without_input() {
    let count = mutable_state_of(0);
    let disabled_clicks = mutable_state_of(0);
    let disabled_handle = disabled_clicks.handle();
    let checked = mutable_state_of(false);
    let (count_handle, checked_handle) = (count.handle(), checked.handle());
    let mut ui = compose_test(move || {
        Column!({
            Box!(
                modifier = Modifier
                    .size((50.dp(), 20.dp()))
                    .clickable(move || count_handle.set(count_handle.get() + 1))
                    .test_tag("button"),
            );
            Box!(
                modifier = Modifier
                    .size((50.dp(), 20.dp()))
                    .toggleable(checked_handle.get(), move |value| checked_handle.set(value))
                    .test_tag("toggle"),
            );
            Box!(
                modifier = Modifier
                    .size((50.dp(), 20.dp()))
                    .clickable_with(
                        ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT },
                        move || disabled_handle.set(disabled_handle.get() + 1)
                    )
                    .test_tag("disabled"),
            );
        });
    });
    ui.on_node_with_tag("button").perform_semantics_action(&SemanticsActions::ON_CLICK);
    assert_eq!(count.get(), 1);
    ui.on_node_with_tag("toggle").perform_semantics_action(&SemanticsActions::ON_CLICK);
    assert!(checked.get());
    // Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, the semantics `onClick` calling `onClick()` whether enabled or not
    // A disabled clickable's `OnClick` clicks too, as Compose's: refusing a disabled node's
    // actions is the platform bridges' (D1255).
    let disabled = ui.on_node_with_tag("disabled").node_id();
    assert!(ui.host().perform_semantics_action(disabled, &SemanticsActions::ON_CLICK));
    assert_eq!(disabled_clicks.get(), 1);
}

#[test]
fn request_focus_focuses_the_node() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.size((50.dp(), 20.dp())).clickable(|| {}).test_tag("button"));
    });
    ui.on_node_with_tag("button").perform_semantics_action(&SemanticsActions::REQUEST_FOCUS);
    ui.on_node_with_tag("button").assert_is_focused();
}
