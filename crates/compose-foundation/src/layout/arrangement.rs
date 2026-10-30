//! `Arrangement`, as Compose's: how a `Row` or `Column` places its children along its main axis.
//!
//! Compose's `Arrangement.Horizontal`, `Arrangement.Vertical` and
//! `Arrangement.HorizontalOrVertical` are [`HorizontalArrangement`], [`VerticalArrangement`] and
//! [`HorizontalOrVerticalArrangement`] here (the last converts to either), so a `Row` can't
//! take `Arrangement::TOP`. [`Arrangement`] is the namespace of the built-in ones, with Compose's
//! names: `Arrangement::START`, `Arrangement::TOP`, `Arrangement::SPACE_BETWEEN`,
//! `Arrangement::spaced_by(8.dp())`, `Arrangement::aligned(Alignment::END)`. A custom
//! arrangement implements [`HorizontalArrangementPolicy`] or [`VerticalArrangementPolicy`] (as
//! Compose's interfaces) and is wrapped with `HorizontalArrangement::custom` or
//! `VerticalArrangement::custom` (D262).

use std::any::Any;
use std::rc::Rc;

use compose_runtime::ComposeArg;
use compose_ui::{Density, Dp, HorizontalAlignment, LayoutDirection, VerticalAlignment};

/// How a custom horizontal arrangement places children, as Compose's `Arrangement.Horizontal`.
pub trait HorizontalArrangementPolicy: Any {
    /// The space between adjacent children, as `Arrangement.Horizontal.spacing`.
    fn spacing(&self) -> Dp {
        Dp(0.0)
    }

    /// Writes the children's positions from the start of `total_size` pixels into
    /// `out_positions`, given their `sizes`, as `Arrangement.Horizontal.arrange`. In right to
    /// left, positions are still from the left (Compose's `outPositions` are too).
    fn arrange(
        &self,
        density: &Density,
        total_size: i32,
        sizes: &[i32],
        layout_direction: LayoutDirection,
        out_positions: &mut [i32],
    );

    /// Whether `other` arranges as this one does, so a layout given it in a new pass needn't
    /// measure again (Compose compares arrangements with `equals`). The default: the same type,
    /// zero-sized (nothing to differ by). A policy with data compares it:
    /// `other.downcast_ref::<Self>() == Some(self)`.
    fn same(&self, other: &dyn Any) -> bool {
        Any::type_id(self) == other.type_id() && std::mem::size_of_val(self) == 0
    }
}

/// How a custom vertical arrangement places children, as Compose's `Arrangement.Vertical`.
pub trait VerticalArrangementPolicy: Any {
    /// The space between adjacent children, as `Arrangement.Vertical.spacing`.
    fn spacing(&self) -> Dp {
        Dp(0.0)
    }

    /// Writes the children's positions from the top, as `Arrangement.Vertical.arrange`.
    fn arrange(&self, density: &Density, total_size: i32, sizes: &[i32], out_positions: &mut [i32]);

    /// Whether `other` arranges as this one does, so a layout given it in a new pass needn't
    /// measure again (Compose compares arrangements with `equals`). The default: the same type,
    /// zero-sized (nothing to differ by). A policy with data compares it:
    /// `other.downcast_ref::<Self>() == Some(self)`.
    fn same(&self, other: &dyn Any) -> bool {
        Any::type_id(self) == other.type_id() && std::mem::size_of_val(self) == 0
    }
}

/// An arrangement along a main axis: the built-ins as data (compared by value), custom ones by
/// reference or their `same`.
#[derive(Clone)]
pub(crate) enum MainAxisArrangement {
    /// At the start (`Start`, `Top`).
    Start,
    /// At the end (`End`, `Bottom`).
    End,
    Center,
    SpaceEvenly,
    SpaceBetween,
    SpaceAround,
    /// A fixed space between children, the group placed by a bias when it doesn't fill the
    /// space (from the start without one), as `spacedBy`; `aligned` is a space of zero.
    SpacedBy(Dp, Option<f32>),
    Horizontal(Rc<dyn HorizontalArrangementPolicy>),
    Vertical(Rc<dyn VerticalArrangementPolicy>),
}

