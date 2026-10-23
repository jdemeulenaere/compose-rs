//! Animated value types, as Compose's `TwoWayConverter`s to `AnimationVector`s: each converts
//! to and from up to four floats, which the specs animate one by one.

use compose_ui::{Color, Dp, IntOffset, Offset, Size};

/// A type that can be animated, as a `TwoWayConverter` (`Float.VectorConverter`, …): up to four
/// components, and a default visibility threshold for springs.
pub trait Animate: Clone + PartialEq + 'static {
    fn to_vector(&self) -> [f32; 4];
    fn from_vector(vector: [f32; 4]) -> Self;
    /// How close counts as there for a spring without a threshold of its own, as Compose's
    /// `VisibilityThreshold`s.
    fn visibility_threshold() -> f32 {
        crate::spec::Spring::DEFAULT_DISPLACEMENT_THRESHOLD
    }
    /// Whether `self` and `other` are the same value, as Kotlin's `equals` of the boxed value
    /// (NaN equals NaN): what writing an animated value compares. By default, their vectors'
    /// components; a type whose conversion is dear compares itself (a color, D420).
    fn same_value(&self, other: &Self) -> bool {
        self.to_vector()
            .iter()
            .zip(other.to_vector())
            .all(|(&a, b)| a == b || (a.is_nan() && b.is_nan()))
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

impl Animate for f32 {
    fn to_vector(&self) -> [f32; 4] {
        [*self, 0.0, 0.0, 0.0]
    }
    fn from_vector(vector: [f32; 4]) -> f32 {
        vector[0]
    }
}

impl Animate for i32 {
    fn to_vector(&self) -> [f32; 4] {
        [*self as f32, 0.0, 0.0, 0.0]
    }
    fn from_vector(vector: [f32; 4]) -> i32 {
        vector[0].round() as i32
    }
    fn visibility_threshold() -> f32 {
        1.0
    }
}

impl Animate for Dp {
    fn to_vector(&self) -> [f32; 4] {
        [self.0, 0.0, 0.0, 0.0]
    }
    fn from_vector(vector: [f32; 4]) -> Dp {
        Dp(vector[0])
    }
    /// As `Dp.VisibilityThreshold`.
    fn visibility_threshold() -> f32 {
        0.1
    }
}

impl Animate for Offset {
    fn to_vector(&self) -> [f32; 4] {
        [self.x, self.y, 0.0, 0.0]
    }
    fn from_vector(vector: [f32; 4]) -> Offset {
        Offset::new(vector[0], vector[1])
    }
    /// As `Offset.VisibilityThreshold` (half a pixel).
    fn visibility_threshold() -> f32 {
        0.5
    }
}

impl Animate for IntOffset {
    fn to_vector(&self) -> [f32; 4] {
        [self.x as f32, self.y as f32, 0.0, 0.0]
    }
    fn from_vector(vector: [f32; 4]) -> IntOffset {
        IntOffset::new(vector[0].round() as i32, vector[1].round() as i32)
    }
    fn visibility_threshold() -> f32 {
        1.0
    }
}

impl Animate for Size {
    fn to_vector(&self) -> [f32; 4] {
        [self.width, self.height, 0.0, 0.0]
    }
    fn from_vector(vector: [f32; 4]) -> Size {
        Size::new(vector[0], vector[1])
    }
    fn visibility_threshold() -> f32 {
        0.5
    }
}

/// Colors animate in the Oklab color space, with alpha, as Compose's `Color.VectorConverter`:
/// perceptually even, without the gray midpoint of an sRGB blend.
impl Animate for Color {
    /// As `Color.equals`: the packed colors (no conversion to Oklab).
    fn same_value(&self, other: &Color) -> bool {
        self == other
    }

    fn to_vector(&self) -> [f32; 4] {
        let linear = |channel: u8| {
            let value = f32::from(channel) / 255.0;
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        let (r, g, b) = (linear(self.red()), linear(self.green()), linear(self.blue()));
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        [
            f32::from(self.alpha()) / 255.0,
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ]
    }

    fn from_vector(vector: [f32; 4]) -> Color {
        let [alpha, lightness, a, b] = vector;
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
        let alpha = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        Color::from_argb(alpha << 24 | red << 16 | green << 8 | blue)
    }

    fn visibility_threshold() -> f32 {
        0.001
    }
}
