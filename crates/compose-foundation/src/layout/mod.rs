//! `compose::foundation::layout`, as `androidx.compose.foundation.layout`: `Box`, `Column`,
//! `Row` and `Spacer`, each a `Layout` with its own measure policy, and `Arrangement`.
//! The layout modifiers (`padding`, `size`, `fill_max_*`, `weight`, `align`) are methods of
//! `Modifier` (docs/06-ui.md).

mod arrangement;
mod window_insets;

use std::rc::Rc;

use compose_runtime::{composable, remember_keyed};
use compose_ui::platform::round_to_int;
use compose_ui::{
    Alignment, Constraints, HorizontalAlignment, IntrinsicMeasurable, IntrinsicMeasureScope,
    Layout, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier, ParentAlignment,
    Placeable, VerticalAlignment, empty_measure_policy,
};
use smallvec_free::Buffer;

pub(crate) use arrangement::MainAxisArrangement;
pub use arrangement::{
    AlignedBy, Arrangement, HorizontalArrangement, HorizontalArrangementPolicy,
    HorizontalOrVerticalArrangement, SpacedBy, VerticalArrangement, VerticalArrangementPolicy,
};
pub use compose_ui::{BoxScope, ColumnScope, PaddingValues, RowScope, SizeSpec};
pub use window_insets::{WindowInsets, WindowInsetsModifiers, WindowInsetsSides};

/// Scratch space for a policy's children, without allocating for up to 16 of them.
mod smallvec_free {
    pub(crate) enum Buffer<T: Copy + Default> {
        Inline([T; 16], usize),
        Heap(Vec<T>),
    }

    impl<T: Copy + Default> Buffer<T> {
        pub(crate) fn new(len: usize) -> Buffer<T> {
            if len <= 16 {
                Buffer::Inline([T::default(); 16], len)
            } else {
                Buffer::Heap(vec![T::default(); len])
            }
        }

        pub(crate) fn as_mut(&mut self) -> &mut [T] {
            match self {
                Buffer::Inline(items, len) => &mut items[..*len],
                Buffer::Heap(items) => items,
            }
        }
    }
}

// ---- Row and Column --------------------------------------------------------------------------

/// The main axis of a `Row` (horizontal) or a `Column` (vertical).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Orientation {
    Horizontal,
    Vertical,
}

/// Constraints along the main and cross axes.
#[derive(Clone, Copy)]
struct AxisConstraints {
    main_min: i32,
    main_max: i32,
    cross_min: i32,
    cross_max: i32,
}

impl AxisConstraints {
    fn new(orientation: Orientation, constraints: Constraints) -> AxisConstraints {
        match orientation {
            Orientation::Horizontal => AxisConstraints {
                main_min: constraints.min_width,
                main_max: constraints.max_width,
                cross_min: constraints.min_height,
                cross_max: constraints.max_height,
            },
            Orientation::Vertical => AxisConstraints {
                main_min: constraints.min_height,
                main_max: constraints.max_height,
                cross_min: constraints.min_width,
                cross_max: constraints.max_width,
            },
        }
    }

    fn to_constraints(self, orientation: Orientation) -> Constraints {
        match orientation {
            Orientation::Horizontal => {
                Constraints::new(self.main_min, self.main_max, self.cross_min, self.cross_max)
            }
            Orientation::Vertical => {
                Constraints::new(self.cross_min, self.cross_max, self.main_min, self.main_max)
            }
        }
    }
}

/// `Row`'s and `Column`'s measure policy, as Compose's `RowColumnMeasurePolicy`: children
/// without weight measured first with the space left, then the weighted ones sharing what
/// remains in proportion; placed by the arrangement along the main axis and by their alignment
/// across it.
pub(crate) struct RowColumnMeasurePolicy {
    orientation: Orientation,
    arrangement: MainAxisArrangement,
    /// A `Row`'s vertical bias or a `Column`'s horizontal bias.
    cross_bias: f32,
}

impl RowColumnMeasurePolicy {
    fn main(&self, placeable: &Placeable) -> i32 {
        match self.orientation {
            Orientation::Horizontal => placeable.width,
            Orientation::Vertical => placeable.height,
        }
    }