impl PartialEq for MainAxisArrangement {
    fn eq(&self, other: &Self) -> bool {
        use MainAxisArrangement::*;
        match (self, other) {
            (Start, Start)
            | (End, End)
            | (Center, Center)
            | (SpaceEvenly, SpaceEvenly)
            | (SpaceBetween, SpaceBetween)
            | (SpaceAround, SpaceAround) => true,
            (SpacedBy(a, b), SpacedBy(c, d)) => a == c && b == d,
            (Horizontal(a), Horizontal(b)) => {
                std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b)) || a.as_ref().same(&**b as &dyn Any)
            }
            (Vertical(a), Vertical(b)) => {
                std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b)) || a.as_ref().same(&**b as &dyn Any)
            }
            _ => false,
        }
    }
}

impl std::fmt::Debug for MainAxisArrangement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use MainAxisArrangement::*;
        match self {
            Start => formatter.write_str("Start"),
            End => formatter.write_str("End"),
            Center => formatter.write_str("Center"),
            SpaceEvenly => formatter.write_str("SpaceEvenly"),
            SpaceBetween => formatter.write_str("SpaceBetween"),
            SpaceAround => formatter.write_str("SpaceAround"),
            SpacedBy(space, bias) => write!(formatter, "SpacedBy({space:?}, {bias:?})"),
            Horizontal(_) | Vertical(_) => formatter.write_str("Custom"),
        }
    }
}

impl MainAxisArrangement {
    /// The space between children, as `Arrangement.spacing`.
    pub(crate) fn spacing_dp(&self) -> Dp {
        spacing_of(self)
    }

    /// The space between children, in pixels, as `Arrangement.spacing`.
    pub(crate) fn spacing(&self, density: &Density) -> i32 {
        match self {
            MainAxisArrangement::SpacedBy(space, _) => density.round_to_px(*space),
            MainAxisArrangement::Horizontal(policy) => density.round_to_px(policy.spacing()),
            MainAxisArrangement::Vertical(policy) => density.round_to_px(policy.spacing()),
            _ => 0,
        }
    }

    /// The positions of children of `sizes` in `total` pixels, as `Arrangement.arrange`.
    /// `direction` is the layout's for a row (right to left arranges from the right), left to
    /// right for a column.
    pub(crate) fn arrange(
        &self,
        total: i32,
        sizes: &[i32],
        direction: LayoutDirection,
        density: &Density,
        positions: &mut [i32],
    ) {
        let reverse = direction == LayoutDirection::Rtl;
        let count = sizes.len();
        let used: i32 = sizes.iter().sum();
        let free = (total - used) as f32;
        let place_from = |positions: &mut [i32], start: f32, gap: f32| {
            let mut position = start;
            for index in 0..count {
                let index = if reverse { count - 1 - index } else { index };
                positions[index] = compose_ui::platform::round_to_int(position);
                position += sizes[index] as f32 + gap;
            }
        };
        match self {
            MainAxisArrangement::Start if !reverse => place_from(positions, 0.0, 0.0),
            MainAxisArrangement::End if reverse => place_from(positions, 0.0, 0.0),
            MainAxisArrangement::Start | MainAxisArrangement::End => {
                place_from(positions, free, 0.0)
            }
            MainAxisArrangement::Center => place_from(positions, free / 2.0, 0.0),
            MainAxisArrangement::SpaceEvenly => {
                let gap = free / (count + 1) as f32;
                place_from(positions, gap, gap)
            }
            MainAxisArrangement::SpaceBetween => {
                let gap = free / (count.max(2) - 1) as f32;
                // A single child goes to the end in right-to-left, as Compose's.
                let start = if reverse && count == 1 { gap } else { 0.0 };
                place_from(positions, start, gap)
            }
            MainAxisArrangement::SpaceAround => {
                let gap = if count > 0 { free / count as f32 } else { 0.0 };
                place_from(positions, gap / 2.0, gap)
            }
            MainAxisArrangement::SpacedBy(space, bias) => {
                // As Compose's `spacedBy`: each child at the end of the last, kept inside the
                // total, and the group aligned when it doesn't fill it.
                let space = density.round_to_px(*space);
                let mut occupied = 0;
                let mut last_space = 0;
                for step in 0..count {
                    let index = if reverse { count - 1 - step } else { step };
                    let size = sizes[index];
                    let position = occupied.min(total - size);
                    positions[index] = position;
                    last_space = space.min(total - position - size);
                    occupied = position + size + last_space;
                }
                occupied -= last_space;
                if occupied < total {
                    // Without an alignment, from the start: the right in right to left.
                    let bias = bias.unwrap_or(-1.0);
                    let bias = if reverse { -bias } else { bias };
                    let shift = (total - occupied) as f32 / 2.0 * (1.0 + bias);
                    let shift = compose_ui::platform::round_to_int(shift);
                    for position in positions.iter_mut().take(count) {
                        *position += shift;
                    }
                }
            }
            MainAxisArrangement::Horizontal(policy) => {
                policy.arrange(density, total, sizes, direction, positions)
            }
            MainAxisArrangement::Vertical(policy) => {
                policy.arrange(density, total, sizes, positions)
            }
        }
    }
}

