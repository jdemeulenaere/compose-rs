//! Units and geometry: `Dp`, `TextUnit`, `Density`, sizes, offsets and rectangles, as Compose's
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
    /// The thinnest line, as `Dp.Hairline`: no space taken, a stroke or border of it a single pixel
    /// wide whatever the density (a stroke of width 0).
    pub const HAIRLINE: Dp = Dp(0.0);
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

/// What a [`TextUnit`] measures, as Compose's `TextUnitType`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum TextUnitType {
    /// No value: decided elsewhere (inherited, or a default).
    #[default]
    Unspecified,
    /// Scale-independent pixels: density-independent pixels times the font scale.
    Sp,
    /// A multiple of the font size (for a font size, of the size it's relative to).
    Em,
}

/// A text measure, as Compose's `TextUnit`: in `sp`, in `em`, or unspecified (decided
/// elsewhere). Made with `14.sp()` and `1.5.em()` ([`UnitExt`]) or [`TextUnit::sp`] and
/// [`TextUnit::em`].
///
/// Equal when their types and their values' bits are, as Compose's packed value: unspecified
/// equals unspecified. Arithmetic needs a specified unit, and comparisons two of the same type:
/// otherwise they panic, as Compose's throw.
#[derive(Clone, Copy)]
pub struct TextUnit {
    value: f32,
    unit: TextUnitType,
}

impl TextUnit {
    /// As `TextUnit.Unspecified`: its value is NaN.
    pub const UNSPECIFIED: TextUnit = TextUnit { value: f32::NAN, unit: TextUnitType::Unspecified };

    /// As `TextUnit(value, type)`. An unspecified type's value is NaN, whatever is given.
    pub const fn new(value: f32, unit: TextUnitType) -> TextUnit {
        match unit {
            TextUnitType::Unspecified => TextUnit::UNSPECIFIED,
            _ => TextUnit { value, unit },
        }
    }

    /// `value` sp, as `value.sp`.
    pub const fn sp(value: f32) -> TextUnit {
        TextUnit { value, unit: TextUnitType::Sp }
    }

    /// `value` em, as `value.em`.
    pub const fn em(value: f32) -> TextUnit {
        TextUnit { value, unit: TextUnitType::Em }
    }

    /// The value, in its type's unit (NaN when unspecified).
    pub const fn value(self) -> f32 {
        self.value
    }

    /// Its type, as Compose's `type`.
    pub const fn unit_type(self) -> TextUnitType {
        self.unit
    }

    pub const fn is_sp(self) -> bool {
        matches!(self.unit, TextUnitType::Sp)
    }

    pub const fn is_em(self) -> bool {
        matches!(self.unit, TextUnitType::Em)
    }

    pub const fn is_specified(self) -> bool {
        !self.is_unspecified()
    }

    pub const fn is_unspecified(self) -> bool {
        matches!(self.unit, TextUnitType::Unspecified)
    }

    /// `self` if specified, else `other`, as `takeOrElse`.
    pub const fn or(self, other: TextUnit) -> TextUnit {
        if self.is_specified() { self } else { other }
    }

    /// Between `start` and `stop`, as Compose's `lerp(TextUnit, TextUnit, Float)`: both must be
    /// specified and of the same type.
    pub fn lerp(start: TextUnit, stop: TextUnit, fraction: f32) -> TextUnit {
        check_arithmetic_pair(start, stop);
        TextUnit { value: lerp_f32(start.value, stop.value, fraction), unit: start.unit }
    }

    fn checked(self) -> TextUnit {
        assert!(self.is_specified(), "Cannot perform operation for Unspecified type.");
        self
    }
}

