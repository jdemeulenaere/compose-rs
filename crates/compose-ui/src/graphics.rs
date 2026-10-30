//! Colors, shapes and paint, as Compose's `androidx.compose.ui.graphics`.

use std::fmt;

use compose_runtime::ComposeArg;

use crate::unit::{Density, Dp, Rect, Size};

/// A color, as packed 8-bit ARGB; `Color::UNSPECIFIED` means "use the default".
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color(u32);

impl Color {
    pub const BLACK: Color = Color(0xFF00_0000);
    pub const WHITE: Color = Color(0xFFFF_FFFF);
    pub const RED: Color = Color(0xFFFF_0000);
    pub const GREEN: Color = Color(0xFF00_FF00);
    pub const BLUE: Color = Color(0xFF00_00FF);
    pub const GRAY: Color = Color(0xFF88_8888);
    pub const LIGHT_GRAY: Color = Color(0xFFCC_CCCC);
    pub const DARK_GRAY: Color = Color(0xFF44_4444);
    pub const YELLOW: Color = Color(0xFFFF_FF00);
    pub const CYAN: Color = Color(0xFF00_FFFF);
    pub const MAGENTA: Color = Color(0xFFFF_00FF);
    pub const TRANSPARENT: Color = Color(0x0000_0000);
    /// No color: as `Color.Unspecified`. Not a valid ARGB value to draw with.
    pub const UNSPECIFIED: Color = Color(0x0000_0001);

    /// From packed ARGB, as `Color(0xFF6200EE)`.
    pub const fn from_argb(argb: u32) -> Color {
        Color(argb)
    }

    pub const fn from_rgb(red: u8, green: u8, blue: u8) -> Color {
        Color(0xFF00_0000 | (red as u32) << 16 | (green as u32) << 8 | blue as u32)
    }

    pub const fn argb(self) -> u32 {
        self.0
    }

    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }

    pub const fn red(self) -> u8 {
        (self.0 >> 16) as u8
    }

    pub const fn green(self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub const fn blue(self) -> u8 {
        self.0 as u8
    }

    pub fn is_specified(self) -> bool {
        self != Color::UNSPECIFIED
    }

    /// The same color with `alpha` (0 to 1) of opacity, as `copy(alpha = …)`.
    pub fn with_alpha(self, alpha: f32) -> Color {
        let alpha = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        Color(alpha << 24 | (self.0 & 0x00FF_FFFF))
    }

    /// This color drawn over `background`, as `compositeOver`: an opaque color for an opaque
    /// background.
    pub fn composite_over(self, background: Color) -> Color {
        let front = self.alpha() as f32 / 255.0;
        let back = background.alpha() as f32 / 255.0;
        let alpha = front + back * (1.0 - front);
        if alpha <= 0.0 {
            return Color(0);
        }
        let channel = |front_value: u8, back_value: u8| {
            let value =
                (front_value as f32 * front + back_value as f32 * back * (1.0 - front)) / alpha;
            value.round().clamp(0.0, 255.0) as u32
        };
        Color(
            ((alpha * 255.0).round() as u32) << 24
                | channel(self.red(), background.red()) << 16
                | channel(self.green(), background.green()) << 8
                | channel(self.blue(), background.blue()),
        )
    }

    /// `self` if specified, else `other`, as `takeOrElse`.
    pub fn or(self, other: Color) -> Color {
        if self.is_specified() { self } else { other }
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Color::UNSPECIFIED {
            formatter.write_str("Color::UNSPECIFIED")
        } else {
            write!(formatter, "#{:08X}", self.0)
        }
    }
}

impl ComposeArg for Color {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A corner's size, as Compose's `CornerSize`: fixed in dp or pixels, or a percentage of the
/// shape's shorter side.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CornerSize {
    /// As `CornerSize(size: Dp)`.
    Dp(Dp),
    /// As `CornerSize(size: Float)`, in pixels.
    Px(f32),
    /// As `CornerSize(percent: Int)`, of the shorter side (0 to 100).
    Percent(f32),
}

impl CornerSize {
    /// No corner, as `ZeroCornerSize`.
    pub const ZERO: CornerSize = CornerSize::Px(0.0);

    /// A percentage of the shorter side, as `CornerSize(percent)`: panics outside 0 to 100, as
    /// Compose throws.
    pub fn percent(percent: i32) -> CornerSize {
        assert!(
            (0..=100).contains(&percent),
            "compose-ui: the percent should be in the range of [0, 100], was {percent}"
        );
        CornerSize::Percent(percent as f32)
    }

