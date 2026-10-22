//! Units and geometry: `Dp`, `Sp`, `Density`, sizes, offsets and rectangles, as Compose's
//! `androidx.compose.ui.unit` and `geometry`.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

use compose_runtime::ComposeArg;

/// Density-independent pixels: the unit of layout, as Compose's `Dp`.
#[derive(Clone, Copy, PartialOrd, Default)]
pub struct Dp(pub f32);

/// Equal, `UNSPECIFIED` (NaN) included: unspecified equals unspecified, as Compose's value
/// classes compare their bits.
fn same_value(a: f32, b: f32) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

impl PartialEq for Dp {
    fn eq(&self, other: &Self) -> bool {
        same_value(self.0, other.0)
    }
}

/// `x` rounded to the nearest integer, halves up, as Kotlin's `roundToInt` (`Math.round`), which
/// Compose's layout uses: `-1.5` rounds to `-1`.
pub fn round_to_int(x: f32) -> i32 {
    (x + 0.5).floor() as i32
}

impl Dp {
    pub const ZERO: Dp = Dp(0.0);
    /// No value: a default meaning "decide elsewhere", as `Dp.Unspecified`.
    pub const UNSPECIFIED: Dp = Dp(f32::NAN);

    pub fn is_specified(self) -> bool {
        !self.0.is_nan()
    }

    pub fn value(self) -> f32 {
        self.0
    }
}

impl fmt::Debug for Dp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.dp", self.0)
    }
}

impl ComposeArg for Dp {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl Add for Dp {
    type Output = Dp;
    fn add(self, other: Dp) -> Dp {
        Dp(self.0 + other.0)
    }
}

impl Sub for Dp {
    type Output = Dp;
    fn sub(self, other: Dp) -> Dp {
        Dp(self.0 - other.0)
    }
}

impl Mul<f32> for Dp {
    type Output = Dp;
    fn mul(self, factor: f32) -> Dp {
        Dp(self.0 * factor)
    }
}

impl Div<f32> for Dp {
    type Output = Dp;
    fn div(self, divisor: f32) -> Dp {
        Dp(self.0 / divisor)
    }
}

impl Neg for Dp {
    type Output = Dp;
    fn neg(self) -> Dp {
        Dp(-self.0)
    }
}

/// Scale-independent pixels: the unit of text size, as Compose's `TextUnit` in `sp`.
#[derive(Clone, Copy, PartialOrd, Default)]
pub struct Sp(pub f32);

impl PartialEq for Sp {
    fn eq(&self, other: &Self) -> bool {
        same_value(self.0, other.0)
    }
}

impl Sp {
    pub const UNSPECIFIED: Sp = Sp(f32::NAN);

    pub fn is_specified(self) -> bool {
        !self.0.is_nan()
    }
}

impl fmt::Debug for Sp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.sp", self.0)
    }
}

impl ComposeArg for Sp {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `16.dp()`, `14.sp()`, as Kotlin's `16.dp`.
pub trait UnitExt {
    fn dp(self) -> Dp;
    fn sp(self) -> Sp;
}

macro_rules! unit_ext {
    ($($type:ty),*) => {
        $(impl UnitExt for $type {
            fn dp(self) -> Dp {
                Dp(self as f32)
            }
            fn sp(self) -> Sp {
                Sp(self as f32)
            }
        })*
    };
}

unit_ext!(i32, u32, f32, f64, usize);

/// Pixels per `Dp`, and the font scale applied to `Sp`, as Compose's `Density`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Density {
    pub density: f32,
    pub font_scale: f32,
}

impl compose_runtime::ComposeArg for Density {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl Default for Density {
    fn default() -> Self {
        Density { density: 1.0, font_scale: 1.0 }
    }
}

impl Density {
    pub fn new(density: f32) -> Density {
        Density { density, font_scale: 1.0 }
    }

    /// The number of pixels for `dp`, rounded as Compose's `roundToPx`.
    pub fn round_to_px(&self, dp: Dp) -> i32 {
        let px = dp.0 * self.density;
        if px.is_infinite() { if px > 0.0 { i32::MAX } else { i32::MIN } } else { round_to_int(px) }
    }

    pub fn to_px(&self, dp: Dp) -> f32 {
        dp.0 * self.density
    }

    pub fn sp_to_px(&self, sp: Sp) -> f32 {
        sp.0 * self.font_scale * self.density
    }

    pub fn to_dp(&self, px: f32) -> Dp {
        Dp(px / self.density)
    }
}

/// A size in pixels, as `IntSize`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct IntSize {
    pub width: i32,
    pub height: i32,
}

