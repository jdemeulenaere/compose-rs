//! Live coordinates, as Compose's `LayoutCoordinates`: a `Copy` handle on what a node's modifiers
//! and policy lay out, whose answers follow the tree. It reaches the tree through the thread's
//! registry, or, in a pass, through the tree's access window (`tree_access`, D1211).
//!
//! A *coordinator* here is what Compose's `NodeCoordinator` is: each layout modifier and each
//! graphics layer ends one, and the node's policy has the last. compose-rs's layers are the layout
//! ones alone, so a coordinator is a layer and how many of the chain's graphics layers are around
//! it.

use std::cell::Cell;

use smallvec::SmallVec;

use crate::graphics_layer::{GraphicsLayerNode, LayerMap};
use crate::node_chain::{EntryId, NodeKinds};
use crate::node_context::{peek_tree, requests_of};
use crate::path::Matrix;
use crate::tree::{LayoutNode, NodeId, UiTree};
use crate::unit::{IntSize, Offset, Rect};

/// As Compose's `LayoutCoordinates`: where something a node lays out is, live. `Copy`, `'static`,
/// and never stale: once what it names is gone it answers `is_attached() == false` and zeros
/// (Compose's fails for most). Asked where its tree is busy (docs/06-ui.md, "Coordinates in a
/// pass"), it answers the same way.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LayoutCoordinates {
    tree: u32,
    node: NodeId,
    /// The chain entry ending the coordinator, or `None` for the node's innermost.
    end: Option<EntryId>,
}

/// A coordinator found in the tree: its layer, and the chain entries outside it.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Place {
    node: NodeId,
    layer: usize,
    before: usize,
}

/// One step of the way from one place to another.
enum Step {
    Move(Offset),
    Layer(LayerMap),
    Unlayer(LayerMap),
}

impl Step {
    fn map(&self, point: Offset) -> Offset {
        match self {
            Step::Move(by) => point + *by,
            Step::Layer(layer) => layer.map(point),
            Step::Unlayer(layer) => layer.unmap(point),
        }
    }

    /// Makes `matrix` do this too. What can't be undone is left out, as Compose's `invert` leaves
    /// a singular matrix as it is.
    fn apply_to(&self, matrix: &mut Matrix) {
        match self {
            Step::Move(by) => matrix.post_concat(&Matrix::translation(by.x, by.y)),
            Step::Layer(layer) => matrix.post_concat(&layer.matrix()),
            Step::Unlayer(layer) => {
                if layer.invertible() {
                    let mut inverse = layer.matrix();
                    inverse.invert();
                    matrix.post_concat(&inverse);
                }
            }
        }
    }
}

thread_local! {
    /// The node whose placing code runs, and whether it asked where a node of its is, as
    /// Compose's `onCoordinatesUsed`.
    static READING: Cell<Option<(NodeId, bool)>> = const { Cell::new(None) };
}

/// Runs `placing`, code placing `node`: with whether it asked where it is.
pub(crate) fn with_position_reads_of<R>(node: NodeId, placing: impl FnOnce() -> R) -> (R, bool) {
    let previous = READING.replace(Some((node, false)));
    let result = placing();
    let reads = READING.replace(previous).is_some_and(|(_, reads)| reads);
    (result, reads)
}

/// A query asked where `node` is.
fn note_position_read(node: NodeId) {
    if let Some((reading, false)) = READING.get()
        && reading == node
    {
        READING.set(Some((node, true)));
    }
}

impl LayoutCoordinates {
    pub(crate) fn new(tree: u32, node: NodeId, end: Option<EntryId>) -> LayoutCoordinates {
        LayoutCoordinates { tree, node, end }
    }

