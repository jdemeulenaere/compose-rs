//! The semantics tree as handles, as Compose's `SemanticsOwner`, `SemanticsNode` and `LayoutInfo`:
//! each answer computed from the live tree when asked, not a copy of it (D1287). What a bridge or a
//! test wants to walk at leisure is `UiHost::semantics_tree`'s `SemanticsSnapshot`.
//!
//! A handle answers while its tree is free; in a pass, or with its node gone or reused for other
//! content, it answers as a node with nothing: an empty configuration, no children, zero bounds.

use smallvec::SmallVec;

use crate::focus_requester::read_free_tree;
use crate::semantics::{
    OwnSemantics, Role, SemanticsActions, SemanticsConfiguration, SemanticsProperties,
    SemanticsPropertyKey, SemanticsValue, ValueSlot,
};
use crate::semantics_properties::AnySemanticsPropertyKey;
use crate::tree::{NodeId, UiTree};
use crate::unit::{IntSize, Offset, Rect};

fn contains(rect: Rect, point: Offset) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

/// Added to a node's id for its fake node with its role, as `RoleFakeNodeIdOffset`.
pub const ROLE_FAKE_NODE_ID_OFFSET: i32 = 1_000_000_000;
/// Added to a node's id for its fake node with its description, as
/// `ContentDescriptionFakeNodeIdOffset`.
pub const CONTENT_DESCRIPTION_FAKE_NODE_ID_OFFSET: i32 = 2_000_000_000;

/// The semantics tree of a host, as Compose's `SemanticsOwner`: `UiHost::semantics_owner`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SemanticsOwner {
    tree: u32,
}

/// A layout node, as Compose's `LayoutInfo` and `SemanticsInfo`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayoutInfo {
    tree: u32,
    node: NodeId,
    semantics_id: i32,
}

/// What a fake node stands for: a merging node's own role or description, kept apart for
/// accessibility while its children take the rest (`emitFakeNodes`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FakeKind {
    Role,
    ContentDescription,
}

/// A node of the semantics tree, as Compose's `SemanticsNode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SemanticsNode {
    tree: u32,
    node: NodeId,
    layout_id: i32,
    merging_enabled: bool,
    fake: Option<FakeKind>,
}

impl SemanticsOwner {
    pub(crate) fn new(tree: u32) -> SemanticsOwner {
        SemanticsOwner { tree }
    }

    fn read<R>(&self, read: impl FnOnce(&mut UiTree) -> R) -> Option<R> {
        read_free_tree(self.tree, read)
    }

    fn root(&self, merging_enabled: bool) -> SemanticsNode {
        let (node, layout_id) = self
            .read(|tree| (tree.root_id(), tree.node(tree.root_id()).semantics_id))
            .unwrap_or((NodeId::from_bits(u64::MAX), -1));
        SemanticsNode { tree: self.tree, node, layout_id, merging_enabled, fake: None }
    }

    /// The root of the merged tree, as `rootSemanticsNode`.
    pub fn root_semantics_node(&self) -> SemanticsNode {
        self.root(true)
    }

    /// The root of the unmerged tree, as `unmergedRootSemanticsNode`.
    pub fn unmerged_root_semantics_node(&self) -> SemanticsNode {
        self.root(false)
    }

    /// The root's layout, as `rootInfo`.
    pub fn root_info(&self) -> LayoutInfo {
        self.root(false).layout_info()
    }

    /// The unmerged node with this id, if a layout node has it, attached and with semantics.
    pub fn get(&self, semantics_id: i32) -> Option<SemanticsNode> {
        self.node(semantics_id, false)
    }

    /// `get`, in the merged tree with `merging_enabled`: a test finder's, which looks in either.
    /// The root has no semantics of its own and is still a node.
    pub fn node(&self, semantics_id: i32, merging_enabled: bool) -> Option<SemanticsNode> {
        let tree = self.tree;
        self.read(|ui| {
            let node = ui.node_with_semantics_id(semantics_id)?;
            let value = ui.get(node)?;
            let has_semantics = value.kinds.any(crate::node_chain::NodeKinds::SEMANTICS);
            (value.attached && (has_semantics || node == ui.root_id())).then_some(SemanticsNode {
                tree,
                node,
                layout_id: semantics_id,
                merging_enabled,
                fake: None,
            })
        })
        .flatten()
    }

    /// The layout node with this id, as `getLayoutInfo`.
    pub fn get_layout_info(&self, semantics_id: i32) -> Option<LayoutInfo> {
        let tree = self.tree;
        self.read(|ui| {
            let node = ui.node_with_semantics_id(semantics_id)?;
            Some(LayoutInfo { tree, node, semantics_id })
        })
        .flatten()
    }

    /// Every node of a tree, as `getAllSemanticsNodes`, parents first.
    pub fn get_all_semantics_nodes(
        &self,
        merging_enabled: bool,
        skip_deactivated_nodes: bool,
    ) -> Vec<SemanticsNode> {
        fn add(node: SemanticsNode, skip_deactivated_nodes: bool, out: &mut Vec<SemanticsNode>) {
            out.push(node);
            for child in node.children_with(!skip_deactivated_nodes) {
                add(child, skip_deactivated_nodes, out);
            }
        }
        let mut out = Vec::new();
        add(self.root(merging_enabled), skip_deactivated_nodes, &mut out);
        out
    }

