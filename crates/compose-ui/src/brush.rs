//! Brushes, as Compose's `Brush`: a solid color, a gradient, or a shader brush (a shader of its
//! own, two brushes composited, or one making its shader for each size), resolved for a
//! drawing's size into a [`PaintShader`] the display list keeps (`DisplayList::shaders`) and
//! paints refer to. [`Shader`] is Compose's `Shader`: gradients, a bitmap's pixels, two shaders
//! composited (D2400).

use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use std::sync::Arc;

use compose_runtime::ComposeArg;

use crate::color_filter::BlendMode;
use crate::graphics::Color;
use crate::image::ImageBitmap;
use crate::matrix::Matrix;
use crate::unit::{Offset, Size};

/// How a gradient continues past its ends, as `TileMode`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum TileMode {
    /// The end colors continue.
    #[default]
    Clamp,
    /// The gradient repeats.
    Repeated,
    /// The gradient repeats, mirrored each time.
    Mirror,
    /// Transparent past the ends.
    Decal,
}

/// What to paint with, as Compose's `Brush`: a solid color, a linear, radial or sweep gradient,
/// or another shader brush. A gradient's positions are in the drawing's coordinates (the draw
/// scope's), and default to its size: `Offset::INFINITE` coordinates are its right or bottom, an
/// `Offset::UNSPECIFIED` center its center, an infinite radius half its smaller side.
#[derive(Clone, PartialEq, Debug)]
pub enum Brush {
    /// As `SolidColor`.
    Solid(Color),
    /// As `LinearGradient`.
    Linear { gradient: Gradient, start: Offset, end: Offset },
    /// As `RadialGradient`.
    Radial { gradient: Gradient, center: Offset, radius: f32 },
    /// As `SweepGradient`.
    Sweep { gradient: Gradient, center: Offset },
    /// Any other `ShaderBrush`: a shader of its own (`ShaderBrush(shader)`), two brushes
    /// composited (`Brush.composite`), or one of the app's making its shader for each size.
    Shader(ShaderBrush),
}

/// A gradient's colors, their stops (evenly spread without), and its tile mode.
#[derive(Clone, PartialEq, Debug)]
pub struct Gradient {
    pub colors: Arc<[Color]>,
    /// Where each color is, from 0 to 1: as many as the colors, increasing.
    pub stops: Option<Arc<[f32]>>,
    pub tile_mode: TileMode,
}

/// A gradient's colors, evenly spread or each at its stop (from 0 to 1): what the gradient
/// constructors take, from `&[Color]` or `&[(f32, Color)]` (arrays and vectors too), as
/// Compose's `colors` and `colorStops` overloads.
#[derive(Clone, PartialEq, Debug)]
pub struct ColorStops {
    colors: Arc<[Color]>,
    stops: Option<Arc<[f32]>>,
}

impl From<&[Color]> for ColorStops {
    fn from(colors: &[Color]) -> ColorStops {
        ColorStops { colors: colors.into(), stops: None }
    }
}

impl<const N: usize> From<&[Color; N]> for ColorStops {
    fn from(colors: &[Color; N]) -> ColorStops {
        ColorStops::from(&colors[..])
    }
}

impl<const N: usize> From<[Color; N]> for ColorStops {
    fn from(colors: [Color; N]) -> ColorStops {
        ColorStops::from(&colors[..])
    }
}

impl From<Vec<Color>> for ColorStops {
    fn from(colors: Vec<Color>) -> ColorStops {
        ColorStops { colors: colors.into(), stops: None }
    }
}

impl From<&[(f32, Color)]> for ColorStops {
    fn from(color_stops: &[(f32, Color)]) -> ColorStops {
        ColorStops {
            colors: color_stops.iter().map(|(_, color)| *color).collect(),
            stops: Some(color_stops.iter().map(|(stop, _)| *stop).collect()),
        }
    }
}

impl From<Vec<(f32, Color)>> for ColorStops {
    fn from(color_stops: Vec<(f32, Color)>) -> ColorStops {
        ColorStops::from(&color_stops[..])
    }
}

impl<const N: usize> From<&[(f32, Color); N]> for ColorStops {
    fn from(color_stops: &[(f32, Color); N]) -> ColorStops {
        ColorStops::from(&color_stops[..])
    }
}

impl<const N: usize> From<[(f32, Color); N]> for ColorStops {
    fn from(color_stops: [(f32, Color); N]) -> ColorStops {
        ColorStops::from(&color_stops[..])
    }
}

impl Gradient {
    fn new(colors: ColorStops, tile_mode: TileMode) -> Gradient {
        Gradient { colors: colors.colors, stops: colors.stops, tile_mode }
    }

    /// The colors' average (each channel, alpha weighted), what a renderer without shaders
    /// paints instead.
    fn average(&self) -> Color {
        let count = self.colors.len().max(1) as f32;
        let (mut alpha, mut red, mut green, mut blue) = (0.0, 0.0, 0.0, 0.0);
        for color in self.colors.iter() {
            let argb = color.to_argb();
            let byte = |shift: u32| f32::from((argb >> shift) as u8);
            let weight = byte(24);
            alpha += weight;
            red += byte(16) * weight;
            green += byte(8) * weight;
            blue += byte(0) * weight;
        }
        if alpha == 0.0 {
            return Color::TRANSPARENT;
        }
        let channel = |value: f32| (value / alpha).round().clamp(0.0, 255.0) as u8;
        Color::from_rgb(channel(red), channel(green), channel(blue))
            .with_alpha(alpha / count / 255.0)
    }

