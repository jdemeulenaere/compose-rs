//! Layout callbacks, as Compose's `Modifier.onSizeChanged` and `Modifier.onGloballyPositioned`:
//! application code told a node's size, or its size and position, once laid out.
//!
//! Both are called after the layout that changed what they report, outside the tree (so they
//! may write state: what that invalidates is composed and laid out again in the same frame,
//! before drawing).

use compose_runtime::{Callback, IntoCallback};

use crate::constraints::Constraints;
use crate::input::Action;
use crate::layout::{Measurable, MeasureResult, MeasureScope};
use crate::modifier::{LayoutModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::tree::{NodeId, UiTree, capability};
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
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(OnSizeChangedNode { callback: self.0.clone(), last: None })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<OnSizeChangedNode>() {
            node.callback = self.0.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<OnSizeChangedElement>().is_some_and(|other| other.0.ptr_eq(&self.0))
    }
    fn name(&self) -> &'static str {
        "on_size_changed"
    }
}

impl ModifierNode for OnSizeChangedNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
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
        if self.last != Some(size) {
            self.last = Some(size);
            let callback = self.callback.clone();
            scope.tree.layout_callbacks.push(Action::Run(Box::new(move || callback(size))));
        }
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

struct OnGloballyPositionedElement(Callback<dyn Fn(LayoutCoordinates)>);

struct OnGloballyPositionedNode {
    callback: Callback<dyn Fn(LayoutCoordinates)>,
    last: Option<LayoutCoordinates>,
}

impl ModifierNodeElement for OnGloballyPositionedElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(OnGloballyPositionedNode { callback: self.0.clone(), last: None })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<OnGloballyPositionedNode>() {
            node.callback = self.0.clone();
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other
            .downcast_ref::<OnGloballyPositionedElement>()
            .is_some_and(|other| other.0.ptr_eq(&self.0))
    }
    fn name(&self) -> &'static str {
        "on_globally_positioned"
    }
}

impl ModifierNode for OnGloballyPositionedNode {
    fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
        Some(self)
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
            if value.chain[index].as_global_position_aware().is_none() {
                continue;
            }
            any = true;
            if !placed {
                continue;
            }
            let layer = value.capabilities[..index]
                .iter()
                .filter(|bits| **bits & capability::LAYOUT != 0)
                .count();
            let coordinates = self.coordinates(node, layer);
            if let Some(action) = self.node_mut(node).chain[index]
                .as_global_position_aware()
                .and_then(|aware| aware.on_globally_positioned(coordinates))
            {
                self.layout_callbacks.push(action);
            }
        }
        any
    }

    /// Whether `node` and its ancestors are placed: laid out where the root shows it.
    fn is_placed_in_root(&self, node: NodeId) -> bool {
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

    /// The coordinates of layer `layer` of `node`: its bounds clipped by the clipping modifiers
    /// around it (its own before that layer, and its ancestors').
    fn coordinates(&self, node: NodeId, layer: usize) -> LayoutCoordinates {
        let value = self.node(node);
        let size = value.layers[layer].size;
        let origin = self.layer_origin(node, layer);
        let mut bounds = Rect::from_offset_size(origin, size.into());
        let clip_node = |id: NodeId, before_layer: Option<usize>, bounds: &mut Rect| {
            let value = self.node(id);
            let mut layer_of_entry = 0;
            for (index, entry) in value.chain.iter().enumerate() {
                if before_layer.is_some_and(|before| layer_of_entry >= before) {
                    break;
                }
                if entry.hit_clip().is_some() {
                    let origin = self.layer_origin(id, layer_of_entry);
                    let size = value.layers[layer_of_entry].size;
                    *bounds = intersect(*bounds, Rect::from_offset_size(origin, size.into()));
                }
                if value.capabilities[index] & capability::LAYOUT != 0 {
                    layer_of_entry += 1;
                }
            }
        };
        clip_node(node, Some(layer + 1), &mut bounds);
        let mut current = if value.overlay { None } else { value.parent };
        while let Some(id) = current {
            clip_node(id, None, &mut bounds);
            let value = self.node(id);
            current = if value.overlay { None } else { value.parent };
        }
        LayoutCoordinates { size, position_in_root: origin, clipped_bounds: bounds }
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
