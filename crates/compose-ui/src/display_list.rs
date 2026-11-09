//! The display list: what a draw pass records and a renderer replays (docs/07-platforms.md).
//! A subset of the `Canvas` API that both Skia and Android's `Canvas` support directly.

use std::fmt::Write as _;
use std::sync::Arc;

use crate::color_filter::{BlendMode, ColorFilter, ColorFilterIndex};
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
    /// Saves, then draws what follows into a layer bounded by `bounds`, composited with `alpha`
    /// at the matching restore, as a graphics layer's alpha. Bounded as Compose's
    /// `Canvas.saveLayer(bounds, paint)`: a recording kept as its node grows bounds it by
    /// them, not by its size when it was recorded (D646).
    SaveLayerAlpha {
        bounds: Rect,
        alpha: f32,
    },
    /// `SaveLayerAlpha` whose layer is also composited through `color_filter` (of the list's
    /// `color_filters`) and with `blend_mode`, as `Canvas.saveLayer(bounds, paint)` with such a
    /// paint: a painter's layer, an icon's tint producer. A layer with neither is a
    /// `SaveLayerAlpha`.
    SaveLayer {
        bounds: Rect,
        alpha: f32,
        color_filter: Option<ColorFilterIndex>,
        blend_mode: BlendMode,
    },
    /// Saves, and draws what follows into a layer composited through `effect` (a blur) at the
    /// matching restore, as a graphics layer's render effect: bounded by `bounds`, unless its
    /// edge treatment lets the effect reach past them (`Decal`).
    SaveLayerEffect {
        bounds: Rect,
        effect: crate::graphics_layer::RenderEffect,
    },
    /// Transforms what follows (until the matching restore of an earlier save): scaled by
    /// `scale` and rotated by `rotation` degrees (clockwise) around `pivot`, then moved by
    /// `translation`, as a graphics layer.
    Transform {
        pivot: Offset,
        scale: (f32, f32),
        rotation: f32,
        translation: Offset,
    },
    /// The part `src` of a bitmap (in its pixels) drawn into `dst`, at `alpha`, through
    /// `color_filter` (of the list's `color_filters`) if any, as `drawImage(…, colorFilter)`.
    DrawImage {
        image: crate::image::ImageBitmap,
        src: Rect,
        dst: Rect,
        alpha: f32,
        color_filter: Option<ColorFilterIndex>,
    },
    /// The shadows of `outline` raised by `elevation` pixels, lit from `light` (x, y, z in the
    /// window's pixels, not moved by transforms) of `light_radius`: an ambient shadow in
    /// `ambient` and a spot shadow in `spot` (their alphas Compose's 3.9 % and 19 %), as
    /// Compose Desktop draws them with Skia's `ShadowUtils`; its occluder transparent (the
    /// shadow under it drawn too) for a layer faded below 1, as Skiko's render node's.
    DrawShadow {
        outline: crate::graphics::Outline,
        elevation: f32,
        light: (f32, f32, f32),
        light_radius: f32,
        ambient: crate::graphics::Color,
        spot: crate::graphics::Color,
        transparent_occluder: bool,
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
        path: std::sync::Arc<crate::path::PathData>,
        offset: Offset,
        scale: (f32, f32),
        paint: Paint,
    },
    /// Clips what follows (until the matching restore of an earlier save) to `path` moved by
    /// `offset` (its fill type deciding what's inside), as `clipPath`.
    ClipPath {
        path: std::sync::Arc<crate::path::PathData>,
        offset: Offset,
    },
    /// What follows, up to the matching [`EndEdgeEffects`](DrawOp::EndEdgeEffects), is drawn under
    /// the edge effects of the scroller `owner` (`compose_ui::EdgeEffect`) of `bounds`, where the
    /// platform has them: Android's replayer stretches or glows it; other renderers draw it as it
    /// is. Only while one is active, each frame a new `stamp`: the platform's effect moves as it's
    /// drawn, with the same bytes. `max_elevation` is how far across the axis a stretch keeps what's
    /// drawn past `bounds`; `glow_padding` (left, top, right, bottom) how far in a glow is.
    BeginEdgeEffects {
        owner: u32,
        bounds: Rect,
        stamp: u32,
        max_elevation: f32,
        glow_padding: [f32; 4],
    },
    EndEdgeEffects,
    /// Transforms what follows (until the matching restore of an earlier save) by a 3 × 3
    /// matrix, row-major as Android's and Skia's (scale x, skew x, translate x, skew y, scale
    /// y, translate y, the perspective row), as `Canvas.concat`: a bitmap's canvas's `skew` and
    /// `concat` (D1916). Boxed: rare, and the operation stays its size.
    Concat(Box<[f32; 9]>),
    /// Clips out `path` moved by `offset` from what follows, as `clipPath(path,
    /// ClipOp.Difference)` (a rectangle's too).
    ClipOutPath {
        path: std::sync::Arc<crate::path::PathData>,
        offset: Offset,
    },
    /// Triangles, each vertex's color blended with `paint`'s by `blend_mode`, as
    /// `drawVertices(vertices, blendMode, paint)`.
    DrawVertices {
        vertices: std::sync::Arc<crate::canvas::Vertices>,
        blend_mode: BlendMode,
        paint: Paint,
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
            | DrawOp::DrawPath { paint, .. }
            | DrawOp::DrawVertices { paint, .. } => Some(paint),
            _ => None,
        }
    }

    /// The color filter a drawing operation or a layer names, if it can name one.
    pub fn color_filter_mut(&mut self) -> Option<&mut Option<ColorFilterIndex>> {
        match self {
            DrawOp::DrawImage { color_filter, .. } | DrawOp::SaveLayer { color_filter, .. } => {
                Some(color_filter)
            }
            op => op.paint_mut().map(|paint| &mut paint.color_filter),
        }
    }
}

