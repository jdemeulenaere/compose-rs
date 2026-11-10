//! Alignment, as Compose's `Alignment`: a bias from -1 (start, top) to 1 (end, bottom), and
//! custom horizontal/vertical alignment policies (`HorizontalAlignment::custom`,
//! `VerticalAlignment::custom`).

use std::any::Any;
use std::fmt;
use std::rc::Rc;

use compose_runtime::ComposeArg;

use crate::unit::{IntOffset, IntSize};

/// The direction of a layout, as `LayoutDirection`: right-to-left mirrors what's placed
/// relatively (`MeasureScope::place_relative`), start and end, and rows.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum LayoutDirection {
    #[default]
    Ltr,
    Rtl,
}

/// The layout direction of what's composed, as Compose's `LocalLayoutDirection`: each layout
/// node takes it where it's composed; the host provides the window's.
pub static LOCAL_LAYOUT_DIRECTION: compose_runtime::CompositionLocal<LayoutDirection> =
    compose_runtime::CompositionLocal::new_static("LocalLayoutDirection", LayoutDirection::default);

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
    crate::unit::round_to_int(center * (1.0 + bias))
}

impl HorizontalAlignment {
    /// A custom horizontal alignment closure, as `object : Alignment.Horizontal`: takes `(size,
    /// space, direction)` in pixels and returns the child's x offset inside `space`.
    pub fn custom(
        align: impl Fn(i32, i32, LayoutDirection) -> i32 + 'static,
    ) -> AnyHorizontalAlignment {
        AnyHorizontalAlignment::Custom(Rc::new(FnHorizontalAlignment(align)))
    }

    /// A custom horizontal alignment implementing [`HorizontalAlignmentPolicy`], allowing
    /// structural equality (`same`) across recompositions.
    pub fn custom_policy(policy: impl HorizontalAlignmentPolicy) -> AnyHorizontalAlignment {
        AnyHorizontalAlignment::Custom(Rc::new(policy))
    }

    pub fn align(&self, size: i32, space: i32, direction: LayoutDirection) -> i32 {
        let bias = if direction == LayoutDirection::Ltr { self.bias } else { -self.bias };
        biased(size, space, bias)
    }
}

impl VerticalAlignment {
    /// A custom vertical alignment closure, as `object : Alignment.Vertical`: takes `(size,
    /// space)` in pixels and returns the child's y offset inside `space`.
    pub fn custom(align: impl Fn(i32, i32) -> i32 + 'static) -> AnyVerticalAlignment {
        AnyVerticalAlignment::Custom(Rc::new(FnVerticalAlignment(align)))
    }

    /// A custom vertical alignment implementing [`VerticalAlignmentPolicy`], allowing
    /// structural equality (`same`) across recompositions.
    pub fn custom_policy(policy: impl VerticalAlignmentPolicy) -> AnyVerticalAlignment {
        AnyVerticalAlignment::Custom(Rc::new(policy))
    }

    pub fn align(&self, size: i32, space: i32) -> i32 {
        biased(size, space, self.bias)
    }
}

/// How a custom horizontal alignment positions a box of `size` inside `space`, as
/// Compose's `Alignment.Horizontal`.
pub trait HorizontalAlignmentPolicy: Any {
    fn align(&self, size: i32, space: i32, direction: LayoutDirection) -> i32;

    /// Whether `other` aligns identically, so layouts needn't remeasure. Defaults to true only for
    /// equal zero-sized types; policies with fields compare `other.downcast_ref::<Self>() == Some(self)`.
    fn same(&self, other: &dyn Any) -> bool {
        Any::type_id(self) == other.type_id() && std::mem::size_of_val(self) == 0
    }
}

/// How a custom vertical alignment positions a box of `size` inside `space`, as
/// Compose's `Alignment.Vertical`.
pub trait VerticalAlignmentPolicy: Any {
    fn align(&self, size: i32, space: i32) -> i32;

    /// Whether `other` aligns identically, so layouts needn't remeasure.
    fn same(&self, other: &dyn Any) -> bool {
        Any::type_id(self) == other.type_id() && std::mem::size_of_val(self) == 0
    }
}

struct FnHorizontalAlignment<F>(F);

impl<F: Fn(i32, i32, LayoutDirection) -> i32 + 'static> HorizontalAlignmentPolicy
    for FnHorizontalAlignment<F>
{
    fn align(&self, size: i32, space: i32, direction: LayoutDirection) -> i32 {
        (self.0)(size, space, direction)
    }
}