    /// The corner's size in pixels for a shape of `shape_size`, as `toPx` (not limited: the
    /// shape scales its corners down to fit).
    pub fn to_px(self, shape_size: Size, density: &Density) -> f32 {
        match self {
            CornerSize::Dp(dp) => density.to_px(dp),
            CornerSize::Px(px) => px,
            CornerSize::Percent(percent) => {
                shape_size.width.min(shape_size.height) * (percent / 100.0)
            }
        }
    }
}

impl From<Dp> for CornerSize {
    fn from(size: Dp) -> CornerSize {
        CornerSize::Dp(size)
    }
}

impl From<f32> for CornerSize {
    /// Pixels, as `CornerSize(size: Float)`.
    fn from(px: f32) -> CornerSize {
        CornerSize::Px(px)
    }
}

impl From<i32> for CornerSize {
    /// A percentage, as `CornerSize(percent: Int)`.
    fn from(percent: i32) -> CornerSize {
        CornerSize::percent(percent)
    }
}

/// What the corner-based shapes take: one size for every corner (a `Dp`, an `f32` in pixels, an
/// `i32` percentage, a `CornerSize`), or four, Compose's overloads taking `topStart`, `topEnd`,
/// `bottomEnd` and `bottomStart` (top left, top right, bottom right and bottom left for the
/// absolute shapes).
pub trait CornerSizes {
    fn corner_sizes(self) -> [CornerSize; 4];
}

impl<T: Into<CornerSize>> CornerSizes for T {
    fn corner_sizes(self) -> [CornerSize; 4] {
        [self.into(); 4]
    }
}

impl<A: Into<CornerSize>, B: Into<CornerSize>, C: Into<CornerSize>, D: Into<CornerSize>> CornerSizes
    for (A, B, C, D)
{
    fn corner_sizes(self) -> [CornerSize; 4] {
        [self.0.into(), self.1.into(), self.2.into(), self.3.into()]
    }
}

impl CornerSizes for [CornerSize; 4] {
    fn corner_sizes(self) -> [CornerSize; 4] {
        self
    }
}

/// A shape of the app's own, as a class implementing Compose's `Shape`: wrap it with
/// [`Shape::custom`].
pub trait CustomShape: std::any::Any {
    /// Its outline in a box of `size`, as `createOutline`.
    fn create_outline(
        &self,
        size: Size,
        layout_direction: crate::alignment::LayoutDirection,
        density: &Density,
    ) -> Outline;

    /// Whether `other` is the same shape, as Compose's `equals`: a shape with no properties
    /// compares its type (`other.downcast_ref::<Self>().is_some()`).
    fn same(&self, other: &dyn CustomShape) -> bool;
}

impl dyn CustomShape {
    /// `self` as `T`, for [`same`](CustomShape::same).
    pub fn downcast_ref<T: CustomShape>(&self) -> Option<&T> {
        (self as &dyn std::any::Any).downcast_ref::<T>()
    }
}

/// A shape, as Compose's `Shape`: what backgrounds, borders, clips and shadows follow. Compose's
/// corner-based shapes are variants, so they're plain values (`foundation::RoundedCornerShape`
/// and its siblings make them); any other is `Custom` (`foundation::GenericShape`, or an app's
/// own [`CustomShape`]).
#[derive(Clone)]
pub enum Shape {
    /// As `RectangleShape`.
    Rectangle,
    /// Rounded corners: top start, top end, bottom end, bottom start (mirrored right to left),
    /// as `RoundedCornerShape`.
    Rounded([CornerSize; 4]),
    /// Cut corners, as `CutCornerShape`.
    Cut([CornerSize; 4]),
    /// Rounded corners: top left, top right, bottom right, bottom left, whatever the direction,
    /// as `AbsoluteRoundedCornerShape`.
    AbsoluteRounded([CornerSize; 4]),
    /// Cut corners top left, top right, bottom right, bottom left, as `AbsoluteCutCornerShape`.
    AbsoluteCut([CornerSize; 4]),
    /// Any other shape.
    Custom(std::rc::Rc<dyn CustomShape>),
}

/// As Compose's `RectangleShape`.
pub const RECTANGLE_SHAPE: Shape = Shape::Rectangle;

impl Shape {
    /// A shape of the app's own.
    pub fn custom(shape: impl CustomShape) -> Shape {
        Shape::Custom(std::rc::Rc::new(shape))
    }

