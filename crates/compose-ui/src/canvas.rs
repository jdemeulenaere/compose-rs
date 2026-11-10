//! Drawing into bitmaps, as Compose's `Canvas(imageBitmap)` and `CanvasDrawScope`
//! (docs/06-ui.md, "Offscreen bitmaps"): what's drawn is a display list's operations, which the
//! platform's [`BitmapRasterizer`] (Skia's on desktop) draws into the bitmap's pixels.

use std::sync::{Arc, RwLock};

use crate::alignment::LayoutDirection;
use crate::color_filter::{BlendMode, ColorFilter};
use crate::colorspace::ColorSpace;
use crate::display_list::{DisplayList, DrawOp};
use crate::draw::DrawScope;
use crate::graphics::{Color, Paint, PaintStyle, StrokeCap};
use crate::image::{ImageBitmap, ImageBitmapConfig};
use crate::path::Path;
use crate::unit::{Density, IntOffset, IntSize, Offset, Rect, Size};

/// Draws display lists into bitmaps' pixels: the platform's, set once by the host
/// ([`set_bitmap_rasterizer`]), as the image decoder is.
pub trait BitmapRasterizer: Send + Sync {
    /// Draws `list`'s operations over `pixels` (`width` × `height`, encoded as `config`, `has_alpha`,
    /// `color_space`, a row after another), in place. Its paragraphs aren't drawn: a bitmap's
    /// canvas has no text measurer.
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        list: &DisplayList,
        width: u32,
        height: u32,
        config: ImageBitmapConfig,
        has_alpha: bool,
        color_space: &ColorSpace,
        pixels: &mut [u8],
    );
}

static RASTERIZER: RwLock<Option<Arc<dyn BitmapRasterizer>>> = RwLock::new(None);

/// Sets the rasterizer that bitmaps' canvases draw with: the platform host's, at startup.
pub fn set_bitmap_rasterizer(rasterizer: Arc<dyn BitmapRasterizer>) {
    *RASTERIZER.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(rasterizer);
}

