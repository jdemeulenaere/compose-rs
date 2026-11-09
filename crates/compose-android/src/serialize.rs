//! The display list as bytes for the Kotlin replayer (docs/07-platforms.md, "Drawing without
//! shipping Skia"): one buffer per frame, read on the Kotlin side with a little-endian
//! `ByteBuffer`, each operation a tag byte and its operands.
//!
//! Paths and images are resources: defined once (a `DefinePath` or `DefineImage` before their
//! first use) and dropped when the last frame using them is gone, so a frame names them by id
//! (drawn with `DRAW_PATH`, clipped to with `CLIP_PATH`). A path changed in place (a `Path`
//! reset and rebuilt each frame) is defined again under the id of its contents no frame uses any
//! more, so the replayer rewinds and refills the same `android.graphics.Path`, as Kotlin's
//! `reset` (D532). A gradient isn't a resource: a paint
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
    BlendMode, Color, ColorFilter, CompositingStrategy, ImageBitmap, Offset, Outline, Paint,
    PaintStyle, PathCommand, PathData, PathFillType, PathOperation, PathStep, Rect, StrokeCap,
    TileMode,
    platform::Shader,
    platform::{
        DisplayList, DrawGroup, DrawOp, Optimization, Optimizations, fits_in, single_drawing,
        single_drawing_extent,
    },
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
    /// (degrees), translation (x, y) (f32s), whether it clips to its outline (u8), its outline
    /// (a kind, u8: 0 none, 1 rectangle, 2 rounded; left, top, right, bottom; the corner radii
    /// top left, top right, bottom right, bottom left: zeros without), its shadow elevation
    /// (f32), its ambient and spot shadow colors (ARGB u32s), its compositing (u8, see
    /// [`compositing`](super::compositing)), then its bounds (left, top, right, bottom):
    /// `LAYER_ENTRY_BYTES` each (D525).
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
    /// A layer with a paint (`DrawOp::SaveLayer`): bounds, alpha (f32), blend mode (u8), color
    /// filter.
    pub const SAVE_LAYER: u8 = 24;
    /// What's drawn up to the end of it, under the edge effects of a scroller (D1475): its number
    /// (u32), its bounds (left, top, right, bottom), a stamp (u32), the elevation kept across a
    /// stretch's axis, the padding in from its bounds of a glow (left, top, right, bottom) (f32s),
    /// then the length in bytes of what it holds (u32), which follows.
    pub const BEGIN_EDGE_EFFECTS: u8 = 25;
    /// A 3 × 3 matrix the canvas is transformed by (`DrawOp::Concat`): nine f32s, row-major.
    pub const CONCAT: u8 = 26;
    /// A path clipped out (`DrawOp::ClipOutPath`): its id (u32), its offset.
    pub const CLIP_OUT_PATH: u8 = 27;
    /// Triangles (`DrawOp::DrawVertices`): their mode (u8: 0 triangles, 1 strip, 2 fan), the
    /// vertex count (u32), positions and texture coordinates (an x, y pair each), colors
    /// (ARGB), the index count (u32) and indices (u16 each), the blend mode (u8), a paint.
    pub const DRAW_VERTICES: u8 = 28;
}

/// What follows a `BEGIN_EDGE_EFFECTS` tag up to what it holds: its operands and that length.
pub const EDGE_EFFECTS_HEADER_BYTES: usize = 4 + 16 + 4 + 4 + 16 + 4;

/// A path command's kind, as `Replayer.kt` reads it.
pub mod command {
    pub const MOVE_TO: u8 = 0;
    pub const LINE_TO: u8 = 1;
    pub const QUAD_TO: u8 = 2;
    pub const CUBIC_TO: u8 = 3;
    pub const CLOSE: u8 = 4;
}

/// A path's step's kind (D535), as `Replayer.kt` reads it.
pub mod step {
    pub const PUSH: u8 = 0;
    pub const COMBINE: u8 = 1;
    pub const ADD: u8 = 2;
    pub const FILL: u8 = 3;
    pub const COMBINE_OR: u8 = 4;
}

/// A color filter's kind, as `Replayer.kt` reads it.
pub mod color_filter {
    pub const NONE: u8 = 0;
    pub const BLEND_MODE: u8 = 1;
    pub const LIGHTING: u8 = 2;
    pub const COLOR_MATRIX: u8 = 3;
}

/// A paint's style flags, as `Replayer.kt` reads them: each says what follows the paint's color,
/// stroke width and shader (a stroke has none: its width is the stroke's).
pub mod paint_flag {
    pub const STROKE: u8 = 1;
    /// Its blend mode's index follows.
    pub const BLEND_MODE: u8 = 2;
    /// Its color filter follows.
    pub const COLOR_FILTER: u8 = 4;
    /// Its blur radius follows.
    pub const BLUR: u8 = 8;
    /// Android's `@ColorLong` of its color follows ([`color_long`](super::color_long)).
    pub const COLOR_LONG: u8 = 16;
}

pub use paint_flag::COLOR_LONG;

