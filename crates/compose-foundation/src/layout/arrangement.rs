//! `Arrangement`, as Compose's: how a `Row` or `Column` places its children along its main axis.

use compose_runtime::ComposeArg;
use compose_ui::{Density, Dp, LayoutDirection};

/// A main-axis arrangement, as `Arrangement.Horizontal` and `Arrangement.Vertical` in one: a
/// `Row` reads `Start` and `End` in the layout direction, a `Column` from the top.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Arrangement {
    /// At the start (`Arrangement.Start`, `Arrangement.Top`).
    Start,
    /// At the end (`Arrangement.End`, `Arrangement.Bottom`).
    End,
    Center,
    SpaceEvenly,
    SpaceBetween,
    SpaceAround,
    /// A fixed space between children, then placed as `alignment` says, as
    /// `Arrangement.spacedBy(space, alignment)`.
    SpacedBy(Dp, SpacedAlignment),
}

/// Where a `spaced_by` arrangement puts its children when they don't fill the space.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpacedAlignment {
    Start,
    Center,
    End,
}

impl Arrangement {
    pub const TOP: Arrangement = Arrangement::Start;
    pub const BOTTOM: Arrangement = Arrangement::End;

    /// `space` between children, at the start, as `Arrangement.spacedBy(space)`.
    pub fn spaced_by(space: Dp) -> Arrangement {
        Arrangement::SpacedBy(space, SpacedAlignment::Start)
    }

    pub fn spaced_by_aligned(space: Dp, alignment: SpacedAlignment) -> Arrangement {
        Arrangement::SpacedBy(space, alignment)
    }

    /// The space between children, in pixels, as `Arrangement.spacing`.
    pub fn spacing(&self, density: &Density) -> i32 {
        match self {
            Arrangement::SpacedBy(space, _) => density.round_to_px(*space),
            _ => 0,
        }
    }

    /// The positions of children of `sizes` in `total` pixels, as `Arrangement.arrange`.
    /// `reverse` arranges from the end (a right-to-left `Row`).
    pub fn arrange(
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
                positions[index] = compose_ui::unit::round_to_int(position);
                position += sizes[index] as f32 + gap;
            }
        };
        match self {
            Arrangement::Start if !reverse => place_from(positions, 0.0, 0.0),
            Arrangement::End if reverse => place_from(positions, 0.0, 0.0),
            Arrangement::Start | Arrangement::End => place_from(positions, free, 0.0),
            Arrangement::Center => place_from(positions, free / 2.0, 0.0),
            Arrangement::SpaceEvenly => {
                let gap = free / (count + 1) as f32;
                place_from(positions, gap, gap)
            }
            Arrangement::SpaceBetween => {
                let gap = free / (count.max(2) - 1) as f32;
                // A single child goes to the end in right-to-left, as Compose's.
                let start = if reverse && count == 1 { gap } else { 0.0 };
                place_from(positions, start, gap)
            }
            Arrangement::SpaceAround => {
                let gap = if count > 0 { free / count as f32 } else { 0.0 };
                place_from(positions, gap / 2.0, gap)
            }
            Arrangement::SpacedBy(space, alignment) => {
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
                    let free = (total - occupied) as f32;
                    let shift = match (alignment, reverse) {
                        (SpacedAlignment::Start, false) | (SpacedAlignment::End, true) => 0.0,
                        (SpacedAlignment::Center, _) => free / 2.0,
                        _ => free,
                    };
                    let shift = compose_ui::unit::round_to_int(shift);
                    for position in positions.iter_mut().take(count) {
                        *position += shift;
                    }
                }
            }
        }
    }
}

impl ComposeArg for Arrangement {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrange(arrangement: Arrangement, total: i32, sizes: &[i32]) -> Vec<i32> {
        let mut positions = vec![0; sizes.len()];
        arrangement.arrange(
            total,
            sizes,
            LayoutDirection::Ltr,
            &Density::default(),
            &mut positions,
        );
        positions
    }

    #[test]
    fn arranges_as_compose() {
        let sizes = [10, 20, 10];
        assert_eq!(arrange(Arrangement::Start, 100, &sizes), [0, 10, 30]);
        assert_eq!(arrange(Arrangement::End, 100, &sizes), [60, 70, 90]);
        assert_eq!(arrange(Arrangement::Center, 100, &sizes), [30, 40, 60]);
        assert_eq!(arrange(Arrangement::SpaceEvenly, 100, &sizes), [15, 40, 75]);
        assert_eq!(arrange(Arrangement::SpaceBetween, 100, &sizes), [0, 40, 90]);
        assert_eq!(arrange(Arrangement::SpaceAround, 100, &sizes), [10, 40, 80]);
        assert_eq!(arrange(Arrangement::spaced_by(Dp(5.0)), 100, &sizes), [0, 15, 40]);
    }
}