/// A horizontal arrangement, as Compose's `Arrangement.Horizontal`: what `Row`'s and
/// `LazyRow`'s `horizontal_arrangement` take. `Arrangement::START`, `End`, `aligned` with a
/// `HorizontalAlignment`, `spaced_by` with one, or a shared arrangement converted.
#[derive(Clone, PartialEq, Debug)]
pub struct HorizontalArrangement(pub(crate) MainAxisArrangement);

/// A vertical arrangement, as Compose's `Arrangement.Vertical`: what `Column`'s and
/// `LazyColumn`'s `vertical_arrangement` take. `Arrangement::TOP`, `Bottom`, `aligned` with a
/// `VerticalAlignment`, `spaced_by` with one, or a shared arrangement converted.
#[derive(Clone, PartialEq, Debug)]
pub struct VerticalArrangement(pub(crate) MainAxisArrangement);

/// An arrangement for either axis, as Compose's `Arrangement.HorizontalOrVertical`:
/// `Arrangement::CENTER`, `SpaceEvenly`, `SpaceBetween`, `SpaceAround` and `spaced_by(space)`.
/// Converts to a [`HorizontalArrangement`] or a [`VerticalArrangement`].
#[derive(Clone, PartialEq, Debug)]
pub struct HorizontalOrVerticalArrangement(MainAxisArrangement);

impl HorizontalArrangement {
    /// A custom arrangement, as an object implementing `Arrangement.Horizontal`. Compared with
    /// its policy's `same` (by type for a zero-sized one), else by reference: a `Row` given a
    /// new one with data each pass measures again, unless its `same` compares the data.
    pub fn custom(policy: impl HorizontalArrangementPolicy) -> HorizontalArrangement {
        HorizontalArrangement(MainAxisArrangement::Horizontal(Rc::new(policy)))
    }

    /// The space between adjacent children, as `Arrangement.Horizontal.spacing`.
    pub fn spacing(&self) -> Dp {
        spacing_of(&self.0)
    }

    /// The children's positions, as `Arrangement.Horizontal.arrange`.
    pub fn arrange(
        &self,
        density: &Density,
        total_size: i32,
        sizes: &[i32],
        layout_direction: LayoutDirection,
        out_positions: &mut [i32],
    ) {
        self.0.arrange(total_size, sizes, layout_direction, density, out_positions)
    }
}

impl VerticalArrangement {
    /// A custom arrangement, as an object implementing `Arrangement.Vertical`. Compared as
    /// `HorizontalArrangement::custom`'s.
    pub fn custom(policy: impl VerticalArrangementPolicy) -> VerticalArrangement {
        VerticalArrangement(MainAxisArrangement::Vertical(Rc::new(policy)))
    }

    /// The space between adjacent children, as `Arrangement.Vertical.spacing`.
    pub fn spacing(&self) -> Dp {
        spacing_of(&self.0)
    }

    /// The children's positions from the top, as `Arrangement.Vertical.arrange`.
    pub fn arrange(
        &self,
        density: &Density,
        total_size: i32,
        sizes: &[i32],
        out_positions: &mut [i32],
    ) {
        self.0.arrange(total_size, sizes, LayoutDirection::Ltr, density, out_positions)
    }
}

