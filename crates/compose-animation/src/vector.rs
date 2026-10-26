//! Animated value types, as Compose's `TwoWayConverter`s to `AnimationVector`s: each converts
//! to and from one to four floats, which the specs animate one by one.

use std::ops::{Index, IndexMut};

use compose_ui::{Color, Dp, DpOffset, IntOffset, IntSize, Offset, Rect, Size};

/// One to four floats an animation moves, as Compose's `AnimationVector1D` to
/// `AnimationVector4D`: an animated value's components, its velocity, a spring's thresholds.
/// `AnimationVector::from(1.0)`, `AnimationVector::from([1.0, 2.0])` make one.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct AnimationVector {
    /// The components; those past `size` are 0.
    values: [f32; 4],
    size: u8,
}

impl AnimationVector {
    /// `size` zeros (1 to 4), as `newInstance`.
    pub const fn zero(size: usize) -> AnimationVector {
        assert!(size >= 1 && size <= 4, "an AnimationVector has 1 to 4 components");
        AnimationVector { values: [0.0; 4], size: size as u8 }
    }

    /// `size` copies of `value`.
    pub const fn filled(size: usize, value: f32) -> AnimationVector {
        let mut vector = AnimationVector::zero(size);
        let mut index = 0;
        while index < size {
            vector.values[index] = value;
            index += 1;
        }
        vector
    }

    /// How many components it has, as `size`.
    pub const fn size(&self) -> usize {
        self.size as usize
    }

    /// Zeros of the same size, as `newInstance`.
    pub const fn new_instance(&self) -> AnimationVector {
        AnimationVector::zero(self.size())
    }

    /// Sets every component to 0, as `reset`.
    pub fn reset(&mut self) {
        self.values = [0.0; 4];
    }

    /// The components.
    pub fn as_slice(&self) -> &[f32] {
        &self.values[..self.size()]
    }

    /// The components, to change.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        let size = self.size();
        &mut self.values[..size]
    }

    /// The components, padded with zeros to four.
    pub const fn to_array(&self) -> [f32; 4] {
        self.values
    }

    /// Each component through `f`.
    pub fn map(mut self, mut f: impl FnMut(usize, f32) -> f32) -> AnimationVector {
        for index in 0..self.size() {
            self.values[index] = f(index, self.values[index]);
        }
        self
    }
}

impl Index<usize> for AnimationVector {
    type Output = f32;
    fn index(&self, index: usize) -> &f32 {
        &self.as_slice()[index]
    }
}

impl IndexMut<usize> for AnimationVector {
    fn index_mut(&mut self, index: usize) -> &mut f32 {
        &mut self.as_mut_slice()[index]
    }
}

impl std::fmt::Debug for AnimationVector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "AnimationVector{:?}", self.as_slice())
    }
}

impl From<f32> for AnimationVector {
    /// A vector of one component, as `AnimationVector(v1)`.
    fn from(value: f32) -> AnimationVector {
        AnimationVector::from([value])
    }
}

impl<const N: usize> From<[f32; N]> for AnimationVector {
    /// A vector of these components (1 to 4), as `AnimationVector(v1, …)`.
    fn from(components: [f32; N]) -> AnimationVector {
        let mut vector = AnimationVector::zero(N);
        vector.values[..N].copy_from_slice(&components);
        vector
    }
}

/// A type that can be animated, as a `TwoWayConverter` (`Float.VectorConverter`, …): one to four
/// components.
pub trait Animate: Clone + PartialEq + 'static {
    fn to_vector(&self) -> AnimationVector;
    fn from_vector(vector: AnimationVector) -> Self;
    /// The threshold of the springs Compose picks for this type itself (a transition's
    /// interruption spring), per component, as `defaultVisibilityThresholdFor`: 0.01 when it has
    /// none, as a spring without one.
    fn visibility_threshold() -> f32 {
        crate::spec::Spring::DEFAULT_DISPLACEMENT_THRESHOLD
    }
    /// Whether `self` and `other` are the same value, as Kotlin's `equals` of the boxed value
    /// (NaN equals NaN): what writing an animated value compares. By default, their vectors'
    /// components; a type whose conversion is dear compares itself (a color, D420).
    fn same_value(&self, other: &Self) -> bool {
        let (a, b) = (self.to_vector(), other.to_vector());
        a.as_slice().iter().zip(b.as_slice()).all(|(&a, &b)| a == b || (a.is_nan() && b.is_nan()))
    }
}

