//! Adversarial review of the desktop accessibility tree (AccessKit, docs/07-platforms.md
//! "Accessibility"). The bridge is a private module, so this test compiles that same file as a
//! module of its own (`#[path]`), as review_f.rs does for the input mapping.

#![allow(dead_code, non_snake_case)]

extern crate compose_runtime as compose;

#[path = "../src/accessibility.rs"]
mod accessibility;

use accesskit::{Node, NodeId as AccessNodeId, Toggled, TreeUpdate};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_ui::ClickableOptions;
use compose_ui::{
    Density, Dialog, ImageBitmap, IntSize, Popup, PopupProperties, Role, UnitExt,
    platform::{FixedTextMeasurer, UiHost},
};

fn host(content: impl Fn() + 'static) -> UiHost {
    let mut host =
        UiHost::new(IntSize::new(400, 400), Density::default(), Box::new(FixedTextMeasurer::new()));
    host.set_content(content);
    host.frame();
    host
}

fn node(update: &TreeUpdate, id: AccessNodeId) -> &Node {
    &update.nodes.iter().find(|(node, _)| *node == id).expect("a node listed in the update").1
}

/// The nodes of the update other than the window.
fn nodes(update: &TreeUpdate) -> Vec<&Node> {
    let root = update.tree.as_ref().unwrap().root;
    update.nodes.iter().filter(|(id, _)| *id != root).map(|(_, node)| node).collect()
}

/// A radio button's checked state is AccessKit's `toggled` (ARIA's `aria-checked`): macOS'
/// adapter reads a radio button's value from `toggled` only (`selected` is for tabs and list
/// items), so a selected radio button, as Compose's `selectable(selected, role = RadioButton)`,
/// must be toggled on, or VoiceOver never says it's selected.
#[test]
fn a_selected_radio_button_is_checked() {
    let host = host(|| {
        Box!(
            modifier = Modifier
                .size(20.dp())
                .clickable_with(
                    ClickableOptions { role: Some(Role::RadioButton), ..ClickableOptions::DEFAULT },
                    || {}
                )
                .semantics(|config| config.set_selected(true)),
        );
    });
    let update = accessibility::tree_update(&host, "App");
    let radio = nodes(&update)
        .into_iter()
        .find(|node| node.role() == accesskit::Role::RadioButton)
        .expect("a radio button");
    assert_eq!(radio.toggled(), Some(Toggled::True), "the radio button isn't checked");
}

/// Compose's dialogs are windows of their own (a `DialogWindow` on desktop, `isDialog` /
/// `paneTitle` on Android): a screen reader is told a dialog opened and stays in it. Here the
/// dialog's content is listed among the window's children, beside the content behind the scrim,
/// with no `Role::Dialog` (or `AlertDialog`) node and nothing modal.
#[test]
fn a_dialog_is_a_modal_dialog_node() {
    let host = host(|| {
        Column!({
            BasicText!("Behind");
            Dialog!(on_dismiss_request = || {}, {
                BasicText!("Delete the file?");
            });
        });
    });
    let update = accessibility::tree_update(&host, "App");
    let dialog = nodes(&update)
        .into_iter()
        .find(|node| matches!(node.role(), accesskit::Role::Dialog | accesskit::Role::AlertDialog));
    let dialog = dialog.expect("no dialog node in the accessibility tree");
    assert!(dialog.is_modal(), "the dialog isn't modal");
}

/// A pane's title is its name, where a drawer or a sheet has no other (D1029).
#[test]
fn a_panes_title_is_its_label() {
    let host = host(|| {
        Box!(modifier = Modifier.size(10.dp()).semantics(|config| {
            config.set_pane_title("Navigation menu");
        }));
    });
    let update = accessibility::tree_update(&host, "App");
    let pane = nodes(&update)
        .into_iter()
        .find(|node| node.role() == accesskit::Role::Pane)
        .expect("no pane in the accessibility tree");
    assert_eq!(pane.label(), Some("Navigation menu"));
}

/// A clickable row (no role) holding a described image is a button to AccessKit (as Compose:
/// clickable, no role); the merged child's `Role::Image` makes it an image, which VoiceOver
/// announces as "Star, Favorite, image".
#[test]
fn a_clickable_row_with_an_image_is_a_button() {
    let host = host(|| {
        Row!(modifier = Modifier.clickable(|| {}), {
            Image!(
                ImageBitmap::from_rgba_premultiplied(1, 1, vec![255, 0, 0, 255]),
                Some("Star"),
                modifier = Modifier.size(24.dp())
            );
            BasicText!("Favorite");
        });
    });
    let update = accessibility::tree_update(&host, "App");
    let root = update.tree.as_ref().unwrap().root;
    let row = node(&update, node(&update, root).children()[0]);
    assert_eq!(row.role(), accesskit::Role::Button);
}

/// Lock-in: every child an update lists is in it, no id is listed twice, and the focus is on a
/// node of the update, with a popup and a dialog open.
#[test]
fn passes_the_update_lists_every_child_once_and_focuses_a_listed_node() {
    let host = host(|| {
        Column!({
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}), {
                BasicText!("A");
            });
            Popup!(properties = PopupProperties { focusable: true, ..Default::default() }, {
                Box!(modifier = Modifier.size(20.dp()).clickable(|| {}), {
                    BasicText!("B");
                });
            });
            Dialog!(on_dismiss_request = || {}, {
                Box!(modifier = Modifier.size(20.dp()).clickable(|| {}), {
                    BasicText!("C");
                });
            });
        });
    });
    let update = accessibility::tree_update(&host, "App");
    let mut ids: Vec<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
    let count = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), count, "an id listed twice");
    for (_, node) in &update.nodes {
        for child in node.children() {
            assert!(ids.contains(&child.0), "child {child:?} isn't in the update");
        }
    }
    assert!(ids.contains(&update.focus.0));
}