impl HorizontalOrVerticalArrangement {
    /// What a lazy layout arranges by.
    pub(crate) fn main_axis(&self) -> MainAxisArrangement {
        self.0.clone()
    }

    /// The space between adjacent children, as `Arrangement.HorizontalOrVertical.spacing`.
    pub fn spacing(&self) -> Dp {
        spacing_of(&self.0)
    }
}

fn spacing_of(arrangement: &MainAxisArrangement) -> Dp {
    match arrangement {
        MainAxisArrangement::SpacedBy(space, _) => *space,
        MainAxisArrangement::Horizontal(policy) => policy.spacing(),
        MainAxisArrangement::Vertical(policy) => policy.spacing(),
        _ => Dp(0.0),
    }
}

impl From<HorizontalOrVerticalArrangement> for HorizontalArrangement {
    fn from(arrangement: HorizontalOrVerticalArrangement) -> HorizontalArrangement {
        HorizontalArrangement(arrangement.0)
    }
}

impl From<HorizontalOrVerticalArrangement> for VerticalArrangement {
    fn from(arrangement: HorizontalOrVerticalArrangement) -> VerticalArrangement {
        VerticalArrangement(arrangement.0)
    }
}

/// The built-in arrangements, as Compose's `Arrangement` object: `Arrangement::START`,
/// `Arrangement::TOP`, `Arrangement::SPACE_BETWEEN`, `Arrangement::spaced_by(8.dp())`.
pub struct Arrangement(());

#[allow(non_upper_case_globals)]
impl Arrangement {
    /// Children at the start of a row (the right in right to left), as `Arrangement.Start`.
    pub const START: HorizontalArrangement = HorizontalArrangement(MainAxisArrangement::Start);
    /// Children at the end of a row, as `Arrangement.End`.
    pub const END: HorizontalArrangement = HorizontalArrangement(MainAxisArrangement::End);
    /// Children at the top of a column, as `Arrangement.Top`.
    pub const TOP: VerticalArrangement = VerticalArrangement(MainAxisArrangement::Start);
    /// Children at the bottom of a column, as `Arrangement.Bottom`.
    pub const BOTTOM: VerticalArrangement = VerticalArrangement(MainAxisArrangement::End);
    /// Children in the middle, as `Arrangement.Center`.
    pub const CENTER: HorizontalOrVerticalArrangement =
        HorizontalOrVerticalArrangement(MainAxisArrangement::Center);
    /// The free space split evenly before, between and after children, as
    /// `Arrangement.SpaceEvenly`.
    pub const SPACE_EVENLY: HorizontalOrVerticalArrangement =
        HorizontalOrVerticalArrangement(MainAxisArrangement::SpaceEvenly);
    /// The free space split evenly between children, none before the first or after the last,
    /// as `Arrangement.SpaceBetween`.
    pub const SPACE_BETWEEN: HorizontalOrVerticalArrangement =
        HorizontalOrVerticalArrangement(MainAxisArrangement::SpaceBetween);
    /// The free space split evenly around children, half of it before the first and after
    /// the last, as `Arrangement.SpaceAround`.
    pub const SPACE_AROUND: HorizontalOrVerticalArrangement =
        HorizontalOrVerticalArrangement(MainAxisArrangement::SpaceAround);

    /// A fixed space between children, as `Arrangement.spacedBy`: `spaced_by(8.dp())` for
    /// either axis (the group at the start), `spaced_by((8.dp(), Alignment::END))` for a row
    /// and `spaced_by((8.dp(), Alignment::BOTTOM))` for a column (the group aligned when it
    /// doesn't fill the space), as Compose's three overloads.
    pub fn spaced_by<S: SpacedBy>(spacing: S) -> S::Arrangement {
        spacing.arrangement()
    }

    /// Children next to each other, the group aligned by `alignment`, as `Arrangement.aligned`:
    /// a `HorizontalAlignment` gives a row's arrangement, a `VerticalAlignment` a column's.
    pub fn aligned<A: AlignedBy>(alignment: A) -> A::Arrangement {
        alignment.arrangement()
    }
}

