//! Replaying display lists onto a Skia canvas (docs/07-platforms.md, "The rendering interface").

use compose_ui::{
    Color, Paint, PaintStyle, PathCommand, PathFillType, Rect,
    platform::{Boundary, DisplayList, DrawGroup, DrawOp, Renderer, TextMeasurer},
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

/// `paint` for Skia, its shader (from `shaders`) moved by `local` (the inverse of a transform
/// the operation applies before drawing, so the gradient stays in the list's coordinates).
fn skia_paint_with(
    paint: &Paint,
    shaders: &[compose_ui::platform::Shader],
    local: Option<&skia_safe::Matrix>,
) -> skia_safe::Paint {
    let mut skia = skia_paint(paint);
    if let Some(shader) = paint.shader().and_then(|index| shaders.get(index.0 as usize))
        && let Some(skia_shader) = skia_shader(shader, local)
    {
        // The shader paints; the paint's alpha is the brush's.
        skia.set_color(skia_safe::Color::BLACK);
        skia.set_alpha_f(shader.alpha());
        skia.set_shader(skia_shader);
    }
    skia
}

fn skia_shader(
    shader: &compose_ui::platform::Shader,
    local: Option<&skia_safe::Matrix>,
) -> Option<skia_safe::Shader> {
    use compose_ui::platform::Shader;
    use skia_safe::gradient::{Colors, Gradient, Interpolation, shaders};
    let gradient = shader.gradient();
    let mut colors: Vec<skia_safe::Color4f> = gradient
        .colors
        .iter()
        .map(|color| skia_safe::Color4f::from(skia_safe::Color::new(color.argb())))
        .collect();
    // A gradient needs two colors: one is a solid color.
    if colors.len() == 1 {
        colors.push(colors[0]);
    }
    if colors.is_empty() {
        return None;
    }
    let stops = gradient.stops.as_deref().filter(|stops| stops.len() == colors.len());
    let mode = match gradient.tile_mode {
        compose_ui::TileMode::Clamp => skia_safe::TileMode::Clamp,
        compose_ui::TileMode::Repeated => skia_safe::TileMode::Repeat,
        compose_ui::TileMode::Mirror => skia_safe::TileMode::Mirror,
        compose_ui::TileMode::Decal => skia_safe::TileMode::Decal,
    };
    // Interpolated premultiplied, as Android's gradients (HWUI builds them with
    // `kInterpolateColorsInPremul_Flag`) and Skiko's (`GradientStyle.DEFAULT`), D210.
    let interpolation = Interpolation {
        in_premul: skia_safe::gradient::interpolation::InPremul::Yes,
        ..Interpolation::default()
    };
    let gradient = Gradient::new(Colors::new(&colors, stops, mode, None), interpolation);
    match shader {
        Shader::LinearGradient { from, to, .. } => shaders::linear_gradient(
            (Point::new(from.x, from.y), Point::new(to.x, to.y)),
            &gradient,
            local,
        ),
        Shader::RadialGradient { center, radius, .. } => {
            shaders::radial_gradient((Point::new(center.x, center.y), *radius), &gradient, local)
        }
        Shader::SweepGradient { center, .. } => {
            shaders::sweep_gradient(Point::new(center.x, center.y), (0.0, 360.0), &gradient, local)
        }
    }
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
            skia.set_stroke_cap(match paint.cap {
                compose_ui::StrokeCap::Butt => paint::Cap::Butt,
                compose_ui::StrokeCap::Round => paint::Cap::Round,
                compose_ui::StrokeCap::Square => paint::Cap::Square,
            });
        }
    }
    skia
}

/// Draws `list` onto `canvas`, its paragraphs from `text_measurer` (a `SkiaTextMeasurer`; other
/// measurers' paragraphs are skipped). Each group's content is drawn from the recording its
/// boundary keeps, by reference (D270), its layer's operations around it.
pub fn draw_display_list(canvas: &Canvas, list: &DisplayList, text_measurer: &dyn TextMeasurer) {
    let measurer = (text_measurer as &dyn std::any::Any).downcast_ref::<SkiaTextMeasurer>();
    forget_dropped_images();
    let mut walk = Walk { canvas, groups: &list.groups, next: 0, measurer };
    walk.recording(list);
}

/// One walk of a display list's recordings.
struct Walk<'a> {
    canvas: &'a Canvas,
    /// The frame's groups, the one at `next` drawn at the next boundary.
    groups: &'a [DrawGroup],
    next: usize,
    measurer: Option<&'a SkiaTextMeasurer>,
}

