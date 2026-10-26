//! Printing nodes, as Compose's `printToString`: a node, its bounds, tag and properties (named
//! as Compose's `SemanticsProperties`), then its children to a depth.

use std::fmt::Write as _;

use compose_ui::SemanticsConfiguration;

use crate::matcher::NodeRef;

/// `node` and its descendants down to `max_depth` levels (0: the node alone, with a count of
/// its children and siblings), in Compose's format.
pub(crate) fn print_node(node: NodeRef<'_>, max_depth: usize) -> String {
    let mut out = String::new();
    print_inner(node, max_depth, 0, "", false, &mut out);
    out
}

fn print_inner(
    node: NodeRef<'_>,
    max_depth: usize,
    level: usize,
    indent: &str,
    followed_by_sibling: bool,
    out: &mut String,
) {
    let new_indent = if level == 0 {
        String::new()
    } else if followed_by_sibling {
        format!("{indent} | ")
    } else {
        format!("{indent}   ")
    };
    if level > 0 {
        let _ = write!(out, "{indent} |-");
    }
    let bounds = node.bounds();
    let _ = write!(
        out,
        "Node #{} at (l={}, t={}, r={}, b={})px",
        node.id().to_bits(),
        bounds.left,
        bounds.top,
        bounds.right,
        bounds.bottom
    );
    let config = node.config();
    if let Some(tag) = &config.test_tag {
        let _ = write!(out, ", Tag: '{tag}'");
    }
    append_config(config, &new_indent, out);
    let children: Vec<NodeRef<'_>> = node.children().collect();
    if level >= max_depth {
        let siblings = node.siblings().count();
        if !children.is_empty() || (siblings > 0 && level == 0) {
            out.push('\n');
            out.push_str(&new_indent);
            out.push_str("Has ");
            match children.len() {
                0 => {}
                1 => out.push_str("1 child"),
                count => {
                    let _ = write!(out, "{count} children");
                }
            }
            if siblings > 0 && level == 0 {
                if !children.is_empty() {
                    out.push_str(", ");
                }
                if siblings == 1 {
                    out.push_str("1 sibling");
                } else {
                    let _ = write!(out, "{siblings} siblings");
                }
            }
        }
        return;
    }
    let count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        out.push('\n');
        print_inner(child, max_depth, level + 1, &new_indent, index + 1 < count, out);
    }
}

/// The properties, sorted by Compose's names, then the flags, the actions and the merging.
fn append_config(config: &SemanticsConfiguration, indent: &str, out: &mut String) {
    let mut properties: Vec<(&str, String)> = Vec::new();
    if !config.content_description.is_empty() {
        let list: Vec<&str> = config.content_description.iter().map(|d| d.as_str()).collect();
        properties.push(("ContentDescription", format!("[{}]", list.join(", "))));
    }
    if let Some(text) = &config.editable_text {
        properties.push(("EditableText", text.to_string()));
    }
    if let Some(error) = &config.error {
        properties.push(("Error", error.to_string()));
    }
    if config.focusable {
        properties.push(("Focused", config.focused.to_string()));
    }
    if let Some(range) = config.horizontal_scroll_axis_range {
        properties.push(("HorizontalScrollAxisRange", format!("{range:?}")));
    }
    if let Some(info) = config.progress_bar_range_info {
        properties.push(("ProgressBarRangeInfo", format!("{info:?}")));
    }
    if let Some(role) = config.role {
        properties.push(("Role", format!("{role:?}")));
    }
    if let Some(selected) = config.selected {
        properties.push(("Selected", selected.to_string()));
    }
    if let Some(state) = &config.state_description {
        properties.push(("StateDescription", state.to_string()));
    }
    if !config.text.is_empty() {
        let list: Vec<&str> = config.text.iter().map(|t| t.as_str()).collect();
        properties.push(("Text", format!("[{}]", list.join(", "))));
    }
    if let Some(state) = config.toggleable_state {
        properties.push(("ToggleableState", format!("{state:?}")));
    }
    if let Some(range) = config.vertical_scroll_axis_range {
        properties.push(("VerticalScrollAxisRange", format!("{range:?}")));
    }
    properties.sort_by_key(|(name, _)| *name);
    for (name, value) in properties {
        let _ = write!(out, "\n{indent}{name} = '{value}'");
    }
    let flags: Vec<&str> = [
        (config.disabled, "Disabled"),
        (config.is_dialog, "IsDialog"),
        (config.is_popup, "IsPopup"),
        (config.password, "Password"),
        (config.selectable_group, "SelectableGroup"),
    ]
    .into_iter()
    .filter_map(|(set, name)| set.then_some(name))
    .collect();
    if !flags.is_empty() {
        let _ = write!(out, "\n{indent}[{}]", flags.join(", "));
    }
    let scrolls = config.horizontal_scroll_axis_range.is_some()
        || config.vertical_scroll_axis_range.is_some();
    let actions: Vec<&str> = [
        (config.clickable, "OnClick"),
        (config.long_clickable, "OnLongClick"),
        (config.focusable, "RequestFocus"),
        (scrolls, "ScrollBy"),
        (config.editable, "SetText"),
        (config.progress_bar_range_info.is_some(), "SetProgress"),
    ]
    .into_iter()
    .filter_map(|(set, name)| set.then_some(name))
    .collect();
    if !actions.is_empty() {
        let _ = write!(out, "\n{indent}Actions = [{}]", actions.join(", "));
    }
    if config.merge_descendants {
        let _ = write!(out, "\n{indent}MergeDescendants = 'true'");
    }
    if config.clear_descendants {
        let _ = write!(out, "\n{indent}ClearAndSetSemantics = 'true'");
    }
}
