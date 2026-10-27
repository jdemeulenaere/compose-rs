//! `FlowRow` and `FlowColumn`, as Compose's (foundation-layout's `FlowLayout.kt`): children
//! side by side (or one below the other), wrapping onto a new line when the next doesn't fit,
//! a line at a time; each line measured as a `Row` (or a `Column`) is, weights and alignments
//! included. The deprecated `overflow` parameter isn't there: lines past `max_lines`, or past
//! the cross-axis size allowed, are left out, as Compose's default (`FlowRowOverflow.Clip`).

use std::rc::Rc;

use compose_runtime::{ComposeArg, composable, remember_keyed};
use compose_ui::platform::round_to_int;
use compose_ui::{
    AlignScope, Constraints, HorizontalAlignment, IntrinsicMeasurable, IntrinsicMeasureScope,
    Layout, LayoutDirection, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier,
    ParentAlignment, Placeable, VerticalAlignment, WeightScope,
};

use super::{HorizontalArrangement, MainAxisArrangement, VerticalArrangement};

/// The content scope of a `FlowRow`, as Compose's `FlowRowScope` (a `RowScope`): `weight` and
/// `align` take it, and `fill_max_row_height`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FlowRowScope(());

/// The content scope of a `FlowColumn`, as Compose's `FlowColumnScope` (a `ColumnScope`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FlowColumnScope(());

impl WeightScope for FlowRowScope {}
impl WeightScope for FlowColumnScope {}

impl AlignScope for FlowRowScope {
    type Alignment = VerticalAlignment;
    fn parent_alignment(alignment: VerticalAlignment) -> ParentAlignment {
        ParentAlignment::Vertical(alignment)
    }
}

impl AlignScope for FlowColumnScope {
    type Alignment = HorizontalAlignment;
    fn parent_alignment(alignment: HorizontalAlignment) -> ParentAlignment {
        ParentAlignment::Horizontal(alignment)
    }
}

impl ComposeArg for FlowRowScope {
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ComposeArg for FlowColumnScope {
    fn same(&self, _: &Self) -> bool {
        true
    }
}

/// The flow layouts' scoped modifiers, on `Modifier`.
pub trait FlowLayoutModifiers {
    /// Fills its row's height, as `FlowRowScope.fillMaxRowHeight()`: as tall as the row's
    /// tallest item.
    fn fill_max_row_height(self, scope: FlowRowScope) -> Modifier;
    /// `fraction` of its row's height, as `fillMaxRowHeight(fraction)`.
    ///
    /// # Panics
    ///
    /// If `fraction` isn't within 0 and 1, as Compose's.
    fn fill_max_row_height_with(self, scope: FlowRowScope, fraction: f32) -> Modifier;
    /// Fills its column's width, as `FlowColumnScope.fillMaxColumnWidth()`.
    fn fill_max_column_width(self, scope: FlowColumnScope) -> Modifier;
    /// `fraction` of its column's width, as `fillMaxColumnWidth(fraction)`.
    ///
    /// # Panics
    ///
    /// If `fraction` isn't within 0 and 1, as Compose's.
    fn fill_max_column_width_with(self, scope: FlowColumnScope, fraction: f32) -> Modifier;
}

impl FlowLayoutModifiers for Modifier {
    fn fill_max_row_height(self, scope: FlowRowScope) -> Modifier {
        self.fill_max_row_height_with(scope, 1.0)
    }
    fn fill_max_row_height_with(self, _scope: FlowRowScope, fraction: f32) -> Modifier {
        self.fill_cross_axis_parent_data(fraction)
    }
    fn fill_max_column_width(self, scope: FlowColumnScope) -> Modifier {
        self.fill_max_column_width_with(scope, 1.0)
    }
    fn fill_max_column_width_with(self, _scope: FlowColumnScope, fraction: f32) -> Modifier {
        self.fill_cross_axis_parent_data(fraction)
    }
}

/// Children from the start, wrapping onto a new row below when the next doesn't fit, as
/// Compose's `FlowRow`: at most `max_items_in_each_row` in a row and `max_lines` rows (those
/// past them, or past the height allowed, left out); each row arranged by
/// `horizontal_arrangement`, its items aligned by `item_vertical_alignment` (or their own
/// `align`), the rows arranged by `vertical_arrangement`. A `weight` shares what's left of the
/// row an item ends up in.
#[composable]
pub fn FlowRow(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::layout::Arrangement::START)] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: impl Into<VerticalArrangement>,
    #[default(crate::Alignment::TOP)] item_vertical_alignment: VerticalAlignment,
    #[default(u32::MAX)] max_items_in_each_row: u32,
    #[default(u32::MAX)] max_lines: u32,
    content: impl Fn(FlowRowScope) + 'static,
) {
    let policy = remember_keyed(
        (
            horizontal_arrangement.clone(),
            vertical_arrangement.clone(),
            item_vertical_alignment,
            max_items_in_each_row,
            max_lines,
        ),
        || {
            Rc::new(FlowMeasurePolicy {
                horizontal: true,
                main: horizontal_arrangement.0.clone(),
                cross: vertical_arrangement.0.clone(),
                bias: item_vertical_alignment.bias,
                max_items: max_items_in_each_row,
                max_lines,
            }) as Rc<dyn MeasurePolicy>
        },
    );
    Layout(modifier, policy, compose_runtime::callback(move || content(FlowRowScope(()))));
}