/// Whether two lists of operations draw the same: their floats compared bit for bit, so a NaN
/// (a 0/0 fraction's) equals itself where `==` says it changed, and a path's data by pointer
/// before its contents. What a `GraphicsLayer` compares a new recording with.
pub(crate) fn same_ops(old: &[DrawOp], new: &[DrawOp]) -> bool {
    old.len() == new.len() && old.iter().zip(new).all(|(old, new)| old.same(new))
}

/// Equality as `same_ops` needs it.
trait Same {
    fn same(&self, other: &Self) -> bool;
}

impl Same for f32 {
    fn same(&self, other: &f32) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Same for Offset {
    fn same(&self, other: &Offset) -> bool {
        self.x.same(&other.x) && self.y.same(&other.y)
    }
}

impl Same for Rect {
    fn same(&self, other: &Rect) -> bool {
        self.left.same(&other.left)
            && self.top.same(&other.top)
            && self.right.same(&other.right)
            && self.bottom.same(&other.bottom)
    }
}

impl<const N: usize> Same for [f32; N] {
    fn same(&self, other: &[f32; N]) -> bool {
        self.iter().zip(other).all(|(a, b)| a.same(b))
    }
}

impl Same for (f32, f32) {
    fn same(&self, other: &(f32, f32)) -> bool {
        self.0.same(&other.0) && self.1.same(&other.1)
    }
}

impl Same for Arc<crate::path::PathData> {
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(self, other) || **self == **other
    }
}

impl Same for Paint {
    fn same(&self, other: &Paint) -> bool {
        let style = match (self.style, other.style) {
            (crate::graphics::PaintStyle::Fill, crate::graphics::PaintStyle::Fill) => true,
            (crate::graphics::PaintStyle::Stroke(a), crate::graphics::PaintStyle::Stroke(b)) => {
                a.same(&b)
            }
            _ => false,
        };
        style
            && self.color == other.color
            && self.cap == other.cap
            && self.shader == other.shader
            && self.blend_mode == other.blend_mode
            && self.color_filter == other.color_filter
            && self.blur_radius().same(&other.blur_radius())
    }
}

