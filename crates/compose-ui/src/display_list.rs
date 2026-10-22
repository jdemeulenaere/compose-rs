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

/// The operations of one frame. Cleared and refilled each draw pass, keeping its capacity.
#[derive(Clone, Default, Debug)]
pub struct DisplayList {
    pub ops: Vec<DrawOp>,
    /// The operations each repaint boundary drew (`Modifier.repaint_boundary`, graphics
    /// layers, lazy items), in its own coordinates, keyed by its node, with a version: a
    /// renderer can keep a group's recording while its version stays the same (Android's
    /// render nodes).
    pub groups: Vec<DrawGroup>,
    /// A translation before this index isn't merged into: a group's operations stay its own.
    pub(crate) merge_floor: usize,
    /// In a recording a repaint boundary keeps (render.rs): the boundaries drawn inside it, by
    /// reference, each before the operation at its index.
    pub(crate) boundaries: Vec<(usize, crate::tree::NodeId)>,
    /// The shaders the operations' paints refer to (`Paint::shader`), by index.
    pub shaders: Vec<crate::brush::Shader>,
}

/// A repaint boundary's operations: its content `ops[start..end]`, in its node's coordinates,
/// keyed by its node.
///
/// A boundary whose node's first drawing modifier is a graphics layer has the layer's
/// properties in `layer` (as Compose's `graphicsLayer` sets its render node's), applied by the
/// operations just outside its content: `ops[layer_start..start]` before it, which are, in this
/// order and each only if needed, `Save` (if it transforms or clips), `Transform` (if it
/// scales, rotates or moves), `ClipRect` (if it clips to its bounds) and `SaveLayerAlpha` (if
/// its alpha is below 1); and `ops[end..layer_end]` after it, a `Restore` for each save. A
/// renderer that ignores groups draws them as any other operations (Skia); one that keeps a
/// recording per group skips them and applies `layer` as its render node's properties
/// (Android). Without a layer, `layer_start == start` and `layer_end == end`.
///
/// `version` comes from a counter of the tree, bumped whenever the group's content is drawn
/// again or a group inside it changed version: the content is the same while it is, except for
/// the operations applying the layers of groups inside it, which their own `layer` carries. A
/// change of a layer's properties alone (an animated alpha or rotation) changes no version,
/// neither its own nor its parents'. A key with the same version never draws something else.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct DrawGroup {
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

    /// Appends `ops`, from a list whose shaders are this list's from `base` on.
    pub(crate) fn extend_shifted(&mut self, ops: &[DrawOp], base: u32) {
        let start = self.ops.len();
        self.ops.extend_from_slice(ops);
        if base > 0 {
            for op in &mut self.ops[start..] {
                if let Some(index) = op.paint_mut().and_then(|paint| paint.shader.as_mut()) {
                    index.0 += base;
                }
            }
        }
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
