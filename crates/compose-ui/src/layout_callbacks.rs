//! Layout callbacks, as Compose's `Modifier.onSizeChanged` and `Modifier.onGloballyPositioned`:
//! application code told a node's size, or its size and position, once laid out.
//!
//! Both are called after the layout that changed what they report, outside the tree (so they
//! may write state: what that invalidates is composed and laid out again in the same frame,
//! before drawing).

use std::mem;
use std::ops::ControlFlow;

use compose_runtime::{Callback, IntoCallback};
use smallvec::SmallVec;

use crate::delegation::dispatch_with_context;
use crate::graphics_layer::{GraphicsLayerNode, LayerProperties};
use crate::input::Action;
use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::node_chain::{NodeKinds, entry_state};
use crate::node_context::NodeContext;
use crate::tree::{LayoutNode, NodeId, UiTree};
use crate::unit::{IntSize, Offset, Rect};

/// Where a laid-out node is, as Compose's `LayoutCoordinates` (a snapshot rather than a live
/// object): the size and position of the layer at the modifier reporting it (inside the layout
/// modifiers before it in the chain), in pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LayoutCoordinates {
    size: IntSize,
    position_in_root: Offset,
    clipped_bounds: Rect,
}

impl LayoutCoordinates {
    /// The size, as `LayoutCoordinates.size`.
    pub fn size(&self) -> IntSize {
        self.size
    }

    /// The top left in the root (the window's content), as `positionInRoot()`.
    pub fn position_in_root(&self) -> Offset {
        self.position_in_root
    }

    /// The top left in the window, as `positionInWindow()`: the root fills the window.
    pub fn position_in_window(&self) -> Offset {
        self.position_in_root
    }

    /// The bounds in the root, clipped by the clipping ancestors (a scroll's viewport, `clip`),
    /// as `boundsInRoot()`: empty when clipped away.
    pub fn bounds_in_root(&self) -> Rect {
        self.clipped_bounds
    }

    /// The bounds in the window, as `boundsInWindow()`.
    pub fn bounds_in_window(&self) -> Rect {
        self.clipped_bounds
    }
}