/// The platform's rasterizer: `None` where no platform set one (headless tests).
pub(crate) fn rasterizer() -> Option<Arc<dyn BitmapRasterizer>> {
    RASTERIZER.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// The shaders and color filters a list of operations names.
type Tables<'a> = (&'a [crate::brush::PaintShader], &'a [ColorFilter]);

/// Gives `op` the shaders and color filters it names in `from`, as entries of `into`'s tables;
/// and a drawing of a bitmap drawn into, a snapshot of its pixels now (what it reads, drawn
/// now: what waits for a bitmap never names one that waits too).
fn adopt(op: &mut DrawOp, from: Tables<'_>, into: &mut DisplayList) {
    if let Some(paint) = op.paint_mut()
        && let Some(shader) = paint.shader
    {
        paint.shader = Some(into.push_shader(from.0[shader.index()].snapshot()));
    }
    if let Some(filter) = op.color_filter_mut()
        && let Some(index) = *filter
    {
        *filter = Some(into.push_color_filter(from.1[index.index()]));
    }
    if let DrawOp::DrawImage { image, .. } = op {
        *image = image.snapshot();
    }
}

/// Each canvas's identity, for a bitmap to tell whose drawings it holds.
static NEXT_CANVAS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Immediate drawing into a bitmap, as Compose's `Canvas(imageBitmap)`: each drawing is in the
/// bitmap's pixels when they're next read, but what's drawn into a layer (`save_layer`), which
/// is drawn when the layer is restored. Its transforms and clips last until the `restore` of
/// the save before them, as a canvas's. Paints name the shaders and color filters of the canvas
/// ([`paint_with_color_filter`](Canvas::paint_with_color_filter)), as a `DrawScope`'s name its
/// list's.
///
/// Drawings wait in the bitmap, drawn by the platform's rasterizer when its pixels are read
/// (`pixels`, `read_pixels`, a renderer drawing it): one pass and at most one copy of its pixels
/// for any number of drawings. A drawing of a bitmap drawn into reads its pixels when it's
/// made. Two canvases (on two threads too) drawing into one bitmap take turns: each drawing is
/// kept, in the order made.
pub struct Canvas {
    image: ImageBitmap,
    /// Its identity, and its transforms' and clips' version: a bitmap adds a drawing to the
    /// last ones while both are the same.
    id: u64,
    version: u64,
    /// The shaders and color filters its paints name.
    tables: DisplayList,
    /// The shaders and color filters what's in its open layers names: emptied when the last is
    /// restored and drawn.
    layer_tables: DisplayList,
    /// The state before any save (transforms and clips), then each open save: its `Save` or
    /// layer, and what followed it (only transforms and clips outside a layer, everything in
    /// one).
    frames: Vec<Frame>,
}

struct Frame {
    ops: Vec<DrawOp>,
    layer: bool,
}

impl Canvas {
    /// A canvas drawing into `image`, as `Canvas(image)`.
    pub fn new(image: &ImageBitmap) -> Canvas {
        Canvas {
            image: image.clone(),
            id: NEXT_CANVAS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            version: 0,
            tables: DisplayList::default(),
            layer_tables: DisplayList::default(),
            frames: vec![Frame { ops: Vec::new(), layer: false }],
        }
    }

    /// The bitmap it draws into.
    pub fn image(&self) -> &ImageBitmap {
        &self.image
    }

    /// `paint` drawing through `color_filter`, as `Paint.colorFilter`.
    pub fn paint_with_color_filter(&mut self, paint: Paint, color_filter: ColorFilter) -> Paint {
        paint.with_color_filter(self.tables.push_color_filter(color_filter))
    }

    /// Saves the transform and clip, as `save`.
    pub fn save(&mut self) {
        self.version += 1;
        self.frames.push(Frame { ops: vec![DrawOp::Save], layer: false });
    }

    /// Saves, and draws what follows into a layer bounded by `bounds`, composited at the
    /// matching restore with `paint`'s alpha, color filter and blend mode, as
    /// `saveLayer(bounds, paint)`.
    pub fn save_layer(&mut self, bounds: Rect, paint: Paint) {
        let mut op = DrawOp::SaveLayer {
            bounds,
            alpha: paint.color.alpha(),
            color_filter: paint.color_filter(),
            blend_mode: paint.blend_mode,
        };
        adopt(&mut op, (&self.tables.shaders, &self.tables.color_filters), &mut self.layer_tables);
        self.version += 1;
        self.frames.push(Frame { ops: vec![op], layer: true });
    }

    /// Undoes what followed the last save, as `restore`; a layer is drawn now. A restore with
    /// no save does nothing, as Skia's.
    pub fn restore(&mut self) {
        if self.frames.len() == 1 {
            return;
        }
        let Some(mut frame) = self.frames.pop() else { return };
        self.version += 1;
        if self.in_layer() {
            frame.ops.push(DrawOp::Restore);
            self.frames.last_mut().expect("the root frame").ops.extend(frame.ops);
        } else if frame.layer {
            frame.ops.push(DrawOp::Restore);
            let tables = std::mem::take(&mut self.layer_tables);
            self.submit(frame.ops, (&tables.shaders, &tables.color_filters));
        }
    }

    /// Whether a layer is open: what's drawn waits for its restore.
    fn in_layer(&self) -> bool {
        self.frames.iter().any(|frame| frame.layer)
    }

    /// Runs `block` between a save and its restore, as `withSave`.
    pub fn with_save(&mut self, block: impl FnOnce(&mut Canvas)) {
        self.save();
        block(self);
        self.restore();
    }

    /// Runs `block` in a layer, as `withSaveLayer`.
    pub fn with_save_layer(&mut self, bounds: Rect, paint: Paint, block: impl FnOnce(&mut Canvas)) {
        self.save_layer(bounds, paint);
        block(self);
        self.restore();
    }

    fn state(&mut self, op: DrawOp) {
        self.version += 1;
        self.frames.last_mut().expect("the root frame").ops.push(op);
    }

    /// Moves what follows by `(dx, dy)`, as `translate`.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.state(DrawOp::Translate(Offset::new(dx, dy)));
    }

    /// Scales what follows about the origin, as `scale(sx, sy)`.
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.state(DrawOp::Transform {
            pivot: Offset::ZERO,
            scale: (sx, sy),
            rotation: 0.0,
            translation: Offset::ZERO,
        });
    }

    /// Scales what follows about `(pivot_x, pivot_y)`, as `Canvas.scale(sx, sy, pivotX, pivotY)`.
    pub fn scale_around(&mut self, sx: f32, sy: f32, pivot_x: f32, pivot_y: f32) {
        self.translate(pivot_x, pivot_y);
        self.scale(sx, sy);
        self.translate(-pivot_x, -pivot_y);
    }

    /// Rotates what follows by `degrees` (clockwise) about the origin, as `rotate(degrees)`.
    pub fn rotate(&mut self, degrees: f32) {
        self.state(DrawOp::Transform {
            pivot: Offset::ZERO,
            scale: (1.0, 1.0),
            rotation: degrees,
            translation: Offset::ZERO,
        });
    }

    /// Rotates what follows about `(pivot_x, pivot_y)`, as
    /// `Canvas.rotate(degrees, pivotX, pivotY)`.
    pub fn rotate_around(&mut self, degrees: f32, pivot_x: f32, pivot_y: f32) {
        self.translate(pivot_x, pivot_y);
        self.rotate(degrees);
        self.translate(-pivot_x, -pivot_y);
    }

    /// Clips what follows to `rect`, as `clipRect(rect)`.
    pub fn clip_rect(&mut self, rect: Rect) {
        self.state(DrawOp::ClipRect(rect));
    }

    /// Clips what follows to `path`, as `clipPath(path)`.
    pub fn clip_path(&mut self, path: &Path) {
        self.state(DrawOp::ClipPath { path: path.data(), offset: Offset::ZERO });
    }

    /// Clips what follows to `rect` or out of it, as `clipRect(rect, clipOp)`.
    pub fn clip_rect_with(&mut self, rect: Rect, clip_op: ClipOp) {
        match clip_op {
            ClipOp::Intersect => self.clip_rect(rect),
            ClipOp::Difference => {
                let path = Path::new();
                path.add_rect(rect);
                self.clip_path_with(&path, ClipOp::Difference);
            }
        }
    }

    /// Clips what follows to `path` or out of it, as `clipPath(path, clipOp)`.
    pub fn clip_path_with(&mut self, path: &Path, clip_op: ClipOp) {
        match clip_op {
            ClipOp::Intersect => self.clip_path(path),
            ClipOp::Difference => {
                self.state(DrawOp::ClipOutPath { path: path.data(), offset: Offset::ZERO });
            }
        }
    }

    /// Skews what follows by `sx` horizontally and `sy` vertically (tangents of the angles),
    /// as `skew(sx, sy)`.
    pub fn skew(&mut self, sx: f32, sy: f32) {
        self.state(DrawOp::Concat(Box::new([1.0, sx, 0.0, sy, 1.0, 0.0, 0.0, 0.0, 1.0])));
    }

    /// Skews what follows by angles in radians, as `skewRad(sxRad, syRad)`.
    pub fn skew_rad(&mut self, sx_rad: f32, sy_rad: f32) {
        self.skew(sx_rad.tan(), sy_rad.tan());
    }

    /// Transforms what follows by `matrix`, as `concat(matrix)`: its 2D part (the z row and
    /// column left out), as Android's canvas takes it.
    pub fn concat(&mut self, matrix: &crate::matrix::Matrix) {
        let m = &matrix.values;
        self.state(DrawOp::Concat(Box::new([
            m[0], m[4], m[12], m[1], m[5], m[13], m[3], m[7], m[15],
        ])));
    }

    /// `points` drawn as `point_mode` says, with `paint`'s stroke width and cap, as
    /// `drawPoints(pointMode, points, paint)`: each a dot (round if the cap is, else square, a
    /// pixel wide for a fill or a hairline), each pair a line, or a line through them all.
    pub fn draw_points(&mut self, point_mode: PointMode, points: &[Offset], paint: Paint) {
        for op in points_ops(point_mode, points, paint) {
            self.draw(op);
        }
    }

    /// [`draw_points`](Canvas::draw_points) of points given as x, y pairs, as
    /// `drawRawPoints(pointMode, points, paint)`: a last odd value is left out.
    pub fn draw_raw_points(&mut self, point_mode: PointMode, points: &[f32], paint: Paint) {
        let points: Vec<Offset> =
            points.as_chunks::<2>().0.iter().map(|[x, y]| Offset::new(*x, *y)).collect();
        self.draw_points(point_mode, &points, paint);
    }

    /// `vertices`' triangles, each vertex's color blended with `paint`'s by `blend_mode`, as
    /// `drawVertices(vertices, blendMode, paint)`.
    pub fn draw_vertices(&mut self, vertices: &Arc<Vertices>, blend_mode: BlendMode, paint: Paint) {
        self.draw(DrawOp::DrawVertices { vertices: vertices.clone(), blend_mode, paint });
    }

    pub fn draw_line(&mut self, from: Offset, to: Offset, paint: Paint) {
        self.draw(DrawOp::DrawLine(from, to, paint));
    }

    pub fn draw_rect(&mut self, rect: Rect, paint: Paint) {
        self.draw(DrawOp::DrawRect(rect, paint));
    }

    /// A rectangle with corners of radii `radius_x` and `radius_y`, as `drawRoundRect`.
    pub fn draw_round_rect(&mut self, rect: Rect, radius_x: f32, radius_y: f32, paint: Paint) {
        if radius_x == radius_y {
            self.draw(DrawOp::DrawRoundRect(rect, [radius_x; 4], paint));
        } else {
            let path = Path::new();
            let radius = crate::geometry::CornerRadius::new(radius_x, radius_y);
            path.add_round_rect(crate::geometry::RoundRect::new(rect, radius));
            self.draw_path(&path, paint);
        }
    }

    pub fn draw_oval(&mut self, rect: Rect, paint: Paint) {
        self.draw_arc(rect, 0.0, 360.0, false, paint);
    }

    pub fn draw_circle(&mut self, center: Offset, radius: f32, paint: Paint) {
        let oval =
            Rect::new(center.x - radius, center.y - radius, center.x + radius, center.y + radius);
        self.draw_oval(oval, paint);
    }

    /// An arc of the oval in `rect` from `start_angle` degrees over `sweep_angle`, closed
    /// through the center if `use_center`, as `drawArc`.
    pub fn draw_arc(
        &mut self,
        rect: Rect,
        start_angle: f32,
        sweep_angle: f32,
        use_center: bool,
        paint: Paint,
    ) {
        self.draw(DrawOp::DrawArc { oval: rect, start_angle, sweep_angle, use_center, paint });
    }

    pub fn draw_path(&mut self, path: &Path, paint: Paint) {
        let path = path.data();
        self.draw(DrawOp::DrawPath { path, offset: Offset::ZERO, scale: (1.0, 1.0), paint });
    }

    /// `image` with its top left at `top_left`, at the paint's alpha and through its color
    /// filter, as `drawImage`.
    pub fn draw_image(&mut self, image: &ImageBitmap, top_left: Offset, paint: Paint) {
        let (width, height) = (image.width() as f32, image.height() as f32);
        self.draw(DrawOp::DrawImage {
            image: image.clone(),
            src: Rect::new(0.0, 0.0, width, height),
            dst: Rect::new(top_left.x, top_left.y, top_left.x + width, top_left.y + height),
            alpha: paint.color.alpha(),
            color_filter: paint.color_filter(),
            filter_quality: crate::image::FilterQuality::Low,
        });
    }

    /// The part of `image` at `src_offset` of `src_size` drawn into the part of the canvas at
    /// `dst_offset` of `dst_size`, as `drawImageRect`.
    pub fn draw_image_rect(
        &mut self,
        image: &ImageBitmap,
        src_offset: IntOffset,
        src_size: IntSize,
        dst_offset: IntOffset,
        dst_size: IntSize,
        paint: Paint,
    ) {
        let rect = |offset: IntOffset, size: IntSize| {
            Rect::new(
                offset.x as f32,
                offset.y as f32,
                (offset.x + size.width) as f32,
                (offset.y + size.height) as f32,
            )
        };
        self.draw(DrawOp::DrawImage {
            image: image.clone(),
            src: rect(src_offset, src_size),
            dst: rect(dst_offset, dst_size),
            alpha: paint.color.alpha(),
            color_filter: paint.color_filter(),
            filter_quality: crate::image::FilterQuality::Low,
        });
    }

    /// Draws `op` now, or keeps it for the layer it's in.
    fn draw(&mut self, op: DrawOp) {
        let tables = std::mem::take(&mut self.tables);
        self.draw_ops(vec![op], (&tables.shaders, &tables.color_filters));
        self.tables = tables;
    }

    /// Draws `ops`, naming `from`'s shaders and filters, now; or keeps them for the layer
    /// they're in, their shaders and filters in the layers' tables.
    fn draw_ops(&mut self, mut ops: Vec<DrawOp>, from: Tables<'_>) {
        if self.in_layer() {
            for op in &mut ops {
                adopt(op, from, &mut self.layer_tables);
            }
            self.frames.last_mut().expect("the root frame").ops.extend(ops);
        } else {
            self.submit(ops, from);
        }
    }

    /// Gives `ops` to the bitmap, drawn after the open saves' transforms and clips at its next
    /// read: after its last ones, in their save, if they're this canvas's in the same state;
    /// else in a save of their own after the state.
    fn submit(&mut self, mut ops: Vec<DrawOp>, from: Tables<'_>) {
        // A drawn bitmap's pixels are read (this one's too) before its lock is taken: an image
        // shader's too.
        for op in &mut ops {
            if let DrawOp::DrawImage { image, .. } = op {
                *image = image.snapshot();
            }
        }
        let snapshots: Option<Vec<crate::brush::PaintShader>> = from
            .0
            .iter()
            .any(|shader| shader.shader.has_image())
            .then(|| from.0.iter().map(crate::brush::PaintShader::snapshot).collect());
        let from = match &snapshots {
            Some(shaders) => (&shaders[..], from.1),
            None => from,
        };
        let key = (self.id, self.version);
        let frames = &self.frames;
        self.image.with_drawing(|drawing| {
            if drawing.open.is_none_or(|(id, version, _)| (id, version) != key) {
                drawing.close();
                drawing.pending.ops.push(DrawOp::Save);
                for frame in frames {
                    drawing.pending.ops.extend(frame.ops.iter().cloned());
                }
                // Its own save and each open one.
                drawing.open = Some((key.0, key.1, frames.len()));
            }
            for mut op in ops {
                adopt(&mut op, from, &mut drawing.pending);
                drawing.pending.ops.push(op);
            }
        });
    }

    /// Draws `recording` (a `DrawScope`'s), naming its own shaders and filters.
    pub(crate) fn draw_recording(&mut self, recording: DisplayList) {
        self.draw_ops(recording.ops, (&recording.shaders, &recording.color_filters));
    }
}

