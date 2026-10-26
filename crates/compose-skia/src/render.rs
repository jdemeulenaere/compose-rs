//! Replaying display lists onto a Skia canvas (docs/07-platforms.md, "The rendering interface").

use compose_ui::{
    Color, CompositingStrategy, Paint, PaintStyle, PathCommand, PathFillType, PathOperation, Rect,
    platform::{
        Boundary, DisplayList, DrawGroup, DrawOp, Optimization, Optimizations, Renderer,
        TextMeasurer, single_drawing,
    },
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
/// boundary keeps, by reference (D270), its layer's operations around it, as Compose Desktop's
/// render nodes: without compose-rs's optimizations (see [`draw_display_list_with`]).
pub fn draw_display_list(canvas: &Canvas, list: &DisplayList, text_measurer: &dyn TextMeasurer) {
    draw_display_list_with(canvas, list, text_measurer, Optimizations::NONE);
}

/// [`draw_display_list`] with compose-rs's `optimizations` (`SINGLE_DRAWING_FADE`: a layer's
/// alpha under `Auto` over one drawing with a paint fades that paint, without an offscreen
/// buffer).
pub fn draw_display_list_with(
    canvas: &Canvas,
    list: &DisplayList,
    text_measurer: &dyn TextMeasurer,
    optimizations: Optimizations,
) {
    let measurer = (text_measurer as &dyn std::any::Any).downcast_ref::<SkiaTextMeasurer>();
    forget_dropped_images();
    let mut walk =
        Walk { canvas, groups: &list.groups, next: 0, measurer, alpha: 1.0, optimizations };
    walk.recording(list);
}

/// One walk of a display list's recordings.
struct Walk<'a> {
    canvas: &'a Canvas,
    /// The frame's groups, the one at `next` drawn at the next boundary.
    groups: &'a [DrawGroup],
    next: usize,
    measurer: Option<&'a SkiaTextMeasurer>,
    /// What each drawing's alpha is multiplied by: the `ModulateAlpha` layers around it.
    alpha: f32,
    optimizations: Optimizations,
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
            draw_op(self.canvas, op, &recording.shaders, self.measurer, start, self.alpha);
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
        let outer_alpha = self.alpha;
        if let Some(mut layer) = group.layer {
            // One drawing faded by its paint, with the optimization (D521).
            if layer.compositing_strategy == CompositingStrategy::Auto
                && layer.alpha < 1.0
                && self.optimizations.contains(Optimization::SINGLE_DRAWING_FADE)
                && self.groups[self.next..self.next + group.descendants]
                    .iter()
                    .all(|nested| nested.empty)
                && single_drawing(&group.content.ops).is_some()
            {
                layer.compositing_strategy = CompositingStrategy::ModulateAlpha;
            }
            for op in layer.operations() {
                match op {
                    // An offscreen layer inside a modulated one is faded as one composite,
                    // its content drawn at full alpha in it (HWUI multiplies a child node's
                    // layer alpha, not what's inside its layer).
                    DrawOp::SaveLayerAlpha { bounds, alpha } => {
                        let alpha = (alpha * self.alpha).clamp(0.0, 1.0);
                        self.canvas.save_layer_alpha_f(rect(&bounds), alpha);
                        self.alpha = 1.0;
                    }
                    op => draw_op(self.canvas, &op, &[], self.measurer, start, self.alpha),
                }
            }
            if let Some(alpha) = layer.modulated_alpha() {
                self.alpha *= alpha;
            }
        }
        self.canvas.save();
        self.recording(&group.content);
        self.canvas.restore_to_count(start);
        self.alpha = outer_alpha;
    }
}

/// `paint` with its alpha multiplied by `alpha` (a `ModulateAlpha` layer's).
fn faded(mut paint: skia_safe::Paint, alpha: f32) -> skia_safe::Paint {
    if alpha < 1.0 {
        let faded = paint.alpha_f() * alpha;
        paint.set_alpha_f(faded);
    }
    paint
}