/// Whether `a` and `b` are the same value, as Kotlin compares boxed floats: NaN equals NaN.
pub(crate) fn same_value<T: Animate>(a: &T, b: &T) -> bool {
    a.same_value(b)
}

/// The policy of an animated value's state: writing the same value (NaN over NaN included)
/// invalidates nothing.
pub(crate) struct SameValuePolicy;

impl<T: Animate> compose_runtime::MutationPolicy<T> for SameValuePolicy {
    fn equivalent(a: &T, b: &T) -> bool {
        same_value(a, b)
    }
}

/// Rounds half up, as Kotlin's `roundToInt` (`Math.round`): -0.5 is 0.
fn round_to_int(value: f32) -> i32 {
    (f64::from(value) + 0.5).floor() as i32
}

/// The visibility threshold of values in pixels (`Offset`, `Size`, `Rect`), as Compose's.
const PX_VISIBILITY_THRESHOLD: f32 = 1.0;
/// The visibility threshold of values in `Dp`, as Compose's.
const DP_VISIBILITY_THRESHOLD: f32 = 0.4;

impl Animate for f32 {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from(*self)
    }
    fn from_vector(vector: AnimationVector) -> f32 {
        vector[0]
    }
}

/// A vector animates as itself, as Compose's `TwoWayConverter({ it }, { it })`.
impl Animate for AnimationVector {
    fn to_vector(&self) -> AnimationVector {
        *self
    }
    fn from_vector(vector: AnimationVector) -> AnimationVector {
        vector
    }
}

/// Truncated toward zero, as `Int.VectorConverter` (`toInt`).
impl Animate for i32 {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from(*self as f32)
    }
    fn from_vector(vector: AnimationVector) -> i32 {
        vector[0] as i32
    }
    fn visibility_threshold() -> f32 {
        1.0
    }
}

impl Animate for Dp {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from(self.0)
    }
    fn from_vector(vector: AnimationVector) -> Dp {
        Dp(vector[0])
    }
    fn visibility_threshold() -> f32 {
        DP_VISIBILITY_THRESHOLD
    }
}

impl Animate for DpOffset {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from([self.x.0, self.y.0])
    }
    fn from_vector(vector: AnimationVector) -> DpOffset {
        DpOffset::new(Dp(vector[0]), Dp(vector[1]))
    }
    fn visibility_threshold() -> f32 {
        DP_VISIBILITY_THRESHOLD
    }
}

impl Animate for Offset {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from([self.x, self.y])
    }
    fn from_vector(vector: AnimationVector) -> Offset {
        Offset::new(vector[0], vector[1])
    }
    fn visibility_threshold() -> f32 {
        PX_VISIBILITY_THRESHOLD
    }
}

/// Rounded half up, as `IntOffset.VectorConverter`.
impl Animate for IntOffset {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from([self.x as f32, self.y as f32])
    }
    fn from_vector(vector: AnimationVector) -> IntOffset {
        IntOffset::new(round_to_int(vector[0]), round_to_int(vector[1]))
    }
    fn visibility_threshold() -> f32 {
        1.0
    }
}

/// Rounded half up and never negative, as `IntSize.VectorConverter`.
impl Animate for IntSize {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from([self.width as f32, self.height as f32])
    }
    fn from_vector(vector: AnimationVector) -> IntSize {
        IntSize::new(round_to_int(vector[0]).max(0), round_to_int(vector[1]).max(0))
    }
    fn visibility_threshold() -> f32 {
        1.0
    }
}

impl Animate for Size {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from([self.width, self.height])
    }
    fn from_vector(vector: AnimationVector) -> Size {
        Size::new(vector[0], vector[1])
    }
    fn visibility_threshold() -> f32 {
        PX_VISIBILITY_THRESHOLD
    }
}

