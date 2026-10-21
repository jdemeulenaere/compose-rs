//! Replaying display lists onto a Skia canvas (docs/07-platforms.md, "The rendering interface").

use compose_ui::{Color, DisplayList, DrawOp, Paint, PaintStyle, Rect, Renderer, TextMeasurer};
use skia_safe::{Canvas, ClipOp, EncodedImageFormat, Point, RRect, Surface, Vector, paint};

use crate::text::SkiaTextMeasurer;

fn rect(rect: &Rect) -> skia_safe::Rect {
    skia_safe::Rect::new(rect.left, rect.top, rect.right, rect.bottom)
}

fn round_rect(bounds: &Rect, radii: &[f32; 4]) -> RRect {
    let radii = radii.map(|radius| Vector::new(radius, radius));
    RRect::new_rect_radii(rect(bounds), &radii)
}

fn skia_paint(paint: &Paint) -> skia_safe::Paint {
    let mut skia = skia_safe::Paint::default();
    skia.set_anti_alias(true);
    skia.set_color(skia_safe::Color::new(paint.color.argb()));
    match paint.style {
        PaintStyle::Fill => {
            skia.set_style(paint::Style::Fill);
        }
        PaintStyle::Stroke(width) => {
            skia.set_style(paint::Style::Stroke);
            skia.set_stroke_width(width);
            skia.set_stroke_cap(paint::Cap::Round);
        }
    }
    skia
}

/// Draws `list` onto `canvas`, its paragraphs from `text_measurer` (a `SkiaTextMeasurer`; other
/// measurers' paragraphs are skipped).
pub fn draw_display_list(canvas: &Canvas, list: &DisplayList, text_measurer: &dyn TextMeasurer) {
    let measurer = (text_measurer as &dyn std::any::Any).downcast_ref::<SkiaTextMeasurer>();
    for op in &list.ops {
        match op {
            DrawOp::Save => {
                canvas.save();
            }
            DrawOp::Restore => {
                canvas.restore();
            }
            DrawOp::Translate(offset) => {
                canvas.translate((offset.x, offset.y));
            }
            DrawOp::ClipRect(bounds) => {
                canvas.clip_rect(rect(bounds), ClipOp::Intersect, true);
            }
            DrawOp::ClipRoundRect(bounds, radii) => {
                canvas.clip_rrect(round_rect(bounds, radii), ClipOp::Intersect, true);
            }
            DrawOp::DrawRect(bounds, paint) => {
                canvas.draw_rect(rect(bounds), &skia_paint(paint));
            }
            DrawOp::DrawRoundRect(bounds, radii, paint) => {
                canvas.draw_rrect(round_rect(bounds, radii), &skia_paint(paint));
            }
            DrawOp::DrawLine(from, to, paint) => {
                canvas.draw_line(
                    Point::new(from.x, from.y),
                    Point::new(to.x, to.y),
                    &skia_paint(paint),
                );
            }
            DrawOp::DrawParagraph(handle, offset) => {
                if let Some(paragraph) = measurer.and_then(|measurer| measurer.paragraph(*handle)) {
                    paragraph.paint(canvas, (offset.x, offset.y));
                }
            }
        }
    }
}

/// A renderer onto a CPU raster surface: for tests, screenshots, and windows presenting pixels.
pub struct RasterRenderer {
    surface: Surface,
}

impl RasterRenderer {
    /// A surface of `width` by `height` device pixels.
    pub fn new(width: i32, height: i32) -> RasterRenderer {
        let surface = skia_safe::surfaces::raster_n32_premul((width.max(1), height.max(1)))
            .expect("compose-skia: a raster surface");
        RasterRenderer { surface }
    }

    pub fn size(&self) -> (i32, i32) {
        (self.surface.width(), self.surface.height())
    }

    /// Resizes the surface (its content is lost).
    pub fn resize(&mut self, width: i32, height: i32) {
        if self.size() != (width, height) {
            *self = RasterRenderer::new(width, height);
        }
    }

    /// The pixels, as premultiplied 32-bit values in the platform's native order (BGRA on
    /// little-endian machines), row by row.
    pub fn pixels(&mut self) -> Vec<u32> {
        let (width, height) = self.size();
        let info = skia_safe::ImageInfo::new_n32_premul((width, height), None);
        let mut bytes = vec![0u8; (width * height * 4) as usize];
        self.surface.read_pixels(&info, &mut bytes, (width * 4) as usize, (0, 0));
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]))
            .collect()
    }

    /// The color of one pixel, as ARGB.
    pub fn pixel(&mut self, x: i32, y: i32) -> Color {
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::BGRA8888,
            skia_safe::AlphaType::Unpremul,
            None,
        );
        let mut bytes = [0u8; 4];
        self.surface.read_pixels(&info, &mut bytes, 4, (x, y));
        Color::from_argb(u32::from_le_bytes(bytes))
    }

    /// The surface as a PNG file.
    pub fn png(&mut self) -> Vec<u8> {
        let image = self.surface.image_snapshot();
        image
            .encode(None, EncodedImageFormat::PNG, 100)
            .map(|data| data.as_bytes().to_vec())
            .unwrap_or_default()
    }

    pub fn canvas(&mut self) -> &Canvas {
        self.surface.canvas()
    }
}

impl Renderer for RasterRenderer {
    fn render(
        &mut self,
        list: &DisplayList,
        background: Color,
        scale: f32,
        text_measurer: &dyn TextMeasurer,
    ) {
        let canvas = self.surface.canvas();
        canvas.clear(skia_safe::Color::new(background.argb()));
        canvas.save();
        canvas.scale((scale, scale));
        draw_display_list(canvas, list, text_measurer);
        canvas.restore();
    }
}
