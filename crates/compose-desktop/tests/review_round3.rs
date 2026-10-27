//! Third adversarial review of the desktop accessibility tree (AccessKit). The bridge is a
//! private module, compiled here as a module of its own (`#[path]`), as review_accessibility.rs
//! does.

#![allow(dead_code, non_snake_case)]

extern crate compose_runtime as compose;

#[path = "../src/accessibility.rs"]
mod accessibility;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_ui::{
    Density, IntSize, Popup, PopupProperties, UnitExt,
    platform::{FixedTextMeasurer, UiHost},
};

fn host(content: impl Fn() + 'static) -> UiHost {
    let mut host =
        UiHost::new(IntSize::new(400, 400), Density::default(), Box::new(FixedTextMeasurer::new()));
    host.set_content(content);
    host.frame();
    host
}

/// Lock-in: a focusable popup (a menu) takes the keyboard focus when it opens, its own window
/// in Compose; AccessKit's focus is on the popup's root node (focusable, focused), not on the
/// clickable card behind it that the popup is composed in.
#[test]
fn passes_a_focused_popup_does_not_put_the_focus_on_the_node_behind_it() {
    let host = host(|| {
        Box!(modifier = Modifier.size(100.dp()).clickable(|| {}).test_tag("card"), {
            BasicText!("Card");
            Popup!(properties = PopupProperties { focusable: true, ..Default::default() }, {
                Box!(modifier = Modifier.size(50.dp()).clickable(|| {}), {
                    BasicText!("Item");
                });
            });
        });
    });
    let card = host.with_tree(|tree| {
        let root = tree.semantics_tree(true);
        root.flatten()
            .into_iter()
            .find(|node| {
                node.config.get(&compose_ui::SemanticsProperties::TEST_TAG).cloned().as_deref()
                    == Some("card")
            })
            .map(|node| node.id)
            .expect("the card")
    });
    assert!(host.with_tree(|tree| tree.focused()).is_some(), "the popup didn't take the focus");
    let update = accessibility::tree_update(&host, "App");
    assert_ne!(
        update.focus,
        accesskit::NodeId(card.to_bits()),
        "AccessKit's focus is on the card behind the open popup"
    );
}

/// AccessKit's Focus on a text field (what VoiceOver does to edit it): the field's node is
/// focusable, so the bridge offers the action, and Compose's `CoreTextField` node performs
/// `RequestFocus`. Here performing it fails: the node is the decoration box, whose own chain has
/// no focus target, and `RequestFocus` now asks for one there.
#[test]
fn accesskit_focus_on_a_text_field_focuses_it() {
    let mut host = host(|| {
        BasicTextField!(
            value = String::from("Hello"),
            on_value_change = |_| {},
            modifier = Modifier.width(200.dp()),
        );
    });
    let update = accessibility::tree_update(&host, "App");
    let (id, field) = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == accesskit::Role::TextInput)
        .expect("a text input node");
    assert!(field.supports_action(accesskit::Action::Focus), "the field offers no Focus action");
    let request = accesskit::ActionRequest {
        action: accesskit::Action::Focus,
        target_tree: accesskit::TreeId::ROOT,
        target_node: *id,
        data: None,
    };
    assert!(accessibility::perform(&mut host, &request), "Focus on the text field did nothing");
    host.frame();
    assert!(host.with_tree(|tree| tree.focused()).is_some(), "the text field isn't focused");
}
