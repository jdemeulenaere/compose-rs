//! The display list as bytes for the Kotlin replayer (docs/07-platforms.md, "Drawing without
//! shipping Skia"): one buffer per frame, read on the Kotlin side with a little-endian
//! `ByteBuffer`, each operation a tag byte and its operands.
//!
//! Paths and images are resources: defined once (a `DefinePath` or `DefineImage` before their
//! first use) and dropped when the last frame using them is gone, so a frame names them by id
//! (drawn with `DRAW_PATH`, clipped to with `CLIP_PATH`). A gradient isn't a resource: a paint
//! with a shader carries its whole definition (D210): a paint names a shader of its own
//! recording's table (D175, D270), and a cached record's bytes stand on their own.
//! Their definitions wait until the Kotlin side takes them with the frame it draws, so a frame
//! drawn twice or skipped loses none; one dropped before it was taken is cancelled.
//!
//! A repaint boundary's group is a `GROUP` record carrying its key and version, which the
//! replayer keeps a render node by; a graphics layer's properties are in a `LAYERS` section
//! before the operations, not in any record, so a key and a version are always the same bytes,
//! copied from a [`GroupCache`] while the version stays (D157), without reading the group's
//! operations (the frame holds each group's recording by reference, D270). A record names the
//! groups drawn inside it by key (`GROUP_REF`), their records written apart: a group whose
//! nested group changed keeps its bytes, and its version for the replayer (D323). The groups
//! composed and not placed (`DisplayList::kept`) are named by key (`KEPT`), their records kept
//! in the cache, so the replayer keeps their nodes for the frame placing them again (D380).

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use compose_ui::{
    Color, ImageBitmap, Offset, Outline, Paint, PaintStyle, Path, PathCommand, PathFillType, Rect,
    StrokeCap, TileMode,
    platform::Shader,
    platform::{DisplayList, DrawGroup, DrawOp},
};

/// The operations' tags, as `Replayer.kt` reads them.
pub mod tag {
    pub const SAVE: u8 = 0;
    pub const RESTORE: u8 = 1;
    pub const TRANSLATE: u8 = 2;
    pub const CLIP_RECT: u8 = 3;
    pub const CLIP_ROUND_RECT: u8 = 4;
    pub const DRAW_RECT: u8 = 5;
    pub const DRAW_ROUND_RECT: u8 = 6;
    pub const DRAW_LINE: u8 = 7;
    pub const DRAW_PARAGRAPH: u8 = 8;
    pub const SAVE_LAYER_ALPHA: u8 = 9;
    pub const TRANSFORM: u8 = 10;
    pub const DRAW_IMAGE: u8 = 11;
    pub const DRAW_SHADOW: u8 = 12;
    pub const DRAW_ARC: u8 = 13;
    pub const DRAW_PATH: u8 = 14;
    /// A repaint boundary's record: its key (u64), its version (u64), its content's length in
    /// bytes (u32), then its content (its layer's operations left out, the groups drawn in it
    /// as `GROUP_REF`s). The same key and version are always the same bytes: the replayer keeps
    /// the group's render node by them. Every group's record comes before the frame's own
    /// operations, a group's before those of the groups drawn in it.
    pub const GROUP: u8 = 15;
    /// The layer properties of the groups that have a layer, before the operations: a count
    /// (u32), then per group its key (u64), alpha, pivot (x, y), scale (x, y), rotation
    /// (degrees), translation (x, y) (f32s), whether it clips (u8) and its clip (left, top,
    /// right, bottom; zeros if it doesn't), then whether its alpha composites its content
    /// offscreen (u8; 0 for one drawing, faded instead), then its bounds (left, top, right,
    /// bottom): `LAYER_ENTRY_BYTES` each.
    pub const LAYERS: u8 = 16;
    /// Clips what follows to a path (u32 id) moved by an offset (x, y).
    pub const CLIP_PATH: u8 = 17;
    /// Draws the group of a key (u64), whose `GROUP` record the frame holds (D323).
    pub const GROUP_REF: u8 = 18;
    /// The groups composed and not placed (`DisplayList::kept`), which no `GROUP_REF` draws: a
    /// count (u32), then per group (outer before inner) its key (u64) and whether it is
    /// outermost (u8), after the records and before the operations. Their records aren't in
    /// the frame (unless their version changed): the replayer keeps their render nodes as
    /// recorded, layers included, drawing the outermost ones out of sight (HWUI drops the
    /// recording of a node no recording draws), so the frame placing one again copies its
    /// record and records nothing (D380).
    pub const KEPT: u8 = 19;
    pub const DEFINE_PATH: u8 = 20;
    pub const DROP_PATH: u8 = 21;
    pub const DEFINE_IMAGE: u8 = 22;
    pub const DROP_IMAGE: u8 = 23;
}

/// A path command's kind, as `Replayer.kt` reads it.
mod command {
    pub const MOVE_TO: u8 = 0;
    pub const LINE_TO: u8 = 1;
    pub const QUAD_TO: u8 = 2;
    pub const CUBIC_TO: u8 = 3;
    pub const CLOSE: u8 = 4;
}

/// A paint's shader kind, as `Replayer.kt` reads it.
pub mod shader {
    pub const NONE: u8 = 0;
    pub const LINEAR: u8 = 1;
    pub const RADIAL: u8 = 2;
    pub const SWEEP: u8 = 3;
}

/// Little-endian writes into a byte buffer.
#[derive(Default)]
pub struct Writer {
    pub bytes: Vec<u8>,
}

impl Writer {
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    fn offset(&mut self, offset: Offset) {
        self.f32(offset.x);
        self.f32(offset.y);
    }
    fn rect(&mut self, rect: &Rect) {
        self.f32(rect.left);
        self.f32(rect.top);
        self.f32(rect.right);
        self.f32(rect.bottom);
    }
    fn radii(&mut self, radii: &[f32; 4]) {
        for radius in radii {
            self.f32(*radius);
        }
    }
    fn color(&mut self, color: Color) {
        self.u32(color.argb());
    }
    /// Color, style (0 fill, 1 stroke), stroke width, cap (0 butt, 1 round, 2 square), then
    /// its shader (from `shaders`): see [`Writer::shader`]. Faded by `alpha` (folded layers'):
    /// its color's alpha, or its shader's. A gradient that is a solid color (see [`shading`])
    /// is that color at the shader's alpha, with no shader.
    fn paint(&mut self, paint: &Paint, shaders: &[Shader], alpha: f32) {
        let shading = paint
            .shader()
            .and_then(|index| shaders.get(index.0 as usize))
            .map_or(Shading::Color, shading);
        let color = match shading {
            Shading::Solid(color, opacity) => {
                color.with_alpha(color.alpha() as f32 / 255.0 * opacity.clamp(0.0, 1.0))
            }
            Shading::Color | Shading::Gradient(_) => paint.color,
        };
        self.color(faded(color, alpha));
        match paint.style {
            PaintStyle::Fill => {
                self.u8(0);
                self.f32(0.0);
            }
            PaintStyle::Stroke(width) => {
                self.u8(1);
                self.f32(width);
            }
        }
        self.u8(match paint.cap {
            StrokeCap::Butt => 0,
            StrokeCap::Round => 1,
            StrokeCap::Square => 2,
        });
        match shading {
            Shading::Gradient(shader) => {
                self.shader(shader, shader.alpha() * alpha.clamp(0.0, 1.0));
            }
            Shading::Color | Shading::Solid(..) => self.u8(shader::NONE),
        }
    }
    /// A shader's definition, then its alpha: its kind (a [`shader`] constant), tile mode (0
    /// clamp, 1 repeated, 2 mirror, 3 decal), geometry (linear: from x, y, to x, y; radial:
    /// center x, y, radius; sweep: center x, y), color count (u32, 2 at least: one color is
    /// written twice), colors (ARGB), stop count (u32: 0 for evenly spread colors, else the
    /// color count), stops, then the alpha the paint has (f32). The definition (kind to stops)
    /// is what `Replayer.kt` keeps its `Shader` objects by.
    fn shader(&mut self, shader: &Shader, alpha: f32) {
        let gradient = shader.gradient();
        let tile_mode = match gradient.tile_mode {
            TileMode::Clamp => 0,
            TileMode::Repeated => 1,
            TileMode::Mirror => 2,
            TileMode::Decal => 3,
        };
        match shader {
            Shader::LinearGradient { from, to, .. } => {
                self.u8(shader::LINEAR);
                self.u8(tile_mode);
                self.offset(*from);
                self.offset(*to);
            }
            Shader::RadialGradient { center, radius, .. } => {
                self.u8(shader::RADIAL);
                self.u8(tile_mode);
                self.offset(*center);
                self.f32(*radius);
            }
            Shader::SweepGradient { center, .. } => {
                self.u8(shader::SWEEP);
                self.u8(tile_mode);
                self.offset(*center);
            }
        }
        let colors = &gradient.colors;
        // A gradient needs two colors: one is a solid color, as compose-skia draws it.
        let count = colors.len().max(2);
        self.u32(count as u32);
        for index in 0..count {
            self.color(colors[index.min(colors.len() - 1)]);
        }
        // Stops not matching the colors are left out (evenly spread), as compose-skia does.
        match gradient.stops.as_deref().filter(|stops| stops.len() == count) {
            Some(stops) => {
                self.u32(count as u32);
                for stop in stops {
                    self.f32(*stop);
                }
            }
            None => self.u32(0),
        }
        self.f32(alpha.clamp(0.0, 1.0));
    }
    /// Kind (0 rectangle, 1 rounded), bounds, radii.
    fn outline(&mut self, outline: &Outline) {
        match outline {
            Outline::Rectangle(rect) => {
                self.u8(0);
                self.rect(rect);
                self.radii(&[0.0; 4]);
            }
            Outline::Rounded(rect, radii) => {
                self.u8(1);
                self.rect(rect);
                self.radii(radii);
            }
        }
    }
}

