//! Adversarial review of the semantics tree and semantics actions, against Jetpack Compose's
//! `SemanticsProperties` merge policies, its modifier order and `performSemanticsAction`.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{ImageBitmap, Role, SemanticsAction, UnitExt};

fn pixel() -> ImageBitmap {
    ImageBitmap::from_rgba_premultiplied(1, 1, vec![255, 0, 0, 255])
}

/// Compose's `SemanticsProperties.Role` merges as `{ parentValue, _ -> parentValue }`: a
/// clickable row (no role) holding a described image stays without a role (a screen reader says
/// "button"/"double tap to activate"), it doesn't become an image.
#[test]
fn a_merging_parent_does_not_take_its_childs_role() {
    let mut ui = compose_test(|| {
        Row!(modifier = Modifier.clickable(|| {}).test_tag("row"), {
            Image!(pixel(), Some("Star"), modifier = Modifier.size(24.dp()));
            BasicText!("Favorite");
        });
    });
    let node = ui.on_node_with_tag("row").fetch_semantics_node();
    assert_eq!(
        node.config.content_description.iter().map(|d| d.as_str()).collect::<Vec<_>>(),
        ["Star"],
        "the description merges (a list)"
    );
    assert_eq!(
        node.config.role, None,
        "the child's Role::Image merged into the row: {:?}",
        node.config
    );
}

/// In Compose a layout node's semantics modifiers apply innermost first (`tailToHead`), so the
/// outer one's value wins: `Modifier.semantics { role = Role.Tab }.clickable(role = Button)` is a
/// tab.
#[test]
fn an_outer_semantics_modifier_wins_over_an_inner_one() {
    let mut ui = compose_test(|| {
        Box!(
            modifier = Modifier
                .test_tag("tab")
                .semantics(false, |config| config.role = Some(Role::Tab))
                .clickable_with(true, Some(Role::Button), || {})
                .size(20.dp()),
        );
    });
    assert_eq!(ui.on_node_with_tag("tab").fetch_semantics_node().config.role, Some(Role::Tab));
}

/// `RequestFocus` is an action of focusable nodes only (`Modifier.focusable`, `clickable`):
/// Compose's `performSemanticsAction(RequestFocus)` on a plain container fails ("the node is
/// missing [RequestFocus]"), it doesn't focus a descendant and report success.
#[test]
fn request_focus_on_a_node_without_the_action_is_not_performed() {
    let mut ui = compose_test(|| {
        Column!(modifier = Modifier.test_tag("column"), {
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag("button"));
        });
    });
    let column = ui.on_node_with_tag("column").node_id();
    assert!(
        !ui.host().perform_semantics_action(column, SemanticsAction::RequestFocus),
        "a non-focusable node performed RequestFocus"
    );
    ui.wait_for_idle();
    assert!(!ui.on_node_with_tag("button").fetch_semantics_node().config.focused);
}

/// Lock-in: bounds are in the root's pixels (density 2: a 20 dp box 10 dp in is 40 px at 20 px),
/// in a dialog too, and follow a scroll.
#[test]
fn passes_bounds_are_in_pixels_and_follow_scrolling() {
    use compose_ui::{Density, FixedTextMeasurer, IntSize};
    thread_local! { static STATE: std::cell::RefCell<Option<ScrollState>> = const { std::cell::RefCell::new(None) }; }
    let mut ui = compose_test_with(
        IntSize::new(400, 400),
        Density::new(2.0),
        Box::new(FixedTextMeasurer::new()),
        || {
            let state = remember_scroll_state();
            STATE.with(|cell| *cell.borrow_mut() = Some(state.clone()));
            Column!(modifier = Modifier.height(50.dp()).vertical_scroll(state), {
                Box!(
                    modifier =
                        Modifier.padding(10.dp()).size(20.dp()).clickable(|| {}).test_tag("a")
                );
                Box!(modifier = Modifier.size(100.dp()).test_tag("b"));
            });
        },
    );
    let a = ui.on_node_with_tag("a").fetch_semantics_node().bounds;
    assert_eq!((a.left, a.top, a.width()), (20.0, 20.0, 40.0));
    STATE.with(|cell| cell.borrow().as_ref().unwrap().scroll_to(10));
    ui.wait_for_idle();
    let a = ui.on_node_with_tag("a").fetch_semantics_node().bounds;
    assert_eq!(a.top, 10.0);
}