    fn cross(&self, placeable: &Placeable) -> i32 {
        match self.orientation {
            Orientation::Horizontal => placeable.height,
            Orientation::Vertical => placeable.width,
        }
    }

    fn cross_bias_of(&self, scope: &MeasureScope<'_>, measurable: Measurable) -> f32 {
        match (self.orientation, scope.parent_data(measurable).alignment) {
            (Orientation::Horizontal, Some(ParentAlignment::Vertical(alignment))) => alignment.bias,
            (Orientation::Vertical, Some(ParentAlignment::Horizontal(alignment))) => alignment.bias,
            _ => self.cross_bias,
        }
    }
}

impl MeasurePolicy for RowColumnMeasurePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let axis = AxisConstraints::new(self.orientation, constraints);
        let density = scope.density();
        let spacing = self.arrangement.spacing(&density);
        let count = measurables.len();
        let mut placeables: Buffer<Option<Placeable>> = Buffer::new(count);
        let placeables = placeables.as_mut();
        let bounded = axis.main_max != Constraints::INFINITY;

        // Children without weight, with what's left of the main axis, each followed by the
        // spacing, clamped to what's left, as Compose's `RowColumnMeasurePolicy`.
        let mut total_weight = 0.0f32;
        let mut weighted = 0i32;
        let mut fixed_space = 0i32;
        let mut cross_size = 0i32;
        let mut space_after_last = 0i32;
        for (index, &measurable) in measurables.iter().enumerate() {
            if let Some((weight, _)) = scope.parent_data(measurable).weight {
                total_weight += weight;
                weighted += 1;
                continue;
            }
            let remaining =
                if bounded { (axis.main_max - fixed_space).max(0) } else { Constraints::INFINITY };
            let child = AxisConstraints {
                main_min: 0,
                main_max: remaining,
                cross_min: 0,
                cross_max: axis.cross_max,
            };
            let placeable = scope.measure(measurable, child.to_constraints(self.orientation));
            let size = self.main(&placeable);
            space_after_last =
                if bounded { spacing.min((remaining - size).max(0)) } else { spacing };
            fixed_space = fixed_space.saturating_add(size + space_after_last);
            cross_size = cross_size.max(self.cross(&placeable));
            placeables[index] = Some(placeable);
        }

        // Weighted children share what remains of the maximum (of the minimum when the main
        // axis is unbounded), rounding as Compose does: each takes its share rounded, and the
        // remainder goes one pixel at a time to the first ones.
        let mut weighted_space = 0i32;
        if weighted == 0 {
            fixed_space -= space_after_last;
        } else {
            let target = if total_weight > 0.0 && bounded { axis.main_max } else { axis.main_min };
            let weighted_gaps = spacing * (weighted - 1);
            let remaining = (target - fixed_space - weighted_gaps).max(0);
            let per_weight = if total_weight > 0.0 { remaining as f32 / total_weight } else { 0.0 };
            let mut rounding = remaining
                - measurables
                    .iter()
                    .filter_map(|&measurable| scope.parent_data(measurable).weight)
                    .map(|(weight, _)| round_to_int(weight * per_weight))
                    .sum::<i32>();
            for (index, &measurable) in measurables.iter().enumerate() {
                let Some((weight, fill)) = scope.parent_data(measurable).weight else { continue };
                let step = rounding.signum();
                rounding -= step;
                let size = (round_to_int(weight * per_weight) + step).max(0);
                let child = AxisConstraints {
                    main_min: if fill { size } else { 0 },
                    main_max: size,
                    cross_min: 0,
                    cross_max: axis.cross_max,
                };
                let placeable = scope.measure(measurable, child.to_constraints(self.orientation));
                weighted_space += self.main(&placeable);
                cross_size = cross_size.max(self.cross(&placeable));
                placeables[index] = Some(placeable);
            }
            let room = if bounded { axis.main_max - fixed_space } else { i32::MAX };
            weighted_space = (weighted_space + weighted_gaps).clamp(0, room.max(0));
        }

        let main_size = (fixed_space + weighted_space).max(0).max(axis.main_min).min(axis.main_max);
        let cross_size = cross_size.max(axis.cross_min).min(axis.cross_max);

        // Main-axis positions from the arrangement, cross-axis ones from the alignments.
        let mut sizes: Buffer<i32> = Buffer::new(count);
        let sizes = sizes.as_mut();
        for (index, placeable) in placeables.iter().enumerate() {
            sizes[index] = placeable.as_ref().map_or(0, |placeable| self.main(placeable));
        }
        let mut positions: Buffer<i32> = Buffer::new(count);
        let positions = positions.as_mut();
        // A column's arrangement runs from the top whatever the layout direction.
        let direction = match self.orientation {
            Orientation::Horizontal => scope.layout_direction(),
            Orientation::Vertical => compose_ui::LayoutDirection::Ltr,
        };
        self.arrangement.arrange(main_size, sizes, direction, &density, positions);
        for (index, &measurable) in measurables.iter().enumerate() {
            let Some(placeable) = placeables[index] else { continue };
            let mut bias = self.cross_bias_of(scope, measurable);
            // A column's start is on the right in a right-to-left layout.
            if self.orientation == Orientation::Vertical
                && scope.layout_direction() == compose_ui::LayoutDirection::Rtl
            {
                bias = -bias;
            }
            let cross = VerticalAlignment { bias }.align(self.cross(&placeable), cross_size);
            match self.orientation {
                Orientation::Horizontal => scope.place(&placeable, positions[index], cross),
                Orientation::Vertical => scope.place(&placeable, cross, positions[index]),
            }
        }
        match self.orientation {
            Orientation::Horizontal => scope.layout(main_size, cross_size),
            Orientation::Vertical => scope.layout(cross_size, main_size),
        }
    }

    // As Compose's `RowMeasurePolicy` and `ColumnMeasurePolicy` (`IntrinsicMeasureBlocks`):
    // along the main axis, the children's sizes and the spacing, weighted children at the
    // largest size per weight; across it, the largest child at the main-axis size it'd get.
    fn min_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        self.intrinsic(scope, measurables, height, false, true)
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        self.intrinsic(scope, measurables, width, false, false)
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        self.intrinsic(scope, measurables, height, true, true)
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        self.intrinsic(scope, measurables, width, true, false)
    }
}

