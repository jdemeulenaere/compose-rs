//! Helpers for the lazy list and grid ports: Compose's lazy tests' `LazyColumnOrRow` and their
//! orientation-aware padding, in a test of a given orientation. Private to this suite; at
//! density 1, so Compose's tests' pixels are dps here.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_ui::{Dp, PaddingValues, UnitExt};

/// What Compose's `BaseLazyListTestWithOrientation` parameterizes, and `LazyColumnOrRow`'s
/// arguments.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Lazy {
    pub vertical: bool,
    pub reverse_layout: bool,
    /// The arrangement from the other end (Compose's `reverseArrangement`).
    pub reverse_arrangement: bool,
    pub spaced_by: f32,
    pub user_scroll_disabled: bool,
}

impl Lazy {
    pub fn column() -> Lazy {
        Lazy { vertical: true, ..Lazy::default() }
    }

    pub fn row() -> Lazy {
        Lazy { vertical: false, ..Lazy::default() }
    }

    /// The four cases of Compose's layout info tests: each orientation, reversed or not.
    pub fn all() -> [Lazy; 4] {
        let reversed = |lazy: Lazy| Lazy { reverse_layout: true, ..lazy };
        [Lazy::column(), reversed(Lazy::column()), Lazy::row(), reversed(Lazy::row())]
    }

    /// Padding before and after the content along the axis, and across it, as Compose's
    /// `PaddingValues(beforeContent, afterContent, beforeContentCrossAxis, afterContentCrossAxis)`
    /// (physical: "before" is the top or the start whatever the reversal).
    pub fn padding(
        &self,
        before: Dp,
        after: Dp,
        cross_before: Dp,
        cross_after: Dp,
    ) -> PaddingValues {
        if self.vertical {
            PaddingValues { start: cross_before, top: before, end: cross_after, bottom: after }
        } else {
            PaddingValues { start: before, top: cross_before, end: after, bottom: cross_after }
        }
    }

    /// A modifier sizing the main axis and the cross axis.
    pub fn sized(&self, main: Dp, cross: Dp) -> Modifier {
        if self.vertical { Modifier.size((cross, main)) } else { Modifier.size((main, cross)) }
    }
}

/// A `LazyColumn` or a `LazyRow`, as Compose's tests' `LazyColumnOrRow`: the arrangement from
/// the bottom (the end) when reversed xor `reverse_arrangement`, or spaced by `spaced_by`.
pub fn LazyColumnOrRow(
    lazy: Lazy,
    modifier: Modifier,
    state: LazyListState,
    content_padding: PaddingValues,
    content: impl Fn(LazyListScope) + 'static,
) {
    let content = std::rc::Rc::new(content);
    let from_end = lazy.reverse_layout ^ lazy.reverse_arrangement;
    if lazy.vertical {
        let arrangement: VerticalArrangement = if lazy.spaced_by != 0.0 {
            Arrangement::spaced_by(Dp(lazy.spaced_by)).into()
        } else if from_end {
            Arrangement::BOTTOM
        } else {
            Arrangement::TOP
        };
        LazyColumn!(
            modifier = modifier,
            state = state,
            content_padding = content_padding,
            reverse_layout = lazy.reverse_layout,
            vertical_arrangement = arrangement,
            user_scroll_enabled = !lazy.user_scroll_disabled,
            move |scope: LazyListScope| content(scope)
        );
    } else {
        let arrangement: HorizontalArrangement = if lazy.spaced_by != 0.0 {
            Arrangement::spaced_by(Dp(lazy.spaced_by)).into()
        } else if from_end {
            Arrangement::END
        } else {
            Arrangement::START
        };
        LazyRow!(
            modifier = modifier,
            state = state,
            content_padding = content_padding,
            reverse_layout = lazy.reverse_layout,
            horizontal_arrangement = arrangement,
            user_scroll_enabled = !lazy.user_scroll_disabled,
            move |scope: LazyListScope| content(scope)
        );
    }
}

/// No padding.
pub fn no_padding() -> PaddingValues {
    PaddingValues::ZERO
}

/// `n.dp()` for an `i32` in pixels at density 1.
pub fn px(n: i32) -> Dp {
    n.dp()
}

