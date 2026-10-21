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
    /// The shape's outline in a box of `size`.
    pub fn outline(&self, size: Size, density: &Density) -> Outline {
        let rect = Rect::new(0.0, 0.0, size.width, size.height);
        match self {
            Shape::Rectangle => Outline::Rectangle(rect),
            Shape::Rounded(corners) => {
                let radii = corners.map(|corner| corner.to_px(size, density));
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

/// What a draw operation paints with, as a subset of Compose's `Paint`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Paint {
    pub color: Color,
    pub style: PaintStyle,
}

impl Paint {
    pub fn fill(color: Color) -> Paint {
        Paint { color, style: PaintStyle::Fill }
    }

    pub fn stroke(color: Color, width: f32) -> Paint {
        Paint { color, style: PaintStyle::Stroke(width) }
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