impl RowColumnMeasurePolicy {
    /// The minimum (or `max`imum) intrinsic width (`width`) or height at `available` on the
    /// other axis: the main axis' size when asked along it, else the cross axis'.
    fn intrinsic(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        available: i32,
        max: bool,
        width: bool,
    ) -> i32 {
        let spacing = self.arrangement.spacing(&scope.density());
        let along_main = width == (self.orientation == Orientation::Horizontal);
        if along_main {
            intrinsic_main_axis_size(
                scope,
                measurables,
                available,
                spacing,
                |scope, child, cross| child_intrinsic(scope, child, max, width, cross),
            )
        } else {
            // The main-axis size a child takes is its maximum intrinsic, as Compose asks.
            intrinsic_cross_axis_size(
                scope,
                measurables,
                available,
                spacing,
                |scope, child, cross| child_intrinsic(scope, child, true, !width, cross),
                |scope, child, main| child_intrinsic(scope, child, max, width, main),
            )
        }
    }
}

/// `child`'s minimum (or `max`imum) intrinsic width (`width`) or height at `other`.
fn child_intrinsic(
    scope: &mut IntrinsicMeasureScope<'_>,
    child: IntrinsicMeasurable,
    max: bool,
    width: bool,
    other: i32,
) -> i32 {
    match (max, width) {
        (false, true) => scope.min_intrinsic_width(child, other),
        (false, false) => scope.min_intrinsic_height(child, other),
        (true, true) => scope.max_intrinsic_width(child, other),
        (true, false) => scope.max_intrinsic_height(child, other),
    }
}

/// A child's weight, if it has one (a positive one), as `RowColumnParentData.weight`.
fn weight_of(scope: &IntrinsicMeasureScope<'_>, child: IntrinsicMeasurable) -> f32 {
    scope.parent_data(child).weight.map_or(0.0, |(weight, _)| weight)
}