/// What paints a paint with a shader on Android.
enum Shading<'a> {
    /// The paint's color (no shader, or one Skia doesn't make: the gradient's average, D175).
    Color,
    /// The gradient.
    Gradient(&'a Shader),
    /// A solid color at an alpha (the shader's): a degenerate gradient.
    Solid(Color, f32),
}

/// A radius Skia takes for none (`SkGradientShader`'s `kDegenerateThreshold`).
const DEGENERATE_RADIUS: f32 = 1.0 / (1 << 15) as f32;

/// How `shader` paints, as compose-skia's Skia draws it: no colors, a position not finite or a
/// negative radius make no shader (the paint's color paints); a radius of (nearly) 0, which
/// Android's `RadialGradient` refuses, is Skia's degenerate gradient: its last color when
/// clamped, its average when repeated or mirrored, nothing with decal.
fn shading(shader: &Shader) -> Shading<'_> {
    let finite = |offset: &Offset| offset.x.is_finite() && offset.y.is_finite();
    let gradient = shader.gradient();
    if gradient.colors.is_empty() {
        return Shading::Color;
    }
    match shader {
        Shader::LinearGradient { from, to, .. } if !finite(from) || !finite(to) => Shading::Color,
        Shader::RadialGradient { center, radius, .. }
            if !finite(center) || !radius.is_finite() || *radius < 0.0 =>
        {
            Shading::Color
        }
        Shader::RadialGradient { radius, .. } if *radius <= DEGENERATE_RADIUS => {
            let color = match gradient.tile_mode {
                TileMode::Clamp => *gradient.colors.last().expect("colors"),
                TileMode::Repeated | TileMode::Mirror => average_color(gradient),
                TileMode::Decal => Color::TRANSPARENT,
            };
            Shading::Solid(color, shader.alpha())
        }
        Shader::SweepGradient { center, .. } if !finite(center) => Shading::Color,
        _ => Shading::Gradient(shader),
    }
}

/// A gradient's mean color over 0 to 1 (unpremultiplied channels, each stretch between two
/// stops its colors' mean, the ends their color), as Skia's `average_gradient_color`.
fn average_color(gradient: &compose_ui::platform::Gradient) -> Color {
    let colors: Vec<[f32; 4]> = gradient
        .colors
        .iter()
        .map(|color| [color.alpha(), color.red(), color.green(), color.blue()].map(f32::from))
        .collect();
    let colors = if colors.len() == 1 { vec![colors[0]; 2] } else { colors };
    let count = colors.len();
    let stops: Vec<f32> = match gradient.stops.as_deref().filter(|stops| stops.len() == count) {
        Some(stops) => stops.to_vec(),
        None => (0..count).map(|index| index as f32 / (count - 1) as f32).collect(),
    };
    let mut sum = [0.0f32; 4];
    let mut add = |color: [f32; 4], weight: f32| {
        for (total, channel) in sum.iter_mut().zip(color) {
            *total += channel * weight;
        }
    };
    let first = stops[0].clamp(0.0, 1.0);
    add(colors[0], first);
    let mut previous = first;
    for index in 1..count {
        let stop = stops[index].clamp(previous, 1.0);
        let middle = std::array::from_fn(|channel| {
            (colors[index - 1][channel] + colors[index][channel]) / 2.0
        });
        add(middle, stop - previous);
        previous = stop;
    }
    add(colors[count - 1], 1.0 - previous);
    let [alpha, red, green, blue] = sum.map(|value| value.round().clamp(0.0, 255.0) as u32);
    Color::from_argb(alpha << 24 | red << 16 | green << 8 | blue)
}

/// A resource change the Kotlin side hasn't taken yet.
enum Pending {
    Path { id: u32, path: Weak<Path> },
    Image { id: u32, width: u32, height: u32, pixels: Weak<[u8]> },
    DropPath(u32),
    DropImage(u32),
}

/// The paths and images the Kotlin side holds, by id.
#[derive(Default)]
pub struct Resources {
    paths: HashMap<usize, (Weak<Path>, u32)>,
    images: HashMap<usize, (Weak<[u8]>, u32)>,
    next_id: u32,
    /// Definitions and drops the Kotlin side hasn't taken yet: a definition dropped before it
    /// was taken is cancelled, so this stays bounded however many frames go undrawn.
    pending: Vec<Pending>,
}

impl Resources {
    fn next(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }

    /// Drops path `id`: its definition, if still pending, or the Kotlin side's.
    fn drop_path(&mut self, id: u32) {
        let before = self.pending.len();
        self.pending
            .retain(|op| !matches!(op, Pending::Path { id: defined, .. } if *defined == id));
        if self.pending.len() == before {
            self.pending.push(Pending::DropPath(id));
        }
    }

    fn drop_image(&mut self, id: u32) {
        let before = self.pending.len();
        self.pending
            .retain(|op| !matches!(op, Pending::Image { id: defined, .. } if *defined == id));
        if self.pending.len() == before {
            self.pending.push(Pending::DropImage(id));
        }
    }

    /// The id of `path`, defined if new.
    fn path_id(&mut self, path: &Arc<Path>) -> u32 {
        let key = Arc::as_ptr(path) as usize;
        if let Some((weak, id)) = self.paths.get(&key)
            && weak.upgrade().is_some_and(|alive| Arc::ptr_eq(&alive, path))
        {
            return *id;
        }
        let id = self.next();
        if let Some((_, old)) = self.paths.insert(key, (Arc::downgrade(path), id)) {
            self.drop_path(old);
        }
        self.pending.push(Pending::Path { id, path: Arc::downgrade(path) });
        id
    }

    /// The id of `image`, defined if new; 0 (nothing) for an empty one.
    fn image_id(&mut self, image: &ImageBitmap) -> u32 {
        if image.width() == 0 || image.height() == 0 {
            return 0;
        }
        let key = image.id();
        if let Some((weak, id)) = self.images.get(&key)
            && weak.strong_count() > 0
        {
            return *id;
        }
        let id = self.next();
        if let Some((_, old)) = self.images.insert(key, (image.downgrade(), id)) {
            self.drop_image(old);
        }
        self.pending.push(Pending::Image {
            id,
            width: image.width(),
            height: image.height(),
            pixels: image.downgrade(),
        });
        id
    }