/// Draws with a `DrawScope` into a [`Canvas`], as Compose's `CanvasDrawScope`.
pub struct CanvasDrawScope;

impl CanvasDrawScope {
    /// Runs `block` with a `DrawScope` of `size`, `density` and `layout_direction`, what it
    /// draws drawn into `canvas` (after its transforms and clips) at once, as
    /// `CanvasDrawScope().draw(density, layoutDirection, canvas, size) { … }`.
    pub fn draw(
        canvas: &mut Canvas,
        density: Density,
        layout_direction: LayoutDirection,
        size: Size,
        block: impl FnOnce(&mut DrawScope<'_>),
    ) {
        let mut recording = DisplayList::default();
        let mut scope = DrawScope {
            list: &mut recording,
            origin: Offset::ZERO,
            size,
            density,
            layout_direction,
            locals: Default::default(),
        };
        block(&mut scope);
        canvas.draw_recording(recording);
    }
}

/// Whether a clip keeps what's inside its shape or what's outside, as Compose's `ClipOp`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ClipOp {
    #[default]
    Intersect,
    Difference,
}

/// How `draw_points` draws its points, as Compose's `PointMode`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PointMode {
    /// Each point a dot.
    Points,
    /// Each pair of points a line (a last odd one left out).
    Lines,
    /// A line from each point to the next.
    Polygon,
}