    /// Its corners (top start, top end, bottom end, bottom start; or top left, top right,
    /// bottom right, bottom left for the absolute ones) if it's corner-based, as
    /// `CornerBasedShape`'s `topStart` and siblings.
    pub fn corners(&self) -> Option<[CornerSize; 4]> {
        match self {
            Shape::Rounded(corners)
            | Shape::Cut(corners)
            | Shape::AbsoluteRounded(corners)
            | Shape::AbsoluteCut(corners) => Some(*corners),
            Shape::Rectangle | Shape::Custom(_) => None,
        }
    }

    /// A corner-based shape of the same kind with `corners`, as `CornerBasedShape.copy`: one
    /// size for all, or four (`(top_start, top_end, bottom_end, bottom_start)`); another shape
    /// as it is.
    pub fn copy(&self, corners: impl CornerSizes) -> Shape {
        let corners = corners.corner_sizes();
        match self {
            Shape::Rounded(_) => Shape::Rounded(corners),
            Shape::Cut(_) => Shape::Cut(corners),
            Shape::AbsoluteRounded(_) => Shape::AbsoluteRounded(corners),
            Shape::AbsoluteCut(_) => Shape::AbsoluteCut(corners),
            Shape::Rectangle | Shape::Custom(_) => self.clone(),
        }
    }

    /// The shape's outline in a box of `size`, left to right.
    pub fn outline(&self, size: Size, density: &Density) -> Outline {
        self.outline_in(size, density, crate::alignment::LayoutDirection::Ltr)
    }

    /// The shape's outline in a box of `size`, as `createOutline`. A corner-based shape scales
    /// its two start corners down together, and its two end corners, when they're more than
    /// the shorter side, as Compose's `CornerBasedShape` (a negative corner panics, as Compose
    /// throws); the relative ones mirror their corners right to left.
    pub fn outline_in(
        &self,
        size: Size,
        density: &Density,
        direction: crate::alignment::LayoutDirection,
    ) -> Outline {
        let rect = Rect::new(0.0, 0.0, size.width, size.height);
        let (corners, rounded, absolute) = match self {
            Shape::Rectangle => return Outline::Rectangle(rect),
            Shape::Custom(shape) => return shape.create_outline(size, direction, density),
            Shape::Rounded(corners) => (corners, true, false),
            Shape::Cut(corners) => (corners, false, false),
            Shape::AbsoluteRounded(corners) => (corners, true, true),
            Shape::AbsoluteCut(corners) => (corners, false, true),
        };
        let [mut top_start, mut top_end, mut bottom_end, mut bottom_start] =
            corners.map(|corner| corner.to_px(size, density));
        let shorter = size.width.min(size.height);
        if top_start + bottom_start > shorter {
            let scale = shorter / (top_start + bottom_start);
            top_start *= scale;
            bottom_start *= scale;
        }
        if top_end + bottom_end > shorter {
            let scale = shorter / (top_end + bottom_end);
            top_end *= scale;
            bottom_end *= scale;
        }
        assert!(
            top_start >= 0.0 && top_end >= 0.0 && bottom_end >= 0.0 && bottom_start >= 0.0,
            "compose-ui: corner size in px can't be negative (top_start = {top_start}, top_end = \
             {top_end}, bottom_end = {bottom_end}, bottom_start = {bottom_start})"
        );
        if top_start + top_end + bottom_end + bottom_start == 0.0 {
            return Outline::Rectangle(rect);
        }
        // Top left, top right, bottom right, bottom left.
        let ltr = absolute || direction == crate::alignment::LayoutDirection::Ltr;
        let radii = if ltr {
            [top_start, top_end, bottom_end, bottom_start]
        } else {
            [top_end, top_start, bottom_start, bottom_end]
        };
        if rounded { Outline::Rounded(rect, radii) } else { Outline::cut(rect, radii) }
    }
}

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Shape::Rectangle, Shape::Rectangle) => true,
            (Shape::Rounded(a), Shape::Rounded(b))
            | (Shape::Cut(a), Shape::Cut(b))
            | (Shape::AbsoluteRounded(a), Shape::AbsoluteRounded(b))
            | (Shape::AbsoluteCut(a), Shape::AbsoluteCut(b)) => a == b,
            (Shape::Custom(a), Shape::Custom(b)) => std::rc::Rc::ptr_eq(a, b) || (**a).same(&**b),
            _ => false,
        }
    }
}