    /// Drops the resources no frame can use any more (their owners are gone).
    pub fn collect(&mut self) {
        let dead_paths: Vec<u32> = self
            .paths
            .values()
            .filter(|(weak, _)| weak.strong_count() == 0)
            .map(|(_, id)| *id)
            .collect();
        self.paths.retain(|_, (weak, _)| weak.strong_count() > 0);
        for id in dead_paths {
            self.drop_path(id);
        }
        let dead_images: Vec<u32> = self
            .images
            .values()
            .filter(|(weak, _)| weak.strong_count() == 0)
            .map(|(_, id)| *id)
            .collect();
        self.images.retain(|_, (weak, _)| weak.strong_count() > 0);
        for id in dead_images {
            self.drop_image(id);
        }
    }

    /// Whether changes wait for the Kotlin side.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Appends the pending changes to `out` (a definition whose owner is already gone is
    /// skipped), and forgets them: the Kotlin side has them. Drops come first (D410): a frame
    /// replacing its paths (a canvas drawing new ones each frame) defines them after the frame
    /// before's are dropped, so the replayer rebuilds the dropped `Path`s rather than making
    /// new ones. (A drop names an id taken before, never one defined beside it: that one's
    /// definition is cancelled instead.)
    pub fn write_pending(&mut self, out: &mut Writer) {
        for op in &self.pending {
            match op {
                Pending::DropPath(id) => {
                    out.u8(tag::DROP_PATH);
                    out.u32(*id);
                }
                Pending::DropImage(id) => {
                    out.u8(tag::DROP_IMAGE);
                    out.u32(*id);
                }
                Pending::Path { .. } | Pending::Image { .. } => {}
            }
        }
        for op in self.pending.drain(..) {
            match op {
                Pending::Path { id, path } => {
                    let Some(path) = path.upgrade() else { continue };
                    write_path(out, id, &path);
                }
                Pending::Image { id, width, height, pixels } => {
                    let Some(pixels) = pixels.upgrade() else { continue };
                    out.u8(tag::DEFINE_IMAGE);
                    out.u32(id);
                    out.u32(width);
                    out.u32(height);
                    out.u32(pixels.len() as u32);
                    out.bytes.extend_from_slice(&pixels);
                }
                Pending::DropPath(_) | Pending::DropImage(_) => {}
            }
        }
    }
}

/// The operand bytes of the operation whose tag is at `at` (not a `GROUP`, `LAYERS` or resource
/// change; a `KEPT` section counts as one), as `Replayer.kt` reads them: for tests and tools
/// reading a frame back.
pub fn operand_bytes(bytes: &[u8], at: usize) -> usize {
    // The paint is the last operand, after `fixed` bytes.
    let painted = |fixed: usize| fixed + paint_bytes(bytes, at + 1 + fixed);
    match bytes[at] {
        tag::SAVE | tag::RESTORE => 0,
        tag::TRANSLATE => 8,
        tag::CLIP_RECT => 16,
        tag::CLIP_ROUND_RECT => 32,
        tag::CLIP_PATH => 4 + 8,
        tag::GROUP_REF => 8,
        tag::KEPT => {
            4 + KEPT_ENTRY_BYTES
                * u32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize
        }
        tag::DRAW_RECT | tag::DRAW_LINE => painted(16),
        tag::DRAW_ROUND_RECT => painted(32),
        tag::DRAW_PARAGRAPH => 12,
        tag::SAVE_LAYER_ALPHA => 4,
        tag::TRANSFORM => 28,
        tag::DRAW_IMAGE => 4 + 32 + 4,
        tag::DRAW_SHADOW => 1 + 32 + 4 + 12 + 4 + 8,
        tag::DRAW_ARC => painted(16 + 8 + 1),
        tag::DRAW_PATH => painted(4 + 16),
        other => panic!("not a drawing operation's tag: {other}"),
    }
}

/// The bytes of the paint at `at` ([`Writer::paint`]).
fn paint_bytes(bytes: &[u8], at: usize) -> usize {
    let count_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let kind_at = at + 4 + 1 + 4 + 1;
    let geometry = match bytes[kind_at] {
        shader::NONE => return kind_at + 1 - at,
        shader::LINEAR => 16,
        shader::RADIAL => 12,
        shader::SWEEP => 8,
        other => panic!("not a shader kind: {other}"),
    };
    let colors_at = kind_at + 2 + geometry;
    let stops_at = colors_at + 4 + 4 * count_at(colors_at);
    let alpha_at = stops_at + 4 + 4 * count_at(stops_at);
    alpha_at + 4 - at
}

fn write_path(writer: &mut Writer, id: u32, path: &Path) {
    writer.u8(tag::DEFINE_PATH);
    writer.u32(id);
    writer.u8(match path.fill_type {
        PathFillType::NonZero => 0,
        PathFillType::EvenOdd => 1,
    });
    writer.u32(path.commands().len() as u32);
    for command in path.commands() {
        match command {
            PathCommand::MoveTo(point) => {
                writer.u8(command::MOVE_TO);
                writer.offset(*point);
            }
            PathCommand::LineTo(point) => {
                writer.u8(command::LINE_TO);
                writer.offset(*point);
            }
            PathCommand::QuadTo(control, end) => {
                writer.u8(command::QUAD_TO);
                writer.offset(*control);
                writer.offset(*end);
            }
            PathCommand::CubicTo(first, second, end) => {
                writer.u8(command::CUBIC_TO);
                writer.offset(*first);
                writer.offset(*second);
                writer.offset(*end);
            }
            PathCommand::Close => writer.u8(command::CLOSE),
        }
    }
}

/// Whether the layer opening at `start` (a `SaveLayerAlpha`) holds one drawing with a paint,
/// among saves, clips and moves: its alpha can go into that paint (its color's, or its
/// gradient's: [`Writer::paint`]), without an offscreen layer (a GPU pass each frame on
/// Android). Returns the index of its closing `Restore` if so.
fn foldable_layer(ops: &[DrawOp], start: usize) -> Option<usize> {
    single_drawing(&ops[start + 1..]).map(|end| start + 1 + end).filter(|end| *end < ops.len())
}

/// Whether `ops`, what an alpha layer holds (from just after its `SaveLayerAlpha`), are one
/// drawing with a paint among saves, clips and moves: returns the index of the layer's closing
/// `Restore` if so, or `ops.len()` if they end first, balanced (a group's content, its layer's
/// restore after it).
fn single_drawing(ops: &[DrawOp]) -> Option<usize> {
    let mut depth = 0usize;
    let mut drawings = 0;
    for (index, op) in ops.iter().enumerate() {
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha(_) => {
                if matches!(op, DrawOp::SaveLayerAlpha(_)) {
                    return None;
                }
                depth += 1;
            }
            DrawOp::Restore if depth == 0 => return (drawings == 1).then_some(index),
            DrawOp::Restore => depth -= 1,
            DrawOp::Translate(_)
            | DrawOp::ClipRect(_)
            | DrawOp::ClipRoundRect(..)
            | DrawOp::ClipPath { .. }
            | DrawOp::Transform { .. } => {}
            DrawOp::DrawRect(..)
            | DrawOp::DrawRoundRect(..)
            | DrawOp::DrawLine(..)
            | DrawOp::DrawArc { .. }
            | DrawOp::DrawPath { .. } => {
                drawings += 1;
                if drawings > 1 {
                    return None;
                }
            }
            // Text, images and shadows draw what a paint's alpha doesn't reach.
            DrawOp::DrawParagraph(..) | DrawOp::DrawImage { .. } | DrawOp::DrawShadow { .. } => {
                return None;
            }
        }
    }
    (drawings == 1 && depth == 0).then_some(ops.len())
}

/// `color` with its alpha multiplied by `alpha`.
fn faded(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return color;
    }
    let argb = color.argb();
    let faded = ((argb >> 24) as f32 * alpha.clamp(0.0, 1.0)).round() as u32;
    Color::from_argb((faded << 24) | (argb & 0x00FF_FFFF))
}