    /// Runs `read` with the tree: in a pass, through its window, else through the registry. It
    /// runs no application code: a graphics layer's answers are its last run's. `None` if the tree
    /// is gone or busy.
    fn read<R>(&self, read: impl FnOnce(&UiTree) -> R) -> Option<R> {
        let requests = requests_of(self.tree)?;
        let mut read = Some(read);
        if let Some(result) = requests.window.read(|tree| (read.take().expect("once"))(tree)) {
            return Some(result);
        }
        let mut result = None;
        peek_tree(self.tree, &mut |tree: &mut UiTree| {
            if let Some(read) = read.take() {
                result = Some(read(tree));
            }
        });
        result
    }

    /// A read of where it is, which a placement block's run keeps for its node (as Compose's
    /// `onCoordinatesUsed`).
    fn read_position<R>(&self, read: impl FnOnce(&UiTree, Place) -> R) -> Option<R> {
        self.read(|tree| {
            note_position_read(self.node);
            let place = tree.find_place(self)?;
            Some(read(tree, place))
        })
        .flatten()
    }

    /// The coordinates of chain entry `entry` of `node`.
    pub(crate) fn of_entry(tree: u32, node: NodeId, entry: EntryId) -> Option<LayoutCoordinates> {
        let handle = LayoutCoordinates::new(tree, node, None);
        handle
            .read(|tree| {
                let index = tree.get(node)?.chain.iter().position(|it| it.id == entry)?;
                Some(tree.coordinates_at(node, index))
            })
            .flatten()
    }

    /// Its node, and the tree's key.
    pub(crate) fn node_in_tree(&self) -> (u32, NodeId) {
        (self.tree, self.node)
    }

    /// Its node and how many of its chain's entries are outside it, in `tree`.
    pub(crate) fn entry_in(&self, tree: &UiTree) -> Option<(NodeId, usize)> {
        let place = tree.find_place(self)?;
        Some((place.node, place.before))
    }

    /// Whether what it names is in the tree, as `isAttached`.
    pub fn is_attached(&self) -> bool {
        self.read(|tree| tree.find_place(self).is_some()).unwrap_or(false)
    }

    /// Its size, as `LayoutCoordinates.size`: what it was measured at.
    pub fn size(&self) -> IntSize {
        self.read(|tree| {
            let place = tree.find_place(self)?;
            Some(tree.node(place.node).layers[place.layer].measured)
        })
        .flatten()
        .unwrap_or(IntSize::ZERO)
    }

    /// The coordinates outside it, as `parentCoordinates`: of the same node, or of its parent's
    /// content.
    pub fn parent_coordinates(&self) -> Option<LayoutCoordinates> {
        self.read_position(|tree, place| {
            let value = tree.node(place.node);
            for index in (0..place.before).rev() {
                if value.chain[index].kinds.any(COORDINATOR_ENDS) {
                    let end = Some(value.chain[index].id);
                    return Some(LayoutCoordinates::new(self.tree, self.node, end));
                }
            }
            self.parent_layout_of(value)
        })
        .flatten()
    }

    /// The coordinates of the parent's content, as `parentLayoutCoordinates`: `None` at the root
    /// (a popup's content is a root).
    pub fn parent_layout_coordinates(&self) -> Option<LayoutCoordinates> {
        self.read_position(|tree, place| self.parent_layout_of(tree.node(place.node))).flatten()
    }

    fn parent_layout_of(&self, value: &LayoutNode) -> Option<LayoutCoordinates> {
        let parent = if value.overlay { None } else { value.parent };
        parent.map(|parent| LayoutCoordinates::new(self.tree, parent, None))
    }

    /// `local` in the root, as `localToRoot`: a popup's content's own, whose root it's placed
    /// from in the window.
    pub fn local_to_root(&self, local: Offset) -> Offset {
        self.read_position(|tree, place| {
            let at = tree.map_to_root(place.node, place.layer, Some(place.before), local);
            let popup = tree.popup_origin(place.node);
            Offset::new(at.x - popup.x, at.y - popup.y)
        })
        .unwrap_or(Offset::ZERO)
    }