impl Walk<'_> {
    /// Draws `recording` (the frame, or a group's content), each group where its boundary is.
    /// Its saves and restores are balanced against each other only: an unmatched restore is
    /// ignored, and unmatched saves are undone at its end, so the canvas is left as found.
    fn recording(&mut self, recording: &DisplayList) {
        let start = self.canvas.save_count();
        let mut boundaries = recording.boundaries.iter().peekable();
        for (index, op) in recording.ops.iter().enumerate() {
            while let Some(boundary) = boundaries.next_if(|boundary| boundary.at <= index) {
                self.group(boundary);
            }
            draw_op(self.canvas, op, &recording.shaders, self.measurer, start);
        }
        for boundary in boundaries {
            self.group(boundary);
        }
        self.canvas.restore_to_count(start);
    }

    /// Draws the next group, `boundary`'s: its layer's operations, then its content in a save
    /// of its own (as a render node: a clip or a move it leaves unbalanced stays in it).
    fn group(&mut self, boundary: &Boundary) {
        let group = self.groups.get(self.next);
        debug_assert!(
            group.is_some_and(|group| group.key == boundary.key),
            "the frame's groups don't match its boundaries"
        );
        let Some(group) = group else { return };
        self.next += 1;
        if group.empty {
            self.next += group.descendants;
            return;
        }
        let start = self.canvas.save_count();
        if let Some(layer) = group.layer {
            for op in layer.operations() {
                draw_op(self.canvas, &op, &[], self.measurer, start);
            }
        }
        self.canvas.save();
        self.recording(&group.content);
        self.canvas.restore_to_count(start);
    }
}

/// Draws `op` onto `canvas`, its paint's shader from `shaders`; a restore below `start` (the
/// save count its recording started at) is ignored.
fn draw_op(
    canvas: &Canvas,
    op: &DrawOp,
    shaders: &[compose_ui::platform::Shader],
    measurer: Option<&SkiaTextMeasurer>,
    start: usize,
) {
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
            canvas.draw_rect(rect(bounds), &skia_paint_with(paint, shaders, None));
        }
        DrawOp::DrawRoundRect(bounds, radii, paint) => {
            canvas.draw_rrect(round_rect(bounds, radii), &skia_paint_with(paint, shaders, None));
        }
        DrawOp::DrawLine(from, to, paint) => {
            canvas.draw_line(
                Point::new(from.x, from.y),
                Point::new(to.x, to.y),
                &skia_paint_with(paint, shaders, None),
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
            // The gradient stays where it was given, outside the path's scale and offset.
            let local = (paint.shader().is_some() && scale.0 != 0.0 && scale.1 != 0.0).then(|| {
                let mut local = skia_safe::Matrix::scale((1.0 / scale.0, 1.0 / scale.1));
                local.pre_translate((-offset.x, -offset.y));
                local
            });
            canvas.draw_path(&skia, &skia_paint_with(paint, shaders, local.as_ref()));
            canvas.restore();
        }
        DrawOp::ClipPath { path, offset } => {
            let skia = skia_path(path).make_offset((offset.x, offset.y));
            canvas.clip_path(&skia, ClipOp::Intersect, true);
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
        DrawOp::DrawArc { oval, start_angle, sweep_angle, use_center, paint } => {
            canvas.draw_arc(
                rect(oval),
                *start_angle,
                *sweep_angle,
                *use_center,
                &skia_paint_with(paint, shaders, None),
            );
        }
        DrawOp::DrawShadow { outline, elevation, light, light_radius, ambient, spot } => {
            let mut path = skia_safe::PathBuilder::new();
            match outline {
                compose_ui::Outline::Rectangle(bounds) => {
                    path.add_rect(rect(bounds), None, None);
                }
                compose_ui::Outline::Rounded(bounds, radii) => {
                    path.add_rrect(round_rect(bounds, radii), None, None);
                }
            }
            // As Compose Desktop: an opaque occluder, the light in the window's pixels.
            skia_safe::utils::shadow_utils::draw_shadow(
                canvas,
                &path.detach(),
                skia_safe::Point3::new(0.0, 0.0, *elevation),
                skia_safe::Point3::new(light.0, light.1, light.2),
                *light_radius,
                skia_safe::Color::new(ambient.argb()),
                skia_safe::Color::new(spot.argb()),
                None,
            );
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

/// The platform image decoder on desktop (`compose_ui::platform::set_image_decoder`): Skia's codecs, as
/// [`decode_image`].
pub struct SkiaImageDecoder;

impl compose_ui::platform::ImageDecoder for SkiaImageDecoder {
    fn decode(&self, bytes: &[u8]) -> Option<compose_ui::ImageBitmap> {
        decode_image(bytes)
    }
}