/// Each group's record (its `GROUP` header and content) as last written, by key: a group whose
/// version is the same is copied instead of written again (D157). Its bytes hold its nested
/// groups' records and no layer's properties (they are the `LAYERS` section's), so a key and a
/// version always give the same bytes (`DrawGroup`: a key with the same version draws the same;
/// the paths and images it draws keep their ids while it holds them).
#[derive(Default)]
pub struct GroupCache {
    records: HashMap<u64, CachedGroup>,
    /// The current write: a record neither written nor copied by it is dropped after it.
    generation: u64,
    /// How many records the last write copied, not counting the ones inside them.
    copied: usize,
    /// How many records the last write wrote afresh.
    written: usize,
    /// How many of those were the bytes cached already (only nested groups changed).
    unchanged: usize,
    /// The groups whose layers are still to be listed (`write_layers`), kept for its capacity.
    open: Vec<usize>,
    /// The keys of the records written ahead (`write_groups_ahead`) that no frame wrote or
    /// copied since, by item (one write's), oldest first: kept until a frame does,
    /// `AHEAD_CAPACITY` items at most.
    ahead: Vec<Vec<u64>>,
    /// How many records the last write copied that were written ahead.
    copied_ahead: usize,
}

#[derive(Default)]
struct CachedGroup {
    /// The group's version in the display list.
    version: u64,
    /// The version its record carries: the display list's when its bytes last changed (a
    /// nested group's change gives the group a new version, but leaves its bytes as they were).
    written_version: u64,
    bytes: Vec<u8>,
    generation: u64,
    /// Its content is one drawing with a paint (`single_drawing`): an alpha layer around it
    /// fades that paint, not overlapping.
    single_drawing: bool,
    /// Written ahead, and no frame wrote or copied it since: kept though the last frame didn't.
    ahead: bool,
}

impl GroupCache {
    /// How many groups' records it holds.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// How many records the last write copied, not counting the ones inside them.
    pub fn copied(&self) -> usize {
        self.copied
    }

    /// How many records the last write wrote afresh.
    pub fn written(&self) -> usize {
        self.written
    }

    /// How many of the records written afresh were the bytes cached (with their version): only
    /// a nested group changed, so Kotlin keeps their recordings.
    pub fn unchanged(&self) -> usize {
        self.unchanged
    }

    /// How many records the last write copied that were written ahead (`write_groups_ahead`).
    pub fn copied_ahead(&self) -> usize {
        self.copied_ahead
    }

    /// The version of the record cached for `key`, if any.
    pub fn cached_version(&self, key: u64) -> Option<u64> {
        self.records.get(&key).map(|record| record.version)
    }

    /// How many records written ahead wait for a frame.
    pub fn waiting_ahead(&self) -> usize {
        self.ahead.iter().map(Vec::len).sum()
    }

    /// How many items' records written ahead wait for a frame.
    pub fn items_waiting_ahead(&self) -> usize {
        self.ahead.len()
    }
}

/// The most items whose records written ahead wait for a frame (`write_groups_ahead`): a lazy
/// list's prefetched items (two ahead), with room for items scrolled away before any frame
/// showed them (their records then go oldest item first, an item's groups together however
/// many it has).
pub const AHEAD_CAPACITY: usize = 8;

/// The bytes of one entry of the `LAYERS` section.
pub const LAYER_ENTRY_BYTES: usize = 8 + 8 * 4 + 1 + 4 * 4 + 1 + 4 * 4;

/// The bytes of one entry of the `KEPT` section: a key, whether it is outermost.
pub const KEPT_ENTRY_BYTES: usize = 8 + 1;

/// Writes `list` into `out` (cleared first), defining in `resources` the paths and images it
/// uses, and copying from `cache` the records of the groups whose version it holds.
///
/// The bytes are a `LAYERS` section (if a group has a layer), then each group's `GROUP` record
/// (outer before inner), a `KEPT` section naming the groups kept (`DisplayList::kept`, if any:
/// their records stay cached, written only if their version isn't the one cached), then the
/// frame's operations; where a boundary is, a `GROUP_REF` names its group. A record leaves out
/// its layer's operations: its layer is its `LAYERS` entry, which a replayer keeping a render
/// node per group sets on that node each frame. A cached group's record is copied without
/// reading its operations (D270): only groups drawn again are walked.
pub fn write_display_list(
    list: &DisplayList,
    resources: &mut Resources,
    cache: &mut GroupCache,
    out: &mut Writer,
) {
    out.bytes.clear();
    cache.generation += 1;
    cache.copied = 0;
    cache.copied_ahead = 0;
    cache.written = 0;
    cache.unchanged = 0;
    write_layers(&list.groups, cache, out);
    let mut serializer =
        Serializer { groups: &list.kept, folds: Vec::new(), resources, cache, out, ahead: false };
    serializer.write_groups_and_kept(&list.groups);
    serializer.groups = &list.groups;
    serializer.write_recording(list, 0);
    let generation = cache.generation;
    // Records written ahead wait for the frame placing their group.
    cache.records.retain(|_, record| record.generation == generation || record.ahead);
    let records = &cache.records;
    for item in &mut cache.ahead {
        item.retain(|key| records.get(key).is_some_and(|record| record.ahead));
    }
    cache.ahead.retain(|item| !item.is_empty());
}

/// Writes into `out` (cleared first) the records of `groups`, a repaint boundary drawn ahead of
/// the frame placing it (`UiHost::draw_ahead`: its group, then those nested in it), for the
/// replayer to record their render nodes now: a `LAYERS` section for those with a layer, then
/// each one's `GROUP` record, as a frame writes them. They're cached as written ahead, kept
/// past frames that don't draw them, so the frame placing the boundary copies them, versions
/// unchanged (a key and a version are the same bytes in both, D157), and the replayer finds
/// their nodes recorded. At most `AHEAD_CAPACITY` items' records wait, the oldest item's dropped
/// first (a frame then writes them again, the same bytes).
pub fn write_groups_ahead(
    groups: &[DrawGroup],
    resources: &mut Resources,
    cache: &mut GroupCache,
    out: &mut Writer,
) {
    out.bytes.clear();
    cache.copied = 0;
    cache.copied_ahead = 0;
    cache.written = 0;
    cache.unchanged = 0;
    write_layers(groups, cache, out);
    cache.ahead.push(Vec::new());
    let mut serializer =
        Serializer { groups, folds: Vec::new(), resources, cache, out, ahead: true };
    serializer.write_groups();
    if cache.ahead.last().is_some_and(Vec::is_empty) {
        cache.ahead.pop();
    }
    while cache.ahead.len() > AHEAD_CAPACITY {
        for key in cache.ahead.remove(0) {
            if cache.records.get(&key).is_some_and(|record| record.ahead) {
                cache.records.remove(&key);
            }
        }
    }
}

/// The `LAYERS` section: every group's layer, if one has one, each after the ones nested in it
/// (as drawing closes them).
fn write_layers(groups: &[DrawGroup], cache: &mut GroupCache, out: &mut Writer) {
    let layered = |group: &DrawGroup| !group.empty && group.layer.is_some();
    let count = groups.iter().filter(|group| layered(group)).count();
    if count == 0 {
        return;
    }
    out.u8(tag::LAYERS);
    out.u32(count as u32);
    let mut open = std::mem::take(&mut cache.open);
    for index in 0..=groups.len() {
        // The groups closed before this one, innermost first.
        while let Some(&last) = open.last()
            && (index == groups.len() || last + groups[last].descendants < index)
        {
            open.pop();
            write_layer(groups, last, cache, out);
        }
        if groups.get(index).is_some_and(layered) {
            open.push(index);
        }
    }
    cache.open = open;
}

/// The `LAYERS` entry of the group at `index` of `groups`.
fn write_layer(groups: &[DrawGroup], index: usize, cache: &GroupCache, out: &mut Writer) {
    let group = &groups[index];
    let layer = group.layer.expect("a layer");
    out.u32(group.key as u32);
    out.u32((group.key >> 32) as u32);
    out.f32(layer.alpha);
    out.offset(layer.pivot);
    out.f32(layer.scale.0);
    out.f32(layer.scale.1);
    out.f32(layer.rotation);
    out.offset(layer.translation);
    out.u8(u8::from(layer.clip.is_some()));
    out.rect(&layer.clip.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)));
    out.u8(u8::from(overlapping(groups, index, cache)));
    out.rect(&layer.bounds);
}