    /// `local` in the window, as `localToWindow`: the root fills it, and a popup's is placed in it.
    pub fn local_to_window(&self, local: Offset) -> Offset {
        self.read_position(|tree, place| {
            tree.map_to_root(place.node, place.layer, Some(place.before), local)
        })
        .unwrap_or(Offset::ZERO)
    }

    /// `window` here, as `windowToLocal`.
    pub fn window_to_local(&self, window: Offset) -> Offset {
        self.read_position(|tree, place| tree.root_to_place(place, window)).unwrap_or(Offset::ZERO)
    }

    /// `local` on the screen, as `localToScreen`: from the origin the host gives
    /// (`UiHost::set_window_origin`).
    pub fn local_to_screen(&self, local: Offset) -> Offset {
        self.read_position(|tree, place| {
            tree.map_to_root(place.node, place.layer, Some(place.before), local)
                + tree.window_origin
        })
        .unwrap_or(Offset::ZERO)
    }

    /// `screen` here, as `screenToLocal`.
    pub fn screen_to_local(&self, screen: Offset) -> Offset {
        self.read_position(|tree, place| {
            let origin = tree.window_origin;
            tree.root_to_place(place, Offset::new(screen.x - origin.x, screen.y - origin.y))
        })
        .unwrap_or(Offset::ZERO)
    }

    /// `relative_to_source` in `source`, here, as `localPositionOf`.
    pub fn local_position_of(
        &self,
        source: &LayoutCoordinates,
        relative_to_source: Offset,
    ) -> Offset {
        self.local_position_of_with(source, relative_to_source, true)
    }

    /// `local_position_of`, leaving out what's placed under a motion frame of reference on the
    /// way if `include_motion_frame_of_reference` is false (a list's scroll, for what animates
    /// bounds through it).
    pub fn local_position_of_with(
        &self,
        source: &LayoutCoordinates,
        relative_to_source: Offset,
        include_motion_frame_of_reference: bool,
    ) -> Offset {
        self.read_position(|tree, place| {
            note_position_read(source.node);
            let Some(from) = tree.find_place(source) else { return Offset::ZERO };
            let mut point = relative_to_source;
            tree.walk(from, place, include_motion_frame_of_reference, &mut |step| {
                point = step.map(point);
            });
            point
        })
        .unwrap_or(Offset::ZERO)
    }

    /// The bounding box here of `source`, as `localBoundingBoxOf`: clipped by the clips on the way
    /// with `clip_bounds`, `Rect` of zeros if that leaves nothing.
    pub fn local_bounding_box_of(&self, source: &LayoutCoordinates, clip_bounds: bool) -> Rect {
        self.read(|tree| {
            note_position_read(self.node);
            note_position_read(source.node);
            let (to, from) = (tree.find_place(self)?, tree.find_place(source)?);
            Some(tree.bounding_box(from, to, clip_bounds))
        })
        .flatten()
        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
    }

    /// Sets `matrix` to what takes `source`'s coordinates here, as `transformFrom`.
    pub fn transform_from(&self, source: &LayoutCoordinates, matrix: &mut Matrix) {
        *matrix = Matrix::new();
        self.read_position(|tree, place| {
            note_position_read(source.node);
            let Some(from) = tree.find_place(source) else { return };
            tree.walk(from, place, true, &mut |step| step.apply_to(matrix));
        });
    }

    /// Sets `matrix` to what takes these coordinates to the screen, as `transformToScreen`.
    pub fn transform_to_screen(&self, matrix: &mut Matrix) {
        *matrix = Matrix::new();
        self.read_position(|tree, place| {
            tree.walk_to_root(place, &mut |step| step.apply_to(matrix));
            let origin = tree.window_origin;
            matrix.post_concat(&Matrix::translation(origin.x, origin.y));
        });
    }

