//! Layout callbacks, as Compose's `Modifier.onSizeChanged` and `Modifier.onGloballyPositioned`:
//! application code told a node's size, or its size and position, once laid out.
//!
//! Both are called after the layout that changed what they report, outside the tree (so they
//! may write state: what that invalidates is composed and laid out again in the same frame,
//! before drawing).

use compose_runtime::{Callback, IntoCallback};

use crate::constraints::Constraints;
use crate::input::Action;
use crate::layout::{IntrinsicMeasureScope, Measurable, MeasureResult, MeasureScope};
use crate::modifier::{LayoutModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::node_chain::NodeKinds;
use crate::tree::{NodeId, UiTree};
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

/// A node told where it is after each layout that moved or resized it, as Compose's
/// `GlobalPositionAwareModifierNode`. Called while the tree is laid out: it queues what it
/// does (`UiTree` runs the actions it returns after the layout).
pub trait GlobalPositionAwareModifierNode {
    /// The node's coordinates changed (or are first known): an action to run, if any.
    fn on_globally_positioned(&mut self, coordinates: LayoutCoordinates) -> Option<Action>;
}

impl Modifier {
    /// Calls `on_size_changed` with the size of what follows in the chain when it's first
    /// measured and whenever it changes, as `Modifier.onSizeChanged`.
    pub fn on_size_changed(self, on_size_changed: impl IntoCallback<dyn Fn(IntSize)>) -> Modifier {
        self.then_custom(OnSizeChangedElement(on_size_changed.into_callback()))
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
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    /// Reused, it reports the new content's size, measured again for it.
    fn on_reset(&mut self, context: &mut crate::node_context::NodeContext<'_>) {
        self.last = None;
        context.invalidate_measurement();
    }
}

impl LayoutModifierNode for OnSizeChangedNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let size = placeable.size();
        // An intrinsic measurement's size isn't the layout's.
        if self.last != Some(size) && !scope.is_intrinsic_measurement() {
            self.last = Some(size);
            let callback = self.callback.clone();
            scope.run_after_layout(move || callback(size));
        }
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }

    // Its content's intrinsics, as Compose's `onSizeChanged`, which isn't a layout modifier.
    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        scope.min_intrinsic_width(content, height)
    }
    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        scope.min_intrinsic_height(content, width)
    }
    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        height: i32,
    ) -> i32 {
        scope.max_intrinsic_width(content, height)
    }
    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: Measurable,
        width: i32,
    ) -> i32 {
        scope.max_intrinsic_height(content, width)
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
    fn on_globally_positioned(&mut self, coordinates: LayoutCoordinates) -> Option<Action> {
        if self.last == Some(coordinates) {
            return None;
        }
        self.last = Some(coordinates);
        let callback = self.callback.clone();
        Some(Action::Run(Box::new(move || callback(coordinates))))
    }
}

impl UiTree {
    /// The callbacks the layouts since the last call queued: sizes changed, and the
    /// coordinates of the position-aware nodes, if anything was laid out.
    pub fn take_layout_callbacks(&mut self) -> Vec<Action> {
        if std::mem::take(&mut self.laid_out_since_callbacks) && !self.positioned.is_empty() {
            let mut positioned = std::mem::take(&mut self.positioned);
            positioned.retain(|&node| self.notify_positioned(node));
            self.positioned = positioned;
        }
        std::mem::take(&mut self.layout_callbacks)
    }

    /// Tells `node`'s position-aware modifier nodes where they are, if it's placed. Returns
    /// whether it still has any.
    fn notify_positioned(&mut self, node: NodeId) -> bool {
        let Some(value) = self.get(node) else { return false };
        let count = value.chain.len();
        let mut any = false;
        let placed = self.is_placed_in_root(node);
        for index in 0..count {
            let value = self.node_mut(node);
            if value.chain[index].node.as_global_position_aware().is_none() {
                continue;
            }
            any = true;
            if !placed {
                continue;
            }
            let layer = value.layer_of(index);
            let coordinates = self.coordinates(node, layer, index);
            if let Some(action) = self.node_mut(node).chain[index]
                .node
                .as_global_position_aware()
                .and_then(|aware| aware.on_globally_positioned(coordinates))
            {
                self.layout_callbacks.push(action);
            }
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
                if entry.node.hit_clip_at(size.into(), self.density).is_some() {
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
            // In the node's own coordinates (its first layer's).
            let layer_origin = |layer: usize| {
                value.layers[1..=layer].iter().fold(Offset::ZERO, |origin, entry| {
                    origin + Offset::new(entry.offset.x as f32, entry.offset.y as f32)
                })
            };
            point = point + layer_origin(layer);
            let end = before.unwrap_or(value.chain.len());
            // The innermost first.
            for index in (0..end).rev() {
                let Some(graphics) = value.chain[index]
                    .node
                    .downcast_ref::<crate::graphics_layer::GraphicsLayerNode>()
                else {
                    continue;
                };
                let entry_layer = value.layer_of(index);
                let properties = graphics.properties(
                    self.density,
                    value.layout_direction,
                    layer_origin(entry_layer),
                    value.layers[entry_layer].measured.into(),
                );
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
