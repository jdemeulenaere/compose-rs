//! Shapes, as Compose's `androidx.compose.foundation.shape`.

use compose_ui::{CornerSize, Dp, Shape};

/// Rounded corners, as `RoundedCornerShape`'s overloads: `RoundedCornerShape(8.dp())` for every
/// corner, `RoundedCornerShape(CornerSize::Percent(50.0))` in percent of the shorter side, and
/// `RoundedCornerShape((top_start, top_end, bottom_end, bottom_start))` in `Dp` or
/// `CornerSize`s, each corner its own.
pub fn RoundedCornerShape(corners: impl RoundedCorners) -> Shape {
    Shape::Rounded(corners.corner_sizes())
}

/// What `RoundedCornerShape` takes: one size for every corner, or four (top start, top end,
/// bottom end, bottom start).
pub trait RoundedCorners {
    fn corner_sizes(self) -> [CornerSize; 4];
}

impl RoundedCorners for Dp {
    fn corner_sizes(self) -> [CornerSize; 4] {
        [CornerSize::Dp(self); 4]
    }
}

impl RoundedCorners for CornerSize {
    fn corner_sizes(self) -> [CornerSize; 4] {
        [self; 4]
    }
}

impl RoundedCorners for (Dp, Dp, Dp, Dp) {
    fn corner_sizes(self) -> [CornerSize; 4] {
        [
            CornerSize::Dp(self.0),
            CornerSize::Dp(self.1),
            CornerSize::Dp(self.2),
            CornerSize::Dp(self.3),
        ]
    }
}

impl RoundedCorners for (CornerSize, CornerSize, CornerSize, CornerSize) {
    fn corner_sizes(self) -> [CornerSize; 4] {
        [self.0, self.1, self.2, self.3]
    }
}

/// A circle (a pill for a non-square box), as `CircleShape`.
pub const CIRCLE_SHAPE: Shape = Shape::Rounded([CornerSize::Percent(50.0); 4]);