    /// Whether a motion frame of reference placed it, as `introducesMotionFrameOfReference`.
    pub fn introduces_motion_frame_of_reference(&self) -> bool {
        self.read(|tree| {
            let place = tree.find_place(self)?;
            let value = tree.node(place.node);
            // The coordinator a layer's placing places: its outermost, and no graphics layer's
            // inside it.
            let outermost = (value.entries_of_layer(place.layer).start..place.before)
                .all(|index| !value.chain[index].kinds.any(NodeKinds::GRAPHICS_LAYER));
            Some(outermost && value.placed_in_motion(place.layer))
        })
        .flatten()
        .unwrap_or(false)
    }

    /// Its top left in the root, as `positionInRoot()`.
    pub fn position_in_root(&self) -> Offset {
        self.local_to_root(Offset::ZERO)
    }

    /// Its top left in the window, as `positionInWindow()`.
    pub fn position_in_window(&self) -> Offset {
        self.local_to_window(Offset::ZERO)
    }

    /// Its top left on the screen, as `positionOnScreen()`.
    pub fn position_on_screen(&self) -> Offset {
        self.local_to_screen(Offset::ZERO)
    }

    /// Its top left in its parent's content, as `positionInParent()`.
    pub fn position_in_parent(&self) -> Offset {
        self.parent_layout_coordinates()
            .map_or(Offset::ZERO, |parent| parent.local_position_of(self, Offset::ZERO))
    }

    /// Its bounds in its parent's content, clipped by the clips on the way, as `boundsInParent()`.
    pub fn bounds_in_parent(&self) -> Rect {
        match self.parent_layout_coordinates() {
            Some(parent) => parent.local_bounding_box_of(self, true),
            None => {
                let size = self.size();
                Rect::new(0.0, 0.0, size.width as f32, size.height as f32)
            }
        }
    }

    /// Its bounds in the root, clipped by the clips around it (a scroll's viewport, `clip`), as
    /// `boundsInRoot()`: zeros when clipped away.
    pub fn bounds_in_root(&self) -> Rect {
        self.find_root_coordinates().local_bounding_box_of(self, true)
    }

    /// Its bounds in the window, clipped by it too, as `boundsInWindow()`: zeros when clipped away.
    pub fn bounds_in_window(&self) -> Rect {
        self.bounds_in_window_with(true)
    }

    /// `bounds_in_window`, not clipped at all without `clip_bounds`, as `boundsInWindow(clipBounds)`.
    pub fn bounds_in_window_with(&self, clip_bounds: bool) -> Rect {
        let root = self.find_root_coordinates();
        let bounds = root.local_bounding_box_of(self, clip_bounds);
        let size = root.size();
        let coerce = |value: f32, max: i32| {
            if clip_bounds { value.clamp(0.0, (max as f32).max(0.0)) } else { value }
        };
        let (left, top) = (coerce(bounds.left, size.width), coerce(bounds.top, size.height));
        let (right, bottom) =
            (coerce(bounds.right, size.width), coerce(bounds.bottom, size.height));
        if left == right || top == bottom {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        let corners = [(left, top), (right, top), (left, bottom), (right, bottom)]
            .map(|(x, y)| root.local_to_window(Offset::new(x, y)));
        bounding_box_of(&corners)
    }

    /// The outermost coordinates of the root, as `findRootCoordinates()`.
    pub fn find_root_coordinates(&self) -> LayoutCoordinates {
        self.read(|tree| {
            let mut root = self.node;
            while let Some(value) = tree.get(root) {
                match value.parent {
                    Some(parent) if !value.overlay => root = parent,
                    _ => break,
                }
            }
            let end = tree.get(root).and_then(|value| {
                (value.chain.iter().find(|entry| entry.kinds.any(COORDINATOR_ENDS)))
                    .map(|entry| entry.id)
            });
            LayoutCoordinates::new(self.tree, root, end)
        })
        .unwrap_or(*self)
    }
}

/// What ends a coordinator: a layout modifier, and a graphics layer (Compose's is one).
const COORDINATOR_ENDS: NodeKinds = NodeKinds::LAYOUT.with(NodeKinds::GRAPHICS_LAYER);

fn bounding_box_of(points: &[Offset]) -> Rect {
    let min = |pick: fn(&Offset) -> f32| points.iter().map(pick).fold(f32::INFINITY, f32::min);
    let max = |pick: fn(&Offset) -> f32| points.iter().map(pick).fold(f32::NEG_INFINITY, f32::max);
    Rect::new(min(|p| p.x), min(|p| p.y), max(|p| p.x), max(|p| p.y))
}

impl UiTree {
    /// The coordinates entry `index` of `node` is in: the next one ending a coordinator.
    pub(crate) fn coordinates_at(&self, node: NodeId, index: usize) -> LayoutCoordinates {
        let Some(value) = self.get(node) else {
            return LayoutCoordinates::new(self.key, node, None);
        };
        let end = (value.chain.get(index..).unwrap_or_default().iter())
            .find(|entry| entry.kinds.any(COORDINATOR_ENDS))
            .map(|entry| entry.id);
        LayoutCoordinates::new(self.key, node, end)
    }