/// A recording's shaders and color filters, which its paints, images and layers name by index.
type Tables<'a> = (&'a [Shader], &'a [ColorFilter]);

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
    pub fn u64(&mut self, value: u64) {
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
        self.u32(color.to_argb());
    }
    /// Color, style flags (1 stroke, 2 a blend mode follows, 4 a color filter follows, 8 a blur
    /// radius follows: a mask filter, a shadow's; 16 a color long follows), stroke
    /// width, cap (0 butt, 1 round, 2 square), its shader (from `shaders`): see
    /// [`Writer::shader`]; then its blend mode (u8, Compose's order) if not `SrcOver`, its
    /// color filter (from `filters`, [`Writer::color_filter`]) if any, its blur radius, and
    /// the `@ColorLong` of a color of another space than sRGB that Android's paints take
    /// ([`color_long`]; the ARGB is then its sRGB conversion): a plain sRGB paint's bytes are
    /// what they were before any. Faded by `alpha` (folded layers'): its color's alpha, or
    /// its shader's. A gradient that is a solid color (see [`shading`]) is that color at the
    /// shader's alpha, with no shader.
    fn paint(&mut self, paint: &Paint, (shaders, filters): Tables<'_>, alpha: f32) {
        let shading = paint
            .shader()
            .and_then(|index| shaders.get(index.index()))
            .map_or(Shading::Color, shading);
        let color = match shading {
            Shading::Solid(color, opacity) => {
                color.with_alpha(color.alpha() * opacity.clamp(0.0, 1.0))
            }
            Shading::Color | Shading::Gradient(_) => paint.color,
        };
        self.color(faded(color, alpha));
        // Most paints are plain: no flag to work out.
        let (filter, blended, blurred, mut flags) = if paint.is_plain() {
            (None, false, false, 0)
        } else {
            let filter = paint.color_filter().and_then(|index| filters.get(index.index()));
            let blended = paint.blend_mode != BlendMode::SrcOver;
            let blurred = paint.blur_radius() > 0.0;
            let flag = |set: bool, flag: u8| if set { flag } else { 0 };
            let flags = flag(blended, paint_flag::BLEND_MODE)
                | flag(filter.is_some(), paint_flag::COLOR_FILTER)
                | flag(blurred, paint_flag::BLUR);
            (filter, blended, blurred, flags)
        };
        // An sRGB color is its ARGB: one test of its space's bits.
        let long = if color.value() & 0x3F != 0 && matches!(shading, Shading::Color) {
            color_long(faded_wide(color, alpha))
        } else {
            None
        };
        if long.is_some() {
            flags |= COLOR_LONG;
        }
        match paint.style {
            PaintStyle::Fill => {
                self.u8(flags);
                self.f32(0.0);
            }
            PaintStyle::Stroke(width) => {
                self.u8(flags | paint_flag::STROKE);
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
        if blended {
            self.u8(paint.blend_mode as u8);
        }
        if let Some(filter) = filter {
            self.color_filter(Some(filter));
        }
        if blurred {
            self.f32(paint.blur_radius());
        }
        if let Some(long) = long {
            self.u64(long);
        }
    }
    /// A color filter: its kind (a [`color_filter`] constant), then for a blend mode filter
    /// its color and mode (u8), for lighting its multiply and add colors, for a matrix its 20
    /// values, row-major, offsets in 0 to 255 (as `android.graphics.ColorMatrix`).
    fn color_filter(&mut self, filter: Option<&ColorFilter>) {
        match filter {
            None => self.u8(color_filter::NONE),
            Some(ColorFilter::BlendMode { color, blend_mode }) => {
                self.u8(color_filter::BLEND_MODE);
                self.color(*color);
                self.u8(*blend_mode as u8);
            }
            Some(ColorFilter::Lighting { multiply, add }) => {
                self.u8(color_filter::LIGHTING);
                self.color(*multiply);
                self.color(*add);
            }
            Some(ColorFilter::ColorMatrix(matrix)) => {
                self.u8(color_filter::COLOR_MATRIX);
                for value in matrix.values {
                    self.f32(value);
                }
            }
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
        let colors = &gradient.colors;
        // A gradient needs two colors: one is a solid color, as compose-skia draws it.
        let count = colors.len().max(2);
        let color = |index: usize| colors[index.min(colors.len() - 1)];
        // Only a gradient with a color of another space than sRGB: one test of each's bits.
        let longs = colors
            .iter()
            .any(|color| color.value() & 0x3F != 0)
            .then(|| gradient_color_longs((0..count).map(color)));
        let tile_mode =
            tile_mode_byte(gradient.tile_mode) | if longs.is_some() { WIDE_GRADIENT } else { 0 };
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
        self.u32(count as u32);
        for index in 0..count {
            self.color(color(index));
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
        for long in longs.into_iter().flatten() {
            self.u64(long);
        }
        self.f32(alpha.clamp(0.0, 1.0));
    }
    /// Kind (0 rectangle, 1 rounded), bounds, radii. A path's outline is its bounds' rectangle
    /// (the replayer skips flattened shadows, which a render node draws).
    fn outline(&mut self, outline: &Outline) {
        match outline {
            Outline::Rectangle(rect) => {
                self.u8(0);
                self.rect(rect);
                self.radii(&[0.0; 4]);
            }
            Outline::Generic(..) => {
                self.u8(0);
                self.rect(&outline.bounds());
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

/// A tile mode's byte: 0 clamp, 1 repeated, 2 mirror, 3 decal.
fn tile_mode_byte(tile_mode: TileMode) -> u8 {
    match tile_mode {
        TileMode::Clamp => 0,
        TileMode::Repeated => 1,
        TileMode::Mirror => 2,
        TileMode::Decal => 3,
    }
}

/// What paints a paint with a shader on Android.
pub(crate) enum Shading<'a> {
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
pub(crate) fn shading(shader: &Shader) -> Shading<'_> {
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
        .map(|color| {
            let argb = color.to_argb();
            [24, 16, 8, 0].map(|shift| f32::from((argb >> shift) as u8))
        })
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

/// A resource change the Kotlin side hasn't taken yet (a path's definition is a
/// [`PendingPath`]).
enum Pending {
    Image { id: u32, width: u32, height: u32, pixels: Weak<[u8]> },
    DropPath(u32),
    DropImage(u32),
}

/// A path's definition the Kotlin side hasn't taken yet.
struct PendingPath {
    id: u32,
    path: Weak<PathData>,
    /// The Kotlin side holds the id already (defined again in place, D532): cancelling this
    /// must still drop it.
    taken: bool,
}

/// A path's contents the Kotlin side holds under an id.
struct DefinedPath {
    /// The contents: alive while a recording (a group's, the frame's) or their path holds them.
    contents: Weak<PathData>,
    id: u32,
}

/// The paths and images the Kotlin side holds, by id.
#[derive(Default)]
pub struct Resources {
    /// Each path's contents still held, by its identity and generation (`PathData::id`,
    /// `generation`), under its own id.
    paths: HashMap<(u64, u64), DefinedPath>,
    /// The ids whose contents nothing holds any more, by path identity, found at the start of
    /// a write ([`Resources::harvest`]): the path's next contents take one (D532); those left
    /// are dropped once the write is over ([`Resources::collect`]).
    free: HashMap<u64, Vec<u32>>,
    /// Each path id's last definition, by a counter of definitions (`serial`): a record cached
    /// before an id it names was defined again draws other contents with the same bytes.
    defined_at: HashMap<u32, u64>,
    serial: u64,
    /// The path ids the current write's operations named, in order (`write_group` takes a
    /// record's).
    named: Vec<u32>,
    images: HashMap<usize, (Weak<[u8]>, u32)>,
    next_id: u32,
    /// `next_id` wrapped round: ids handed out from now on skip those in use.
    wrapped: bool,
    /// Drops, and images' definitions, the Kotlin side hasn't taken yet: a definition dropped
    /// before it was taken is cancelled, so this stays bounded however many frames go undrawn.
    pending: Vec<Pending>,
    /// Paths' definitions the Kotlin side hasn't taken yet, in order (`None`: cancelled), and
    /// where each id's is.
    pending_paths: Vec<Option<PendingPath>>,
    pending_path_at: HashMap<u32, usize>,
}

impl Resources {
    /// A new resource id: never 0 (nothing), and once the ids wrapped round (after 4 billion),
    /// never one a path or an image holds. (An id whose drop is pending may be taken again: the
    /// Kotlin side reads drops before definitions.)
    fn next(&mut self) -> u32 {
        loop {
            self.next_id = self.next_id.wrapping_add(1);
            if self.next_id == 0 {
                self.wrapped = true;
                continue;
            }
            let id = self.next_id;
            if !self.wrapped
                || !(self.defined_at.contains_key(&id)
                    || self.images.values().any(|(_, image)| *image == id))
            {
                return id;
            }
        }
    }

    /// Drops path `id`: its definition, if still pending, or the Kotlin side's.
    fn drop_path(&mut self, id: u32) {
        self.defined_at.remove(&id);
        if self.cancel_path(id).is_none_or(|taken| taken) {
            self.pending.push(Pending::DropPath(id));
        }
    }

    /// Cancels path `id`'s pending definition, if any: whether the Kotlin side holds the id.
    /// Once cancelled ones are half the list, it's compacted: frames written and not taken (a
    /// view hidden by its own visibility, whose window shows) leave one definition per id.
    fn cancel_path(&mut self, id: u32) -> Option<bool> {
        let at = self.pending_path_at.remove(&id)?;
        let taken = self.pending_paths[at].take().map(|pending| pending.taken);
        if 2 * self.pending_path_at.len() < self.pending_paths.len() {
            self.pending_paths.retain(Option::is_some);
            for (at, pending) in self.pending_paths.iter().enumerate() {
                let id = pending.as_ref().expect("kept").id;
                self.pending_path_at.insert(id, at);
            }
        }
        taken
    }

    fn drop_image(&mut self, id: u32) {
        let before = self.pending.len();
        self.pending
            .retain(|op| !matches!(op, Pending::Image { id: defined, .. } if *defined == id));
        if self.pending.len() == before {
            self.pending.push(Pending::DropImage(id));
        }
    }

    /// Notes the path contents nothing holds any more: their ids are free for their paths' next
    /// contents (D532). Called before a write, as the frame's drawing let go of the last frame's.
    fn harvest(&mut self) {
        let free = &mut self.free;
        self.paths.retain(|(path, _), defined| {
            let alive = defined.contents.strong_count() > 0;
            if !alive {
                free.entry(*path).or_default().push(defined.id);
            }
            alive
        });
    }

    /// Starts a write: the ids free since the last one noted, none named yet.
    fn begin_write(&mut self) {
        self.harvest();
        self.named.clear();
    }

    /// The id of `path`'s contents, defined if new: under an id of contents of the same path
    /// that nothing holds any more if there is one (D532), the replayer then refilling that
    /// id's `android.graphics.Path`; under a new id otherwise. Contents still held keep their
    /// id, so a record naming it (cached, D157) draws them whenever it's replayed. Constant
    /// time, however many contents a path has (one path reset for each of many shapes).
    fn path_id(&mut self, path: &Arc<PathData>) -> u32 {
        let key = (path.id(), path.generation());
        if let Some(defined) = self.paths.get(&key) {
            debug_assert!(
                defined.contents.upgrade().is_some_and(|alive| Arc::ptr_eq(&alive, path)),
                "a path's generation names one set of contents"
            );
            self.named.push(defined.id);
            return defined.id;
        }
        let (id, taken) = match self.free.get_mut(&path.id()).and_then(Vec::pop) {
            // Defined again in place: a definition the Kotlin side hasn't taken is replaced.
            Some(id) => (id, self.cancel_path(id).unwrap_or(true)),
            None => (self.next(), false),
        };
        self.paths.insert(key, DefinedPath { contents: Arc::downgrade(path), id });
        self.serial += 1;
        self.defined_at.insert(id, self.serial);
        self.pending_path_at.insert(id, self.pending_paths.len());
        self.pending_paths.push(Some(PendingPath { id, path: Arc::downgrade(path), taken }));
        self.named.push(id);
        id
    }

    /// The definitions so far: a record written now draws its paths' contents as they're
    /// defined after this many.
    fn serial(&self) -> u64 {
        self.serial
    }

    /// Whether a path among `ids` was defined (again) after `serial` definitions.
    fn defined_after(&self, ids: &[u32], serial: u64) -> bool {
        ids.iter().any(|id| self.defined_at.get(id).is_some_and(|at| *at > serial))
    }

    /// The id of `image`, defined if new; 0 (nothing) for an empty one.
    fn image_id(&mut self, image: &ImageBitmap) -> u32 {
        if image.width() == 0 || image.height() == 0 {
            return 0;
        }
        // Read once: a bitmap drawn into on another thread may have new pixels at a second read.
        let pixels = image.pixels();
        let key = pixels.as_ptr() as usize;
        if let Some((weak, id)) = self.images.get(&key)
            && weak.strong_count() > 0
        {
            return *id;
        }
        let id = self.next();
        let weak = std::sync::Arc::downgrade(&pixels);
        if let Some((_, old)) = self.images.insert(key, (weak.clone(), id)) {
            self.drop_image(old);
        }
        self.pending.push(Pending::Image {
            id,
            width: image.width(),
            height: image.height(),
            pixels: weak,
        });
        id
    }

    /// Drops the resources no frame can use any more (their owners are gone): the paths' ids no
    /// new contents took, and gone images.
    pub fn collect(&mut self) {
        self.harvest();
        let free: Vec<u32> = self.free.drain().flat_map(|(_, ids)| ids).collect();
        for id in free {
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
        !self.pending.is_empty() || !self.pending_path_at.is_empty()
    }

    /// Appends the pending changes to `out` (a definition whose owner is already gone is
    /// skipped), and forgets them: the Kotlin side has them. Drops come first (D410): a frame
    /// replacing its paths (a canvas drawing new ones each frame) defines them after the frame
    /// before's are dropped, so the replayer rebuilds the dropped `Path`s rather than making
    /// new ones. (A drop names an id taken before, never one defined beside it: that one's
    /// definition is cancelled instead.) Then paths' definitions, then images'.
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
                Pending::Image { .. } => {}
            }
        }
        self.pending_path_at.clear();
        for pending in self.pending_paths.drain(..).flatten() {
            let Some(path) = pending.path.upgrade() else { continue };
            write_path(out, pending.id, &path);
        }
        for op in self.pending.drain(..) {
            if let Pending::Image { id, width, height, pixels } = op {
                let Some(pixels) = pixels.upgrade() else { continue };
                out.u8(tag::DEFINE_IMAGE);
                out.u32(id);
                out.u32(width);
                out.u32(height);
                out.u32(pixels.len() as u32);
                out.bytes.extend_from_slice(&pixels);
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
        tag::CLIP_PATH | tag::CLIP_OUT_PATH => 4 + 8,
        tag::CONCAT => 9 * 4,
        tag::DRAW_VERTICES => {
            let count = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            let vertices = count(at + 2) as usize;
            let indices_at = at + 2 + 4 + vertices * (8 + 8 + 4);
            let paint_at = indices_at + 4 + 2 * count(indices_at) as usize + 1;
            paint_at - at - 1 + paint_bytes(bytes, paint_at)
        }
        tag::GROUP_REF => 8,
        // What it holds comes after, and reads as what it is.
        tag::BEGIN_EDGE_EFFECTS => EDGE_EFFECTS_HEADER_BYTES,
        tag::KEPT => {
            4 + KEPT_ENTRY_BYTES
                * u32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize
        }
        tag::DRAW_RECT | tag::DRAW_LINE => painted(16),
        tag::DRAW_ROUND_RECT => painted(32),
        tag::DRAW_PARAGRAPH => 12,
        tag::SAVE_LAYER_ALPHA => 16 + 4,
        tag::SAVE_LAYER => 16 + 4 + 1 + color_filter_bytes(bytes, at + 1 + 16 + 4 + 1),
        tag::TRANSFORM => 28,
        tag::DRAW_IMAGE => 4 + 32 + 4 + color_filter_bytes(bytes, at + 1 + 4 + 32 + 4),
        tag::DRAW_SHADOW => 1 + 32 + 4 + 12 + 4 + 8,
        tag::DRAW_ARC => painted(16 + 8 + 1),
        tag::DRAW_PATH => painted(4 + 16),
        other => panic!("not a drawing operation's tag: {other}"),
    }
}

/// The bytes of the paint at `at` ([`Writer::paint`]).
fn paint_bytes(bytes: &[u8], at: usize) -> usize {
    let count_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let flags = bytes[at + 4];
    let kind_at = at + 4 + 1 + 4 + 1;
    let geometry = match bytes[kind_at] {
        shader::NONE => 0,
        shader::LINEAR => 16,
        shader::RADIAL => 12,
        shader::SWEEP => 8,
        other => panic!("not a shader kind: {other}"),
    };
    let mut end = kind_at + 1;
    if bytes[kind_at] != shader::NONE {
        let colors_at = kind_at + 2 + geometry;
        let stops_at = colors_at + 4 + 4 * count_at(colors_at);
        let longs =
            if bytes[kind_at + 1] & WIDE_GRADIENT != 0 { 8 * count_at(colors_at) } else { 0 };
        end = stops_at + 4 + 4 * count_at(stops_at) + longs + 4;
    }
    if flags & paint_flag::BLEND_MODE != 0 {
        end += 1;
    }
    if flags & paint_flag::COLOR_FILTER != 0 {
        end += color_filter_bytes(bytes, end);
    }
    if flags & paint_flag::BLUR != 0 {
        end += 4;
    }
    if flags & COLOR_LONG != 0 {
        end += 8;
    }
    end - at
}

/// The bytes of the color filter at `at` ([`Writer::color_filter`]).
fn color_filter_bytes(bytes: &[u8], at: usize) -> usize {
    1 + match bytes[at] {
        color_filter::NONE => 0,
        color_filter::BLEND_MODE => 4 + 1,
        color_filter::LIGHTING => 8,
        color_filter::COLOR_MATRIX => 80,
        other => panic!("not a color filter kind: {other}"),
    }
}

fn write_path(writer: &mut Writer, id: u32, path: &PathData) {
    writer.u8(tag::DEFINE_PATH);
    writer.u32(id);
    write_path_contents(writer, path);
}

/// A path's contents, as `Replayer.kt` reads them: its fill type (u8: 0 non-zero, 1 even-odd),
/// its steps (D535: a count, u32, then each a kind, u8: `step::PUSH` and an operand's contents,
/// as these, which have no steps; `step::COMBINE` and the operation, u8, `PathOperation`'s
/// order; `step::COMBINE_OR` and the operation; `step::ADD`; `step::FILL` and a fill type), then
/// its commands' count (u32) and each
/// command (its kind, its points). A list, written and read in one pass however many steps.
pub(crate) fn write_path_contents(writer: &mut Writer, path: &PathData) {
    write_fill_type(writer, path.fill_type());
    writer.u32(path.steps().len() as u32);
    for path_step in path.steps() {
        match path_step {
            PathStep::Push(operand) => {
                writer.u8(step::PUSH);
                debug_assert!(operand.steps().is_empty(), "an operand has no steps");
                write_fill_type(writer, operand.fill_type());
                writer.u32(0);
                write_commands(writer, operand.commands());
            }
            PathStep::Combine(operation) => {
                writer.u8(step::COMBINE);
                write_operation(writer, *operation);
            }
            PathStep::CombineOr(operation) => {
                writer.u8(step::COMBINE_OR);
                write_operation(writer, *operation);
            }
            PathStep::Add => writer.u8(step::ADD),
            PathStep::SetFillType(fill_type) => {
                writer.u8(step::FILL);
                write_fill_type(writer, *fill_type);
            }
        }
    }
    write_commands(writer, path.commands());
}

fn write_operation(writer: &mut Writer, operation: PathOperation) {
    writer.u8(match operation {
        PathOperation::Difference => 0,
        PathOperation::Intersect => 1,
        PathOperation::Union => 2,
        PathOperation::Xor => 3,
        PathOperation::ReverseDifference => 4,
    });
}

fn write_fill_type(writer: &mut Writer, fill_type: PathFillType) {
    writer.u8(match fill_type {
        PathFillType::NonZero => 0,
        PathFillType::EvenOdd => 1,
    });
}

fn write_commands(writer: &mut Writer, commands: &[PathCommand]) {
    writer.u32(commands.len() as u32);
    for command in commands {
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

/// Whether the layer opening at `start` (a `SaveLayerAlpha` of `bounds`) holds one fill, among
/// saves, clips and moves, that provably lies within `bounds` ([`single_drawing_extent`]): its
/// alpha can go into that fill's paint (its color's, or its gradient's: [`Writer::paint`]),
/// without an offscreen layer (a GPU pass each frame on Android), the same coverage (D648).
/// Returns the index of its closing `Restore` if so.
pub(crate) fn foldable_layer(ops: &[DrawOp], start: usize, bounds: Rect) -> Option<usize> {
    let layer = &ops[start + 1..];
    let end = single_drawing(layer).map(|end| start + 1 + end).filter(|end| *end < ops.len())?;
    single_drawing_extent(layer).is_some_and(|extent| fits_in(extent, bounds)).then_some(end)
}

/// The `@ColorLong` an Android paint takes for `color`, as Compose's `toSupportedColorLong`
/// (docs/parity/design/color-spaces.md): `None` for an sRGB color, which its ARGB says, and for
/// a space Android's paints don't take (unspecified, Oklab, CIE XYZ and Lab), drawn as its sRGB
/// conversion. Compose's ids 0 to 15 are Android's; BT.2020 HLG and PQ, Compose's 17 and 18, are
/// Android's 16 and 17 (API 34: below it, the replayer keeps the ARGB, as Compose's
/// `toColorLong` converts to sRGB).
pub fn color_long(color: Color) -> Option<u64> {
    let value = color.value();
    match value & 0x3F {
        0 | 14 | 15 | 16 | 19 => None,
        17 | 18 => Some(value - 1),
        _ => Some(value),
    }
}

/// In a gradient's tile mode byte: Android's `@ColorLong`s of its colors follow its stops,
/// one per color ([`gradient_color_longs`]).
pub const WIDE_GRADIENT: u8 = 0x80;

/// The `@ColorLong`s of a gradient's `colors`, as Compose's `toPlatformLongArray`
/// ([`color_long`] of each, an sRGB one its ARGB shifted up), for Android's gradients taking
/// longs (API 29): of one space, as Android's gradients want them. Colors of two spaces (where
/// Compose's gradient throws) are converted to extended sRGB, which holds both.
pub fn gradient_color_longs(colors: impl Iterator<Item = Color> + Clone) -> Vec<u64> {
    let long = |color: Color| color_long(color).unwrap_or_else(|| u64::from(color.to_argb()) << 32);
    let longs: Vec<u64> = colors.clone().map(long).collect();
    if longs.iter().all(|value| value & 0x3F == longs[0] & 0x3F) {
        return longs;
    }
    colors
        .map(|color| color.convert(&compose_ui::colorspace::ColorSpaces::EXTENDED_SRGB).value())
        .collect()
}

/// A color of another space than sRGB with its alpha multiplied by `alpha`.
fn faded_wide(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 { color } else { color.with_alpha(color.alpha() * alpha.clamp(0.0, 1.0)) }
}

/// `color` with its alpha multiplied by `alpha`.
pub(crate) fn faded(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return color;
    }
    let argb = color.to_argb();
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
    /// The render effects of the layers listed, in their order (`write_layers`), kept for its
    /// capacity.
    effects: Vec<compose_ui::RenderEffect>,
    /// The keys of the records written ahead (`write_groups_ahead`) that no frame wrote or
    /// copied since, by item (one write's), oldest first: kept until a frame does,
    /// `AHEAD_CAPACITY` items at most.
    ahead: Vec<Vec<u64>>,
    /// How many records the last write copied that were written ahead.
    copied_ahead: usize,
    /// The optimizations its records are written with (`set_optimizations`).
    optimizations: Optimizations,
}

#[derive(Default)]
struct CachedGroup {
    /// The group's version in the display list.
    version: u64,
    /// The version its record carries: the display list's when its bytes last changed (a
    /// nested group's change gives the group a new version, but leaves its bytes as they were).
    written_version: u64,
    bytes: Vec<u8>,
    /// The resources' definitions when its bytes were written with `written_version`
    /// (`Resources::serial`): a path it names defined again since draws other contents.
    recorded_serial: u64,
    generation: u64,
    /// Its content's one drawing's extent (`single_drawing_extent`), if it's one drawing with a
    /// paint that can be bounded: an alpha layer around it whose bounds hold it fades that
    /// paint, not overlapping. Compared with the layer's bounds each frame (they change without
    /// the version).
    single_drawing_extent: Option<Rect>,
    /// Written ahead, and no frame wrote or copied it since: kept though the last frame didn't.
    ahead: bool,
}

impl GroupCache {
    /// A cache writing records with `optimizations` (an alpha layer folded into its one
    /// drawing's paint with `SINGLE_DRAWING_FADE`).
    pub fn new(optimizations: Optimizations) -> GroupCache {
        GroupCache { optimizations, ..GroupCache::default() }
    }

    /// Writes records with `optimizations` from now on: each cached record is written again by
    /// the next write (the same bytes where they don't depend on them, so the replayer keeps
    /// their recordings).
    pub fn set_optimizations(&mut self, optimizations: Optimizations) {
        let draws_differently = self.optimizations.draws_differently(optimizations);
        self.optimizations = optimizations;
        if draws_differently {
            for record in self.records.values_mut() {
                record.version = u64::MAX;
            }
        }
    }

    pub fn optimizations(&self) -> Optimizations {
        self.optimizations
    }

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

    /// The version the replayer has the record cached for `key` under, if any: the cached
    /// version, or the one its bytes were first written with when written again the same
    /// (a node reused for content drawing the same, D430).
    pub fn cached_written_version(&self, key: u64) -> Option<u64> {
        self.records.get(&key).map(|record| record.written_version)
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
pub const LAYER_ENTRY_BYTES: usize = 8 + layer_entry::END;

/// Where each property is in a `LAYERS` entry, after its key (8 bytes), as `Replayer.kt`'s
/// `ENTRY_` constants read it (`write_layer` writes them in this order).
pub mod layer_entry {
    pub const ALPHA: usize = 0;
    pub const PIVOT: usize = 4;
    pub const SCALE: usize = 12;
    pub const ROTATION: usize = 20;
    pub const TRANSLATION: usize = 24;
    pub const CLIP: usize = 32;
    pub const OUTLINE_KIND: usize = 33;
    pub const OUTLINE_BOUNDS: usize = 34;
    pub const OUTLINE_RADII: usize = 50;
    pub const ELEVATION: usize = 66;
    pub const AMBIENT: usize = 70;
    pub const SPOT: usize = 74;
    pub const COMPOSITING: usize = 78;
    pub const BOUNDS: usize = 79;
    pub const END: usize = 95;
}

/// A layer's render effect, after the `LAYERS` section's entries: one for each entry whose
/// compositing byte has [`compositing::EFFECT`], in the entries' order, so a frame without
/// effects writes no byte more for them (D1904). Its kind (0 none, 1 a blur), the blur's radii
/// (f32 each) and its edge treatment (a tile mode: 0 clamp, 1 repeated, 2 mirror, 3 decal), as
/// `Replayer.kt`'s `EFFECT_` constants read them.
pub mod layer_effect {
    pub const KIND: usize = 0;
    pub const RADII: usize = 1;
    pub const TILE: usize = 9;
    pub const BYTES: usize = 10;
    pub const BLUR: u8 = 1;
}

/// Where the `LAYERS` section at `at` in `bytes` ends: its entries, then the render effects
/// those flagging one have.
pub fn layers_section_end(bytes: &[u8], at: usize) -> usize {
    let count = u32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize;
    let entries = at + 5;
    let effects = (0..count)
        .filter(|index| {
            let compositing = entries + index * LAYER_ENTRY_BYTES + 8 + layer_entry::COMPOSITING;
            bytes[compositing] & compositing::EFFECT != 0
        })
        .count();
    entries + count * LAYER_ENTRY_BYTES + effects * layer_effect::BYTES
}

/// A `LAYERS` entry's outline kind, as `Replayer.kt` reads it.
pub mod outline_kind {
    pub const NONE: u8 = 0;
    pub const RECTANGLE: u8 = 1;
    pub const ROUNDED: u8 = 2;
    /// A path (`Outline::Generic`): its bounds, then in the radii's place its id (defined
    /// before the `LAYERS` section, in the resource changes), its offset, and the low bits of
    /// its definition's serial (an id defined again in place, D532, changes the entry).
    pub const PATH: u8 = 3;
}

/// A `LAYERS` entry's compositing byte, as `Replayer.kt` reads it.
pub mod compositing {
    /// Offscreen for an alpha below 1 (`CompositingStrategy::Auto`).
    pub const AUTO: u8 = 0;
    /// Always offscreen (`Offscreen`: a compositing layer).
    pub const OFFSCREEN: u8 = 1;
    /// Never: the alpha multiplies each drawing's (`ModulateAlpha`).
    pub const MODULATE_ALPHA: u8 = 2;
    /// A flag on the strategy: the layer's render effect follows the section's entries
    /// ([`layer_effect`](super::layer_effect)).
    pub const EFFECT: u8 = 4;
}

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
    resources.begin_write();
    cache.generation += 1;
    cache.copied = 0;
    cache.copied_ahead = 0;
    cache.written = 0;
    cache.unchanged = 0;
    write_layers(&list.groups, cache, resources, out);
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
    resources.begin_write();
    cache.copied = 0;
    cache.copied_ahead = 0;
    cache.written = 0;
    cache.unchanged = 0;
    write_layers(groups, cache, resources, out);
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
/// (as drawing closes them), then the render effects of those that have one, in that order.
fn write_layers(
    groups: &[DrawGroup],
    cache: &mut GroupCache,
    resources: &mut Resources,
    out: &mut Writer,
) {
    let layered = |group: &DrawGroup| !group.empty && group.layer.is_some();
    let count = groups.iter().filter(|group| layered(group)).count();
    if count == 0 {
        return;
    }
    out.u8(tag::LAYERS);
    out.u32(count as u32);
    let mut open = std::mem::take(&mut cache.open);
    let mut effects = std::mem::take(&mut cache.effects);
    for index in 0..=groups.len() {
        // The groups closed before this one, innermost first.
        while let Some(&last) = open.last()
            && (index == groups.len() || last + groups[last].descendants < index)
        {
            open.pop();
            if let Some(effect) = write_layer(groups, last, cache, resources, out) {
                effects.push(effect);
            }
        }
        if groups.get(index).is_some_and(layered) {
            open.push(index);
        }
    }
    for compose_ui::RenderEffect::Blur { radius_x, radius_y, edge_treatment } in effects.drain(..) {
        out.u8(layer_effect::BLUR);
        out.f32(radius_x);
        out.f32(radius_y);
        out.u8(tile_mode_byte(edge_treatment));
    }
    cache.open = open;
    cache.effects = effects;
}

/// The `LAYERS` entry of the group at `index` of `groups`, and its render effect, which the
/// section writes after its entries.
fn write_layer(
    groups: &[DrawGroup],
    index: usize,
    cache: &GroupCache,
    resources: &mut Resources,
    out: &mut Writer,
) -> Option<compose_ui::RenderEffect> {
    let group = &groups[index];
    let layer = group.layer.as_ref().expect("a layer");
    out.u32(group.key as u32);
    out.u32((group.key >> 32) as u32);
    out.f32(layer.alpha);
    out.offset(layer.pivot);
    out.f32(layer.scale.0);
    out.f32(layer.scale.1);
    out.f32(layer.rotation);
    out.offset(layer.translation);
    let start = out.bytes.len();
    out.u8(u8::from(layer.clip && layer.outline.is_some()));
    match &layer.outline {
        None => {
            out.u8(outline_kind::NONE);
            out.rect(&Rect::new(0.0, 0.0, 0.0, 0.0));
            out.radii(&[0.0; 4]);
        }
        Some(Outline::Rectangle(rect)) => {
            out.u8(outline_kind::RECTANGLE);
            out.rect(rect);
            out.radii(&[0.0; 4]);
        }
        Some(Outline::Rounded(rect, radii)) => {
            out.u8(outline_kind::ROUNDED);
            out.rect(rect);
            out.radii(radii);
        }
        // The node's outline is the path, as Compose's layer's `setPathOutline` (D1055).
        Some(outline @ Outline::Generic(path, offset)) => {
            let id = resources.path_id(path);
            out.u8(outline_kind::PATH);
            out.rect(&outline.bounds());
            out.u32(id);
            out.offset(*offset);
            out.u32(resources.defined_at.get(&id).copied().unwrap_or(0) as u32);
        }
    }
    out.f32(layer.shadow_elevation);
    out.color(layer.ambient_shadow_color());
    out.color(layer.spot_shadow_color());
    let effect = layer.render_effect();
    let flag = if effect.is_some() { compositing::EFFECT } else { 0 };
    out.u8(compositing_of(groups, index, cache) | flag);
    out.rect(&layer.bounds);
    debug_assert_eq!(out.bytes.len() - start, layer_entry::END - layer_entry::CLIP);
    effect
}

/// How the group at `index` of `groups` composites its content: its layer's strategy, except
/// that with `Optimization::SINGLE_DRAWING_FADE`, an `Auto` alpha below 1 over one fill and no
/// group (as a folded layer, D153) multiplies that fill's alpha instead (`ModulateAlpha`: a
/// render node's `setHasOverlappingRendering(false)`), when the fill provably lies within the
/// node's bounds, which bound HWUI's alpha layer: the same coverage without an offscreen
/// buffer, but for the promoted layer's resampling under the node's transform (D521, D648). Read from its cached record while its version stays.
fn compositing_of(groups: &[DrawGroup], index: usize, cache: &GroupCache) -> u8 {
    let group = &groups[index];
    let Some(layer) = &group.layer else { return compositing::AUTO };
    match layer.compositing_strategy {
        CompositingStrategy::Offscreen => return compositing::OFFSCREEN,
        CompositingStrategy::ModulateAlpha => return compositing::MODULATE_ALPHA,
        CompositingStrategy::Auto => {}
    }
    if !cache.optimizations.contains(Optimization::SINGLE_DRAWING_FADE) || layer.alpha >= 1.0 {
        return compositing::AUTO;
    }
    let nested = &groups[index + 1..=index + group.descendants];
    if nested.iter().any(|inner| !inner.empty) {
        return compositing::AUTO;
    }
    let extent = match cache.records.get(&group.key) {
        Some(record) if record.version == group.version => record.single_drawing_extent,
        _ => single_drawing_extent(&group.content.ops),
    };
    // The node's bounds: from the group's origin to the layer's far corner.
    let node = Rect::new(0.0, 0.0, layer.bounds.right, layer.bounds.bottom);
    if extent.is_some_and(|extent| fits_in(extent, node)) {
        compositing::MODULATE_ALPHA
    } else {
        compositing::AUTO
    }
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
        // Where the lengths of the edge effects open are.
        let mut edge_effects: Vec<usize> = Vec::new();
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
            if let DrawOp::SaveLayerAlpha { bounds, alpha } = ops[index]
                && self.cache.optimizations.contains(Optimization::SINGLE_DRAWING_FADE)
                && let Some(end) = foldable_layer(ops, index, bounds)
                && !self.draws_before(&boundaries[boundary..], end, child)
            {
                self.folds.push((end, alpha));
                self.out.u8(tag::SAVE);
                continue;
            }
            let alpha: f32 = self.folds.iter().map(|(_, alpha)| alpha).product();
            let tables = (&recording.shaders[..], &recording.color_filters[..]);
            write_op(&ops[index], alpha, tables, self.resources, self.out);
            match ops[index] {
                DrawOp::BeginEdgeEffects { .. } => edge_effects.push(self.out.bytes.len() - 4),
                DrawOp::EndEdgeEffects => {
                    let at = edge_effects.pop().expect("edge effects begin before they end");
                    let length = (self.out.bytes.len() - at - 4) as u32;
                    self.out.bytes[at..at + 4].copy_from_slice(&length.to_le_bytes());
                }
                _ => {}
            }
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
        let named = self.resources.named.len();
        self.write_recording(&group.content, index + 1);
        self.folds = folds;
        let length = (self.out.bytes.len() - length_at - 4) as u32;
        self.out.bytes[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
        let record = self.cache.records.entry(group.key).or_default();
        let content = length_at + 4;
        // A path it names defined again since its bytes were recorded (changed in place, D532,
        // by this write or an earlier one): the same bytes draw other contents, so its version
        // tells the replayer to record it again.
        let redefined =
            self.resources.defined_after(&self.resources.named[named..], record.recorded_serial);
        let unchanged = !redefined
            && !record.bytes.is_empty()
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
            record.recorded_serial = self.resources.serial();
            record.bytes.clear();
            record.bytes.extend_from_slice(&self.out.bytes[at..]);
        }
        record.version = group.version;
        record.generation = generation;
        // Of the display list's operations, which bytes alone don't tell (a folded layer's).
        record.single_drawing_extent = single_drawing_extent(&group.content.ops);
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

/// Writes `op`, its paint faded by `alpha` (the folded layers around it), its shader and color
/// filter from `tables`.
fn write_op(
    op: &DrawOp,
    alpha: f32,
    tables: Tables<'_>,
    resources: &mut Resources,
    out: &mut Writer,
) {
    let filter = |index: &Option<compose_ui::platform::ColorFilterIndex>| {
        index.and_then(|index| tables.1.get(index.index()))
    };
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
            out.paint(paint, tables, alpha);
        }
        DrawOp::DrawRoundRect(rect, radii, paint) => {
            out.u8(tag::DRAW_ROUND_RECT);
            out.rect(rect);
            out.radii(radii);
            out.paint(paint, tables, alpha);
        }
        DrawOp::DrawLine(from, to, paint) => {
            out.u8(tag::DRAW_LINE);
            out.offset(*from);
            out.offset(*to);
            out.paint(paint, tables, alpha);
        }
        DrawOp::DrawParagraph(handle, offset) => {
            out.u8(tag::DRAW_PARAGRAPH);
            out.u32(handle.0 as u32);
            out.offset(*offset);
        }
        DrawOp::BeginEdgeEffects { owner, bounds, stamp, max_elevation, glow_padding } => {
            out.u8(tag::BEGIN_EDGE_EFFECTS);
            out.u32(*owner);
            out.rect(bounds);
            out.u32(*stamp);
            out.f32(*max_elevation);
            out.radii(glow_padding);
            // Its length, set at its end (`write_recording`).
            out.u32(0);
        }
        // The end of its length.
        DrawOp::EndEdgeEffects => {}
        DrawOp::SaveLayerAlpha { bounds, alpha } => {
            out.u8(tag::SAVE_LAYER_ALPHA);
            out.rect(bounds);
            out.f32(*alpha);
        }
        // A render effect is a group's, which its render node applies (its `LAYERS` entry):
        // drawn in place, it's a plain layer.
        DrawOp::SaveLayerEffect { bounds, .. } => {
            out.u8(tag::SAVE_LAYER_ALPHA);
            out.rect(bounds);
            out.f32(1.0);
        }
        DrawOp::SaveLayer { bounds, alpha, color_filter, blend_mode } => {
            out.u8(tag::SAVE_LAYER);
            out.rect(bounds);
            out.f32(*alpha);
            out.u8(*blend_mode as u8);
            out.color_filter(filter(color_filter));
        }
        DrawOp::Transform { pivot, scale, rotation, translation } => {
            out.u8(tag::TRANSFORM);
            out.offset(*pivot);
            out.f32(scale.0);
            out.f32(scale.1);
            out.f32(*rotation);
            out.offset(*translation);
        }
        DrawOp::DrawImage { image, src, dst, alpha, color_filter } => {
            let id = resources.image_id(image);
            out.u8(tag::DRAW_IMAGE);
            out.u32(id);
            out.rect(src);
            out.rect(dst);
            out.f32(*alpha);
            out.color_filter(filter(color_filter));
        }
        DrawOp::DrawShadow { outline, elevation, light, light_radius, ambient, spot, .. } => {
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
            out.paint(paint, tables, alpha);
        }
        DrawOp::DrawPath { path, offset, scale, paint } => {
            let id = resources.path_id(path);
            out.u8(tag::DRAW_PATH);
            out.u32(id);
            out.offset(*offset);
            out.f32(scale.0);
            out.f32(scale.1);
            out.paint(paint, tables, alpha);
        }
        DrawOp::ClipPath { path, offset } => {
            let id = resources.path_id(path);
            out.u8(tag::CLIP_PATH);
            out.u32(id);
            out.offset(*offset);
        }
        DrawOp::ClipOutPath { path, offset } => {
            let id = resources.path_id(path);
            out.u8(tag::CLIP_OUT_PATH);
            out.u32(id);
            out.offset(*offset);
        }
        DrawOp::Concat(matrix) => {
            out.u8(tag::CONCAT);
            for value in matrix.iter() {
                out.f32(*value);
            }
        }
        DrawOp::DrawVertices { vertices, blend_mode, paint } => {
            use compose_ui::canvas::VertexMode;
            out.u8(tag::DRAW_VERTICES);
            out.u8(match vertices.vertex_mode() {
                VertexMode::Triangles => 0,
                VertexMode::TriangleStrip => 1,
                VertexMode::TriangleFan => 2,
            });
            out.u32(vertices.positions().len() as u32);
            for point in vertices.positions().iter().chain(vertices.texture_coordinates()) {
                out.offset(*point);
            }
            for color in vertices.colors() {
                out.u32(color.to_argb());
            }
            out.u32(vertices.indices().len() as u32);
            for index in vertices.indices() {
                out.bytes.extend_from_slice(&index.to_le_bytes());
            }
            out.u8(*blend_mode as u8);
            out.paint(paint, tables, alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::Path;
    use compose_ui::platform::Gradient;

    /// The optimizations folding a layer into its one drawing's paint.
    fn fade() -> Optimizations {
        Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true)
    }

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

    /// A blended, filtered paint flags both in its style byte and writes them after its shader;
    /// a layer with a paint writes its bounds, alpha, mode and filter; the reader's lengths
    /// (`operand_bytes`) land on the next operation.
    #[test]
    fn a_paints_blend_mode_and_filter_follow_its_shader() {
        let mut list = DisplayList::default();
        let matrix = list.push_color_filter(ColorFilter::color_matrix(Default::default()));
        let paint = Paint::stroke(Color::BLACK, 2.0)
            .with_blend_mode(BlendMode::Multiply)
            .with_color_filter(matrix);
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), paint));
        let lighting = list.push_color_filter(ColorFilter::lighting(Color::RED, Color::BLUE));
        list.push(DrawOp::SaveLayer {
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
            alpha: 0.5,
            color_filter: Some(lighting),
            blend_mode: BlendMode::Screen,
        });
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        assert_eq!(bytes[1 + 16 + 4], 1 | 2 | 4, "stroke, blend mode, filter");
        let after_shader = 1 + 16 + 4 + 1 + 4 + 1 + 1;
        assert_eq!(bytes[after_shader], BlendMode::Multiply as u8);
        assert_eq!(bytes[after_shader + 1], color_filter::COLOR_MATRIX);
        assert_eq!(f32::from_le_bytes(bytes[after_shader + 2..][..4].try_into().unwrap()), 1.0);
        let layer = 1 + operand_bytes(bytes, 0);
        assert_eq!(layer, after_shader + 2 + 80);
        assert_eq!(bytes[layer], tag::SAVE_LAYER);
        assert_eq!(bytes[layer + 1 + 20], BlendMode::Screen as u8);
        assert_eq!(bytes[layer + 1 + 21], color_filter::LIGHTING);
        assert_eq!(read_u32(bytes, layer + 1 + 22), Color::RED.to_argb());
        assert_eq!(bytes[layer + 1 + operand_bytes(bytes, layer)], tag::RESTORE);
    }

    /// A paint with only a blend mode flags 2 and writes the mode after its shader; one with only
    /// a filter flags 4 and writes the filter there: each bit says which follows, read back
    /// apart.
    #[test]
    fn a_blend_mode_alone_and_a_filter_alone_have_their_own_flags() {
        let mut list = DisplayList::default();
        let rect = Rect::new(0.0, 0.0, 1.0, 1.0);
        let blended = Paint::fill(Color::RED).with_blend_mode(BlendMode::Xor);
        list.push(DrawOp::DrawRect(rect, blended));
        let tint = list.push_color_filter(ColorFilter::tint(Color::BLUE));
        list.push(DrawOp::DrawRect(rect, Paint::fill(Color::RED).with_color_filter(tint)));
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        let after_shader = 1 + 16 + 4 + 1 + 4 + 1 + 1;
        assert_eq!(bytes[1 + 16 + 4], 2, "fill, a blend mode");
        assert_eq!(bytes[after_shader], BlendMode::Xor as u8);
        let second = 1 + operand_bytes(bytes, 0);
        assert_eq!(second, after_shader + 1);
        assert_eq!(bytes[second], tag::DRAW_RECT);
        assert_eq!(bytes[second + 1 + 16 + 4], 4, "fill, a filter");
        assert_eq!(bytes[second + after_shader], color_filter::BLEND_MODE);
        assert_eq!(read_u32(bytes, second + after_shader + 1), Color::BLUE.to_argb());
        assert_eq!(bytes[second + after_shader + 5], BlendMode::SrcIn as u8);
        assert_eq!(bytes[second + 1 + operand_bytes(bytes, second)], tag::RESTORE);
    }

    /// A blurred paint (a shadow's) flags 8 and writes its radius last.
    #[test]
    fn a_paints_blur_radius_comes_last() {
        let mut list = DisplayList::default();
        let rect = Rect::new(0.0, 0.0, 1.0, 1.0);
        let paint = Paint::fill(Color::RED).with_blend_mode(BlendMode::Xor).with_blur(3.5);
        list.push(DrawOp::DrawRect(rect, paint));
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        assert_eq!(bytes[1 + 16 + 4], 2 | 8, "fill, a blend mode, a blur");
        let blur = 1 + 16 + 4 + 1 + 4 + 1 + 1 + 1;
        assert_eq!(f32::from_le_bytes(bytes[blur..blur + 4].try_into().unwrap()), 3.5);
        assert_eq!(bytes[1 + operand_bytes(bytes, 0)], tag::RESTORE);
    }

    /// A rectangle of `paint`'s bytes: its ARGB, its flags, and the color long if any.
    fn wide_rect(paint: Paint) -> (u32, u8, Option<u64>) {
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), paint));
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        let flags = bytes[1 + 16 + 4];
        let after_shader = 1 + 16 + 4 + 1 + 4 + 1 + 1;
        let long = (flags & COLOR_LONG != 0)
            .then(|| u64::from_le_bytes(bytes[after_shader..after_shader + 8].try_into().unwrap()));
        assert_eq!(bytes[1 + operand_bytes(bytes, 0)], tag::RESTORE, "the paint's length");
        (read_u32(bytes, 1 + 16), flags, long)
    }

    // Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/PaintTest.kt, testColorLongConfiguration
    // Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ColorTest.kt, toColorLong, toColorLong34
    /// A color of a space Android's paints take is written as its color long (Compose's packing,
    /// the ids Android's) after the rest, its ARGB being its sRGB conversion; an sRGB paint's
    /// bytes are what they were.
    #[test]
    fn a_wide_color_is_written_as_androids_color_long() {
        use compose_ui::colorspace::ColorSpaces;
        let adobe = Color::new_with(0.8916, 0.4980, 0.1168, 1.0, &ColorSpaces::ADOBE_RGB);
        let (argb, flags, long) = wide_rect(Paint::fill(adobe));
        assert_eq!(flags, COLOR_LONG);
        assert_eq!(argb, adobe.to_argb());
        assert_eq!(long, Some(adobe.value()));
        assert_eq!(long.unwrap() & 0x3F, 10, "Android's ADOBE_RGB");
        // With the other flags, it comes last.
        let blurred = Paint::fill(adobe).with_blend_mode(BlendMode::Xor).with_blur(2.0);
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 1.0, 1.0), blurred));
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let at = 1 + 16 + 4 + 1 + 4 + 1 + 1 + 1 + 4;
        assert_eq!(out.bytes[1 + 16 + 4], 2 | 8 | COLOR_LONG);
        assert_eq!(u64::from_le_bytes(out.bytes[at..at + 8].try_into().unwrap()), adobe.value());
        // Every space up to CIE XYZ keeps its id; sRGB writes no long.
        let color = Color::from_argb(0x1F2B_3D4E);
        for id in 1..14 {
            let converted = color.convert(ColorSpaces::get(id).unwrap());
            assert_eq!(wide_rect(Paint::fill(converted)).2, Some(converted.value()), "{id}");
        }
        assert_eq!(wide_rect(Paint::fill(color)), (0x1F2B_3D4E, 0, None));
    }

    // Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/PaintTest.kt, testColorLongConfigurationBt2020Hlg, testColorLongConfigurationBt2020Pq
    #[test]
    fn hlg_and_pq_take_androids_ids() {
        use compose_ui::colorspace::ColorSpaces;
        let hlg = Color::new_with(0.1, 0.2, 0.3, 1.0, &ColorSpaces::BT2020_HLG);
        assert_eq!(wide_rect(Paint::fill(hlg)).2.map(|long| long & 0x3F), Some(16));
        assert_eq!(color_long(hlg).unwrap() & !0x3F, hlg.value() & !0x3F);
        let pq = Color::new_with(0.1, 0.2, 0.3, 1.0, &ColorSpaces::BT2020_PQ);
        assert_eq!(wide_rect(Paint::fill(pq)).2.map(|long| long & 0x3F), Some(17));
    }

    // Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/PaintTest.kt, testColorLongConfigurationOkLabFallsBackToSrgb, testColorLongConfigurationCieXyzFallsBackToSrgb, testColorUnspecifiedSafeFallback, testColorUnspecifiedCompositeOverFallback
    // Compose: compose/ui/ui-graphics/src/androidHostTest/kotlin/androidx/compose/ui/graphics/AndroidColorTest.kt, testUnspecifiedCompositeOverToColorLong, testColorUnspecifiedToColorLong, testUnspecifiedColorSpaceWithVariousBlueChannels, testOklabFallbackOnOlderApi
    // Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ColorTest.kt, toColorLongUnspecified
    /// The spaces Android's paints don't take are drawn as their sRGB conversion, with no long.
    #[test]
    fn spaces_android_paints_dont_take_are_written_as_srgb() {
        use compose_ui::colorspace::ColorSpaces;
        let oklab = Color::new_with(0.5, 0.1, 0.1, 1.0, &ColorSpaces::OKLAB);
        let xyz = Color::new_with(0.5, 0.5, 0.5, 1.0, &ColorSpaces::CIE_XYZ);
        let lab = Color::new_with(50.0, 10.0, 10.0, 1.0, &ColorSpaces::CIE_LAB);
        for color in [oklab, xyz, lab] {
            assert_eq!(wide_rect(Paint::fill(color)), (color.to_argb(), 0, None), "{color}");
        }
        // Unspecified, and a color composited over it, are in the unspecified space.
        assert_eq!(color_long(Color::UNSPECIFIED), None);
        let real = Color::new(3.0 / 255.0, 252.0 / 255.0, 21.0 / 255.0, 0.38);
        let blended = real.composite_over(Color::UNSPECIFIED);
        assert_eq!(*blended.color_space(), ColorSpaces::UNSPECIFIED);
        assert_eq!(color_long(blended), None);
        for blue in [21.0 / 255.0, 45.0 / 255.0, 63.0 / 255.0, 1.0] {
            let color = Color::new_with(0.5, 0.5, blue, 0.8, &ColorSpaces::UNSPECIFIED);
            assert_eq!(color_long(color), None);
        }
        let (argb, flags, long) = wide_rect(Paint::fill(blended));
        assert_eq!((argb, flags, long), (blended.to_argb(), 0, None));
    }

    /// A wide color under a folded layer's alpha has its long faded too.
    #[test]
    fn a_folded_alpha_fades_the_color_long() {
        use compose_ui::colorspace::ColorSpaces;
        let p3 = Color::new_with(0.9, 0.2, 0.1, 1.0, &ColorSpaces::DISPLAY_P3);
        let mut writer = Writer::default();
        writer.paint(&Paint::fill(p3), (&[], &[]), 0.5);
        let long = u64::from_le_bytes(writer.bytes[11..19].try_into().unwrap());
        assert_eq!(writer.bytes[4], COLOR_LONG);
        assert_eq!(Color::from_value(long), p3.with_alpha(0.5));
    }

    // Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidBlendModeTest.kt, testBlendModeClearMapsToPorterDuffClear and the other mappings
    /// `Replayer.kt` reads the tags, filter kinds and blend modes this writes: its constants and
    /// its table of Android's modes, in Compose's order, are this file's.
    #[test]
    fn the_replayer_reads_the_same_constants_and_modes() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../android/compose-rs/src/main/java/rs/compose/android/Replayer.kt"
        );
        let kotlin = std::fs::read_to_string(path).expect("the replayer");
        let number = |name: &str| -> usize {
            let line = kotlin
                .lines()
                .find(|line| line.trim().starts_with(&format!("const val {name} = ")))
                .unwrap_or_else(|| panic!("{name} in Replayer.kt"));
            line.rsplit(' ').next().unwrap().parse().unwrap()
        };
        let constant = |name: &str| number(name) as u8;
        assert_eq!(number("LAYER_ENTRY_BYTES"), LAYER_ENTRY_BYTES);
        assert_eq!(number("ENTRY_BOUNDS"), layer_entry::BOUNDS);
        assert_eq!(number("ENTRY_END"), layer_entry::END);
        assert_eq!(number("EFFECT_BYTES"), layer_effect::BYTES);
        assert_eq!(number("EFFECT_KIND"), layer_effect::KIND);
        assert_eq!(number("EFFECT_RADII"), layer_effect::RADII);
        assert_eq!(number("EFFECT_TILE"), layer_effect::TILE);
        assert_eq!(constant("EFFECT_BLUR"), layer_effect::BLUR);
        assert_eq!(constant("COMPOSITING_EFFECT"), compositing::EFFECT);
        assert_eq!(constant("COMPOSITING_STRATEGY"), compositing::EFFECT - 1);
        assert_eq!(constant("SAVE_LAYER"), tag::SAVE_LAYER);
        assert_eq!(constant("BEGIN_EDGE_EFFECTS"), tag::BEGIN_EDGE_EFFECTS);
        assert_eq!(constant("DRAW_IMAGE"), tag::DRAW_IMAGE);
        assert_eq!(constant("FILTER_NONE"), color_filter::NONE);
        assert_eq!(constant("FILTER_BLEND_MODE"), color_filter::BLEND_MODE);
        assert_eq!(constant("FILTER_LIGHTING"), color_filter::LIGHTING);
        assert_eq!(constant("FILTER_COLOR_MATRIX"), color_filter::COLOR_MATRIX);
        assert_eq!(constant("BLEND_SRC_OVER"), BlendMode::SrcOver as u8);
        assert_eq!(constant("PAINT_STROKE"), paint_flag::STROKE);
        assert_eq!(constant("PAINT_BLEND_MODE"), paint_flag::BLEND_MODE);
        assert_eq!(constant("PAINT_COLOR_FILTER"), paint_flag::COLOR_FILTER);
        assert_eq!(constant("PAINT_BLUR"), paint_flag::BLUR);
        assert_eq!(constant("PAINT_COLOR_LONG"), COLOR_LONG);
        assert_eq!(constant("WIDE_GRADIENT"), WIDE_GRADIENT);
        assert_eq!(constant("CONCAT"), tag::CONCAT);
        assert_eq!(constant("CLIP_OUT_PATH"), tag::CLIP_OUT_PATH);
        assert_eq!(constant("DRAW_VERTICES"), tag::DRAW_VERTICES);
        // The flags are read through these names only, never as bare numbers.
        assert!(!kotlin.contains("flags and 1 ") && !kotlin.contains("flags and 16 "));
        let table = kotlin.split("BLEND_MODES = arrayOf(").nth(1).expect("the modes").split(')');
        let android: Vec<String> = table
            .into_iter()
            .next()
            .unwrap()
            .split(',')
            .map(|name| name.trim().trim_start_matches("BlendMode.").replace('_', ""))
            .filter(|name| !name.is_empty())
            .collect();
        let compose: Vec<String> = (0..=BlendMode::Luminosity as u8)
            .map(|index| format!("{:?}", mode_at(index)).to_uppercase())
            .collect();
        assert_eq!(android, compose);
    }

    /// The blend mode whose index is `index`.
    fn mode_at(index: u8) -> BlendMode {
        use BlendMode::*;
        [
            Clear, Src, Dst, SrcOver, DstOver, SrcIn, DstIn, SrcOut, DstOut, SrcAtop, DstAtop, Xor,
            Plus, Modulate, Screen, Overlay, Darken, Lighten, ColorDodge, ColorBurn, Hardlight,
            Softlight, Difference, Exclusion, Multiply, Hue, Saturation, Color, Luminosity,
        ]
        .into_iter()
        .find(|mode| *mode as u8 == index)
        .unwrap()
    }

    /// A layer carries its bounds, as Compose's `Canvas.saveLayer(bounds, paint)`: the replayer
    /// bounds the offscreen buffer by them, not by the recording canvas's size when it was
    /// recorded, which a recording kept as its node grows outlives (D646).
    #[test]
    fn a_layer_carries_its_bounds() {
        let mut list = DisplayList::default();
        let bounds = Rect::new(1.0, 2.0, 30.0, 40.0);
        list.push(DrawOp::SaveLayerAlpha { bounds, alpha: 0.5 });
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        list.push(DrawOp::DrawRect(Rect::new(1.0, 1.0, 2.0, 2.0), Paint::fill(Color::WHITE)));
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(
            &list,
            &mut resources,
            &mut GroupCache::new(Optimizations::NONE),
            &mut out,
        );
        assert_eq!(out.bytes[0], tag::SAVE_LAYER_ALPHA);
        let float = |at: usize| f32::from_le_bytes(out.bytes[at..at + 4].try_into().unwrap());
        assert_eq!([float(1), float(5), float(9), float(13)], [1.0, 2.0, 30.0, 40.0]);
        assert_eq!(float(17), 0.5);
        assert_eq!(operand_bytes(&out.bytes, 0), 20);
        assert_eq!(out.bytes[21], tag::DRAW_RECT);
    }

    /// A layer whose fill reaches past its bounds isn't folded into its paint: its offscreen
    /// buffer cuts the fill there, which a faded paint wouldn't (D648). Within them, it is; a
    /// stroke never is.
    #[test]
    fn a_layer_past_its_bounds_is_not_folded() {
        let layer = |rect: Rect, paint: Paint| {
            let mut list = DisplayList::default();
            let bounds = Rect::new(0.0, 0.0, 20.0, 20.0);
            list.push(DrawOp::SaveLayerAlpha { bounds, alpha: 0.5 });
            list.push(DrawOp::Translate(Offset::new(0.5, 0.0)));
            list.push(DrawOp::DrawRect(rect, paint));
            list.push(DrawOp::Restore);
            let (mut resources, mut out) = (Resources::default(), Writer::default());
            write_display_list(&list, &mut resources, &mut GroupCache::new(fade()), &mut out);
            out.bytes[0]
        };
        let fill = Paint::fill(Color::RED);
        assert_eq!(layer(Rect::new(0.0, 0.0, 19.5, 20.0), fill), tag::SAVE, "within: folded");
        // Moved half a pixel, past its bounds.
        assert_eq!(layer(Rect::new(0.0, 0.0, 20.0, 20.0), fill), tag::SAVE_LAYER_ALPHA);
        // Inverted, it's drawn sorted: x 10.5 to 30.5.
        assert_eq!(layer(Rect::new(30.0, 5.0, 10.0, 15.0), fill), tag::SAVE_LAYER_ALPHA);
        // A stroke, well within.
        let stroke = Paint::stroke(Color::RED, 3.0);
        assert_eq!(layer(Rect::new(4.0, 4.0, 10.0, 10.0), stroke), tag::SAVE_LAYER_ALPHA);
    }

    #[test]
    fn a_layer_with_one_drawing_fades_its_paint_instead() {
        let mut list = DisplayList::default();
        list.push(DrawOp::SaveLayerAlpha { bounds: Rect::new(0.0, 0.0, 10.0, 10.0), alpha: 0.5 });
        list.push(DrawOp::ClipRect(Rect::new(0.0, 0.0, 4.0, 4.0)));
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        list.push(DrawOp::Restore);
        // After it, the paint is whole again.
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        // Without the optimization (parity: the default has it), an offscreen layer, as Compose's.
        write_display_list(
            &list,
            &mut resources,
            &mut GroupCache::new(Optimizations::NONE),
            &mut out,
        );
        assert_eq!(out.bytes[0], tag::SAVE_LAYER_ALPHA);
        write_display_list(&list, &mut resources, &mut GroupCache::new(fade()), &mut out);
        assert_eq!(out.bytes[0], tag::SAVE);
        // Save, clip (17 bytes), then the rect: its color at 1 + 17 + 1 + 16.
        assert_eq!(read_u32(&out.bytes, 35), 0x8000_0000);
        let last = out.bytes.len() - 28;
        assert_eq!(out.bytes[last], tag::DRAW_RECT);
        assert_eq!(read_u32(&out.bytes, last + 17), 0xFF00_0000);
        // Two drawings: a layer, as their overlap must fade together.
        list.ops
            .insert(3, DrawOp::DrawRect(Rect::new(1.0, 1.0, 2.0, 2.0), Paint::fill(Color::WHITE)));
        write_display_list(&list, &mut resources, &mut GroupCache::new(fade()), &mut out);
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
        content
            .push(DrawOp::SaveLayerAlpha { bounds: Rect::new(0.0, 0.0, 10.0, 10.0), alpha: 0.5 });
        content.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK)));
        content.push(DrawOp::Restore);
        let mut list = DisplayList::default();
        boundary(&mut list, 7);
        list.groups = vec![group(7, 0, content, 0)];
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::new(fade()), &mut out);
        assert_eq!(out.bytes[0], tag::GROUP);
        // Its record, then the frame naming it.
        assert_eq!(read_u32(&out.bytes, 17) as usize, out.bytes.len() - 21 - 9);
        assert_eq!(out.bytes[21], tag::SAVE);
    }

    #[test]
    fn a_path_is_defined_once_and_dropped_with_its_owner() {
        let path = Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap().data();
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
        let path = Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap().data();
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
            color_filter: None,
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
                // An id, a fill type, no step, a count, and a triangle's 4 commands.
                tag::DEFINE_PATH => 14 + 3 * 9 + 1,
                other => panic!("not a resource change: {other}"),
            };
        }
        tags
    }

    /// A frame drawing `path`'s contents now, written, the list returned (holding them).
    fn frame_drawing(resources: &mut Resources, path: &Path) -> DisplayList {
        let mut list = DisplayList::default();
        list.push(DrawOp::DrawPath {
            path: path.data(),
            offset: Offset::ZERO,
            scale: (1.0, 1.0),
            paint: Paint::fill(Color::BLACK),
        });
        write_display_list(&list, resources, &mut GroupCache::default(), &mut Writer::default());
        list
    }

    /// A path changed after the Kotlin side took its definition is defined again under its id
    /// (D532); if that definition is cancelled before it's taken (the path gone), the id the
    /// Kotlin side holds is still dropped.
    #[test]
    fn a_cancelled_redefinition_still_drops_the_id() {
        let mut resources = Resources::default();
        let path = Path::new();
        path.add_rect(Rect::new(0.0, 0.0, 1.0, 1.0));
        let first = frame_drawing(&mut resources, &path);
        let mut taken = Writer::default();
        resources.write_pending(&mut taken);
        let id = read_u32(&taken.bytes, 1);
        path.reset();
        path.add_rect(Rect::new(0.0, 0.0, 2.0, 2.0));
        drop(first);
        let second = frame_drawing(&mut resources, &path);
        resources.collect();
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!((pending.bytes[0], read_u32(&pending.bytes, 1)), (tag::DEFINE_PATH, id));
        // Changed again, and gone before the Kotlin side takes that definition.
        path.reset();
        drop(second);
        let third = frame_drawing(&mut resources, &path);
        drop((third, path));
        resources.collect();
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!(pending.bytes, [&[tag::DROP_PATH][..], &id.to_le_bytes()].concat());
    }

    /// Resource ids wrap round after 4 billion, skipping those in use.
    #[test]
    fn wrapped_ids_skip_those_in_use() {
        let mut resources = Resources::default();
        let first = Path::new();
        first.add_rect(Rect::new(0.0, 0.0, 1.0, 1.0));
        let _first_list = frame_drawing(&mut resources, &first);
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        assert_eq!(read_u32(&pending.bytes, 1), 1);
        resources.next_id = u32::MAX - 1;
        let (second, third) = (Path::new(), Path::new());
        second.add_rect(Rect::new(0.0, 0.0, 2.0, 2.0));
        third.add_rect(Rect::new(0.0, 0.0, 3.0, 3.0));
        let _second_list = frame_drawing(&mut resources, &second);
        let _third_list = frame_drawing(&mut resources, &third);
        let mut pending = Writer::default();
        resources.write_pending(&mut pending);
        let ids = resource_ids(&pending.bytes);
        assert_eq!(ids, [u32::MAX, 2], "0 and 1 (in use) skipped");
    }

    /// The ids of the paths `bytes` defines (no steps), in order.
    fn resource_ids(bytes: &[u8]) -> Vec<u32> {
        let (mut ids, mut at) = (Vec::new(), 0);
        while at < bytes.len() {
            assert_eq!(bytes[at], tag::DEFINE_PATH);
            ids.push(read_u32(bytes, at + 1));
            let count = read_u32(bytes, at + 10) as usize;
            at += 14;
            for _ in 0..count {
                at += 1 + 8 * [1, 1, 2, 3, 0][bytes[at] as usize];
            }
        }
        ids
    }

    /// Frames written but not taken by the Kotlin side (a view hidden by its own visibility while
    /// its window is visible keeps getting frames, but no `onDraw`, D536): a path redefined in
    /// place each frame leaves one pending definition (a review found a slot per frame kept).
    #[test]
    fn pending_definitions_stay_bounded_over_undrawn_frames() {
        let mut resources = Resources::default();
        let path = Path::new();
        path.add_rect(Rect::new(0.0, 0.0, 1.0, 1.0));
        let mut list = frame_drawing(&mut resources, &path);
        resources.collect();
        let mut taken = Writer::default();
        resources.write_pending(&mut taken);
        for frame in 0..1000 {
            path.reset();
            path.add_rect(Rect::new(0.0, 0.0, frame as f32, 1.0));
            drop(list);
            list = frame_drawing(&mut resources, &path);
            resources.collect();
        }
        drop(list);
        assert!(
            resources.pending_paths.len() <= 2,
            "pending path slots after 1000 undrawn frames: {}",
            resources.pending_paths.len()
        );
    }

    /// A path's operation is in its definition as steps (D535), its operands' contents inline:
    /// the bytes `Replayer.kt` reads.
    #[test]
    fn an_operations_steps_are_in_its_definition() {
        let (square, inner) = (Path::new(), Path::new());
        square.add_rect(Rect::new(0.0, 0.0, 4.0, 4.0));
        inner.add_rect(Rect::new(1.0, 1.0, 2.0, 2.0));
        let framed = Path::combine(PathOperation::Difference, &square, &inner);
        framed.line_to(9.0, 9.0);
        let mut resources = Resources::default();
        let _list = frame_drawing(&mut resources, &framed);
        let mut out = Writer::default();
        resources.write_pending(&mut out);
        let mut expected = Writer::default();
        expected.u8(tag::DEFINE_PATH);
        expected.u32(read_u32(&out.bytes, 1));
        expected.u8(1); // even-odd, as the result of Skia's path ops
        expected.u32(4);
        // The path's contents before (empty): what it keeps if the operation fails.
        expected.u8(step::PUSH);
        expected.u8(0);
        expected.u32(0);
        expected.u32(0);
        for (left, right) in [(0.0, 4.0), (1.0, 2.0)] {
            expected.u8(step::PUSH);
            expected.u8(0);
            expected.u32(0);
            expected.u32(5);
            let corners = [(left, left), (left, right), (right, right), (right, left)];
            for (index, (x, y)) in corners.into_iter().enumerate() {
                // Counterclockwise from the top left: down the left side first.
                expected.u8(if index == 0 { command::MOVE_TO } else { command::LINE_TO });
                expected.f32(if index == 2 || index == 3 { right } else { left });
                expected.f32(if index == 1 || index == 2 { right } else { left });
                let _ = (x, y);
            }
            expected.u8(command::CLOSE);
        }
        expected.u8(step::COMBINE_OR);
        expected.u8(0); // a difference
        expected.u32(1);
        expected.u8(command::LINE_TO);
        expected.f32(9.0);
        expected.f32(9.0);
        assert_eq!(out.bytes, expected.bytes);
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
        assert_eq!(colors, [Color::RED.to_argb(), Color::GREEN.to_argb(), Color::BLUE.to_argb()]);
        let at = at + 4 + 12;
        assert_eq!(read_u32(bytes, at), 3);
        let stops: Vec<f32> = (0..3).map(|index| read_f32(bytes, at + 4 + index * 4)).collect();
        assert_eq!(stops, [0.0, 0.25, 1.0]);
        assert_eq!(read_f32(bytes, at + 16), 0.5, "the shader's alpha");
        assert_eq!(bytes.len(), at + 20);
    }

    // Compose: compose/ui/ui-graphics/src/androidMain/kotlin/androidx/compose/ui/graphics/AndroidCanvas.android.kt, concat, clipPath(ClipOp.Difference), drawVertices (no Compose test)
    /// A bitmap canvas's matrix, clip out and vertices are written as `Replayer.kt` reads them.
    #[test]
    fn a_matrix_a_clip_out_and_vertices_are_written() {
        use compose_ui::canvas::{VertexMode, Vertices};
        let path = Path::new();
        path.add_rect(Rect::new(0.0, 0.0, 2.0, 2.0));
        let corners = [Offset::new(0.0, 0.0), Offset::new(4.0, 0.0), Offset::new(0.0, 4.0)];
        let vertices = Vertices::new(
            VertexMode::TriangleFan,
            &corners,
            &corners,
            &[Color::RED; 3],
            &[0, 2, 1],
        );
        let mut list = DisplayList::default();
        list.push(DrawOp::Concat(Box::new([1.0, 0.5, 2.0, 0.0, 1.0, 3.0, 0.0, 0.0, 1.0])));
        list.push(DrawOp::ClipOutPath { path: path.data(), offset: Offset::new(1.0, 1.0) });
        let paint = Paint::fill(Color::BLACK).with_blend_mode(BlendMode::Xor);
        let draw = DrawOp::DrawVertices {
            vertices: Arc::new(vertices),
            blend_mode: BlendMode::Dst,
            paint,
        };
        list.push(draw);
        list.push(DrawOp::Restore);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        assert_eq!(bytes[0], tag::CONCAT);
        assert_eq!(read_f32(bytes, 1 + 4), 0.5);
        assert_eq!(read_f32(bytes, 1 + 5 * 4), 3.0);
        let clip = 1 + operand_bytes(bytes, 0);
        assert_eq!(bytes[clip], tag::CLIP_OUT_PATH);
        assert_eq!(read_f32(bytes, clip + 1 + 4), 1.0);
        let at = clip + 1 + operand_bytes(bytes, clip);
        assert_eq!((bytes[at], bytes[at + 1]), (tag::DRAW_VERTICES, 2), "a fan");
        assert_eq!(read_u32(bytes, at + 2), 3);
        let colors_at = at + 2 + 4 + 3 * 16;
        assert_eq!(read_u32(bytes, colors_at), Color::RED.to_argb());
        let indices_at = colors_at + 3 * 4;
        assert_eq!(read_u32(bytes, indices_at), 3);
        assert_eq!(&bytes[indices_at + 4..indices_at + 10], &[0, 0, 2, 0, 1, 0]);
        assert_eq!(bytes[indices_at + 10], BlendMode::Dst as u8);
        assert_eq!(bytes[indices_at + 11 + 4], paint_flag::BLEND_MODE, "the paint follows");
        let end = at + 1 + operand_bytes(bytes, at);
        assert_eq!((bytes[end], end + 1), (tag::RESTORE, bytes.len()));
        assert_eq!(
            resources.pending.len() + resources.pending_paths.len(),
            1,
            "the path's definition"
        );
    }

    /// A sweep gradient of `colors`' bytes: its tile mode byte, ARGB colors and longs if any.
    fn sweep_bytes(colors: &[Color]) -> (u8, Vec<u32>, Vec<u64>) {
        let mut list = DisplayList::default();
        let gradient = gradient(colors, None, TileMode::Clamp);
        gradient_rect(
            &mut list,
            Shader::SweepGradient { gradient, center: Offset::ZERO, alpha: 1.0 },
        );
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(&list, &mut resources, &mut GroupCache::default(), &mut out);
        let bytes = &out.bytes;
        let at = RECT_BEFORE_SHADER;
        assert_eq!(bytes[at], shader::SWEEP);
        let count = read_u32(bytes, at + 2 + 8) as usize;
        let argb = (0..count).map(|index| read_u32(bytes, at + 2 + 8 + 4 + index * 4)).collect();
        let longs_at = at + 2 + 8 + 4 + count * 4 + 4;
        let wide = bytes[at + 1] & WIDE_GRADIENT != 0;
        let long = |index: usize| {
            u64::from_le_bytes(
                bytes[longs_at + index * 8..longs_at + index * 8 + 8].try_into().unwrap(),
            )
        };
        let longs = if wide { (0..count).map(long).collect() } else { Vec::new() };
        assert_eq!(1 + operand_bytes(bytes, 0), bytes.len(), "the paint's length");
        (bytes[at + 1], argb, longs)
    }

    // Compose: compose/ui/ui-graphics/src/androidMain/kotlin/androidx/compose/ui/graphics/AndroidShader.android.kt, ActualSweepGradientShader (colors.toPlatformLongArray(); no Compose test)
    /// A gradient with a wide color carries its colors' longs after its stops, as Compose's
    /// gradients take longs (API 29); an sRGB one's bytes are what they were.
    #[test]
    fn a_wide_gradient_carries_its_color_longs() {
        use compose_ui::colorspace::ColorSpaces;
        assert_eq!(
            sweep_bytes(&[Color::RED, Color::BLUE]),
            (0, vec![0xFFFF_0000, 0xFF00_00FF], vec![])
        );
        let p3 = Color::new_with(0.9, 0.2, 0.1, 1.0, &ColorSpaces::DISPLAY_P3);
        let blue = Color::new_with(0.1, 0.2, 0.9, 1.0, &ColorSpaces::DISPLAY_P3);
        let (tile, argb, longs) = sweep_bytes(&[p3, blue]);
        assert_eq!(tile, WIDE_GRADIENT);
        assert_eq!(argb, [p3.to_argb(), blue.to_argb()], "their sRGB conversions");
        assert_eq!(longs, [p3.value(), blue.value()]);
        // Two spaces, which Android's gradients don't take: both in extended sRGB.
        let (_, _, longs) = sweep_bytes(&[p3, Color::BLUE]);
        let extended = |color: Color| color.convert(&ColorSpaces::EXTENDED_SRGB).value();
        assert_eq!(longs, [extended(p3), extended(Color::BLUE)]);
        assert_eq!(longs[0] & 0x3F, 2, "Android's EXTENDED_SRGB");
        // One color, written twice.
        assert_eq!(sweep_bytes(&[p3]).2, [p3.value(), p3.value()]);
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

    /// The replayer reads what edge effects hold by a length written at their end, and carries on
    /// with what follows it.
    #[test]
    fn edge_effects_say_how_long_what_they_hold_is() {
        let mut list = DisplayList::default();
        list.push(DrawOp::BeginEdgeEffects {
            owner: 7,
            bounds: Rect::new(1.0, 2.0, 3.0, 4.0),
            stamp: 9,
            max_elevation: 30.0,
            glow_padding: [5.0, 6.0, 7.0, 8.0],
        });
        list.push(DrawOp::Save);
        list.push(DrawOp::Restore);
        list.push(DrawOp::EndEdgeEffects);
        list.push(DrawOp::Save);
        let (mut resources, mut out) = (Resources::default(), Writer::default());
        write_display_list(
            &list,
            &mut resources,
            &mut GroupCache::new(Optimizations::defaults()),
            &mut out,
        );
        let bytes = &out.bytes;
        assert_eq!(bytes[0], tag::BEGIN_EDGE_EFFECTS);
        let le = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
        assert_eq!(le(1), 7);
        assert_eq!(le(1 + 4 + 16), 9);
        let content = 1 + EDGE_EFFECTS_HEADER_BYTES;
        assert_eq!(le(content - 4), 2);
        assert_eq!(&bytes[content..content + 2], &[tag::SAVE, tag::RESTORE]);
        assert_eq!(&bytes[content + 2..], &[tag::SAVE]);
    }

    /// A layer folded into a gradient's paint multiplies the shader's alpha (what Android's
    /// paint alpha is with a shader), not only the color's.
    #[test]
    fn a_folded_layer_fades_a_shaders_alpha() {
        let mut list = DisplayList::default();
        list.push(DrawOp::SaveLayerAlpha { bounds: Rect::new(0.0, 0.0, 10.0, 10.0), alpha: 0.5 });
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
        write_display_list(&list, &mut resources, &mut GroupCache::new(fade()), &mut out);
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
        let path = Path::from_svg("M0,0 L10,0 L10,10 Z").unwrap().data();
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