    /// Between this gradient and `other`'s colors and stops, as Compose's `lerpColorList` and
    /// `lerpNullableFloatList`: the longer list's length, the shorter's last entry repeated;
    /// stops only if both have them. The tile mode switches halfway.
    fn lerp(&self, other: &Gradient, t: f32) -> Gradient {
        fn lerp_list<T: Copy>(a: &[T], b: &[T], lerp: impl Fn(T, T) -> T) -> Vec<T> {
            (0..a.len().max(b.len()))
                .map(|index| {
                    let left = a[index.min(a.len().saturating_sub(1))];
                    let right = b[index.min(b.len().saturating_sub(1))];
                    lerp(left, right)
                })
                .collect()
        }
        let colors = if self.colors.is_empty() || other.colors.is_empty() {
            Vec::new()
        } else {
            lerp_list(&self.colors, &other.colors, |a, b| Color::lerp(a, b, t))
        };
        let stops = match (&self.stops, &other.stops) {
            (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => {
                Some(lerp_list(a, b, |a, b| crate::math::lerp(a, b, t)).into())
            }
            _ => None,
        };
        Gradient {
            colors: colors.into(),
            stops,
            tile_mode: if t < 0.5 { self.tile_mode } else { other.tile_mode },
        }
    }

    /// This gradient's stops and tile mode, every color `color`: what a solid color is as a
    /// gradient of this shape, to interpolate to it without a jump.
    fn solid(&self, color: Color) -> Gradient {
        Gradient { colors: self.colors.iter().map(|_| color).collect(), ..self.clone() }
    }
}

impl Brush {
    /// Colors evenly spread (or at their stops) along the drawing's diagonal, as
    /// `Brush.linearGradient(colors)`.
    pub fn linear_gradient(colors: impl Into<ColorStops>) -> Brush {
        Brush::linear_gradient_with(colors, Offset::ZERO, Offset::INFINITE, TileMode::Clamp)
    }

    /// Colors left to right across the drawing, as `Brush.horizontalGradient(colors)`.
    pub fn horizontal_gradient(colors: impl Into<ColorStops>) -> Brush {
        Brush::horizontal_gradient_with(colors, 0.0, f32::INFINITY, TileMode::Clamp)
    }

    /// Colors top to bottom across the drawing, as `Brush.verticalGradient(colors)`.
    pub fn vertical_gradient(colors: impl Into<ColorStops>) -> Brush {
        Brush::vertical_gradient_with(colors, 0.0, f32::INFINITY, TileMode::Clamp)
    }

    /// Colors from the drawing's center out to half its smaller side, as
    /// `Brush.radialGradient(colors)`.
    pub fn radial_gradient(colors: impl Into<ColorStops>) -> Brush {
        Brush::radial_gradient_with(colors, Offset::UNSPECIFIED, f32::INFINITY, TileMode::Clamp)
    }

    /// Colors around the drawing's center, as `Brush.sweepGradient(colors)`.
    pub fn sweep_gradient(colors: impl Into<ColorStops>) -> Brush {
        Brush::sweep_gradient_with(colors, Offset::UNSPECIFIED)
    }

    /// This brush at `alpha` (0 to 1) of its opacity: each color's alpha multiplied (a shader
    /// brush's paint's alpha), as the `alpha` Compose's `background(brush, shape, alpha)` and
    /// draw calls take.
    pub fn with_alpha(self, alpha: f32) -> Brush {
        let alpha = alpha.clamp(0.0, 1.0);
        let fade = |color: Color| color.with_alpha(color.alpha() * alpha);
        let faded = |gradient: Gradient| Gradient {
            colors: gradient.colors.iter().map(|color| fade(*color)).collect(),
            ..gradient
        };
        match self {
            Brush::Solid(color) => Brush::Solid(fade(color)),
            Brush::Linear { gradient, start, end } => {
                Brush::Linear { gradient: faded(gradient), start, end }
            }
            Brush::Radial { gradient, center, radius } => {
                Brush::Radial { gradient: faded(gradient), center, radius }
            }
            Brush::Sweep { gradient, center } => Brush::Sweep { gradient: faded(gradient), center },
            Brush::Shader(brush) => {
                Brush::Shader(ShaderBrush { alpha: brush.alpha * alpha, ..brush })
            }
        }
    }

    /// A solid color, as `SolidColor(color)`.
    pub fn solid(color: Color) -> Brush {
        Brush::Solid(color)
    }

    /// Colors from `start` to `end`, as `Brush.linearGradient(colors, start, end, tileMode)`
    /// (or `colorStops`: each color at its stop) (Compose's defaults: `Offset::ZERO` to `Offset::INFINITE`, the
    /// drawing's diagonal, `TileMode::Clamp`).
    pub fn linear_gradient_with(
        colors: impl Into<ColorStops>,
        start: Offset,
        end: Offset,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::Linear { gradient: Gradient::new(colors.into(), tile_mode), start, end }
    }