/// What `Arrangement::spaced_by` takes: a space, or a space and an alignment.
pub trait SpacedBy {
    type Arrangement;
    fn arrangement(self) -> Self::Arrangement;
}

impl SpacedBy for Dp {
    type Arrangement = HorizontalOrVerticalArrangement;
    fn arrangement(self) -> HorizontalOrVerticalArrangement {
        HorizontalOrVerticalArrangement(MainAxisArrangement::SpacedBy(self, None))
    }
}

impl SpacedBy for (Dp, HorizontalAlignment) {
    type Arrangement = HorizontalArrangement;
    fn arrangement(self) -> HorizontalArrangement {
        HorizontalArrangement(MainAxisArrangement::SpacedBy(self.0, Some(self.1.bias)))
    }
}

impl SpacedBy for (Dp, VerticalAlignment) {
    type Arrangement = VerticalArrangement;
    fn arrangement(self) -> VerticalArrangement {
        VerticalArrangement(MainAxisArrangement::SpacedBy(self.0, Some(self.1.bias)))
    }
}

/// What `Arrangement::aligned` takes: a horizontal or a vertical alignment.
pub trait AlignedBy {
    type Arrangement;
    fn arrangement(self) -> Self::Arrangement;
}

impl AlignedBy for HorizontalAlignment {
    type Arrangement = HorizontalArrangement;
    fn arrangement(self) -> HorizontalArrangement {
        HorizontalArrangement(MainAxisArrangement::SpacedBy(Dp(0.0), Some(self.bias)))
    }
}

impl AlignedBy for VerticalAlignment {
    type Arrangement = VerticalArrangement;
    fn arrangement(self) -> VerticalArrangement {
        VerticalArrangement(MainAxisArrangement::SpacedBy(Dp(0.0), Some(self.bias)))
    }
}

