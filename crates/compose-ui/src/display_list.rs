//! The display list: what a draw pass records and a renderer replays (docs/07-platforms.md).
//! A subset of the `Canvas` API that both Skia and Android's `Canvas` support directly.

use std::fmt::Write as _;
use std::sync::Arc;

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
    /// The part `src` of a bitmap (in its pixels) drawn into `dst`, at `alpha`.
    DrawImage {
        image: crate::image::ImageBitmap,
        src: Rect,
        dst: Rect,
        alpha: f32,
    },
    /// The shadows of `outline` raised by `elevation` pixels, lit from `light` (x, y, z in the
    /// window's pixels, not moved by transforms) of `light_radius`: an ambient shadow in
    /// `ambient` and a spot shadow in `spot` (their alphas Compose's 3.9 % and 19 %), as
    /// Compose Desktop draws them with Skia's `ShadowUtils`.
    DrawShadow {
        outline: crate::graphics::Outline,
        elevation: f32,
        light: (f32, f32, f32),
        light_radius: f32,
        ambient: crate::graphics::Color,
        spot: crate::graphics::Color,
    },
    /// An arc of the oval in `oval`: from `start_angle` degrees (0 at 3 o'clock, clockwise),
    /// over `sweep_angle`, closed through the center if `use_center`.
    DrawArc {
        oval: Rect,
        start_angle: f32,
        sweep_angle: f32,
        use_center: bool,
        paint: Paint,
    },
    /// A path, scaled by `scale` then moved by `offset` (a vector image's viewport to its size).
    DrawPath {
        path: std::sync::Arc<crate::path::Path>,
        offset: Offset,
        scale: (f32, f32),
        paint: Paint,
    },
    /// Clips what follows (until the matching restore of an earlier save) to `path` moved by
    /// `offset` (its fill type deciding what's inside), as `clipPath`.
    ClipPath {
        path: std::sync::Arc<crate::path::Path>,
        offset: Offset,
    },
}

impl DrawOp {
    /// The paint of a drawing operation, if it has one.
    pub fn paint_mut(&mut self) -> Option<&mut Paint> {
        match self {
            DrawOp::DrawRect(_, paint)
            | DrawOp::DrawRoundRect(_, _, paint)
            | DrawOp::DrawLine(_, _, paint)
            | DrawOp::DrawArc { paint, .. }
            | DrawOp::DrawPath { paint, .. } => Some(paint),
            _ => None,
        }
    }
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

/// A recording, and the display list of a frame: operations, the shaders their paints name,
/// and the repaint boundaries drawn among them by reference. Cleared and refilled each draw
/// pass, keeping its capacity.
///
/// A frame's list (`UiTree::display_list`) holds what's drawn outside every repaint boundary,
/// and in `groups` each boundary's group, whose content is the recording the boundary keeps,
/// shared (`Arc`), not copied: a renderer walks the tree, drawing a group's content where its
/// [`Boundary`] is (Skia), or copying what it made of a group while its version stays
/// (Android's serializer). [`flatten`](DisplayList::flatten) copies every group into one list
/// of operations, for tests and dumps.
#[derive(Clone, Default, Debug)]
pub struct DisplayList {
    pub ops: Vec<DrawOp>,
    /// The shaders its paints refer to (`Paint::shader`), by index: each recording's own, so a
    /// group's content is drawn with its content's shaders.
    pub shaders: Vec<crate::brush::Shader>,
    /// The repaint boundaries drawn inside it, by reference, in drawing order: each before the
    /// operation at its `at`. Their groups are the frame's (`groups`), in the same order.
    pub boundaries: Vec<Boundary>,
    /// In a frame: the group of each repaint boundary drawn (`Modifier.repaint_boundary`,
    /// graphics layers, lazy items), in drawing order, a group before the ones nested in it
    /// (its `descendants`). The frame's boundaries, each followed by its group's content's
    /// (depth first), name them one by one.
    pub groups: Vec<DrawGroup>,
    /// A translation before this index isn't merged into: a group's operations stay its own.
    pub(crate) merge_floor: usize,
}

/// Where a recording draws a repaint boundary's group: before its operation `at`. `key` is the
/// group's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Boundary {
    pub at: usize,
    pub key: u64,
}