    /// A left-to-right gradient from `start_x` to `end_x`, as `Brush.horizontalGradient`
    /// (Compose's defaults: 0 to `f32::INFINITY`, the drawing's width).
    pub fn horizontal_gradient_with(
        colors: impl Into<ColorStops>,
        start_x: f32,
        end_x: f32,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::linear_gradient_with(
            colors,
            Offset::new(start_x, 0.0),
            Offset::new(end_x, 0.0),
            tile_mode,
        )
    }

    /// A top-to-bottom gradient from `start_y` to `end_y`, as `Brush.verticalGradient`.
    pub fn vertical_gradient_with(
        colors: impl Into<ColorStops>,
        start_y: f32,
        end_y: f32,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::linear_gradient_with(
            colors,
            Offset::new(0.0, start_y),
            Offset::new(0.0, end_y),
            tile_mode,
        )
    }

    /// Colors evenly spread from `center` out to `radius`, as `Brush.radialGradient`
    /// (Compose's defaults: `Offset::UNSPECIFIED`, the drawing's center, and `f32::INFINITY`,
    /// half its smaller side).
    pub fn radial_gradient_with(
        colors: impl Into<ColorStops>,
        center: Offset,
        radius: f32,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::Radial { gradient: Gradient::new(colors.into(), tile_mode), center, radius }
    }

    /// Colors around `center`, clockwise from 3 o'clock, as `Brush.sweepGradient`.
    pub fn sweep_gradient_with(colors: impl Into<ColorStops>, center: Offset) -> Brush {
        Brush::Sweep { gradient: Gradient::new(colors.into(), TileMode::Clamp), center }
    }

    /// `src` composited over `dst` by `blend_mode`, as `Brush.composite(dstBrush, srcBrush,
    /// blendMode)`: each made a shader for the drawing's size (a solid color a vertical gradient
    /// of that color), blended as `CompositeShader`.
    pub fn composite(dst: Brush, src: Brush, blend_mode: BlendMode) -> Brush {
        let to_shader_brush = |brush: Brush| match brush {
            Brush::Solid(color) => Brush::vertical_gradient([color, color]),
            other => other,
        };
        Brush::Shader(ShaderBrush::of(ShaderBrushKind::Composite {
            dst: to_shader_brush(dst),
            src: to_shader_brush(src),
            blend_mode,
        }))
    }

    /// The size it has of its own, as `Brush.intrinsicSize`: a linear gradient's span (NaN on
    /// an axis it doesn't fix), a radial gradient's diameter; `Size::UNSPECIFIED` for a color,
    /// a sweep, a radial gradient of the drawing's size, and a shader brush that says none.
    pub fn intrinsic_size(&self) -> Size {
        match self {
            Brush::Solid(_) | Brush::Sweep { .. } => Size::UNSPECIFIED,
            Brush::Linear { start, end, .. } => {
                let span = |a: f32, b: f32| {
                    if a.is_finite() && b.is_finite() { (a - b).abs() } else { f32::NAN }
                };
                Size::new(span(start.x, end.x), span(start.y, end.y))
            }
            Brush::Radial { radius, .. } if radius.is_finite() => {
                Size::new(radius * 2.0, radius * 2.0)
            }
            Brush::Radial { .. } => Size::UNSPECIFIED,
            Brush::Shader(brush) => brush.intrinsic_size(),
        }
    }

    /// The shader a gradient makes for a drawing of `size`, in the drawing's coordinates, as
    /// its `createShader(size)`: `None` for a color or a shader brush.
    fn gradient_shader(&self, size: Size) -> Option<Shader> {
        let point = |offset: Offset| {
            let x = if offset.x == f32::INFINITY { size.width } else { offset.x };
            let y = if offset.y == f32::INFINITY { size.height } else { offset.y };
            Offset::new(x, y)
        };
        let center = |center: Offset| {
            if center.is_specified() { point(center) } else { size.center() }
        };
        match self {
            Brush::Solid(_) | Brush::Shader(_) => None,
            Brush::Linear { gradient, start, end } => Some(Shader::LinearGradient {
                gradient: gradient.clone(),
                from: point(*start),
                to: point(*end),
            }),
            Brush::Radial { gradient, center: at, radius } => Some(Shader::RadialGradient {
                gradient: gradient.clone(),
                center: center(*at),
                radius: if *radius == f32::INFINITY { size.min_dimension() / 2.0 } else { *radius },
            }),
            Brush::Sweep { gradient, center: at } => {
                Some(Shader::SweepGradient { gradient: gradient.clone(), center: center(*at) })
            }
        }
    }

    /// Whether this brush computes its shader dynamically (`CustomShaderBrush`).
    #[doc(hidden)]
    pub fn is_dynamic_shader(&self) -> bool {
        matches!(self, Brush::Shader(brush) if brush.is_dynamic())
    }

    /// Resolves any dynamic [`CustomShaderBrush`] at `size` into a fixed shader brush, leaving
    /// non-dynamic brushes as they are; over an empty or partial-NaN size, skips `create_shader`
    /// as `ShaderBrushSpan` / `ShaderBrush.applyTo` (D2430, D2433).
    #[doc(hidden)]
    pub fn resolved_at(&self, size: Size) -> Brush {
        match self {
            Brush::Shader(brush) if brush.is_dynamic() => match brush.resolved_at(size) {
                Some(resolved) => Brush::Shader(resolved),
                None => Brush::Solid(Color::BLACK.with_alpha(brush.alpha)),
            },
            _ => self.clone(),
        }
    }