impl Same for Outline {
    fn same(&self, other: &Outline) -> bool {
        match (self, other) {
            (Outline::Rectangle(a), Outline::Rectangle(b)) => a.same(b),
            (Outline::Rounded(a, ra), Outline::Rounded(b, rb)) => a.same(b) && ra.same(rb),
            (Outline::Generic(a, oa), Outline::Generic(b, ob)) => a.same(b) && oa.same(ob),
            _ => false,
        }
    }
}

impl Same for DrawOp {
    fn same(&self, other: &DrawOp) -> bool {
        use DrawOp as D;
        match (self, other) {
            (D::Save, D::Save) | (D::Restore, D::Restore) => true,
            (D::Translate(a), D::Translate(b)) => a.same(b),
            (D::ClipRect(a), D::ClipRect(b)) => a.same(b),
            (D::ClipRoundRect(a, ra), D::ClipRoundRect(b, rb)) => a.same(b) && ra.same(rb),
            (D::DrawRect(a, pa), D::DrawRect(b, pb)) => a.same(b) && pa.same(pb),
            (D::DrawRoundRect(a, ra, pa), D::DrawRoundRect(b, rb, pb)) => {
                a.same(b) && ra.same(rb) && pa.same(pb)
            }
            (D::DrawLine(a1, a2, pa), D::DrawLine(b1, b2, pb)) => {
                a1.same(b1) && a2.same(b2) && pa.same(pb)
            }
            (D::DrawParagraph(a, oa), D::DrawParagraph(b, ob)) => a == b && oa.same(ob),
            (
                D::SaveLayerAlpha { bounds: a, alpha: aa },
                D::SaveLayerAlpha { bounds: b, alpha: ab },
            ) => a.same(b) && aa.same(ab),
            (
                D::SaveLayer { bounds: a, alpha: aa, color_filter: fa, blend_mode: ma },
                D::SaveLayer { bounds: b, alpha: ab, color_filter: fb, blend_mode: mb },
            ) => a.same(b) && aa.same(ab) && fa == fb && ma == mb,
            (
                D::SaveLayerEffect { bounds: a, effect: ea },
                D::SaveLayerEffect { bounds: b, effect: eb },
            ) => {
                let crate::graphics_layer::RenderEffect::Blur {
                    radius_x: xa,
                    radius_y: ya,
                    edge_treatment: ta,
                } = ea;
                let crate::graphics_layer::RenderEffect::Blur {
                    radius_x: xb,
                    radius_y: yb,
                    edge_treatment: tb,
                } = eb;
                a.same(b) && xa.same(xb) && ya.same(yb) && ta == tb
            }
            (
                D::Transform { pivot: pa, scale: sa, rotation: ra, translation: ta },
                D::Transform { pivot: pb, scale: sb, rotation: rb, translation: tb },
            ) => pa.same(pb) && sa.same(sb) && ra.same(rb) && ta.same(tb),
            (
                D::DrawImage { image: ia, src: sa, dst: da, alpha: aa, color_filter: fa },
                D::DrawImage { image: ib, src: sb, dst: db, alpha: ab, color_filter: fb },
            ) => ia == ib && sa.same(sb) && da.same(db) && aa.same(ab) && fa == fb,
            (
                D::DrawShadow {
                    outline: oa,
                    elevation: ea,
                    light: (lxa, lya, lza),
                    light_radius: ra,
                    ambient: aa,
                    spot: sa,
                    transparent_occluder: ta,
                },
                D::DrawShadow {
                    outline: ob,
                    elevation: eb,
                    light: (lxb, lyb, lzb),
                    light_radius: rb,
                    ambient: ab,
                    spot: sb,
                    transparent_occluder: tb,
                },
            ) => {
                oa.same(ob)
                    && ea.same(eb)
                    && [*lxa, *lya, *lza].same(&[*lxb, *lyb, *lzb])
                    && ra.same(rb)
                    && aa == ab
                    && sa == sb
                    && ta == tb
            }
            (
                D::DrawArc { oval: a, start_angle: sa, sweep_angle: wa, use_center: ca, paint: pa },
                D::DrawArc { oval: b, start_angle: sb, sweep_angle: wb, use_center: cb, paint: pb },
            ) => a.same(b) && sa.same(sb) && wa.same(wb) && ca == cb && pa.same(pb),
            (
                D::DrawPath { path: a, offset: oa, scale: sa, paint: pa },
                D::DrawPath { path: b, offset: ob, scale: sb, paint: pb },
            ) => a.same(b) && oa.same(ob) && sa.same(sb) && pa.same(pb),
            (D::ClipPath { path: a, offset: oa }, D::ClipPath { path: b, offset: ob })
            | (D::ClipOutPath { path: a, offset: oa }, D::ClipOutPath { path: b, offset: ob }) => {
                a.same(b) && oa.same(ob)
            }
            (D::Concat(a), D::Concat(b)) => a.iter().zip(b.iter()).all(|(a, b)| a.same(b)),
            (
                D::DrawVertices { vertices: va, blend_mode: ma, paint: pa },
                D::DrawVertices { vertices: vb, blend_mode: mb, paint: pb },
            ) => Arc::ptr_eq(va, vb) && ma == mb && pa.same(pb),
            (
                D::BeginEdgeEffects {
                    owner: wa,
                    bounds: a,
                    stamp: sa,
                    max_elevation: ea,
                    glow_padding: pa,
                },
                D::BeginEdgeEffects {
                    owner: wb,
                    bounds: b,
                    stamp: sb,
                    max_elevation: eb,
                    glow_padding: pb,
                },
            ) => wa == wb && a.same(b) && sa == sb && ea.same(eb) && pa.same(pb),
            (D::EndEdgeEffects, D::EndEdgeEffects) => true,
            _ => false,
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
    /// The color filters its paints, images and layers refer to (`Paint::color_filter`), by
    /// index, as `shaders`.
    pub color_filters: Vec<ColorFilter>,
    /// The repaint boundaries drawn inside it, by reference, in drawing order: each before the
    /// operation at its `at`. Their groups are the frame's (`groups`), in the same order.
    pub boundaries: Vec<Boundary>,
    /// In a frame: the group of each repaint boundary drawn (`Modifier.repaint_boundary`,
    /// graphics layers, lazy items), in drawing order, a group before the ones nested in it
    /// (its `descendants`). The frame's boundaries, each followed by its group's content's
    /// (depth first), name them one by one.
    pub groups: Vec<DrawGroup>,
    /// In a frame: the groups of the repaint boundaries an earlier frame drew that are composed
    /// and not placed now (a lazy list's items kept beside the wanted ones, D375), each followed
    /// by the ones nested in it, as in `groups`. No boundary draws them: a renderer keeping a
    /// recording per group keeps theirs while they're listed, so the frame placing one again
    /// finds it recorded (Android, D380); one drawing the tree ignores them.
    pub kept: Vec<DrawGroup>,
    /// A translation before this index isn't merged into: a group's operations stay its own.
    pub(crate) merge_floor: usize,
    /// What `draw_text` drew: kept as long as this recording, which names their handles in `ops`
    /// (D1116).
    pub(crate) laid_out_paragraphs: Vec<crate::text::ParagraphKeeper>,
    /// Monotonic generation bumped whenever this list's own operations are cleared and redrawn.
    pub content_generation: u64,
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
    /// The color filters they refer to, by index, as `shaders`.
    pub color_filters: Vec<ColorFilter>,
}

/// A group in a flattened list: its content `ops[start..end]`, in its node's coordinates, keyed
/// by its node, with its version ([`DrawGroup`]). Its layer's operations are just outside it:
/// `ops[layer_start..start]` before it, which are, in this order and each only if needed,
/// `Save` (if it transforms or clips), `Transform` (if it scales, rotates or moves), `ClipRect`
/// (if it clips to its bounds) and `SaveLayerAlpha` (if its alpha is below 1); and
/// `ops[end..layer_end]` after it, a `Restore` for each save. Without a layer,
/// `layer_start == start` and `layer_end == end`.
#[derive(Clone, PartialEq, Debug, Default)]
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
        static NEXT_CONTENT_GENERATION: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(1);
        self.ops.clear();
        self.groups.clear();
        self.kept.clear();
        self.merge_floor = 0;
        self.boundaries.clear();
        self.shaders.clear();
        self.color_filters.clear();
        self.laid_out_paragraphs.clear();
        self.content_generation =
            NEXT_CONTENT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn push(&mut self, op: DrawOp) {
        self.ops.push(op);
    }

    /// Keeps `shader` for this list's paints, returning the index they refer to it by.
    pub fn push_shader(&mut self, shader: crate::brush::Shader) -> crate::brush::ShaderIndex {
        self.shaders.push(shader);
        crate::brush::ShaderIndex::new(self.shaders.len() as u32 - 1)
    }

    /// Keeps `color_filter` for this list's paints, images and layers, returning the index they
    /// refer to it by: the last one's if it's the same (a filter drawn again and again is kept
    /// once).
    pub fn push_color_filter(&mut self, color_filter: ColorFilter) -> ColorFilterIndex {
        if self.color_filters.last() != Some(&color_filter) {
            self.color_filters.push(color_filter);
        }
        ColorFilterIndex::new(self.color_filters.len() as u32 - 1)
    }

    /// The color filter `index` names in this list, if any.
    pub fn color_filter(&self, index: Option<ColorFilterIndex>) -> Option<&ColorFilter> {
        self.color_filters.get(index?.index())
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
            Outline::Generic(path, offset) => {
                self.push(DrawOp::DrawPath { path, offset, scale: (1.0, 1.0), paint })
            }
        }
    }