/// Children from the top, wrapping onto a new column beside when the next doesn't fit, as
/// Compose's `FlowColumn` (`FlowRow` turned: the first column at the start, on the right in a
/// right-to-left layout).
#[composable]
pub fn FlowColumn(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: impl Into<VerticalArrangement>,
    #[default(crate::layout::Arrangement::START)] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(crate::Alignment::START)] item_horizontal_alignment: HorizontalAlignment,
    #[default(u32::MAX)] max_items_in_each_column: u32,
    #[default(u32::MAX)] max_lines: u32,
    content: impl Fn(FlowColumnScope) + 'static,
) {
    let policy = remember_keyed(
        (
            vertical_arrangement.clone(),
            horizontal_arrangement.clone(),
            item_horizontal_alignment,
            max_items_in_each_column,
            max_lines,
        ),
        || {
            Rc::new(FlowMeasurePolicy {
                horizontal: false,
                main: vertical_arrangement.0.clone(),
                cross: horizontal_arrangement.0.clone(),
                bias: item_horizontal_alignment.bias,
                max_items: max_items_in_each_column,
                max_lines,
            }) as Rc<dyn MeasurePolicy>
        },
    );
    Layout(modifier, policy, compose_runtime::callback(move || content(FlowColumnScope(()))));
}

/// The flow layouts' policy, as Compose's `FlowMeasurePolicy` with `FlowRowOverflow.Clip`.
#[derive(PartialEq)]
struct FlowMeasurePolicy {
    /// A `FlowRow` (else a `FlowColumn`).
    horizontal: bool,
    /// Along a line: a row's horizontal arrangement, a column's vertical one.
    main: MainAxisArrangement,
    /// Of the lines: a row's vertical arrangement, a column's horizontal one.
    cross: MainAxisArrangement,
    /// The items' alignment across their line.
    bias: f32,
    max_items: u32,
    max_lines: u32,
}

/// Where the wrapping decided an item goes, as Compose's `FlowLayoutBuildingBlocks.WrapInfo`.
#[derive(Clone, Copy)]
struct WrapInfo {
    last_in_line: bool,
    last_in_container: bool,
}

/// What wrapping depends on, as Compose's `FlowLayoutBuildingBlocks` (without the deprecated
/// overflow indicators).
struct Wrapping {
    max_items: u32,
    max_lines: u32,
    main_max: i32,
    main_spacing: i32,
    cross_spacing: i32,
}

impl Wrapping {
    /// Whether the item before the next ends its line, and the container, as Compose's
    /// `getWrapInfo`: the next item (of `next` main and cross size, its spacing included past a
    /// line's first) goes on a new line when it's the line's `max_items`-th or doesn't fit
    /// what's left of it (`left`: main, cross); it's left out past `max_lines` or the cross
    /// size left.
    fn wrap(
        &self,
        next_index_in_line: u32,
        left: (i32, i32),
        next: Option<(i32, i32)>,
        line_index: u32,
        current_line_cross: i32,
        wrapping_round: bool,
    ) -> WrapInfo {
        let Some(next) = next else {
            return WrapInfo { last_in_line: true, last_in_container: true };
        };
        if line_index >= self.max_lines || left.1.saturating_sub(next.1) < 0 {
            return WrapInfo { last_in_line: true, last_in_container: true };
        }
        let wraps = next_index_in_line != 0
            && (next_index_in_line >= self.max_items || left.0.saturating_sub(next.0) < 0);
        if wraps {
            if wrapping_round {
                return WrapInfo { last_in_line: true, last_in_container: true };
            }
            let on_next_line = self.wrap(
                0,
                (
                    self.main_max,
                    left.1.saturating_sub(self.cross_spacing).saturating_sub(current_line_cross),
                ),
                Some((next.0 - self.main_spacing, next.1)),
                line_index + 1,
                0,
                true,
            );
            return WrapInfo {
                last_in_line: true,
                last_in_container: on_next_line.last_in_container,
            };
        }
        WrapInfo { last_in_line: false, last_in_container: false }
    }
}

