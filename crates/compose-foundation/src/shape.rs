//! Shapes, as Compose's `androidx.compose.foundation.shape`: the corner-based shapes, rounded
//! or cut, relative to the layout direction or absolute, and `GenericShape`.

use compose_ui::{CustomShape, Density, LayoutDirection, Outline, Path, Shape, Size};

pub use compose_ui::{CornerSize, CornerSizes};

/// What the corner-based shapes take, as `RoundedCornerShape`'s overloads (compose-rs's earlier
/// name of [`CornerSizes`]).
pub use compose_ui::CornerSizes as RoundedCorners;

/// Rounded corners, as `RoundedCornerShape`'s overloads: `RoundedCornerShape(8.dp())` for every
/// corner, `RoundedCornerShape(4.0)` in pixels, `RoundedCornerShape(50)` in percent of the
/// shorter side, a `CornerSize`, or four (`(top_start, top_end, bottom_end, bottom_start)`, each
/// a `Dp`, pixels, a percent or a `CornerSize`). The start corners are the right ones right to
/// left.
pub fn RoundedCornerShape(corners: impl CornerSizes) -> Shape {
    Shape::Rounded(corners.corner_sizes())
}

/// Cut corners, as `CutCornerShape`'s overloads (taking what `RoundedCornerShape` takes).
pub fn CutCornerShape(corners: impl CornerSizes) -> Shape {
    Shape::Cut(corners.corner_sizes())
}

/// Rounded corners that don't follow the layout direction, as `AbsoluteRoundedCornerShape`: four
/// are `(top_left, top_right, bottom_right, bottom_left)`.
pub fn AbsoluteRoundedCornerShape(corners: impl CornerSizes) -> Shape {
    Shape::AbsoluteRounded(corners.corner_sizes())
}

/// Cut corners that don't follow the layout direction, as `AbsoluteCutCornerShape`.
pub fn AbsoluteCutCornerShape(corners: impl CornerSizes) -> Shape {
    Shape::AbsoluteCut(corners.corner_sizes())
}

/// A circle (a pill for a non-square box), as `CircleShape`.
pub const CIRCLE_SHAPE: Shape = Shape::Rounded([CornerSize::Percent(50.0); 4]);

/// No corner, as `ZeroCornerSize`.
pub const ZERO_CORNER_SIZE: CornerSize = CornerSize::ZERO;

/// A shape `builder` draws into a path, given the box's size and the layout direction, as
/// `GenericShape { size, layoutDirection -> … }`: its outline is that path. Each call is a new
/// shape (Compose's compares by its lambda): remember it to keep it the same.
pub fn GenericShape(builder: impl Fn(&Path, Size, LayoutDirection) + 'static) -> Shape {
    Shape::custom(GenericShapeOutline(Box::new(builder)))
}

#[allow(clippy::type_complexity)]
struct GenericShapeOutline(Box<dyn Fn(&Path, Size, LayoutDirection)>);

impl CustomShape for GenericShapeOutline {
    fn create_outline(
        &self,
        size: Size,
        layout_direction: LayoutDirection,
        _density: &Density,
    ) -> Outline {
        let path = Path::new();
        (self.0)(&path, size, layout_direction);
        Outline::generic(&path)
    }

    /// The same shape only (Compose's compares its lambda, by identity).
    fn same(&self, other: &dyn CustomShape) -> bool {
        other.downcast_ref::<GenericShapeOutline>().is_some_and(|other| std::ptr::eq(self, other))
    }
}