    pub fn clip_outline(&mut self, outline: Outline) {
        match outline {
            Outline::Rectangle(rect) => self.push(DrawOp::ClipRect(rect)),
            Outline::Rounded(rect, radii) => self.push(DrawOp::ClipRoundRect(rect, radii)),
            Outline::Generic(path, offset) => self.push(DrawOp::ClipPath { path, offset }),
        }
    }
}

impl FlatDisplayList {
    /// The color filter `index` names in this list, if any.
    pub fn color_filter(&self, index: Option<ColorFilterIndex>) -> Option<&ColorFilter> {
        self.color_filters.get(index?.index())
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
            let paint_text = |paint: &Paint| paint_text(paint, &self.shaders, &self.color_filters);
            let filter_text = |index: &Option<ColorFilterIndex>| {
                index
                    .and_then(|index| self.color_filters.get(index.index()))
                    .map_or(String::new(), filter_text)
            };
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
                DrawOp::SaveLayerAlpha { alpha, .. } => writeln!(out, "save layer alpha {alpha}"),
                DrawOp::SaveLayerEffect { effect, .. } => writeln!(out, "save layer {effect:?}"),
                DrawOp::SaveLayer { alpha, color_filter, blend_mode, .. } => writeln!(
                    out,
                    "save layer alpha {alpha}{}{}",
                    filter_text(color_filter),
                    blend_text(*blend_mode)
                ),
                DrawOp::DrawImage { image, src, dst, alpha, color_filter } => writeln!(
                    out,
                    "image {}x{} {} into {} alpha {alpha}{}",
                    image.width(),
                    image.height(),
                    rect_text(src),
                    rect_text(dst),
                    filter_text(color_filter),
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
                        crate::graphics::Outline::Rectangle(rect) => (*rect, [0.0; 4]),
                        crate::graphics::Outline::Rounded(rect, radii) => (*rect, *radii),
                        crate::graphics::Outline::Generic(..) => (outline.bounds(), [-1.0; 4]),
                    };
                    writeln!(
                        out,
                        "shadow {} radii {:?} elevation {elevation} {ambient:?} {spot:?}",
                        rect_text(&rect),
                        radii
                    )
                }
                DrawOp::Transform { pivot, scale, rotation, translation } => writeln!(
                    out,
                    "transform pivot {},{} scale {}x{} rotate {} translate {},{}",
                    pivot.x, pivot.y, scale.0, scale.1, rotation, translation.x, translation.y
                ),
                DrawOp::BeginEdgeEffects { owner, bounds, stamp, .. } => {
                    writeln!(out, "edge effects {owner} {} stamp {stamp}", rect_text(bounds))
                }
                DrawOp::EndEdgeEffects => writeln!(out, "end edge effects"),
                DrawOp::ClipPath { path, offset } => writeln!(
                    out,
                    "clip path of {} commands at {},{}",
                    path.commands().len(),
                    offset.x,
                    offset.y
                ),
                DrawOp::Concat(matrix) => writeln!(out, "concat {matrix:?}"),
                DrawOp::ClipOutPath { path, offset } => writeln!(
                    out,
                    "clip out path of {} commands at {},{}",
                    path.commands().len(),
                    offset.x,
                    offset.y
                ),
                DrawOp::DrawVertices { vertices, blend_mode, paint } => writeln!(
                    out,
                    "vertices {:?} of {} {blend_mode:?} {paint:?}",
                    vertices.vertex_mode(),
                    vertices.positions().len()
                ),
            };
            if matches!(
                op,
                DrawOp::Save
                    | DrawOp::SaveLayerAlpha { .. }
                    | DrawOp::SaveLayer { .. }
                    | DrawOp::SaveLayerEffect { .. }
            ) {
                depth += 1;
            }
        }
        out
    }
}