    /// The coordinates a layer's measurement places in.
    pub(crate) fn coordinates_of_layer(&self, node: NodeId, layer: usize) -> LayoutCoordinates {
        let value = self.node(node);
        let end = value.layout_indices.get(layer).map(|&index| value.chain[index as usize].id);
        LayoutCoordinates::new(self.key, node, end)
    }

    /// Where `coordinates` is, if what it names is still there and attached.
    fn find_place(&self, coordinates: &LayoutCoordinates) -> Option<Place> {
        if coordinates.tree != self.key {
            return None;
        }
        let node = coordinates.node;
        let value = self.get(node).filter(|value| value.attached && !value.deactivated)?;
        match coordinates.end {
            None => Some(Place { node, layer: value.layers.len() - 1, before: value.chain.len() }),
            Some(end) => {
                let before = value.chain.iter().position(|entry| entry.id == end)?;
                Some(Place { node, layer: value.layer_of(before), before })
            }
        }
    }

    /// `node`'s ancestors from itself up to its root (a popup's content is one).
    fn path_to_root(&self, node: NodeId) -> SmallVec<[NodeId; 16]> {
        let mut path = SmallVec::new();
        let mut current = Some(node);
        while let Some(id) = current {
            path.push(id);
            let value = self.node(id);
            current = if value.overlay { None } else { value.parent };
        }
        path
    }

    /// From layer `from.0` under the graphics layers before entry `from.1`, out to the same with
    /// `to`, inside one node. The transforms are in the node's first layer's coordinates, so what
    /// they're given is too.
    fn walk_out(
        &self,
        node: NodeId,
        from: (usize, usize),
        to: (usize, usize),
        motion: bool,
        visit: &mut dyn FnMut(Step),
    ) {
        let value = self.node(node);
        visit(Step::Move(crate::layout_callbacks::layer_origin(value, from.0)));
        for index in (to.1..from.1).rev() {
            if let Some(layer) = self.layer_at(node, index) {
                visit(Step::Layer(layer));
            }
        }
        let mut back = crate::layout_callbacks::layer_origin(value, to.0);
        if !motion {
            for layer in (to.0 + 1)..=from.0 {
                if value.placed_in_motion(layer) {
                    let offset = value.layers[layer].offset;
                    back = back + Offset::new(offset.x as f32, offset.y as f32);
                }
            }
        }
        visit(Step::Move(Offset::new(0.0 - back.x, 0.0 - back.y)));
    }

    /// The inverse of `walk_out`.
    fn walk_in(
        &self,
        node: NodeId,
        from: (usize, usize),
        to: (usize, usize),
        motion: bool,
        visit: &mut dyn FnMut(Step),
    ) {
        let value = self.node(node);
        let mut origin = crate::layout_callbacks::layer_origin(value, from.0);
        if !motion {
            for layer in (from.0 + 1)..=to.0 {
                if value.placed_in_motion(layer) {
                    let offset = value.layers[layer].offset;
                    origin = origin + Offset::new(offset.x as f32, offset.y as f32);
                }
            }
        }
        visit(Step::Move(origin));
        for index in from.1..to.1 {
            if let Some(layer) = self.layer_at(node, index) {
                visit(Step::Unlayer(layer));
            }
        }
        let back = crate::layout_callbacks::layer_origin(value, to.0);
        visit(Step::Move(Offset::new(0.0 - back.x, 0.0 - back.y)));
    }

