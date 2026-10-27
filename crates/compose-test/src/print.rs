//! Printing nodes, as Compose's `printToString`: a node, its bounds, tag and properties (named
//! as Compose's `SemanticsProperties`), then its children to a depth.

use std::fmt::Write as _;

use compose_ui::{SemanticsConfiguration, SemanticsProperties, ValueSlot};

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
    if let Some(tag) = config.get(&SemanticsProperties::TEST_TAG) {
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

/// The properties, sorted by name (the test tag left out: it's on the node's line), then the
/// flags (`Unit` values), the actions and the merging, as Compose's `appendConfigInfo`.
fn append_config(config: &SemanticsConfiguration, indent: &str, out: &mut String) {
    let mut properties: Vec<(&str, &ValueSlot)> = config
        .properties()
        .filter(|(key, _)| key.name() != SemanticsProperties::TEST_TAG.name())
        .map(|(key, value)| (key.name(), value))
        .collect();
    properties.sort_by_key(|(name, _)| *name);
    let mut flags = Vec::new();
    let mut actions = Vec::new();
    for (name, value) in properties {
        if value.is_action() {
            actions.push(name);
        } else if value.is_unit() {
            flags.push(name);
        } else {
            let _ = write!(out, "\n{indent}{name} = '{}'", value.display());
        }
    }
    if !flags.is_empty() {
        let _ = write!(out, "\n{indent}[{}]", flags.join(", "));
    }
    if !actions.is_empty() {
        let _ = write!(out, "\n{indent}Actions = [{}]", actions.join(", "));
    }
    if config.is_merging_semantics_of_descendants {
        let _ = write!(out, "\n{indent}MergeDescendants = 'true'");
    }
    if config.is_clearing_semantics {
        let _ = write!(out, "\n{indent}ClearAndSetSemantics = 'true'");
    }
}