/// A repaint boundary's group in a frame: its content, kept by the boundary and shared by each
/// frame drawing it, keyed by its node, with a version.
///
/// A boundary whose node's first drawing modifier is a graphics layer has the layer's
/// properties in `layer` (as Compose's `graphicsLayer` sets its render node's), outside its
/// content: a renderer drawing operations applies them around it
/// ([`LayerProperties::operations`](crate::platform::LayerProperties::operations), as
/// [`flatten`](DisplayList::flatten) does); one keeping a recording per group sets them as its
/// render node's properties (Android).
///
/// `version` comes from a counter of the tree, bumped whenever the group's content is drawn
/// again or a group inside it changed version: the content is the same while it is, except for
/// the layers of groups inside it, which their own `layer` carries. A change of a layer's
/// properties alone (an animated alpha or rotation) changes no version, neither its own nor its
/// parents'. A key with the same version never draws something else.
#[derive(Clone, Debug)]
pub struct DrawGroup {
    pub key: u64,
    pub version: u64,
    pub layer: Option<crate::graphics_layer::LayerProperties>,
    /// Its operations in its node's coordinates (its layer's left out), the boundaries nested
    /// in it by reference.
    pub content: Arc<DisplayList>,
    /// How many groups are nested in it, at any depth: the ones after it in the frame's
    /// `groups`.
    pub descendants: usize,
    /// It draws nothing (no operation, its own or a nested group's): a renderer skips it, its
    /// layer and its nested groups.
    pub empty: bool,
}

/// A display list with every group's operations in place ([`DisplayList::flatten`]): for tests
/// and dumps.
#[derive(Clone, Default, Debug)]
pub struct FlatDisplayList {
    pub ops: Vec<DrawOp>,
    /// Each group drawn (not empty), after the ones nested in it.
    pub groups: Vec<FlatGroup>,
    /// The shaders the paints refer to, by index: every recording's, their paints' indices
    /// shifted to them.
    pub shaders: Vec<crate::brush::Shader>,
}

/// A group in a flattened list: its content `ops[start..end]`, in its node's coordinates, keyed
/// by its node, with its version ([`DrawGroup`]). Its layer's operations are just outside it:
/// `ops[layer_start..start]` before it, which are, in this order and each only if needed,
/// `Save` (if it transforms or clips), `Transform` (if it scales, rotates or moves), `ClipRect`
/// (if it clips to its bounds) and `SaveLayerAlpha` (if its alpha is below 1); and
/// `ops[end..layer_end]` after it, a `Restore` for each save. Without a layer,
/// `layer_start == start` and `layer_end == end`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct FlatGroup {
    pub key: u64,
    pub version: u64,
    pub start: usize,
    pub end: usize,
    pub layer_start: usize,
    pub layer_end: usize,
    pub layer: Option<crate::graphics_layer::LayerProperties>,
}

impl DisplayList {
    pub fn clear(&mut self) {
        self.ops.clear();
        self.groups.clear();
        self.merge_floor = 0;
        self.boundaries.clear();
        self.shaders.clear();
    }

    pub fn push(&mut self, op: DrawOp) {
        self.ops.push(op);
    }

    /// Keeps `shader` for this list's paints, returning the index they refer to it by.
    pub fn push_shader(&mut self, shader: crate::brush::Shader) -> crate::brush::ShaderIndex {
        self.shaders.push(shader);
        crate::brush::ShaderIndex(self.shaders.len() as u32 - 1)
    }

    /// The frame with every group's operations in place, its layer's around it: what a
    /// renderer draws, as one list. For tests and dumps (renderers walk the groups).
    pub fn flatten(&self) -> FlatDisplayList {
        let mut flat = FlatDisplayList::default();
        let mut next = 0;
        flatten_into(self, &self.groups, &mut next, &mut flat);
        flat
    }

    /// The flattened list's dump ([`FlatDisplayList::dump`]): for golden tests.
    pub fn dump(&self) -> String {
        self.flatten().dump()
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
}

impl FlatDisplayList {
    /// A text dump, one operation per line, indented by save depth: for golden tests.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        for op in &self.ops {
            if matches!(op, DrawOp::Restore) {
                depth = depth.saturating_sub(1);
            }
            let _ = write!(out, "{}", "  ".repeat(depth));
            let paint_text = |paint: &Paint| paint_text(paint, &self.shaders);
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
                DrawOp::DrawImage { image, src, dst, alpha } => writeln!(
                    out,
                    "image {}x{} {} into {} alpha {alpha}",
                    image.width(),
                    image.height(),
                    rect_text(src),
                    rect_text(dst)
                ),
                DrawOp::DrawPath { path, offset, scale, paint } => writeln!(
                    out,
                    "path of {} commands at {},{} scale {}x{} {}",
                    path.commands().len(),
                    offset.x,
                    offset.y,
                    scale.0,
                    scale.1,
                    paint_text(paint)
                ),
                DrawOp::DrawArc { oval, start_angle, sweep_angle, use_center, paint } => writeln!(
                    out,
                    "arc {} from {start_angle} sweep {sweep_angle}{} {}",
                    rect_text(oval),
                    if *use_center { " to center" } else { "" },
                    paint_text(paint)
                ),
                DrawOp::DrawShadow { outline, elevation, ambient, spot, .. } => {
                    let (rect, radii) = match outline {
                        crate::graphics::Outline::Rectangle(rect) => (rect, [0.0; 4]),
                        crate::graphics::Outline::Rounded(rect, radii) => (rect, *radii),
                    };
                    writeln!(
                        out,
                        "shadow {} radii {:?} elevation {elevation} {ambient:?} {spot:?}",
                        rect_text(rect),
                        radii
                    )
                }
                DrawOp::Transform { pivot, scale, rotation, translation } => writeln!(
                    out,
                    "transform pivot {},{} scale {}x{} rotate {} translate {},{}",
                    pivot.x, pivot.y, scale.0, scale.1, rotation, translation.x, translation.y
                ),
                DrawOp::ClipPath { path, offset } => writeln!(
                    out,
                    "clip path of {} commands at {},{}",
                    path.commands().len(),
                    offset.x,
                    offset.y
                ),
            };
            if matches!(op, DrawOp::Save | DrawOp::SaveLayerAlpha(_)) {
                depth += 1;
            }
        }
        out
    }
}