impl ComposeArg for HorizontalArrangement {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ComposeArg for VerticalArrangement {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ComposeArg for HorizontalOrVerticalArrangement {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// An arrangement given where an optional one is taken (a reversed lazy list's default
/// depends on its reversal, so its parameter is an `Option`): `CENTER`, `spaced_by(…)`.
impl From<HorizontalOrVerticalArrangement> for Option<VerticalArrangement> {
    fn from(arrangement: HorizontalOrVerticalArrangement) -> Option<VerticalArrangement> {
        Some(arrangement.into())
    }
}

impl From<HorizontalOrVerticalArrangement> for Option<HorizontalArrangement> {
    fn from(arrangement: HorizontalOrVerticalArrangement) -> Option<HorizontalArrangement> {
        Some(arrangement.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::Alignment;

    fn arrange(
        arrangement: impl Into<HorizontalArrangement>,
        total: i32,
        sizes: &[i32],
    ) -> Vec<i32> {
        let mut positions = vec![0; sizes.len()];
        arrangement.into().arrange(
            &Density::default(),
            total,
            sizes,
            LayoutDirection::Ltr,
            &mut positions,
        );
        positions
    }

    fn arrange_rtl(
        arrangement: impl Into<HorizontalArrangement>,
        total: i32,
        sizes: &[i32],
    ) -> Vec<i32> {
        let mut positions = vec![0; sizes.len()];
        arrangement.into().arrange(
            &Density::default(),
            total,
            sizes,
            LayoutDirection::Rtl,
            &mut positions,
        );
        positions
    }

    fn arrange_vertically(
        arrangement: impl Into<VerticalArrangement>,
        total: i32,
        sizes: &[i32],
    ) -> Vec<i32> {
        let mut positions = vec![0; sizes.len()];
        arrangement.into().arrange(&Density::default(), total, sizes, &mut positions);
        positions
    }

    #[test]
    fn arranges_as_compose() {
        let sizes = [10, 20, 10];
        assert_eq!(arrange(Arrangement::START, 100, &sizes), [0, 10, 30]);
        assert_eq!(arrange(Arrangement::END, 100, &sizes), [60, 70, 90]);
        assert_eq!(arrange(Arrangement::CENTER, 100, &sizes), [30, 40, 60]);
        assert_eq!(arrange(Arrangement::SPACE_EVENLY, 100, &sizes), [15, 40, 75]);
        assert_eq!(arrange(Arrangement::SPACE_BETWEEN, 100, &sizes), [0, 40, 90]);
        assert_eq!(arrange(Arrangement::SPACE_AROUND, 100, &sizes), [10, 40, 80]);
        assert_eq!(arrange(Arrangement::spaced_by(Dp(5.0)), 100, &sizes), [0, 15, 40]);
        assert_eq!(arrange_vertically(Arrangement::TOP, 100, &sizes), [0, 10, 30]);
        assert_eq!(arrange_vertically(Arrangement::BOTTOM, 100, &sizes), [60, 70, 90]);
        assert_eq!(arrange_vertically(Arrangement::SPACE_BETWEEN, 100, &sizes), [0, 40, 90]);
    }

    #[test]
    fn spaced_by_and_aligned_take_an_alignment_per_axis() {
        let sizes = [10, 20, 10];
        // 40 used plus 2 × 5 of spacing: 50 free.
        assert_eq!(
            arrange(Arrangement::spaced_by((Dp(5.0), Alignment::END)), 100, &sizes),
            [50, 65, 90]
        );
        assert_eq!(
            arrange(Arrangement::spaced_by((Dp(5.0), Alignment::CENTER_HORIZONTALLY)), 100, &sizes),
            [25, 40, 65]
        );
        assert_eq!(
            arrange_vertically(Arrangement::spaced_by((Dp(5.0), Alignment::BOTTOM)), 100, &sizes),
            [50, 65, 90]
        );
        assert_eq!(arrange(Arrangement::aligned(Alignment::END), 100, &sizes), [60, 70, 90]);
        assert_eq!(
            arrange_vertically(Arrangement::aligned(Alignment::CENTER_VERTICALLY), 100, &sizes),
            [30, 40, 60]
        );
        // The start of a right-to-left row is its right; an alignment follows the direction.
        assert_eq!(arrange_rtl(Arrangement::spaced_by(Dp(5.0)), 100, &sizes), [90, 65, 50]);
        assert_eq!(arrange_rtl(Arrangement::aligned(Alignment::START), 100, &sizes), [90, 70, 60]);
        assert_eq!(Arrangement::spaced_by(Dp(5.0)).spacing(), Dp(5.0));
    }

    struct Reversed;

    impl HorizontalArrangementPolicy for Reversed {
        fn spacing(&self) -> Dp {
            Dp(1.0)
        }

        fn arrange(
            &self,
            _: &Density,
            total_size: i32,
            sizes: &[i32],
            _: LayoutDirection,
            out_positions: &mut [i32],
        ) {
            let mut end = total_size;
            for (index, size) in sizes.iter().enumerate() {
                end -= size;
                out_positions[index] = end;
            }
        }
    }

    #[derive(PartialEq)]
    struct Shifted(i32);

    impl HorizontalArrangementPolicy for Shifted {
        fn arrange(&self, _: &Density, _: i32, sizes: &[i32], _: LayoutDirection, out: &mut [i32]) {
            for (index, _) in sizes.iter().enumerate() {
                out[index] = self.0;
            }
        }

        fn same(&self, other: &dyn Any) -> bool {
            other.downcast_ref::<Self>() == Some(self)
        }
    }

    #[test]
    fn custom_arrangements_arrange_and_compare_by_their_same() {
        let custom = HorizontalArrangement::custom(Reversed);
        assert_eq!(arrange(custom.clone(), 100, &[10, 20]), [90, 70]);
        assert_eq!(custom.spacing(), Dp(1.0));
        assert!(custom.same(&custom.clone()));
        // Zero-sized: any two are the same.
        assert!(custom.same(&HorizontalArrangement::custom(Reversed)));
        assert!(!custom.same(&HorizontalArrangement::custom(Shifted(1))));
        assert!(
            HorizontalArrangement::custom(Shifted(1))
                .same(&HorizontalArrangement::custom(Shifted(1)))
        );
        assert!(
            !HorizontalArrangement::custom(Shifted(1))
                .same(&HorizontalArrangement::custom(Shifted(2)))
        );
        assert!(Arrangement::START.same(&Arrangement::START));
        assert!(!Arrangement::START.same(&Arrangement::END));
    }
}
