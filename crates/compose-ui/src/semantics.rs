//! Semantics: what nodes mean (text, role, state, actions), as Compose's semantics tree. Tests
//! find nodes through it, and accessibility bridges read it (docs/06-ui.md).

use std::fmt::Write as _;

use compose_runtime::ComposeArg;
use smallvec::SmallVec;

pub use crate::semantics_properties::*;
use crate::tree::NodeId;
use crate::unit::{Offset, Rect};

/// A node's role, as `Role`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum Role {
    Button,
    Checkbox,
    Switch,
    RadioButton,
    Tab,
    Image,
    DropdownList,
    ValuePicker,
    Carousel,
}

impl ComposeArg for Role {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// How far a scrolling container is scrolled along an axis, as Compose's `ScrollAxisRange`: its
/// offset, the largest it can be (a lazy list's estimated from its average item size where its
/// end isn't laid out), and whether its content runs from the end (a reversed layout).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollAxisRange {
    pub value: f32,
    pub max_value: f32,
    pub reverse_scrolling: bool,
}

/// A value in a range, as Compose's `ProgressBarRangeInfo`: a slider's or a progress bar's.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ProgressBarRangeInfo {
    pub current: f32,
    pub range: (f32, f32),
    /// The stops between the ends (0: continuous).
    pub steps: u32,
}

impl ProgressBarRangeInfo {
    /// An indeterminate progress, as `ProgressBarRangeInfo.Indeterminate`: 0 in an empty range.
    pub const INDETERMINATE: ProgressBarRangeInfo =
        ProgressBarRangeInfo { current: 0.0, range: (0.0, 0.0), steps: 0 };
}

/// A toggle's state, as `ToggleableState`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToggleableState {
    On,
    Off,
    Indeterminate,
}

impl From<bool> for ToggleableState {
    fn from(value: bool) -> ToggleableState {
        if value { ToggleableState::On } else { ToggleableState::Off }
    }
}

/// A node of the semantics tree: a layout node with semantics, its bounds in the root (in
/// pixels), and its configuration (merged, in the merged tree).
#[derive(Clone, Debug)]
pub struct SemanticsNode {
    pub id: NodeId,
    pub bounds: Rect,
    /// Laid out right to left, as Compose's `layoutInfo.layoutDirection`: a horizontal
    /// scroll's left and right go the other way (D484).
    pub rtl: bool,
    pub config: SemanticsConfiguration,
    /// A merging node's own configuration, in the merged tree (`None`: the same as `config`):
    /// what Compose's accessibility reads actions and scrolls from (`unmergedConfig`, D491).
    pub own_config: Option<Box<SemanticsConfiguration>>,
    pub children: Vec<SemanticsNode>,
}

impl SemanticsNode {
    /// The node's own configuration, unmerged, as Compose's `unmergedConfig`.
    pub fn unmerged_config(&self) -> &SemanticsConfiguration {
        self.own_config.as_deref().unwrap_or(&self.config)
    }

    /// The center of the node's bounds, where test clicks land.
    pub fn center(&self) -> Offset {
        Offset::new(
            (self.bounds.left + self.bounds.right) / 2.0,
            (self.bounds.top + self.bounds.bottom) / 2.0,
        )
    }

    /// This node and its descendants, depth-first.
    pub fn flatten(&self) -> Vec<&SemanticsNode> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            out.push(node);
            stack.extend(node.children.iter().rev());
        }
        out
    }

    /// An indented dump, one node per line.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_into(&mut out, 0);
        out
    }

    fn dump_into(&self, out: &mut String, depth: usize) {
        let _ = writeln!(
            out,
            "{}[{},{} {}x{}] {:?}",
            "  ".repeat(depth),
            self.bounds.left,
            self.bounds.top,
            self.bounds.width(),
            self.bounds.height(),
            self.config
        );
        for child in &self.children {
            child.dump_into(out, depth + 1);
        }
    }
}

// ---- The semantics tree ----------------------------------------------------------------------

use crate::node_chain::NodeKinds;
use crate::tree::UiTree;