    /// The shader this brush makes for a drawing of `size`, in the drawing's coordinates, as a
    /// `ShaderBrush`'s `createShader(size)` (a shader brush's own, made anew); a solid color, as
    /// `Brush.composite` takes one, is a vertical gradient of that color.
    pub fn create_shader(&self, size: Size) -> Shader {
        match self {
            Brush::Solid(color) => Brush::vertical_gradient([*color, *color])
                .gradient_shader(size)
                .expect("a gradient makes a shader"),
            Brush::Shader(brush) => brush.create_shader(size),
            gradient => gradient.gradient_shader(size).expect("a gradient makes a shader"),
        }
    }

    /// This brush over a drawing of `size`, moved by `origin`, its coordinates fixed: what paints
    /// a part of that drawing whose top left is `-origin` as the whole drawing would (a gradient
    /// spanning every paragraph of a text, D2052).
    pub(crate) fn placed(&self, size: Size, origin: Offset) -> Brush {
        let point = |offset: Offset| {
            let x = if offset.x == f32::INFINITY { size.width } else { offset.x };
            let y = if offset.y == f32::INFINITY { size.height } else { offset.y };
            Offset::new(x + origin.x, y + origin.y)
        };
        let center = |center: Offset| {
            if center.is_specified() {
                point(center)
            } else {
                Offset::new(origin.x + size.width / 2.0, origin.y + size.height / 2.0)
            }
        };
        match self {
            Brush::Solid(color) => Brush::Solid(*color),
            Brush::Linear { gradient, start, end } => {
                Brush::Linear { gradient: gradient.clone(), start: point(*start), end: point(*end) }
            }
            Brush::Radial { gradient, center: at, radius } => Brush::Radial {
                gradient: gradient.clone(),
                center: center(*at),
                radius: if *radius == f32::INFINITY { size.min_dimension() / 2.0 } else { *radius },
            },
            Brush::Sweep { gradient, center: at } => {
                Brush::Sweep { gradient: gradient.clone(), center: center(*at) }
            }
            Brush::Shader(brush) => {
                let mut unscaled = brush.clone();
                unscaled.alpha = 1.0;
                Brush::Shader(ShaderBrush {
                    alpha: brush.alpha,
                    ..ShaderBrush::of(ShaderBrushKind::Placed { brush: unscaled, size, origin })
                })
            }
        }
    }

    /// The shader painting this brush over a drawing of `size` whose top left is at `origin`
    /// (in the display list's coordinates), at `alpha`: `None` for a solid color, and for a
    /// shader brush over an empty size (as `ShaderBrush.applyTo`, D2401).
    pub fn shader(&self, size: Size, origin: Offset, alpha: f32) -> Option<PaintShader> {
        let alpha = alpha.clamp(0.0, 1.0);
        match self {
            Brush::Solid(_) => None,
            Brush::Shader(brush) => brush.paint_shader(size, origin, alpha),
            _ if !has_non_empty_area(size) => None,
            gradient => Some(PaintShader {
                shader: gradient.gradient_shader(size)?.moved(origin),
                transform: None,
                alpha,
            }),
        }
    }

    /// The color a paint has with this brush: its color, a gradient's average (what a renderer
    /// without shaders paints), or black for another shader brush (as `ShaderBrush.applyTo`
    /// sets), at `alpha`.
    pub fn color(&self, alpha: f32) -> Color {
        let color = match self {
            Brush::Solid(color) => *color,
            Brush::Linear { gradient, .. }
            | Brush::Radial { gradient, .. }
            | Brush::Sweep { gradient, .. } => gradient.average(),
            Brush::Shader(brush) => Color::BLACK.with_alpha(brush.alpha),
        };
        color.with_alpha(color.alpha() * alpha.clamp(0.0, 1.0))
    }

    /// Between two brushes, as Compose's `Interpolatable.lerp(start, stop, t)` with brushes: a
    /// missing one is transparent; colors, and gradients of one kind, interpolate (a color as a
    /// gradient of the other's shape); others, and shader brushes, switch halfway.
    pub fn lerp(start: Option<&Brush>, stop: Option<&Brush>, t: f32) -> Option<Brush> {
        if start == stop {
            return if t < 0.5 { start.cloned() } else { stop.cloned() };
        }
        let lerped = match start {
            Some(start) => start.lerp_with(stop, t),
            None => None,
        };
        let lerped = lerped.or_else(|| stop.and_then(|stop| stop.lerp_with(start, 1.0 - t)));
        lerped.or_else(|| if t < 0.5 { start.cloned() } else { stop.cloned() })
    }

