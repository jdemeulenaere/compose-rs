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

impl compose_runtime::ComposeArg for ToggleableState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl From<bool> for ToggleableState {
    fn from(value: bool) -> ToggleableState {
        if value { ToggleableState::On } else { ToggleableState::Off }
    }
}

/// A copy of a node of the semantics tree and what's below it: a layout node with semantics, its
/// bounds in the root (in pixels), and its configuration (merged, in the merged tree). For what
/// walks a tree at leisure, a bridge's frame, a test's matchers; `SemanticsNode` is the live handle.
#[derive(Clone, Debug)]
pub struct SemanticsSnapshot {
    pub id: NodeId,
    /// As Compose's `SemanticsNode.id`: its layout node's semantics id, another when the node is
    /// reused, what the accessibility bridges name the node by (D1262).
    pub semantics_id: i32,
    pub bounds: Rect,
    /// Laid out right to left, as Compose's `layoutInfo.layoutDirection`: a horizontal
    /// scroll's left and right go the other way (D484).
    pub rtl: bool,
    pub config: SemanticsConfiguration,
    /// A merging node's own configuration, in the merged tree (`None`: the same as `config`):
    /// what Compose's accessibility reads actions and scrolls from (`unmergedConfig`, D491).
    pub own_config: Option<Box<SemanticsConfiguration>>,
    pub children: Vec<SemanticsSnapshot>,
    /// Its `z_index`, among what the traversal order compares (`SemanticsSort`).
    pub z_index: f32,
    /// When it was placed, among what the traversal order compares.
    pub place_order: u64,
    /// Accessibility leaves it out, as `isHidden`: under a layer with no alpha, or hiding.
    pub hidden: bool,
}