/// The operations `points` drawn as `point_mode` says are (no operation of their own: dots are
/// lines of no length with a square or round cap, as Skia draws points).
pub(crate) fn points_ops(point_mode: PointMode, points: &[Offset], paint: Paint) -> Vec<DrawOp> {
    let width = match paint.style {
        PaintStyle::Stroke(width) => width,
        PaintStyle::Fill => 0.0,
    };
    let mut stroke = paint;
    stroke.style = PaintStyle::Stroke(width);
    match point_mode {
        PointMode::Points => {
            stroke.style = PaintStyle::Stroke(if width > 0.0 { width } else { 1.0 });
            if stroke.cap != StrokeCap::Round {
                stroke.cap = StrokeCap::Square;
            }
            points.iter().map(|point| DrawOp::DrawLine(*point, *point, stroke)).collect()
        }
        PointMode::Lines => points
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[from, to]| DrawOp::DrawLine(*from, *to, stroke))
            .collect(),
        PointMode::Polygon => {
            points.windows(2).map(|pair| DrawOp::DrawLine(pair[0], pair[1], stroke)).collect()
        }
    }
}

/// How `Vertices`' points make triangles, as Compose's `VertexMode`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VertexMode {
    /// Each three points a triangle.
    Triangles,
    /// Each point a triangle with the two before it.
    TriangleStrip,
    /// Each point a triangle with the one before it and the first.
    TriangleFan,
}