/// Whether `ops` (what an alpha layer holds, from just after its `SaveLayerAlpha`, or a group's
/// content) are one drawing with a paint among saves, clips and moves: returns the index of the
/// layer's closing `Restore` if so, or `ops.len()` if they end first, balanced. An alpha over
/// them can fade that paint instead of compositing them offscreen, the same pixels, when the
/// layer's bounds hold the drawing ([`single_drawing_extent`],
/// `Optimization::SINGLE_DRAWING_FADE`).
pub fn single_drawing(ops: &[DrawOp]) -> Option<usize> {
    let mut depth = 0usize;
    let mut drawings = 0;
    for (index, op) in ops.iter().enumerate() {
        match op {
            DrawOp::SaveLayerAlpha { .. }
            | DrawOp::SaveLayer { .. }
            | DrawOp::SaveLayerEffect { .. }
            | DrawOp::BeginEdgeEffects { .. } => return None,
            DrawOp::EndEdgeEffects => {}
            DrawOp::Save => depth += 1,
            DrawOp::Restore if depth == 0 => return (drawings == 1).then_some(index),
            DrawOp::Restore => depth -= 1,
            DrawOp::Translate(_)
            | DrawOp::ClipRect(_)
            | DrawOp::ClipRoundRect(..)
            | DrawOp::ClipPath { .. }
            | DrawOp::ClipOutPath { .. }
            | DrawOp::Concat(_)
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
            // Text, images, shadows and vertices' colors draw what a paint's alpha doesn't reach.
            DrawOp::DrawParagraph(..)
            | DrawOp::DrawImage { .. }
            | DrawOp::DrawShadow { .. }
            | DrawOp::DrawVertices { .. } => {
                return None;
            }
        }
    }
    (drawings == 1 && depth == 0).then_some(ops.len())
}