/// Between `start` and `stop`, as Compose's `lerp(Float, Float, Float)`.
pub(crate) fn lerp_f32(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

fn check_arithmetic_pair(a: TextUnit, b: TextUnit) {
    assert!(a.is_specified() && b.is_specified(), "Cannot perform operation for Unspecified type.");
    assert!(a.unit == b.unit, "Cannot perform operation for {:?} and {:?}", a.unit, b.unit);
}

impl Default for TextUnit {
    fn default() -> Self {
        TextUnit::UNSPECIFIED
    }
}

impl PartialEq for TextUnit {
    fn eq(&self, other: &Self) -> bool {
        self.unit == other.unit && self.value.to_bits() == other.value.to_bits()
    }
}

impl Eq for TextUnit {}

impl std::hash::Hash for TextUnit {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.unit.hash(state);
        self.value.to_bits().hash(state);
    }
}

/// Compares two specified units of the same type, as Compose's `compareTo`: anything else
/// panics, as Compose's throws.
impl PartialOrd for TextUnit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        check_arithmetic_pair(*self, *other);
        self.value.partial_cmp(&other.value)
    }
}

impl Neg for TextUnit {
    type Output = TextUnit;
    fn neg(self) -> TextUnit {
        let unit = self.checked();
        TextUnit { value: -unit.value, ..unit }
    }
}

impl Mul<f32> for TextUnit {
    type Output = TextUnit;
    fn mul(self, other: f32) -> TextUnit {
        let unit = self.checked();
        TextUnit { value: unit.value * other, ..unit }
    }
}

impl Mul<TextUnit> for f32 {
    type Output = TextUnit;
    fn mul(self, other: TextUnit) -> TextUnit {
        let unit = other.checked();
        TextUnit { value: self * unit.value, ..unit }
    }
}

impl Div<f32> for TextUnit {
    type Output = TextUnit;
    fn div(self, other: f32) -> TextUnit {
        let unit = self.checked();
        TextUnit { value: unit.value / other, ..unit }
    }
}

/// Compose's `Int` and `Double` overloads of `times` and `div`: computed in `f32`, as
/// Compose's (a `Double` is converted after the operation).
macro_rules! text_unit_arithmetic {
    ($($type:ty),*) => {
        $(
            impl Mul<$type> for TextUnit {
                type Output = TextUnit;
                fn mul(self, other: $type) -> TextUnit {
                    let unit = self.checked();
                    TextUnit { value: (unit.value as f64 * other as f64) as f32, ..unit }
                }
            }

            impl Mul<TextUnit> for $type {
                type Output = TextUnit;
                fn mul(self, other: TextUnit) -> TextUnit {
                    other * self
                }
            }

            impl Div<$type> for TextUnit {
                type Output = TextUnit;
                fn div(self, other: $type) -> TextUnit {
                    let unit = self.checked();
                    TextUnit { value: (unit.value as f64 / other as f64) as f32, ..unit }
                }
            }
        )*
    };
}

text_unit_arithmetic!(i32, f64);

impl fmt::Debug for TextUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.unit {
            TextUnitType::Unspecified => formatter.write_str("Unspecified"),
            TextUnitType::Sp => write!(formatter, "{:?}.sp", self.value),
            TextUnitType::Em => write!(formatter, "{:?}.em", self.value),
        }
    }
}

impl fmt::Display for TextUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl ComposeArg for TextUnit {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `16.dp()`, `14.sp()`, `1.5.em()`, as Kotlin's `16.dp`, `14.sp` and `1.5.em`.
pub trait UnitExt {
    fn dp(self) -> Dp;
    fn sp(self) -> TextUnit;
    fn em(self) -> TextUnit;
}

macro_rules! unit_ext {
    ($($type:ty),*) => {
        $(impl UnitExt for $type {
            fn dp(self) -> Dp {
                Dp(self as f32)
            }
            fn sp(self) -> TextUnit {
                TextUnit::sp(self as f32)
            }
            fn em(self) -> TextUnit {
                TextUnit::em(self as f32)
            }
        })*
    };
}

unit_ext!(i32, u32, f32, f64, usize);

/// Pixels per `Dp`, and the font scale applied to `sp`, as Compose's `Density`.
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