    /// This brush's own `lerp(other, t)`, as Compose's `SolidColor`, `LinearGradient`,
    /// `RadialGradient` and `SweepGradient` implement it: `None` where it can't.
    fn lerp_with(&self, other: Option<&Brush>, t: f32) -> Option<Brush> {
        let transparent = Brush::Solid(Color::TRANSPARENT);
        let other = other.unwrap_or(&transparent);
        let solid = match other {
            Brush::Solid(color) => Some(*color),
            _ => None,
        };
        let lerp_offset = |a: Offset, b: Offset| crate::math::lerp(a, b, t);
        match (self, other) {
            (Brush::Solid(a), Brush::Solid(b)) => Some(Brush::Solid(Color::lerp(*a, *b, t))),
            (Brush::Linear { gradient, start, end }, other) => {
                let (other_gradient, other_start, other_end) = match (other, solid) {
                    (_, Some(color)) => (gradient.solid(color), *start, *end),
                    (Brush::Linear { gradient, start, end }, None) => {
                        (gradient.clone(), *start, *end)
                    }
                    _ => return None,
                };
                // Compose's `lerpSafe`: an infinite end switches halfway.
                let safe = |a: Offset, b: Offset| {
                    if a.is_finite() && b.is_finite() {
                        lerp_offset(a, b)
                    } else if t < 0.5 {
                        a
                    } else {
                        b
                    }
                };
                Some(Brush::Linear {
                    gradient: gradient.lerp(&other_gradient, t),
                    start: safe(*start, other_start),
                    end: safe(*end, other_end),
                })
            }
            (Brush::Radial { gradient, center, radius }, other) => {
                let (other_gradient, other_center, other_radius) = match (other, solid) {
                    (_, Some(color)) => (gradient.solid(color), *center, *radius),
                    (Brush::Radial { gradient, center, radius }, None) => {
                        (gradient.clone(), *center, *radius)
                    }
                    _ => return None,
                };
                Some(Brush::Radial {
                    gradient: gradient.lerp(&other_gradient, t),
                    center: lerp_offset(*center, other_center),
                    radius: crate::math::lerp(*radius, other_radius, t),
                })
            }
            (Brush::Sweep { gradient, center }, other) => {
                let (other_gradient, other_center) = match (other, solid) {
                    (_, Some(color)) => (gradient.solid(color), *center),
                    (Brush::Sweep { gradient, center }, None) => (gradient.clone(), *center),
                    _ => return None,
                };
                Some(Brush::Sweep {
                    gradient: gradient.lerp(&other_gradient, t),
                    center: lerp_offset(*center, other_center),
                })
            }
            _ => None,
        }
    }
}

impl crate::interpolatable::Interpolatable for Brush {
    fn lerp(&self, other: Option<&dyn Any>, t: f32) -> Option<Box<dyn Any>> {
        let other = match other {
            None => None,
            Some(other) => Some(other.downcast_ref::<Brush>()?),
        };
        self.lerp_with(other, t).map(|brush| Box::new(brush) as Box<dyn Any>)
    }

    fn equals(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Brush>() == Some(self)
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Brush {
        Brush::Solid(color)
    }
}

impl From<ShaderBrush> for Brush {
    fn from(brush: ShaderBrush) -> Brush {
        Brush::Shader(brush)
    }
}

/// A brush painting with `shader` whatever the size, as `ShaderBrush(shader)`.
impl From<Shader> for Brush {
    fn from(shader: Shader) -> Brush {
        Brush::Shader(ShaderBrush::new(shader))
    }
}

impl ComposeArg for Brush {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A shader brush of the app's: what a subclass of Compose's `ShaderBrush` implements, its
/// shader for each size the brush is drawn at (`createShader`), and its intrinsic size. Wrap it
/// with [`ShaderBrush::custom`]. A brush drawn again at the same size draws the shader it made
/// then (as Compose's `ShaderBrush.applyTo`): a shader that reads state is made again when the
/// size changes, or the app makes a new brush.
pub trait CustomShaderBrush: Any {
    /// The shader for a drawing of `size`, in its coordinates, as `createShader(size)`.
    fn create_shader(&self, size: Size) -> Shader;

    /// Its size of its own, as `intrinsicSize`: `Size::UNSPECIFIED` (the default) for none.
    fn intrinsic_size(&self) -> Size {
        Size::UNSPECIFIED
    }

    /// Whether `other` paints the same, as its `equals`: `false` by default, a brush being
    /// equal to itself (and its clones) only, as a Kotlin object.
    fn same(&self, _other: &dyn CustomShaderBrush) -> bool {
        false
    }
}

impl dyn CustomShaderBrush {
    /// `self` as `T`, for [`same`](CustomShaderBrush::same).
    pub fn downcast_ref<T: CustomShaderBrush>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}

/// A brush painting with a shader, as Compose's `ShaderBrush` (its gradients are [`Brush`]'s own
/// variants): a shader whatever the size (`ShaderBrush(shader)`), two brushes composited
/// (`Brush.composite`), or an app's [`CustomShaderBrush`]. Shared as a Kotlin object is: a clone
/// is the same brush, with the same transform and the shader it last made.
#[derive(Clone)]
pub struct ShaderBrush {
    inner: Rc<ShaderBrushInner>,
    /// Its paint's alpha, below 1 for a brush faded by [`Brush::with_alpha`].
    alpha: f32,
}

struct ShaderBrushInner {
    kind: ShaderBrushKind,
    state: RefCell<ShaderBrushState>,
}

enum ShaderBrushKind {
    Fixed(Shader),
    Composite {
        dst: Brush,
        src: Brush,
        blend_mode: BlendMode,
    },
    Custom(Box<dyn CustomShaderBrush>),
    /// A brush over a drawing of `size`, moved by `origin` (`Brush::placed`).
    Placed {
        brush: ShaderBrush,
        size: Size,
        origin: Offset,
    },
}

#[derive(Default)]
struct ShaderBrushState {
    transform: Option<Matrix>,
    /// The shader last made, and the size it was made for, as `ShaderBrush`'s `createdSize`.
    created: Option<(Size, Shader)>,
}

/// Both sides strictly positive (false when zero, negative, unspecified, or partially NaN).
#[inline]
fn has_non_empty_area(size: Size) -> bool {
    size.width > 0.0 && size.height > 0.0
}

impl ShaderBrush {
    fn of(kind: ShaderBrushKind) -> ShaderBrush {
        let inner = ShaderBrushInner { kind, state: RefCell::new(ShaderBrushState::default()) };
        ShaderBrush { inner: Rc::new(inner), alpha: 1.0 }
    }