/// Triangles to draw with `Canvas::draw_vertices`, as Compose's `Vertices`: positions, the
/// texture coordinates a paint's shader is sampled at, a color per vertex, and the indices of
/// the vertices drawn in order (all of them in order if none).
#[derive(Clone, PartialEq, Debug)]
pub struct Vertices {
    vertex_mode: VertexMode,
    positions: Vec<Offset>,
    texture_coordinates: Vec<Offset>,
    colors: Vec<Color>,
    indices: Vec<u16>,
}

impl Vertices {
    /// As `Vertices(vertexMode, positions, textureCoordinates, colors, indices)`.
    ///
    /// # Panics
    ///
    /// As Compose's throws: if `texture_coordinates` or `colors` aren't one per position, or an
    /// index names no position.
    pub fn new(
        vertex_mode: VertexMode,
        positions: &[Offset],
        texture_coordinates: &[Offset],
        colors: &[Color],
        indices: &[u16],
    ) -> Vertices {
        assert!(
            texture_coordinates.len() == positions.len(),
            "positions and textureCoordinates lengths must match."
        );
        assert!(colors.len() == positions.len(), "positions and colors lengths must match.");
        assert!(
            indices.iter().all(|index| usize::from(*index) < positions.len()),
            "indices values must be valid indices in the positions list."
        );
        Vertices {
            vertex_mode,
            positions: positions.to_vec(),
            texture_coordinates: texture_coordinates.to_vec(),
            colors: colors.to_vec(),
            indices: indices.to_vec(),
        }
    }