/// An item placed by its line: its index, its main-axis position in the line and its
/// cross-axis position in it.
struct Placed {
    index: usize,
    main: i32,
    cross: i32,
}

/// A measured line: its size and its items.
struct Line {
    main: i32,
    cross: i32,
    items: Vec<Placed>,
}

impl FlowMeasurePolicy {
    /// Constraints from main- and cross-axis bounds.
    fn constraints(
        &self,
        main_min: i32,
        main_max: i32,
        cross_min: i32,
        cross_max: i32,
    ) -> Constraints {
        if self.horizontal {
            Constraints::new(main_min, main_max, cross_min, cross_max)
        } else {
            Constraints::new(cross_min, cross_max, main_min, main_max)
        }
    }

    /// A placeable's (main, cross) size as it measured itself, before coercion into its
    /// constraints, as Compose's flow's `mainAxisSize` and `crossAxisSize` (D1353).
    fn sizes(&self, placeable: &Placeable) -> (i32, i32) {
        let (width, height) = (placeable.measured_width(), placeable.measured_height());
        if self.horizontal { (width, height) } else { (height, width) }
    }

    fn weight(scope: &IntrinsicMeasureScope<'_>, measurable: Measurable) -> f32 {
        scope.parent_data(measurable).weight.map_or(0.0, |(weight, _)| weight)
    }

    /// An item's (main, cross) size for the wrapping, as Compose's `measureAndCache`: measured
    /// once if it has a fixed size, else (weighted, or filling its line) from its minimum
    /// intrinsics, measured with its line.
    fn item_size(
        &self,
        scope: &mut MeasureScope<'_>,
        measurable: Measurable,
        constraints: Constraints,
        placeables: &mut [Option<Placeable>],
        index: usize,
    ) -> (i32, i32) {
        let data = scope.parent_data(measurable);
        let weighted = data.weight.is_some_and(|(weight, _)| weight > 0.0);
        if !weighted && data.fill_cross_axis_fraction.is_none() {
            let placeable = scope.measure(measurable, constraints);
            placeables[index] = Some(placeable);
            return self.sizes(&placeable);
        }
        let main = self.intrinsic(scope, measurable, false, true, Constraints::INFINITY);
        let cross = self.intrinsic(scope, measurable, false, false, main);
        (main, cross)
    }

    /// `measurable`'s minimum (or `max`imum) intrinsic along the main axis (`main`) or across
    /// it, at `other`.
    fn intrinsic(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurable: Measurable,
        max: bool,
        main: bool,
        other: i32,
    ) -> i32 {
        match (max, main == self.horizontal) {
            (false, true) => scope.min_intrinsic_width(measurable, other),
            (false, false) => scope.min_intrinsic_height(measurable, other),
            (true, true) => scope.max_intrinsic_width(measurable, other),
            (true, false) => scope.max_intrinsic_height(measurable, other),
        }
    }