    /// The pixels for a measure in `sp`, as Compose's `TextUnit.toPx`. Panics for another
    /// type ("Only Sp can convert to Px", as Compose's): an `em` is relative to a font size.
    pub fn sp_to_px(&self, sp: TextUnit) -> f32 {
        assert!(sp.is_sp(), "Only Sp can convert to Px");
        sp.value() * self.font_scale * self.density
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

/// A floating-point position, as `Offset`: pointer positions and drawing. Equal when both
/// coordinates are, unspecified (NaN) included: `UNSPECIFIED` equals itself, as Compose's
/// packed value.
#[derive(Clone, Copy, Default, Debug)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl PartialEq for Offset {
    fn eq(&self, other: &Self) -> bool {
        same_value(self.x, other.x) && same_value(self.y, other.y)
    }
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

/// A rectangle in `Dp`, as Compose's `DpRect`: what a test's bounds assertions read
/// (`getUnclippedBoundsInRoot`).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct DpRect {
    pub left: Dp,
    pub top: Dp,
    pub right: Dp,
    pub bottom: Dp,
}

impl DpRect {
    pub const fn new(left: Dp, top: Dp, right: Dp, bottom: Dp) -> DpRect {
        DpRect { left, top, right, bottom }
    }

    pub fn width(&self) -> Dp {
        Dp(self.right.0 - self.left.0)
    }

    pub fn height(&self) -> Dp {
        Dp(self.bottom.0 - self.top.0)
    }

    pub fn size(&self) -> DpSize {
        DpSize::new(self.width(), self.height())
    }
}

/// An offset in `Dp`, as Compose's `DpOffset`: what a menu's `offset` takes.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct DpOffset {
    pub x: Dp,
    pub y: Dp,
}

impl DpOffset {
    pub const ZERO: DpOffset = DpOffset { x: Dp::ZERO, y: Dp::ZERO };
    /// No value, as `DpOffset.Unspecified`.
    pub const UNSPECIFIED: DpOffset = DpOffset { x: Dp::UNSPECIFIED, y: Dp::UNSPECIFIED };

    pub const fn new(x: Dp, y: Dp) -> DpOffset {
        DpOffset { x, y }
    }

    /// Whether it has a value, as `isSpecified`: both coordinates do.
    pub fn is_specified(self) -> bool {
        self.x.is_specified() && self.y.is_specified()
    }

    pub fn is_unspecified(self) -> bool {
        !self.is_specified()
    }

    /// Itself if specified, else `or_else()`, as `takeOrElse`.
    pub fn take_or_else(self, or_else: impl FnOnce() -> DpOffset) -> DpOffset {
        if self.is_specified() { self } else { or_else() }
    }

    /// From `start` to `stop` by `fraction`, as `lerp(DpOffset, DpOffset, Float)`.
    pub fn lerp(start: DpOffset, stop: DpOffset, fraction: f32) -> DpOffset {
        let lerp = |a: Dp, b: Dp| Dp(a.0 + (b.0 - a.0) * fraction);
        DpOffset::new(lerp(start.x, stop.x), lerp(start.y, stop.y))
    }
}

impl fmt::Debug for DpOffset {
    /// As Compose's `toString`: `(1.0.dp, 2.0.dp)`, or `DpOffset.Unspecified`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_specified() {
            write!(formatter, "({:?}.dp, {:?}.dp)", self.x.0, self.y.0)
        } else {
            formatter.write_str("DpOffset.Unspecified")
        }
    }
}

impl ComposeArg for DpOffset {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl Add for DpOffset {
    type Output = DpOffset;
    fn add(self, other: DpOffset) -> DpOffset {
        DpOffset::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for DpOffset {
    type Output = DpOffset;
    fn sub(self, other: DpOffset) -> DpOffset {
        DpOffset::new(self.x - other.x, self.y - other.y)
    }
}

impl From<(Dp, Dp)> for DpOffset {
    fn from((x, y): (Dp, Dp)) -> DpOffset {
        DpOffset::new(x, y)
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