/// A node told its layer was measured and placed, as Compose's `LayoutAwareModifierNode`.
///
/// `on_remeasured` runs where the layer was measured, its reads not observed; the context may
/// invalidate, not measure. `on_placed` runs after the layout, with today's snapshot of the
/// coordinates, the outer nodes first (the placement pass will run it in place: D1186).
pub trait LayoutAwareModifierNode {
    /// The layer it's in was placed, or laid out again.
    fn on_placed(&mut self, _coordinates: LayoutCoordinates, _context: &mut NodeContext<'_>) {}

    /// The layer it's in was measured, at `size`.
    fn on_remeasured(&mut self, _size: IntSize, _context: &mut NodeContext<'_>) {}
}

/// A node told the size its layer was measured at, as Compose's `MeasuredSizeAwareModifierNode`.
pub trait MeasuredSizeAwareModifierNode {
    /// The layer it's in was measured, at `size`.
    fn on_remeasured(&mut self, size: IntSize, context: &mut NodeContext<'_>);
}

/// A node told where it is after each layout, and each change of a graphics layer around it that
/// moves it, as Compose's `GlobalPositionAwareModifierNode`. Called after the layout, outside
/// any node's code.
pub trait GlobalPositionAwareModifierNode {
    /// Its coordinates may have changed (or are first known).
    fn on_globally_positioned(
        &mut self,
        coordinates: LayoutCoordinates,
        context: &mut NodeContext<'_>,
    );
}

impl Modifier {
    /// Calls `on_size_changed` with the size of what follows in the chain when it's first
    /// measured and whenever it changes, as `Modifier.onSizeChanged`.
    pub fn on_size_changed(self, on_size_changed: impl IntoCallback<dyn Fn(IntSize)>) -> Modifier {
        self.then_custom(OnSizeChangedElement(on_size_changed.into_callback()))
    }

    /// Calls `on_placed` with the coordinates of what follows in the chain each time it's
    /// placed, as `Modifier.onPlaced`, after the layout (D1186).
    pub fn on_placed(self, on_placed: impl IntoCallback<dyn Fn(LayoutCoordinates)>) -> Modifier {
        self.then_custom(OnPlacedElement(on_placed.into_callback()))
    }

    /// Calls `on_globally_positioned` with the coordinates of what follows in the chain once
    /// laid out, whenever its size or position in the window changed, as
    /// `Modifier.onGloballyPositioned`.
    pub fn on_globally_positioned(
        self,
        on_globally_positioned: impl IntoCallback<dyn Fn(LayoutCoordinates)>,
    ) -> Modifier {
        self.then_custom(OnGloballyPositionedElement(on_globally_positioned.into_callback()))
    }
}

struct OnSizeChangedElement(Callback<dyn Fn(IntSize)>);

struct OnSizeChangedNode {
    callback: Callback<dyn Fn(IntSize)>,
    last: Option<IntSize>,
}

impl ModifierNodeElement for OnSizeChangedElement {
    type Node = OnSizeChangedNode;
    fn create(&self) -> OnSizeChangedNode {
        OnSizeChangedNode { callback: self.0.clone(), last: None }
    }
    fn update(&self, node: &mut OnSizeChangedNode) {
        node.callback = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        other.0.ptr_eq(&self.0)
    }
    fn name(&self) -> &'static str {
        "on_size_changed"
    }
}

impl ModifierNode for OnSizeChangedNode {
    fn as_measured_size_aware(&mut self) -> Option<&mut dyn MeasuredSizeAwareModifierNode> {
        Some(self)
    }
    /// A new callback (a closure is one at each recomposition, Compose's compiler keeping its
    /// own) is told the next change: nothing to measure for it (D1188).
    fn should_auto_invalidate(&self) -> bool {
        false
    }
    /// Reused, it reports the new content's size, measured again for it.
    fn on_reset(&mut self, context: &mut NodeContext<'_>) {
        self.last = None;
        context.invalidate_measurement();
    }
}

impl MeasuredSizeAwareModifierNode for OnSizeChangedNode {
    fn on_remeasured(&mut self, size: IntSize, context: &mut NodeContext<'_>) {
        if self.last != Some(size) {
            self.last = Some(size);
            let callback = self.callback.clone();
            context.run_after_layout(move || callback(size));
        }
    }
}

struct OnPlacedElement(Callback<dyn Fn(LayoutCoordinates)>);

pub(crate) struct OnPlacedNode {
    callback: Callback<dyn Fn(LayoutCoordinates)>,
}

impl ModifierNodeElement for OnPlacedElement {
    type Node = OnPlacedNode;
    fn create(&self) -> OnPlacedNode {
        OnPlacedNode { callback: self.0.clone() }
    }
    fn update(&self, node: &mut OnPlacedNode) {
        node.callback = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        other.0.ptr_eq(&self.0)
    }
    fn name(&self) -> &'static str {
        "on_placed"
    }
}

impl ModifierNode for OnPlacedNode {
    fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
        Some(self)
    }
    /// As `on_size_changed`: a new closure, each recomposition, lays nothing out.
    fn should_auto_invalidate(&self) -> bool {
        false
    }
}

impl LayoutAwareModifierNode for OnPlacedNode {
    fn on_placed(&mut self, coordinates: LayoutCoordinates, context: &mut NodeContext<'_>) {
        let callback = self.callback.clone();
        context.run_after_layout(move || callback(coordinates));
    }
}

struct OnGloballyPositionedElement(Callback<dyn Fn(LayoutCoordinates)>);

struct OnGloballyPositionedNode {
    callback: Callback<dyn Fn(LayoutCoordinates)>,
    last: Option<LayoutCoordinates>,
}

impl ModifierNodeElement for OnGloballyPositionedElement {
    type Node = OnGloballyPositionedNode;
    fn create(&self) -> OnGloballyPositionedNode {
        OnGloballyPositionedNode { callback: self.0.clone(), last: None }
    }
    fn update(&self, node: &mut OnGloballyPositionedNode) {
        node.callback = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        other.0.ptr_eq(&self.0)
    }
    fn name(&self) -> &'static str {
        "on_globally_positioned"
    }
}