impl fmt::Debug for Shape {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shape::Rectangle => formatter.write_str("Rectangle"),
            Shape::Rounded(corners) => write!(formatter, "Rounded({corners:?})"),
            Shape::Cut(corners) => write!(formatter, "Cut({corners:?})"),
            Shape::AbsoluteRounded(corners) => write!(formatter, "AbsoluteRounded({corners:?})"),
            Shape::AbsoluteCut(corners) => write!(formatter, "AbsoluteCut({corners:?})"),
            Shape::Custom(_) => formatter.write_str("Custom"),
        }
    }
}

impl ComposeArg for Shape {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A shape's outline in pixels, as `Outline`.
#[derive(Clone, PartialEq, Debug)]
pub enum Outline {
    Rectangle(Rect),
    /// Corner radii: top left, top right, bottom right, bottom left.
    Rounded(Rect, [f32; 4]),
    /// Any path, as `Outline.Generic(path)`: its contents (shared, as the display list holds
    /// them), moved by the offset.
    Generic(std::sync::Arc<crate::path::PathData>, crate::unit::Offset),
}

impl Outline {
    /// A path's outline, as `Outline.Generic(path)`: its contents now.
    pub fn generic(path: &crate::path::Path) -> Outline {
        Outline::Generic(path.data(), crate::unit::Offset::ZERO)
    }

    /// `rect` with its corners cut by `cuts` (top left, top right, bottom right, bottom left),
    /// as `CutCornerShape`'s path.
    fn cut(rect: Rect, cuts: [f32; 4]) -> Outline {
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let path = crate::path::Path::new();
        path.move_to(0.0, cuts[0]);
        path.line_to(cuts[0], 0.0);
        path.line_to(width - cuts[1], 0.0);
        path.line_to(width, cuts[1]);
        path.line_to(width, height - cuts[2]);
        path.line_to(width - cuts[2], height);
        path.line_to(cuts[3], height);
        path.line_to(0.0, height - cuts[3]);
        path.close();
        Outline::Generic(path.data(), crate::unit::Offset::new(rect.left, rect.top))
    }

    /// Its bounds, as `Outline.bounds`.
    pub fn bounds(&self) -> Rect {
        match self {
            Outline::Rectangle(rect) | Outline::Rounded(rect, _) => *rect,
            Outline::Generic(path, offset) => {
                let bounds = path.bounds();
                Rect::new(
                    bounds.left + offset.x,
                    bounds.top + offset.y,
                    bounds.right + offset.x,
                    bounds.bottom + offset.y,
                )
            }
        }
    }

    /// The same outline moved by `by`.
    pub fn translate(&self, by: crate::unit::Offset) -> Outline {
        let moved = |rect: &Rect| {
            Rect::new(rect.left + by.x, rect.top + by.y, rect.right + by.x, rect.bottom + by.y)
        };
        match self {
            Outline::Rectangle(rect) => Outline::Rectangle(moved(rect)),
            Outline::Rounded(rect, radii) => Outline::Rounded(moved(rect), *radii),
            Outline::Generic(path, offset) => Outline::Generic(path.clone(), *offset + by),
        }
    }

    /// Whether `point` is inside the outline: for clipping hit tests.
    pub fn contains(&self, point: crate::unit::Offset) -> bool {
        match self {
            Outline::Rectangle(rect) => rect.contains(point),
            Outline::Generic(path, offset) => path.contains(point - *offset),
            Outline::Rounded(rect, radii) => {
                if !rect.contains(point) {
                    return false;
                }
                // Each corner's circle: top left, top right, bottom right, bottom left.
                let corners = [
                    (rect.left + radii[0], rect.top + radii[0], radii[0]),
                    (rect.right - radii[1], rect.top + radii[1], radii[1]),
                    (rect.right - radii[2], rect.bottom - radii[2], radii[2]),
                    (rect.left + radii[3], rect.bottom - radii[3], radii[3]),
                ];
                corners.iter().enumerate().all(|(index, &(x, y, radius))| {
                    let beyond_x = if index == 0 || index == 3 { point.x < x } else { point.x > x };
                    let beyond_y = if index < 2 { point.y < y } else { point.y > y };
                    if !(beyond_x && beyond_y) {
                        return true;
                    }
                    let (dx, dy) = (point.x - x, point.y - y);
                    dx * dx + dy * dy <= radius * radius
                })
            }
        }
    }
}

/// How a shape is drawn: filled, or stroked with a width in pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PaintStyle {
    Fill,
    Stroke(f32),
}

