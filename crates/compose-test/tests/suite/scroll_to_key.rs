//! `perform_scroll_to_key` on lazy containers and semantic nodes, as Compose's
//! `ui-test/.../actions/ScrollToKeyTest.kt`.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, Modifier, UnitExt};

const LIST_TAG: &str = "list";
const BOX_TAG: &str = "tag";

#[track_caller]
fn expect_error(action: impl FnOnce()) -> String {
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
    let panic = failed.expect_err("expected an assertion error");
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(500, 500), content)
}

fn LazyColumnContent(modifier: Modifier) {
    LazyColumn!(
        modifier = modifier.required_size(100.dp()).test_tag(LIST_TAG),
        |scope: LazyListScope| {
            scope.items_keyed(
                20,
                |index| format!("key_{index}"),
                |index| {
                    Spacer(Modifier.required_size(30.dp()).test_tag(format!("tag_{index}")));
                },
            );
        }
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToKeyTest.kt, scrollToKeyInsideViewport
#[test]
fn scroll_to_key_inside_viewport() {
    let mut ui = rule(|| LazyColumnContent(Modifier));
    ui.on_node_with_tag("tag_1").assert_is_displayed();
    ui.on_node(has_scroll_to_key_action()).perform_scroll_to_key("key_1");
    ui.on_node_with_tag("tag_1").assert_is_displayed();
    ui.on_node_with_tag("tag_0").assert_does_not_exist();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToKeyTest.kt, scrollToKeyOutOfViewport
#[test]
fn scroll_to_key_out_of_viewport() {
    let mut ui = rule(|| LazyColumnContent(Modifier));
    ui.on_node_with_tag("tag_10").assert_does_not_exist();
    ui.on_node(has_scroll_to_key_action()).perform_scroll_to_key("key_10");
    ui.on_node_with_tag("tag_10").assert_is_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToKeyTest.kt, scrollToNonExistentKey
#[test]
fn scroll_to_non_existent_key() {
    let mut ui = rule(|| LazyColumnContent(Modifier));
    let message = expect_error(|| {
        ui.on_node(has_scroll_to_key_action()).perform_scroll_to_key("hello");
    });
    assert!(
        message.contains(
            "Failed to scroll to the item identified by \"hello\", couldn't find the key."
        ),
        "{message}"
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToKeyTest.kt, missingSemantics_ScrollToIndex
#[test]
fn missing_semantics_scroll_to_index() {
    let mut ui = rule(|| {
        Box!(
            modifier = Modifier
                .semantics(|config| {
                    config.index_for_key(|_| 1);
                })
                .test_tag(BOX_TAG)
        );
    });
    ui.on_node_with_tag(BOX_TAG).assert(!has_scroll_to_key_action());
    let message = expect_error(|| {
        ui.on_node_with_tag(BOX_TAG).perform_scroll_to_key(1);
    });
    assert!(
        message.contains(
            "Failed to scroll to the item identified by \"1\", the node is missing [ScrollToIndex]"
        ),
        "{message}"
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToKeyTest.kt, missingSemantics_IndexForKey
#[test]
fn missing_semantics_index_for_key() {
    let mut ui = rule(|| {
        Box!(
            modifier = Modifier
                .semantics(|config| {
                    config.scroll_to_index(None, |_| true);
                })
                .test_tag(BOX_TAG)
        );
    });
    ui.on_node_with_tag(BOX_TAG).assert(!has_scroll_to_key_action());
    let message = expect_error(|| {
        ui.on_node_with_tag(BOX_TAG).perform_scroll_to_key(1);
    });
    assert!(
        message.contains(
            "Failed to scroll to the item identified by \"1\", the node is missing [IndexForKey]"
        ),
        "{message}"
    );
}

#[test]
fn missing_semantics_index_for_key_and_scroll_to_index() {
    let mut ui = rule(|| {
        Box!(modifier = Modifier.semantics(|_| {}).test_tag(BOX_TAG));
    });
    let message = expect_error(|| {
        ui.on_node_with_tag(BOX_TAG).perform_scroll_to_key(1);
    });
    assert!(
        message.contains(
            "Failed to scroll to the item identified by \"1\", the node is missing [IndexForKey, ScrollToIndex]"
        ),
        "{message}"
    );
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/actions/ScrollToKeyTest.kt, performScrollToKey_invokesActionOnOnUiThread
#[test]
fn perform_scroll_to_key_invokes_action_on_ui_thread() {
    let scrolled_to = Rc::new(Cell::new(None));
    let recorded = scrolled_to.clone();
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        LazyColumnContent(Modifier.semantics(move |config| {
            let recorded = recorded.clone();
            config.scroll_to_index(None, move |index| {
                recorded.set(Some(index));
                true
            });
        }));
    });
    ui.on_node_with_tag(LIST_TAG).perform_scroll_to_key("key_10");
    assert_eq!(scrolled_to.get(), Some(10));
}