impl Animate for Rect {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from([self.left, self.top, self.right, self.bottom])
    }
    fn from_vector(vector: AnimationVector) -> Rect {
        Rect::new(vector[0], vector[1], vector[2], vector[3])
    }
    fn visibility_threshold() -> f32 {
        PX_VISIBILITY_THRESHOLD
    }
}

/// Colors animate in the Oklab color space, with alpha, as Compose's `Color.VectorConverter`:
/// perceptually even, without the gray midpoint of an sRGB blend. The vector is (alpha, L, a,
/// b); converting back clamps it to Oklab's range first (alpha and L to 0..1, a and b to
/// ±0.5), as Compose's, so an overshooting spring stays a color.
impl Animate for Color {
    /// As `Color.equals`: the packed colors (no conversion to Oklab).
    fn same_value(&self, other: &Color) -> bool {
        self == other
    }

    fn to_vector(&self) -> AnimationVector {
        let linear = |channel: u8| {
            let value = f32::from(channel) / 255.0;
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        let (r, g, b) = (linear(self.red()), linear(self.green()), linear(self.blue()));
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        AnimationVector::from([
            f32::from(self.alpha()) / 255.0,
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ])
    }

    fn from_vector(vector: AnimationVector) -> Color {
        let alpha = vector[0].clamp(0.0, 1.0);
        let lightness = vector[1].clamp(0.0, 1.0);
        let a = vector[2].clamp(-0.5, 0.5);
        let b = vector[3].clamp(-0.5, 0.5);
        let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
        let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
        let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
        let encode = |value: f32| {
            let value = value.clamp(0.0, 1.0);
            let encoded = if value <= 0.003_130_8 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            };
            (encoded * 255.0).round().clamp(0.0, 255.0) as u32
        };
        let red = encode(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s);
        let green = encode(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s);
        let blue = encode(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s);
        let alpha = (alpha * 255.0).round() as u32;
        Color::from_argb(alpha << 24 | red << 16 | green << 8 | blue)
    }
}

/// The visibility threshold Compose gives a type's default springs (`animate*AsState`,
/// `Transition.animate*`), as its `VisibilityThreshold` extension values:
/// `Dp::VISIBILITY_THRESHOLD` is 0.4 dp.
pub trait VisibilityThreshold: Sized {
    const VISIBILITY_THRESHOLD: Self;
}

impl VisibilityThreshold for i32 {
    const VISIBILITY_THRESHOLD: i32 = 1;
}

impl VisibilityThreshold for IntOffset {
    const VISIBILITY_THRESHOLD: IntOffset = IntOffset::new(1, 1);
}

impl VisibilityThreshold for IntSize {
    const VISIBILITY_THRESHOLD: IntSize = IntSize::new(1, 1);
}

impl VisibilityThreshold for Offset {
    const VISIBILITY_THRESHOLD: Offset =
        Offset::new(PX_VISIBILITY_THRESHOLD, PX_VISIBILITY_THRESHOLD);
}

impl VisibilityThreshold for Size {
    const VISIBILITY_THRESHOLD: Size = Size::new(PX_VISIBILITY_THRESHOLD, PX_VISIBILITY_THRESHOLD);
}

impl VisibilityThreshold for Rect {
    const VISIBILITY_THRESHOLD: Rect = Rect::new(
        PX_VISIBILITY_THRESHOLD,
        PX_VISIBILITY_THRESHOLD,
        PX_VISIBILITY_THRESHOLD,
        PX_VISIBILITY_THRESHOLD,
    );
}

impl VisibilityThreshold for Dp {
    const VISIBILITY_THRESHOLD: Dp = Dp(DP_VISIBILITY_THRESHOLD);
}

impl VisibilityThreshold for DpOffset {
    const VISIBILITY_THRESHOLD: DpOffset =
        DpOffset::new(Dp(DP_VISIBILITY_THRESHOLD), Dp(DP_VISIBILITY_THRESHOLD));
}

/// The default spring of `T`'s `animate*AsState` and `Transition.animate*`: `spring()` with
/// `T`'s visibility threshold.
#[doc(hidden)]
pub fn default_spring<T: Animate + VisibilityThreshold>() -> crate::spec::AnimationSpec {
    crate::spec::AnimationSpec::Spring(
        crate::spec::spring().visibility_threshold(T::VISIBILITY_THRESHOLD),
    )
}