/// The extent of the one drawing `ops` hold ([`single_drawing`]), a fill, in their
/// coordinates: its geometry (sorted, as Skia and Android draw a rect given right before left;
/// a curve's control points), the moves before it applied. `None` when they aren't one
/// drawing, or it can't be bounded and drawn alike faded or offscreen: under a transform, or a
/// stroke. A stroke a device pixel wide or less is drawn as a hairline, its joints blended
/// twice where the offscreen buffer blends once (15 to 20 of 255 apart), and wider ones aren't
/// always the same either (a 2 px stroked rounded rect's corners, 5 of 255 on desktop): no
/// stroke is folded. An alpha layer can fade that fill's paint instead of compositing it
/// offscreen when the extent lies within the layer's bounds ([`fits_in`]): the offscreen
/// buffer, its edges rounded outward in device space, would cut nothing of it
/// (`Optimization::SINGLE_DRAWING_FADE`, D648).
pub fn single_drawing_extent(ops: &[DrawOp]) -> Option<Rect> {
    let end = single_drawing(ops)?;
    let mut offsets: smallvec::SmallVec<[Offset; 4]> = smallvec::smallvec![Offset::ZERO];
    let mut extent = None;
    for op in &ops[..end] {
        let offset = *offsets.last().expect("balanced");
        match op {
            DrawOp::Save => offsets.push(offset),
            DrawOp::Restore => {
                offsets.pop();
            }
            DrawOp::Translate(by) => *offsets.last_mut().expect("balanced") = offset + *by,
            DrawOp::Transform { .. } | DrawOp::Concat(_) => return None,
            DrawOp::ClipRect(_)
            | DrawOp::ClipRoundRect(..)
            | DrawOp::ClipPath { .. }
            | DrawOp::ClipOutPath { .. } => {}
            drawing => {
                let own = fill_extent(drawing)?;
                extent = Some(Rect::new(
                    own.left + offset.x,
                    own.top + offset.y,
                    own.right + offset.x,
                    own.bottom + offset.y,
                ));
            }
        }
    }
    extent
}

