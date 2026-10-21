//! The display list: what a draw pass records and a renderer replays (docs/07-platforms.md).
//! A subset of the `Canvas` API that both Skia and Android's `Canvas` support directly.

use std::fmt::Write as _;

use crate::graphics::{Outline, Paint};
use crate::text::ParagraphHandle;
use crate::unit::{Offset, Rect};

#[derive(Clone, PartialEq, Debug)]
pub enum DrawOp {
    Save,
    Restore,
    Translate(Offset),
    ClipRect(Rect),
    /// Corner radii: top left, top right, bottom right, bottom left.
    ClipRoundRect(Rect, [f32; 4]),
    DrawRect(Rect, Paint),
    DrawRoundRect(Rect, [f32; 4], Paint),
    /// A line from the first point to the second, stroked with the paint's width (round caps).
    DrawLine(Offset, Offset, Paint),
    /// A paragraph laid out by the text measurer, drawn by handle at an offset.
    DrawParagraph(ParagraphHandle, Offset),
    /// Saves, then draws what follows into a layer composited with `alpha` at the matching
    /// restore, as a graphics layer's alpha.
    SaveLayerAlpha(f32),
    /// Transforms what follows (until the matching restore of an earlier save): scaled by
    /// `scale` and rotated by `rotation` degrees (clockwise) around `pivot`, then moved by
    /// `translation`, as a graphics layer.
    Transform {
        pivot: Offset,
        scale: (f32, f32),
        rotation: f32,
        translation: Offset,
    },
}

/// Replays display lists onto a surface: Skia on desktop, Android's `Canvas` (docs/07-platforms.md).
/// Paragraphs are drawn by the text measurer that laid them out, which the renderer receives and
/// downcasts to its own type.
pub trait Renderer {
    /// Draws `list` over `background`, scaled by `scale` (device pixels per layout pixel, for
    /// HiDPI surfaces whose layout runs at density 1).
    fn render(
        &mut self,
        list: &DisplayList,
        background: crate::graphics::Color,
        scale: f32,
        text_measurer: &dyn crate::text::TextMeasurer,
    );
}

/// The operations of one frame. Cleared and refilled each draw pass, keeping its capacity.
#[derive(Clone, Default, Debug)]
pub struct DisplayList {
    pub ops: Vec<DrawOp>,
}

impl DisplayList {
    pub fn clear(&mut self) {
        self.ops.clear();
    }

    pub fn push(&mut self, op: DrawOp) {
        self.ops.push(op);
    }

    pub fn draw_outline(&mut self, outline: Outline, paint: Paint) {
        match outline {
            Outline::Rectangle(rect) => self.push(DrawOp::DrawRect(rect, paint)),
            Outline::Rounded(rect, radii) => self.push(DrawOp::DrawRoundRect(rect, radii, paint)),
        }
    }

    pub fn clip_outline(&mut self, outline: Outline) {
        match outline {
            Outline::Rectangle(rect) => self.push(DrawOp::ClipRect(rect)),
            Outline::Rounded(rect, radii) => self.push(DrawOp::ClipRoundRect(rect, radii)),
        }
    }

    /// A text dump, one operation per line, indented by save depth: for golden tests.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        for op in &self.ops {
            if matches!(op, DrawOp::Restore) {
                depth = depth.saturating_sub(1);
            }
            let _ = write!(out, "{}", "  ".repeat(depth));
            let _ = match op {
                DrawOp::Save => writeln!(out, "save"),
                DrawOp::Restore => writeln!(out, "restore"),
                DrawOp::Translate(offset) => writeln!(out, "translate {} {}", offset.x, offset.y),
                DrawOp::ClipRect(rect) => writeln!(out, "clip {}", rect_text(rect)),
                DrawOp::ClipRoundRect(rect, radii) => {
                    writeln!(out, "clip {} radii {:?}", rect_text(rect), radii)
                }
                DrawOp::DrawRect(rect, paint) => {
                    writeln!(out, "rect {} {}", rect_text(rect), paint_text(paint))
                }
                DrawOp::DrawRoundRect(rect, radii, paint) => {
                    writeln!(
                        out,
                        "round rect {} radii {:?} {}",
                        rect_text(rect),
                        radii,
                        paint_text(paint)
                    )
                }
                DrawOp::DrawLine(from, to, paint) => {
                    writeln!(
                        out,
                        "line {},{} to {},{} {}",
                        from.x,
                        from.y,
                        to.x,
                        to.y,
                        paint_text(paint)
                    )
                }
                DrawOp::DrawParagraph(handle, offset) => {
                    writeln!(out, "paragraph {} at {} {}", handle.0, offset.x, offset.y)
                }
                DrawOp::SaveLayerAlpha(alpha) => writeln!(out, "save layer alpha {alpha}"),
                DrawOp::Transform { pivot, scale, rotation, translation } => writeln!(
                    out,
                    "transform pivot {},{} scale {}x{} rotate {} translate {},{}",
                    pivot.x, pivot.y, scale.0, scale.1, rotation, translation.x, translation.y
                ),
            };
            if matches!(op, DrawOp::Save | DrawOp::SaveLayerAlpha(_)) {
                depth += 1;
            }
        }
        out
    }
}

fn rect_text(rect: &Rect) -> String {
    format!("{},{} {}x{}", rect.left, rect.top, rect.width(), rect.height())
}

fn paint_text(paint: &Paint) -> String {
    match paint.style {
        crate::graphics::PaintStyle::Fill => format!("{:?}", paint.color),
        crate::graphics::PaintStyle::Stroke(width) => format!("{:?} stroke {width}", paint.color),
    }
}
