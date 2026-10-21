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
}

impl DrawScope<'_> {
    pub fn size(&self) -> Size {
        self.size
    }

    pub fn density(&self) -> Density {
        self.density
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
        let outline = shape.outline(self.size, &self.density);
        self.draw_outline(outline, Paint::fill(color));
    }

    pub fn draw_outline(&mut self, outline: Outline, paint: Paint) {
        match outline {
            Outline::Rectangle(rect) => self.draw_rect(rect, paint),
            Outline::Rounded(rect, radii) => self.draw_round_rect(rect, radii, paint),
        }
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