    /// From flat arrays, as the array constructor: positions and texture coordinates as x, y
    /// pairs, colors as ARGB.
    ///
    /// # Panics
    ///
    /// As Compose's throws: if `positions` has an odd length, `texture_coordinates` isn't as
    /// long, `colors` isn't one per position, or an index names no position.
    pub fn from_raw(
        vertex_mode: VertexMode,
        positions: &[f32],
        texture_coordinates: &[f32],
        colors: &[u32],
        indices: &[u16],
    ) -> Vertices {
        assert!(positions.len().is_multiple_of(2), "positions length must be even");
        assert!(
            texture_coordinates.len() == positions.len(),
            "positions and textureCoordinates lengths must match."
        );
        assert!(colors.len() == positions.len() / 2, "positions and colors lengths must match.");
        let pairs = |values: &[f32]| -> Vec<Offset> {
            values.as_chunks::<2>().0.iter().map(|[x, y]| Offset::new(*x, *y)).collect()
        };
        let colors: Vec<Color> = colors.iter().map(|argb| Color::from_argb(*argb)).collect();
        Vertices::new(vertex_mode, &pairs(positions), &pairs(texture_coordinates), &colors, indices)
    }

    pub fn vertex_mode(&self) -> VertexMode {
        self.vertex_mode
    }

    pub fn positions(&self) -> &[Offset] {
        &self.positions
    }

    pub fn texture_coordinates(&self) -> &[Offset] {
        &self.texture_coordinates
    }

    pub fn colors(&self) -> &[Color] {
        &self.colors
    }

    pub fn indices(&self) -> &[u16] {
        &self.indices
    }
}