/// Whether `extent` lies within `bounds`, both sorted (an inverted one holds nothing).
pub fn fits_in(extent: Rect, bounds: Rect) -> bool {
    let sorted = |rect: Rect| rect.left <= rect.right && rect.top <= rect.bottom;
    sorted(extent)
        && sorted(bounds)
        && extent.left >= bounds.left
        && extent.top >= bounds.top
        && extent.right <= bounds.right
        && extent.bottom <= bounds.bottom
}

/// What `op`, a drawing with a fill paint, can touch ([`single_drawing_extent`]); `None` for a
/// stroke (a line is always one).
fn fill_extent(op: &DrawOp) -> Option<Rect> {
    // A filter or a blend mode doesn't commute with a layer's alpha.
    let fill = |paint: &Paint| {
        matches!(paint.style, crate::graphics::PaintStyle::Fill) && paint.is_plain()
    };
    let sorted = |rect: Rect| {
        Rect::new(
            rect.left.min(rect.right),
            rect.top.min(rect.bottom),
            rect.left.max(rect.right),
            rect.top.max(rect.bottom),
        )
    };
    match op {
        DrawOp::DrawRect(rect, paint) | DrawOp::DrawRoundRect(rect, _, paint) if fill(paint) => {
            Some(sorted(*rect))
        }
        DrawOp::DrawArc { oval, paint, .. } if fill(paint) => Some(sorted(*oval)),
        DrawOp::DrawPath { path, offset, scale, paint } if fill(paint) => {
            let bounds = path.bounds();
            let (x1, x2) = (bounds.left * scale.0, bounds.right * scale.0);
            let (y1, y2) = (bounds.top * scale.1, bounds.bottom * scale.1);
            Some(Rect::new(
                x1.min(x2) + offset.x,
                y1.min(y2) + offset.y,
                x1.max(x2) + offset.x,
                y1.max(y2) + offset.y,
            ))
        }
        _ => None,
    }
}

