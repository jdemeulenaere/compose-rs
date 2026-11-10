//! Compose's `VisibleTest`: `Modifier.visible(visible)` keeps a node's measured space while
//! skipping placement (not drawn, hit or focused) when hidden.

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::{ComposeTestRule, has_test_tag};
use compose_ui::{
    Density, FocusRequester, IntSize, LayoutCoordinates, SemanticsProperties, UnitExt,
};

const TEST_TAG: &str = "visibility";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), content)
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_true_isDisplayed
#[test]
fn visible_true_is_displayed() {
    let mut ui = rule(|| {
        Box!(modifier = Modifier.size(50.dp()).visible(true).test_tag(TEST_TAG));
    });
    ui.on_node_with_tag(TEST_TAG).assert_is_displayed();
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_false_isNotDisplayed
#[test]
fn visible_false_is_not_displayed() {
    let mut ui = rule(|| {
        Box!(modifier = Modifier.size(50.dp()).visible(false).test_tag(TEST_TAG));
    });
    ui.on_node_with_tag(TEST_TAG).assert_is_not_displayed();
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_false_occupiesSpace
#[test]
fn visible_false_occupies_space() {
    let parent_size = Rc::new(Cell::new(IntSize::ZERO));
    let recorded = parent_size.clone();
    let mut ui = rule(move || {
        let recorded = recorded.clone();
        Box!(
            modifier =
                Modifier.on_globally_positioned(move |c: LayoutCoordinates| recorded.set(c.size())),
            |_| {
                Box!(modifier = Modifier.size(50.dp()).visible(false).test_tag(TEST_TAG));
            }
        );
    });
    ui.on_node_with_tag(TEST_TAG).assert_is_not_displayed();
    ui.wait_for_idle();
    let expected = Density::default().round_to_px(50.dp());
    assert_eq!(parent_size.get().width, expected);
    assert_eq!(parent_size.get().height, expected);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_toggle_changesDisplay
#[test]
fn visible_toggle_changes_display() {
    let is_visible = mutable_state_of(true);
    let current = is_visible.handle();
    let mut ui = rule(move || {
        Box!(modifier = Modifier.size(50.dp()).visible(current.get()).test_tag(TEST_TAG));
    });
    ui.on_node_with_tag(TEST_TAG).assert_is_displayed();
    ui.run_on_idle(|_| is_visible.set(false));
    ui.on_node_with_tag(TEST_TAG).assert_is_not_displayed();
    ui.run_on_idle(|_| is_visible.set(true));
    ui.on_node_with_tag(TEST_TAG).assert_is_displayed();
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_false_isNotClickable
#[test]
fn visible_false_is_not_clickable() {
    let clicked = Rc::new(Cell::new(false));
    let mark = clicked.clone();
    let mut ui = rule(move || {
        let mark = mark.clone();
        Box!(
            modifier = Modifier
                .size(50.dp())
                .visible(false)
                .clickable(move || mark.set(true))
                .test_tag(TEST_TAG)
        );
    });
    ui.on_node_with_tag(TEST_TAG).perform_click();
    ui.run_on_idle(|_| assert!(!clicked.get()));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_false_childIsNotClickable
#[test]
#[ignore = "unplaced content: a child of content a layout modifier doesn't place isn't in the semantics tree (bounds.rs's 6, D1037), so it can't be found to click"]
fn visible_false_child_is_not_clickable() {
    let clicked = Rc::new(Cell::new(false));
    let mark = clicked.clone();
    let mut ui = rule(move || {
        let mark = mark.clone();
        Box!(modifier = Modifier.size(50.dp()).visible(false).test_tag(TEST_TAG), |_| {
            let mark = mark.clone();
            Box!(
                modifier =
                    Modifier.size(20.dp()).clickable(move || mark.set(true)).test_tag("child")
            );
        });
    });
    ui.on_node_with_tag("child").perform_click();
    ui.run_on_idle(|_| assert!(!clicked.get()));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_false_isNotFocusable
#[test]
fn visible_false_is_not_focusable() {
    let requester = FocusRequester::new();
    let mut ui = rule(move || {
        Box!(
            modifier = Modifier
                .size(50.dp())
                .visible(false)
                .focus_requester(requester)
                .focusable()
                .test_tag(TEST_TAG)
        );
    });
    ui.run_on_idle(|_| {
        requester.request_focus();
    });
    ui.on_node_with_tag(TEST_TAG).assert_is_not_focused();
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/VisibleTest.kt, visible_toggle_losesFocus
#[test]
fn visible_toggle_loses_focus() {
    let is_visible = mutable_state_of(true);
    let current = is_visible.handle();
    let requester = FocusRequester::new();
    let mut ui = rule(move || {
        Box!(
            modifier = Modifier
                .size(50.dp())
                .visible(current.get())
                .focus_requester(requester)
                .focusable()
                .test_tag(TEST_TAG)
        );
    });
    ui.run_on_idle(|_| {
        requester.request_focus();
    });
    ui.on_node_with_tag(TEST_TAG).assert_is_focused();
    ui.run_on_idle(|_| is_visible.set(false));
    ui.on_node_with_tag(TEST_TAG).assert_is_not_focused();
}

/// `Modifier.visible(false)` keeps its measured space in its parent, sets `VISIBLE` semantics, and
/// clears `FOCUSED` semantics even before layout-modifier unplacement propagates across the tree.
#[test]
fn visible_false_keeps_parent_space_and_updates_semantics() {
    let is_visible = mutable_state_of(true);
    let current = is_visible.handle();
    let parent_size = Rc::new(Cell::new(IntSize::ZERO));
    let sibling_x = Rc::new(Cell::new(-1.0));
    let (ps, sx) = (parent_size.clone(), sibling_x.clone());
    let requester = FocusRequester::new();
    let mut ui = rule(move || {
        let (ps, sx) = (ps.clone(), sx.clone());
        Row!(
            modifier =
                Modifier.on_globally_positioned(move |c: LayoutCoordinates| ps.set(c.size())),
            |_| {
                Box!(
                    modifier = Modifier
                        .size(50.dp())
                        .visible(current.get())
                        .focus_requester(requester)
                        .focusable()
                        .test_tag(TEST_TAG)
                );
                let sx = sx.clone();
                Box!(
                    modifier = Modifier.size(20.dp()).on_globally_positioned(
                        move |c: LayoutCoordinates| sx.set(c.position_in_parent().x)
                    )
                );
            }
        );
    });
    ui.run_on_idle(|_| {
        assert!(requester.request_focus());
    });
    let expected_50 = Density::default().round_to_px(50.dp());
    let expected_20 = Density::default().round_to_px(20.dp());
    assert_eq!(parent_size.get(), IntSize::new(expected_50 + expected_20, expected_50));
    assert_eq!(sibling_x.get(), expected_50 as f32);
    let config = ui.on_node(has_test_tag(TEST_TAG)).fetch_semantics_snapshot().config;
    assert_eq!(config.get(&SemanticsProperties::VISIBLE), Some(&true));
    assert_eq!(config.get(&SemanticsProperties::FOCUSED), Some(&true));

    ui.run_on_idle(|_| is_visible.set(false));
    assert_eq!(parent_size.get(), IntSize::new(expected_50 + expected_20, expected_50));
    assert_eq!(sibling_x.get(), expected_50 as f32);
    // Until w2-layout-scheduler unplaces the node when its modifier doesn't place, it remains in
    // semantics_tree with `VISIBLE = false` and `FOCUSED = false`.
    let config = ui.on_node(has_test_tag(TEST_TAG)).fetch_semantics_snapshot().config;
    assert_eq!(config.get(&SemanticsProperties::VISIBLE), Some(&false));
    assert_eq!(config.get(&SemanticsProperties::FOCUSED), Some(&false));

    // Also verifies `Modifier.visible` equality (`same`).
    assert!(Modifier.visible(false).same(&Modifier.visible(false)));
    assert!(!Modifier.visible(false).same(&Modifier.visible(true)));
}

/// Toggling `Modifier.visible` toggles `VISIBLE` semantics and modifier equality.
#[test]
fn visible_modifier_toggles_semantics_and_equality() {
    let is_visible = mutable_state_of(false);
    let current = is_visible.handle();
    let mut ui = rule(move || {
        Box!(modifier = Modifier.size(40.dp()).visible(current.get()).test_tag(TEST_TAG));
    });
    let config = ui.on_node(has_test_tag(TEST_TAG)).fetch_semantics_snapshot().config;
    assert_eq!(config.get(&SemanticsProperties::VISIBLE), Some(&false));

    ui.run_on_idle(|_| is_visible.set(true));
    let config = ui.on_node(has_test_tag(TEST_TAG)).fetch_semantics_snapshot().config;
    assert_eq!(config.get(&SemanticsProperties::VISIBLE), Some(&true));
    ui.on_node_with_tag(TEST_TAG).assert_is_displayed();
}
