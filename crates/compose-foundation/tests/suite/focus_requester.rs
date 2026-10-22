//! `FocusRequester` and `Modifier.focus_requester`, as Compose's.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{LaunchedEffect, mutable_state_of, remember};
use compose_test::*;
use compose_ui::{FocusRequester, UnitExt};

#[test]
fn an_effect_focuses_a_field_when_it_shows() {
    let mut ui = compose_test(|| {
        let requester = remember(FocusRequester::new);
        let text = remember(|| mutable_state_of(String::new()));
        let shown = requester;
        LaunchedEffect((), async move {
            shown.request_focus();
        });
        BasicTextField!(
            text.get(),
            move |value: String| text.set(value),
            modifier = Modifier.focus_requester(requester).test_tag("field")
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_focused();
}

#[test]
fn a_click_moves_focus_to_another_node() {
    let mut ui = compose_test(|| {
        let requester = remember(FocusRequester::new);
        let target = requester;
        Column!({
            Box!(
                modifier = Modifier.size(20.dp()).test_tag("button").clickable(move || {
                    target.request_focus();
                })
            );
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .focus_requester(requester)
                    .focusable()
                    .test_tag("target")
            );
        });
    });
    ui.on_node_with_tag("button").perform_click();
    ui.on_node_with_tag("target").assert_is_focused();
}

#[test]
fn a_requester_without_a_node_does_nothing() {
    let requester = FocusRequester::new();
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let attached = requester;
    let mut ui = compose_test(move || {
        if handle.get() {
            Box!(modifier = Modifier.focus_requester(attached).focusable());
        }
    });
    shown.set(false);
    ui.wait_for_idle();
    assert!(!requester.request_focus());
    ui.wait_for_idle();
}
