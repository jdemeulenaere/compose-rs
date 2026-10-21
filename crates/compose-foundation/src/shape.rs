//! Shapes, as Compose's `androidx.compose.foundation.shape`.

use compose_ui::{CornerSize, Dp, Shape};

/// Rounded corners of `radius`, as `RoundedCornerShape(size)`.
pub fn RoundedCornerShape(radius: Dp) -> Shape {
    Shape::Rounded([CornerSize::Dp(radius); 4])
}

/// Rounded corners in percent of the shorter side, as `RoundedCornerShape(percent)`.
pub fn RoundedCornerShapePercent(percent: f32) -> Shape {
    Shape::Rounded([CornerSize::Percent(percent); 4])
}

/// Each corner its own radius: top start, top end, bottom end, bottom start.
pub fn RoundedCornerShapeEach(
    top_start: Dp,
    top_end: Dp,
    bottom_end: Dp,
    bottom_start: Dp,
) -> Shape {
    Shape::Rounded([
        CornerSize::Dp(top_start),
        CornerSize::Dp(top_end),
        CornerSize::Dp(bottom_end),
        CornerSize::Dp(bottom_start),
    ])
}

/// A circle (a pill for a non-square box), as `CircleShape`.
pub const CIRCLE_SHAPE: Shape = Shape::Rounded([CornerSize::Percent(50.0); 4]);