thread_local! {
    /// The semantics tree being built takes deactivated content in
    /// ([`UiTree::semantics_tree_with_deactivated`]).
    static WITH_DEACTIVATED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl UiTree {
    /// Whether `node` is left out of the semantics tree: not placed, unless it's deactivated
    /// content and the tree takes that in.
    fn left_out(&self, node: NodeId) -> bool {
        let value = self.node(node);
        !value.placed && !(value.deactivated && WITH_DEACTIVATED.with(std::cell::Cell::get))
    }

    /// The semantics tree with deactivated content in it too (a lazy list's items kept for
    /// reuse), where they were last placed, as Compose's `skipDeactivatedNodes = false`: for
    /// tests' `assert_is_deactivated`.
    pub fn semantics_tree_with_deactivated(&mut self, merged: bool) -> SemanticsNode {
        /// Restores the flag however the build ends (a panic in it included).
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                WITH_DEACTIVATED.with(|with| with.set(self.0));
            }
        }
        let _restore = Restore(WITH_DEACTIVATED.with(|with| with.replace(true)));
        self.semantics_tree(merged)
    }

    /// `node`'s configuration in the unmerged tree (its own, with what its parts set), if it's
    /// a semantics node there.
    pub fn unmerged_semantics_config(&mut self, node: NodeId) -> Option<SemanticsConfiguration> {
        fn find(tree: &SemanticsNode, id: NodeId) -> Option<&SemanticsConfiguration> {
            if tree.id == id {
                return Some(&tree.config);
            }
            tree.children.iter().find_map(|child| find(child, id))
        }
        let tree = self.semantics_tree(false);
        find(&tree, node).cloned()
    }

    /// Whether `node` is in deactivated content (it or an ancestor deactivated), as Compose's
    /// `LayoutInfo.isDeactivated`.
    pub fn is_deactivated(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            let Some(value) = self.get(id) else { return false };
            if value.deactivated {
                return true;
            }
            current = value.parent;
        }
        false
    }
}

impl UiTree {
    /// The semantics a node's own modifiers set, and the layer of the first one (whose bounds
    /// are the node's semantic bounds).
    pub(crate) fn own_semantics(
        &mut self,
        node: NodeId,
    ) -> Option<(SemanticsConfiguration, usize)> {
        let value = self.get_mut(node)?;
        if !value.kinds.any(NodeKinds::SEMANTICS) {
            return None;
        }
        let mut config = SemanticsConfiguration::default();
        let mut layers: SmallVec<[usize; 8]> = SmallVec::new();
        let mut layer = 0;
        for bits in value.chain.iter().map(|entry| entry.kinds) {
            layers.push(layer);
            if bits.any(NodeKinds::LAYOUT) {
                layer += 1;
            }
        }
        let mut first_layer = None;
        let mut merging_layer = None;
        // From the innermost to the outermost, so an outer modifier's values win, as Compose's
        // `collapsedSemantics` (`tailToHead`).
        for index in (0..value.chain.len()).rev() {
            if value.chain[index].kinds.any(NodeKinds::SEMANTICS)
                && let Some(semantics) = value.chain[index].node.as_semantics()
            {
                let mut own = SemanticsConfiguration::default();
                semantics.apply_semantics(&mut own);
                // Clearing drops what the modifiers after it set, as Compose's
                // `calculateSemanticsConfiguration`.
                if own.is_clearing_semantics {
                    config = SemanticsConfiguration::default();
                }
                config.apply(&own);
                first_layer = Some(layers[index]);
                // A merging node's bounds are those of the outermost modifier that merges.
                if own.is_merging_semantics_of_descendants {
                    merging_layer = Some(layers[index]);
                }
            }
        }
        self.bind_request_focus(node, &mut config);
        Some((config, merging_layer.or(first_layer).unwrap_or(0)))
    }

    /// Binds a `RequestFocus` offered without an action (`request_focus_of_node`) to `node`:
    /// performing it asks the tree to focus the node's focus target, as a `FocusRequester`
    /// does, once the tree is free (D1251).
    fn bind_request_focus(&self, node: NodeId, config: &mut SemanticsConfiguration) {
        let unbound = config
            .get(&SemanticsActions::REQUEST_FOCUS)
            .is_some_and(|action| action.action.is_none());
        if unbound {
            let requests = std::rc::Rc::downgrade(&self.focus_requests);
            let label = config.get(&SemanticsActions::REQUEST_FOCUS).and_then(|a| a.label.clone());
            config.request_focus(label, move || match requests.upgrade() {
                Some(requests) => {
                    requests.push(crate::focus_requester::FocusRequest::Node(node));
                    true
                }
                None => false,
            });
        }
    }

