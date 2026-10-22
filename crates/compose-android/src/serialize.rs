//! The display list as bytes for the Kotlin replayer (docs/07-platforms.md, "Drawing without
//! shipping Skia"): one buffer per frame, read on the Kotlin side with a little-endian
//! `ByteBuffer`, each operation a tag byte and its operands.
//!
//! Paths and images are resources: defined once (a `DefinePath` or `DefineImage` before their
//! first use) and dropped when the last frame using them is gone, so a frame names them by id
//! (drawn with `DRAW_PATH`, clipped to with `CLIP_PATH`). A gradient isn't a resource: a paint
//! with a shader carries its whole definition (D210), as a shader's index is the list's (D175)
//! and changes when a retained recording is spliced at another place, while a group's cached
//! bytes must not.
//! Their definitions wait until the Kotlin side takes them with the frame it draws, so a frame
//! drawn twice or skipped loses none; one dropped before it was taken is cancelled.
//!
//! A repaint boundary's group is a `GROUP` record carrying its key and version, which the
//! replayer keeps a render node by; a graphics layer's properties are in a `LAYERS` section
//! before the operations, not in any record, so a key and a version are always the same bytes,
//! copied from a [`GroupCache`] while the version stays (D157).

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
    /// bytes (u32), then its content (its layer's operations left out). The same key and
    /// version are always the same bytes: the replayer keeps the group's render node by them.
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
    /// skipped), and forgets them: the Kotlin side has them.
    pub fn write_pending(&mut self, out: &mut Writer) {
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
                Pending::DropPath(id) => {
                    out.u8(tag::DROP_PATH);
                    out.u32(id);
                }
                Pending::DropImage(id) => {
                    out.u8(tag::DROP_IMAGE);
                    out.u32(id);
                }
            }
        }
    }
}

/// The operand bytes of the operation whose tag is at `at` (not a `GROUP`, `LAYERS` or resource
/// change), as `Replayer.kt` reads them: for tests and tools reading a frame back.
pub fn operand_bytes(bytes: &[u8], at: usize) -> usize {
    // The paint is the last operand, after `fixed` bytes.
    let painted = |fixed: usize| fixed + paint_bytes(bytes, at + 1 + fixed);
    match bytes[at] {
        tag::SAVE | tag::RESTORE => 0,
        tag::TRANSLATE => 8,
        tag::CLIP_RECT => 16,
        tag::CLIP_ROUND_RECT => 32,
        tag::CLIP_PATH => 4 + 8,
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
    let mut depth = 0usize;
    let mut drawings = 0;
    for (index, op) in ops.iter().enumerate().skip(start + 1) {
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
            | DrawOp::DrawPath { .. } => drawings += 1,
            // Text, images and shadows draw what a paint's alpha doesn't reach.
            DrawOp::DrawParagraph(..) | DrawOp::DrawImage { .. } | DrawOp::DrawShadow { .. } => {
                return None;
            }
        }
    }
    None
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
}

#[derive(Default)]
struct CachedGroup {
    version: u64,
    bytes: Vec<u8>,
    generation: u64,
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

    /// How many records the last write wrote afresh (a new version each: Kotlin records them).
    pub fn written(&self) -> usize {
        self.written
    }
}

/// The bytes of one entry of the `LAYERS` section.
pub const LAYER_ENTRY_BYTES: usize = 8 + 8 * 4 + 1 + 4 * 4 + 1 + 4 * 4;

/// Writes `list` into `out` (cleared first), defining in `resources` the paths and images it
/// uses, and copying from `cache` the records of the groups whose version it holds.
///
/// The bytes are a `LAYERS` section (if a group has a layer), then the operations, each group a
/// `GROUP` record in place of its operations, its layer's operations (`layer_start..start`,
/// `end..layer_end`) left out: its layer is its `LAYERS` entry, which a replayer keeping a
/// render node per group sets on that node each frame, nested groups' included.
pub fn write_display_list(
    list: &DisplayList,
    resources: &mut Resources,
    cache: &mut GroupCache,
    out: &mut Writer,
) {
    out.bytes.clear();
    cache.generation += 1;
    cache.copied = 0;
    cache.written = 0;
    write_layers(list, out);
    // The groups, outer before inner (they nest): of two with the same range, the outer one
    // was drawn last (a stable sort of the reversed list keeps it first).
    let mut groups: Vec<&DrawGroup> =
        list.groups.iter().rev().filter(|group| group.start < group.end).collect();
    groups.sort_by_key(|group| (group.layer_start, std::cmp::Reverse(group.layer_end)));
    let mut serializer = Serializer {
        ops: &list.ops,
        shaders: &list.shaders,
        groups: groups.into_iter().peekable(),
        folds: Vec::new(),
        resources,
        cache,
        out,
    };
    serializer.write_range(0, list.ops.len());
    let generation = cache.generation;
    cache.records.retain(|_, record| record.generation == generation);
}

