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
                        || panic!("disabled")
                    )
                    .test_tag("disabled"),
            );
        });
    });
    ui.on_node_with_tag("button").perform_semantics_action(&SemanticsActions::ON_CLICK);
    assert_eq!(count.get(), 1);
    ui.on_node_with_tag("toggle").perform_semantics_action(&SemanticsActions::ON_CLICK);
    assert!(checked.get());
    let disabled = ui.on_node_with_tag("disabled").node_id();
    assert!(!ui.host().perform_semantics_action(disabled, &SemanticsActions::ON_CLICK));
}

#[test]
fn request_focus_focuses_the_node() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.size((50.dp(), 20.dp())).clickable(|| {}).test_tag("button"));
    });
    ui.on_node_with_tag("button").perform_semantics_action(&SemanticsActions::REQUEST_FOCUS);
    ui.on_node_with_tag("button").assert_is_focused();
}