/// The group first in `groups` flattened, with those nested in it that follow it.
pub(crate) fn flatten_groups(groups: &[DrawGroup]) -> FlatDisplayList {
    let mut flat = FlatDisplayList::default();
    if let Some(first) = groups.first() {
        let boundary = Boundary { at: 0, key: first.key };
        flatten_group(&boundary, groups, &mut 0, &mut flat);
    }
    flat
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
    let filter_base =
        if recording.color_filters.is_empty() { 0 } else { flat.color_filters.len() as u32 };
    flat.color_filters.extend_from_slice(&recording.color_filters);
    let bases = (base, filter_base);
    let mut from = 0;
    for boundary in &recording.boundaries {
        let at = boundary.at.clamp(from, recording.ops.len());
        extend_shifted(&mut flat.ops, &recording.ops[from..at], bases);
        flatten_group(boundary, groups, next, flat);
        from = at;
    }
    extend_shifted(&mut flat.ops, &recording.ops[from..], bases);
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
    if let Some(layer) = &group.layer {
        layer.operations(|op| flat.ops.push(op));
    }
    let start = flat.ops.len();
    flatten_into(&group.content, groups, next, flat);
    let end = flat.ops.len();
    if end == start && !group.layer.as_ref().is_some_and(|layer| layer.casts_shadow()) {
        // Nothing to apply them to.
        flat.ops.truncate(layer_start);
        return;
    }
    let restores = group.layer.as_ref().map_or(0, |layer| layer.restores());
    flat.ops.extend((0..restores).map(|_| DrawOp::Restore));
    flat.groups.push(FlatGroup {
        key: group.key,
        version: group.version,
        start,
        end,
        layer_start,
        layer_end: flat.ops.len(),
        layer: group.layer.clone(),
    });
}

/// Appends `ops`, from a list whose shaders and color filters are the target's from `bases` on.
pub(crate) fn extend_shifted(
    target: &mut Vec<DrawOp>,
    ops: &[DrawOp],
    (base, filter_base): (u32, u32),
) {
    let start = target.len();
    target.extend_from_slice(ops);
    if base > 0 {
        for op in &mut target[start..] {
            if let Some(index) = op.paint_mut().and_then(|paint| paint.shader.as_mut()) {
                *index = index.offset(base);
            }
        }
    }
    if filter_base > 0 {
        for op in &mut target[start..] {
            if let Some(Some(index)) = op.color_filter_mut() {
                *index = index.offset(filter_base);
            }
        }
    }
}

fn rect_text(rect: &Rect) -> String {
    format!("{},{} {}x{}", rect.left, rect.top, rect.width(), rect.height())
}

fn paint_text(paint: &Paint, shaders: &[crate::brush::Shader], filters: &[ColorFilter]) -> String {
    let color = match paint.shader.and_then(|index| shaders.get(index.index())) {
        Some(shader) => shader_text(shader),
        None => format!("{:?}", paint.color),
    };
    let mut text = match paint.style {
        crate::graphics::PaintStyle::Fill => color,
        crate::graphics::PaintStyle::Stroke(width) => format!("{color} stroke {width}"),
    };
    if let Some(filter) = paint.color_filter.and_then(|index| filters.get(index.index())) {
        text.push_str(&filter_text(filter));
    }
    text.push_str(&blend_text(paint.blend_mode));
    if paint.blur_radius > 0.0 {
        text.push_str(&format!(" blur {}", paint.blur_radius));
    }
    text
}

/// A color filter in a dump, after a space: a `SrcIn` tint as `tint`.
fn filter_text(filter: &ColorFilter) -> String {
    match filter {
        ColorFilter::BlendMode { color, blend_mode: BlendMode::SrcIn } => {
            format!(" tint {color:?}")
        }
        ColorFilter::BlendMode { color, blend_mode } => format!(" tint {color:?} {blend_mode:?}"),
        ColorFilter::Lighting { multiply, add } => format!(" lighting {multiply:?} {add:?}"),
        ColorFilter::ColorMatrix(matrix) => format!(" color matrix {:?}", matrix.values),
    }
}

/// A blend mode in a dump, after a space: nothing for `SrcOver`.
fn blend_text(blend_mode: BlendMode) -> String {
    if blend_mode == BlendMode::SrcOver { String::new() } else { format!(" blend {blend_mode:?}") }
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