/// As Compose's `intrinsicMainAxisSize`: the children's main-axis sizes at `cross_available`
/// plus the spacing, the weighted ones at the largest main-axis size per weight.
fn intrinsic_main_axis_size(
    scope: &mut IntrinsicMeasureScope<'_>,
    children: &[IntrinsicMeasurable],
    cross_available: i32,
    spacing: i32,
    mut main_axis_size: impl FnMut(&mut IntrinsicMeasureScope<'_>, IntrinsicMeasurable, i32) -> i32,
) -> i32 {
    if children.is_empty() {
        return 0;
    }
    let mut weight_unit_space = 0;
    let mut fixed_space = 0;
    let mut total_weight = 0.0f32;
    for &child in children {
        let weight = weight_of(scope, child);
        let size = main_axis_size(scope, child, cross_available);
        if weight == 0.0 {
            fixed_space += size;
        } else if weight > 0.0 {
            total_weight += weight;
            weight_unit_space = weight_unit_space.max(round_to_int(size as f32 / weight));
        }
    }
    round_to_int(weight_unit_space as f32 * total_weight)
        + fixed_space
        + (children.len() as i32 - 1) * spacing
}

/// As Compose's `intrinsicCrossAxisSize`: the largest cross-axis size of the children, each
/// at the main-axis space it would get (its own maximum, within what's left of
/// `main_available`; for weighted ones, their share of what remains).
fn intrinsic_cross_axis_size(
    scope: &mut IntrinsicMeasureScope<'_>,
    children: &[IntrinsicMeasurable],
    main_available: i32,
    spacing: i32,
    mut main_axis_size: impl FnMut(&mut IntrinsicMeasureScope<'_>, IntrinsicMeasurable, i32) -> i32,
    mut cross_axis_size: impl FnMut(&mut IntrinsicMeasureScope<'_>, IntrinsicMeasurable, i32) -> i32,
) -> i32 {
    if children.is_empty() {
        return 0;
    }
    let mut fixed_space = ((children.len() as i32 - 1) * spacing).min(main_available);
    let mut cross_axis_max = 0;
    let mut total_weight = 0.0f32;
    for &child in children {
        let weight = weight_of(scope, child);
        if weight == 0.0 {
            let remaining = if main_available == Constraints::INFINITY {
                Constraints::INFINITY
            } else {
                main_available - fixed_space
            };
            let main_axis_space =
                main_axis_size(scope, child, Constraints::INFINITY).min(remaining);
            fixed_space += main_axis_space;
            cross_axis_max = cross_axis_max.max(cross_axis_size(scope, child, main_axis_space));
        } else if weight > 0.0 {
            total_weight += weight;
        }
    }
    let weight_unit_space = if total_weight == 0.0 {
        0
    } else if main_available == Constraints::INFINITY {
        Constraints::INFINITY
    } else {
        round_to_int((main_available - fixed_space).max(0) as f32 / total_weight)
    };
    for &child in children {
        let weight = weight_of(scope, child);
        if weight > 0.0 {
            let main = if weight_unit_space != Constraints::INFINITY {
                round_to_int(weight_unit_space as f32 * weight)
            } else {
                Constraints::INFINITY
            };
            cross_axis_max = cross_axis_max.max(cross_axis_size(scope, child, main));
        }
    }
    cross_axis_max
}

/// What `BoxWithConstraints`' content knows: the constraints the box was measured with, as
/// Compose's `BoxWithConstraintsScope`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BoxWithConstraintsScope {
    pub constraints: compose_ui::Constraints,
    density: compose_ui::Density,
}

impl BoxWithConstraintsScope {
    /// The smallest width allowed, as `minWidth`.
    pub fn min_width(&self) -> compose_ui::Dp {
        compose_ui::Dp(self.constraints.min_width as f32 / self.density.density)
    }

    /// The largest width allowed (`Dp::INFINITY` if unbounded), as `maxWidth`.
    pub fn max_width(&self) -> compose_ui::Dp {
        if self.constraints.max_width == compose_ui::Constraints::INFINITY {
            compose_ui::Dp(f32::INFINITY)
        } else {
            compose_ui::Dp(self.constraints.max_width as f32 / self.density.density)
        }
    }

    pub fn min_height(&self) -> compose_ui::Dp {
        compose_ui::Dp(self.constraints.min_height as f32 / self.density.density)
    }

    pub fn max_height(&self) -> compose_ui::Dp {
        if self.constraints.max_height == compose_ui::Constraints::INFINITY {
            compose_ui::Dp(f32::INFINITY)
        } else {
            compose_ui::Dp(self.constraints.max_height as f32 / self.density.density)
        }
    }
}

impl compose_runtime::ComposeArg for BoxWithConstraintsScope {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A box whose content depends on its constraints, as Compose's `BoxWithConstraints`: the
/// content reads them from its scope. Compose composes it while measuring; here it first
/// composes for the constraints it'll likely get (`LOCAL_CONSTRAINTS_HINT`: the window's, a
/// lazy list's items', a grid's cells'), then measuring writes those it got as state and the
/// content composes again within the frame if they differ.
#[composable]
pub fn BoxWithConstraints(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default(false)] propagate_min_constraints: bool,
    content: impl Fn(BoxWithConstraintsScope) + 'static,
) {
    // First the constraints it'll likely get (`LOCAL_CONSTRAINTS_HINT`), then those it got.
    let hint = compose_ui::platform::LOCAL_CONSTRAINTS_HINT.current();
    let measured = compose_runtime::remember(|| {
        compose_runtime::mutable_state_of(hint.unwrap_or(compose_ui::Constraints::fixed(0, 0)))
    });
    let policy = compose_runtime::remember_keyed(
        (AlignmentKey(content_alignment), propagate_min_constraints),
        || {
            Rc::new(BoxWithConstraintsPolicy {
                inner: BoxMeasurePolicy { alignment: content_alignment, propagate_min_constraints },
                measured,
            }) as Rc<dyn compose_ui::MeasurePolicy>
        },
    );
    let density = compose_ui::LOCAL_DENSITY.current();
    Layout(
        modifier,
        policy,
        compose_runtime::callback(move || {
            let constraints = measured.get();
            compose_runtime::CompositionLocalProvider(
                compose_ui::platform::LOCAL_CONSTRAINTS_HINT.provides(Some(constraints.loosen())),
                || content(BoxWithConstraintsScope { constraints, density }),
            )
        }),
    );
}

#[derive(Clone, Copy, PartialEq)]
struct AlignmentKey(Alignment);

impl compose_runtime::ComposeArg for AlignmentKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A box's policy that also writes the constraints it's measured with, for the content.
struct BoxWithConstraintsPolicy {
    inner: BoxMeasurePolicy,
    measured: compose_runtime::MutableState<compose_ui::Constraints>,
}

impl compose_ui::MeasurePolicy for BoxWithConstraintsPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        if self.measured.get() != constraints {
            self.measured.set(constraints);
        }
        self.inner.measure(scope, measurables, constraints)
    }

    // As Compose's `SubcomposeLayout`: it composes while measuring, so it has no intrinsics.
    fn min_intrinsic_width(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn min_intrinsic_height(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_width(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_height(
        &self,
        _: &mut compose_ui::IntrinsicMeasureScope<'_>,
        _: &[compose_ui::IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", compose_ui::platform::NO_INTRINSICS_MESSAGE)
    }

    fn debug_name(&self) -> &'static str {
        "BoxWithConstraints"
    }
}

/// A node to draw in, as Compose's `Canvas`: `on_draw` draws behind nothing, at the size
/// `modifier` gives it. A plain function (its draw lambda takes a borrowed scope, which
/// composable parameters can't), called as one: `Canvas(Modifier.size(…), |scope| …)`.
#[allow(non_snake_case)]
pub fn Canvas(modifier: Modifier, on_draw: impl Fn(&mut compose_ui::DrawScope<'_>) + 'static) {
    Spacer(modifier.draw_behind(on_draw));
}

/// `Canvas` described to accessibility as `content_description`, as Compose's
/// `Canvas(modifier, contentDescription, onDraw)`.
#[allow(non_snake_case)]
pub fn Canvas_with(
    modifier: Modifier,
    content_description: impl Into<compose_ui::SharedString>,
    on_draw: impl Fn(&mut compose_ui::DrawScope<'_>) + 'static,
) {
    let content_description = content_description.into();
    Spacer(
        modifier
            .draw_behind(on_draw)
            .semantics(move |config| config.set_content_description(content_description.clone())),
    );
}

/// A row: its children side by side, as Compose's `Row`.
#[composable]
pub fn Row(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::layout::Arrangement::START)] horizontal_arrangement: impl Into<
        HorizontalArrangement,
    >,
    #[default(crate::Alignment::TOP)] vertical_alignment: VerticalAlignment,
    content: impl Fn(RowScope) + 'static,
) {
    let policy = remember_keyed((horizontal_arrangement.clone(), vertical_alignment), || {
        Rc::new(RowColumnMeasurePolicy {
            orientation: Orientation::Horizontal,
            arrangement: horizontal_arrangement.0,
            cross_bias: vertical_alignment.bias,
        }) as Rc<dyn MeasurePolicy>
    });
    Layout(modifier, policy, compose_runtime::callback(move || content(RowScope::__new())));
}

/// A column: its children one below the other, as Compose's `Column`.
#[composable]
pub fn Column(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::layout::Arrangement::TOP)] vertical_arrangement: impl Into<VerticalArrangement>,
    #[default(crate::Alignment::START)] horizontal_alignment: HorizontalAlignment,
    content: impl Fn(ColumnScope) + 'static,
) {
    let policy = remember_keyed((vertical_arrangement.clone(), horizontal_alignment), || {
        Rc::new(RowColumnMeasurePolicy {
            orientation: Orientation::Vertical,
            arrangement: vertical_arrangement.0,
            cross_bias: horizontal_alignment.bias,
        }) as Rc<dyn MeasurePolicy>
    });
    Layout(modifier, policy, compose_runtime::callback(move || content(ColumnScope::__new())));
}

// ---- Box and Spacer --------------------------------------------------------------------------

/// `Box`'s measure policy, as Compose's: children stacked, each aligned by its own `align` or
/// the box's content alignment; the box as large as its largest child, within its constraints.
pub(crate) struct BoxMeasurePolicy {
    alignment: Alignment,
    propagate_min_constraints: bool,
}

impl MeasurePolicy for BoxMeasurePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        if measurables.is_empty() {
            return scope.layout(constraints.min_width, constraints.min_height);
        }
        let child_constraints =
            if self.propagate_min_constraints { constraints } else { constraints.loosen() };
        let count = measurables.len();
        let mut placeables: Buffer<Option<Placeable>> = Buffer::new(count);
        let placeables = placeables.as_mut();
        let (mut width, mut height) = (constraints.min_width, constraints.min_height);
        for (index, &measurable) in measurables.iter().enumerate() {
            let placeable = scope.measure(measurable, child_constraints);
            width = width.max(placeable.width);
            height = height.max(placeable.height);
            placeables[index] = Some(placeable);
        }
        let space = compose_ui::IntSize::new(width, height);
        for (index, &measurable) in measurables.iter().enumerate() {
            let Some(placeable) = placeables[index] else { continue };
            let alignment = match scope.parent_data(measurable).alignment {
                Some(ParentAlignment::Box(alignment)) => alignment,
                _ => self.alignment,
            };
            let position = alignment.align(placeable.size(), space, scope.layout_direction());
            scope.place_at(&placeable, position);
        }
        scope.layout(width, height)
    }
}

/// A box: its children stacked, as Compose's `Box`. `Box` the composable lives in the value
/// namespace, `std::boxed::Box` in the type namespace, so `Box::new` still works.
#[composable]
pub fn Box(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Alignment::TOP_START)] content_alignment: Alignment,
    #[default(false)] propagate_min_constraints: bool,
    #[default(|_| {})] content: impl Fn(BoxScope) + 'static,
) {
    let policy = remember_keyed((content_alignment, propagate_min_constraints), || {
        Rc::new(BoxMeasurePolicy { alignment: content_alignment, propagate_min_constraints })
            as Rc<dyn MeasurePolicy>
    });
    Layout(modifier, policy, compose_runtime::callback(move || content(BoxScope::__new())));
}

/// Empty space of the size its modifier gives it, as Compose's `Spacer` (whose modifier has no
/// default; `Spacer!()` is an empty space of no size).
#[composable]
pub fn Spacer(#[default(crate::Modifier)] modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), compose_runtime::callback(|| {}));
}