/// Whether the alpha (below 1) of the group at `index` must composite its content offscreen:
/// unless it is one drawing with a paint and no group (as a folded layer, D153), which a render
/// node fades without a layer (`setHasOverlappingRendering(false)`). Read from its cached
/// record while its version stays.
fn overlapping(groups: &[DrawGroup], index: usize, cache: &GroupCache) -> bool {
    let group = &groups[index];
    if group.layer.is_none_or(|layer| layer.alpha >= 1.0) {
        return true;
    }
    let nested = &groups[index + 1..=index + group.descendants];
    if nested.iter().any(|inner| !inner.empty) {
        return true;
    }
    let single = match cache.records.get(&group.key) {
        Some(record) if record.version == group.version => record.single_drawing,
        _ => single_drawing(&group.content.ops).is_some(),
    };
    !single
}

/// One write of a display list.
struct Serializer<'a> {
    /// The frame's groups, each before the ones nested in it.
    groups: &'a [DrawGroup],
    /// Layers folded into their one drawing's paint: where each ends (in the recording being
    /// written), and its alpha.
    folds: Vec<(usize, f32)>,
    resources: &'a mut Resources,
    cache: &'a mut GroupCache,
    out: &'a mut Writer,
    /// Writing groups drawn ahead (`write_groups_ahead`), not a frame.
    ahead: bool,
}

impl<'a> Serializer<'a> {
    /// Writes the records of `frame`'s groups that draw, then the `KEPT` section naming the kept
    /// groups that draw (its `groups`): their records stay cached, written only if their version
    /// isn't the one cached (a nested group drawn again since), so the replayer records them
    /// now.
    fn write_groups_and_kept(&mut self, frame: &'a [DrawGroup]) {
        let kept = self.groups;
        self.groups = frame;
        self.write_groups();
        self.groups = kept;
        let generation = self.cache.generation;
        let mut count = 0;
        for (index, group) in kept.iter().enumerate() {
            if group.empty {
                continue;
            }
            count += 1;
            match self.cache.records.get_mut(&group.key) {
                Some(record) if record.version == group.version => {
                    record.generation = generation;
                    record.ahead = false;
                }
                _ => self.write_group(index),
            }
        }
        if count == 0 {
            return;
        }
        self.out.u8(tag::KEPT);
        self.out.u32(count as u32);
        let mut nested_until = 0;
        for (index, group) in kept.iter().enumerate() {
            if group.empty {
                continue;
            }
            self.out.u32(group.key as u32);
            self.out.u32((group.key >> 32) as u32);
            self.out.u8(u8::from(index >= nested_until));
            if index >= nested_until {
                nested_until = index + 1 + group.descendants;
            }
        }
    }

    /// Writes the record of each group that draws, outer before inner.
    fn write_groups(&mut self) {
        let mut index = 0;
        while let Some(group) = self.groups.get(index) {
            if group.empty {
                // Nothing drawn in it either.
                index += 1 + group.descendants;
            } else {
                self.write_group(index);
                index += 1;
            }
        }
    }

    /// Writes `recording` (the frame, or a group's content), each group drawn in it named where
    /// its boundary is; those groups are the ones from `child` on, siblings. Its paints'
    /// shaders are its own (written whole into each paint, D210).
    fn write_recording(&mut self, recording: &DisplayList, mut child: usize) {
        let ops = &recording.ops;
        let boundaries = &recording.boundaries;
        let mut boundary = 0;
        for index in 0..ops.len() {
            while boundaries.get(boundary).is_some_and(|boundary| boundary.at <= index) {
                child = self.write_reference(boundaries[boundary].key, child);
                boundary += 1;
            }
            while self.folds.last().is_some_and(|(end, _)| *end < index) {
                self.folds.pop();
            }
            // A layer holding one drawing and no group: a folded alpha inside a group's record
            // would change it without changing its version.
            if let DrawOp::SaveLayerAlpha(alpha) = ops[index]
                && let Some(end) = foldable_layer(ops, index)
                && !self.draws_before(&boundaries[boundary..], end, child)
            {
                self.folds.push((end, alpha));
                self.out.u8(tag::SAVE);
                continue;
            }
            let alpha: f32 = self.folds.iter().map(|(_, alpha)| alpha).product();
            write_op(&ops[index], alpha, &recording.shaders, self.resources, self.out);
        }
        for rest in &boundaries[boundary..] {
            child = self.write_reference(rest.key, child);
        }
    }

    /// Names the group at `child`, the boundary of `key`, unless it draws nothing; returns its
    /// next sibling's index.
    fn write_reference(&mut self, key: u64, child: usize) -> usize {
        let group = self.groups.get(child);
        debug_assert!(
            group.is_some_and(|group| group.key == key),
            "the frame's groups don't match its boundaries"
        );
        let Some(group) = group else { return child };
        if !group.empty {
            self.out.u8(tag::GROUP_REF);
            self.out.u32(key as u32);
            self.out.u32((key >> 32) as u32);
        }
        child + 1 + group.descendants
    }

    /// Whether a group that draws is drawn at one of `boundaries` (the next ones, the first
    /// one's group at `child`) before operation `end`.
    fn draws_before(
        &self,
        boundaries: &[compose_ui::platform::Boundary],
        end: usize,
        child: usize,
    ) -> bool {
        let inside = boundaries.iter().take_while(|boundary| boundary.at <= end).count();
        let mut next = child;
        for _ in 0..inside {
            let Some(group) = self.groups.get(next) else { return false };
            if !group.empty {
                return true;
            }
            next += 1 + group.descendants;
        }
        false
    }