impl ModifierNode for OnGloballyPositionedNode {
    fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
        Some(self)
    }
    /// Reused, it reports the new content's coordinates.
    fn on_reset(&mut self, _context: &mut crate::node_context::NodeContext<'_>) {
        self.last = None;
    }
}

impl GlobalPositionAwareModifierNode for OnGloballyPositionedNode {
    fn on_globally_positioned(
        &mut self,
        coordinates: LayoutCoordinates,
        context: &mut NodeContext<'_>,
    ) {
        if self.last != Some(coordinates) {
            self.last = Some(coordinates);
            let callback = self.callback.clone();
            context.run_after_layout(move || callback(coordinates));
        }
    }
}

impl UiTree {
    /// The callbacks the layouts since the last call queued: sizes changed, the layers placed,
    /// and the coordinates of the position-aware nodes, if anything was laid out or a layer
    /// around them moved.
    pub fn take_layout_callbacks(&mut self) -> Vec<Action> {
        // What the nodes delegated while measured.
        if !self.delegates_noted.is_empty() {
            self.layout_nodes_told = true;
            self.sync_noted_delegates();
        }
        if !self.placed_layers.is_empty() {
            self.layout_nodes_told = true;
            self.notify_placed();
        }
        let laid_out = mem::take(&mut self.laid_out_since_callbacks);
        if !self.positioned.is_empty() && (laid_out || !self.layers_moved.is_empty()) {
            self.layout_nodes_told = true;
            let mut positioned = mem::take(&mut self.positioned);
            let listening = positioned.len();
            let waited = mem::replace(&mut self.delegates_wait, true);
            positioned.retain(|&node| {
                if laid_out || self.under_a_moved_layer(node) {
                    self.notify_positioned(node)
                } else {
                    self.get(node).is_some()
                }
            });
            self.delegates_wait = waited;
            let left = positioned.len() < listening;
            // Those that came with what the nodes delegated.
            for node in mem::take(&mut self.positioned) {
                if !positioned.contains(&node) {
                    positioned.push(node);
                }
            }
            self.positioned = positioned;
            self.sync_noted_delegates();
            if left {
                self.forget_layers_no_one_listens_to();
            }
        }
        if !self.layers_moved.is_empty() {
            for node in mem::take(&mut self.layers_moved) {
                if let Some(value) = self.get_mut(node) {
                    value.layer_moved = false;
                }
            }
        }
        mem::take(&mut self.layout_callbacks)
    }

    /// One of `node`'s graphics layers may have changed: if it now transforms differently from
    /// what a node under it was last told, those under it are told, with no layout, as
    /// `NodeCoordinator.updateLayerParameters`. Its block runs here only if a node under it was
    /// told; with `keep`, a state's change, what it computes for the node's group is what the
    /// frame's drawing uses, observed as there.
    pub(crate) fn note_layer_change(&mut self, node: NodeId, keep: bool) {
        if self.positioned.is_empty() {
            return;
        }
        let Some(value) = self.get(node) else { return };
        if value.layer_moved || value.deactivated || !value.kinds.any(NodeKinds::GRAPHICS_LAYER) {
            return;
        }
        let group_layer = value.retained.as_ref().and_then(|retained| retained.layer_entry);
        let listeners = self.position_listeners;
        let mut moved = false;
        for index in 0..value.chain.len() {
            let value = self.node(node);
            let Some(graphics) = value.chain[index].node.downcast_ref::<GraphicsLayerNode>() else {
                continue;
            };
            if !graphics.told_a_node(listeners) {
                continue;
            }
            if keep && group_layer == Some(index as u16) {
                let observer = self.observer.clone();
                let Some(properties) = self.layer_properties(node, observer.as_deref()) else {
                    continue;
                };
                let value = self.node(node);
                let graphics =
                    value.chain[index].node.downcast_ref::<GraphicsLayerNode>().expect("a layer");
                moved |= graphics.transform_changed(&properties);
                let (density, direction) = (value.density, value.layout_direction);
                let retained = self.node_mut(node).retained.as_mut().expect("its group");
                retained.layer_ahead = Some((properties, density, direction));
            } else {
                moved |=
                    graphics.transform_changed(&self.properties_of_layer(node, index, graphics));
            }
        }
        if moved {
            self.node_mut(node).layer_moved = true;
            self.layers_moved.push(node);
        }
    }

