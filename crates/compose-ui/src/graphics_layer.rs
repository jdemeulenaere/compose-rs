//! `Modifier.graphics_layer`, as Compose's `graphicsLayer`: alpha, scale, rotation and
//! translation applied to a node's drawing, without layout. Its block runs while drawing, so
//! state it reads (an animation's value) changes the drawing without recomposing or measuring.
//! A layer that is its node's first drawing modifier gives its properties to its repaint
//! boundary's group, as a render node's (render.rs): a write to what its block read changes
//! them without drawing the content again.
//!
//! Hit testing doesn't follow the transform yet: a scaled or translated node takes pointer
//! events at its laid-out bounds.

use std::rc::Rc;

use crate::display_list::{DisplayList, DrawOp};
use crate::draw::DrawScope;
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::unit::{Offset, Rect, Size};

/// Where a layer scales and rotates around, as fractions of its size, as `TransformOrigin`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransformOrigin {
    pub pivot_fraction_x: f32,
    pub pivot_fraction_y: f32,
}

impl TransformOrigin {
    pub const CENTER: TransformOrigin =
        TransformOrigin { pivot_fraction_x: 0.5, pivot_fraction_y: 0.5 };
}

/// The properties a graphics layer applies, as Compose's `GraphicsLayerScope`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GraphicsLayerScope {
    pub alpha: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    /// Degrees, clockwise.
    pub rotation_z: f32,
    /// Pixels.
    pub translation_x: f32,
    pub translation_y: f32,
    pub transform_origin: TransformOrigin,
    /// Clips the drawing to the node's bounds.
    pub clip: bool,
}

impl Default for GraphicsLayerScope {
    fn default() -> GraphicsLayerScope {
        GraphicsLayerScope {
            alpha: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_z: 0.0,
            translation_x: 0.0,
            translation_y: 0.0,
            transform_origin: TransformOrigin::CENTER,
            clip: false,
        }
    }
}

#[derive(Clone)]
enum Layer {
    Fixed(GraphicsLayerScope),
    Block(Rc<dyn Fn(&mut GraphicsLayerScope)>),
}

struct GraphicsLayerElement(Layer);

pub(crate) struct GraphicsLayerNode {
    layer: Layer,
    /// Saves `draw` made, for `draw_after` to restore.
    saves: u8,
}

impl GraphicsLayerNode {
    /// Its properties: its block's, run now (reading state the caller observes).
    pub(crate) fn scope(&self) -> GraphicsLayerScope {
        match &self.layer {
            Layer::Fixed(layer) => *layer,
            Layer::Block(block) => {
                let mut layer = GraphicsLayerScope::default();
                block(&mut layer);
                layer
            }
        }
    }
}

/// A graphics layer's properties as a repaint boundary's group carries them
/// ([`DrawGroup::layer`](crate::DrawGroup)), as a render node's: in its group's coordinates.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LayerProperties {
    /// In 0..=1.
    pub alpha: f32,
    /// Where it scales and rotates around.
    pub pivot: Offset,
    pub scale: (f32, f32),
    /// Degrees, clockwise.
    pub rotation: f32,
    pub translation: Offset,
    /// The bounds it clips its content to (after its transform), if it clips.
    pub clip: Option<Rect>,
}

impl LayerProperties {
    /// `layer`'s properties for a layer at `origin` in its group, of `size`.
    pub(crate) fn new(layer: &GraphicsLayerScope, origin: Offset, size: Size) -> LayerProperties {
        let pivot = Offset::new(
            size.width * layer.transform_origin.pivot_fraction_x,
            size.height * layer.transform_origin.pivot_fraction_y,
        );
        LayerProperties {
            alpha: layer.alpha.clamp(0.0, 1.0),
            pivot: pivot + origin,
            scale: (layer.scale_x, layer.scale_y),
            rotation: layer.rotation_z,
            translation: Offset::new(layer.translation_x, layer.translation_y),
            clip: layer.clip.then(|| {
                Rect::new(origin.x, origin.y, origin.x + size.width, origin.y + size.height)
            }),
        }
    }

    fn transforms(&self) -> bool {
        self.scale != (1.0, 1.0) || self.rotation != 0.0 || self.translation != Offset::ZERO
    }