    /// A brush painting with `shader` whatever the size, as `ShaderBrush(shader)`.
    pub fn new(shader: Shader) -> ShaderBrush {
        ShaderBrush::of(ShaderBrushKind::Fixed(shader))
    }

    /// A brush making its shader with `brush`, as a subclass of `ShaderBrush`.
    pub fn custom(brush: impl CustomShaderBrush) -> ShaderBrush {
        ShaderBrush::of(ShaderBrushKind::Custom(Box::new(brush)))
    }

    /// The app's brush, if it is a `T`.
    pub fn downcast_ref<T: CustomShaderBrush>(&self) -> Option<&T> {
        match &self.inner.kind {
            ShaderBrushKind::Custom(brush) => brush.downcast_ref::<T>(),
            _ => None,
        }
    }

    fn state(&self) -> Ref<'_, ShaderBrushState> {
        self.inner.state.borrow()
    }

    fn state_mut(&self) -> RefMut<'_, ShaderBrushState> {
        self.inner.state.borrow_mut()
    }

    /// Its shader's transform, as `transform`.
    pub fn transform(&self) -> Option<Matrix> {
        self.state().transform
    }

    /// Sets its shader's transform (from the shader's coordinates to the drawing's), as
    /// `transform = matrix`: every clone of it draws with it.
    pub fn set_transform(&self, transform: Option<Matrix>) {
        self.state_mut().transform = transform;
    }

    /// Its size of its own, as `intrinsicSize`.
    pub fn intrinsic_size(&self) -> Size {
        match &self.inner.kind {
            ShaderBrushKind::Custom(brush) => brush.intrinsic_size(),
            ShaderBrushKind::Placed { brush, .. } => brush.intrinsic_size(),
            ShaderBrushKind::Fixed(_) | ShaderBrushKind::Composite { .. } => Size::UNSPECIFIED,
        }
    }

    /// Whether this brush computes its shader dynamically (`CustomShaderBrush` or a composite
    /// containing one).
    #[doc(hidden)]
    pub fn is_dynamic(&self) -> bool {
        match &self.inner.kind {
            ShaderBrushKind::Fixed(_) => false,
            ShaderBrushKind::Custom(_) => true,
            ShaderBrushKind::Composite { dst, src, .. } => {
                dst.is_dynamic_shader() || src.is_dynamic_shader()
            }
            ShaderBrushKind::Placed { brush, .. } => brush.is_dynamic(),
        }
    }

    /// Resolves a dynamic [`ShaderBrush`] at `size` into a fixed [`ShaderBrush`], bypassing its
    /// size cache so state reads happen inside `create_shader`; `None` when `size` is empty or
    /// partially NaN.
    #[doc(hidden)]
    pub fn resolved_at(&self, size: Size) -> Option<ShaderBrush> {
        if let ShaderBrushKind::Placed { brush, size, origin } = &self.inner.kind {
            let resolved = brush.resolved_at(*size)?;
            return Some(ShaderBrush {
                alpha: self.alpha,
                ..ShaderBrush::of(ShaderBrushKind::Placed {
                    brush: resolved,
                    size: *size,
                    origin: *origin,
                })
            });
        }
        if !has_non_empty_area(size) {
            return None;
        }
        if !self.is_dynamic() {
            return Some(self.clone());
        }
        let mut resolved = ShaderBrush::new(self.create_shader(size));
        resolved.alpha = self.alpha;
        resolved.set_transform(self.transform());
        Some(resolved)
    }

    /// Its shader for a drawing of `size`, made anew, as `createShader(size)`.
    pub fn create_shader(&self, size: Size) -> Shader {
        match &self.inner.kind {
            ShaderBrushKind::Fixed(shader) => shader.clone(),
            ShaderBrushKind::Composite { dst, src, blend_mode } => {
                Shader::composite(dst.create_shader(size), src.create_shader(size), *blend_mode)
            }
            ShaderBrushKind::Custom(brush) => brush.create_shader(size),
            ShaderBrushKind::Placed { brush, size, .. } => brush.create_shader(*size),
        }
    }