/// How a stroke's open ends are drawn, as `StrokeCap`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum StrokeCap {
    /// Cut flat at the end.
    #[default]
    Butt,
    /// A half circle past the end.
    Round,
    /// A half square past the end.
    Square,
}

impl compose_runtime::ComposeArg for StrokeCap {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// What a draw operation paints with, as a subset of Compose's `Paint`.
///
/// A paint with a `shader` (a gradient, from `DrawScope::brush_paint`) paints with it; its
/// `color` is then the gradient's average, what a renderer without shaders paints instead.
#[derive(Clone, Copy, PartialEq, Debug)]
#[non_exhaustive]
pub struct Paint {
    pub color: Color,
    pub style: PaintStyle,
    /// A stroke's ends.
    pub cap: StrokeCap,
    /// The shader, in the display list it's recorded in (`DisplayList::shaders`).
    pub(crate) shader: Option<crate::brush::ShaderIndex>,
}

impl Paint {
    pub fn fill(color: Color) -> Paint {
        Paint { color, style: PaintStyle::Fill, cap: StrokeCap::Butt, shader: None }
    }

    /// A stroke `width` pixels wide, its ends flat, as `Stroke(width)`.
    pub fn stroke(color: Color, width: f32) -> Paint {
        Paint::stroke_with(color, width, StrokeCap::Butt)
    }

    /// A stroke with `cap` ends, as `Stroke(width, cap = …)`.
    pub fn stroke_with(color: Color, width: f32, cap: StrokeCap) -> Paint {
        Paint { color, style: PaintStyle::Stroke(width), cap, shader: None }
    }

    /// The same paint at `alpha` (0 to 1) of its opacity, as `Paint.alpha` and the draw calls'
    /// `alpha`: its color's alpha multiplied. A brush paint's gradient keeps the alpha given to
    /// `brush_paint` (only its fallback color changes): pass the alpha there.
    /// The shader a brush paint names, in the display list it's recorded in
    /// (`DisplayList::shaders`): for hosts.
    pub fn shader(&self) -> Option<crate::brush::ShaderIndex> {
        self.shader
    }

    /// This paint naming `shader` of the display list it goes in (a platform's shader index:
    /// `DrawScope::brush_paint` is the app-facing way).
    pub fn with_shader(self, shader: crate::brush::ShaderIndex) -> Paint {
        Paint { shader: Some(shader), ..self }
    }

    pub fn with_alpha(self, alpha: f32) -> Paint {
        let opacity = self.color.alpha() as f32 / 255.0 * alpha.clamp(0.0, 1.0);
        Paint { color: self.color.with_alpha(opacity), ..self }
    }
}

/// sRGB's transfer function, to linear light.
fn srgb_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 { channel / 12.92 } else { ((channel + 0.055) / 1.055).powf(2.4) }
}

/// sRGB's transfer function, from linear light.
fn linear_to_srgb(channel: f32) -> f32 {
    if channel <= 0.003_130_8 { channel * 12.92 } else { 1.055 * channel.powf(1.0 / 2.4) - 0.055 }
}

/// A color's Oklab lightness and opponents, and its alpha.
fn to_oklab(color: Color) -> [f32; 4] {
    let channel = |value: u8| srgb_to_linear(value as f32 / 255.0);
    let (r, g, b) = (channel(color.red()), channel(color.green()), channel(color.blue()));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        color.alpha() as f32 / 255.0,
    ]
}

fn from_oklab([lightness, a, b, alpha]: [f32; 4]) -> Color {
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let red = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
    let green = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
    let blue = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;
    let channel = |value: f32| (linear_to_srgb(value).clamp(0.0, 1.0) * 255.0).round() as u32;
    let alpha = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    Color::from_argb(alpha << 24 | channel(red) << 16 | channel(green) << 8 | channel(blue))
}

impl Color {
    /// Between `start` and `stop`, interpolated in the Oklab color space, as Compose's
    /// `lerp(Color, Color, Float)`; `fraction` is kept in 0 to 1. An unspecified end counts as
    /// transparent black, and two unspecified ends give unspecified, as Compose's (whose
    /// unspecified color is transparent black in an unspecified color space).
    pub fn lerp(start: Color, stop: Color, fraction: f32) -> Color {
        if !start.is_specified() && !stop.is_specified() {
            return Color::UNSPECIFIED;
        }
        let lab = |color: Color| to_oklab(if color.is_specified() { color } else { Color(0) });
        let (start, stop) = (lab(start), lab(stop));
        let t = fraction.clamp(0.0, 1.0);
        let mix = |index: usize| crate::unit::lerp_f32(start[index], stop[index], t);
        from_oklab([mix(0), mix(1), mix(2), mix(3)])
    }

