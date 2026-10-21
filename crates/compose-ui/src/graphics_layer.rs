//! `Modifier.graphics_layer`, as Compose's `graphicsLayer`: alpha, scale, rotation and
//! translation applied to a node's drawing, without layout. Its block runs while drawing, so
//! state it reads (an animation's value) redraws the node without recomposing or measuring it.
//!
//! Hit testing doesn't follow the transform yet: a scaled or translated node takes pointer
//! events at its laid-out bounds.

use std::rc::Rc;

use crate::draw::DrawScope;
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::unit::Offset;

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

struct GraphicsLayerNode {
    layer: Layer,
    /// Saves `draw` made, for `draw_after` to restore.
    saves: u8,
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
}

impl DrawModifierNode for GraphicsLayerNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let layer = match &self.layer {
            Layer::Fixed(layer) => *layer,
            Layer::Block(block) => {
                let mut layer = GraphicsLayerScope::default();
                block(&mut layer);
                layer
            }
        };
        self.saves = 0;
        let transformed = layer.scale_x != 1.0
            || layer.scale_y != 1.0
            || layer.rotation_z != 0.0
            || layer.translation_x != 0.0
            || layer.translation_y != 0.0;
        if transformed || layer.clip {
            scope.save();
            self.saves += 1;
        }
        if transformed {
            let size = scope.size();
            let pivot = Offset::new(
                size.width * layer.transform_origin.pivot_fraction_x,
                size.height * layer.transform_origin.pivot_fraction_y,
            );
            scope.transform(
                pivot,
                (layer.scale_x, layer.scale_y),
                layer.rotation_z,
                Offset::new(layer.translation_x, layer.translation_y),
            );
        }
        if layer.clip {
            let bounds = scope.bounds();
            scope.clip_outline(crate::graphics::Outline::Rectangle(bounds));
        }
        if layer.alpha < 1.0 {
            scope.save_layer_alpha(layer.alpha.max(0.0));
            self.saves += 1;
        }
    }

    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        for _ in 0..std::mem::take(&mut self.saves) {
            scope.restore();
        }
    }
}

impl Modifier {
    /// Applies `block`'s layer properties to the drawing, as `Modifier.graphicsLayer { … }`.
    /// The block runs while drawing: state it reads redraws the node only. The block isn't
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