/// Draws `op` onto `canvas`, its paint's shader from `shaders`, its alpha multiplied by
/// `alpha` (a drawing a paint's alpha doesn't reach, text and shadows, in a layer of that
/// alpha); a restore below `start` (the save count its recording started at) is ignored.
fn draw_op(
    canvas: &Canvas,
    op: &DrawOp,
    shaders: &[compose_ui::platform::Shader],
    measurer: Option<&SkiaTextMeasurer>,
    start: usize,
    alpha: f32,
) {
    let skia_paint_with = |paint: &Paint,
                           shaders: &[compose_ui::platform::Shader],
                           local: Option<&skia_safe::Matrix>| {
        faded(skia_paint_with(paint, shaders, local), alpha)
    };
    let own_layer =
        alpha < 1.0 && matches!(op, DrawOp::DrawParagraph(..) | DrawOp::DrawShadow { .. });
    if own_layer {
        canvas.save_layer_alpha_f(None, alpha);
    }
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
        DrawOp::DrawImage { image, src, dst, alpha: image_alpha } => {
            let (alpha, outer_alpha) = (*image_alpha, alpha);
            if let Some(skia) = skia_image(image) {
                let mut paint = skia_safe::Paint::default();
                paint.set_alpha_f(alpha.clamp(0.0, 1.0) * outer_alpha);
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
        DrawOp::DrawShadow {
            outline,
            elevation,
            light,
            light_radius,
            ambient,
            spot,
            transparent_occluder,
        } => {
            let mut path = skia_safe::PathBuilder::new();
            match outline {
                compose_ui::Outline::Rectangle(bounds) => {
                    path.add_rect(rect(bounds), None, None);
                }
                compose_ui::Outline::Rounded(bounds, radii) => {
                    path.add_rrect(round_rect(bounds, radii), None, None);
                }
            }
            // As Compose Desktop (Skiko's render node): the light in the window's pixels, the
            // occluder transparent for a faded layer.
            let flags = if *transparent_occluder {
                skia_safe::utils::shadow_utils::ShadowFlags::TRANSPARENT_OCCLUDER
            } else {
                skia_safe::utils::shadow_utils::ShadowFlags::empty()
            };
            skia_safe::utils::shadow_utils::draw_shadow(
                canvas,
                &path.detach(),
                skia_safe::Point3::new(0.0, 0.0, *elevation),
                skia_safe::Point3::new(light.0, light.1, light.2),
                *light_radius,
                skia_safe::Color::new(ambient.argb()),
                skia_safe::Color::new(spot.argb()),
                flags,
            );
        }
        DrawOp::SaveLayerAlpha { bounds, alpha } => {
            canvas.save_layer_alpha_f(rect(bounds), alpha.clamp(0.0, 1.0));
        }
        DrawOp::Transform { pivot, scale, rotation, translation } => {
            canvas.translate((pivot.x + translation.x, pivot.y + translation.y));
            canvas.rotate(*rotation, None);
            canvas.scale((scale.0, scale.1));
            canvas.translate((-pivot.x, -pivot.y));
        }
    }
    if own_layer {
        canvas.restore();
    }
}

/// A renderer onto a CPU raster surface: for tests, screenshots, and windows presenting pixels.
pub struct RasterRenderer {
    surface: Surface,
    /// compose-rs's optimizations it draws with (`set_optimizations`; by default
    /// `Optimizations::defaults()`, the recommended set, as a host's: D610).
    optimizations: Optimizations,
}

impl RasterRenderer {
    /// A surface of `width` by `height` device pixels.
    pub fn new(width: i32, height: i32) -> RasterRenderer {
        let surface = skia_safe::surfaces::raster_n32_premul((width.max(1), height.max(1)))
            .expect("compose-skia: a raster surface");
        RasterRenderer { surface, optimizations: Optimizations::defaults() }
    }

    /// Draws with compose-rs's `optimizations` from now on (its host's,
    /// `UiHost::optimizations`).
    pub fn set_optimizations(&mut self, optimizations: Optimizations) {
        self.optimizations = optimizations;
    }

    /// The optimizations it draws with.
    pub fn optimizations(&self) -> Optimizations {
        self.optimizations
    }

    pub fn size(&self) -> (i32, i32) {
        (self.surface.width(), self.surface.height())
    }

    /// Resizes the surface (its content is lost).
    pub fn resize(&mut self, width: i32, height: i32) {
        if self.size() != (width, height) {
            let optimizations = self.optimizations;
            *self = RasterRenderer::new(width, height);
            self.optimizations = optimizations;
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
        draw_display_list_with(canvas, list, text_measurer, self.optimizations);
        canvas.restore_to_count(count);
    }
}

/// Skia's paths by path identity and generation, each with its contents to tell when they're
/// gone, and how many entries make it forget the gone ones.
struct PathCache {
    paths: std::collections::HashMap<
        (u64, u64),
        (std::sync::Weak<compose_ui::PathData>, skia_safe::Path),
    >,
    /// Twice the entries left by the last sweep (256 at least): sweeping when it's reached keeps
    /// inserts amortized constant time, however many paths a frame draws.
    sweep_at: usize,
}

thread_local! {
    /// Skia's path of each path's contents drawn, kept while they live: an icon drawn every frame
    /// is converted once, a `Path` changed each frame once per change (D532).
    static PATHS: std::cell::RefCell<PathCache> = std::cell::RefCell::new(PathCache {
        paths: std::collections::HashMap::new(),
        sweep_at: 256,
    });
}

/// Skia's path for `path`, converted once per contents (while they live).
fn skia_path(path: &std::sync::Arc<compose_ui::PathData>) -> skia_safe::Path {
    let key = (path.id(), path.generation());
    PATHS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((alive, skia)) = cache.paths.get(&key)
            && alive.upgrade().is_some_and(|alive| std::sync::Arc::ptr_eq(&alive, path))
        {
            return skia.clone();
        }
        // Contents that are gone are forgotten as new ones come.
        if cache.paths.len() >= cache.sweep_at {
            cache.paths.retain(|_, (alive, _)| alive.strong_count() > 0);
            cache.sweep_at = (2 * cache.paths.len()).max(256);
        }
        let skia = build_skia_path(path);
        cache.paths.insert(key, (std::sync::Arc::downgrade(path), skia.clone()));
        skia
    })
}