    /// Its shader for a drawing of `size` whose top left is at `origin` in the display list, at
    /// `alpha`: the one made for the last drawing of that size, else made now (as
    /// `ShaderBrush.applyTo`); none for an empty size.
    fn paint_shader(&self, size: Size, origin: Offset, alpha: f32) -> Option<PaintShader> {
        let alpha = alpha * self.alpha;
        if let ShaderBrushKind::Placed { brush, size, origin: by } = &self.inner.kind {
            return brush.paint_shader(*size, origin + *by, alpha);
        }
        if !has_non_empty_area(size) {
            self.state_mut().created = None;
            return None;
        }
        let (shader, transform) = match &self.inner.kind {
            ShaderBrushKind::Fixed(shader) => (shader.clone(), self.transform()),
            _ => {
                let state = self.state();
                let transform = state.transform;
                let shader = match &state.created {
                    Some((created, shader)) if *created == size => shader.clone(),
                    _ => {
                        // Made unborrowed: the app's code may draw or resolve with this brush too.
                        drop(state);
                        let shader = self.create_shader(size);
                        self.state_mut().created = Some((size, shader.clone()));
                        shader
                    }
                };
                (shader, transform)
            }
        };
        Some(PaintShader::placed(shader, transform, origin, alpha))
    }
}

impl PartialEq for ShaderBrush {
    fn eq(&self, other: &ShaderBrush) -> bool {
        if self.alpha != other.alpha {
            return false;
        }
        if Rc::ptr_eq(&self.inner, &other.inner) {
            return true;
        }
        match (&self.inner.kind, &other.inner.kind) {
            (ShaderBrushKind::Fixed(a), ShaderBrushKind::Fixed(b)) => {
                a == b && self.transform() == other.transform()
            }
            (
                ShaderBrushKind::Composite { dst: da, src: sa, blend_mode: ma },
                ShaderBrushKind::Composite { dst: db, src: sb, blend_mode: mb },
            ) => da == db && sa == sb && ma == mb && self.transform() == other.transform(),
            (ShaderBrushKind::Custom(a), ShaderBrushKind::Custom(b)) => {
                a.same(&**b) && self.transform() == other.transform()
            }
            (
                ShaderBrushKind::Placed { brush: a, size: sa, origin: oa },
                ShaderBrushKind::Placed { brush: b, size: sb, origin: ob },
            ) => a == b && sa == sb && oa == ob,
            _ => false,
        }
    }
}

impl std::fmt::Debug for ShaderBrush {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner.kind {
            ShaderBrushKind::Fixed(shader) => write!(formatter, "ShaderBrush({shader:?})")?,
            ShaderBrushKind::Composite { dst, src, blend_mode } => write!(
                formatter,
                "CompositeShaderBrush(dstBrush={dst:?}, srcBrush={src:?}, blendMode={blend_mode:?})"
            )?,
            ShaderBrushKind::Custom(_) => write!(formatter, "ShaderBrush(custom)")?,
            ShaderBrushKind::Placed { brush, size, origin } => {
                write!(formatter, "{brush:?} over {size:?} at {origin:?}")?
            }
        }
        if self.alpha < 1.0 {
            write!(formatter, " alpha {}", self.alpha)?;
        }
        Ok(())
    }
}

/// What paints a shader brush's drawings, as Compose's `Shader` (an Android or Skia shader):
/// a gradient, a bitmap's pixels, or two shaders composited. Made by [`Shader::linear_gradient`]
/// and its siblings, as Compose's `LinearGradientShader`, `RadialGradientShader`,
/// `SweepGradientShader`, `ImageShader` and `CompositeShader`. Its positions are in the
/// drawing's coordinates.
#[derive(Clone, PartialEq, Debug)]
pub enum Shader {
    LinearGradient {
        gradient: Gradient,
        from: Offset,
        to: Offset,
    },
    RadialGradient {
        gradient: Gradient,
        center: Offset,
        radius: f32,
    },
    SweepGradient {
        gradient: Gradient,
        center: Offset,
    },
    /// A bitmap's pixels from the drawing's origin, continued past its edges as `tile_mode_x`
    /// and `tile_mode_y` say.
    Image {
        image: ImageBitmap,
        tile_mode_x: TileMode,
        tile_mode_y: TileMode,
    },
    Composite(Arc<CompositeShader>),
}

/// Two shaders blended, as `CompositeShader(dst, src, blendMode)`.
#[derive(Clone, PartialEq, Debug)]
pub struct CompositeShader {
    pub dst: Shader,
    pub src: Shader,
    pub blend_mode: BlendMode,
}

impl Shader {
    /// Colors from `from` to `to`, as `LinearGradientShader(from, to, colors, colorStops,
    /// tileMode)`.
    pub fn linear_gradient(
        from: Offset,
        to: Offset,
        colors: impl Into<ColorStops>,
        tile_mode: TileMode,
    ) -> Shader {
        Shader::LinearGradient { gradient: Gradient::new(colors.into(), tile_mode), from, to }
    }

    /// Colors from `center` out to `radius`, as `RadialGradientShader(center, radius, colors,
    /// colorStops, tileMode)`.
    pub fn radial_gradient(
        center: Offset,
        radius: f32,
        colors: impl Into<ColorStops>,
        tile_mode: TileMode,
    ) -> Shader {
        Shader::RadialGradient { gradient: Gradient::new(colors.into(), tile_mode), center, radius }
    }

    /// Colors around `center`, clockwise from 3 o'clock, as `SweepGradientShader(center,
    /// colors, colorStops)`.
    pub fn sweep_gradient(center: Offset, colors: impl Into<ColorStops>) -> Shader {
        Shader::SweepGradient { gradient: Gradient::new(colors.into(), TileMode::Clamp), center }
    }

    /// `image`'s pixels, from the drawing's origin, as `ImageShader(image, tileModeX,
    /// tileModeY)` (Compose's defaults: `TileMode::Clamp` both).
    pub fn image(image: &ImageBitmap, tile_mode_x: TileMode, tile_mode_y: TileMode) -> Shader {
        Shader::Image { image: image.clone(), tile_mode_x, tile_mode_y }
    }