    /// How chain entry `index` of `node` moves points, if it's a graphics layer: its last run's.
    fn layer_at(&self, node: NodeId, index: usize) -> Option<LayerMap> {
        let value = self.node(node);
        let graphics = value.chain[index].node.downcast_ref::<GraphicsLayerNode>()?;
        let layer = value.layer_of(index);
        let origin = crate::layout_callbacks::layer_origin(value, layer);
        Some(graphics.layer_map(origin, value.layers[layer].measured.into()))
    }

    /// Where a node's top left is in its parent's content, as a step, if `motion` or not placed
    /// under a motion frame of reference.
    fn position_step(&self, node: NodeId, motion: bool) -> Option<Offset> {
        let value = self.node(node);
        (motion || !value.placed_in_motion(0)).then(|| {
            let position = value.layers[0].offset;
            Offset::new(position.x as f32, position.y as f32)
        })
    }

    /// Visits the steps from `from` to `to`, through their common ancestor, as `localPositionOf`.
    /// In different roots, through the window.
    fn walk(&self, from: Place, to: Place, motion: bool, visit: &mut dyn FnMut(Step)) {
        let (up, down) = (self.path_to_root(from.node), self.path_to_root(to.node));
        // The first of `up` that's in `down`.
        let meeting = up.iter().position(|node| down.contains(node));
        let Some(meeting) = meeting else {
            self.walk_to_root(from, visit);
            self.walk_from_root(to, visit);
            return;
        };
        let common = up[meeting];
        let inner = |node: NodeId| {
            let value = self.node(node);
            (value.layers.len() - 1, value.chain.len())
        };
        // Out of `from` to the common node.
        let mut at = (from.layer, from.before);
        for &node in &up[..meeting] {
            self.walk_out(node, at, (0, 0), motion, visit);
            if let Some(by) = self.position_step(node, motion) {
                visit(Step::Move(by));
            }
            at = inner(self.node(node).parent.expect("under the common node"));
        }
        let at_end = if common == to.node { (to.layer, to.before) } else { inner(common) };
        // The outer of the two, within it.
        let middle = if at.1 <= at_end.1 { at } else { at_end };
        self.walk_out(common, at, middle, motion, visit);
        self.walk_in(common, middle, at_end, motion, visit);
        // In from the common node to `to`.
        let way_down = down.iter().position(|node| *node == common).expect("the common node");
        for &node in down[..way_down].iter().rev() {
            if let Some(by) = self.position_step(node, motion) {
                visit(Step::Move(Offset::new(0.0 - by.x, 0.0 - by.y)));
            }
            let end = if node == to.node { (to.layer, to.before) } else { inner(node) };
            self.walk_in(node, (0, 0), end, motion, visit);
        }
    }

    /// Visits the steps from `from` to the root's coordinates (or the window's, from a popup).
    fn walk_to_root(&self, from: Place, visit: &mut dyn FnMut(Step)) {
        let mut at = (from.layer, from.before);
        for &node in &self.path_to_root(from.node) {
            self.walk_out(node, at, (0, 0), true, visit);
            let value = self.node(node);
            if value.overlay {
                visit(Step::Move(value.overlay_origin));
                return;
            }
            if let Some(by) = self.position_step(node, true) {
                visit(Step::Move(by));
            }
            if let Some(parent) = value.parent {
                let value = self.node(parent);
                at = (value.layers.len() - 1, value.chain.len());
            }
        }
    }