/// Skia's path for `commands` filled by `fill_type`.
fn skia_commands(commands: &[PathCommand], fill_type: PathFillType) -> skia_safe::Path {
    let mut skia = skia_safe::PathBuilder::new();
    append_commands(&mut skia, commands);
    skia.set_fill_type(skia_fill_type(fill_type));
    skia.detach()
}

fn append_commands(skia: &mut skia_safe::PathBuilder, commands: &[PathCommand]) {
    for command in commands {
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
}

fn skia_fill_type(fill_type: PathFillType) -> skia_safe::PathFillType {
    match fill_type {
        PathFillType::NonZero => skia_safe::PathFillType::Winding,
        PathFillType::EvenOdd => skia_safe::PathFillType::EvenOdd,
    }
}

fn skia_operation(operation: PathOperation) -> skia_safe::PathOp {
    match operation {
        PathOperation::Difference => skia_safe::PathOp::Difference,
        PathOperation::Intersect => skia_safe::PathOp::Intersect,
        PathOperation::Union => skia_safe::PathOp::Union,
        PathOperation::Xor => skia_safe::PathOp::XOR,
        PathOperation::ReverseDifference => skia_safe::PathOp::ReverseDifference,
    }
}

/// Skia's path for `path`: its steps' result (Skia's path ops), then its commands, with its fill
/// type.
fn build_skia_path(path: &compose_ui::PathData) -> skia_safe::Path {
    let base = path.evaluate_steps(
        |operand| skia_commands(operand.commands(), operand.fill_type()),
        // An operation Skia can't do leaves the path it's done on as it was, as Android's.
        |first, second, operation| {
            skia_safe::op(&first, &second, skia_operation(operation)).unwrap_or(first)
        },
        |before, first, second, operation| {
            skia_safe::op(&first, &second, skia_operation(operation)).unwrap_or(before)
        },
        |first, second| {
            let mut builder = skia_safe::PathBuilder::new_path(&first);
            builder.add_path(&second, None);
            builder.set_fill_type(first.fill_type());
            builder.detach()
        },
        |top, fill_type| {
            top.set_fill_type(skia_fill_type(fill_type));
        },
    );
    let mut skia = match &base {
        Some(base) => skia_safe::PathBuilder::new_path(base),
        None => skia_safe::PathBuilder::new(),
    };
    append_commands(&mut skia, path.commands());
    skia.set_fill_type(skia_fill_type(path.fill_type()));
    skia.detach()
}

/// The commands of a Skia path (its conics as quadratic curves, which compose-rs's commands
/// don't have).
fn commands_of(skia: &skia_safe::Path) -> Vec<PathCommand> {
    use skia_safe::path::Verb;
    let point = |point: skia_safe::Point| compose_ui::Offset::new(point.x, point.y);
    let mut commands = Vec::new();
    let mut iter = skia_safe::path::Iter::new(skia, false);
    while let Some((verb, points)) = iter.next() {
        match verb {
            Verb::Move => commands.push(PathCommand::MoveTo(point(points[0]))),
            Verb::Line => commands.push(PathCommand::LineTo(point(points[1]))),
            Verb::Quad => commands.push(PathCommand::QuadTo(point(points[1]), point(points[2]))),
            Verb::Conic => {
                let weight = iter.conic_weight().unwrap_or(1.0);
                let mut quads = [skia_safe::Point::default(); 1 + 2 * 4];
                let count = skia_safe::Path::convert_conic_to_quads(
                    points[0], points[1], points[2], weight, &mut quads, 2,
                )
                .unwrap_or(0);
                for quad in 0..count {
                    commands.push(PathCommand::QuadTo(
                        point(quads[1 + 2 * quad]),
                        point(quads[2 + 2 * quad]),
                    ));
                }
            }
            Verb::Cubic => commands.push(PathCommand::CubicTo(
                point(points[1]),
                point(points[2]),
                point(points[3]),
            )),
            Verb::Close => commands.push(PathCommand::Close),
            _ => {}
        }
    }
    commands
}

/// `Path::op` computed when it's called, by Skia's path ops (`compose_ui::platform::PathOperator`,
/// D535), as Compose Desktop's: flat contents, or `None` if Skia can't.
fn operate(
    operation: PathOperation,
    first: &compose_ui::PathData,
    second: &compose_ui::PathData,
) -> Option<(Vec<PathCommand>, PathFillType)> {
    let result = skia_safe::op(
        &build_skia_path(first),
        &build_skia_path(second),
        skia_operation(operation),
    )?;
    let fill_type = match result.fill_type() {
        skia_safe::PathFillType::Winding | skia_safe::PathFillType::InverseWinding => {
            PathFillType::NonZero
        }
        _ => PathFillType::EvenOdd,
    };
    Some((commands_of(&result), fill_type))
}

/// Has `Path::op` computed by Skia's path ops when it's called on this thread (D535), as Compose
/// Desktop's: the desktop application installs it on its thread; tests that want it install it.
pub fn install_path_operator() {
    compose_ui::platform::set_path_operator(Some(operate));
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