impl IntSize {
    pub const ZERO: IntSize = IntSize { width: 0, height: 0 };

    pub const fn new(width: i32, height: i32) -> IntSize {
        IntSize { width, height }
    }
}

impl fmt::Debug for IntSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}x{}", self.width, self.height)
    }
}

/// A position in pixels, as `IntOffset`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct IntOffset {
    pub x: i32,
    pub y: i32,
}

impl IntOffset {
    pub const ZERO: IntOffset = IntOffset { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> IntOffset {
        IntOffset { x, y }
    }
}

impl fmt::Debug for IntOffset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "({}, {})", self.x, self.y)
    }
}

impl Add for IntOffset {
    type Output = IntOffset;
    fn add(self, other: IntOffset) -> IntOffset {
        IntOffset::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for IntOffset {
    type Output = IntOffset;
    fn sub(self, other: IntOffset) -> IntOffset {
        IntOffset::new(self.x - other.x, self.y - other.y)
    }
}

/// A floating-point position, as `Offset`: pointer positions and drawing.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub const ZERO: Offset = Offset { x: 0.0, y: 0.0 };
    /// Infinitely far, as `Offset.Infinite`: a gradient's default end (the drawing's size).
    pub const INFINITE: Offset = Offset { x: f32::INFINITY, y: f32::INFINITY };
    /// No position, as `Offset.Unspecified`: a radial gradient's default center (the
    /// drawing's center).
    pub const UNSPECIFIED: Offset = Offset { x: f32::NAN, y: f32::NAN };

    pub const fn new(x: f32, y: f32) -> Offset {
        Offset { x, y }
    }

    /// Whether it's a position (not `UNSPECIFIED`), as `isSpecified`.
    pub fn is_specified(self) -> bool {
        !self.x.is_nan() && !self.y.is_nan()
    }
}

impl Add for Offset {
    type Output = Offset;
    fn add(self, other: Offset) -> Offset {
        Offset::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Offset {
    type Output = Offset;
    fn sub(self, other: Offset) -> Offset {
        Offset::new(self.x - other.x, self.y - other.y)
    }
}

/// A floating-point size, as `Size`.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// A width and a height in `Dp`, as Compose's `DpSize`: what `Modifier.size` takes, from a
/// `Dp` (square) or a `(width, height)` tuple.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DpSize {
    pub width: Dp,
    pub height: Dp,
}

impl DpSize {
    pub const fn new(width: Dp, height: Dp) -> DpSize {
        DpSize { width, height }
    }
}

impl From<Dp> for DpSize {
    /// A square.
    fn from(size: Dp) -> DpSize {
        DpSize::new(size, size)
    }
}

impl From<(Dp, Dp)> for DpSize {
    fn from((width, height): (Dp, Dp)) -> DpSize {
        DpSize::new(width, height)
    }
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };

    pub const fn new(width: f32, height: f32) -> Size {
        Size { width, height }
    }
}

impl From<IntSize> for Size {
    fn from(size: IntSize) -> Size {
        Size::new(size.width as f32, size.height as f32)
    }
}

/// A floating-point rectangle, as `Rect`.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Rect {
        Rect { left, top, right, bottom }
    }

    pub fn from_offset_size(offset: Offset, size: Size) -> Rect {
        Rect::new(offset.x, offset.y, offset.x + size.width, offset.y + size.height)
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    pub fn contains(&self, point: Offset) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dp_converts_with_density() {
        let density = Density::new(2.0);
        assert_eq!(density.round_to_px(16.dp()), 32);
        assert_eq!(density.round_to_px(Dp(0.3)), 1);
        assert_eq!(density.to_dp(10.0), 5.dp());
        assert_eq!(Density { density: 2.0, font_scale: 1.5 }.sp_to_px(10.sp()), 30.0);
        assert!(!Dp::UNSPECIFIED.is_specified());
        assert!(Dp::UNSPECIFIED.same(&Dp::UNSPECIFIED));
    }

    #[test]
    fn rects_contain_their_top_left_but_not_their_bottom_right() {
        let rect = Rect::new(0.0, 0.0, 10.0, 5.0);
        assert!(rect.contains(Offset::new(0.0, 0.0)));
        assert!(rect.contains(Offset::new(9.9, 4.9)));
        assert!(!rect.contains(Offset::new(10.0, 2.0)));
    }
}

macro_rules! compose_arg_by_value {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

compose_arg_by_value!(IntOffset, IntSize, Offset, Size, Rect);