    /// The inverse of `walk_to_root`.
    fn walk_from_root(&self, to: Place, visit: &mut dyn FnMut(Step)) {
        for &node in self.path_to_root(to.node).iter().rev() {
            let value = self.node(node);
            let by = if value.overlay {
                value.overlay_origin
            } else {
                self.position_step(node, true).unwrap_or(Offset::ZERO)
            };
            visit(Step::Move(Offset::new(0.0 - by.x, 0.0 - by.y)));
            let end = if node == to.node {
                (to.layer, to.before)
            } else {
                (value.layers.len() - 1, value.chain.len())
            };
            self.walk_in(node, (0, 0), end, true, visit);
        }
    }

    /// `point` of the root's coordinates (or the window's, for what's in a popup), in `place`.
    fn root_to_place(&self, place: Place, point: Offset) -> Offset {
        let mut point = point;
        self.walk_from_root(place, &mut |step| point = step.map(point));
        point
    }

    /// `point` of `from`, in `to`.
    fn map_between(&self, from: Place, to: Place, point: Offset) -> Offset {
        let mut point = point;
        self.walk(from, to, true, &mut |step| point = step.map(point));
        point
    }

    /// The bounding box in `to` of `from`, clipped by the clips on the way with `clip_bounds`,
    /// as `localBoundingBoxOf`: up from `from` to the common coordinator and down to `to`, the
    /// clips of those in between, not its own.
    fn bounding_box(&self, from: Place, to: Place, clip_bounds: bool) -> Rect {
        let of_place = |place: Place, size: IntSize| {
            let (width, height) = (size.width as f32, size.height as f32);
            let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)]
                .map(|(x, y)| self.map_between(place, to, Offset::new(x, y)));
            bounding_box_of(&corners)
        };
        let mut bounds = of_place(from, self.node(from.node).layers[from.layer].measured);
        if clip_bounds {
            self.clips_between(from, to, &mut |place, size| {
                bounds = crate::layout_callbacks::intersect(bounds, of_place(place, size));
            });
        }
        // Zeros once what it goes through leaves nothing, and only then.
        if from != to && (bounds.right <= bounds.left || bounds.bottom <= bounds.top) {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        bounds
    }

    /// Visits the clips between `from` and `to`, each as the place and size of what it clips to.
    fn clips_between(&self, from: Place, to: Place, visit: &mut dyn FnMut(Place, IntSize)) {
        let mut clips = |node: NodeId, entries: std::ops::Range<usize>| {
            let value = self.node(node);
            for index in entries {
                if value.chain[index].clips() {
                    let layer = value.layer_of(index);
                    let place = Place { node, layer, before: index + 1 };
                    visit(place, value.layers[layer].measured);
                }
            }
        };
        let (up, down) = (self.path_to_root(from.node), self.path_to_root(to.node));
        let all = |node: NodeId| self.node(node).chain.len();
        let Some(meeting) = up.iter().position(|node| down.contains(node)) else {
            for &node in &up {
                clips(node, 0..if node == from.node { from.before } else { all(node) });
            }
            for &node in &down {
                clips(node, 0..if node == to.node { to.before } else { all(node) });
            }
            return;
        };
        let common = up[meeting];
        for &node in &up[..meeting] {
            clips(node, 0..if node == from.node { from.before } else { all(node) });
        }
        let at = if common == from.node { from.before } else { all(common) };
        let at_end = if common == to.node { to.before } else { all(common) };
        let middle = at.min(at_end);
        clips(common, middle..at);
        clips(common, middle..at_end);
        let way_down = down.iter().position(|node| *node == common).expect("the common node");
        for &node in down[..way_down].iter().rev() {
            clips(node, 0..if node == to.node { to.before } else { all(node) });
        }
    }

    /// Where a popup's root is in the window, for what's in one.
    fn popup_origin(&self, node: NodeId) -> Offset {
        let top = *self.path_to_root(node).last().expect("a node");
        let value = self.node(top);
        if value.overlay { value.overlay_origin } else { Offset::ZERO }
    }
}
