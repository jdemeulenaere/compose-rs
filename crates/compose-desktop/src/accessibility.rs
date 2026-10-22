//! The window's accessibility tree, for screen readers: AccessKit nodes built from the merged
//! semantics tree (docs/07-platforms.md), and their actions sent back as semantics actions.

use accesskit::{Action, ActionRequest, Node, NodeId as AccessNodeId, Rect, Toggled, TreeUpdate};
use compose_ui::{
    Role, SemanticsAction, ToggleableState,
    platform::{NodeId, SemanticsNode, UiHost},
};

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
        // A role added later: a generic container.
        Some(_) => accesskit::Role::GenericContainer,
        None if config.is_dialog => accesskit::Role::Dialog,
        None if config.password => accesskit::Role::PasswordInput,
        None if config.editable => accesskit::Role::TextInput,
        None if config.progress_bar_range_info.is_some() => accesskit::Role::Slider,
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
    if let Some(error) = &config.error {
        access.set_invalid(accesskit::Invalid::True);
        access.set_description(error.as_str());
    }
    if let Some(state) = config.toggleable_state {
        access.set_toggled(match state {
            ToggleableState::On => Toggled::True,
            ToggleableState::Off => Toggled::False,
            ToggleableState::Indeterminate => Toggled::Mixed,
        });
    }
    // A radio button's selection is its checked state, as ARIA's `aria-checked`.
    match config.selected {
        Some(selected) if role == accesskit::Role::RadioButton => {
            access.set_toggled(if selected { Toggled::True } else { Toggled::False });
        }
        Some(selected) => access.set_selected(selected),
        None => {}
    }
    if config.is_dialog {
        access.set_modal();
    }
    if let Some(info) = config.progress_bar_range_info {
        access.set_numeric_value(info.current as f64);
        access.set_min_numeric_value(info.range.0 as f64);
        access.set_max_numeric_value(info.range.1 as f64);
        access.set_numeric_value_step(progress_step(info) as f64);
        if !config.disabled {
            access.add_action(Action::Increment);
            access.add_action(Action::Decrement);
            access.add_action(Action::SetValue);
        }
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

/// How far a screen reader's increment moves a value: a step, or a twentieth of the range when
/// continuous, as Compose's accessibility delegate.
fn progress_step(info: compose_ui::ProgressBarRangeInfo) -> f32 {
    let length = info.range.1 - info.range.0;
    if info.steps > 0 { length / (info.steps + 1) as f32 } else { length / 20.0 }
}

/// Performs an assistive technology's request (a click, a focus, a value's increment or a new
/// value) as a semantics action. Returns whether a node did it.
pub(crate) fn perform(host: &mut UiHost, request: &ActionRequest) -> bool {
    let node = NodeId::from_bits(request.target_node.0);
    let progress = || {
        host.with_tree(|tree| {
            let root = tree.semantics_tree(true);
            fn find(node: &SemanticsNode, id: NodeId) -> Option<&SemanticsNode> {
                if node.id == id {
                    return Some(node);
                }
                node.children.iter().find_map(|child| find(child, id))
            }
            find(&root, node).and_then(|found| found.config.progress_bar_range_info)
        })
    };
    let action = match request.action {
        Action::Click => SemanticsAction::OnClick,
        Action::Focus => SemanticsAction::RequestFocus,
        Action::Increment | Action::Decrement => {
            let Some(info) = progress() else { return false };
            let step = progress_step(info);
            let delta = if request.action == Action::Increment { step } else { -step };
            SemanticsAction::SetProgress(info.current + delta)
        }
        Action::SetValue => match &request.data {
            Some(accesskit::ActionData::NumericValue(value)) => {
                SemanticsAction::SetProgress(*value as f32)
            }
            Some(accesskit::ActionData::Value(text)) => match text.trim().parse::<f32>() {
                Ok(value) => SemanticsAction::SetProgress(value),
                Err(_) => return false,
            },
            _ => return false,
        },
        _ => return false,
    };
    host.perform_semantics_action(node, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::{
        ClickableOptions, Density, IntSize, Modifier, UnitExt, platform::FixedTextMeasurer,
    };

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
                Modifier.size((80.dp(), 30.dp())).clickable_with(
                    ClickableOptions { role: Some(Role::Button), ..ClickableOptions::DEFAULT },
                    move || counted.set(counted.get() + 1),
                ),
                compose_ui::Alignment::TOP_START,
                false,
                compose_runtime::callback(|_| {
                    compose_foundation::BasicText(
                        "Save".into(),
                        Modifier,
                        compose_ui::TextStyle::DEFAULT,
                        None,
                        compose_ui::TextOverflow::Clip,
                        true,
                        u32::MAX,
                        1,
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

    #[test]
    fn progress_steps_are_a_stop_or_a_twentieth_of_the_range() {
        let info =
            |steps| compose_ui::ProgressBarRangeInfo { current: 0.0, range: (0.0, 10.0), steps };
        assert_eq!(progress_step(info(0)), 0.5);
        assert_eq!(progress_step(info(4)), 2.0);
    }
}