    /// The layouts with semantics at `position` in the root, the top-most first, until `on_hit`
    /// says yes: that one, as `hitTest`. Hidden ones are in it, link markers aren't. Siblings under
    /// a hit aren't looked at; a node clearing its semantics hides what's below it.
    pub fn hit_test(
        &self,
        position: Offset,
        mut on_hit: impl FnMut(&LayoutInfo) -> bool,
    ) -> Option<LayoutInfo> {
        let tree = self.tree;
        let hits = self.read(|ui| {
            let mut hits = Vec::new();
            ui.semantics_hits(ui.root_id(), position, &mut hits);
            hits.into_iter()
                .map(|node| LayoutInfo { tree, node, semantics_id: ui.node(node).semantics_id })
                .collect::<Vec<_>>()
        })?;
        hits.into_iter().find(|info| on_hit(info))
    }

    /// Visits the nodes of the unmerged tree no sibling over them completely covers, with the
    /// bounding box of what's left of their touch bounds, and whether a merging node hiding from
    /// accessibility holds them, as `forEachUncoveredSemanticsNode`. The root always; synthetic
    /// nodes with their owner's bounds.
    pub fn for_each_uncovered_semantics_node(
        &self,
        mut visitor: impl FnMut(SemanticsNode, Rect, bool),
    ) {
        let root = self.unmerged_root_semantics_node();
        if !root.layout_info().is_attached() {
            return;
        }
        // The root always, and its windows each on their own: one over another covers nothing, as
        // in Compose, where each is a host of its own.
        let bounds = root.bounds_in_root();
        visitor(root, SemanticsRegion::of(bounds).bounds_or(rounded(bounds)), false);
        let mut windows: Vec<(Option<NodeId>, SemanticsRegion)> = Vec::new();
        let mut children = Vec::new();
        root.for_each_accessibility_child(|child| children.push(child));
        for child in children.into_iter().rev() {
            let window = child.read(|tree, node| tree.overlay_around(node)).flatten();
            let at = match windows.iter().position(|(it, _)| *it == window) {
                Some(at) => at,
                None => {
                    windows.push((window, SemanticsRegion::of(bounds)));
                    windows.len() - 1
                }
            };
            visit_uncovered(child, &mut windows[at].1, false, &mut visitor);
        }
    }
}

fn is_placed(node: SemanticsNode) -> bool {
    let info = node.layout_info();
    info.is_placed() && info.is_attached()
}

/// Whether the node hides what's in it, as `hideFromAccessibility` on a merging node.
fn hides_what_it_merges(config: &SemanticsConfiguration) -> bool {
    config.is_merging_semantics_of_descendants
        && config.contains(&SemanticsProperties::HIDE_FROM_ACCESSIBILITY)
}