    /// An updated element changed a layer of `node` while composing, as
    /// `NodeCoordinator.updateLayerBlock`: its block's reads aren't the composing scope's.
    pub(crate) fn note_updated_layer(&mut self, node: NodeId) {
        if let Some(retained) = self.node_mut(node).retained.as_mut() {
            retained.layer_ahead = None;
        }
        compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            self.note_layer_change(node, false);
        });
    }

    /// The properties of `graphics`, the node of chain entry `index` of `node`, seen from the
    /// node's first layer.
    fn properties_of_layer(
        &self,
        node: NodeId,
        index: usize,
        graphics: &GraphicsLayerNode,
    ) -> LayerProperties {
        let value = self.node(node);
        let layer = value.layer_of(index);
        graphics.properties(
            value.density,
            value.layout_direction,
            layer_origin(value, layer),
            value.layers[layer].measured.into(),
        )
    }

    /// A position-aware node left: only the layers those left map through keep what they told.
    fn forget_layers_no_one_listens_to(&mut self) {
        self.position_listeners = self.position_listeners.wrapping_add(1);
        for &node in &self.positioned {
            if self.get(node).is_some() {
                let layer = self.node(node).layers.len() - 1;
                self.map_to_root(node, layer, None, Offset::ZERO);
            }
        }
    }

    /// Whether `node` or an ancestor has a graphics layer that moved.
    fn under_a_moved_layer(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            let Some(value) = self.get(id) else { return false };
            if value.layer_moved {
                return true;
            }
            current = if value.overlay { None } else { value.parent };
        }
        false
    }

    /// Tells the nodes of the layers placed and of those inside them in their layout nodes
    /// (`LayoutModifierNodeCoordinator`'s placing places what it wraps), each once, the outer
    /// ones first as a placement goes: those whose node is placed, a layer measured but not
    /// placed being told when it is.
    fn notify_placed(&mut self) {
        // Each by the place orders from the root: a parent, then what's in it, then its next
        // sibling (`MeasurePassDelegate`'s placement is depth first).
        type Path = SmallVec<[u64; 8]>;
        let mut placed: SmallVec<[(Path, u32, NodeId); 4]> = SmallVec::new();
        for (node, layer) in mem::take(&mut self.placed_layers) {
            let Some(value) = self.get(node) else { continue };
            let mut path: Path = SmallVec::new();
            path.push(value.place_order);
            let mut current = value.parent;
            while let Some(parent) = current {
                let value = self.node(parent);
                path.push(value.place_order);
                current = value.parent;
            }
            path.reverse();
            placed.push((path, layer, node));
        }
        placed.sort_unstable_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        placed.dedup_by_key(|&mut (.., node)| node);
        let waited = mem::replace(&mut self.delegates_wait, true);
        for (_, layer, node) in placed {
            let layer = layer as usize;
            // A node removed by what an earlier one did stays out.
            if self.get(node).is_none_or(|value| value.deactivated || layer >= value.layers.len())
                || !self.is_placed_in_root(node)
            {
                continue;
            }
            let first = self.node(node).entries_of_layer(layer).start;
            for index in first..self.node(node).chain.len() {
                let Some(entry) = self.node(node).chain.get(index) else { break };
                if !entry.kinds.any(NodeKinds::ON_PLACED)
                    || entry.state & entry_state::ATTACHED == 0
                {
                    continue;
                }
                let own = entry.own;
                let layer = self.node(node).layer_of(index);
                let coordinates = self.coordinates(node, layer, index);
                self.with_entry(node, index, |modifier_node, context| {
                    let _ = dispatch_with_context(
                        modifier_node,
                        own,
                        NodeKinds::ON_PLACED,
                        false,
                        context,
                        &mut |modifier_node, context| {
                            if let Some(aware) = modifier_node.as_layout_aware() {
                                aware.on_placed(coordinates, context);
                            }
                            ControlFlow::Continue(())
                        },
                    );
                });
            }
        }
        self.delegates_wait = waited;
        self.sync_noted_delegates();
    }

    /// Tells `node`'s position-aware modifier nodes where they are, if it's placed. Returns
    /// whether it still has any.
    fn notify_positioned(&mut self, node: NodeId) -> bool {
        let Some(value) = self.get(node) else { return false };
        let count = value.chain.len();
        let mut any = false;
        let placed = self.is_placed_in_root(node);
        for index in 0..count {
            let Some(entry) = self.node(node).chain.get(index) else { break };
            if !entry.kinds.any(NodeKinds::GLOBAL_POSITION_AWARE) {
                continue;
            }
            any = true;
            if !placed || entry.state & entry_state::ATTACHED == 0 {
                continue;
            }
            let own = entry.own;
            let layer = self.node(node).layer_of(index);
            let coordinates = self.coordinates(node, layer, index);
            // The node, or each of its delegates that wants to know.
            self.with_entry(node, index, |modifier_node, context| {
                let _ = dispatch_with_context(
                    modifier_node,
                    own,
                    NodeKinds::GLOBAL_POSITION_AWARE,
                    false,
                    context,
                    &mut |modifier_node, context| {
                        if let Some(aware) = modifier_node.as_global_position_aware() {
                            aware.on_globally_positioned(coordinates, context);
                        }
                        ControlFlow::Continue(())
                    },
                );
            });
        }
        any
    }

    /// The bounds in the root of semantics node `node` (its semantics modifier's layer), in
    /// pixels: unclipped, its top left mapped through the graphics layers around it and its own
    /// size, unscaled, as Compose's `unclippedBoundsInRoot` (`Rect(positionInRoot, size)` in
    /// ui-test's `BoundsAssertions.kt`); and the layer-mapped bounding box clipped by the clips
    /// around it (its own outside its semantics, its ancestors'), as `SemanticsNode.boundsInRoot`.
    /// `None` if it has no semantics or isn't placed where the root shows it. For tests' bounds
    /// assertions.
    pub fn semantics_bounds_in_root(&mut self, node: NodeId) -> Option<(Rect, Rect)> {
        self.get(node)?;
        let layer = self.own_semantics(node)?.layer;
        if !self.is_placed_in_root(node) {
            return None;
        }
        let value = self.node(node);
        let mut layer_of_entry = 0;
        let mut entry = None;
        for (index, bits) in value.chain.iter().map(|entry| entry.kinds).enumerate() {
            if entry.is_none() && layer_of_entry == layer && bits.any(NodeKinds::SEMANTICS) {
                entry = Some(index);
            }
            if bits.any(NodeKinds::LAYOUT) {
                layer_of_entry += 1;
            }
        }
        let coordinates = self.coordinates(node, layer, entry.unwrap_or(0));
        let size = self.node(node).layers[layer].measured;
        let unclipped = Rect::from_offset_size(coordinates.position_in_root, size.into());
        Some((unclipped, coordinates.clipped_bounds))
    }

    /// Whether `node` and its ancestors are placed: laid out where the root shows it.
    pub(crate) fn is_placed_in_root(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            let value = self.node(id);
            if !value.placed && value.parent.is_some() && !value.overlay {
                return false;
            }
            current = if value.overlay { None } else { value.parent };
        }
        true
    }

    /// The coordinates of layer `layer` of `node`, for its chain entry `entry`: its bounds
    /// clipped by the clipping modifiers around it (its own before the entry, and its
    /// ancestors').
    fn coordinates(&self, node: NodeId, layer: usize, entry: usize) -> LayoutCoordinates {
        let value = self.node(node);
        let size = value.layers[layer].size;
        let origin = self.map_to_root(node, layer, Some(entry), Offset::ZERO);
        let mut bounds = self.rect_in_root(node, layer, Some(entry), size);
        let clip_node = |id: NodeId, before_entry: Option<usize>, bounds: &mut Rect| {
            let value = self.node(id);
            let mut layer_of_entry = 0;
            for (index, entry) in value.chain.iter().enumerate() {
                if before_entry.is_some_and(|before| index >= before) {
                    break;
                }
                let size = value.layers[layer_of_entry].size;
                if entry.node.hit_clip_at(size.into(), value.density).is_some() {
                    let clip = self.rect_in_root(id, layer_of_entry, Some(index), size);
                    *bounds = intersect(*bounds, clip);
                }
                if value.chain[index].kinds.any(NodeKinds::LAYOUT) {
                    layer_of_entry += 1;
                }
            }
        };
        clip_node(node, Some(entry), &mut bounds);
        let mut current = if value.overlay { None } else { value.parent };
        while let Some(id) = current {
            clip_node(id, None, &mut bounds);
            let value = self.node(id);
            current = if value.overlay { None } else { value.parent };
        }
        LayoutCoordinates { size, position_in_root: origin, clipped_bounds: bounds }
    }

    /// The bounding box in the root of a rectangle of `size` at the origin of layer `layer` of
    /// `node` (seen by its chain entry `before_entry`), through the graphics layers around it.
    fn rect_in_root(
        &self,
        node: NodeId,
        layer: usize,
        before_entry: Option<usize>,
        size: IntSize,
    ) -> Rect {
        let (width, height) = (size.width as f32, size.height as f32);
        let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)]
            .map(|(x, y)| self.map_to_root(node, layer, before_entry, Offset::new(x, y)));
        let min = |pick: fn(&Offset) -> f32| corners.iter().map(pick).fold(f32::INFINITY, f32::min);
        let max =
            |pick: fn(&Offset) -> f32| corners.iter().map(pick).fold(f32::NEG_INFINITY, f32::max);
        Rect::new(min(|p| p.x), min(|p| p.y), max(|p| p.x), max(|p| p.y))
    }

    /// Where `point`, in layer `layer` of `node`, is in the root: through each layer's offset
    /// and the graphics layers' transforms (`node`'s before its chain entry `before_entry`,
    /// all of its ancestors'), as Compose's `localToRoot`.
    fn map_to_root(
        &self,
        node: NodeId,
        layer: usize,
        before_entry: Option<usize>,
        point: Offset,
    ) -> Offset {
        let (mut current, mut layer, mut before) = (node, layer, before_entry);
        let mut point = point;
        loop {
            let value = self.node(current);
            point = point + layer_origin(value, layer);
            let end = before.unwrap_or(value.chain.len());
            // The innermost first.
            for index in (0..end).rev() {
                let Some(graphics) = value.chain[index].node.downcast_ref::<GraphicsLayerNode>()
                else {
                    continue;
                };
                let properties = self.properties_of_layer(current, index, graphics);
                graphics.note_transform(&properties, self.position_listeners);
                point = properties.map(point);
            }
            if value.overlay {
                return point + value.overlay_origin;
            }
            let position = value.layers[0].offset;
            point = point + Offset::new(position.x as f32, position.y as f32);
            match value.parent {
                Some(parent) => {
                    current = parent;
                    layer = self.node(parent).layers.len() - 1;
                    before = None;
                }
                None => return point,
            }
        }
    }

    /// Registers `node` as having position-aware modifier nodes (from its chain refresh).
    pub(crate) fn register_positioned(&mut self, node: NodeId) {
        if !self.positioned.contains(&node) {
            self.positioned.push(node);
        }
        self.laid_out_since_callbacks = true;
    }
}

/// Where layer `layer` of `value` is, in the node's own coordinates (its first layer's).
fn layer_origin(value: &LayoutNode, layer: usize) -> Offset {
    value.layers[1..=layer].iter().fold(Offset::ZERO, |origin, entry| {
        origin + Offset::new(entry.offset.x as f32, entry.offset.y as f32)
    })
}

/// The intersection of two rectangles (empty at the first's top left if they don't meet).
fn intersect(a: Rect, b: Rect) -> Rect {
    let rect = Rect::new(
        a.left.max(b.left),
        a.top.max(b.top),
        a.right.min(b.right),
        a.bottom.min(b.bottom),
    );
    if rect.right <= rect.left || rect.bottom <= rect.top {
        Rect::new(rect.left, rect.top, rect.left, rect.top)
    } else {
        rect
    }
}
