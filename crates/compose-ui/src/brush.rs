//! Brushes, as Compose's `Brush`: a solid color or a gradient, resolved for a drawing's size
//! into a `Shader` the display list keeps (`DisplayList::shaders`) and paints refer to.

use std::sync::Arc;

use compose_runtime::ComposeArg;

use crate::graphics::Color;
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

/// What to paint with, as Compose's `Brush`: a solid color, or a linear, radial or sweep
/// gradient. A gradient's positions are in the drawing's coordinates (the draw scope's), and
/// default to its size: `Offset::INFINITE` coordinates are its right or bottom, an
/// `Offset::UNSPECIFIED` center its center, an infinite radius half its smaller side.
#[derive(Clone, PartialEq, Debug)]
pub enum Brush {
    /// As `SolidColor`.
    Solid(Color),
    Linear {
        gradient: Gradient,
        start: Offset,
        end: Offset,
    },
    Radial {
        gradient: Gradient,
        center: Offset,
        radius: f32,
    },
    Sweep {
        gradient: Gradient,
        center: Offset,
    },
}

/// A gradient's colors, their stops (evenly spread without), and its tile mode.
#[derive(Clone, PartialEq, Debug)]
pub struct Gradient {
    pub colors: Arc<[Color]>,
    /// Where each color is, from 0 to 1: as many as the colors, increasing.
    pub stops: Option<Arc<[f32]>>,
    pub tile_mode: TileMode,
}

impl Gradient {
    fn new(colors: &[Color], tile_mode: TileMode) -> Gradient {
        Gradient { colors: colors.into(), stops: None, tile_mode }
    }

    fn with_stops(color_stops: &[(f32, Color)], tile_mode: TileMode) -> Gradient {
        Gradient {
            colors: color_stops.iter().map(|(_, color)| *color).collect(),
            stops: Some(color_stops.iter().map(|(stop, _)| *stop).collect()),
            tile_mode,
        }
    }

    /// The colors' average (each channel, alpha weighted), what a renderer without shaders
    /// paints instead.
    fn average(&self) -> Color {
        let count = self.colors.len().max(1) as f32;
        let (mut alpha, mut red, mut green, mut blue) = (0.0, 0.0, 0.0, 0.0);
        for color in self.colors.iter() {
            let weight = color.alpha() as f32;
            alpha += weight;
            red += color.red() as f32 * weight;
            green += color.green() as f32 * weight;
            blue += color.blue() as f32 * weight;
        }
        if alpha == 0.0 {
            return Color::TRANSPARENT;
        }
        let channel = |value: f32| (value / alpha).round().clamp(0.0, 255.0) as u8;
        Color::from_rgb(channel(red), channel(green), channel(blue))
            .with_alpha(alpha / count / 255.0)
    }
}

impl Brush {
    /// A solid color, as `SolidColor(color)`.
    pub fn solid(color: Color) -> Brush {
        Brush::Solid(color)
    }

    /// Colors evenly spread from `start` to `end`, as `Brush.linearGradient(colors, start,
    /// end, tileMode)` (Compose's defaults: `Offset::ZERO` to `Offset::INFINITE`, the
    /// drawing's diagonal, `TileMode::Clamp`).
    pub fn linear_gradient(
        colors: &[Color],
        start: Offset,
        end: Offset,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::Linear { gradient: Gradient::new(colors, tile_mode), start, end }
    }

    /// `linear_gradient` with each color at its stop, as `linearGradient(vararg colorStops)`.
    pub fn linear_gradient_stops(
        color_stops: &[(f32, Color)],
        start: Offset,
        end: Offset,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::Linear { gradient: Gradient::with_stops(color_stops, tile_mode), start, end }
    }

    /// A left-to-right gradient from `start_x` to `end_x`, as `Brush.horizontalGradient`
    /// (Compose's defaults: 0 to `f32::INFINITY`, the drawing's width).
    pub fn horizontal_gradient(
        colors: &[Color],
        start_x: f32,
        end_x: f32,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::linear_gradient(
            colors,
            Offset::new(start_x, 0.0),
            Offset::new(end_x, 0.0),
            tile_mode,
        )
    }

    /// A top-to-bottom gradient from `start_y` to `end_y`, as `Brush.verticalGradient`.
    pub fn vertical_gradient(
        colors: &[Color],
        start_y: f32,
        end_y: f32,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::linear_gradient(
            colors,
            Offset::new(0.0, start_y),
            Offset::new(0.0, end_y),
            tile_mode,
        )
    }