    /// The semantics tree, as Compose's: merged (`merged`), a node merging its descendants
    /// takes their properties and hides them, unless they merge themselves; unmerged, every
    /// node with semantics appears. Nodes not placed are left out.
    pub fn semantics_tree(&mut self, merged: bool) -> SemanticsNode {
        let root = self.root_id();
        let mut children = Vec::new();
        let mut config = SemanticsConfiguration::default();
        let count = self.node(root).children.len();
        for index in 0..count {
            let child = self.node(root).children[index].node;
            self.semantics_of(child, merged, &mut config, &mut children);
        }
        // Overlays (popups, dialogs) are roots of their own, as Compose's windows: after the
        // content, never merged into what's around their anchor.
        for index in 0..self.overlays.len() {
            let overlay = self.overlays[index];
            if self.get(overlay).is_some_and(|value| value.overlay && value.placed) {
                let mut overlay_config = SemanticsConfiguration::default();
                self.semantics_of_overlay(overlay, merged, &mut overlay_config, &mut children);
            }
        }
        let rtl = self.node(root).layout_direction == crate::alignment::LayoutDirection::Rtl;
        SemanticsNode {
            id: root,
            bounds: self.bounds_in_root(root),
            rtl,
            config,
            own_config: None,
            children,
        }
    }

    /// Adds `node`'s semantics nodes to `out`, the children of the node whose properties are
    /// `parent`.
    fn semantics_of_overlay(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsNode>,
    ) {
        self.semantics_of_node(node, merged, parent, out);
    }

    fn semantics_of(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsNode>,
    ) {
        // Overlays are visited as roots of their own.
        if self.node(node).overlay {
            return;
        }
        self.semantics_of_node(node, merged, parent, out);
    }

    fn semantics_of_node(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsNode>,
    ) {
        if self.left_out(node) {
            return;
        }
        let own = self.own_semantics(node).filter(|(config, _)| !config.is_empty());
        let children_of = |tree: &mut UiTree| {
            (0..tree.node(node).children.len())
                .map(|index| tree.node(node).children[index].node)
                .collect::<SmallVec<[NodeId; 8]>>()
        };
        match own {
            None => {
                for child in children_of(self) {
                    self.semantics_of(child, merged, parent, out);
                }
            }
            Some((mut own, _)) if own.contains(&PART_OF_PARENT) => {
                // Its properties are the parent's, as if its own modifiers set them.
                own.remove(&PART_OF_PARENT);
                parent.apply(&own);
                for child in children_of(self) {
                    self.semantics_of(child, merged, parent, out);
                }
            }
            Some((mut config, layer)) => {
                let mut children = Vec::new();
                let mut own_config = None;
                if config.is_clearing_semantics {
                    // Nothing inside reads.
                } else if merged && config.is_merging_semantics_of_descendants {
                    let mut own = config.clone();
                    for child in children_of(self) {
                        self.merge_into(child, &mut config, Some(&mut own), &mut children);
                    }
                    if own != config {
                        own_config = Some(Box::new(own));
                    }
                } else {
                    for child in children_of(self) {
                        self.semantics_of(child, merged, &mut config, &mut children);
                    }
                }
                let origin = self.layer_origin(node, layer);
                let size = self.node(node).layers[layer].measured;
                let rtl =
                    self.node(node).layout_direction == crate::alignment::LayoutDirection::Rtl;
                out.push(SemanticsNode {
                    id: node,
                    bounds: Rect::from_offset_size(origin, size.into()),
                    rtl,
                    config,
                    own_config,
                    children,
                });
            }
        }
    }

    /// Merges `node`'s subtree into `config`, except descendants that merge themselves, which
    /// become `children`. `own` is the merging node's own configuration while no semantics node
    /// is between it and `node`: a part of it (`PART_OF_PARENT`, a text field's inner field) sets
    /// its properties there too, as if its own modifiers did.
    fn merge_into(
        &mut self,
        node: NodeId,
        config: &mut SemanticsConfiguration,
        mut own: Option<&mut SemanticsConfiguration>,
        children: &mut Vec<SemanticsNode>,
    ) {
        if self.left_out(node) || self.node(node).overlay {
            return;
        }
        if let Some((mut semantics, _)) = self.own_semantics(node) {
            if semantics.is_merging_semantics_of_descendants {
                self.semantics_of(node, true, config, children);
                return;
            }
            match own.as_deref_mut() {
                Some(own) if semantics.contains(&PART_OF_PARENT) => {
                    semantics.remove(&PART_OF_PARENT);
                    own.apply(&semantics);
                    config.apply(&semantics);
                }
                _ => {
                    config.merge_child(&semantics);
                    own = None;
                }
            }
            if semantics.is_clearing_semantics {
                return;
            }
        }
        let count = self.node(node).children.len();
        for index in 0..count {
            let child = self.node(node).children[index].node;
            self.merge_into(child, config, own.as_deref_mut(), children);
        }
    }
}