    /// The items `range` as a line, as Compose's `RowColumnMeasurePolicy.measure`: those
    /// without weight first, with what's left of the line; the weighted ones sharing the rest;
    /// items filling the line across at their fraction of `cross_max`.
    #[allow(clippy::too_many_arguments)]
    fn measure_line(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        placeables: &mut [Option<Placeable>],
        range: std::ops::Range<usize>,
        main_min: i32,
        main_max: i32,
        cross_max: i32,
        spacing: i32,
    ) -> Line {
        let start = range.start;
        let count = range.len();
        let mut total_weight = 0.0f32;
        let mut fixed = 0i32;
        let mut cross_space = 0i32;
        let mut weighted_count = 0i32;
        let mut space_after_last = 0;
        let mut main_sizes = vec![0; count];
        let desired = |fraction: Option<f32>| {
            if cross_max == Constraints::INFINITY {
                None
            } else {
                fraction.map(|fraction| round_to_int(fraction * cross_max as f32))
            }
        };
        for index in range.clone() {
            let measurable = measurables[index];
            let data = scope.parent_data(measurable);
            let weight = data.weight.map_or(0.0, |(weight, _)| weight);
            if weight > 0.0 {
                total_weight += weight;
                weighted_count += 1;
                continue;
            }
            let wanted = desired(data.fill_cross_axis_fraction);
            let remaining = main_max.saturating_sub(fixed);
            let placeable = match placeables[index] {
                Some(placeable) => placeable,
                None => scope.measure(
                    measurable,
                    self.constraints(
                        0,
                        if main_max == Constraints::INFINITY {
                            Constraints::INFINITY
                        } else {
                            remaining.max(0)
                        },
                        wanted.unwrap_or(0),
                        wanted.unwrap_or(cross_max),
                    ),
                ),
            };
            let (main, cross) = self.sizes(&placeable);
            main_sizes[index - start] = main;
            space_after_last = spacing.min(remaining.saturating_sub(main).max(0));
            fixed = fixed.saturating_add(main + space_after_last);
            cross_space = cross_space.max(cross);
            placeables[index] = Some(placeable);
        }
        let mut weighted_space = 0i64;
        if weighted_count == 0 {
            fixed -= space_after_last;
        } else {
            let target = if main_max != Constraints::INFINITY { main_max } else { main_min };
            let spacing_total = spacing as i64 * (weighted_count as i64 - 1);
            let remaining_to_target = (target as i64 - fixed as i64 - spacing_total).max(0);
            let unit = remaining_to_target as f32 / total_weight;
            let mut remainder = remaining_to_target
                - range
                    .clone()
                    .map(|index| {
                        round_to_int(unit * Self::weight(scope, measurables[index])) as i64
                    })
                    .sum::<i64>();
            for index in range.clone() {
                if placeables[index].is_some() {
                    continue;
                }
                let measurable = measurables[index];
                let data = scope.parent_data(measurable);
                let (weight, fill) = data.weight.expect("compose-foundation: a weighted item");
                let wanted = desired(data.fill_cross_axis_fraction);
                let step = remainder.signum();
                remainder -= step;
                let size = (round_to_int(unit * weight) as i64 + step).max(0) as i32;
                let placeable = scope.measure(
                    measurable,
                    self.constraints(
                        if fill && size != Constraints::INFINITY { size } else { 0 },
                        size,
                        wanted.unwrap_or(0),
                        wanted.unwrap_or(cross_max),
                    ),
                );
                let (main, cross) = self.sizes(&placeable);
                main_sizes[index - start] = main;
                weighted_space += main as i64;
                cross_space = cross_space.max(cross);
                placeables[index] = Some(placeable);
            }
            weighted_space =
                (weighted_space + spacing_total).clamp(0, (main_max as i64 - fixed as i64).max(0));
        }
        let main = ((fixed as i64 + weighted_space).max(0) as i32).max(main_min);
        let cross = cross_space;
        let mut positions = vec![0; count];
        let density = scope.density();
        let direction =
            if self.horizontal { scope.layout_direction() } else { LayoutDirection::Ltr };
        self.main.arrange(main, &main_sizes, direction, &density, &mut positions);
        let items = range
            .clone()
            .map(|index| {
                let placeable = placeables[index].expect("measured");
                let (_, item_cross) = self.sizes(&placeable);
                let own = match scope.parent_data(measurables[index]).alignment {
                    Some(ParentAlignment::Vertical(alignment)) if self.horizontal => alignment.bias,
                    Some(ParentAlignment::Horizontal(alignment)) if !self.horizontal => {
                        alignment.bias
                    }
                    _ => self.bias,
                };
                let cross = if self.horizontal {
                    VerticalAlignment { bias: own }.align(item_cross, cross)
                } else {
                    HorizontalAlignment { bias: own }.align(
                        item_cross,
                        cross,
                        scope.layout_direction(),
                    )
                };
                Placed { index, main: positions[index - start], cross }
            })
            .collect();
        Line { main, cross, items }
    }
}

