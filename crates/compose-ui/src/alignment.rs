//! Alignment, as Compose's `Alignment`: a bias from -1 (start, top) to 1 (end, bottom).

use compose_runtime::ComposeArg;

use crate::unit::{IntOffset, IntSize};

/// The direction of a layout, as `LayoutDirection`. Only left-to-right is laid out for now.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum LayoutDirection {
    #[default]
    Ltr,
    Rtl,
}

/// A 2D alignment, as `Alignment` (a `BiasAlignment`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Alignment {
    pub horizontal_bias: f32,
    pub vertical_bias: f32,
}

/// A horizontal alignment, as `Alignment.Horizontal`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HorizontalAlignment {
    pub bias: f32,
}

/// A vertical alignment, as `Alignment.Vertical`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct VerticalAlignment {
    pub bias: f32,
}

impl Alignment {
    pub const TOP_START: Alignment = Alignment::new(-1.0, -1.0);
    pub const TOP_CENTER: Alignment = Alignment::new(0.0, -1.0);
    pub const TOP_END: Alignment = Alignment::new(1.0, -1.0);
    pub const CENTER_START: Alignment = Alignment::new(-1.0, 0.0);
    pub const CENTER: Alignment = Alignment::new(0.0, 0.0);
    pub const CENTER_END: Alignment = Alignment::new(1.0, 0.0);
    pub const BOTTOM_START: Alignment = Alignment::new(-1.0, 1.0);
    pub const BOTTOM_CENTER: Alignment = Alignment::new(0.0, 1.0);
    pub const BOTTOM_END: Alignment = Alignment::new(1.0, 1.0);

    pub const TOP: VerticalAlignment = VerticalAlignment { bias: -1.0 };
    pub const CENTER_VERTICALLY: VerticalAlignment = VerticalAlignment { bias: 0.0 };
    pub const BOTTOM: VerticalAlignment = VerticalAlignment { bias: 1.0 };

    pub const START: HorizontalAlignment = HorizontalAlignment { bias: -1.0 };
    pub const CENTER_HORIZONTALLY: HorizontalAlignment = HorizontalAlignment { bias: 0.0 };
    pub const END: HorizontalAlignment = HorizontalAlignment { bias: 1.0 };

    pub const fn new(horizontal_bias: f32, vertical_bias: f32) -> Alignment {
        Alignment { horizontal_bias, vertical_bias }
    }

    /// The position of a box of `size` in `space`, as `Alignment.align`.
    pub fn align(&self, size: IntSize, space: IntSize, direction: LayoutDirection) -> IntOffset {
        IntOffset::new(
            HorizontalAlignment { bias: self.horizontal_bias }.align(
                size.width,
                space.width,
                direction,
            ),
            VerticalAlignment { bias: self.vertical_bias }.align(size.height, space.height),
        )
    }
}

fn biased(size: i32, space: i32, bias: f32) -> i32 {
    let center = (space - size) as f32 / 2.0;
    (center * (1.0 + bias)).round() as i32
}

impl HorizontalAlignment {
    pub fn align(&self, size: i32, space: i32, direction: LayoutDirection) -> i32 {
        let bias = if direction == LayoutDirection::Ltr { self.bias } else { -self.bias };
        biased(size, space, bias)
    }
}

impl VerticalAlignment {
    pub fn align(&self, size: i32, space: i32) -> i32 {
        biased(size, space, self.bias)
    }
}

macro_rules! compose_arg_by_eq {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

compose_arg_by_eq!(Alignment, HorizontalAlignment, VerticalAlignment, LayoutDirection);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_as_bias_alignment() {
        let size = IntSize::new(10, 10);
        let space = IntSize::new(100, 51);
        assert_eq!(
            Alignment::TOP_START.align(size, space, LayoutDirection::Ltr),
            IntOffset::new(0, 0)
        );
        assert_eq!(
            Alignment::CENTER.align(size, space, LayoutDirection::Ltr),
            IntOffset::new(45, 21)
        );
        assert_eq!(
            Alignment::BOTTOM_END.align(size, space, LayoutDirection::Ltr),
            IntOffset::new(90, 41)
        );
        assert_eq!(Alignment::END.align(10, 100, LayoutDirection::Rtl), 0);
    }
}