    /// `src` blended over `dst` by `blend_mode`, as `CompositeShader(dst, src, blendMode)`.
    pub fn composite(dst: Shader, src: Shader, blend_mode: BlendMode) -> Shader {
        Shader::Composite(Arc::new(CompositeShader { dst, src, blend_mode }))
    }

    /// A gradient's colors, stops and tile mode; `None` for another shader.
    pub fn gradient(&self) -> Option<&Gradient> {
        match self {
            Shader::LinearGradient { gradient, .. }
            | Shader::RadialGradient { gradient, .. }
            | Shader::SweepGradient { gradient, .. } => Some(gradient),
            Shader::Image { .. } | Shader::Composite(_) => None,
        }
    }

    /// A gradient moved by `by`; another shader as it is.
    fn moved(self, by: Offset) -> Shader {
        match self {
            Shader::LinearGradient { gradient, from, to } => {
                Shader::LinearGradient { gradient, from: from + by, to: to + by }
            }
            Shader::RadialGradient { gradient, center, radius } => {
                Shader::RadialGradient { gradient, center: center + by, radius }
            }
            Shader::SweepGradient { gradient, center } => {
                Shader::SweepGradient { gradient, center: center + by }
            }
            shader => shader,
        }
    }

    /// Whether it paints a bitmap's pixels.
    pub(crate) fn has_image(&self) -> bool {
        match self {
            Shader::Image { .. } => true,
            Shader::Composite(composite) => composite.dst.has_image() || composite.src.has_image(),
            _ => false,
        }
    }

    /// The same shader, its bitmaps drawn into read now (a canvas's drawing names no bitmap
    /// that waits for drawings itself).
    pub(crate) fn snapshot(&self) -> Shader {
        match self {
            Shader::Image { image, tile_mode_x, tile_mode_y } => Shader::Image {
                image: image.snapshot(),
                tile_mode_x: *tile_mode_x,
                tile_mode_y: *tile_mode_y,
            },
            Shader::Composite(composite) => Shader::composite(
                composite.dst.snapshot(),
                composite.src.snapshot(),
                composite.blend_mode,
            ),
            gradient => gradient.clone(),
        }
    }
}

/// A brush resolved for one drawing: its shader in the display list's coordinates, with the
/// paint's alpha. A gradient's positions are moved into the list's coordinates; any other
/// shader (and a transformed one) keeps its own, `transform` taking them to the list's.
#[derive(Clone, PartialEq, Debug)]
pub struct PaintShader {
    pub shader: Shader,
    /// From the shader's coordinates to the list's, as a platform shader's local matrix:
    /// `None` for the identity.
    pub transform: Option<Box<Matrix>>,
    pub alpha: f32,
}

impl PaintShader {
    /// `shader` (in a drawing's coordinates, transformed by `transform` there) for a drawing
    /// whose top left is `origin` in the list, at `alpha`.
    fn placed(shader: Shader, transform: Option<Matrix>, origin: Offset, alpha: f32) -> Self {
        let transform = transform.filter(|transform| !transform.is_identity());
        let transform = match (transform, &shader) {
            // A gradient's positions take the origin: the same bytes as a gradient brush's.
            (None, Shader::LinearGradient { .. })
            | (None, Shader::RadialGradient { .. })
            | (None, Shader::SweepGradient { .. }) => {
                return PaintShader { shader: shader.moved(origin), transform: None, alpha };
            }
            (None, _) if origin == Offset::ZERO => None,
            (None, _) => Some(Matrix::translation(origin.x, origin.y)),
            (Some(mut transform), _) => {
                transform.post_concat(&Matrix::translation(origin.x, origin.y));
                Some(transform)
            }
        };
        PaintShader { shader, transform: transform.map(Box::new), alpha }
    }

    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Scales the shader's opacity by `factor` (`ModulateAlpha` layers).
    pub(crate) fn scale_alpha(&mut self, factor: f32) {
        self.alpha *= factor.clamp(0.0, 1.0);
    }

    /// Its gradient, if its shader is one.
    pub fn gradient(&self) -> Option<&Gradient> {
        self.shader.gradient()
    }

    /// The same, its bitmaps drawn into read now ([`Shader::snapshot`]).
    pub(crate) fn snapshot(&self) -> PaintShader {
        PaintShader { shader: self.shader.snapshot(), ..self.clone() }
    }
}

/// A shader of a display list, by its index in `DisplayList::shaders`.
/// Kept as the index plus one (`NonZeroU32`), so an `Option` of it is 4 bytes and a `Paint`
/// naming a shader and a filter stays 32 (28 with a 32-bit color).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ShaderIndex(std::num::NonZeroU32);

impl ShaderIndex {
    /// The one naming entry `index`.
    pub fn new(index: u32) -> ShaderIndex {
        ShaderIndex(std::num::NonZeroU32::new(index + 1).expect("an index below u32::MAX"))
    }

    /// The entry it names.
    pub fn index(self) -> usize {
        self.0.get() as usize - 1
    }

    /// The one naming the entry `by` later: a list's entries copied after another's.
    pub fn offset(self, by: u32) -> ShaderIndex {
        ShaderIndex(self.0.checked_add(by).expect("an index below u32::MAX"))
    }
}
