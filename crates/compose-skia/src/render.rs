//! Replaying display lists onto a Skia canvas (docs/07-platforms.md, "The rendering interface").

use compose_ui::{
    Color, DisplayList, DrawOp, Paint, PaintStyle, PathCommand, PathFillType, Rect, Renderer,
    TextMeasurer,
};
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
    forget_dropped_images();
    // A list's saves and restores are balanced against each other only: an unmatched restore
    // is ignored, and unmatched saves are undone at the end, so the canvas is left as found.
    let start = canvas.save_count();
    for op in &list.ops {
        match op {
            DrawOp::Save => {
                canvas.save();
            }
            DrawOp::Restore => {
                if canvas.save_count() > start {
                    canvas.restore();
                }
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
            DrawOp::DrawPath { path, offset, scale, paint } => {
                let skia = skia_path(path);
                canvas.save();
                canvas.translate((offset.x, offset.y));
                canvas.scale((scale.0, scale.1));
                canvas.draw_path(&skia, &skia_paint(paint));
                canvas.restore();
            }
            DrawOp::DrawImage { image, src, dst, alpha } => {
                if let Some(skia) = skia_image(image) {
                    let mut paint = skia_safe::Paint::default();
                    paint.set_alpha_f(alpha.clamp(0.0, 1.0));
                    let sampling = skia_safe::SamplingOptions::new(
                        skia_safe::FilterMode::Linear,
                        skia_safe::MipmapMode::None,
                    );
                    canvas.draw_image_rect_with_sampling_options(
                        &skia,
                        Some((&rect(src), skia_safe::canvas::SrcRectConstraint::Strict)),
                        rect(dst),
                        sampling,
                        &paint,
                    );
                }
            }
            DrawOp::SaveLayerAlpha(alpha) => {
                canvas.save_layer_alpha_f(None, alpha.clamp(0.0, 1.0));
            }
            DrawOp::Transform { pivot, scale, rotation, translation } => {
                canvas.translate((pivot.x + translation.x, pivot.y + translation.y));
                canvas.rotate(*rotation, None);
                canvas.scale((scale.0, scale.1));
                canvas.translate((-pivot.x, -pivot.y));
            }
        }
    }
    canvas.restore_to_count(start);
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

    /// Copies the pixels into `target` (row by row, as `pixels`), allocating nothing: what a
    /// window presents.
    pub fn copy_pixels_to(&mut self, target: &mut [u32]) {
        let Some(pixmap) = self.surface.peek_pixels() else { return };
        let Some(bytes) = pixmap.bytes() else { return };
        for (pixel, chunk) in target.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *pixel = u32::from_ne_bytes(*chunk);
        }
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
        let count = canvas.save();
        canvas.scale((scale, scale));
        draw_display_list(canvas, list, text_measurer);
        canvas.restore_to_count(count);
    }
}

thread_local! {
    /// Skia's path of each path drawn, by its `Arc` (kept while the path lives): an icon drawn
    /// every frame is converted once.
    static PATHS: std::cell::RefCell<std::collections::HashMap<usize, (std::sync::Weak<compose_ui::Path>, skia_safe::Path)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Skia's path for `path`, converted once per path (while it lives).
fn skia_path(path: &std::sync::Arc<compose_ui::Path>) -> skia_safe::Path {
    let address = std::sync::Arc::as_ptr(path) as usize;
    PATHS.with(|paths| {
        let mut paths = paths.borrow_mut();
        if let Some((alive, skia)) = paths.get(&address)
            && alive.upgrade().is_some_and(|alive| std::sync::Arc::ptr_eq(&alive, path))
        {
            return skia.clone();
        }
        // Paths that are gone are forgotten as new ones come.
        if paths.len() > 256 {
            paths.retain(|_, (alive, _)| alive.strong_count() > 0);
        }
        let skia = build_skia_path(path);
        paths.insert(address, (std::sync::Arc::downgrade(path), skia.clone()));
        skia
    })
}

fn build_skia_path(path: &compose_ui::Path) -> skia_safe::Path {
    let mut skia = skia_safe::PathBuilder::new();
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo(point) => {
                skia.move_to((point.x, point.y));
            }
            PathCommand::LineTo(point) => {
                skia.line_to((point.x, point.y));
            }
            PathCommand::QuadTo(control, end) => {
                skia.quad_to((control.x, control.y), (end.x, end.y));
            }
            PathCommand::CubicTo(first, second, end) => {
                skia.cubic_to((first.x, first.y), (second.x, second.y), (end.x, end.y));
            }
            PathCommand::Close => {
                skia.close();
            }
        }
    }
    skia.set_fill_type(match path.fill_type {
        PathFillType::NonZero => skia_safe::PathFillType::Winding,
        PathFillType::EvenOdd => skia_safe::PathFillType::EvenOdd,
    });
    skia.detach()
}

/// Skia's images by bitmap address, each with its bitmap's pixels to tell when it's gone.
type ImageCache = std::collections::HashMap<usize, (std::sync::Weak<[u8]>, skia_safe::Image)>;

thread_local! {
    /// Skia's image of each bitmap drawn, by its pixels (kept while the bitmap lives).
    static IMAGES: std::cell::RefCell<ImageCache> = std::cell::RefCell::new(ImageCache::new());
}

/// Forgets the images of bitmaps that are gone, so their pixels (copied into Skia) are freed
/// at the next frame rather than kept.
fn forget_dropped_images() {
    IMAGES.with(|images| images.borrow_mut().retain(|_, (alive, _)| alive.strong_count() > 0));
}

/// Skia's image for `image`, made once per bitmap (while it lives).
fn skia_image(image: &compose_ui::ImageBitmap) -> Option<skia_safe::Image> {
    IMAGES.with(|images| {
        let mut images = images.borrow_mut();
        if let Some((alive, skia)) = images.get(&image.id())
            && alive.strong_count() > 0
        {
            return Some(skia.clone());
        }
        let info = skia_safe::ImageInfo::new(
            (image.width() as i32, image.height() as i32),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let data = skia_safe::Data::new_copy(image.pixels());
        let skia = skia_safe::images::raster_from_data(&info, data, image.width() as usize * 4)?;
        images.insert(image.id(), (image.downgrade(), skia.clone()));
        Some(skia)
    })
}

/// Decodes an encoded image (PNG, JPEG, WebP, GIF's first frame, …) with Skia, as the
/// desktop's `decodeToImageBitmap`. `None` if it isn't one Skia reads.
pub fn decode_image(bytes: &[u8]) -> Option<compose_ui::ImageBitmap> {
    let encoded = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(bytes))?;
    let (width, height) = (encoded.width(), encoded.height());
    // Its header's size is trusted only up to 2^28 pixels (a gigabyte decoded).
    if width <= 0 || height <= 0 || (width as u64) * (height as u64) > 1 << 28 {
        return None;
    }
    let info = skia_safe::ImageInfo::new(
        (width, height),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    if !encoded.read_pixels(
        &info,
        &mut pixels,
        width as usize * 4,
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    ) {
        return None;
    }
    Some(compose_ui::ImageBitmap::from_rgba_premultiplied(width as u32, height as u32, pixels))
}