/// Appends `recording` (the frame, or a group's content) to `flat`, each group it draws (the
/// frame's `groups` from `next` on) in place.
fn flatten_into(
    recording: &DisplayList,
    groups: &[DrawGroup],
    next: &mut usize,
    flat: &mut FlatDisplayList,
) {
    // Its shaders become the list's, its paints' indices shifted to them.
    let base = if recording.shaders.is_empty() { 0 } else { flat.shaders.len() as u32 };
    flat.shaders.extend_from_slice(&recording.shaders);
    let mut from = 0;
    for boundary in &recording.boundaries {
        let at = boundary.at.clamp(from, recording.ops.len());
        extend_shifted(&mut flat.ops, &recording.ops[from..at], base);
        flatten_group(boundary, groups, next, flat);
        from = at;
    }
    extend_shifted(&mut flat.ops, &recording.ops[from..], base);
}

/// Appends the group at `next` (and the ones nested in it) to `flat`: its layer's operations
/// around its content, none if it draws nothing.
fn flatten_group(
    boundary: &Boundary,
    groups: &[DrawGroup],
    next: &mut usize,
    flat: &mut FlatDisplayList,
) {
    let group = groups.get(*next);
    debug_assert!(
        group.is_some_and(|group| group.key == boundary.key),
        "the frame's groups don't match its boundaries"
    );
    let Some(group) = group else { return };
    *next += 1;
    let layer_start = flat.ops.len();
    if let Some(layer) = group.layer {
        flat.ops.extend(layer.operations());
    }
    let start = flat.ops.len();
    flatten_into(&group.content, groups, next, flat);
    let end = flat.ops.len();
    if end == start {
        // Nothing to apply them to.
        flat.ops.truncate(layer_start);
        return;
    }
    let restores = group.layer.map_or(0, |layer| layer.restores());
    flat.ops.extend((0..restores).map(|_| DrawOp::Restore));
    flat.groups.push(FlatGroup {
        key: group.key,
        version: group.version,
        start,
        end,
        layer_start,
        layer_end: flat.ops.len(),
        layer: group.layer,
    });
}

/// Appends `ops`, from a list whose shaders are the target's from `base` on.
fn extend_shifted(target: &mut Vec<DrawOp>, ops: &[DrawOp], base: u32) {
    let start = target.len();
    target.extend_from_slice(ops);
    if base > 0 {
        for op in &mut target[start..] {
            if let Some(index) = op.paint_mut().and_then(|paint| paint.shader.as_mut()) {
                index.0 += base;
            }
        }
    }
}

fn rect_text(rect: &Rect) -> String {
    format!("{},{} {}x{}", rect.left, rect.top, rect.width(), rect.height())
}

fn paint_text(paint: &Paint, shaders: &[crate::brush::Shader]) -> String {
    let color = match paint.shader.and_then(|index| shaders.get(index.0 as usize)) {
        Some(shader) => shader_text(shader),
        None => format!("{:?}", paint.color),
    };
    match paint.style {
        crate::graphics::PaintStyle::Fill => color,
        crate::graphics::PaintStyle::Stroke(width) => format!("{color} stroke {width}"),
    }
}

fn shader_text(shader: &crate::brush::Shader) -> String {
    use crate::brush::Shader;
    let gradient = shader.gradient();
    let colors: Vec<String> = gradient.colors.iter().map(|color| format!("{color:?}")).collect();
    let stops = gradient.stops.as_ref().map_or(String::new(), |stops| format!(" at {stops:?}"));
    let alpha =
        if shader.alpha() < 1.0 { format!(" alpha {}", shader.alpha()) } else { String::new() };
    let geometry = match shader {
        Shader::LinearGradient { from, to, .. } => {
            format!("linear {},{} to {},{}", from.x, from.y, to.x, to.y)
        }
        Shader::RadialGradient { center, radius, .. } => {
            format!("radial at {},{} radius {radius}", center.x, center.y)
        }
        Shader::SweepGradient { center, .. } => format!("sweep at {},{}", center.x, center.y),
    };
    format!("{geometry} [{}]{stops} {:?}{alpha}", colors.join(", "), gradient.tile_mode)
}