impl MeasurePolicy for FlowMeasurePolicy {
    /// Equal data measures the same (Compose's policies are data classes; D1354).
    fn same(&self, other: &dyn std::any::Any) -> bool {
        other.downcast_ref::<Self>() == Some(self)
    }

    /// As Compose's `breakDownItems`: the items wrapped into lines, measured once where their
    /// size is fixed; then each line measured, and the lines arranged.
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        if self.max_lines == 0
            || self.max_items == 0
            || measurables.is_empty()
            || constraints.max_height == 0
        {
            return scope.layout(0, 0);
        }
        let (main_min, main_max, cross_min, cross_max) = if self.horizontal {
            (
                constraints.min_width,
                constraints.max_width,
                constraints.min_height,
                constraints.max_height,
            )
        } else {
            (
                constraints.min_height,
                constraints.max_height,
                constraints.min_width,
                constraints.max_width,
            )
        };
        let density = scope.density();
        let ceil_px = |dp: compose_ui::Dp| density.to_px(dp).ceil() as i32;
        let spacing = ceil_px(self.main.spacing_dp());
        let cross_spacing = ceil_px(self.cross.spacing_dp());
        let wrapping = Wrapping {
            max_items: self.max_items,
            max_lines: self.max_lines,
            main_max,
            main_spacing: spacing,
            cross_spacing,
        };
        let item_constraints = self.constraints(0, main_max, 0, cross_max);
        let count = measurables.len();
        let mut placeables: Vec<Option<Placeable>> = vec![None; count];

        let mut line_index = 0u32;
        let mut left = main_max;
        let mut left_cross = cross_max;
        let mut next =
            Some(self.item_size(scope, measurables[0], item_constraints, &mut placeables, 0));
        let mut start_break = 0usize;
        let mut ends: Vec<usize> = Vec::new();
        let mut cross_sizes: Vec<i32> = Vec::new();
        let mut filled_lines: Vec<bool> = Vec::new();
        let mut wrap = wrapping.wrap(0, (left, left_cross), next, 0, 0, false);
        let mut main_total = main_min;
        let mut cross_total = 0i32;
        let (mut line_main, mut line_cross, mut line_filled) = (0i32, 0i32, false);
        let mut index = 0usize;
        while !wrap.last_in_container && index < count {
            let (item_main, item_cross) = next.expect("an item");
            line_main = line_main.saturating_add(item_main);
            line_cross = line_cross.max(item_cross);
            left = left.saturating_sub(item_main);
            if scope.parent_data(measurables[index]).fill_cross_axis_fraction.is_some() {
                line_filled = true;
            }
            let next_index_in_line = (index + 1 - start_break) as u32;
            next = (index + 1 < count).then(|| {
                let (main, cross) = self.item_size(
                    scope,
                    measurables[index + 1],
                    item_constraints,
                    &mut placeables,
                    index + 1,
                );
                (main + spacing, cross)
            });
            wrap = wrapping.wrap(
                next_index_in_line,
                (left, left_cross),
                next,
                line_index,
                line_cross,
                false,
            );
            if wrap.last_in_line {
                main_total = main_total.max(line_main).min(main_max);
                cross_total = cross_total.saturating_add(line_cross);
                cross_sizes.push(line_cross);
                filled_lines.push(line_filled);
                line_filled = false;
                left = main_max;
                left_cross = cross_max.saturating_sub(cross_total).saturating_sub(cross_spacing);
                start_break = index + 1;
                ends.push(index + 1);
                line_main = 0;
                line_cross = 0;
                // Spacing comes before an item that isn't first in its line.
                next = next.map(|(main, cross)| (main - spacing, cross));
                line_index += 1;
                cross_total = cross_total.saturating_add(cross_spacing);
            }
            index += 1;
        }

        // Each line, as a row (or a column) of its items, within what's left across.
        let mut lines = Vec::with_capacity(ends.len());
        let mut start = 0;
        let mut cross_total = 0i32;
        for (line, &end) in ends.iter().enumerate() {
            let line_cross_max = if filled_lines[line] {
                cross_sizes[line]
            } else if cross_max == Constraints::INFINITY {
                Constraints::INFINITY
            } else {
                (cross_max - cross_total).max(0)
            };
            let measured = self.measure_line(
                scope,
                measurables,
                &mut placeables,
                start..end,
                main_total,
                main_max,
                line_cross_max,
                spacing,
            );
            cross_total = cross_total.saturating_add(measured.cross);
            main_total = main_total.max(measured.main);
            lines.push(measured);
            start = end;
        }
        if lines.is_empty() {
            main_total = 0;
            cross_total = 0;
        }