/// The `LAYERS` section: every group's layer, if one has one.
fn write_layers(list: &DisplayList, out: &mut Writer) {
    let layered = |group: &&DrawGroup| group.start < group.end && group.layer.is_some();
    let count = list.groups.iter().filter(layered).count();
    if count == 0 {
        return;
    }
    out.u8(tag::LAYERS);
    out.u32(count as u32);
    for group in list.groups.iter().filter(layered) {
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
        out.u8(u8::from(overlapping(list, group)));
        out.rect(&layer.bounds);
    }
}

/// Whether `group`'s alpha (below 1) must composite its content offscreen: unless it is one
/// drawing with a paint and no group (as a folded layer, D153), which a render node fades
/// without a layer (`setHasOverlappingRendering(false)`).
fn overlapping(list: &DisplayList, group: &DrawGroup) -> bool {
    // Its alpha's layer is the last of its layer's operations.
    let Some(alpha_layer) = group.start.checked_sub(1).filter(|at| *at >= group.layer_start) else {
        return true;
    };
    if !matches!(list.ops[alpha_layer], DrawOp::SaveLayerAlpha(_))
        || foldable_layer(&list.ops, alpha_layer).is_none()
    {
        return true;
    }
    list.groups.iter().any(|inner| {
        inner.key != group.key
            && inner.start < inner.end
            && group.start <= inner.layer_start
            && inner.layer_end <= group.end
    })
}

/// One write of a display list.
struct Serializer<'a> {
    ops: &'a [DrawOp],
    /// The shaders the paints name (written whole into each paint, D210).
    shaders: &'a [Shader],
    /// The groups not written yet, by where they start.
    groups: std::iter::Peekable<std::vec::IntoIter<&'a DrawGroup>>,
    /// Layers folded into their one drawing's paint: where each ends, and its alpha.
    folds: Vec<(usize, f32)>,
    resources: &'a mut Resources,
    cache: &'a mut GroupCache,
    out: &'a mut Writer,
}

