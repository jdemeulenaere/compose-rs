//! Shadows, as Compose's `Modifier.shadow`: an elevation's ambient and spot shadows under a
//! shape, drawn by the backend from a light above the window (Skia's shadow utilities, as
//! Compose Desktop's).

use crate::draw::DrawScope;
use crate::graphics::{Color, Shape};
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::unit::Dp;

/// The shadows' default color, as Compose's `DefaultShadowColor`.
pub const DEFAULT_SHADOW_COLOR: Color = Color::BLACK;

impl Modifier {
    /// Draws the shadow of `shape` raised by `elevation` behind the content, then clips the
    /// content to `shape` if `clip`, as `Modifier.shadow(elevation, shape, clip)` (which clips
    /// by default when there is an elevation).
    pub fn shadow(self, elevation: Dp, shape: Shape, clip: bool) -> Modifier {
        self.shadow_colored(elevation, shape, clip, DEFAULT_SHADOW_COLOR, DEFAULT_SHADOW_COLOR)
    }

    /// `shadow` with its ambient and spot colors, as `Modifier.shadow(…, ambientColor,
    /// spotColor)`.
    pub fn shadow_colored(
        self,
        elevation: Dp,
        shape: Shape,
        clip: bool,
        ambient_color: Color,
        spot_color: Color,
    ) -> Modifier {
        let modifier = if elevation.0 > 0.0 {
            self.then_custom(ShadowElement { elevation, shape, ambient_color, spot_color })
        } else {
            self
        };
        if clip { modifier.clip(shape) } else { modifier }
    }
}

struct ShadowElement {
    elevation: Dp,
    shape: Shape,
    ambient_color: Color,
    spot_color: Color,
}

struct ShadowNode {
    elevation: Dp,
    shape: Shape,
    ambient_color: Color,
    spot_color: Color,
}

impl ModifierNodeElement for ShadowElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(ShadowNode {
            elevation: self.elevation,
            shape: self.shape,
            ambient_color: self.ambient_color,
            spot_color: self.spot_color,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<ShadowNode>() {
            node.elevation = self.elevation;
            node.shape = self.shape;
            node.ambient_color = self.ambient_color;
            node.spot_color = self.spot_color;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<ShadowElement>().is_some_and(|other| {
            other.elevation == self.elevation
                && other.shape == self.shape
                && other.ambient_color == self.ambient_color
                && other.spot_color == self.spot_color
        })
    }
    fn name(&self) -> &'static str {
        "shadow"
    }
}

impl ModifierNode for ShadowNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for ShadowNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let outline =
            self.shape.outline_in(scope.size(), &scope.density(), scope.layout_direction());
        let elevation = scope.density().to_px(self.elevation);
        scope.draw_shadow(outline, elevation, self.ambient_color, self.spot_color);
    }
}
