//! The window's accessibility tree, for screen readers: AccessKit nodes built from the merged
//! semantics tree (docs/07-platforms.md), and their actions sent back as semantics actions.

use accesskit::{Action, ActionRequest, Node, NodeId as AccessNodeId, Rect, Toggled, TreeUpdate};
use compose_ui::{NodeId, Role, SemanticsAction, SemanticsNode, ToggleableState, UiHost};

/// The whole tree, as AccessKit asks for it: the window, then the merged semantics nodes, the
/// focus on the one holding the focused node.
pub(crate) fn tree_update(host: &UiHost, title: &str) -> TreeUpdate {
    host.with_tree(|tree| {
        let root = tree.semantics_tree(true);
        let focused = tree.focused();
        let mut update = TreeUpdate {
            nodes: Vec::new(),
            tree: Some(accesskit::TreeInfo::new(id(root.id))),
            tree_id: accesskit::TreeId::ROOT,
            focus: id(root.id),
        };
        let mut window = Node::new(accesskit::Role::Window);
        window.set_label(title);
        window.set_children(root.children.iter().map(|child| id(child.id)).collect::<Vec<_>>());
        let mut focus = None;
        for child in &root.children {
            add(child, &mut update.nodes, &mut |node| {
                // The innermost semantics node at or above the focused one.
                if focused.is_some_and(|focused| tree.is_at_or_below(focused, node)) {
                    focus = Some(node);
                }
            });
        }
        update.nodes.push((id(root.id), window));
        if let Some(focus) = focus {
            update.focus = id(focus);
        }
        update
    })
}

fn id(node: NodeId) -> AccessNodeId {
    AccessNodeId(node.to_bits())
}

/// Adds `node` and its descendants to `nodes`, telling `visit` each id (parents first).
fn add(node: &SemanticsNode, nodes: &mut Vec<(AccessNodeId, Node)>, visit: &mut dyn FnMut(NodeId)) {
    visit(node.id);
    let config = &node.config;
    let texts = config.text.iter().map(|text| text.as_str()).collect::<Vec<_>>().join(" ");
    let descriptions =
        config.content_description.iter().map(|text| text.as_str()).collect::<Vec<_>>().join(" ");
    let role = match config.role {
        Some(Role::Button) => accesskit::Role::Button,
        Some(Role::Checkbox) => accesskit::Role::CheckBox,
        Some(Role::Switch) => accesskit::Role::Switch,
        Some(Role::RadioButton) => accesskit::Role::RadioButton,
        Some(Role::Tab) => accesskit::Role::Tab,
        Some(Role::Image) => accesskit::Role::Image,
        Some(Role::DropdownList) => accesskit::Role::ComboBox,
        None if config.editable => accesskit::Role::TextInput,
        None if config.clickable => accesskit::Role::Button,
        None if !texts.is_empty() => accesskit::Role::Label,
        None => accesskit::Role::GenericContainer,
    };
    let mut access = Node::new(role);
    let bounds = node.bounds;
    access.set_bounds(Rect {
        x0: bounds.left as f64,
        y0: bounds.top as f64,
        x1: bounds.right as f64,
        y1: bounds.bottom as f64,
    });
    // What a screen reader says: its description, else its text (a label's own value).
    let label = if descriptions.is_empty() { texts.clone() } else { descriptions };
    if role == accesskit::Role::Label {
        access.set_value(label);
    } else if !label.is_empty() {
        access.set_label(label);
    }
    if let Some(text) = &config.editable_text {
        access.set_value(text.as_str());
    } else if let Some(state) = &config.state_description {
        access.set_value(state.as_str());
    }
    if let Some(state) = config.toggleable_state {
        access.set_toggled(match state {
            ToggleableState::On => Toggled::True,
            ToggleableState::Off => Toggled::False,
            ToggleableState::Indeterminate => Toggled::Mixed,
        });
    }
    if let Some(selected) = config.selected {
        access.set_selected(selected);
    }
    if config.disabled {
        access.set_disabled();
    } else {
        if config.clickable {
            access.add_action(Action::Click);
        }
        if config.focusable {
            access.add_action(Action::Focus);
        }
    }
    access.set_children(node.children.iter().map(|child| id(child.id)).collect::<Vec<_>>());
    nodes.push((id(node.id), access));
    for child in &node.children {
        add(child, nodes, visit);
    }
}

/// Performs an assistive technology's request: a click or a focus, as semantics actions.
/// Returns whether a node did it.
pub(crate) fn perform(host: &mut UiHost, request: &ActionRequest) -> bool {
    let action = match request.action {
        Action::Click => SemanticsAction::OnClick,
        Action::Focus => SemanticsAction::RequestFocus,
        _ => return false,
    };
    host.perform_semantics_action(NodeId::from_bits(request.target_node.0), action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::{Density, FixedTextMeasurer, IntSize, Modifier, UnitExt};

    #[test]
    fn the_tree_has_the_window_its_nodes_and_their_actions() {
        use std::cell::Cell;
        use std::rc::Rc;
        let clicks = Rc::new(Cell::new(0));
        let counted = clicks.clone();
        let mut host = UiHost::new(
            IntSize::new(200, 100),
            Density::default(),
            Box::new(FixedTextMeasurer::default()),
        );
        host.set_content(move || {
            let counted = counted.clone();
            compose_foundation::layout::Box(
                Modifier.size_wh(80.dp(), 30.dp()).clickable_with(
                    true,
                    Some(Role::Button),
                    move || counted.set(counted.get() + 1),
                ),
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(|_| {
                    compose_foundation::BasicText(
                        "Save".into(),
                        Modifier,
                        compose_ui::TextStyle::DEFAULT,
                        true,
                        u32::MAX,
                    )
                }),
            );
        });
        host.frame();
        let update = tree_update(&host, "App");
        let root = update.tree.as_ref().unwrap().root;
        let (_, window) = update.nodes.iter().find(|(node, _)| *node == root).unwrap();
        assert_eq!(window.role(), accesskit::Role::Window);
        assert_eq!(window.label(), Some("App"));
        let button = window.children()[0];
        let (_, node) = update.nodes.iter().find(|(node, _)| *node == button).unwrap();
        assert_eq!(node.role(), accesskit::Role::Button);
        assert_eq!(node.label(), Some("Save"), "the merged text");
        assert!(node.supports_action(Action::Click));
        let bounds = node.bounds().unwrap();
        assert_eq!((bounds.x1, bounds.y1), (80.0, 30.0));
        // A screen reader's click.
        let request = ActionRequest {
            action: Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: button,
            data: None,
        };
        assert!(perform(&mut host, &request));
        assert_eq!(clicks.get(), 1);
        // Its focus, then the tree's focus on it.
        let focus = ActionRequest { action: Action::Focus, ..request };
        assert!(perform(&mut host, &focus));
        host.frame();
        assert_eq!(tree_update(&host, "App").focus, button);
    }
}