    /// This color with its alpha multiplied by `alpha` (when not NaN and below 1), as
    /// Compose's internal `Color.modulate` for text.
    pub(crate) fn modulate(self, alpha: f32) -> Color {
        if alpha.is_nan() || alpha >= 1.0 {
            self
        } else {
            self.with_alpha(self.alpha() as f32 / 255.0 * alpha)
        }
    }
}

/// A shadow cast by text, as Compose's `Shadow`: its color, offset from the text, and blur
/// radius in pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shadow {
    pub color: Color,
    pub offset: crate::unit::Offset,
    pub blur_radius: f32,
}

impl Shadow {
    /// As `Shadow.None`: black, no offset, no blur.
    pub const NONE: Shadow =
        Shadow { color: Color::BLACK, offset: crate::unit::Offset::ZERO, blur_radius: 0.0 };

    /// Between `start` and `stop`, as Compose's `lerp(Shadow, Shadow, Float)`.
    pub fn lerp(start: Shadow, stop: Shadow, fraction: f32) -> Shadow {
        use crate::unit::lerp_f32;
        Shadow {
            color: Color::lerp(start.color, stop.color, fraction),
            offset: crate::unit::Offset::new(
                lerp_f32(start.offset.x, stop.offset.x, fraction),
                lerp_f32(start.offset.y, stop.offset.y, fraction),
            ),
            blur_radius: lerp_f32(start.blur_radius, stop.blur_radius, fraction),
        }
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow::NONE
    }
}

impl compose_runtime::ComposeArg for Shadow {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// How the corners of a stroke's segments are drawn, as `StrokeJoin`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum StrokeJoin {
    /// Sharp, up to the miter limit.
    #[default]
    Miter,
    Round,
    Bevel,
}

/// A stroke's geometry, as Compose's `Stroke` draw style (without a path effect).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Stroke {
    /// The width in pixels; 0 is a hairline.
    pub width: f32,
    /// How far a sharp corner may reach, as a multiple of the width.
    pub miter: f32,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
}

impl Stroke {
    /// As `Stroke.DefaultMiter`.
    pub const DEFAULT_MITER: f32 = 4.0;

    /// A stroke `width` pixels wide with Compose's defaults: miter 4, flat ends, mitered
    /// corners.
    pub const fn new(width: f32) -> Stroke {
        Stroke {
            width,
            miter: Stroke::DEFAULT_MITER,
            cap: StrokeCap::Butt,
            join: StrokeJoin::Miter,
        }
    }

    /// The paint stroking with it in `color` (a paint has the width and the ends; its join and
    /// miter are the renderer's defaults, a miter of 4, as `Stroke.DefaultJoin`).
    pub fn paint(self, color: Color) -> Paint {
        Paint::stroke_with(color, self.width, self.cap)
    }
}

impl compose_runtime::ComposeArg for Stroke {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke::new(0.0)
    }
}

/// Whether a drawing fills its shape or strokes its outline, as Compose's `DrawStyle` (`Fill`
/// or a `Stroke`).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum DrawStyle {
    #[default]
    Fill,
    Stroke(Stroke),
}

impl compose_runtime::ComposeArg for DrawStyle {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_pack_argb() {
        let color = Color::from_rgb(0x62, 0x00, 0xEE);
        assert_eq!(color.argb(), 0xFF62_00EE);
        assert_eq!(
            (color.red(), color.green(), color.blue(), color.alpha()),
            (0x62, 0, 0xEE, 0xFF)
        );
        assert_eq!(color.with_alpha(0.5).alpha(), 128);
        assert_eq!(Color::UNSPECIFIED.or(Color::RED), Color::RED);
    }

    /// Corners larger than the box scale down to fit, as Compose's `CornerBasedShape`.
    #[test]
    fn oversized_rounded_corners_scale_down_to_fit() {
        let shape = Shape::Rounded([CornerSize::Dp(Dp(100.0)); 4]);
        let outline = shape.outline(Size::new(40.0, 20.0), &Density::default());
        assert_eq!(outline, Outline::Rounded(Rect::new(0.0, 0.0, 40.0, 20.0), [10.0; 4]));
        let pill = Shape::Rounded([CornerSize::Percent(50.0); 4]);
        assert_eq!(pill.outline(Size::new(40.0, 20.0), &Density::default()), outline);
    }
}