/// The bounds in the root of the node tagged `tag` (unclipped), as Compose's
/// `getUnclippedBoundsInRoot`.
pub fn bounds(ui: &mut compose_test::ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

impl Lazy {
    /// Where the node tagged `tag` starts along the axis (its top or its left), in the root, as
    /// Compose's tests' `assertMainAxisStartPositionInRootIsEqualTo` reads it.
    pub fn main_start(&self, ui: &mut compose_test::ComposeTestRule, tag: &str) -> f32 {
        let rect = bounds(ui, tag);
        if self.vertical { rect.top } else { rect.left }
    }

    /// Where the node tagged `tag` starts across the axis, in the root.
    pub fn cross_start(&self, ui: &mut compose_test::ComposeTestRule, tag: &str) -> f32 {
        let rect = bounds(ui, tag);
        if self.vertical { rect.left } else { rect.top }
    }

    /// The node's length along the axis.
    pub fn main_size(&self, ui: &mut compose_test::ComposeTestRule, tag: &str) -> f32 {
        let rect = bounds(ui, tag);
        if self.vertical { rect.height() } else { rect.width() }
    }

    /// The node's length across the axis.
    pub fn cross_size(&self, ui: &mut compose_test::ComposeTestRule, tag: &str) -> f32 {
        let rect = bounds(ui, tag);
        if self.vertical { rect.width() } else { rect.height() }
    }

    /// Scrolls the node tagged `tag` by `distance` along the axis with the semantics `ScrollBy`
    /// action, as Compose's tests' `scrollMainAxisBy`, then lets the UI settle.
    pub fn scroll_main_axis_by(
        &self,
        ui: &mut compose_test::ComposeTestRule,
        tag: &str,
        distance: f32,
    ) {
        let vertical = self.vertical;
        ui.on_node_with_tag(tag).perform_semantics_action_with(
            &compose_ui::SemanticsActions::SCROLL_BY,
            |scroll| {
                let (x, y) = if vertical { (0.0, distance) } else { (distance, 0.0) };
                scroll(x, y);
            },
        );
        ui.wait_for_idle();
    }

    /// Drags the node tagged `tag` by `distance` along the axis (positive: towards the content's
    /// end, the finger moving the other way) without a fling, as Compose's tests' touch
    /// `scrollBy(x, y, density)`: past the touch slop (8 dp), then the distance, a pause, a
    /// release.
    pub fn drag_main_axis_by(
        &self,
        ui: &mut compose_test::ComposeTestRule,
        tag: &str,
        distance: f32,
    ) {
        let rect = bounds(ui, tag);
        let center =
            compose_ui::Offset::new((rect.left + rect.right) / 2.0, (rect.top + rect.bottom) / 2.0);
        let along = |amount: f32| {
            if self.vertical {
                compose_ui::Offset::new(0.0, amount)
            } else {
                compose_ui::Offset::new(amount, 0.0)
            }
        };
        let slop = 8.0f32.copysign(distance);
        let mut finger = crate::scroll_support::Touch::down(ui, center);
        finger.move_by(ui, along(-slop));
        finger.move_by(ui, along(-distance));
        finger.advance_event_time(100);
        finger.up(ui);
        ui.wait_for_idle();
    }

    /// `fill_max_width` or `fill_max_height` across the axis, as Compose's tests'
    /// `fillMaxCrossAxis`.
    pub fn fill_max_cross_axis(&self) -> Modifier {
        if self.vertical { Modifier.fill_max_width() } else { Modifier.fill_max_height() }
    }

    /// The list's `fill_parent_max_*` along the axis, as Compose's tests' `fillParentMaxMainAxis`.
    pub fn fill_parent_max_main_axis(&self, scope: &LazyItemScope) -> Modifier {
        if self.vertical {
            Modifier.fill_parent_max_height(scope)
        } else {
            Modifier.fill_parent_max_width(scope)
        }
    }

    /// The list's `fill_parent_max_*` across the axis, as Compose's tests' `fillParentMaxCrossAxis`.
    pub fn fill_parent_max_cross_axis(&self, scope: &LazyItemScope) -> Modifier {
        if self.vertical {
            Modifier.fill_parent_max_width(scope)
        } else {
            Modifier.fill_parent_max_height(scope)
        }
    }
}

/// Whether the node tagged `tag` shows inside the node tagged `container`: it exists and some of
/// it is within the container's bounds, as Compose's `assertIsDisplayed` (its bounds clipped by
/// the scrolling container around it, non-empty).
pub fn is_displayed(ui: &mut compose_test::ComposeTestRule, tag: &str, container: &str) -> bool {
    if ui.count_nodes(&compose_test::has_test_tag(tag)) == 0 {
        return false;
    }
    let (item, around) = (bounds(ui, tag), bounds(ui, container));
    let width = item.right.min(around.right) - item.left.max(around.left);
    let height = item.bottom.min(around.bottom) - item.top.max(around.top);
    width > 0.0 && height > 0.0
}

impl Lazy {
    /// The two cases of Compose's grid tests compose-rs's grids take: each orientation, not
    /// reversed (grids have no `reverse_layout` yet: feature:grid-parity).
    pub fn grids() -> [Lazy; 2] {
        [Lazy::column(), Lazy::row()]
    }

    /// A modifier sizing the main axis only.
    pub fn main_axis_size(&self, main: Dp) -> Modifier {
        if self.vertical { Modifier.height(main) } else { Modifier.width(main) }
    }

    /// A modifier sizing the cross axis only.
    pub fn cross_axis_size(&self, cross: Dp) -> Modifier {
        if self.vertical { Modifier.width(cross) } else { Modifier.height(cross) }
    }
}

/// A `LazyVerticalGrid` or a `LazyHorizontalGrid` of `cells` fixed cells, as Compose's grid
/// tests' `LazyGrid`: the arrangement from the end with `reverse_arrangement`, or spaced by
/// `spaced_by` along the axis.
pub fn LazyGrid(
    lazy: Lazy,
    cells: usize,
    modifier: Modifier,
    state: LazyGridState,
    content_padding: PaddingValues,
    content: impl Fn(LazyGridScope) + 'static,
) {
    let content = std::rc::Rc::new(content);
    let from_end = lazy.reverse_layout ^ lazy.reverse_arrangement;
    if lazy.vertical {
        let arrangement: VerticalArrangement = if lazy.spaced_by != 0.0 {
            Arrangement::spaced_by(Dp(lazy.spaced_by)).into()
        } else if from_end {
            Arrangement::BOTTOM
        } else {
            Arrangement::TOP
        };
        LazyVerticalGrid!(
            GridCells::Fixed(cells),
            modifier = modifier,
            state = state,
            content_padding = content_padding,
            vertical_arrangement = arrangement,
            move |scope: LazyGridScope| content(scope)
        );
    } else {
        let arrangement: HorizontalArrangement = if lazy.spaced_by != 0.0 {
            Arrangement::spaced_by(Dp(lazy.spaced_by)).into()
        } else if from_end {
            Arrangement::END
        } else {
            Arrangement::START
        };
        LazyHorizontalGrid!(
            GridCells::Fixed(cells),
            modifier = modifier,
            state = state,
            content_padding = content_padding,
            horizontal_arrangement = arrangement,
            move |scope: LazyGridScope| content(scope)
        );
    }
}
