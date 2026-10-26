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
