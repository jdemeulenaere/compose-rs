//! Drop and inner shadows, as Compose's `androidx.compose.ui.graphics.shadow`: a shape's shadow
//! blurred by a radius, spread, moved, in a color or a brush, drawn by `Modifier.drop_shadow`
//! and `Modifier.inner_shadow` or by their painters.

use std::cell::Cell;

use crate::brush::Brush;
use crate::color_filter::{BlendMode, ColorFilter};
use crate::draw::DrawScope;
use crate::graphics::{Color, Outline, Paint, PaintStyle, Shape};
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode, ModifierNodeElement};
use crate::painter::CustomPainter;
use crate::path::{CornerRadius, Path, PathFillType, RoundRect};
use crate::unit::{Dp, DpOffset, Offset, Rect, Size};

/// A shadow's look, as Compose's `shadow.Shadow`: its blur `radius`, how far it `spread`s past
/// (or, inside, into) its shape, its `offset`, its color or brush (a solid brush is its color),
/// `alpha` and `blend_mode`. Compose's defaults: black, no spread or offset, opaque, `SrcOver`.
#[derive(Clone, PartialEq, Debug)]
pub struct Shadow {
    pub radius: Dp,
    pub spread: Dp,
    pub offset: DpOffset,
    pub color: Color,
    pub brush: Option<Brush>,
    pub alpha: f32,
    pub blend_mode: BlendMode,
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow::new(Dp(0.0))
    }
}

impl Shadow {
    /// A black shadow of `radius`, as `Shadow(radius)`.
    pub fn new(radius: Dp) -> Shadow {
        Shadow {
            radius,
            spread: Dp(0.0),
            offset: DpOffset::default(),
            color: Color::BLACK,
            brush: None,
            alpha: 1.0,
            blend_mode: BlendMode::SrcOver,
        }
    }

    /// A shadow of `radius` in `color` (an unspecified color is black), as `Shadow(radius,
    /// color)`.
    pub fn colored(radius: Dp, color: Color) -> Shadow {
        Shadow { color: color.or(Color::BLACK), ..Shadow::new(radius) }
    }

    /// A shadow of `radius` painted by `brush` (a solid brush is its color), as `Shadow(radius,
    /// brush)`.
    pub fn brushed(radius: Dp, brush: Brush) -> Shadow {
        match brush {
            Brush::Solid(color) => Shadow::colored(radius, color),
            brush => Shadow { brush: Some(brush), ..Shadow::new(radius) },
        }
    }

    /// The same shadow at `alpha` (kept in 0 to 1), as the `alpha` parameter.
    pub fn with_alpha(self, alpha: f32) -> Shadow {
        Shadow { alpha: alpha.clamp(0.0, 1.0), ..self }
    }

    /// How opaque it's drawn at the painter's `alpha`: times its own, and its color's.
    fn opacity(&self, alpha: f32) -> f32 {
        let alpha = (alpha * self.alpha).clamp(0.0, 1.0);
        match self.brush {
            Some(_) => alpha,
            None => alpha * self.color.alpha(),
        }
    }

    /// Its fill, blurred by its radius, its brush resolved over `extent` (in `scope`'s
    /// coordinates; `None`: its bounds): at `alpha` (the painter's times its own), with its
    /// blend mode and `filter`; or `opaque`, at full alpha and plain, for a layer composited
    /// with them ([`draw_as_one`]).
    fn paint(
        &self,
        scope: &mut DrawScope<'_>,
        alpha: f32,
        filter: Option<&ColorFilter>,
        extent: Option<Rect>,
        opaque: bool,
    ) -> Paint {
        let alpha = if opaque { 1.0 } else { (alpha * self.alpha).clamp(0.0, 1.0) };
        let mut paint = match &self.brush {
            Some(brush) => {
                let mut paint = Paint::fill(brush.color(alpha));
                let (size, origin) = match extent {
                    Some(extent) => (
                        Size::new(extent.width(), extent.height()),
                        scope.origin + Offset::new(extent.left, extent.top),
                    ),
                    None => (scope.size(), scope.origin),
                };
                if let Some(shader) = brush.shader(size, origin, alpha) {
                    paint.shader = Some(scope.list.push_shader(shader));
                }
                paint
            }
            None if opaque => Paint::fill(self.color.with_alpha(1.0)),
            None => Paint::fill(self.color).with_alpha(alpha),
        };
        if !opaque {
            paint = paint.with_blend_mode(self.blend_mode);
            paint = scope.paint_with_color_filter(paint, filter);
        }
        paint.with_blur(scope.density().to_px(self.radius))
    }
}