    /// Writes the record of the group at `index`: copied if the cache has its version (its
    /// operations untouched), else its header and its content (its layer's operations left
    /// out, the groups in it named). Written again with the bytes cached (a nested group
    /// changed), it keeps the version they were cached with: the replayer keeps its recording.
    fn write_group(&mut self, index: usize) {
        let group = &self.groups[index];
        let generation = self.cache.generation;
        if let Some(record) = self.cache.records.get_mut(&group.key)
            && record.version == group.version
        {
            record.generation = generation;
            self.out.bytes.extend_from_slice(&record.bytes);
            self.cache.copied += 1;
            if !self.ahead && record.ahead {
                self.cache.copied_ahead += 1;
            }
            self.mark(group.key);
            return;
        }
        self.cache.written += 1;
        let at = self.out.bytes.len();
        self.out.u8(tag::GROUP);
        self.out.u32(group.key as u32);
        self.out.u32((group.key >> 32) as u32);
        let version_at = self.out.bytes.len();
        self.out.u32(group.version as u32);
        self.out.u32((group.version >> 32) as u32);
        let length_at = self.out.bytes.len();
        self.out.u32(0);
        // No fold spans a group's edge: the ones open here end before it.
        let folds = std::mem::take(&mut self.folds);
        self.write_recording(&group.content, index + 1);
        self.folds = folds;
        let length = (self.out.bytes.len() - length_at - 4) as u32;
        self.out.bytes[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
        let record = self.cache.records.entry(group.key).or_default();
        let content = length_at + 4;
        let unchanged = !record.bytes.is_empty()
            && record.bytes.len() == self.out.bytes.len() - at
            && record.bytes[content - at..] == self.out.bytes[content..];
        if unchanged {
            self.cache.unchanged += 1;
            let version = record.written_version;
            self.out.bytes[version_at..version_at + 4]
                .copy_from_slice(&(version as u32).to_le_bytes());
            self.out.bytes[version_at + 4..version_at + 8]
                .copy_from_slice(&((version >> 32) as u32).to_le_bytes());
        } else {
            record.written_version = group.version;
            record.bytes.clear();
            record.bytes.extend_from_slice(&self.out.bytes[at..]);
        }
        record.version = group.version;
        record.generation = generation;
        // Of the display list's operations, which bytes alone don't tell (a folded layer's).
        record.single_drawing = single_drawing(&group.content.ops).is_some();
        self.mark(group.key);
    }

    /// Notes the record of `key`, just written or copied, as written ahead or by a frame.
    fn mark(&mut self, key: u64) {
        let Some(record) = self.cache.records.get_mut(&key) else { return };
        if self.ahead {
            if !record.ahead {
                record.ahead = true;
                self.cache.ahead.last_mut().expect("an item being written").push(key);
            }
        } else {
            record.ahead = false;
        }
    }
}

/// Writes `op`, its paint faded by `alpha` (the folded layers around it), its shader from
/// `shaders`.
fn write_op(
    op: &DrawOp,
    alpha: f32,
    shaders: &[Shader],
    resources: &mut Resources,
    out: &mut Writer,
) {
    match op {
        DrawOp::Save => out.u8(tag::SAVE),
        DrawOp::Restore => out.u8(tag::RESTORE),
        DrawOp::Translate(offset) => {
            out.u8(tag::TRANSLATE);
            out.offset(*offset);
        }
        DrawOp::ClipRect(rect) => {
            out.u8(tag::CLIP_RECT);
            out.rect(rect);
        }
        DrawOp::ClipRoundRect(rect, radii) => {
            out.u8(tag::CLIP_ROUND_RECT);
            out.rect(rect);
            out.radii(radii);
        }
        DrawOp::DrawRect(rect, paint) => {
            out.u8(tag::DRAW_RECT);
            out.rect(rect);
            out.paint(paint, shaders, alpha);
        }
        DrawOp::DrawRoundRect(rect, radii, paint) => {
            out.u8(tag::DRAW_ROUND_RECT);
            out.rect(rect);
            out.radii(radii);
            out.paint(paint, shaders, alpha);
        }
        DrawOp::DrawLine(from, to, paint) => {
            out.u8(tag::DRAW_LINE);
            out.offset(*from);
            out.offset(*to);
            out.paint(paint, shaders, alpha);
        }
        DrawOp::DrawParagraph(handle, offset) => {
            out.u8(tag::DRAW_PARAGRAPH);
            out.u32(handle.0 as u32);
            out.offset(*offset);
        }
        DrawOp::SaveLayerAlpha(alpha) => {
            out.u8(tag::SAVE_LAYER_ALPHA);
            out.f32(*alpha);
        }
        DrawOp::Transform { pivot, scale, rotation, translation } => {
            out.u8(tag::TRANSFORM);
            out.offset(*pivot);
            out.f32(scale.0);
            out.f32(scale.1);
            out.f32(*rotation);
            out.offset(*translation);
        }
        DrawOp::DrawImage { image, src, dst, alpha } => {
            let id = resources.image_id(image);
            out.u8(tag::DRAW_IMAGE);
            out.u32(id);
            out.rect(src);
            out.rect(dst);
            out.f32(*alpha);
        }
        DrawOp::DrawShadow { outline, elevation, light, light_radius, ambient, spot } => {
            out.u8(tag::DRAW_SHADOW);
            out.outline(outline);
            out.f32(*elevation);
            out.f32(light.0);
            out.f32(light.1);
            out.f32(light.2);
            out.f32(*light_radius);
            out.color(*ambient);
            out.color(*spot);
        }
        DrawOp::DrawArc { oval, start_angle, sweep_angle, use_center, paint } => {
            out.u8(tag::DRAW_ARC);
            out.rect(oval);
            out.f32(*start_angle);
            out.f32(*sweep_angle);
            out.u8(u8::from(*use_center));
            out.paint(paint, shaders, alpha);
        }
        DrawOp::DrawPath { path, offset, scale, paint } => {
            let id = resources.path_id(path);
            out.u8(tag::DRAW_PATH);
            out.u32(id);
            out.offset(*offset);
            out.f32(scale.0);
            out.f32(scale.1);
            out.paint(paint, shaders, alpha);
        }
        DrawOp::ClipPath { path, offset } => {
            let id = resources.path_id(path);
            out.u8(tag::CLIP_PATH);
            out.u32(id);
            out.offset(*offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::platform::Gradient;

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// Draws the group of `key` at the end of `recording`.
    fn boundary(recording: &mut DisplayList, key: u64) {
        recording.boundaries.push(compose_ui::platform::Boundary { at: recording.ops.len(), key });
    }

    /// A group of `key` and `version` drawing `content`, the next `descendants` groups nested
    /// in it.
    fn group(key: u64, version: u64, content: DisplayList, descendants: usize) -> DrawGroup {
        DrawGroup {
            key,
            version,
            layer: None,
            empty: content.ops.is_empty() && descendants == 0,
            content: Arc::new(content),
            descendants,
        }
    }

    #[test]
    fn a_rect_is_its_tag_bounds_and_paint() {
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawRect(Rect::new(1.0, 2.0, 3.0, 4.0), Paint::fill(Color::BLACK)));
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        // Tag, 4 floats, color, style, width, cap, no shader.
        assert_eq!(out.bytes.len(), 1 + 16 + 4 + 1 + 4 + 1 + 1);
        assert_eq!(out.bytes[0], tag::DRAW_RECT);
        assert_eq!(f32::from_le_bytes(out.bytes[13..17].try_into().unwrap()), 4.0);
        assert_eq!(read_u32(&out.bytes, 17), 0xFF00_0000);
    }

    #[test]
    fn a_layer_with_one_drawing_fades_its_paint_instead() {
        let mut list = DisplayList::default();
        list.push(DrawOp::SaveLayerAlpha(0.5));
        list.push(DrawOp::ClipRect(Rect::new(0.0, 0.0, 4.0, 4.0)));
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        list.push(DrawOp::Restore);
        // After it, the paint is whole again.
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert_eq!(out.bytes[0], tag::SAVE);
        // Save, clip (17 bytes), then the rect: its color at 1 + 17 + 1 + 16.
        assert_eq!(read_u32(&out.bytes, 35), 0x8000_0000);
        let last = out.bytes.len() - 28;
        assert_eq!(out.bytes[last], tag::DRAW_RECT);
        assert_eq!(read_u32(&out.bytes, last + 17), 0xFF00_0000);
        // Two drawings: a layer, as their overlap must fade together.
        list.ops
            .insert(3, DrawOp::DrawRect(Rect::new(1.0, 1.0, 2.0, 2.0), Paint::fill(Color::WHITE)));
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert_eq!(out.bytes[0], tag::SAVE_LAYER_ALPHA);
    }

    /// Each group is a record, before the frame's operations and the records of the groups in
    /// it, which it names where they're drawn (D323).
    #[test]
    fn groups_are_records_named_where_drawn() {
        let rect = || DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK));
        // The outer group holds a rect, the inner one, an empty one (left out), and a rect.
        let mut outer = DisplayList::default();
        outer.push(rect());
        boundary(&mut outer, 2);
        boundary(&mut outer, 3);
        outer.push(rect());
        let mut inner = DisplayList::default();
        inner.push(rect());
        let mut list = DisplayList::default();
        boundary(&mut list, 1 << 40 | 1);
        list.groups = vec![
            group(1 << 40 | 1, 0, outer, 2),
            group(2, 7, inner, 0),
            group(3, 0, DisplayList::default(), 0),
        ];
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let rect_bytes = 1 + 16 + 4 + 1 + 4 + 1 + 1;
        let (header, reference) = (1 + 8 + 8 + 4, 1 + 8);
        let bytes = &out.bytes;
        assert_eq!(bytes.len(), 2 * header + 3 * rect_bytes + 2 * reference);
        assert_eq!(bytes[0], tag::GROUP);
        assert_eq!((read_u32(bytes, 1), read_u32(bytes, 5)), (1, 1 << 8));
        assert_eq!(read_u32(bytes, 17) as usize, 2 * rect_bytes + reference);
        let named = header + rect_bytes;
        assert_eq!((bytes[named], read_u32(bytes, named + 1)), (tag::GROUP_REF, 2));
        assert_eq!(bytes[named + reference], tag::DRAW_RECT);
        let inner = header + 2 * rect_bytes + reference;
        assert_eq!(bytes[inner], tag::GROUP);
        assert_eq!(read_u32(bytes, inner + 1), 2);
        assert_eq!((read_u32(bytes, inner + 9), read_u32(bytes, inner + 13)), (7, 0), "version");
        assert_eq!(read_u32(bytes, inner + 17) as usize, rect_bytes);
        assert_eq!(bytes[inner + header], tag::DRAW_RECT);
        // Then the frame's operations: the outer group, named.
        let frame = inner + header + rect_bytes;
        assert_eq!((bytes[frame], read_u32(bytes, frame + 1)), (tag::GROUP_REF, 1));
        assert_eq!(read_u32(bytes, frame + 5), 1 << 8);
    }

    /// A frame of two nested groups, the inner one drawing a rect of `color`.
    fn nested_frame(outer: u64, inner: u64, color: Color) -> DisplayList {
        let mut content = DisplayList::default();
        content.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(color)));
        let mut around = DisplayList::default();
        around.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 2.0, 2.0), Paint::fill(Color::WHITE)));
        boundary(&mut around, 2);
        let mut list = DisplayList::default();
        boundary(&mut list, 1);
        list.groups = vec![group(1, outer, around, 1), group(2, inner, content, 0)];
        list
    }

    /// A group whose version the cache holds is its cached record: its operations aren't read
    /// (D270).
    #[test]
    fn a_cached_group_is_copied_without_reading_its_operations() {
        let (mut resources, mut cache) = (Resources::default(), GroupCache::default());
        let mut first = Writer::default();
        write_display_list(
            &nested_frame(5, 4, Color::BLACK),
            &mut resources,
            &mut cache,
            &mut first,
        );
        assert_eq!((cache.written(), cache.copied()), (2, 0));
        // The same versions drawing something else (which a tree never does): the cache's.
        let mut second = Writer::default();
        write_display_list(
            &nested_frame(5, 4, Color::WHITE),
            &mut resources,
            &mut cache,
            &mut second,
        );
        assert_eq!((cache.written(), cache.copied()), (0, 2));
        assert_eq!(second.bytes, first.bytes);
        assert_eq!(cache.len(), 2);
    }

    /// A nested group's change gives its parent a new version (D155), but leaves the parent's
    /// bytes as they were (it names the group by key): its record keeps its version, so the
    /// replayer keeps its recording (D323).
    #[test]
    fn a_group_whose_nested_group_changed_keeps_its_version() {
        let (mut resources, mut cache) = (Resources::default(), GroupCache::default());
        let mut first = Writer::default();
        write_display_list(
            &nested_frame(5, 4, Color::BLACK),
            &mut resources,
            &mut cache,
            &mut first,
        );
        let mut second = Writer::default();
        write_display_list(
            &nested_frame(9, 8, Color::RED),
            &mut resources,
            &mut cache,
            &mut second,
        );
        assert_eq!((cache.written(), cache.unchanged(), cache.copied()), (2, 1, 0));
        let version = |bytes: &[u8]| read_u32(bytes, 9);
        let inner = |bytes: &[u8]| 21 + read_u32(bytes, 17) as usize;
        assert_eq!(version(&second.bytes), 5, "the outer record's version stays");
        assert_eq!(second.bytes[..inner(&second.bytes)], first.bytes[..inner(&first.bytes)]);
        assert_eq!(version(&second.bytes[inner(&second.bytes)..]), 8, "the inner one's is new");
        // Its own content changed: a new version.
        let mut changed = nested_frame(10, 8, Color::RED);
        Arc::make_mut(&mut changed.groups[0].content).ops.clear();
        let mut third = Writer::default();
        write_display_list(&changed, &mut resources, &mut cache, &mut third);
        assert_eq!((cache.written(), cache.unchanged(), cache.copied()), (1, 0, 1));
        assert_eq!(version(&third.bytes), 10);
    }

    /// A group's record comes before those of the groups drawn in it.
    #[test]
    fn a_groups_record_comes_before_those_of_the_groups_in_it() {
        let mut inner = DisplayList::default();
        inner.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK)));
        let mut outer = DisplayList::default();
        boundary(&mut outer, 1);
        let mut list = DisplayList::default();
        boundary(&mut list, 2);
        list.groups = vec![group(2, 0, outer, 1), group(1, 0, inner, 0)];
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert_eq!((out.bytes[0], read_u32(&out.bytes, 1)), (tag::GROUP, 2));
        assert_eq!((out.bytes[21], read_u32(&out.bytes, 22)), (tag::GROUP_REF, 1));
        assert_eq!((out.bytes[30], read_u32(&out.bytes, 31)), (tag::GROUP, 1));
    }

    /// A folded layer's `Save` opens the group as any operation does.
    #[test]
    fn a_group_can_start_with_a_folded_layer() {
        let mut content = DisplayList::default();
        content.push(DrawOp::SaveLayerAlpha(0.5));
        content.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        content.push(DrawOp::Restore);
        let mut list = DisplayList::default();
        boundary(&mut list, 7);
        list.groups = vec![group(7, 0, content, 0)];
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert_eq!(out.bytes[0], tag::GROUP);
        // Its record, then the frame naming it.
        assert_eq!(read_u32(&out.bytes, 17) as usize, out.bytes.len() - 21 - 9);
        assert_eq!(out.bytes[21], tag::SAVE);
    }

    #[test]
    fn a_path_is_defined_once_and_dropped_with_its_owner() {
        let path = Arc::new(Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap());
        let mut list = DisplayList::default();
        let op = DrawOp::DrawPath {
            path: path.clone(),
            offset: Offset::ZERO,
            scale: (1.0, 1.0),
            paint: Paint::fill(Color::BLACK),
        };
        list.push(op.clone());
        list.push(op);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!(pending.bytes[0], tag::DEFINE_PATH);
        // Drawn again: nothing new to define.
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert!(!resources.has_pending());
        drop(list);
        drop(path);
        resources.collect();
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!(pending.bytes[0], tag::DROP_PATH);
    }

    /// A frame drawing a path and an image of its own, new each frame (as a canvas drawing a
    /// wave), replacing `previous` (its owners gone with it), and the pending changes after it,
    /// as `frame` and `takeDrawing` take them.
    fn frame_with_new_resources(
        resources: &mut Resources,
        previous: Option<DisplayList>,
        seed: u8,
    ) -> (DisplayList, Vec<u8>) {
        let path = Arc::new(Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap());
        let image = ImageBitmap::from_rgba_premultiplied(1, 1, vec![seed; 4]);
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawPath {
            path,
            offset: Offset::ZERO,
            scale: (1.0, 1.0),
            paint: Paint::fill(Color::BLACK),
        });
        list.push(DrawOp::DrawImage {
            image,
            src: Rect::new(0.0, 0.0, 1.0, 1.0),
            dst: Rect::new(0.0, 0.0, 1.0, 1.0),
            alpha: 1.0,
        });
        let mut out = Writer::default();
        write_display_list(&list, resources, &mut GroupCache::default(), &mut out);
        drop(previous);
        resources.collect();
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        (list, pending.bytes)
    }

    /// The tags of the resource changes in `bytes`, in order.
    fn resource_tags(bytes: &[u8]) -> Vec<u8> {
        let (mut tags, mut at) = (Vec::new(), 0);
        while at < bytes.len() {
            let tag = bytes[at];
            tags.push(tag);
            at += match tag {
                tag::DROP_PATH | tag::DROP_IMAGE => 5,
                tag::DEFINE_IMAGE => 17 + read_u32(bytes, at + 13) as usize,
                // An id, a fill type, a count, and a triangle's 4 commands (3 points, a close).
                tag::DEFINE_PATH => 10 + 3 * 9 + 1,
                other => panic!("not a resource change: {other}"),
            };
        }
        tags
    }

    /// Drops come before definitions (D410): a path replaced each frame frees its Kotlin `Path`
    /// for the replacement's definition, in the same frame.
    #[test]
    fn drops_come_before_definitions() {
        let mut resources = Resources::default();
        let (first, bytes) = frame_with_new_resources(&mut resources, None, 1);
        assert_eq!(resource_tags(&bytes), [tag::DEFINE_PATH, tag::DEFINE_IMAGE]);
        let (_second, second) = frame_with_new_resources(&mut resources, Some(first), 2);
        assert_eq!(
            resource_tags(&second),
            [tag::DROP_PATH, tag::DROP_IMAGE, tag::DEFINE_PATH, tag::DEFINE_IMAGE]
        );
    }

    fn read_f32(bytes: &[u8], at: usize) -> f32 {
        f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// The bytes before a paint's shader in a `DRAW_RECT`: tag, rect, color, style, width, cap.
    const RECT_BEFORE_SHADER: usize = 1 + 16 + 4 + 1 + 4 + 1;

    fn gradient(colors: &[Color], stops: Option<&[f32]>, tile_mode: TileMode) -> Gradient {
        Gradient { colors: colors.into(), stops: stops.map(Into::into), tile_mode }
    }

    fn gradient_rect(list: &mut DisplayList, shader: Shader) {
        let mut paint = Paint::fill(Color::BLACK);
        paint = paint.with_shader(list.push_shader(shader));
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 10.0, 10.0), paint));
    }

    #[test]
    fn a_gradient_paint_carries_its_definition_and_alpha() {
        let mut list = DisplayList::default();
        // Another shader first: the paint's index is 1, its bytes its content.
        list.push_shader(Shader::SweepGradient {
            gradient: gradient(&[Color::RED, Color::BLUE], None, TileMode::Clamp),
            center: Offset::ZERO,
            alpha: 1.0,
        });
        gradient_rect(
            &mut list,
            Shader::LinearGradient {
                gradient: gradient(
                    &[Color::RED, Color::GREEN, Color::BLUE],
                    Some(&[0.0, 0.25, 1.0]),
                    TileMode::Repeated,
                ),
                from: Offset::new(1.0, 2.0),
                to: Offset::new(3.0, 4.0),
                alpha: 0.5,
            },
        );
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        let at = RECT_BEFORE_SHADER;
        assert_eq!((bytes[at], bytes[at + 1]), (shader::LINEAR, 1), "kind, tile mode");
        let floats: Vec<f32> = (0..4).map(|index| read_f32(bytes, at + 2 + index * 4)).collect();
        assert_eq!(floats, [1.0, 2.0, 3.0, 4.0]);
        let at = at + 2 + 16;
        assert_eq!(read_u32(bytes, at), 3);
        let colors: Vec<u32> = (0..3).map(|index| read_u32(bytes, at + 4 + index * 4)).collect();
        assert_eq!(colors, [Color::RED.argb(), Color::GREEN.argb(), Color::BLUE.argb()]);
        let at = at + 4 + 12;
        assert_eq!(read_u32(bytes, at), 3);
        let stops: Vec<f32> = (0..3).map(|index| read_f32(bytes, at + 4 + index * 4)).collect();
        assert_eq!(stops, [0.0, 0.25, 1.0]);
        assert_eq!(read_f32(bytes, at + 16), 0.5, "the shader's alpha");
        assert_eq!(bytes.len(), at + 20);
    }

    /// One color is a solid color (written twice, its stop left out).
    #[test]
    fn one_color_is_written_twice() {
        let mut list = DisplayList::default();
        gradient_rect(
            &mut list,
            Shader::RadialGradient {
                gradient: gradient(&[Color::RED], Some(&[0.5]), TileMode::Decal),
                center: Offset::new(5.0, 5.0),
                radius: 2.0,
                alpha: 1.0,
            },
        );
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let at = RECT_BEFORE_SHADER;
        assert_eq!((out.bytes[at], out.bytes[at + 1]), (shader::RADIAL, 3));
        assert_eq!(read_f32(&out.bytes, at + 2 + 8), 2.0, "the radius");
        let at = at + 2 + 12;
        assert_eq!(read_u32(&out.bytes, at), 2);
        assert_eq!(read_u32(&out.bytes, at + 4), read_u32(&out.bytes, at + 8));
        assert_eq!(read_u32(&out.bytes, at + 12), 0, "no stops");
    }

    /// The color and shader kind a radial gradient of `radius` and `tile_mode` is written with,
    /// at alpha 0.5: red, then green from 0.5 on, then blue at 0.75.
    fn radial_written(radius: f32, tile_mode: TileMode) -> (Color, u8) {
        let mut list = DisplayList::default();
        let colors = [Color::RED, Color::GREEN, Color::BLUE];
        let shader = Shader::RadialGradient {
            gradient: gradient(&colors, Some(&[0.5, 0.75, 0.75]), tile_mode),
            center: Offset::new(5.0, 5.0),
            radius,
            alpha: 0.5,
        };
        let mut paint = Paint::fill(Color::from_argb(0x8011_2233));
        paint = paint.with_shader(list.push_shader(shader));
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 10.0, 10.0), paint));
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        (Color::from_argb(read_u32(&out.bytes, 17)), out.bytes[RECT_BEFORE_SHADER])
    }

    /// A radial gradient with no radius, which Android's `RadialGradient` refuses, is Skia's
    /// degenerate gradient (a solid color at the shader's alpha); a negative radius is no shader
    /// in Skia (the paint's color paints).
    #[test]
    fn a_radial_gradient_without_a_radius_is_skias_degenerate_gradient() {
        assert_eq!(radial_written(0.0, TileMode::Clamp), (Color::BLUE.with_alpha(0.5), 0));
        // Red over half, red to green over a quarter, green to blue at once, blue past it:
        // (0.5 + 0.125) of red, 0.125 of green, 0.25 of blue.
        let average = Color::from_argb(0x809F_2040);
        assert_eq!(radial_written(0.0, TileMode::Repeated), (average, 0));
        assert_eq!(radial_written(1e-6, TileMode::Mirror), (average, 0));
        assert_eq!(radial_written(0.0, TileMode::Decal), (Color::TRANSPARENT, 0));
        assert_eq!(radial_written(-1.0, TileMode::Clamp), (Color::from_argb(0x8011_2233), 0));
        assert_eq!(radial_written(1.0, TileMode::Clamp).1, shader::RADIAL);
    }

    /// A layer folded into a gradient's paint multiplies the shader's alpha (what Android's
    /// paint alpha is with a shader), not only the color's.
    #[test]
    fn a_folded_layer_fades_a_shaders_alpha() {
        let mut list = DisplayList::default();
        list.push(DrawOp::SaveLayerAlpha(0.5));
        gradient_rect(
            &mut list,
            Shader::SweepGradient {
                gradient: gradient(&[Color::RED, Color::BLUE], None, TileMode::Clamp),
                center: Offset::new(5.0, 5.0),
                alpha: 0.8,
            },
        );
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert_eq!(out.bytes[0], tag::SAVE, "folded");
        // Save, then the rect; its sweep: kind, tile, center, 2 colors, no stops, alpha.
        let at = 1 + RECT_BEFORE_SHADER;
        assert_eq!(out.bytes[at], shader::SWEEP);
        let alpha_at = at + 2 + 8 + 4 + 8 + 4;
        assert!((read_f32(&out.bytes, alpha_at) - 0.4).abs() < 1e-6);
        assert_eq!(out.bytes[alpha_at + 4], tag::RESTORE);
    }

    /// A clip path in a group's record names its path, defined beside the frame, and its offset.
    #[test]
    fn a_clip_path_names_its_path_and_offset() {
        let path = Arc::new(Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap());
        let mut content = DisplayList::default();
        content.push(DrawOp::Save);
        content.push(DrawOp::ClipPath { path: path.clone(), offset: Offset::new(3.0, 4.0) });
        content.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        content.push(DrawOp::Restore);
        let mut list = DisplayList::default();
        boundary(&mut list, 1);
        list.groups = vec![group(1, 0, content, 0)];
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        assert_eq!((bytes[0], bytes[21], bytes[22]), (tag::GROUP, tag::SAVE, tag::CLIP_PATH));
        let id = read_u32(bytes, 23);
        assert_eq!((read_f32(bytes, 27), read_f32(bytes, 31)), (3.0, 4.0));
        assert_eq!(bytes[35], tag::DRAW_RECT);
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!((pending.bytes[0], read_u32(&pending.bytes, 1)), (tag::DEFINE_PATH, id));
    }
}