impl Serializer<'_> {
    /// Writes `ops[from..to]`, each group in it as its record.
    fn write_range(&mut self, from: usize, to: usize) {
        let mut index = from;
        while index < to {
            while self.folds.last().is_some_and(|(end, _)| *end < index) {
                self.folds.pop();
            }
            if let Some(group) = self.groups.next_if(|group| group.layer_start <= index) {
                debug_assert_eq!(group.layer_start, index, "groups nest");
                self.write_group(group);
                index = group.layer_end.max(index + 1);
                continue;
            }
            // A layer holding one drawing and no group: a folded alpha inside a group's record
            // would change it without changing its version.
            if let DrawOp::SaveLayerAlpha(alpha) = self.ops[index]
                && let Some(end) = foldable_layer(self.ops, index)
                && self.groups.peek().is_none_or(|group| group.layer_start > end)
            {
                self.folds.push((end, alpha));
                self.out.u8(tag::SAVE);
                index += 1;
                continue;
            }
            let alpha: f32 = self.folds.iter().map(|(_, alpha)| alpha).product();
            write_op(&self.ops[index], alpha, self.shaders, self.resources, self.out);
            index += 1;
        }
    }

    /// Writes `group`'s record: copied if the cache has its version, else its header and its
    /// content (`start..end`: its layer's operations left out).
    fn write_group(&mut self, group: &DrawGroup) {
        let generation = self.cache.generation;
        if let Some(record) = self.cache.records.get_mut(&group.key)
            && record.version == group.version
        {
            record.generation = generation;
            self.out.bytes.extend_from_slice(&record.bytes);
            self.cache.copied += 1;
            // The groups inside it are in its bytes, and stay cached.
            while let Some(inner) = self.groups.next_if(|inner| inner.layer_start < group.layer_end)
            {
                if let Some(record) = self.cache.records.get_mut(&inner.key) {
                    record.generation = generation;
                }
            }
            return;
        }
        self.cache.written += 1;
        let at = self.out.bytes.len();
        self.out.u8(tag::GROUP);
        self.out.u32(group.key as u32);
        self.out.u32((group.key >> 32) as u32);
        self.out.u32(group.version as u32);
        self.out.u32((group.version >> 32) as u32);
        let length_at = self.out.bytes.len();
        self.out.u32(0);
        // No fold spans a group's edge: the ones open here end before it.
        let folds = std::mem::take(&mut self.folds);
        self.write_range(group.start, group.end);
        self.folds = folds;
        let length = (self.out.bytes.len() - length_at - 4) as u32;
        self.out.bytes[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
        let record = self.cache.records.entry(group.key).or_default();
        record.version = group.version;
        record.generation = generation;
        record.bytes.clear();
        record.bytes.extend_from_slice(&self.out.bytes[at..]);
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

    #[test]
    fn groups_nest_and_carry_their_length() {
        let rect = || DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK));
        let mut list = DisplayList::default();
        for _ in 0..3 {
            list.push(rect());
        }
        // The outer group holds ops 0..3, the inner 1..2; an empty one is left out. They come
        // inner first, as drawing closes them.
        list.groups.push(compose_ui::platform::DrawGroup {
            key: 2,
            version: 7,
            start: 1,
            end: 2,
            layer_start: 1,
            layer_end: 2,
            ..Default::default()
        });
        list.groups.push(compose_ui::platform::DrawGroup {
            key: 3,
            start: 2,
            end: 2,
            layer_start: 2,
            layer_end: 2,
            ..Default::default()
        });
        list.groups.push(compose_ui::platform::DrawGroup {
            key: 1 << 40 | 1,
            start: 0,
            end: 3,
            layer_start: 0,
            layer_end: 3,
            ..Default::default()
        });
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let rect_bytes = 1 + 16 + 4 + 1 + 4 + 1 + 1;
        let header = 1 + 8 + 8 + 4;
        let bytes = &out.bytes;
        assert_eq!(bytes.len(), 2 * header + 3 * rect_bytes);
        assert_eq!(bytes[0], tag::GROUP);
        assert_eq!((read_u32(bytes, 1), read_u32(bytes, 5)), (1, 1 << 8));
        assert_eq!(read_u32(bytes, 17) as usize, header + 3 * rect_bytes);
        let inner = header + rect_bytes;
        assert_eq!(bytes[inner], tag::GROUP);
        assert_eq!(read_u32(bytes, inner + 1), 2);
        assert_eq!((read_u32(bytes, inner + 9), read_u32(bytes, inner + 13)), (7, 0), "version");
        assert_eq!(read_u32(bytes, inner + 17) as usize, rect_bytes);
        assert_eq!(bytes[inner + header], tag::DRAW_RECT);
    }

    /// A boundary holding only a boundary: the same range, the outer one (drawn last) first.
    #[test]
    fn of_two_groups_with_the_same_range_the_outer_opens_first() {
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLACK)));
        list.groups.push(compose_ui::platform::DrawGroup {
            key: 1,
            start: 0,
            end: 1,
            layer_start: 0,
            layer_end: 1,
            ..Default::default()
        });
        list.groups.push(compose_ui::platform::DrawGroup {
            key: 2,
            start: 0,
            end: 1,
            layer_start: 0,
            layer_end: 1,
            ..Default::default()
        });
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert_eq!((out.bytes[0], read_u32(&out.bytes, 1)), (tag::GROUP, 2));
        assert_eq!((out.bytes[21], read_u32(&out.bytes, 22)), (tag::GROUP, 1));
    }

    /// A folded layer's `Save` opens the group as any operation does.
    #[test]
    fn a_group_can_start_with_a_folded_layer() {
        let mut list = DisplayList::default();
        list.push(DrawOp::SaveLayerAlpha(0.5));
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        list.push(DrawOp::Restore);
        list.groups.push(compose_ui::platform::DrawGroup {
            key: 7,
            start: 0,
            end: 3,
            layer_start: 0,
            layer_end: 3,
            ..Default::default()
        });
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        assert_eq!(out.bytes[0], tag::GROUP);
        assert_eq!(read_u32(&out.bytes, 17) as usize, out.bytes.len() - 21);
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
        let mut list = DisplayList::default();
        list.push(DrawOp::Save);
        list.push(DrawOp::ClipPath { path: path.clone(), offset: Offset::new(3.0, 4.0) });
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        list.push(DrawOp::Restore);
        list.groups.push(compose_ui::platform::DrawGroup {
            key: 1,
            start: 0,
            end: 4,
            layer_start: 0,
            layer_end: 4,
            ..Default::default()
        });
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