    /// Colors evenly spread from `center` out to `radius`, as `Brush.radialGradient`
    /// (Compose's defaults: `Offset::UNSPECIFIED`, the drawing's center, and `f32::INFINITY`,
    /// half its smaller side).
    pub fn radial_gradient(
        colors: &[Color],
        center: Offset,
        radius: f32,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::Radial { gradient: Gradient::new(colors, tile_mode), center, radius }
    }

    /// `radial_gradient` with each color at its stop.
    pub fn radial_gradient_stops(
        color_stops: &[(f32, Color)],
        center: Offset,
        radius: f32,
        tile_mode: TileMode,
    ) -> Brush {
        Brush::Radial { gradient: Gradient::with_stops(color_stops, tile_mode), center, radius }
    }

    /// Colors around `center`, clockwise from 3 o'clock, as `Brush.sweepGradient`.
    pub fn sweep_gradient(colors: &[Color], center: Offset) -> Brush {
        Brush::Sweep { gradient: Gradient::new(colors, TileMode::Clamp), center }
    }

    /// The shader painting this brush over a drawing of `size` whose top left is at `origin`
    /// (in the display list's coordinates), at `alpha`; `None` for a solid color.
    pub fn shader(&self, size: Size, origin: Offset, alpha: f32) -> Option<Shader> {
        let point = |offset: Offset| {
            let x = if offset.x.is_infinite() { size.width } else { offset.x };
            let y = if offset.y.is_infinite() { size.height } else { offset.y };
            Offset::new(x + origin.x, y + origin.y)
        };
        let center = |center: Offset| {
            if center.is_specified() {
                point(center)
            } else {
                Offset::new(origin.x + size.width / 2.0, origin.y + size.height / 2.0)
            }
        };
        let alpha = alpha.clamp(0.0, 1.0);
        match self {
            Brush::Solid(_) => None,
            Brush::Linear { gradient, start, end } => Some(Shader::LinearGradient {
                gradient: gradient.clone(),
                from: point(*start),
                to: point(*end),
                alpha,
            }),
            Brush::Radial { gradient, center: at, radius } => Some(Shader::RadialGradient {
                gradient: gradient.clone(),
                center: center(*at),
                radius: if radius.is_infinite() {
                    size.width.min(size.height) / 2.0
                } else {
                    *radius
                },
                alpha,
            }),
            Brush::Sweep { gradient, center: at } => Some(Shader::SweepGradient {
                gradient: gradient.clone(),
                center: center(*at),
                alpha,
            }),
        }
    }

    /// The color a paint has with this brush: its color, or a gradient's average (what a
    /// renderer without shaders paints), at `alpha`.
    pub fn color(&self, alpha: f32) -> Color {
        let color = match self {
            Brush::Solid(color) => *color,
            Brush::Linear { gradient, .. }
            | Brush::Radial { gradient, .. }
            | Brush::Sweep { gradient, .. } => gradient.average(),
        };
        color.with_alpha(color.alpha() as f32 / 255.0 * alpha.clamp(0.0, 1.0))
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Brush {
        Brush::Solid(color)
    }
}

impl ComposeArg for Brush {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A brush resolved for a drawing, in the display list's coordinates, with its alpha.
#[derive(Clone, PartialEq, Debug)]
pub enum Shader {
    LinearGradient { gradient: Gradient, from: Offset, to: Offset, alpha: f32 },
    RadialGradient { gradient: Gradient, center: Offset, radius: f32, alpha: f32 },
    SweepGradient { gradient: Gradient, center: Offset, alpha: f32 },
}

impl Shader {
    pub fn gradient(&self) -> &Gradient {
        match self {
            Shader::LinearGradient { gradient, .. }
            | Shader::RadialGradient { gradient, .. }
            | Shader::SweepGradient { gradient, .. } => gradient,
        }
    }

    pub fn alpha(&self) -> f32 {
        match self {
            Shader::LinearGradient { alpha, .. }
            | Shader::RadialGradient { alpha, .. }
            | Shader::SweepGradient { alpha, .. } => *alpha,
        }
    }
}

/// A shader of a display list, by its index in `DisplayList::shaders`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ShaderIndex(pub u32);