    /// Records the operations applying it to what follows (a group's `layer_start..start`):
    /// returns how many restores end it.
    pub(crate) fn push_operations(&self, list: &mut DisplayList) -> u8 {
        let mut saves = 0;
        let transforms = self.transforms();
        if transforms || self.clip.is_some() {
            list.push(DrawOp::Save);
            saves += 1;
        }
        if transforms {
            list.push(DrawOp::Transform {
                pivot: self.pivot,
                scale: self.scale,
                rotation: self.rotation,
                translation: self.translation,
            });
        }
        if let Some(clip) = self.clip {
            list.push(DrawOp::ClipRect(clip));
        }
        if self.alpha < 1.0 {
            list.push(DrawOp::SaveLayerAlpha(self.alpha));
            saves += 1;
        }
        saves
    }
}

impl ModifierNodeElement for GraphicsLayerElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(GraphicsLayerNode { layer: self.0.clone(), saves: 0 })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<GraphicsLayerNode>() {
            node.layer = self.0.clone();
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<GraphicsLayerElement>().is_some_and(|other| {
            match (&self.0, &other.0) {
                (Layer::Fixed(a), Layer::Fixed(b)) => a == b,
                (Layer::Block(a), Layer::Block(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
        })
    }

    fn name(&self) -> &'static str {
        "graphics_layer"
    }
}

impl ModifierNode for GraphicsLayerNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    /// A layer, as Compose's (a render node on Android).
    fn is_repaint_boundary(&self) -> bool {
        true
    }
}

/// `Modifier.repaint_boundary`'s element.
struct RepaintBoundaryElement;

struct RepaintBoundaryNode;

impl crate::modifier::ModifierNodeElement for RepaintBoundaryElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(RepaintBoundaryNode)
    }
    fn update(&self, _node: &mut dyn ModifierNode) {}
    fn same(&self, other: &dyn crate::modifier::ModifierNodeElement) -> bool {
        other.downcast_ref::<RepaintBoundaryElement>().is_some()
    }
    fn name(&self) -> &'static str {
        "repaint_boundary"
    }
}

impl ModifierNode for RepaintBoundaryNode {
    fn is_repaint_boundary(&self) -> bool {
        true
    }
}

impl Modifier {
    /// Makes what this node draws a group a renderer can keep while it doesn't change (a
    /// render node on Android), as a Compose `graphicsLayer { }` without properties: for
    /// content that often stays the same while what's around it changes (lazy items).
    pub fn repaint_boundary(self) -> Modifier {
        self.then_custom(RepaintBoundaryElement)
    }
}

impl DrawModifierNode for GraphicsLayerNode {
    /// A layer its node draws before applies its properties here, in its node's content (its
    /// block's reads draw the node again); otherwise its group does (render.rs), and this and
    /// `draw_after` aren't called.
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let properties = LayerProperties::new(&self.scope(), scope.origin, scope.size);
        self.saves = properties.push_operations(scope.list);
    }

    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        for _ in 0..std::mem::take(&mut self.saves) {
            scope.restore();
        }
    }
}

impl Modifier {
    /// Applies `block`'s layer properties to the drawing, as `Modifier.graphicsLayer { … }`.
    /// The block runs while drawing: state it reads changes the layer's properties only (or, after
    /// a drawing modifier, draws its node again). The block isn't
    /// memoized: build the modifier in a `remember` (or pass fixed values to
    /// [`graphics_layer_with`](Modifier::graphics_layer_with)) so a callee given it can skip.
    pub fn graphics_layer(self, block: impl Fn(&mut GraphicsLayerScope) + 'static) -> Modifier {
        self.then_custom(GraphicsLayerElement(Layer::Block(Rc::new(block))))
    }

    /// Fixed layer properties, as `Modifier.graphicsLayer(scaleX = …, alpha = …)`.
    pub fn graphics_layer_with(self, layer: GraphicsLayerScope) -> Modifier {
        self.then_custom(GraphicsLayerElement(Layer::Fixed(layer)))
    }

    /// Draws the content with `alpha` opacity, as `Modifier.alpha`.
    pub fn alpha(self, alpha: f32) -> Modifier {
        self.graphics_layer_with(GraphicsLayerScope { alpha, ..GraphicsLayerScope::default() })
    }

    /// Scales the drawing around its center, as `Modifier.scale`.
    pub fn scale(self, scale: f32) -> Modifier {
        self.graphics_layer_with(GraphicsLayerScope {
            scale_x: scale,
            scale_y: scale,
            ..GraphicsLayerScope::default()
        })
    }

    /// Rotates the drawing around its center (degrees, clockwise), as `Modifier.rotate`.
    pub fn rotate(self, degrees: f32) -> Modifier {
        self.graphics_layer_with(GraphicsLayerScope {
            rotation_z: degrees,
            ..GraphicsLayerScope::default()
        })
    }
}