/// Draws with `draw` (given the shadow's fill) drawings of `shadow` that overlap, a path's fill
/// and its spread's band, as one mask, as Compose's bitmap is: in a layer bounded by `bounds`
/// composited at its opacity, blend mode and `filter`, the drawings in it opaque, so its alpha
/// doesn't compound where they overlap. Without one when they're opaque and plain already.
fn draw_as_one(
    scope: &mut DrawScope<'_>,
    shadow: &Shadow,
    alpha: f32,
    filter: Option<&ColorFilter>,
    (bounds, extent): (Rect, Option<Rect>),
    draw: impl FnOnce(&mut DrawScope<'_>, Paint),
) {
    let opacity = shadow.opacity(alpha);
    let plain = opacity >= 1.0
        && shadow.brush.is_none()
        && shadow.blend_mode == BlendMode::SrcOver
        && filter.is_none();
    if plain {
        let paint = shadow.paint(scope, alpha, filter, extent, false);
        draw(scope, paint);
        return;
    }
    scope.save_layer(bounds, opacity, filter, shadow.blend_mode);
    let paint = shadow.paint(scope, alpha, None, extent, true);
    draw(scope, paint);
    scope.restore();
}

impl compose_runtime::ComposeArg for Shadow {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Between `start` and `stop`, as Compose's `shadow.lerp`: a missing end is the other one,
/// transparent and without a brush; brushes interpolate as `Interpolatable.lerp` does
/// ([`Brush::lerp`](crate::Brush::lerp), D2402); the blend mode switches halfway.
pub fn lerp(start: Option<&Shadow>, stop: Option<&Shadow>, fraction: f32) -> Option<Shadow> {
    let transparent =
        |shadow: &Shadow| Shadow { color: Color::TRANSPARENT, brush: None, ..shadow.clone() };
    let (start, stop) = match (start, stop) {
        (None, None) => return None,
        (None, Some(stop)) => (transparent(stop), stop.clone()),
        (Some(start), None) => (start.clone(), transparent(start)),
        (Some(start), Some(stop)) => (start.clone(), stop.clone()),
    };
    let dp = |a: Dp, b: Dp| Dp(crate::unit::lerp_f32(a.0, b.0, fraction));
    let half = fraction < 0.5;
    Some(Shadow {
        radius: dp(start.radius, stop.radius),
        spread: dp(start.spread, stop.spread),
        offset: DpOffset::lerp(start.offset, stop.offset, fraction),
        color: Color::lerp(start.color, stop.color, fraction),
        brush: crate::brush::Brush::lerp(start.brush.as_ref(), stop.brush.as_ref(), fraction),
        alpha: crate::unit::lerp_f32(start.alpha, stop.alpha, fraction),
        blend_mode: if half { start.blend_mode } else { stop.blend_mode },
    })
}

/// `rect` grown by `by` on each side (shrunk for a negative one).
fn inflate(rect: Rect, by: f32) -> Rect {
    Rect::new(rect.left - by, rect.top - by, rect.right + by, rect.bottom + by)
}

/// Draws `shape`'s drop shadow at `size`, as `DropShadowPainter`: the shape grown by the spread,
/// moved by the offset, filled blurred by the radius.
fn draw_drop_shadow(
    scope: &mut DrawScope<'_>,
    shape: &Shape,
    shadow: &Shadow,
    alpha: f32,
    filter: Option<&ColorFilter>,
) {
    let size = scope.size();
    let outline = shape.outline_in(size, &scope.density(), scope.layout_direction());
    let density = scope.density();
    let (radius, spread) = (density.to_px(shadow.radius), density.to_px(shadow.spread));
    let offset = Offset::new(density.to_px(shadow.offset.x), density.to_px(shadow.offset.y));
    // Compose's shadow bitmap, which its brush is resolved for: the size grown by the radius
    // and the spread each side, drawn from its top left moved back by them.
    let reach = radius + spread;
    let extent = Rect::new(
        0.0 - reach,
        0.0 - reach,
        (size.width + reach * 2.0).ceil() - reach,
        (size.height + reach * 2.0).ceil() - reach,
    );
    scope.translate(offset.x, offset.y, |scope| match &outline {
        Outline::Rectangle(rect) => {
            let paint = shadow.paint(scope, alpha, filter, Some(extent), false);
            scope.draw_rect(inflate(*rect, spread), paint);
        }
        Outline::Rounded(rect, radii) => {
            let paint = shadow.paint(scope, alpha, filter, Some(extent), false);
            let radii = radii.map(|radius| radius + spread);
            scope.draw_round_rect(inflate(*rect, spread), radii, paint);
        }
        Outline::Generic(path, at) if spread > 0.0 => {
            // The fill and the spread's band, as one mask: their blurs overlap.
            let bounds = inflate(outline.bounds(), spread + radius * 3.0 + 1.0);
            draw_as_one(scope, shadow, alpha, filter, (bounds, Some(extent)), |scope, paint| {
                scope.draw_path_scaled(path, *at, (1.0, 1.0), paint);
                let mut stroke = paint;
                stroke.style = PaintStyle::Stroke(spread * 2.0);
                scope.draw_path_scaled(path, *at, (1.0, 1.0), stroke);
            });
        }
        Outline::Generic(path, at) => {
            let paint = shadow.paint(scope, alpha, filter, Some(extent), false);
            scope.draw_path_scaled(path, *at, (1.0, 1.0), paint);
        }
    });
}

/// Draws `shape`'s inner shadow at `size`, as `InnerShadowPainter`: clipped to the shape, what's
/// outside the shape (shrunk by the spread, moved by the offset) filled blurred by the radius,
/// so its blur reaches in from the edges.
fn draw_inner_shadow(
    scope: &mut DrawScope<'_>,
    shape: &Shape,
    shadow: &Shadow,
    alpha: f32,
    filter: Option<&ColorFilter>,
) {
    let size = scope.size();
    let density = scope.density();
    let outline = shape.outline_in(size, &density, scope.layout_direction());
    let (radius, spread) = (density.to_px(shadow.radius), density.to_px(shadow.spread));
    let offset = Offset::new(density.to_px(shadow.offset.x), density.to_px(shadow.offset.y));
    // Around the shape, far enough that the blur doesn't reach its outer edge.
    let reach = radius * 3.0 + spread + offset.x.abs().max(offset.y.abs()) + 1.0;
    let around = Path::new();
    around.add_rect(inflate(outline.bounds(), reach));
    let mut band = None;
    match &outline {
        Outline::Rectangle(rect) => around.add_rect(inflate(*rect, -spread)),
        Outline::Rounded(rect, radii) => {
            let mut round_rect = RoundRect::new(inflate(*rect, -spread), CornerRadius::default());
            let corners = radii.map(|radius| {
                let radius = (radius - spread).max(0.0);
                CornerRadius { x: radius, y: radius }
            });
            [
                round_rect.top_left_corner_radius,
                round_rect.top_right_corner_radius,
                round_rect.bottom_right_corner_radius,
                round_rect.bottom_left_corner_radius,
            ] = corners;
            around.add_round_rect(round_rect);
        }
        Outline::Generic(path, at) => {
            around.add_path(&Path::from_shared_data(std::sync::Arc::clone(path)), *at);
            // The spread as a band along the edge, as Compose strokes it.
            band = (spread > 0.0).then(|| (std::sync::Arc::clone(path), *at));
        }
    }
    around.set_fill_type(PathFillType::EvenOdd);
    let bounds = outline.bounds();
    scope.save();
    scope.clip_outline(outline);
    match band {
        // What's outside the shape and the spread's band, as one mask: their blurs overlap.
        Some((path, at)) => {
            draw_as_one(scope, shadow, alpha, filter, (bounds, None), |scope, paint| {
                scope.draw_path_scaled(&around, offset, (1.0, 1.0), paint);
                let mut stroke = paint;
                stroke.style = PaintStyle::Stroke(spread * 2.0);
                scope.draw_path_scaled(&path, at + offset, (1.0, 1.0), stroke);
            })
        }
        None => {
            let paint = shadow.paint(scope, alpha, filter, None, false);
            scope.draw_path_scaled(&around, offset, (1.0, 1.0), paint);
        }
    }
    scope.restore();
}

/// A painter of a shape's drop shadow, as Compose's `DropShadowPainter`: no intrinsic size,
/// faded and filtered itself.
pub struct DropShadowPainter {
    shape: Shape,
    shadow: Shadow,
    alpha: Cell<f32>,
    color_filter: Cell<Option<ColorFilter>>,
}

/// A painter of a shape's inner shadow, as Compose's `InnerShadowPainter`.
pub struct InnerShadowPainter {
    shape: Shape,
    shadow: Shadow,
    alpha: Cell<f32>,
    color_filter: Cell<Option<ColorFilter>>,
}

macro_rules! shadow_painter {
    ($painter:ident, $draw:ident) => {
        impl $painter {
            pub fn new(shape: Shape, shadow: Shadow) -> $painter {
                $painter { shape, shadow, alpha: Cell::new(1.0), color_filter: Cell::new(None) }
            }
        }

        impl CustomPainter for $painter {
            fn intrinsic_size(&self) -> Size {
                Size::UNSPECIFIED
            }
            fn on_draw(&self, scope: &mut DrawScope<'_>) {
                let filter = self.color_filter.get();
                $draw(scope, &self.shape, &self.shadow, self.alpha.get(), filter.as_ref());
            }
            fn apply_alpha(&self, alpha: f32) -> bool {
                self.alpha.set(alpha);
                true
            }
            fn apply_color_filter(&self, color_filter: Option<&ColorFilter>) -> bool {
                self.color_filter.set(color_filter.copied());
                true
            }
            fn apply_layout_direction(&self, _: crate::alignment::LayoutDirection) -> bool {
                true
            }
        }
    };
}

shadow_painter!(DropShadowPainter, draw_drop_shadow);
shadow_painter!(InnerShadowPainter, draw_inner_shadow);

impl Modifier {
    /// Draws `shape`'s drop shadow behind the content, as `Modifier.dropShadow(shape, shadow)`.
    pub fn drop_shadow(self, shape: Shape, shadow: Shadow) -> Modifier {
        self.then_custom(ShadowElement { shape, shadow, inner: false })
    }

    /// Draws `shape`'s inner shadow (before the content, as Compose's), as
    /// `Modifier.innerShadow(shape, shadow)`.
    pub fn inner_shadow(self, shape: Shape, shadow: Shadow) -> Modifier {
        self.then_custom(ShadowElement { shape, shadow, inner: true })
    }
}

#[derive(Clone)]
struct ShadowElement {
    shape: Shape,
    shadow: Shadow,
    inner: bool,
}

impl ModifierNodeElement for ShadowElement {
    type Node = ShadowElement;
    fn create(&self) -> ShadowElement {
        self.clone()
    }
    fn update(&self, node: &mut ShadowElement) {
        *node = self.clone();
    }
    fn same(&self, other: &Self) -> bool {
        self.shape == other.shape && self.shadow == other.shadow && self.inner == other.inner
    }
    fn name(&self) -> &'static str {
        if self.inner { "innerShadow" } else { "dropShadow" }
    }
}

impl ModifierNode for ShadowElement {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for ShadowElement {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if self.inner {
            draw_inner_shadow(scope, &self.shape, &self.shadow, 1.0, None);
        } else {
            draw_drop_shadow(scope, &self.shape, &self.shadow, 1.0, None);
        }
    }
}
