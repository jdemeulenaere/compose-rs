//! The draw scope modifier nodes record into (docs/06-ui.md, "Drawing").

use crate::display_list::{DisplayList, DrawOp};
use crate::graphics::{Color, Outline, Paint, Shape};
use crate::text::ParagraphHandle;
use crate::unit::{Density, Offset, Rect, Size};

/// What a draw node draws with, as `DrawScope`: its size, in pixels, and the display list.
/// Coordinates are the node's own: (0, 0) is its top left.
pub struct DrawScope<'a> {
    pub(crate) list: &'a mut DisplayList,
    pub(crate) origin: Offset,
    pub(crate) size: Size,
    pub(crate) density: Density,
    pub(crate) layout_direction: crate::alignment::LayoutDirection,
}

impl DrawScope<'_> {
    pub fn size(&self) -> Size {
        self.size
    }

    pub fn density(&self) -> Density {
        self.density
    }

    /// The node's layout direction, as `DrawScope.layoutDirection`: what's drawn from the start
    /// (a slider's track, a switch's thumb) is mirrored in right-to-left.
    pub fn layout_direction(&self) -> crate::alignment::LayoutDirection {
        self.layout_direction
    }

    fn to_list(&self, rect: Rect) -> Rect {
        Rect::new(
            rect.left + self.origin.x,
            rect.top + self.origin.y,
            rect.right + self.origin.x,
            rect.bottom + self.origin.y,
        )
    }

    /// The whole node.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.size.width, self.size.height)
    }

    pub fn draw_rect(&mut self, rect: Rect, paint: Paint) {
        let rect = self.to_list(rect);
        self.list.push(DrawOp::DrawRect(rect, paint));
    }

    pub fn draw_round_rect(&mut self, rect: Rect, radii: [f32; 4], paint: Paint) {
        let rect = self.to_list(rect);
        self.list.push(DrawOp::DrawRoundRect(rect, radii, paint));
    }

    /// Fills `shape` over the whole node.
    pub fn draw_shape(&mut self, shape: &Shape, color: Color) {
        let outline = shape.outline_in(self.size, &self.density, self.layout_direction());
        self.draw_outline(outline, Paint::fill(color));
    }

    pub fn draw_outline(&mut self, outline: Outline, paint: Paint) {
        match outline {
            Outline::Rectangle(rect) => self.draw_rect(rect, paint),
            Outline::Rounded(rect, radii) => self.draw_round_rect(rect, radii, paint),
        }
    }

    /// Draws an arc of the oval in `oval` from `start_angle` degrees (0 at 3 o'clock,
    /// clockwise) over `sweep_angle`, as `drawArc(useCenter = false)`.
    pub fn draw_arc(&mut self, oval: Rect, start_angle: f32, sweep_angle: f32, paint: Paint) {
        let oval = self.to_list(oval);
        self.list.push(DrawOp::DrawArc {
            oval,
            start_angle,
            sweep_angle,
            use_center: false,
            paint,
        });
    }

    pub fn draw_line(&mut self, from: Offset, to: Offset, paint: Paint) {
        let (from, to) = (from + self.origin, to + self.origin);
        self.list.push(DrawOp::DrawLine(from, to, paint));
    }

    pub fn draw_paragraph(&mut self, paragraph: ParagraphHandle, position: Offset) {
        let position = position + self.origin;
        self.list.push(DrawOp::DrawParagraph(paragraph, position));
    }

    /// Saves the clip, to restore with [`restore`](DrawScope::restore).
    pub fn save(&mut self) {
        self.list.push(DrawOp::Save);
    }

    pub fn restore(&mut self) {
        self.list.push(DrawOp::Restore);
    }

    /// Draws the part `src` of `image` (in its pixels) into `dst` (in the node's coordinates), at
    /// `alpha`.
    pub fn draw_image(
        &mut self,
        image: &crate::image::ImageBitmap,
        src: Rect,
        dst: Rect,
        alpha: f32,
    ) {
        let dst = self.to_list(dst);
        self.list.push(DrawOp::DrawImage { image: image.clone(), src, dst, alpha });
    }

    /// Draws the shadows of `outline` (in the node's coordinates) raised by `elevation` pixels,
    /// in `ambient_color` and `spot_color` (their alphas set to Compose's), as a graphics
    /// layer's `shadowElevation` does: from a light 300 dp above the window's top, 600 dp up,
    /// 800 dp wide, as Compose Desktop's.
    pub fn draw_shadow(
        &mut self,
        outline: Outline,
        elevation: f32,
        ambient_color: Color,
        spot_color: Color,
    ) {
        if elevation <= 0.0 {
            return;
        }
        let outline = match outline {
            Outline::Rectangle(rect) => Outline::Rectangle(self.to_list(rect)),
            Outline::Rounded(rect, radii) => Outline::Rounded(self.to_list(rect), radii),
        };
        let density = self.density;
        let px = |dp: f32| density.to_px(crate::unit::Dp(dp));
        self.list.push(DrawOp::DrawShadow {
            outline,
            elevation,
            light: (0.0, -px(300.0), px(600.0)),
            light_radius: px(800.0),
            ambient: ambient_color.with_alpha(0.039),
            spot: spot_color.with_alpha(0.19),
        });
    }

    /// Draws `path` (in the node's coordinates).
    pub fn draw_path(&mut self, path: &std::sync::Arc<crate::path::Path>, paint: Paint) {
        self.draw_path_scaled(path, Offset::ZERO, (1.0, 1.0), paint);
    }

    /// Draws `path` scaled by `scale`, then moved by `offset` (in the node's coordinates): a
    /// vector image's viewport fitted to its size.
    pub fn draw_path_scaled(
        &mut self,
        path: &std::sync::Arc<crate::path::Path>,
        offset: Offset,
        scale: (f32, f32),
        paint: Paint,
    ) {
        let offset = offset + self.origin;
        self.list.push(DrawOp::DrawPath { path: path.clone(), offset, scale, paint });
    }

    /// Saves, and draws what follows into a layer composited with `alpha` at the next restore.
    pub fn save_layer_alpha(&mut self, alpha: f32) {
        self.list.push(DrawOp::SaveLayerAlpha(alpha));
    }

    /// Transforms what follows until the restore of a save made before: scaled and rotated
    /// (degrees, clockwise) around `pivot` (in the node's coordinates), then moved by
    /// `translation`.
    pub fn transform(
        &mut self,
        pivot: Offset,
        scale: (f32, f32),
        rotation: f32,
        translation: Offset,
    ) {
        let pivot = pivot + self.origin;
        self.list.push(DrawOp::Transform { pivot, scale, rotation, translation });
    }

    pub fn clip_outline(&mut self, outline: Outline) {
        match outline {
            Outline::Rectangle(rect) => {
                let rect = self.to_list(rect);
                self.list.push(DrawOp::ClipRect(rect));
            }
            Outline::Rounded(rect, radii) => {
                let rect = self.to_list(rect);
                self.list.push(DrawOp::ClipRoundRect(rect, radii));
            }
        }
    }
}