/// As `isPartiallyOffscreenInScrollParent`: some of it is past the nearest scrolling node's ends.
fn is_partly_past_its_scroller(node: SemanticsNode) -> bool {
    let mut parent = node.parent();
    while let Some(scroller) = parent {
        let scrolls = scroller.read(|tree, id| {
            tree.own_semantics(id).is_some_and(|own| {
                own.config.contains(&SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
                    || own.config.contains(&SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE)
            })
        });
        if scrolls == Some(true) {
            let (own, around) =
                (node.unclipped_bounds_in_root(), scroller.unclipped_bounds_in_root());
            return own.left < around.left
                || own.top < around.top
                || own.right > around.right
                || own.bottom > around.bottom;
        }
        parent = scroller.parent();
    }
    false
}

fn fake_node_bounds(node: SemanticsNode) -> Rect {
    match node.parent() {
        Some(owner) if owner.layout_info().is_placed() => rounded(owner.bounds_in_root()),
        _ => Rect::new(0.0, 0.0, 10.0, 10.0),
    }
}

fn visit_uncovered(
    node: SemanticsNode,
    unaccounted: &mut SemanticsRegion,
    hidden: bool,
    visitor: &mut impl FnMut(SemanticsNode, Rect, bool),
) {
    if unaccounted.is_empty() || (node.fake.is_none() && !is_placed(node)) {
        return;
    }
    if node.fake.is_some() {
        visitor(node, fake_node_bounds(node), hidden);
        return;
    }
    let (touch, clipped) =
        node.read(|tree, id| tree.semantics_touch_bounds_both(id)).unwrap_or((EMPTY, EMPTY));
    let (touch, clipped) = (rounded(touch), rounded(clipped));
    let config = node.unmerged_config();
    let hidden = hidden || hides_what_it_merges(&config);
    // What's uncovered is decided inside what clips it, and a node left in is reported with all its
    // touch target: an overhang past a clip neither keeps it alone nor covers what's beside.
    let mut region = SemanticsRegion::of(clipped);
    if !region.intersect(unaccounted) {
        return;
    }
    if clipped != touch {
        region = SemanticsRegion::of(touch);
        region.intersect(unaccounted);
    }
    visitor(node, region.bounds(), hidden);
    let mut children = Vec::new();
    node.for_each_accessibility_child_of(&config, |child| children.push(child));
    if config.is_merging_semantics_of_descendants && is_partly_past_its_scroller(node) {
        // What's past the scroller is reported too, unclipped, for a screen reader to scroll to.
        let mut inside = SemanticsRegion::of(node.unclipped_bounds_in_root());
        for child in children.into_iter().rev() {
            visit_uncovered_past_scroller(child, &mut inside, hidden, visitor);
        }
    } else {
        for child in children.into_iter().rev() {
            visit_uncovered(child, unaccounted, hidden, visitor);
        }
    }
    if node.is_important_given(&config) {
        unaccounted.difference(clipped);
    }
}

/// `visit_uncovered` inside a merging node partly past its scroller: what's all past it counts by
/// its unclipped bounds, and what's accounted for is within that node.
fn visit_uncovered_past_scroller(
    node: SemanticsNode,
    unaccounted: &mut SemanticsRegion,
    hidden: bool,
    visitor: &mut impl FnMut(SemanticsNode, Rect, bool),
) {
    if node.fake.is_some() {
        if is_placed(node.parent().unwrap_or(node)) || !unaccounted.is_empty() {
            visitor(node, fake_node_bounds(node), hidden);
        }
        return;
    }
    if !is_placed(node) || unaccounted.is_empty() {
        return;
    }
    let touch = node.touch_bounds_in_root();
    let empty = touch.right <= touch.left || touch.bottom <= touch.top;
    let bounds = rounded(if empty { node.unclipped_bounds_in_root() } else { touch });
    let config = node.unmerged_config();
    let hidden = hidden || hides_what_it_merges(&config);
    let mut region = SemanticsRegion::of(bounds);
    if !region.intersect(unaccounted) {
        return;
    }
    visitor(node, region.bounds(), hidden);
    let mut children = Vec::new();
    node.for_each_accessibility_child_of(&config, |child| children.push(child));
    for child in children.into_iter().rev() {
        visit_uncovered_past_scroller(child, unaccounted, hidden, visitor);
    }
    if node.is_important_given(&config) {
        unaccounted.difference(bounds);
    }
}

fn rounded(rect: Rect) -> Rect {
    Rect::new(rect.left.round(), rect.top.round(), rect.right.round(), rect.bottom.round())
}

/// A region of the root as rectangles that don't overlap, as `SemanticsRegion`.
struct SemanticsRegion {
    rects: SmallVec<[Rect; 4]>,
}

impl SemanticsRegion {
    fn of(rect: Rect) -> SemanticsRegion {
        let rect = rounded(rect);
        let mut rects = SmallVec::new();
        if rect.right > rect.left && rect.bottom > rect.top {
            rects.push(rect);
        }
        SemanticsRegion { rects }
    }

    fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// Its bounds, or `otherwise` if nothing's in it (a root of no size is still a root).
    fn bounds_or(&self, otherwise: Rect) -> Rect {
        if self.is_empty() { otherwise } else { self.bounds() }
    }

    /// Keeps what's in `other`: whether anything is left.
    fn intersect(&mut self, other: &SemanticsRegion) -> bool {
        let mut left = SmallVec::new();
        for mine in &self.rects {
            for theirs in &other.rects {
                let rect = Rect::new(
                    mine.left.max(theirs.left),
                    mine.top.max(theirs.top),
                    mine.right.min(theirs.right),
                    mine.bottom.min(theirs.bottom),
                );
                if rect.right > rect.left && rect.bottom > rect.top {
                    left.push(rect);
                }
            }
        }
        self.rects = left;
        !self.is_empty()
    }

    /// Takes `cut` out.
    fn difference(&mut self, cut: Rect) {
        let mut left = SmallVec::new();
        for rect in &self.rects {
            if cut.right <= rect.left
                || cut.left >= rect.right
                || cut.bottom <= rect.top
                || cut.top >= rect.bottom
            {
                left.push(*rect);
                continue;
            }
            let (top, bottom) = (rect.top.max(cut.top), rect.bottom.min(cut.bottom));
            if rect.top < cut.top {
                left.push(Rect::new(rect.left, rect.top, rect.right, cut.top));
            }
            if cut.bottom < rect.bottom {
                left.push(Rect::new(rect.left, cut.bottom, rect.right, rect.bottom));
            }
            if rect.left < cut.left {
                left.push(Rect::new(rect.left, top, cut.left, bottom));
            }
            if cut.right < rect.right {
                left.push(Rect::new(cut.right, top, rect.right, bottom));
            }
        }
        self.rects = left;
    }

    fn bounds(&self) -> Rect {
        let mut rects = self.rects.iter();
        let Some(first) = rects.next() else { return EMPTY };
        rects.fold(*first, |bounds, rect| {
            Rect::new(
                bounds.left.min(rect.left),
                bounds.top.min(rect.top),
                bounds.right.max(rect.right),
                bounds.bottom.max(rect.bottom),
            )
        })
    }
}

impl LayoutInfo {
    /// `f` on the tree if it's free and the node is still this content.
    fn read<R>(&self, f: impl FnOnce(&mut UiTree, NodeId) -> R) -> Option<R> {
        let (node, id) = (self.node, self.semantics_id);
        read_free_tree(self.tree, |tree| {
            tree.get(node).filter(|value| value.semantics_id == id)?;
            Some(f(tree, node))
        })
        .flatten()
    }

    fn info_of(&self, tree: &UiTree, node: NodeId) -> LayoutInfo {
        LayoutInfo { tree: self.tree, node, semantics_id: tree.node(node).semantics_id }
    }

    pub fn semantics_id(&self) -> i32 {
        self.semantics_id
    }

    pub fn is_attached(&self) -> bool {
        self.read(|tree, node| tree.node(node).attached).unwrap_or(false)
    }

    pub fn is_placed(&self) -> bool {
        self.read(|tree, node| tree.node(node).placed).unwrap_or(false)
    }

    /// In content kept for reuse, as `isDeactivated`.
    pub fn is_deactivated(&self) -> bool {
        self.read(|tree, node| tree.is_deactivated(node)).unwrap_or(false)
    }

    /// Where its parent placed it among its children, as `placeOrder`;
    /// `NOT_PLACED_PLACE_ORDER` if it didn't.
    pub fn place_order(&self) -> i32 {
        self.read(|tree, node| {
            let value = tree.node(node);
            if !value.placed {
                return Self::NOT_PLACED_PLACE_ORDER;
            }
            let Some(parent) = value.parent else { return 0 };
            let before = tree.node(parent).children.iter().filter(|child| {
                let sibling = tree.node(child.node);
                sibling.placed && sibling.place_order < value.place_order
            });
            before.count() as i32
        })
        .unwrap_or(Self::NOT_PLACED_PLACE_ORDER)
    }

    /// As `LayoutInfo.NotPlacedPlaceOrder`.
    pub const NOT_PLACED_PLACE_ORDER: i32 = i32::MAX;

    pub fn z_index(&self) -> f32 {
        self.read(|tree, node| tree.node(node).z_index).unwrap_or(0.0)
    }

    /// Whether a graphics layer with no alpha is around it, as `isTransparent`.
    pub fn is_transparent(&self) -> bool {
        self.read(|tree, node| tree.alpha_around(node, Some(0)) <= 0.0).unwrap_or(false)
    }

    /// The layout node around it, as `parentInfo`.
    pub fn parent_info(&self) -> Option<LayoutInfo> {
        self.read(|tree, node| {
            let parent = tree.node(node).parent?;
            Some(self.info_of(tree, parent))
        })
        .flatten()
    }

    /// The layout nodes in it in drawing order, as `childrenInfo`.
    pub fn children_info(&self) -> Vec<LayoutInfo> {
        self.read(|tree, node| {
            let children = tree.z_sorted_children(node);
            children.into_iter().map(|child| self.info_of(tree, child)).collect()
        })
        .unwrap_or_default()
    }

    /// Its modifiers' semantics, unmerged; `None` with none, or in content kept for reuse, as
    /// `semanticsConfiguration`.
    pub fn semantics_configuration(&self) -> Option<SemanticsConfiguration> {
        self.read(|tree, node| {
            if tree.is_deactivated(node) {
                return None;
            }
            tree.own_semantics(node).map(|own| own.config.clone())
        })
        .flatten()
    }

    /// The closest layout around it with semantics, as `nearestParentThatHasSemantics`.
    pub fn nearest_parent_that_has_semantics(&self) -> Option<LayoutInfo> {
        self.read(|tree, node| {
            let mut current = tree.node(node).parent;
            while let Some(id) = current {
                if tree.own_semantics(id).is_some() {
                    return Some(self.info_of(tree, id));
                }
                current = tree.node(id).parent;
            }
            None
        })
        .flatten()
    }

    /// The closest layout around it merging its descendants, as `findMergingSemanticsParent`.
    pub fn find_merging_semantics_parent(&self) -> Option<LayoutInfo> {
        self.read(|tree, node| {
            let mut current = tree.node(node).parent;
            while let Some(id) = current {
                if tree
                    .own_semantics(id)
                    .is_some_and(|own| own.config.is_merging_semantics_of_descendants)
                {
                    return Some(self.info_of(tree, id));
                }
                current = tree.node(id).parent;
            }
            None
        })
        .flatten()
    }

    /// Its outermost bounds in the root, unclipped, as `calculateOuterBoundsInRoot`.
    pub fn calculate_outer_bounds_in_root(&self) -> Rect {
        self.read(|tree, node| tree.outer_bounds_in_root(node)).unwrap_or(EMPTY)
    }
}

const EMPTY: Rect = Rect { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };

impl SemanticsNode {
    fn read<R>(&self, f: impl FnOnce(&mut UiTree, NodeId) -> R) -> Option<R> {
        let (node, id) = (self.node, self.layout_id);
        read_free_tree(self.tree, |tree| {
            tree.get(node).filter(|value| value.semantics_id == id)?;
            Some(f(tree, node))
        })
        .flatten()
    }

    fn handle_of(&self, node: NodeId, tree: &UiTree, merging_enabled: bool) -> SemanticsNode {
        SemanticsNode {
            tree: self.tree,
            node,
            layout_id: tree.node(node).semantics_id,
            merging_enabled,
            fake: None,
        }
    }

    /// As `SemanticsNode.id`: its layout node's semantics id, a fake node's offset from it.
    pub fn id(&self) -> i32 {
        self.layout_id
            + match self.fake {
                None => 0,
                Some(FakeKind::Role) => ROLE_FAKE_NODE_ID_OFFSET,
                Some(FakeKind::ContentDescription) => CONTENT_DESCRIPTION_FAKE_NODE_ID_OFFSET,
            }
    }

    pub fn merging_enabled(&self) -> bool {
        self.merging_enabled
    }

    /// Whether it stands for a part of another node, as `isFake`.
    pub fn is_fake(&self) -> bool {
        self.fake.is_some()
    }

    /// Whether it's the root of its tree, as `isRoot`.
    pub fn is_root(&self) -> bool {
        self.fake.is_none() && self.read(|tree, node| node == tree.root_id()).unwrap_or(false)
    }

    pub fn layout_info(&self) -> LayoutInfo {
        LayoutInfo { tree: self.tree, node: self.node, semantics_id: self.layout_id }
    }

    /// What's declared on it, as `unmergedConfig`.
    pub fn unmerged_config(&self) -> SemanticsConfiguration {
        let own = self.read(|tree, node| tree.own_with_parts(node)).flatten().unwrap_or_default();
        match self.fake {
            None => own,
            Some(FakeKind::Role) => {
                let mut config = SemanticsConfiguration::default();
                if let Some(role) = own.get(&SemanticsProperties::ROLE) {
                    config.set_role(*role);
                }
                config
            }
            Some(FakeKind::ContentDescription) => {
                let mut config = SemanticsConfiguration::default();
                let description = own.get(&SemanticsProperties::CONTENT_DESCRIPTION);
                if let Some(first) = description.and_then(|all| all.first()) {
                    config.set_content_description(first.clone());
                }
                config
            }
        }
    }

    /// It and what it merges, as `mergedConfig`: its own unless it merges, in either tree.
    pub fn merged_config(&self) -> SemanticsConfiguration {
        let own = self.unmerged_config();
        if self.fake.is_some() || !own.is_merging_semantics_of_descendants {
            return own;
        }
        self.read(|tree, node| tree.merged_config_of(node)).flatten().unwrap_or(own)
    }

    /// Its configuration in its tree, as `config`: merged where it merges and the tree does.
    pub fn config(&self) -> SemanticsConfiguration {
        if self.merging_enabled { self.merged_config() } else { self.unmerged_config() }
    }

    /// The value of `key` in `merged_config`, as `getMergedOrNull`.
    pub fn get_merged_or_null<T: SemanticsValue>(
        &self,
        key: &'static SemanticsPropertyKey<T>,
    ) -> Option<T> {
        self.merged_config().get(key).cloned()
    }

    /// Each property making `merged_config`, a key perhaps more than once and values not combined,
    /// its own first, then its descendants' in merge order, as `forEachMergedProperty`.
    pub fn for_each_merged_property(
        &self,
        mut action: impl FnMut(&dyn AnySemanticsPropertyKey, &ValueSlot),
    ) {
        self.any_merged_property(|key, value| {
            action(key, value);
            false
        });
    }

    /// Whether a property making `merged_config` says yes, the first stopping it, as
    /// `anyMergedProperty`.
    pub fn any_merged_property(
        &self,
        mut predicate: impl FnMut(&dyn AnySemanticsPropertyKey, &ValueSlot) -> bool,
    ) -> bool {
        let configs = self.configs_merged();
        configs.iter().any(|config| config.properties().any(|(key, value)| predicate(key, value)))
    }

    fn configs_merged(&self) -> Vec<SemanticsConfiguration> {
        let own = self.unmerged_config();
        let merges = own.is_merging_semantics_of_descendants && !own.is_clearing_semantics;
        let mut configs = vec![own];
        if self.fake.is_none() && merges {
            self.read(|tree, node| {
                let mut stack: Vec<NodeId> =
                    tree.z_sorted_children(node).into_iter().rev().collect();
                while let Some(id) = stack.pop() {
                    let value = tree.node(id);
                    if value.overlay || value.deactivated || !value.attached {
                        continue;
                    }
                    let mut goes_on = true;
                    if let Some(own) = tree.own_semantics(id) {
                        if own.config.is_merging_semantics_of_descendants {
                            continue;
                        }
                        configs.push(own.config.clone());
                        goes_on = !own.config.is_clearing_semantics;
                    }
                    if goes_on {
                        stack.extend(tree.z_sorted_children(id).into_iter().rev());
                    }
                }
            });
        }
        configs
    }

    /// Whether it merges its descendants in this tree, as `isMergingSemanticsOfDescendants`.
    pub fn is_merging_semantics_of_descendants(&self) -> bool {
        self.merging_enabled
            && self.fake.is_none()
            && self.unmerged_config().is_merging_semantics_of_descendants
    }

    /// The semantics nodes below it, as `children`: in drawing order; in the merged tree, none
    /// under a node clearing its semantics, and of a merging node, those merging themselves.
    pub fn children(&self) -> Vec<SemanticsNode> {
        self.children_with(false)
    }

    fn children_with(&self, include_deactivated: bool) -> Vec<SemanticsNode> {
        if self.fake.is_some() {
            return Vec::new();
        }
        let merging_enabled = self.merging_enabled;
        self.read(|tree, node| {
            let own = tree.own_semantics(node);
            let clears = own.as_ref().is_some_and(|own| own.config.is_clearing_semantics);
            let merges =
                own.as_ref().is_some_and(|own| own.config.is_merging_semantics_of_descendants);
            if merging_enabled && clears {
                return Vec::new();
            }
            let mut out: SmallVec<[NodeId; 8]> = SmallVec::new();
            if merging_enabled && merges {
                tree.one_layer_of_merging_nodes(node, &mut out);
            } else {
                tree.one_layer_of_semantics_nodes(node, include_deactivated, &mut out);
            }
            out.into_iter().map(|child| self.handle_of(child, tree, merging_enabled)).collect()
        })
        .unwrap_or_default()
    }

    /// Each child for the accessibility tree, as `forEachAccessibilityChild`: none under a node
    /// clearing its semantics, link markers left out; a merging node's own description and role as
    /// fake nodes around its children in the unmerged tree.
    pub fn for_each_accessibility_child(&self, block: impl FnMut(SemanticsNode)) {
        if self.fake.is_some() {
            return;
        }
        self.for_each_accessibility_child_of(&self.unmerged_config(), block);
    }

    /// `for_each_accessibility_child`, its unmerged configuration at hand.
    fn for_each_accessibility_child_of(
        &self,
        config: &SemanticsConfiguration,
        mut block: impl FnMut(SemanticsNode),
    ) {
        if config.is_clearing_semantics {
            return;
        }
        let marker = &SemanticsProperties::LINK_TEST_MARKER;
        let is_link_marker = |child: &SemanticsNode| {
            child.read(|tree, node| {
                tree.own_semantics(node).is_some_and(|own| own.config.contains(marker))
            }) == Some(true)
        };
        if self.is_merging_semantics_of_descendants() {
            self.children().iter().filter(|child| !is_link_marker(child)).for_each(|c| block(*c));
            return;
        }
        let children = self.children_with(false);
        let merges = config.is_merging_semantics_of_descendants;
        let description = config.get(&SemanticsProperties::CONTENT_DESCRIPTION);
        if merges && !children.is_empty() && description.is_some_and(|all| !all.is_empty()) {
            block(SemanticsNode { fake: Some(FakeKind::ContentDescription), ..*self });
        }
        children.iter().filter(|child| !is_link_marker(child)).for_each(|child| block(*child));
        if merges && !children.is_empty() && config.contains(&SemanticsProperties::ROLE) {
            block(SemanticsNode { fake: Some(FakeKind::Role), ..*self });
        }
    }

    /// `for_each_accessibility_child`'s, appended to `children`, as `collectAccessibilityChildren`.
    pub fn collect_accessibility_children(&self, children: &mut Vec<SemanticsNode>) {
        self.for_each_accessibility_child(|child| children.push(child));
    }

    /// Whether `for_each_accessibility_child` visits one, as `hasAccessibilityChild`.
    pub fn has_accessibility_child(&self) -> bool {
        let mut any = false;
        self.for_each_accessibility_child(|_| any = true);
        any
    }

    /// The node around it in its tree, as `parent`: a fake node's its owner, the closest merging
    /// one first in the merged tree. An overlay's is the root, as its window's.
    pub fn parent(&self) -> Option<SemanticsNode> {
        if self.fake.is_some() {
            return Some(SemanticsNode { fake: None, ..*self });
        }
        let merging_enabled = self.merging_enabled;
        self.read(|tree, node| {
            let parent = tree.semantics_parent(node, merging_enabled)?;
            Some(self.handle_of(parent, tree, merging_enabled))
        })
        .flatten()
    }

    /// Visits the layouts with semantics around it, the nearest first, a fake node's owner before
    /// those, until `block` says yes: that one, as `forEachSemanticsAncestor`.
    pub fn for_each_semantics_ancestor(
        &self,
        mut block: impl FnMut(&LayoutInfo, &SemanticsConfiguration) -> bool,
    ) -> Option<LayoutInfo> {
        let info = self.layout_info();
        if self.fake.is_some() {
            let config = SemanticsNode { fake: None, ..*self }.unmerged_config();
            if block(&info, &config) {
                return Some(info);
            }
        }
        let mut current = info.parent_info();
        while let Some(parent) = current {
            if let Some(config) = parent.semantics_configuration()
                && block(&parent, &config)
            {
                return Some(parent);
            }
            current = parent.parent_info();
        }
        None
    }

    fn owner_of_bounds(&self) -> SemanticsNode {
        SemanticsNode { fake: None, ..*self }
    }

    /// The size of what gives it its bounds, unclipped.
    pub fn size(&self) -> IntSize {
        let this = self.owner_of_bounds();
        this.read(|tree, node| tree.semantics_geometry(node).1).unwrap_or(IntSize::new(0, 0))
    }

    /// Its bounds in the root, clipped, as `boundsInRoot`.
    pub fn bounds_in_root(&self) -> Rect {
        let this = self.owner_of_bounds();
        this.read(|tree, node| tree.semantics_geometry(node).0.clipped).unwrap_or(EMPTY)
    }

    /// Its bounds in the root, unclipped, as `unclippedBoundsInRoot`.
    pub fn unclipped_bounds_in_root(&self) -> Rect {
        let this = self.owner_of_bounds();
        this.read(|tree, node| tree.semantics_geometry(node).0.unclipped).unwrap_or(EMPTY)
    }

    /// Its top left in the root, unclipped, as `positionInRoot`.
    pub fn position_in_root(&self) -> Offset {
        let bounds = self.unclipped_bounds_in_root();
        Offset::new(bounds.left, bounds.top)
    }

    /// As `boundsInWindow`: the root fills the window.
    pub fn bounds_in_window(&self) -> Rect {
        self.bounds_in_root()
    }

    /// As `positionInWindow`: the root fills the window.
    pub fn position_in_window(&self) -> Offset {
        self.position_in_root()
    }

    /// As `positionOnScreen`, where a window at the screen's origin would have it: the hosts don't
    /// say where their windows are (ui-input's divergences).
    pub fn position_on_screen(&self) -> Offset {
        self.position_in_root()
    }

    /// Where it takes touches in the root, clipped, as `touchBoundsInRoot`: with a click action,
    /// at least the minimum touch target around it.
    pub fn touch_bounds_in_root(&self) -> Rect {
        let this = self.owner_of_bounds();
        this.read(|tree, node| tree.semantics_touch_bounds(node, false)).unwrap_or(EMPTY)
    }

    /// Its bounds in the closest layout around it with bounds, or the root, clipped, as
    /// `calculateBoundsInParent`.
    pub fn calculate_bounds_in_parent(&self) -> Rect {
        let this = self.owner_of_bounds();
        let fake = self.fake.is_some();
        this.read(|tree, node| {
            let bounds = tree.semantics_geometry(node).0.clipped;
            let root = tree.root_id();
            let mut current = if fake { Some(node) } else { tree.node(node).parent };
            while let Some(id) = current {
                if id == root || tree.own_semantics(id).is_some_and(|own| own.has_bounds) {
                    let origin = tree.semantics_geometry(id).0.unclipped;
                    return Rect::new(
                        bounds.left - origin.left,
                        bounds.top - origin.top,
                        bounds.right - origin.left,
                        bounds.bottom - origin.top,
                    );
                }
                current = if tree.node(id).overlay { Some(root) } else { tree.node(id).parent };
            }
            EMPTY
        })
        .unwrap_or(EMPTY)
    }

    /// The product of the graphics layers' alphas around it, as `computeEffectiveAlpha`.
    pub fn compute_effective_alpha(&self) -> f32 {
        self.read(|tree, node| tree.alpha_around(node, None)).unwrap_or(1.0)
    }

    /// Whether accessibility should leave it out, as `isHidden`: transparent or hiding.
    pub fn is_hidden(&self) -> bool {
        let this = self.owner_of_bounds();
        this.read(|tree, node| tree.semantics_is_transparent(node)).unwrap_or(false)
            || self.unmerged_config().contains(&SemanticsProperties::HIDE_FROM_ACCESSIBILITY)
    }

    /// Whether it means anything to accessibility, as `isImportantForAccessibility`: not hidden nor
    /// a link marker, and it merges or sets what accessibility reads.
    pub fn is_important_for_accessibility(&self) -> bool {
        self.is_important_given(&self.unmerged_config())
    }

    /// `is_important_for_accessibility`, its unmerged configuration at hand: what says so first, what
    /// walks the layers around it last.
    fn is_important_given(&self, config: &SemanticsConfiguration) -> bool {
        if config.contains(&SemanticsProperties::LINK_TEST_MARKER)
            || config.contains(&SemanticsProperties::HIDE_FROM_ACCESSIBILITY)
        {
            return false;
        }
        let says = config.is_merging_semantics_of_descendants
            || config.properties().any(|(key, _)| key.is_important_for_accessibility());
        says && !self
            .owner_of_bounds()
            .read(|tree, node| tree.semantics_is_transparent(node))
            .unwrap_or(false)
    }

    /// Whether a click action makes `touch_bounds_in_root` the minimum touch target.
    pub fn uses_minimum_touch_target(&self) -> bool {
        self.unmerged_config().contains(&SemanticsActions::ON_CLICK)
    }

    /// Its role, as `Role`, for a fake node too.
    pub fn role(&self) -> Option<Role> {
        self.unmerged_config().get(&SemanticsProperties::ROLE).copied()
    }
}

/// Bounds of a semantics node in the root.
pub(crate) struct SemanticsBounds {
    pub(crate) unclipped: Rect,
    pub(crate) clipped: Rect,
}

impl UiTree {
    /// The layout node with this semantics id, reused content's old id finding nothing.
    pub(crate) fn node_with_semantics_id(&self, id: i32) -> Option<NodeId> {
        let mut stack = vec![self.root_id()];
        while let Some(node) = stack.pop() {
            let value = self.node(node);
            if value.semantics_id == id {
                return Some(node);
            }
            stack.extend(value.children.iter().map(|child| child.node));
        }
        None
    }

    /// Where `node`'s semantic bounds are, and its size: the root's, or with no modifier saying
    /// so, its innermost layer's.
    pub(crate) fn semantics_geometry(&mut self, node: NodeId) -> (SemanticsBounds, IntSize) {
        if let Some((unclipped, clipped)) = self.semantics_bounds_in_root(node) {
            let layer =
                self.own_semantics(node).map_or(0, |own: std::rc::Rc<OwnSemantics>| own.layer);
            let measured = self.node(node).layers[layer].measured;
            return (SemanticsBounds { unclipped, clipped }, measured);
        }
        let value = self.node(node);
        let measured = value.layers.last().map_or(IntSize::new(0, 0), |layer| layer.measured);
        let attached = value.placed || value.parent.is_none();
        let bounds = if attached { self.bounds_in_root(node) } else { EMPTY };
        (SemanticsBounds { unclipped: bounds, clipped: bounds }, measured)
    }

    /// `node`'s touch bounds: its clipped bounds, with a click action at least the minimum touch
    /// target around it, each clip around it letting the target past as `rectInParent` does. With
    /// `clip_to_large_parents`, a clip at least the target's size stops it (`clippedTouchBoundsInRoot`):
    /// what's covered is decided inside such a clip.
    /// `semantics_touch_bounds` both ways, for what's asked of every node of a tree: one without a
    /// click has no target past its clips.
    pub(crate) fn semantics_touch_bounds_both(&mut self, node: NodeId) -> (Rect, Rect) {
        let clicks = self
            .own_semantics(node)
            .is_some_and(|own| own.config.contains(&SemanticsActions::ON_CLICK));
        if clicks {
            (self.semantics_touch_bounds(node, false), self.semantics_touch_bounds(node, true))
        } else {
            let clipped = self.semantics_geometry(node).0.clipped;
            (clipped, clipped)
        }
    }

    pub(crate) fn semantics_touch_bounds(
        &mut self,
        node: NodeId,
        clip_to_large_parents: bool,
    ) -> Rect {
        let (bounds, size) = self.semantics_geometry(node);
        let clicks = self
            .own_semantics(node)
            .is_some_and(|own| own.config.contains(&SemanticsActions::ON_CLICK));
        if !clicks {
            return bounds.clipped;
        }
        let density = self.density_of(node);
        let minimum = self.view_configuration_of(node).minimum_touch_target_size;
        let (target_width, target_height) =
            (density.to_px(minimum.width), density.to_px(minimum.height));
        let horizontal = ((target_width - size.width as f32) / 2.0).max(0.0);
        let vertical = ((target_height - size.height as f32) / 2.0).max(0.0);
        let mut touch = Rect::new(
            bounds.unclipped.left - horizontal,
            bounds.unclipped.top - vertical,
            bounds.unclipped.right + horizontal,
            bounds.unclipped.bottom + vertical,
        );
        for clip in self.clips_around_semantics(node) {
            let (width, height) = (clip.width(), clip.height());
            let (x, y) = (touch.left - clip.left, touch.top - clip.top);
            let (right, bottom) = (touch.right - clip.left, touch.bottom - clip.top);
            // How far past its start the clip lets the target go.
            let past = right < 0.0 || x > width || bottom < 0.0 || y > height;
            let before = |start: f32, end: f32, target: f32| {
                let under = (target - (end - start)) / 2.0;
                if past {
                    0.0
                } else if under > 0.0 {
                    start - under
                } else {
                    start.max((end - target).min(0.0)).max(-target / 2.0)
                }
            };
            let (from_x, from_y) =
                (before(x, right, target_width), before(y, bottom, target_height));
            let to_x = (width + target_width).min(width.max(from_x + target_width));
            let to_y = (height + target_height).min(height.max(from_y + target_height));
            let large_across = clip_to_large_parents && width >= target_width;
            let large_along = clip_to_large_parents && height >= target_height;
            let left = clip.left + if large_across { 0.0 } else { from_x };
            let top = clip.top + if large_along { 0.0 } else { from_y };
            let right = clip.left + if large_across { width } else { to_x };
            let bottom = clip.top + if large_along { height } else { to_y };
            touch = Rect::new(
                touch.left.max(left),
                touch.top.max(top),
                touch.right.min(right),
                touch.bottom.min(bottom),
            );
            if touch.right <= touch.left || touch.bottom <= touch.top {
                return EMPTY;
            }
        }
        touch
    }

    /// The window `node` is in: the overlay around it, or `None` for the main one.
    pub(crate) fn overlay_around(&self, node: NodeId) -> Option<NodeId> {
        let mut current = Some(node);
        while let Some(id) = current {
            let value = self.node(id);
            if value.overlay {
                return Some(id);
            }
            current = value.parent;
        }
        None
    }

    /// The one layer of semantics nodes below `node`, in drawing order: a node with nothing to say
    /// and a part of its parent looked through, an overlay a root of its own.
    pub(crate) fn one_layer_of_semantics_nodes(
        &mut self,
        node: NodeId,
        include_deactivated: bool,
        out: &mut SmallVec<[NodeId; 8]>,
    ) {
        for child in self.z_sorted_children(node) {
            let value = self.node(child);
            if value.overlay || !value.attached || (value.deactivated && !include_deactivated) {
                continue;
            }
            if self.has_semantics_node(child) {
                out.push(child);
            } else {
                self.one_layer_of_semantics_nodes(child, include_deactivated, out);
            }
        }
        if node == self.root_id() {
            for index in 0..self.overlays.len() {
                let overlay = self.overlays[index];
                if self.get(overlay).is_some_and(|value| value.overlay && value.placed) {
                    if self.has_semantics_node(overlay) {
                        out.push(overlay);
                    } else {
                        self.one_layer_of_semantics_nodes(overlay, include_deactivated, out);
                    }
                }
            }
        }
    }

    /// The nodes below `node` merging themselves, as `findOneLayerOfMergingSemanticsNodes`.
    pub(crate) fn one_layer_of_merging_nodes(
        &mut self,
        node: NodeId,
        out: &mut SmallVec<[NodeId; 8]>,
    ) {
        for child in self.z_sorted_children(node) {
            let value = self.node(child);
            if value.overlay || value.deactivated || !value.attached {
                continue;
            }
            match self.own_semantics(child) {
                Some(own) if own.config.is_merging_semantics_of_descendants => out.push(child),
                Some(own) if own.config.is_clearing_semantics => {}
                _ => self.one_layer_of_merging_nodes(child, out),
            }
        }
    }

    /// Whether `node` is a node of the semantics tree: it says something of its own.
    pub(crate) fn has_semantics_node(&mut self, node: NodeId) -> bool {
        self.own_semantics(node).is_some_and(|own| {
            !own.config.is_empty()
                && !own.config.contains(&crate::semantics_properties::PART_OF_PARENT)
        })
    }

    /// The node of the semantics tree around `node`.
    pub(crate) fn semantics_parent(
        &mut self,
        node: NodeId,
        merging_enabled: bool,
    ) -> Option<NodeId> {
        let root = self.root_id();
        if node == root {
            return None;
        }
        if self.node(node).overlay {
            return Some(root);
        }
        if merging_enabled {
            let mut current = self.node(node).parent;
            while let Some(id) = current {
                let merges = self
                    .own_semantics(id)
                    .is_some_and(|own| own.config.is_merging_semantics_of_descendants);
                if merges {
                    return Some(id);
                }
                if self.node(id).overlay {
                    break;
                }
                current = self.node(id).parent;
            }
        }
        let mut current = self.node(node).parent;
        while let Some(id) = current {
            if id == root || self.has_semantics_node(id) {
                return Some(id);
            }
            if self.node(id).overlay {
                return Some(root);
            }
            current = self.node(id).parent;
        }
        None
    }

    /// The layouts with semantics at `position` under `node`, the innermost first; a child hit
    /// stopping those under it (`hitTestChildren`), overlays over the rest.
    pub(crate) fn semantics_hits(
        &mut self,
        node: NodeId,
        position: Offset,
        hits: &mut Vec<NodeId>,
    ) -> bool {
        let mut hit = false;
        if node == self.root_id() {
            for index in (0..self.overlays.len()).rev() {
                let overlay = self.overlays[index];
                if self.get(overlay).is_some_and(|value| value.overlay && value.placed)
                    && self.semantics_hits(overlay, position, hits)
                {
                    return true;
                }
            }
        }
        let clears = self.own_semantics(node).is_some_and(|own| own.config.is_clearing_semantics);
        if !clears {
            for child in self.z_sorted_children(node).into_iter().rev() {
                let value = self.node(child);
                if !value.placed || value.overlay || value.deactivated {
                    continue;
                }
                if self.semantics_hits(child, position, hits) {
                    hit = true;
                    break;
                }
            }
        }
        if let Some(own) = self.own_semantics(node)
            && !own.config.contains(&SemanticsProperties::LINK_TEST_MARKER)
            && contains(self.semantics_geometry(node).0.clipped, position)
        {
            hits.push(node);
            hit = true;
        }
        hit
    }

    /// A layer with no alpha around what gives `node` its semantic bounds.
    pub(crate) fn semantics_is_transparent(&mut self, node: NodeId) -> bool {
        let entry = self.own_semantics(node).map_or(0, |own| own.bounds_entry);
        self.alpha_around(node, Some(entry)) <= 0.0
    }
}
