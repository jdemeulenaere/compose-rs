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

    /// The node of the merged tree a service's touch at `position` in the root finds, as Compose's
    /// `hitTestSemanticsAndInteropAt`, for a window's hit test each.
    pub fn accessibility_hit_test(&self, position: Offset) -> Option<i32> {
        self.read(|ui| ui.accessibility_hit(position)).flatten()
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
    let Some(left) = unaccounted.bounds_within(clipped) else {
        return;
    };
    let left =
        if clipped != touch { unaccounted.bounds_within(touch).unwrap_or(EMPTY) } else { left };
    visitor(node, left, hidden);
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
    let Some(left) = unaccounted.bounds_within(bounds) else {
        return;
    };
    visitor(node, left, hidden);
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

/// A region of the root, as `SemanticsRegion`, in bands as Android's `Region` keeps its: from the top,
/// each band a height and what's in it across, left to right and apart. Touching bands across alike
/// are one, so a list of rows narrower than the window leaves a band beside them, not a rectangle a
/// row: taking a row out is a matter of the few bands it meets.
struct SemanticsRegion {
    bands: Vec<RegionBand>,
}

#[derive(Clone, PartialEq, Debug)]
struct RegionBand {
    top: f32,
    bottom: f32,
    /// Left and right of each part.
    spans: SmallVec<[(f32, f32); 2]>,
}

impl SemanticsRegion {
    fn of(rect: Rect) -> SemanticsRegion {
        let rect = rounded(rect);
        let mut bands = Vec::new();
        if rect.right > rect.left && rect.bottom > rect.top {
            let spans = SmallVec::from_slice(&[(rect.left, rect.right)]);
            bands.push(RegionBand { top: rect.top, bottom: rect.bottom, spans });
        }
        SemanticsRegion { bands }
    }

    fn is_empty(&self) -> bool {
        self.bands.is_empty()
    }

    /// Its bounds, or `otherwise` if nothing's in it (a root of no size is still a root).
    fn bounds_or(&self, otherwise: Rect) -> Rect {
        let all = Rect::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::INFINITY);
        self.bounds_within(all).unwrap_or(otherwise)
    }

    /// The bounds of what's left of it in `rect`, if anything is.
    fn bounds_within(&self, rect: Rect) -> Option<Rect> {
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return None;
        }
        let first = self.bands.partition_point(|band| band.bottom <= rect.top);
        let mut bounds: Option<Rect> = None;
        for band in &self.bands[first..] {
            if band.top >= rect.bottom {
                break;
            }
            let start = band.spans.partition_point(|span| span.1 <= rect.left);
            let Some(&(left, _)) = band.spans.get(start).filter(|span| span.0 < rect.right) else {
                continue;
            };
            let end = band.spans.partition_point(|span| span.0 < rect.right);
            let (left, right) = (left.max(rect.left), band.spans[end - 1].1.min(rect.right));
            let (top, bottom) = (band.top.max(rect.top), band.bottom.min(rect.bottom));
            bounds = Some(match bounds {
                None => Rect::new(left, top, right, bottom),
                Some(it) => Rect::new(it.left.min(left), it.top, it.right.max(right), bottom),
            });
        }
        bounds
    }

    /// Takes `cut` out.
    fn difference(&mut self, cut: Rect) {
        if cut.right <= cut.left || cut.bottom <= cut.top {
            return;
        }
        let first = self.bands.partition_point(|band| band.bottom <= cut.top);
        let mut at = first;
        while at < self.bands.len() && self.bands[at].top < cut.bottom {
            let band = &self.bands[at];
            let start = band.spans.partition_point(|span| span.1 <= cut.left);
            if band.spans.get(start).is_none_or(|span| span.0 >= cut.right) {
                at += 1;
                continue;
            }
            // Only the height it's cut at changes.
            if band.top < cut.top {
                let above = RegionBand { bottom: cut.top, ..band.clone() };
                self.bands[at].top = cut.top;
                self.bands.insert(at, above);
                at += 1;
            }
            if cut.bottom < self.bands[at].bottom {
                let below = RegionBand { top: cut.bottom, ..self.bands[at].clone() };
                self.bands[at].bottom = cut.bottom;
                self.bands.insert(at + 1, below);
            }
            let spans = &mut self.bands[at].spans;
            let end = spans.partition_point(|span| span.0 < cut.right);
            let (before, after) = (spans[start].0, spans[end - 1].1);
            spans.drain(start..end);
            let mut put = start;
            if before < cut.left {
                spans.insert(put, (before, cut.left));
                put += 1;
            }
            if cut.right < after {
                spans.insert(put, (cut.right, after));
            }
            if spans.is_empty() {
                self.bands.remove(at);
            } else {
                at += 1;
            }
        }
        // What was split may be as its neighbours are now.
        let mut joined = first.saturating_sub(1);
        let mut end = (at + 1).min(self.bands.len());
        while joined + 1 < end {
            let (one, next) = (&self.bands[joined], &self.bands[joined + 1]);
            if one.bottom == next.top && one.spans == next.spans {
                self.bands[joined].bottom = self.bands[joined + 1].bottom;
                self.bands.remove(joined + 1);
                end -= 1;
            } else {
                joined += 1;
            }
        }
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
        says_something_important(config)
            && !self
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

/// `is_important_for_accessibility`, but for what's transparent: what its configuration says.
fn says_something_important(config: &SemanticsConfiguration) -> bool {
    if config.contains(&SemanticsProperties::LINK_TEST_MARKER)
        || config.contains(&SemanticsProperties::HIDE_FROM_ACCESSIBILITY)
    {
        return false;
    }
    config.is_merging_semantics_of_descendants
        || config.properties().any(|(key, _)| key.is_important_for_accessibility())
}

/// What a service's touch found at and below a node, as `HitTestResult` for its `SemanticsSource`.
struct AccessibilityHit {
    /// The innermost node hit a service reads, or what merges it. `None`: something's hit that it
    /// isn't told of (it's in a merging node hiding it), which still stops what's under it.
    found: Option<NodeId>,
    /// Negative: on it. Else the square of how far outside it, in its minimum touch target.
    distance: f32,
    /// In the layers around it, as a touch target past a clip isn't.
    in_layer: bool,
}

impl AccessibilityHit {
    /// Nothing under it can do better, as `hasHit`.
    fn stops_the_search(&self) -> bool {
        self.distance < 0.0 && self.in_layer
    }

    /// As `DistanceAndFlags`'s order: an earlier one wins a tie.
    fn is_better_than(&self, other: &AccessibilityHit) -> bool {
        if self.in_layer != other.in_layer {
            return self.in_layer;
        }
        self.distance < other.distance
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

    /// `node`'s touch bounds: its clipped bounds, with a click action at least the minimum touch
    /// target around it, each clip around it letting the target past as `rectInParent` does. With
    /// `clip_to_large_parents`, a clip at least the target's size stops it (`clippedTouchBoundsInRoot`):
    /// what's covered is decided inside such a clip.
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

    /// `SemanticsOwner::accessibility_hit_test`: each window over the one under it, the top-most first,
    /// a touch in one being its own, as each is a view of its own in Compose; what a window shutting
    /// out what's under it doesn't cover isn't anyone's.
    pub(crate) fn accessibility_hit(&mut self, position: Offset) -> Option<i32> {
        let shutting_out = self.overlay_shutting_out();
        for index in (shutting_out.unwrap_or(0)..self.overlays.len()).rev() {
            let overlay = self.overlays[index];
            let Some(value) = self.get(overlay).filter(|value| value.overlay && value.placed)
            else {
                continue;
            };
            let size = value.layers[0].size;
            let at = position - value.overlay_origin;
            let inside = contains(Rect::new(0.0, 0.0, size.width as f32, size.height as f32), at);
            let pass_through = self.overlay_node(overlay).is_some_and(|it| it.pass_through);
            let hit = self.accessibility_walk(overlay, position, true, None, false);
            if let Some(found) = hit.and_then(|hit| hit.found) {
                return Some(self.node(found).semantics_id);
            }
            if inside && !pass_through {
                return None;
            }
        }
        if shutting_out.is_some() {
            return None;
        }
        let found = self.accessibility_walk(self.root_id(), position, true, None, false)?.found?;
        Some(self.node(found).semantics_id)
    }

    /// What's hit at and below `node`, as `NodeCoordinator.hitTest` for `SemanticsSource`: a node a
    /// service doesn't read isn't hit, so what's under it still can be; every clip stops what's in it,
    /// past a node's minimum touch target; a hit on a sibling stops what's under that, but one in its
    /// touch target leaves the search going for something better. `merged_into` is what merges what's
    /// here, which is what a service knows of it, and `hidden` that it hides it from the service.
    fn accessibility_walk(
        &mut self,
        node: NodeId,
        position: Offset,
        in_layer: bool,
        merged_into: Option<NodeId>,
        hidden: bool,
    ) -> Option<AccessibilityHit> {
        let own = self.own_semantics(node);
        let mut merged_into = merged_into;
        let mut hidden = hidden;
        let mut clears = false;
        let mut hit = None;
        if let Some(own) = own {
            let config = &own.config;
            clears = config.is_clearing_semantics;
            let merges = config.is_merging_semantics_of_descendants;
            hidden |= hides_what_it_merges(config);
            if says_something_important(config) && !self.semantics_is_transparent(node) {
                let found =
                    (!hidden).then_some(if merges { node } else { merged_into.unwrap_or(node) });
                let (bounds, size) = self.semantics_geometry(node);
                let unclipped = bounds.unclipped;
                let in_clips = self.clips_contain(node, Some(own.bounds_entry), position);
                hit = if in_clips && contains(unclipped, position) {
                    Some(AccessibilityHit { found, distance: -1.0, in_layer })
                } else {
                    self.distance_in_touch_target(node, unclipped, size, position).map(|distance| {
                        AccessibilityHit { found, distance, in_layer: in_layer && in_clips }
                    })
                };
            }
            if merges {
                merged_into = Some(node);
            }
        }
        if clears {
            return hit;
        }
        // Past its own clips only where its touch target is.
        let in_layer = in_layer && {
            if self.clips_contain(node, None, position) {
                true
            } else {
                let value = self.node(node);
                let size = value.layers.last().map_or(IntSize::new(0, 0), |layer| layer.measured);
                let content = self.bounds_in_root(node);
                if self.distance_in_touch_target(node, content, size, position).is_none() {
                    return hit;
                }
                false
            }
        };
        let mut below: Option<AccessibilityHit> = None;
        for child in self.z_sorted_children(node).into_iter().rev() {
            let value = self.node(child);
            if !value.placed || value.overlay || value.deactivated {
                continue;
            }
            let Some(found) =
                self.accessibility_walk(child, position, in_layer, merged_into, hidden)
            else {
                continue;
            };
            let stops = found.stops_the_search();
            if below.as_ref().is_none_or(|best| found.is_better_than(best)) {
                below = Some(found);
            }
            if stops {
                break;
            }
        }
        match (hit, below) {
            (Some(hit), Some(below)) => {
                let found = below.found.or(hit.found);
                let best = if below.is_better_than(&hit) { below } else { hit };
                Some(AccessibilityHit { found, ..best })
            }
            (hit, below) => below.or(hit),
        }
    }

    /// As `distanceInMinimumTouchTarget`: the square of how far `position` is outside `bounds`, of a
    /// node measuring `size`, if that's within its minimum touch target.
    fn distance_in_touch_target(
        &self,
        node: NodeId,
        bounds: Rect,
        size: IntSize,
        position: Offset,
    ) -> Option<f32> {
        let density = self.density_of(node);
        let minimum = self.view_configuration_of(node).minimum_touch_target_size;
        let (width, height) = (density.to_px(minimum.width), density.to_px(minimum.height));
        let horizontal = ((width - size.width as f32) / 2.0).max(0.0);
        let vertical = ((height - size.height as f32) / 2.0).max(0.0);
        if horizontal <= 0.0 && vertical <= 0.0 {
            return None;
        }
        let x = (bounds.left - position.x).max(position.x - bounds.right).max(0.0);
        let y = (bounds.top - position.y).max(position.y - bounds.bottom).max(0.0);
        (x <= horizontal && y <= vertical).then_some(x * x + y * y)
    }

    /// Whether `position` in the root is in each clip of `node`'s chain, those before chain entry
    /// `before_entry` if it's given, by its outline.
    fn clips_contain(
        &mut self,
        node: NodeId,
        before_entry: Option<usize>,
        position: Offset,
    ) -> bool {
        let mut layer = 0;
        for index in 0..self.node(node).chain.len() {
            if before_entry.is_some_and(|before| index >= before) {
                break;
            }
            let value = self.node_mut(node);
            let measured = value.layers[layer].measured;
            let (density, direction) = (value.density, value.layout_direction);
            let bits = value.chain[index].kinds;
            use crate::node_chain::NodeKinds;
            if bits.any(NodeKinds::DRAW | NodeKinds::DELEGATING)
                && let Some(clip) = value.chain[index].hit_clip_at(measured.into(), density)
            {
                let origin = self.rect_in_root(node, layer, Some(index), measured);
                let local = position - Offset::new(origin.left, origin.top);
                if !clip.outline_in(measured.into(), &density, direction).contains(local) {
                    return false;
                }
            }
            if bits.any(NodeKinds::LAYOUT) {
                layer += 1;
            }
        }
        true
    }

    /// A layer with no alpha around what gives `node` its semantic bounds.
    pub(crate) fn semantics_is_transparent(&mut self, node: NodeId) -> bool {
        let entry = self.own_semantics(node).map_or(0, |own| own.bounds_entry);
        self.alpha_around(node, Some(entry)) <= 0.0
    }
}

#[cfg(test)]
mod region_tests {
    use super::*;

    const SIDE: usize = 40;

    /// What `region` holds, cell by cell.
    fn cells(region: &SemanticsRegion) -> Vec<bool> {
        let mut out = vec![false; SIDE * SIDE];
        for band in &region.bands {
            assert!(band.top < band.bottom);
            for pair in band.spans.windows(2) {
                assert!(pair[0].1 < pair[1].0, "apart, in order: {:?}", band.spans);
            }
            for &(left, right) in &band.spans {
                assert!(left < right);
                for y in band.top as usize..band.bottom as usize {
                    for x in left as usize..right as usize {
                        out[y * SIDE + x] = true;
                    }
                }
            }
        }
        for pair in region.bands.windows(2) {
            assert!(pair[0].bottom <= pair[1].top, "from the top: {:?}", region.bands);
            assert!(
                pair[0].bottom < pair[1].top || pair[0].spans != pair[1].spans,
                "joined: {:?}",
                region.bands
            );
        }
        out
    }

    fn rect(numbers: &mut u64) -> Rect {
        let mut next = |bound: u64| {
            *numbers = numbers.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((*numbers >> 33) % bound) as f32
        };
        let (x, y) = (next(SIDE as u64), next(SIDE as u64));
        Rect::new(x, y, (x + next(12)).min(SIDE as f32), (y + next(12)).min(SIDE as f32))
    }

    /// Against the cells themselves, through any cuts: what's in it, and its bounds in any rectangle.
    #[test]
    fn a_region_holds_what_cuts_left_of_it_whatever_they_are() {
        let mut numbers = 7u64;
        for _ in 0..50 {
            let side = SIDE as f32;
            let mut region = SemanticsRegion::of(Rect::new(0.0, 0.0, side, side));
            let mut expected = vec![true; SIDE * SIDE];
            for _ in 0..60 {
                let cut = rect(&mut numbers);
                region.difference(cut);
                for y in cut.top as usize..cut.bottom as usize {
                    for x in cut.left as usize..cut.right as usize {
                        expected[y * SIDE + x] = false;
                    }
                }
                assert_eq!(cells(&region), expected);
                let within = rect(&mut numbers);
                let mut bounds: Option<Rect> = None;
                for y in within.top as usize..within.bottom as usize {
                    for x in within.left as usize..within.right as usize {
                        if expected[y * SIDE + x] {
                            let (x, y) = (x as f32, y as f32);
                            bounds = Some(match bounds {
                                None => Rect::new(x, y, x + 1.0, y + 1.0),
                                Some(it) => Rect::new(
                                    it.left.min(x),
                                    it.top.min(y),
                                    it.right.max(x + 1.0),
                                    it.bottom.max(y + 1.0),
                                ),
                            });
                        }
                    }
                }
                assert_eq!(region.bounds_within(within), bounds);
            }
        }
    }

    /// The order the walk costs depends on: a column's rows narrower than the window.
    #[test]
    fn rows_narrower_than_the_region_leave_few_bands_however_many() {
        let mut region = SemanticsRegion::of(Rect::new(0.0, 0.0, 100.0, 10_000.0));
        for row in (0..1000).rev() {
            let top = row as f32 * 10.0;
            region.difference(Rect::new(10.0, top, 90.0, top + 10.0));
            assert!(region.bands.len() <= 2, "{}", region.bands.len());
        }
        assert_eq!(region.bands.len(), 1);
        assert_eq!(&region.bands[0].spans[..], &[(0.0, 10.0), (90.0, 100.0)]);
    }

    #[test]
    fn nothing_to_take_out_changes_nothing() {
        let whole = Rect::new(0.0, 0.0, 10.0, 10.0);
        let mut region = SemanticsRegion::of(whole);
        region.difference(Rect::new(5.0, 5.0, 5.0, 8.0));
        region.difference(Rect::new(20.0, 0.0, 30.0, 10.0));
        assert_eq!(region.bounds_or(EMPTY), whole);
        assert_eq!(region.bands.len(), 1);
        region.difference(whole);
        assert!(region.is_empty());
        assert_eq!(region.bounds_or(EMPTY), EMPTY);
    }
}