impl SemanticsSnapshot {
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
    pub fn flatten(&self) -> Vec<&SemanticsSnapshot> {
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
use crate::tree::{Observation, UiTree, observation_key};

thread_local! {
    /// Semantics trees check the kept configurations first (`set_semantics_cache_check`).
    static CHECK_CACHE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread's semantics trees first check every kept configuration against its
/// modifiers' now, panicking on a stale one: compose-test's rules turn it on, so every test
/// checks the invalidations (D1257). It computes every configuration again: for tests.
pub fn set_semantics_cache_check(on: bool) {
    CHECK_CACHE.with(|check| check.set(on));
}
use std::rc::Rc;

/// A node's own semantics, kept per layout node (D1257): its modifiers' configuration and the
/// layer whose bounds are its semantic bounds.
#[derive(Debug)]
pub(crate) struct OwnSemantics {
    pub(crate) config: SemanticsConfiguration,
    pub(crate) layer: usize,
    /// The chain entry giving those bounds.
    pub(crate) bounds_entry: usize,
    /// Whether a modifier here is important for bounds.
    pub(crate) has_bounds: bool,
}

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
    pub fn semantics_tree_with_deactivated(&mut self, merged: bool) -> SemanticsSnapshot {
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
    /// The semantics a node's own modifiers set and the layer of the one whose bounds are the
    /// node's semantic bounds, as Compose's `LayoutNode.semanticsConfiguration`: computed when
    /// asked and kept until invalidated (D1257), its state reads observed for a custom node's.
    pub(crate) fn own_semantics(&mut self, node: NodeId) -> Option<Rc<OwnSemantics>> {
        let value = self.get(node)?;
        if !value.kinds.any(NodeKinds::SEMANTICS) {
            return None;
        }
        if let Some(cached) = &value.semantics {
            return Some(cached.clone());
        }
        let observed = value.kinds.any(NodeKinds::OBSERVED);
        let computed = match self.observer.clone().filter(|_| observed) {
            Some(observer) => observer
                .observe(observation_key(node, Observation::Semantics), || {
                    self.compute_own_semantics(node)
                }),
            None => self.compute_own_semantics(node),
        };
        let computed = Rc::new(computed);
        self.node_mut(node).semantics = Some(computed.clone());
        Some(computed)
    }

    /// `own_semantics` computed, as Compose's `calculateSemanticsConfiguration`. Invalidations of
    /// its semantics asked while they're computed don't count, as Compose's
    /// (`isCurrentlyCalculatingSemanticsConfiguration`), whoever computes them (D1261).
    fn compute_own_semantics(&mut self, node: NodeId) -> OwnSemantics {
        let queued = self.focus_requests.node_invalidations.borrow().len();
        let computed = self.apply_own_semantics(node);
        let mut pending = self.focus_requests.node_invalidations.borrow_mut();
        let mut index = 0;
        pending.retain(|&(asking, _, what)| {
            index += 1;
            index <= queued
                || asking != node
                || what != crate::node_context::Invalidation::Semantics
        });
        drop(pending);
        computed
    }

    /// Each semantics modifier's `apply_semantics`, tail to head.
    fn apply_own_semantics(&mut self, node: NodeId) -> OwnSemantics {
        let value = self.node_mut(node);
        let mut config = SemanticsConfiguration::default();
        let mut layers: SmallVec<[usize; 8]> = SmallVec::new();
        let mut layer = 0;
        for bits in value.chain.iter().map(|entry| entry.kinds) {
            layers.push(layer);
            if bits.any(NodeKinds::LAYOUT) {
                layer += 1;
            }
        }
        let mut first_bounds = None;
        let mut merging_bounds = None;
        // From the innermost to the outermost, so an outer modifier's values win, as Compose's
        // `collapsedSemantics` (`tailToHead`).
        for index in (0..value.chain.len()).rev() {
            let entry = &mut value.chain[index];
            if !entry.kinds.any(NodeKinds::SEMANTICS) {
                continue;
            }
            // The node, or each of its delegates that adds semantics.
            crate::delegation::dispatch_for_kind(
                &mut *entry.node,
                entry.own,
                NodeKinds::SEMANTICS,
                crate::node_chain::EntryId::NO_DELEGATE,
                &mut |modifier_node, _| {
                    let Some(semantics) = modifier_node.as_semantics() else { return };
                    let mut own = SemanticsConfiguration::default();
                    own.is_merging_semantics_of_descendants =
                        semantics.should_merge_descendant_semantics();
                    own.is_clearing_semantics = semantics.should_clear_descendant_semantics();
                    semantics.apply_semantics(&mut own);
                    // Clearing drops what the modifiers after it set, as Compose's
                    // `calculateSemanticsConfiguration`.
                    if own.is_clearing_semantics {
                        config = SemanticsConfiguration::default();
                    }
                    config.apply(&own);
                    // The bounds: the outermost merging modifier's that is important for them,
                    // else the outermost important one's (Compose's `findCoordinatorToGetBounds`).
                    if semantics.is_important_for_bounds() {
                        first_bounds = Some((layers[index], index));
                        if own.is_merging_semantics_of_descendants {
                            merging_bounds = Some((layers[index], index));
                        }
                    }
                },
            );
        }
        self.bind_request_focus(node, &mut config);
        let (layer, bounds_entry) = merging_bounds.or(first_bounds).unwrap_or((0, 0));
        OwnSemantics { config, layer, bounds_entry, has_bounds: first_bounds.is_some() }
    }

    /// Checks every kept configuration of content in use against its modifiers' now: a stale
    /// one (an invalidation missed) panics, naming the node (`set_semantics_cache_check`).
    fn check_semantics_cache(&mut self) {
        let mut nodes = Vec::new();
        let mut stack = vec![self.root_id()];
        while let Some(node) = stack.pop() {
            nodes.push(node);
            stack.extend(self.node(node).children.iter().map(|child| child.node));
        }
        for node in nodes {
            let Some(cached) = self.get(node).and_then(|value| {
                (value.attached && !value.deactivated).then(|| value.semantics.clone()).flatten()
            }) else {
                continue;
            };
            let fresh = self.compute_own_semantics(node);
            assert!(
                fresh.layer == cached.layer && fresh.config.same_ignoring_functions(&cached.config),
                "compose-ui: the semantics kept for {node:?} are stale (an invalidation missed, \
                 D1257):\nkept:  {:?}\nnow:   {:?}",
                cached.config,
                fresh.config
            );
        }
    }

    /// Forgets `node`'s semantics, to be computed again when asked, and says they may have
    /// changed (the frame's `semantics_changed`), as `invalidateSemantics`.
    pub(crate) fn invalidate_semantics_of(&mut self, node: NodeId) {
        if let Some(value) = self.get_mut(node) {
            value.semantics = None;
            if value.attached && !value.deactivated {
                self.semantics_changed = true;
            }
        }
    }

    /// Forgets `node`'s semantics without telling the frame (the change that made them stale
    /// tells it, if it should).
    pub(crate) fn forget_semantics(&mut self, node: NodeId) {
        if let Some(value) = self.get_mut(node) {
            value.semantics = None;
        }
    }

    /// `node`'s configuration in the unmerged tree, as performing its actions reads it: its own
    /// with what its parts set (`PART_OF_PARENT`, a text field's inner field), found below it
    /// without building the tree. `None` if the unmerged tree leaves it out.
    pub fn unmerged_semantics_config(&mut self, node: NodeId) -> Option<SemanticsConfiguration> {
        // Gone: no answer, not a panic (a bridge's call comes across JNI).
        self.get(node)?;
        if self.left_out(node) || self.get(node).is_some_and(|value| value.deactivated) {
            return None;
        }
        let own = self.own_semantics(node)?;
        if own.config.is_empty() || own.config.contains(&PART_OF_PARENT) {
            return None;
        }
        // Out of the tree under a parent not placed. An overlay is a root of its own.
        let mut at = node;
        while !self.node(at).overlay
            && let Some(parent) = self.node(at).parent
        {
            if self.left_out(parent) {
                return None;
            }
            at = parent;
        }
        self.own_with_parts(node)
    }

    /// `node`'s own configuration with what its parts set (`PART_OF_PARENT`), placed or not.
    pub(crate) fn own_with_parts(&mut self, node: NodeId) -> Option<SemanticsConfiguration> {
        let own = self.own_semantics(node)?;
        let mut config = own.config.clone();
        if !config.is_clearing_semantics {
            let mut stack: SmallVec<[NodeId; 8]> =
                self.node(node).children.iter().rev().map(|child| child.node).collect();
            while let Some(child) = stack.pop() {
                if self.left_out(child) || self.node(child).overlay {
                    continue;
                }
                match self.own_semantics(child) {
                    Some(part) if part.config.contains(&PART_OF_PARENT) => {
                        let mut part = part.config.clone();
                        part.remove(&PART_OF_PARENT);
                        config.apply(&part);
                    }
                    // Another semantics node: its own and below are its.
                    Some(other) if !other.config.is_empty() => continue,
                    _ => {}
                }
                let children = &self.node(child).children;
                stack.extend(children.iter().rev().map(|child| child.node));
            }
        }
        Some(config)
    }

    /// `node` with what it merges, as `mergeConfig`.
    pub(crate) fn merged_config_of(&mut self, node: NodeId) -> Option<SemanticsConfiguration> {
        let mut own = self.own_with_parts(node)?;
        let mut config = own.clone();
        if !config.is_clearing_semantics {
            let mut merging = Vec::new();
            for child in self.z_sorted_children(node) {
                self.merge_into(child, &mut config, Some(&mut own), &mut merging);
            }
        }
        Some(config)
    }

    /// `node`'s children in drawing order, as `zSortedChildren`: by `z_index`, then placed, those
    /// not placed last.
    pub(crate) fn z_sorted_children(&self, node: NodeId) -> SmallVec<[NodeId; 8]> {
        let children = &self.node(node).children;
        let mut out: SmallVec<[NodeId; 8]> = SmallVec::with_capacity(children.len());
        match self.placement_order(node) {
            Some(order) => out.extend(order.iter().map(|&index| children[index as usize].node)),
            None => {
                out.extend(
                    children.iter().map(|child| child.node).filter(|&id| self.node(id).placed),
                );
            }
        }
        out.extend(children.iter().map(|child| child.node).filter(|&id| !self.node(id).placed));
        out
    }

    /// Binds a `RequestFocus` offered without an action (`request_focus_of_node`) to `node`:
    /// performing it asks the tree to focus the node's focus target, as a `FocusRequester`
    /// does: at once where the tree is free (D1251, D1268).
    fn bind_request_focus(&self, node: NodeId, config: &mut SemanticsConfiguration) {
        let unbound = config
            .get(&SemanticsActions::REQUEST_FOCUS)
            .is_some_and(|action| action.action.is_none());
        if unbound {
            let requests = std::rc::Rc::downgrade(&self.focus_requests);
            let label = config.get(&SemanticsActions::REQUEST_FOCUS).and_then(|a| a.label.clone());
            config.request_focus(label, move || match requests.upgrade() {
                Some(requests) => crate::focus_requester::perform_or_queue(
                    &requests,
                    crate::focus_requester::FocusRequest::Node(node),
                ),
                None => false,
            });
        }
    }

    /// The semantics tree, as Compose's: merged (`merged`), a node merging its descendants
    /// takes their properties and hides them, unless they merge themselves; unmerged, every
    /// node with semantics appears. Nodes not placed are left out.
    pub fn semantics_tree(&mut self, merged: bool) -> SemanticsSnapshot {
        if CHECK_CACHE.with(std::cell::Cell::get) {
            self.check_semantics_cache();
        }
        let root = self.root_id();
        let mut children = Vec::new();
        let mut config = SemanticsConfiguration::default();
        for child in self.z_sorted_children(root) {
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
        SemanticsSnapshot {
            id: root,
            semantics_id: self.node(root).semantics_id,
            bounds: self.bounds_in_root(root),
            rtl,
            config,
            own_config: None,
            children,
            z_index: 0.0,
            place_order: 0,
            hidden: false,
        }
    }

    /// The merged tree an accessibility service reads: the top window that takes the keys and what's
    /// over it (D1290). What's behind a dialog is another window's, which a service doesn't read
    /// while the dialog's is active, as Compose's `Dialog` in a window of its own.
    pub fn accessibility_tree(&mut self) -> SemanticsSnapshot {
        let Some(top) = self.focus.window else { return self.semantics_tree(true) };
        let mut root = self.semantics_tree(true);
        let first = self.overlays.iter().position(|&overlay| overlay == top);
        let shown: SmallVec<[NodeId; 4]> = match first {
            Some(first) => self.overlays[first..].iter().copied().collect(),
            None => SmallVec::new(),
        };
        root.children
            .retain(|child| shown.iter().any(|&overlay| self.is_at_or_below(child.id, overlay)));
        root
    }

    /// Adds `node`'s semantics nodes to `out`, the children of the node whose properties are
    /// `parent`.
    fn semantics_of_overlay(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsSnapshot>,
    ) {
        self.semantics_of_node(node, merged, parent, out);
    }

    fn semantics_of(
        &mut self,
        node: NodeId,
        merged: bool,
        parent: &mut SemanticsConfiguration,
        out: &mut Vec<SemanticsSnapshot>,
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
        out: &mut Vec<SemanticsSnapshot>,
    ) {
        if self.left_out(node) {
            return;
        }
        let own = self.own_semantics(node).filter(|own| !own.config.is_empty());
        let children_of = |tree: &mut UiTree| tree.z_sorted_children(node);
        match own {
            None => {
                for child in children_of(self) {
                    self.semantics_of(child, merged, parent, out);
                }
            }
            Some(own) if own.config.contains(&PART_OF_PARENT) => {
                // Its properties are the parent's, as if its own modifiers set them.
                let mut own = own.config.clone();
                own.remove(&PART_OF_PARENT);
                parent.apply(&own);
                for child in children_of(self) {
                    self.semantics_of(child, merged, parent, out);
                }
            }
            Some(own) => {
                let (mut config, layer) = (own.config.clone(), own.layer);
                let mut children = Vec::new();
                let mut own_config = None;
                if config.is_clearing_semantics && merged {
                    // Nothing inside reads; the unmerged tree shows it, for tests and debugging,
                    // as Compose's `includeReplacedSemantics`.
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
                let hidden = config.contains(&SemanticsProperties::HIDE_FROM_ACCESSIBILITY)
                    || self.semantics_is_transparent(node);
                let value = self.node(node);
                out.push(SemanticsSnapshot {
                    id: node,
                    semantics_id: value.semantics_id,
                    bounds: Rect::from_offset_size(origin, size.into()),
                    rtl,
                    config,
                    own_config,
                    children,
                    z_index: value.z_index,
                    place_order: value.place_order,
                    hidden,
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
        children: &mut Vec<SemanticsSnapshot>,
    ) {
        // Only deactivated content is skipped, as `fillOneLayerOfSemanticsWrappers`
        // (`isAttached && !isDeactivated`): what's merely unplaced still merges (a navigation
        // item's hidden label names it). A merging node apart from it is a node of its own, and
        // unplaced nodes have none (`semantics_of_node`).
        let value = self.node(node);
        if value.overlay || (value.deactivated && !WITH_DEACTIVATED.with(std::cell::Cell::get)) {
            return;
        }
        if let Some(own_semantics) = self.own_semantics(node) {
            let mut semantics = own_semantics.config.clone();
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
        for child in self.z_sorted_children(node) {
            self.merge_into(child, config, own.as_deref_mut(), children);
        }
    }
}