        // The lines, arranged across, as Compose's `placeHelper`.
        let line_count = lines.len() as i32;
        let cross_spacing_rounded = density.round_to_px(self.cross.spacing_dp());
        let total_cross = cross_total
            .saturating_add(cross_spacing_rounded * (line_count - 1).max(0))
            .clamp(cross_min, cross_max);
        let line_sizes: Vec<i32> = lines.iter().map(|line| line.cross).collect();
        let mut line_positions = vec![0; lines.len()];
        let cross_direction =
            if self.horizontal { LayoutDirection::Ltr } else { scope.layout_direction() };
        self.cross.arrange(
            total_cross,
            &line_sizes,
            cross_direction,
            &density,
            &mut line_positions,
        );
        let main_size = main_total.clamp(main_min, main_max);
        for (line, offset) in lines.iter().zip(&line_positions) {
            for item in &line.items {
                let placeable = placeables[item.index].expect("measured");
                if self.horizontal {
                    scope.place(&placeable, item.main, offset + item.cross);
                } else {
                    scope.place(&placeable, offset + item.cross, item.main);
                }
            }
        }
        if self.horizontal {
            scope.layout(main_size, total_cross)
        } else {
            scope.layout(total_cross, main_size)
        }
    }

    fn min_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        if self.horizontal {
            self.min_intrinsic_main_size(scope, measurables, height)
        } else {
            self.intrinsic_cross_size(scope, measurables, height)
        }
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        if self.horizontal {
            self.intrinsic_cross_size(scope, measurables, width)
        } else {
            self.min_intrinsic_main_size(scope, measurables, width)
        }
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        if self.horizontal {
            self.max_intrinsic_main_size(scope, measurables, height)
        } else {
            self.intrinsic_cross_size(scope, measurables, height)
        }
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        if self.horizontal {
            self.intrinsic_cross_size(scope, measurables, width)
        } else {
            self.max_intrinsic_main_size(scope, measurables, width)
        }
    }

    fn debug_name(&self) -> &'static str {
        if self.horizontal { "FlowRow" } else { "FlowColumn" }
    }
}

impl FlowMeasurePolicy {
    /// The spacings the intrinsics use: (along a line, between lines), rounded.
    fn intrinsic_spacings(&self, scope: &IntrinsicMeasureScope<'_>) -> (i32, i32) {
        let density = scope.density();
        (density.round_to_px(self.main.spacing_dp()), density.round_to_px(self.cross.spacing_dp()))
    }

    /// As Compose's `maxIntrinsicMainAxisSize`: the longest line of `max_items` items at their
    /// maximum intrinsic main-axis sizes, with the spacing.
    fn max_intrinsic_main_size(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        children: &[IntrinsicMeasurable],
        cross_available: i32,
    ) -> i32 {
        let (spacing, _) = self.intrinsic_spacings(scope);
        let (mut longest, mut current, mut last_break) = (0i32, 0i32, 0usize);
        for (index, &child) in children.iter().enumerate() {
            let size = self.intrinsic(scope, child, true, true, cross_available) + spacing;
            if (index + 1 - last_break) as u64 == self.max_items as u64
                || index + 1 == children.len()
            {
                last_break = index;
                current = current.saturating_add(size) - spacing;
                longest = longest.max(current);
                current = 0;
            } else {
                current = current.saturating_add(size);
            }
        }
        longest
    }