struct FnVerticalAlignment<F>(F);

impl<F: Fn(i32, i32) -> i32 + 'static> VerticalAlignmentPolicy for FnVerticalAlignment<F> {
    fn align(&self, size: i32, space: i32) -> i32 {
        (self.0)(size, space)
    }
}

/// Either a bias [`HorizontalAlignment`] or a custom [`HorizontalAlignmentPolicy`]: accepted by
/// `Column`, `FlowColumn`, and `Modifier.align` so built-in bias alignments stay `Copy` (D2322).
#[derive(Clone)]
pub enum AnyHorizontalAlignment {
    Bias(HorizontalAlignment),
    Custom(Rc<dyn HorizontalAlignmentPolicy>),
}

impl AnyHorizontalAlignment {
    /// Returns the underlying bias alignment if this is a bias alignment.
    pub fn as_bias(&self) -> Option<HorizontalAlignment> {
        match self {
            AnyHorizontalAlignment::Bias(alignment) => Some(*alignment),
            AnyHorizontalAlignment::Custom(_) => None,
        }
    }

    pub fn align(&self, size: i32, space: i32, direction: LayoutDirection) -> i32 {
        match self {
            AnyHorizontalAlignment::Bias(alignment) => alignment.align(size, space, direction),
            AnyHorizontalAlignment::Custom(policy) => policy.align(size, space, direction),
        }
    }
}

impl From<HorizontalAlignment> for AnyHorizontalAlignment {
    fn from(alignment: HorizontalAlignment) -> AnyHorizontalAlignment {
        AnyHorizontalAlignment::Bias(alignment)
    }
}

impl PartialEq for AnyHorizontalAlignment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AnyHorizontalAlignment::Bias(a), AnyHorizontalAlignment::Bias(b)) => a == b,
            (AnyHorizontalAlignment::Custom(a), AnyHorizontalAlignment::Custom(b)) => {
                std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b)) || a.as_ref().same(&**b as &dyn Any)
            }
            _ => false,
        }
    }
}

impl fmt::Debug for AnyHorizontalAlignment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnyHorizontalAlignment::Bias(alignment) => alignment.fmt(formatter),
            AnyHorizontalAlignment::Custom(_) => formatter.write_str("HorizontalAlignment::custom"),
        }
    }
}

/// Either a bias [`VerticalAlignment`] or a custom [`VerticalAlignmentPolicy`]: accepted by
/// `Row`, `FlowRow`, and `Modifier.align` so built-in bias alignments stay `Copy` (D2322).
#[derive(Clone)]
pub enum AnyVerticalAlignment {
    Bias(VerticalAlignment),
    Custom(Rc<dyn VerticalAlignmentPolicy>),
}

impl AnyVerticalAlignment {
    /// Returns the underlying bias alignment if this is a bias alignment.
    pub fn as_bias(&self) -> Option<VerticalAlignment> {
        match self {
            AnyVerticalAlignment::Bias(alignment) => Some(*alignment),
            AnyVerticalAlignment::Custom(_) => None,
        }
    }

    pub fn align(&self, size: i32, space: i32) -> i32 {
        match self {
            AnyVerticalAlignment::Bias(alignment) => alignment.align(size, space),
            AnyVerticalAlignment::Custom(policy) => policy.align(size, space),
        }
    }
}

impl From<VerticalAlignment> for AnyVerticalAlignment {
    fn from(alignment: VerticalAlignment) -> AnyVerticalAlignment {
        AnyVerticalAlignment::Bias(alignment)
    }
}

impl PartialEq for AnyVerticalAlignment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AnyVerticalAlignment::Bias(a), AnyVerticalAlignment::Bias(b)) => a == b,
            (AnyVerticalAlignment::Custom(a), AnyVerticalAlignment::Custom(b)) => {
                std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b)) || a.as_ref().same(&**b as &dyn Any)
            }
            _ => false,
        }
    }
}

impl fmt::Debug for AnyVerticalAlignment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnyVerticalAlignment::Bias(alignment) => alignment.fmt(formatter),
            AnyVerticalAlignment::Custom(_) => formatter.write_str("VerticalAlignment::custom"),
        }
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

compose_arg_by_eq!(
    Alignment,
    HorizontalAlignment,
    VerticalAlignment,
    AnyHorizontalAlignment,
    AnyVerticalAlignment,
    LayoutDirection
);

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
