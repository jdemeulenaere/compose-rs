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

/// A corner size, as `CornerSize`: a fixed size or a percentage of the shorter side.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CornerSize {
    Dp(Dp),
    Percent(f32),
}

impl CornerSize {
    pub fn to_px(self, size: Size, density: &Density) -> f32 {
        let shorter = size.width.min(size.height);
        let px = match self {
            CornerSize::Dp(dp) => density.to_px(dp),
            CornerSize::Percent(percent) => shorter * percent / 100.0,
        };
        px.clamp(0.0, shorter / 2.0)
    }
}

/// A shape, as Compose's `Shape`: what backgrounds, borders and clips follow.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    Rectangle,
    /// Rounded corners: top start, top end, bottom end, bottom start.
    Rounded([CornerSize; 4]),
}

/// As Compose's `RectangleShape`.
pub const RECTANGLE_SHAPE: Shape = Shape::Rectangle;

impl Shape {
    /// The shape's outline in a box of `size`, left to right.
    pub fn outline(&self, size: Size, density: &Density) -> Outline {
        self.outline_in(size, density, crate::alignment::LayoutDirection::Ltr)
    }

    /// The shape's outline in a box of `size`, its start and end corners mirrored in a
    /// right-to-left layout, as Compose's `RoundedCornerShape`.
    pub fn outline_in(
        &self,
        size: Size,
        density: &Density,
        direction: crate::alignment::LayoutDirection,
    ) -> Outline {
        let rect = Rect::new(0.0, 0.0, size.width, size.height);
        match self {
            Shape::Rectangle => Outline::Rectangle(rect),
            Shape::Rounded(corners) => {
                let mut radii = corners.map(|corner| corner.to_px(size, density));
                // Top left, top right, bottom right, bottom left.
                if direction == crate::alignment::LayoutDirection::Rtl {
                    radii = [radii[1], radii[0], radii[3], radii[2]];
                }
                if radii.iter().all(|radius| *radius == 0.0) {
                    Outline::Rectangle(rect)
                } else {
                    Outline::Rounded(rect, radii)
                }
            }
        }
    }
}

impl ComposeArg for Shape {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A shape's outline in pixels, as `Outline`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Outline {
    Rectangle(Rect),
    /// Corner radii: top left, top right, bottom right, bottom left.
    Rounded(Rect, [f32; 4]),
}

impl Outline {
    /// Whether `point` is inside the outline: for clipping hit tests.
    pub fn contains(&self, point: crate::unit::Offset) -> bool {
        match self {
            Outline::Rectangle(rect) => rect.contains(point),
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

    #[test]
    fn rounded_corners_are_clamped_to_half_the_shorter_side() {
        let shape = Shape::Rounded([CornerSize::Dp(Dp(100.0)); 4]);
        let outline = shape.outline(Size::new(40.0, 20.0), &Density::default());
        assert_eq!(outline, Outline::Rounded(Rect::new(0.0, 0.0, 40.0, 20.0), [10.0; 4]));
        let pill = Shape::Rounded([CornerSize::Percent(50.0); 4]);
        assert_eq!(pill.outline(Size::new(40.0, 20.0), &Density::default()), outline);
    }
}