    /// As Compose's `intrinsicCrossAxisSize` with the minimum intrinsics: the lines' total
    /// cross-axis size when wrapped at `main_available`.
    fn intrinsic_cross_size(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        children: &[IntrinsicMeasurable],
        main_available: i32,
    ) -> i32 {
        let (spacing, cross_spacing) = self.intrinsic_spacings(scope);
        let mut sizes =
            |scope: &mut IntrinsicMeasureScope<'_>, index: usize, main_available: i32| {
                let cross = self.intrinsic(scope, children[index], false, false, main_available);
                let main = self.intrinsic(scope, children[index], false, true, cross);
                (main, cross)
            };
        self.cross_size_of(
            scope,
            children.len(),
            &mut sizes,
            main_available,
            spacing,
            cross_spacing,
        )
        .0
    }

    /// As Compose's `intrinsicCrossAxisSize`: the lines' total cross-axis size, and how many
    /// items show, wrapping `count` items of `sizes` (main, cross at the main size available)
    /// at `main_available`.
    fn cross_size_of(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        count: usize,
        sizes: &mut dyn FnMut(&mut IntrinsicMeasureScope<'_>, usize, i32) -> (i32, i32),
        main_available: i32,
        spacing: i32,
        cross_spacing: i32,
    ) -> (i32, usize) {
        if count == 0 {
            return (0, 0);
        }
        let wrapping = Wrapping {
            max_items: self.max_items,
            max_lines: self.max_lines,
            main_max: main_available,
            main_spacing: spacing,
            cross_spacing,
        };
        let (mut next_main, mut next_cross) = sizes(scope, 0, main_available);
        let mut remaining = main_available;
        let (mut current_cross, mut total, mut last_break, mut line) = (0i32, 0i32, 0usize, 0u32);
        let wrap = wrapping.wrap(
            0,
            (remaining, Constraints::INFINITY),
            Some((next_main, next_cross)),
            line,
            current_cross,
            false,
        );
        if wrap.last_in_container {
            return (0, 0);
        }
        let mut shown = 0;
        for index in 0..count {
            let (child_main, child_cross) = (next_main, next_cross);
            remaining = remaining.saturating_sub(child_main);
            shown = index + 1;
            current_cross = current_cross.max(child_cross);
            let next = (index + 1 < count).then(|| {
                let (main, cross) = sizes(scope, index + 1, main_available);
                (main + spacing, cross)
            });
            (next_main, next_cross) = next.unwrap_or((0, 0));
            let wrap = wrapping.wrap(
                (index + 1 - last_break) as u32,
                (remaining, Constraints::INFINITY),
                next,
                line,
                current_cross,
                false,
            );
            if wrap.last_in_line {
                total = total.saturating_add(current_cross + cross_spacing);
                current_cross = 0;
                remaining = main_available;
                last_break = index + 1;
                next_main -= spacing;
                line += 1;
                if wrap.last_in_container {
                    break;
                }
            }
        }
        (total - cross_spacing, shown)
    }

    /// As Compose's `minIntrinsicMainAxisSize`: the smallest main-axis size, between the widest
    /// item and all of them in one line, at which the lines fit `cross_available` and show
    /// every item they can, found by bisection.
    fn min_intrinsic_main_size(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        children: &[IntrinsicMeasurable],
        cross_available: i32,
    ) -> i32 {
        if children.is_empty() {
            return 0;
        }
        let (spacing, cross_spacing) = self.intrinsic_spacings(scope);
        let main_sizes: Vec<i32> = children
            .iter()
            .map(|&child| self.intrinsic(scope, child, false, true, cross_available))
            .collect();
        let cross_sizes: Vec<i32> = children
            .iter()
            .zip(&main_sizes)
            .map(|(&child, &main)| self.intrinsic(scope, child, false, false, main))
            .collect();
        let showable = if self.max_lines != u32::MAX && self.max_items != u32::MAX {
            (self.max_items as u64 * self.max_lines as u64).min(children.len() as u64) as usize
        } else {
            children.len()
        };
        let all = main_sizes
            .iter()
            .fold(0i32, |sum, &size| sum.saturating_add(size))
            .saturating_add((children.len() as i32 - 1).saturating_mul(spacing));
        let mut main_used = all;
        let mut cross_used = cross_sizes.iter().copied().max().unwrap_or(0);
        let mut low = main_sizes.iter().copied().max().unwrap_or(0);
        let mut high = all;
        let mut sizes = |_: &mut IntrinsicMeasureScope<'_>, index: usize, _: i32| {
            (main_sizes[index], cross_sizes[index])
        };
        while low <= high {
            if cross_used == cross_available {
                return main_used;
            }
            let mid = low + (high - low) / 2;
            main_used = mid;
            let (cross, shown) =
                self.cross_size_of(scope, children.len(), &mut sizes, mid, spacing, cross_spacing);
            cross_used = cross;
            if cross_used > cross_available || shown < showable {
                low = mid + 1;
                if low > high {
                    return low;
                }
            } else if cross_used < cross_available {
                high = mid - 1;
            } else {
                return main_used;
            }
        }
        main_used
    }
}
