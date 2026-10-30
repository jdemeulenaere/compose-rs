//! Staggered grids, as Compose's `LazyStaggeredGridTest` and `LazyStaggeredGridLaneInfoTest`, in
//! both orientations, at density 1. Items are composed in rounds of a frame here, so what depends
//! on a measurement composing as it goes (`onlyOneInitialMeasurePass`) isn't ported.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Dp, IntOffset, IntSize, PaddingValues, UnitExt};

use crate::lazy_support::{Lazy, no_padding};
use crate::scroll_support::run;

const GRID: &str = "LazyStaggeredGridTag";
const ITEM: i32 = 50;

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

fn orientations() -> [Lazy; 2] {
    [Lazy::column(), Lazy::row()]
}

/// Compose's tests' `LazyStaggeredGrid`, with what these need of its arguments.
#[derive(Clone)]
struct Args {
    lazy: Lazy,
    cells: StaggeredGridCells,
    modifier: Modifier,
    state: Option<LazyStaggeredGridState>,
    padding: PaddingValues,
    main_spacing: f32,
    cross_spacing: f32,
    cross_center: bool,
    /// `spacedBy(cross_spacing, Alignment.End or Bottom)`.
    cross_end: bool,
    reverse: bool,
}

impl Args {
    fn new(lazy: Lazy, lanes: usize, modifier: Modifier) -> Args {
        Args {
            lazy,
            cells: StaggeredGridCells::Fixed(lanes),
            modifier,
            state: None,
            padding: no_padding(),
            main_spacing: 0.0,
            cross_spacing: 0.0,
            cross_center: false,
            cross_end: false,
            reverse: false,
        }
    }

    fn padding(mut self, padding: PaddingValues) -> Args {
        self.padding = padding;
        self
    }

    fn state(mut self, state: LazyStaggeredGridState) -> Args {
        self.state = Some(state);
        self
    }
}

fn Grid(args: Args, content: impl Fn(LazyStaggeredGridScope) + 'static) {
    let content = Rc::new(content);
    let remembered = remember_lazy_staggered_grid_state(0, 0);
    let state = args.state.unwrap_or(remembered);
    if args.lazy.vertical {
        LazyVerticalStaggeredGrid!(
            args.cells.clone(),
            modifier = args.modifier.clone(),
            state = state,
            content_padding = args.padding,
            reverse_layout = args.reverse,
            vertical_item_spacing = Dp(args.main_spacing),
            horizontal_arrangement = if args.cross_center {
                HorizontalArrangement::from(Arrangement::CENTER)
            } else if args.cross_end {
                Arrangement::spaced_by((Dp(args.cross_spacing), compose_ui::Alignment::END))
            } else {
                Arrangement::spaced_by(Dp(args.cross_spacing)).into()
            },
            move |scope: LazyStaggeredGridScope| content(scope)
        );
    } else {
        LazyHorizontalStaggeredGrid!(
            args.cells.clone(),
            modifier = args.modifier.clone(),
            state = state,
            content_padding = args.padding,
            reverse_layout = args.reverse,
            vertical_arrangement = if args.cross_center {
                VerticalArrangement::from(Arrangement::CENTER)
            } else if args.cross_end {
                Arrangement::spaced_by((Dp(args.cross_spacing), compose_ui::Alignment::BOTTOM))
            } else {
                Arrangement::spaced_by(Dp(args.cross_spacing)).into()
            },
            horizontal_item_spacing = Dp(args.main_spacing),
            move |scope: LazyStaggeredGridScope| content(scope)
        );
    }
}

/// `axisSize(crossAxis, mainAxis)`.
fn axis_size(lazy: Lazy, cross: i32, main: i32) -> Modifier {
    lazy.sized(Dp(main as f32), Dp(cross as f32))
}

fn at(lazy: Lazy, cross: i32, main: i32) -> IntOffset {
    if lazy.vertical { IntOffset::new(cross, main) } else { IntOffset::new(main, cross) }
}

fn sized(lazy: Lazy, cross: i32, main: i32) -> IntSize {
    if lazy.vertical { IntSize::new(cross, main) } else { IntSize::new(main, cross) }
}

/// A state of `Compose`'s, kept for the test.
fn state_cell() -> Rc<Cell<Option<LazyStaggeredGridState>>> {
    Rc::default()
}

fn state_of(cell: &Rc<Cell<Option<LazyStaggeredGridState>>>) -> LazyStaggeredGridState {
    cell.get().expect("composed")
}

fn scroll_by(ui: &mut ComposeTestRule, state: LazyStaggeredGridState, pixels: i32) {
    let _ = run(ui, async move { state.scroll_by(pixels as f32).await });
}

fn assert_at(ui: &mut ComposeTestRule, lazy: Lazy, tag: &str, cross: i32, main: i32) {
    assert!(ui.on_node_with_tag(tag).is_displayed(), "{tag} isn't displayed");
    assert_eq!(lazy.main_start(ui, tag), main as f32, "{tag} along the axis");
    assert_eq!(lazy.cross_start(ui, tag), cross as f32, "{tag} across the axis");
}

/// `Spacer(Modifier.axisSize(itemSizeDp, itemSizeDp * (index + 1)).testTag(index))`: taller and
/// taller.
fn growing(lazy: Lazy, index: usize) {
    Spacer(axis_size(lazy, ITEM, ITEM * (index as i32 + 1)).test_tag(index.to_string()));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt emptySpan_unset
#[test]
fn a_lane_not_set_is_unset() {
    assert_eq!(LazyStaggeredGridLaneInfo::new().get_lane(0), LazyStaggeredGridLaneInfo::LANE_UNSET);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt setLane
#[test]
fn a_lane_is_kept() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(0, 42);
    info.set_lane(1, 0);
    assert_eq!(info.get_lane(0), 42);
    assert_eq!(info.get_lane(1), 0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt setLane_beyondBound
#[test]
fn a_lane_past_the_bound_grows_it() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    let bound = info.upper_bound();
    info.set_lane(bound - 1, 42);
    info.set_lane(bound, 42);
    assert_eq!(info.get_lane(bound - 1), 42);
    assert_eq!(info.get_lane(bound), 42);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt setLane_largeNumber
#[test]
fn a_lane_at_a_large_index_moves_the_window() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(i32::MAX / 2, 42);
    assert_eq!(info.get_lane(i32::MAX / 2), 42);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt setLane_decreaseBound
#[test]
fn a_lane_far_back_moves_the_window_back() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(i32::MAX / 2, 42);
    info.set_lane(0, 42);
    assert_eq!(info.get_lane(i32::MAX / 2), -1);
    assert_eq!(info.get_lane(0), 42);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt setLane_negative
#[test]
#[should_panic(expected = "Negative lanes are not supported")]
fn a_lane_of_a_negative_item_panics() {
    LazyStaggeredGridLaneInfo::new().set_lane(-1, 0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt setLaneGaps
#[test]
fn gaps_are_kept_by_item() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(0, 0);
    info.set_lane(1, 1);
    info.set_gaps(0, Some(vec![42, 24]));
    info.set_gaps(1, Some(vec![12, 21]));
    assert_eq!(info.get_gaps(0), Some(&[42, 24][..]));
    assert_eq!(info.get_gaps(1), Some(&[12, 21][..]));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt missingLaneGaps
#[test]
fn another_items_gaps_are_none() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(42, 0);
    info.set_gaps(0, Some(vec![42, 24]));
    assert_eq!(info.get_gaps(42), None);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt clearLaneGaps
#[test]
fn gaps_can_be_cleared() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(42, 0);
    info.set_gaps(42, Some(vec![42, 24]));
    assert!(info.get_gaps(42).is_some());
    info.set_gaps(42, None);
    assert_eq!(info.get_gaps(42), None);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLaneInfoTest.kt resetOnLaneInfoContentMove
#[test]
fn gaps_out_of_the_window_are_forgotten() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(0, 0);
    info.set_gaps(0, Some(vec![42, 24]));
    info.set_lane(i32::MAX / 2, 1);
    info.set_gaps(0, None);
    assert_eq!(info.get_gaps(0), None);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt showsZeroItems
#[test]
fn a_grid_of_no_items_shows_nothing() {
    for lazy in orientations() {
        let kept = state_cell();
        let keep = kept.clone();
        let mut ui = rule(move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            keep.set(Some(state));
            Grid(Args::new(lazy, 3, Modifier.test_tag(GRID)).state(state), |_| {});
        });
        ui.on_node_with_tag(GRID).on_children().assert_count_equals(0);
        let state = state_of(&kept);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt showsOneItem
#[test]
fn a_grid_of_one_item_shows_it() {
    for lazy in orientations() {
        let kept = state_cell();
        let keep = kept.clone();
        let mut ui = rule(move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            keep.set(Some(state));
            Grid(Args::new(lazy, 3, Modifier).state(state), |scope| {
                scope.item(|| Spacer(Modifier.size(50.dp()).test_tag("item")));
            });
        });
        assert!(ui.on_node_with_tag("item").is_displayed());
        let state = state_of(&kept);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt distributesSingleLine
#[test]
fn three_items_take_a_lane_each() {
    for lazy in orientations() {
        let modifier = lazy.cross_axis_size(150.dp());
        let mut ui = rule(move || {
            Grid(Args::new(lazy, 3, modifier.clone()), |scope| {
                scope.items(3, |index| Spacer(Modifier.size(50.dp()).test_tag(index.to_string())));
            });
        });
        assert_at(&mut ui, lazy, "0", 0, 0);
        assert_at(&mut ui, lazy, "1", ITEM, 0);
        assert_at(&mut ui, lazy, "2", ITEM * 2, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt distributesTwoLines
#[test]
fn each_item_goes_in_the_lane_that_ends_first() {
    for lazy in orientations() {
        let modifier = lazy.cross_axis_size(150.dp());
        let mut ui = rule(move || {
            Grid(Args::new(lazy, 3, modifier.clone()), move |scope| {
                scope.items(6, move |index| growing(lazy, index));
            });
        });
        assert_at(&mut ui, lazy, "0", 0, 0);
        assert_at(&mut ui, lazy, "1", ITEM, 0);
        assert_at(&mut ui, lazy, "2", ITEM * 2, 0);
        assert_at(&mut ui, lazy, "3", 0, ITEM);
        assert_at(&mut ui, lazy, "4", ITEM, ITEM * 2);
        assert_at(&mut ui, lazy, "5", ITEM * 2, ITEM * 3);
    }
}

/// A grid three items across and one along, of `growing` items, and its state.
fn short_grid(lazy: Lazy) -> (ComposeTestRule, LazyStaggeredGridState) {
    let kept = state_cell();
    let keep = kept.clone();
    let ui = rule(move || {
        let state = remember_lazy_staggered_grid_state(0, 0);
        keep.set(Some(state));
        Grid(Args::new(lazy, 3, axis_size(lazy, ITEM * 3, ITEM)).state(state), move |scope| {
            scope.items(6, move |index| growing(lazy, index));
        });
    });
    let state = state_of(&kept);
    (ui, state)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt moreItemsDisplayedOnScroll
#[test]
fn a_scroll_shows_more_items() {
    for lazy in orientations() {
        let (mut ui, state) = short_grid(lazy);
        ui.on_node_with_tag("3").assert_does_not_exist();
        scroll_by(&mut ui, state, ITEM * 3);
        assert_at(&mut ui, lazy, "3", 0, -ITEM * 2);
        assert_at(&mut ui, lazy, "4", ITEM, -ITEM);
        assert_at(&mut ui, lazy, "5", ITEM * 2, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemSizeInLayoutInfo
#[test]
fn layout_info_has_each_items_size_and_offset() {
    for lazy in orientations() {
        let (mut ui, state) = short_grid(lazy);
        scroll_by(&mut ui, state, ITEM * 3);
        let items = state.layout_info().visible_items_info;
        assert_eq!(items.len(), 3);
        for (position, item) in items.iter().enumerate() {
            let position = position as i32;
            assert_eq!(item.index, 3 + position as usize);
            assert_eq!(item.size, sized(lazy, ITEM, ITEM * (4 + position)));
            assert_eq!(item.offset, at(lazy, ITEM * position, -ITEM * (2 - position)));
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemCanEmitZeroNodes
#[test]
fn items_may_emit_nothing() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            let modifier = axis_size(lazy, ITEM * 3, ITEM).test_tag(GRID);
            Grid(Args::new(lazy, 3, modifier), |scope| scope.items(6, |_| {}));
        });
        assert!(ui.on_node_with_tag(GRID).is_displayed());
        ui.on_node_with_tag(GRID).on_children().assert_count_equals(0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreHiddenOnScroll
#[test]
fn a_scroll_hides_items() {
    for lazy in orientations() {
        let (mut ui, state) = short_grid(lazy);
        assert!(ui.on_node_with_tag("0").is_displayed());
        scroll_by(&mut ui, state, ITEM * 3);
        for tag in ["0", "1", "2"] {
            assert!(!ui.on_node_with_tag(tag).is_displayed(), "{tag}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsArePresentedWhenScrollingBack
#[test]
fn a_scroll_back_presents_them_again() {
    for lazy in orientations() {
        let (mut ui, state) = short_grid(lazy);
        assert!(ui.on_node_with_tag("0").is_displayed());
        scroll_by(&mut ui, state, ITEM * 3);
        assert!(!ui.on_node_with_tag("0").is_displayed());
        scroll_by(&mut ui, state, -ITEM * 3);
        assert_at(&mut ui, lazy, "0", 0, 0);
    }
}

fn assert_bounds(
    ui: &mut ComposeTestRule,
    lazy: Lazy,
    tag: &str,
    (cross, main): (i32, i32),
    (cross_size, main_size): (i32, i32),
) {
    assert_at(ui, lazy, tag, cross, main);
    assert_eq!(lazy.main_size(ui, tag), main_size as f32, "{tag}'s size along the axis");
    assert_eq!(lazy.cross_size(ui, tag), cross_size as f32, "{tag}'s size across the axis");
}

/// A grid with a state kept for the test.
fn keeping(
    args: impl Fn(LazyStaggeredGridState) -> Args + 'static,
    initial: (usize, i32),
    content: impl Fn(LazyStaggeredGridScope) + Clone + 'static,
) -> (ComposeTestRule, LazyStaggeredGridState) {
    let kept = state_cell();
    let keep = kept.clone();
    let ui = rule(move || {
        let state = remember_lazy_staggered_grid_state(initial.0, initial.1);
        keep.set(Some(state));
        Grid(args(state), content.clone());
    });
    let state = state_of(&kept);
    (ui, state)
}

fn two_lanes(lazy: Lazy, main: i32) -> impl Fn(LazyStaggeredGridState) -> Args + 'static {
    move |state| Args::new(lazy, 2, axis_size(lazy, ITEM * 2, main).test_tag(GRID)).state(state)
}

fn three_lanes(lazy: Lazy, main: i32) -> impl Fn(LazyStaggeredGridState) -> Args + 'static {
    move |state| Args::new(lazy, 3, axis_size(lazy, ITEM * 3, main).test_tag(GRID)).state(state)
}

/// `Modifier.mainAxisSize(size).testTag(tag)`, as wide as its lane.
fn along(lazy: Lazy, size: i32, tag: impl ToString) {
    Spacer(lazy.main_axis_size(Dp(size as f32)).test_tag(tag.to_string()));
}

/// `first` is the first item's main size at each composition, then `count` items of `ITEM`.
fn one_changing(
    lazy: Lazy,
    first: compose_runtime::MutableState<i32>,
    count: usize,
) -> impl Fn(LazyStaggeredGridScope) + Clone + 'static {
    move |scope| {
        scope.item(move || {
            Spacer(axis_size(lazy, ITEM, first.get()).test_tag("0"));
        });
        scope.items(count, move |index| {
            Spacer(axis_size(lazy, ITEM, ITEM).test_tag((index + 1).to_string()));
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreCorrectedWhenSizeIncreased
#[test]
fn an_item_grown_out_of_view_changes_the_lanes_coming_back() {
    for lazy in orientations() {
        let first = mutable_state_of(ITEM);
        let (mut ui, state) =
            keeping(two_lanes(lazy, ITEM * 2), (0, 0), one_changing(lazy, first.handle(), 5));
        assert_bounds(&mut ui, lazy, "0", (0, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "1", (ITEM, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "2", (0, ITEM), (ITEM, ITEM));
        scroll_by(&mut ui, state, ITEM * 3);
        first.set(ITEM * 2);
        scroll_by(&mut ui, state, -ITEM * 3);
        assert_eq!(lazy.cross_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_size(&mut ui, "0"), (ITEM * 2) as f32);
        assert_bounds(&mut ui, lazy, "2", (ITEM, ITEM), (ITEM, ITEM));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreCorrectedWhenSizeDecreased
#[test]
fn an_item_shrunk_out_of_view_changes_the_lanes_coming_back() {
    for lazy in orientations() {
        let first = mutable_state_of(ITEM * 2);
        let (mut ui, state) =
            keeping(two_lanes(lazy, ITEM * 2), (0, 0), one_changing(lazy, first.handle(), 5));
        assert_bounds(&mut ui, lazy, "0", (0, 0), (ITEM, ITEM * 2));
        assert_bounds(&mut ui, lazy, "1", (ITEM, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "2", (ITEM, ITEM), (ITEM, ITEM));
        scroll_by(&mut ui, state, ITEM * 3);
        first.set(ITEM);
        scroll_by(&mut ui, state, -ITEM * 3);
        assert_eq!(lazy.cross_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_size(&mut ui, "0"), ITEM as f32);
        assert_bounds(&mut ui, lazy, "2", (0, ITEM), (ITEM, ITEM));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreCorrectedWhenItemCountIsIncreasedFromZero
#[test]
fn items_coming_to_an_empty_grid_show() {
    for lazy in orientations() {
        let count = mutable_state_of(0usize);
        let read = count.handle();
        let (mut ui, _state) = keeping(two_lanes(lazy, ITEM * 2), (0, 0), move |scope| {
            scope.items(read.get(), move |index| along(lazy, ITEM, index));
        });
        ui.on_node_with_tag("0").assert_does_not_exist();
        count.set(4);
        ui.wait_for_idle();
        assert!(ui.on_node_with_tag("0").is_displayed());
        assert!(ui.on_node_with_tag("1").is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreCorrectedWithWrongColumns
#[test]
fn an_initial_position_in_the_wrong_lane_is_corrected() {
    for lazy in orientations() {
        // Normally [0, 1][2, 3][4, 5].
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (3, ITEM / 2), move |scope| {
            scope.items(6, move |index| {
                Spacer(axis_size(lazy, ITEM, ITEM).test_tag(index.to_string()));
            });
        });
        ui.on_node_with_tag("0").assert_does_not_exist();
        assert_eq!(lazy.main_start(&mut ui, "3"), (-ITEM / 2) as f32);
        assert_eq!(lazy.main_size(&mut ui, "4"), ITEM as f32);
        assert_eq!(lazy.main_start(&mut ui, "4"), (-ITEM / 2) as f32);
        scroll_by(&mut ui, state, -ITEM * 3);
        assert_at(&mut ui, lazy, "0", 0, 0);
        assert_at(&mut ui, lazy, "1", ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreCorrectedWithAlignedOffsets
#[test]
fn offsets_aligned_after_a_change_are_corrected() {
    for lazy in orientations() {
        let expanded = mutable_state_of(false);
        let read = expanded.handle();
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (0, 0), move |scope| {
            scope.items(6, move |index| {
                along(lazy, if index % 2 == 1 && read.get() { ITEM * 2 } else { ITEM }, index);
            });
        });
        assert!(ui.on_node_with_tag("0").is_displayed());
        scroll_by(&mut ui, state, ITEM * 2);
        expanded.set(true);
        ui.wait_for_idle();
        scroll_by(&mut ui, state, -ITEM * 2);
        scroll_by(&mut ui, state, -ITEM);
        assert_at(&mut ui, lazy, "0", 0, 0);
        assert_at(&mut ui, lazy, "1", ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreCorrectedWhenItemIncreased
#[test]
fn an_item_grown_in_the_middle_is_corrected() {
    for lazy in orientations() {
        let expanded = mutable_state_of(false);
        let read = expanded.handle();
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (0, 0), move |scope| {
            scope.items(6, move |index| {
                along(lazy, if index == 3 && read.get() { ITEM * 2 } else { ITEM }, index);
            });
        });
        assert!(ui.on_node_with_tag("0").is_displayed());
        scroll_by(&mut ui, state, ITEM * 2);
        expanded.set(true);
        ui.wait_for_idle();
        scroll_by(&mut ui, state, -ITEM * 2);
        scroll_by(&mut ui, state, -ITEM);
        assert_at(&mut ui, lazy, "0", 0, 0);
        assert_at(&mut ui, lazy, "1", ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt addItems
#[test]
fn items_added_can_be_scrolled_to() {
    for lazy in orientations() {
        let count = mutable_state_of(1usize);
        let read = count.handle();
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (0, 0), move |scope| {
            scope.items(read.get(), move |index| {
                Spacer(axis_size(lazy, ITEM, ITEM).test_tag(index.to_string()));
            });
        });
        assert!(ui.on_node_with_tag("0").is_displayed());
        ui.on_node_with_tag("1").assert_does_not_exist();
        count.set(10);
        ui.wait_for_idle();
        scroll_by(&mut ui, state, ITEM * 10);
        assert!(ui.on_node_with_tag("8").is_displayed());
        assert!(ui.on_node_with_tag("9").is_displayed());
        count.set(20);
        ui.wait_for_idle();
        scroll_by(&mut ui, state, ITEM * 10);
        assert!(ui.on_node_with_tag("18").is_displayed());
        assert!(ui.on_node_with_tag("19").is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt removeItems
#[test]
fn items_removed_leave_those_that_remain_in_view() {
    for lazy in orientations() {
        let count = mutable_state_of(20usize);
        let read = count.handle();
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (0, 0), move |scope| {
            scope.items(read.get(), move |index| {
                Spacer(axis_size(lazy, ITEM, ITEM).test_tag(index.to_string()));
            });
        });
        scroll_by(&mut ui, state, ITEM * 20);
        assert!(ui.on_node_with_tag("18").is_displayed());
        assert!(ui.on_node_with_tag("19").is_displayed());
        count.set(10);
        ui.wait_for_idle();
        assert!(ui.on_node_with_tag("8").is_displayed());
        assert!(ui.on_node_with_tag("9").is_displayed());
        count.set(1);
        ui.wait_for_idle();
        assert!(ui.on_node_with_tag("0").is_displayed());
        assert!(!ui.on_node_with_tag("1").is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt resizingItems_maintainsScrollingRange
#[test]
fn items_resized_out_of_view_keep_the_range() {
    for lazy in orientations() {
        let sizes = mutable_state_of((0..10).map(|it| ITEM * (it % 4 + 1)).collect::<Vec<i32>>());
        let read = sizes.handle();
        let (mut ui, _state) = keeping(two_lanes(lazy, ITEM * 5), (0, 0), move |scope| {
            scope.items(read.get().len(), move |index| {
                Spacer(axis_size(lazy, ITEM, read.get()[index]).test_tag(index.to_string()));
            });
        });
        lazy.scroll_main_axis_by(&mut ui, GRID, (ITEM * 10) as f32);
        assert_eq!(lazy.main_size(&mut ui, "8"), read.get()[8] as f32);
        assert_eq!(lazy.main_size(&mut ui, "9"), read.get()[9] as f32);
        sizes.set(read.get().into_iter().rev().collect());
        ui.wait_for_idle();
        assert!(ui.on_node_with_tag("8").is_displayed());
        assert!(ui.on_node_with_tag("9").is_displayed());
        lazy.scroll_main_axis_by(&mut ui, GRID, -(ITEM * 10) as f32);
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt removingItems_maintainsCorrectOffsets
#[test]
fn items_removed_before_the_position_keep_offsets_right() {
    for lazy in orientations() {
        let count = mutable_state_of(20usize);
        let read = count.handle();
        let (mut ui, _state) = keeping(two_lanes(lazy, ITEM * 5), (10, 0), move |scope| {
            scope.items(read.get(), move |index| {
                Spacer(
                    axis_size(lazy, ITEM, ITEM * (index as i32 % 3 + 1))
                        .test_tag(index.to_string()),
                );
            });
        });
        count.set(3);
        ui.wait_for_idle();
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt staggeredGrid_supportsLargeIndices
#[test]
fn large_indices_scroll_back_and_forth() {
    for lazy in orientations() {
        let middle = (i32::MAX / 2) as usize;
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM * 5), (middle, 0), move |scope| {
            scope.items(i32::MAX as usize, move |index| {
                Spacer(axis_size(lazy, ITEM, ITEM).test_tag(index.to_string()));
            });
        });
        let (one, other) = (middle.to_string(), (middle + 1).to_string());
        assert_eq!(lazy.main_start(&mut ui, &one), 0.0);
        assert_eq!(lazy.main_start(&mut ui, &other), 0.0);
        let delta = ITEM * 5;
        scroll_by(&mut ui, state, -delta);
        scroll_by(&mut ui, state, delta * 2);
        scroll_by(&mut ui, state, -delta);
        assert_eq!(lazy.main_start(&mut ui, &one), 0.0);
        assert_eq!(lazy.main_start(&mut ui, &other), 0.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt screenRotate_oneItem_withAdaptiveCells_fillsContentCorrectly
#[test]
fn a_rotation_keeps_one_item_at_the_top() {
    for lazy in orientations() {
        let rotated = mutable_state_of(false);
        let read = rotated.handle();
        let (mut ui, state) = keeping(
            move |state| {
                let (cross, main) = if read.get() { (9, 6) } else { (6, 9) };
                let mut args =
                    Args::new(lazy, 1, axis_size(lazy, ITEM * cross, ITEM * main)).state(state);
                args.cells = StaggeredGridCells::Adaptive(Dp((ITEM * 3) as f32));
                args
            },
            (0, 0),
            move |scope| {
                scope.items(1, move |index| along(lazy, ITEM, index));
            },
        );
        for value in [true, false, true] {
            rotated.set(value);
            ui.wait_for_idle();
            assert_eq!(state.first_visible_item_index(), 0);
            assert_eq!(state.first_visible_item_scroll_offset(), 0);
            assert!(ui.on_node_with_tag("0").is_displayed());
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt screenRotate_twoItems_withAdaptiveCells_fillsContentCorrectly
#[test]
fn a_rotation_keeps_two_items_at_the_top() {
    for lazy in orientations() {
        let rotated = mutable_state_of(false);
        let read = rotated.handle();
        let (mut ui, state) = keeping(
            move |state| {
                let (cross, main) = if read.get() { (9, 6) } else { (6, 9) };
                let mut args =
                    Args::new(lazy, 1, axis_size(lazy, ITEM * cross, ITEM * main)).state(state);
                args.cells = StaggeredGridCells::Adaptive(Dp((ITEM * 3) as f32));
                args
            },
            (0, 0),
            move |scope| {
                scope.items(2, move |index| along(lazy, ITEM, index));
            },
        );
        for value in [true, false, true] {
            rotated.set(value);
            ui.wait_for_idle();
            assert_eq!(state.first_visible_item_index(), 0);
            assert_eq!(state.first_visible_item_scroll_offset(), 0);
            for tag in ["0", "1"] {
                assert!(ui.on_node_with_tag(tag).is_displayed());
            }
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fillingFullSize_nextItemIsNotComposed
#[test]
fn an_item_filling_the_grid_leaves_the_next_not_composed() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(
            move |state| Args::new(lazy, 1, lazy.main_axis_size(5.dp())).state(state),
            (0, 0),
            move |scope| {
                scope.items(3, |index| {
                    Box!(modifier = Modifier.size(5.dp()).test_tag(index.to_string()))
                });
            },
        );
        for index in 0..3 {
            assert!(ui.on_node_with_tag(&index.to_string()).is_displayed());
            ui.on_node_with_tag(&(index + 1).to_string()).assert_does_not_exist();
            scroll_by(&mut ui, state, 5);
        }
    }
}

/// `Modifier.testTag(tag).mainAxisSize(size)` in a `Box`.
fn tagged_box(lazy: Lazy, size: i32, tag: impl ToString) {
    Box!(modifier = Modifier.test_tag(tag.to_string()).then(lazy.main_axis_size(Dp(size as f32))));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpan_fillsAllCrossAxisSpace
#[test]
fn a_full_line_fills_the_cross_axis() {
    for lazy in orientations() {
        let (mut ui, _) = keeping(three_lanes(lazy, ITEM * 10), (0, 0), move |scope| {
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                tagged_box(lazy, ITEM, "0")
            });
        });
        assert_bounds(&mut ui, lazy, "0", (0, 0), (ITEM * 3, ITEM));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpan_leavesEmptyGapsWithOtherItems
#[test]
fn a_full_line_leaves_an_empty_lane_before_it() {
    for lazy in orientations() {
        let (mut ui, _) = keeping(three_lanes(lazy, ITEM * 10), (0, 0), move |scope| {
            scope.items(2, move |index| tagged_box(lazy, ITEM, index));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                tagged_box(lazy, ITEM, "full")
            });
        });
        assert_bounds(&mut ui, lazy, "0", (0, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "1", (ITEM, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "full", (0, ITEM), (ITEM * 3, ITEM));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpan_leavesGapsBetweenItems
#[test]
fn a_full_line_starts_past_the_longest_lane() {
    for lazy in orientations() {
        let (mut ui, _) = keeping(three_lanes(lazy, ITEM * 10), (0, 0), move |scope| {
            scope.items(3, move |index| tagged_box(lazy, ITEM + ITEM * index as i32 / 2, index));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                tagged_box(lazy, ITEM, "full")
            });
        });
        assert_bounds(&mut ui, lazy, "0", (0, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "1", (ITEM, 0), (ITEM, ITEM + ITEM / 2));
        assert_bounds(&mut ui, lazy, "2", (ITEM * 2, 0), (ITEM, ITEM * 2));
        assert_bounds(&mut ui, lazy, "full", (0, ITEM * 2), (ITEM * 3, ITEM));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpan_scrollsCorrectly
#[test]
fn a_full_line_scrolls_to_the_start() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(three_lanes(lazy, ITEM * 2), (0, 0), move |scope| {
            scope.items(3, move |index| tagged_box(lazy, ITEM + ITEM * index as i32 / 2, index));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                tagged_box(lazy, ITEM, "full")
            });
            scope
                .items(3, move |index| tagged_box(lazy, ITEM + ITEM * index as i32 / 2, index + 3));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                tagged_box(lazy, ITEM, "full-2")
            });
        });
        lazy.scroll_main_axis_by(&mut ui, GRID, (ITEM * 2) as f32);
        assert_bounds(&mut ui, lazy, "full", (0, 0), (ITEM * 3, ITEM));
        assert_eq!(state.first_visible_item_index(), 3);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

fn repeated_with_full_lines(lazy: Lazy) -> impl Fn(LazyStaggeredGridScope) + Clone + 'static {
    move |scope| {
        for repeat in 0..10 {
            scope.items(3, move |index| {
                tagged_box(lazy, ITEM + ITEM * index as i32 / 2, repeat * 3 + index)
            });
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                tagged_box(lazy, ITEM, format!("full-{repeat}"))
            });
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpan_scrollsCorrectly_pastFullSpan
#[test]
fn a_scroll_goes_past_a_full_line() {
    for lazy in orientations() {
        let (mut ui, state) =
            keeping(three_lanes(lazy, ITEM * 2), (0, 0), repeated_with_full_lines(lazy));
        lazy.scroll_main_axis_by(&mut ui, GRID, (ITEM * 3) as f32);
        assert_bounds(&mut ui, lazy, "3", (0, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "4", (ITEM, 0), (ITEM, ITEM + ITEM / 2));
        assert_bounds(&mut ui, lazy, "5", (ITEM * 2, 0), (ITEM, ITEM * 2));
        assert_eq!(state.first_visible_item_index(), 4);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpan_scrollsCorrectly_pastFullSpan_andBack
#[test]
fn a_scroll_goes_past_a_full_line_and_back() {
    for lazy in orientations() {
        let (mut ui, state) =
            keeping(three_lanes(lazy, ITEM * 2), (0, 0), repeated_with_full_lines(lazy));
        lazy.scroll_main_axis_by(&mut ui, GRID, (ITEM * 3) as f32);
        lazy.scroll_main_axis_by(&mut ui, GRID, -(ITEM * 3) as f32);
        assert_bounds(&mut ui, lazy, "0", (0, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "1", (ITEM, 0), (ITEM, ITEM + ITEM / 2));
        assert_bounds(&mut ui, lazy, "2", (ITEM * 2, 0), (ITEM, ITEM * 2));
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpan_scrollsCorrectly_multipleFullSpans
#[test]
fn full_lines_only_scroll_like_a_list() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(three_lanes(lazy, ITEM * 2), (0, 0), move |scope| {
            scope.items_with_span(
                10,
                |_| (),
                |_| StaggeredGridItemSpan::FullLine,
                move |index| tagged_box(lazy, ITEM, index),
            );
        });
        lazy.scroll_main_axis_by(&mut ui, GRID, (ITEM * 3) as f32);
        assert_bounds(&mut ui, lazy, "3", (0, 0), (ITEM * 3, ITEM));
        assert_bounds(&mut ui, lazy, "4", (0, ITEM), (ITEM * 3, ITEM));
        assert_eq!(state.first_visible_item_index(), 3);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        lazy.scroll_main_axis_by(&mut ui, GRID, (ITEM * 10) as f32);
        assert_bounds(&mut ui, lazy, "8", (0, 0), (ITEM * 3, ITEM));
        assert_bounds(&mut ui, lazy, "9", (0, ITEM), (ITEM * 3, ITEM));
        assert_eq!(state.first_visible_item_index(), 8);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

/// Item sizes of `initialIndex_largerThanItemCount`.
fn sized_six(index: usize) -> i32 {
    match index {
        0 | 3 => ITEM * 2,
        1 | 4 => ITEM + ITEM / 2,
        _ => ITEM,
    }
}

/// A box `ITEM * 4` along holding a grid at item 20, three lanes across.
fn at_twenty(
    lazy: Lazy,
    content: impl Fn(LazyStaggeredGridScope) + Clone + 'static,
) -> ComposeTestRule {
    rule(move || {
        let content = content.clone();
        let state = remember_lazy_staggered_grid_state(20, 0);
        Box!(modifier = lazy.main_axis_size(Dp((ITEM * 4) as f32)), {
            let modifier = lazy.cross_axis_size(Dp((ITEM * 3) as f32)).test_tag(GRID);
            Grid(Args::new(lazy, 3, modifier).state(state), content.clone());
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt initialIndex_largerThanItemCount_ordersItemsCorrectly_withFullSpan
#[test]
fn an_initial_index_past_the_items_orders_them_with_a_full_line() {
    for lazy in orientations() {
        let mut ui = at_twenty(lazy, move |scope| {
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                along(lazy, ITEM * 2, "full")
            });
            scope.items(6, move |index| along(lazy, sized_six(index), index));
        });
        assert_bounds(&mut ui, lazy, "full", (0, -ITEM), (ITEM * 3, ITEM * 2));
        assert_bounds(&mut ui, lazy, "0", (0, ITEM), (ITEM, ITEM * 2));
        assert_bounds(&mut ui, lazy, "1", (ITEM, ITEM), (ITEM, ITEM + ITEM / 2));
        assert_bounds(&mut ui, lazy, "2", (ITEM * 2, ITEM), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "3", (ITEM * 2, ITEM * 2), (ITEM, ITEM * 2));
        assert_bounds(&mut ui, lazy, "4", (ITEM, ITEM * 2 + ITEM / 2), (ITEM, ITEM + ITEM / 2));
        assert_bounds(&mut ui, lazy, "5", (0, ITEM * 3), (ITEM, ITEM));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt initialIndex_largerThanItemCount_ordersItemsCorrectly
#[test]
fn an_initial_index_past_the_items_orders_them() {
    for lazy in orientations() {
        let mut ui = at_twenty(lazy, move |scope| {
            scope.items(6, move |index| along(lazy, sized_six(index), index));
        });
        assert_eq!(lazy.main_size(&mut ui, GRID), (ITEM * 3) as f32);
        assert_bounds(&mut ui, lazy, "0", (0, 0), (ITEM, ITEM * 2));
        assert_bounds(&mut ui, lazy, "1", (ITEM, 0), (ITEM, ITEM + ITEM / 2));
        assert_bounds(&mut ui, lazy, "2", (ITEM * 2, 0), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "3", (ITEM * 2, ITEM), (ITEM, ITEM * 2));
        assert_bounds(&mut ui, lazy, "4", (ITEM, ITEM + ITEM / 2), (ITEM, ITEM + ITEM / 2));
        assert_bounds(&mut ui, lazy, "5", (0, ITEM * 2), (ITEM, ITEM));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt changeItemsAndScrollImmediately
#[test]
fn items_changing_and_a_scroll_at_once_keep_the_key() {
    for lazy in orientations() {
        let keys = mutable_state_of((0..10).collect::<Vec<i32>>());
        let read = keys.handle();
        let (mut ui, state) = keeping(
            move |state| Args::new(lazy, 2, lazy.main_axis_size(Dp(ITEM as f32))).state(state),
            (0, 0),
            move |scope| {
                scope.items_list_keyed(
                    read.get(),
                    |it: &i32| *it,
                    |_| {
                        Box!(modifier = Modifier.size(Dp((ITEM * 2) as f32)));
                    },
                );
            },
        );
        ui.wait_for_idle();
        let _ = run(&mut ui, async move { state.scroll_to_item(8, 0).await });
        assert_eq!(state.first_visible_item_index(), 8);
        let mut changed = read.get();
        changed.insert(0, -1);
        changed.insert(0, -2);
        keys.set(changed);
        run(&mut ui, async move {
            let _ = state.scroll_by(10.0).await;
            let _ = state.scroll_by(-10.0).await;
        });
        assert_eq!(state.first_visible_item_index(), 10);
    }
}
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt scrollPositionIsRestored
#[test]
fn the_position_is_restored() {
    for lazy in orientations() {
        let mut tester = StateRestorationTester::new();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), || {});
        tester.set_content(&mut ui, move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            let modifier = lazy.main_axis_size(Dp((ITEM * 10) as f32)).test_tag(GRID);
            Grid(Args::new(lazy, 3, modifier).state(state), move |scope| {
                scope.items(1000, move |index| along(lazy, ITEM, index));
            });
        });
        lazy.scroll_main_axis_by(&mut ui, GRID, (ITEM * 10) as f32);
        assert!(ui.on_node_with_tag("30").is_displayed());
        tester.emulate_save_and_restore(&mut ui);
        assert!(ui.on_node_with_tag("30").is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt restoredScrollPositionIsCorrectWhenItemsAreLoadedAsynchronously
#[test]
fn a_restored_position_waits_for_its_items() {
    for lazy in orientations() {
        let count = Rc::new(Cell::new(100usize));
        let again = mutable_state_of(0);
        let read = again.handle();
        let kept = state_cell();
        let (keep, items) = (kept.clone(), count.clone());
        let mut tester = StateRestorationTester::new();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), || {});
        tester.set_content(&mut ui, move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            keep.set(Some(state));
            let items = items.clone();
            let modifier = lazy.main_axis_size(Dp((ITEM * 10) as f32)).test_tag(GRID);
            Grid(Args::new(lazy, 3, modifier).state(state), move |scope| {
                read.get();
                scope.items(items.get(), move |index| along(lazy, ITEM, index));
            });
        });
        let state = state_of(&kept);
        let _ = run(&mut ui, async move { state.scroll_to_item(9, 10).await });
        count.set(0);
        tester.emulate_save_and_restore(&mut ui);
        count.set(100);
        again.set(1);
        ui.wait_for_idle();
        let state = state_of(&kept);
        assert_eq!(state.first_visible_item_index(), 9);
        assert_eq!(state.first_visible_item_scroll_offset(), 10);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt scrollingALot_layoutIsNotRecomposed
#[test]
fn scrolling_a_lot_doesnt_recompose_the_grids_caller() {
    for lazy in orientations() {
        let recomposed = Rc::new(Cell::new(0));
        let counted = recomposed.clone();
        let kept = state_cell();
        let keep = kept.clone();
        let mut ui = rule(move || {
            counted.set(counted.get() + 1);
            let state = remember_lazy_staggered_grid_state(0, 0);
            keep.set(Some(state));
            let modifier = lazy.main_axis_size(Dp((ITEM * 10) as f32));
            Grid(Args::new(lazy, 3, modifier).state(state), move |scope| {
                scope.items(1000, move |index| along(lazy, ITEM, index));
            });
        });
        ui.wait_for_idle();
        assert_eq!(recomposed.get(), 1);
        scroll_by(&mut ui, state_of(&kept), 1000);
        ui.wait_for_idle();
        assert_eq!(recomposed.get(), 1);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fixedSizeCell_forcesFixedSize
#[test]
fn a_fixed_size_cell_forces_its_size() {
    for lazy in orientations() {
        let (mut ui, _) = keeping(
            move |state| {
                let mut args = Args::new(lazy, 1, axis_size(lazy, ITEM * 5, ITEM * 5)).state(state);
                args.cells = StaggeredGridCells::FixedSize(Dp((ITEM * 2) as f32));
                args
            },
            (0, 0),
            move |scope| {
                scope.items(10, |index| {
                    Box!(modifier = Modifier.size(Dp(ITEM as f32)).test_tag(index.to_string()));
                });
            },
        );
        assert_eq!(lazy.cross_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.cross_size(&mut ui, "0"), (ITEM * 2) as f32);
        assert_eq!(lazy.cross_start(&mut ui, "1"), (ITEM * 2) as f32);
        assert_eq!(lazy.cross_size(&mut ui, "1"), (ITEM * 2) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt manyPlaceablesInItem_itemSizeIsMaxOfPlaceables
#[test]
fn an_item_of_several_layouts_is_as_large_as_the_largest() {
    for lazy in orientations() {
        let (mut ui, _) = keeping(two_lanes(lazy, ITEM * 5), (0, 0), move |scope| {
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, || {
                Box!(modifier = Modifier.size(Dp((ITEM * 2) as f32)));
                Box!(modifier = Modifier.size(Dp(ITEM as f32)));
            });
            scope.items(10, |index| {
                Box!(modifier = Modifier.size(Dp(ITEM as f32)).test_tag(index.to_string()));
            });
        });
        assert_bounds(&mut ui, lazy, "0", (0, ITEM * 2), (ITEM, ITEM));
        assert_bounds(&mut ui, lazy, "1", (ITEM, ITEM * 2), (ITEM, ITEM));
    }
}

fn one_lane_of_twenty(lazy: Lazy, state: LazyStaggeredGridState) {
    Grid(Args::new(lazy, 1, axis_size(lazy, ITEM * 2, ITEM * 2)).state(state), move |scope| {
        scope.items(20, move |index| along(lazy, ITEM, index));
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt scrollDuringMeasure
#[test]
fn a_scroll_in_a_box_with_constraints_lands() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            BoxWithConstraints!(move |_| {
                let state = remember_lazy_staggered_grid_state(0, 0);
                one_lane_of_twenty(lazy, state);
                LaunchedEffect(state, async move {
                    let _ = state.scroll_to_item(10, 0).await;
                });
            });
        });
        ui.wait_for_idle();
        assert_eq!(lazy.main_start(&mut ui, "10"), 0.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt scrollInLaunchedEffect
#[test]
fn a_scroll_in_a_launched_effect_lands() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            one_lane_of_twenty(lazy, state);
            LaunchedEffect(state, async move {
                let _ = state.scroll_to_item(10, 0).await;
            });
        });
        ui.wait_for_idle();
        assert_eq!(lazy.main_start(&mut ui, "10"), 0.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt scrollToPreviouslyFullSpanItem
#[test]
fn a_scroll_to_an_item_that_was_a_full_line_lands() {
    for lazy in orientations() {
        let first_item = mutable_state_of(false);
        let read = first_item.handle();
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (0, 0), move |scope| {
            if read.get() {
                scope.item(move || along(lazy, ITEM, "first"));
            }
            scope.items_with_span(
                20,
                |_| (),
                |index| {
                    if index == 10 {
                        StaggeredGridItemSpan::FullLine
                    } else {
                        StaggeredGridItemSpan::SingleLane
                    }
                },
                move |index| along(lazy, ITEM, index),
            );
        });
        let _ = run(&mut ui, async move { state.scroll_to_item(10, 0).await });
        first_item.set(true);
        run(&mut ui, async move {
            let _ = state.scroll_to_item(17, 0).await;
            let _ = state.scroll_to_item(10, 0).await;
        });
        assert_eq!(lazy.main_start(&mut ui, "9"), 0.0);
    }
}

/// A grid two lanes across and `ITEM * 1.5` along of 100 items noting those composed.
fn noting_composed(
    lazy: Lazy,
    initial: usize,
) -> (
    ComposeTestRule,
    LazyStaggeredGridState,
    Rc<std::cell::RefCell<std::collections::BTreeSet<usize>>>,
) {
    let composed: Rc<std::cell::RefCell<std::collections::BTreeSet<usize>>> = Rc::default();
    let noted = composed.clone();
    let (ui, state) = keeping(
        move |state| Args::new(lazy, 2, axis_size(lazy, ITEM * 2, ITEM + ITEM / 2)).state(state),
        (initial, 0),
        move |scope| {
            let noted = noted.clone();
            scope.items(100, move |index| {
                along(lazy, ITEM, "");
                let noted = noted.clone();
                DisposableEffect(index, move |effects| {
                    noted.borrow_mut().insert(index);
                    let noted = noted.clone();
                    effects.on_dispose(move || {
                        noted.borrow_mut().remove(&index);
                    })
                });
            });
        },
    );
    (ui, state, composed)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsRemovedAfterLargeThenSmallScrollForward
#[test]
fn items_scrolled_past_forward_leave_the_composition() {
    for lazy in orientations() {
        let (mut ui, state, composed) = noting_composed(lazy, 0);
        scroll_by(&mut ui, state, ITEM * 3);
        assert_eq!(state.first_visible_item_index(), 6);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        scroll_by(&mut ui, state, 10);
        assert_eq!(state.first_visible_item_index(), 6);
        assert_eq!(state.first_visible_item_scroll_offset(), 10);
        ui.wait_for_idle();
        let later = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while ui.host().prefetch(later) {
            ui.wait_for_idle();
        }
        assert_eq!(composed.borrow().iter().copied().collect::<Vec<_>>(), [6, 7, 8, 9]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsRemovedAfterLargeThenSmallScrollBackward
#[test]
fn items_scrolled_past_backward_leave_the_composition() {
    for lazy in orientations() {
        let (mut ui, state, composed) = noting_composed(lazy, 6);
        scroll_by(&mut ui, state, -(ITEM * 2 + ITEM / 2));
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), ITEM / 2);
        scroll_by(&mut ui, state, -5);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), ITEM / 2 - 5);
        ui.wait_for_idle();
        let later = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while ui.host().prefetch(later) {
            ui.wait_for_idle();
        }
        assert_eq!(composed.borrow().iter().copied().collect::<Vec<_>>(), [0, 1, 2, 3]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt zeroSizeItemIsPlacedWhenItIsAtTheTop
#[test]
fn an_item_of_no_size_at_the_top_is_placed() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(
            move |state| Args::new(lazy, 2, axis_size(lazy, ITEM * 2, ITEM * 2)).state(state),
            (0, 0),
            move |scope| {
                for repeat in 0..10 {
                    scope.items(2, move |index| {
                        Spacer(Modifier.test_tag((repeat * 10 + index).to_string()))
                    });
                    scope.items(8, move |_| along(lazy, ITEM, "other"));
                }
            },
        );
        for tag in ["0", "1"] {
            assert_eq!(lazy.main_start(&mut ui, tag), 0.0);
            assert_eq!(lazy.main_size(&mut ui, tag), 0.0);
        }
        let _ = run(&mut ui, async move { state.scroll_to_item(10, 0).await });
        for tag in ["10", "11"] {
            assert_eq!(lazy.main_start(&mut ui, tag), 0.0);
            assert_eq!(lazy.main_size(&mut ui, tag), 0.0);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt itemsAreDistributedCorrectlyOnOverscrollPassWithSameOffset
#[test]
fn items_stay_in_their_lanes_scrolling_past_the_end_again() {
    for lazy in orientations() {
        let height = ITEM * 11;
        let (mut ui, state) = keeping(
            move |state| Args::new(lazy, 2, axis_size(lazy, ITEM * 2, height)).state(state),
            (0, 0),
            move |scope| {
                scope.items(20, move |index| {
                    along(lazy, if index % 2 == 0 { ITEM * 5 } else { ITEM / 2 }, index)
                });
            },
        );
        scroll_by(&mut ui, state, height * 2);
        assert_at(&mut ui, lazy, "12", 0, 0);
        assert_at(&mut ui, lazy, "13", ITEM, 0);
        scroll_by(&mut ui, state, -ITEM * 5);
        scroll_by(&mut ui, state, height);
        assert_at(&mut ui, lazy, "12", 0, 0);
        assert_at(&mut ui, lazy, "13", ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fixedCells_withSpacing_notEnoughSpace
#[test]
fn lanes_with_no_room_for_their_spacing_are_measured() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            Box!(modifier = Modifier.size(Dp(ITEM as f32)), {
                let mut args = Args::new(lazy, 2, lazy.main_axis_size(Dp((ITEM * 5) as f32)));
                args.cross_spacing = (ITEM * 2) as f32;
                Grid(args, move |scope| {
                    scope.items(20, move |index| along(lazy, ITEM, index));
                });
            });
        });
        ui.wait_for_idle();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpanItemShowsCorrectly_afterASmallScrollThroughAGap
#[test]
fn a_full_line_shows_after_a_scroll_through_a_gap() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (0, 0), move |scope| {
            scope.item(move || along(lazy, ITEM * 3, "0"));
            scope.item(move || along(lazy, ITEM, "1"));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                along(lazy, ITEM, "2")
            });
        });
        let visible =
            || state.layout_info().visible_items_info.iter().map(|it| it.index).collect::<Vec<_>>();
        scroll_by(&mut ui, state, ITEM);
        assert_eq!(lazy.main_start(&mut ui, "0"), -ITEM as f32);
        assert!(!ui.on_node_with_tag("1").is_displayed());
        assert!(!ui.on_node_with_tag("2").is_displayed());
        assert_eq!(visible(), [0]);
        scroll_by(&mut ui, state, ITEM - 1);
        assert_eq!(lazy.main_start(&mut ui, "0"), (-ITEM * 2 + 1) as f32);
        assert!(!ui.on_node_with_tag("1").is_displayed());
        assert!(!ui.on_node_with_tag("2").is_displayed());
        assert_eq!(visible(), [0]);
        scroll_by(&mut ui, state, 1);
        assert_eq!(lazy.main_start(&mut ui, "0"), (-ITEM * 2) as f32);
        assert!(!ui.on_node_with_tag("1").is_displayed());
        assert_eq!(lazy.main_start(&mut ui, "2"), ITEM as f32);
        assert_eq!(visible(), [0, 2]);
        scroll_by(&mut ui, state, ITEM * 3);
        assert!(!ui.on_node_with_tag("0").is_displayed());
        assert!(!ui.on_node_with_tag("1").is_displayed());
        assert_eq!(lazy.main_start(&mut ui, "2"), 0.0);
        assert_eq!(visible(), [2]);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt mainSize_minusOne
#[test]
fn a_main_size_of_minus_one_is_measured() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            let mut args =
                Args::new(lazy, 2, axis_size(lazy, ITEM * 2, ITEM * 2 - 1).test_tag(GRID));
            args.padding = lazy.padding(Dp(ITEM as f32), 0.dp(), 0.dp(), 0.dp());
            Grid(args, move |scope| {
                scope.items(2, move |_| along(lazy, ITEM, "item"));
            });
        });
        assert!(ui.on_node_with_tag(GRID).is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpanItem_correctOrderWhenUpdated
#[test]
fn items_before_a_full_line_stay_ordered_when_more_come() {
    for lazy in orientations() {
        let count = mutable_state_of(1usize);
        let read = count.handle();
        let (mut ui, _) = keeping(three_lanes(lazy, ITEM * 2), (0, 0), move |scope| {
            scope.items(read.get(), move |index| along(lazy, ITEM, index));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                along(lazy, ITEM, "full")
            });
        });
        count.set(2);
        ui.wait_for_idle();
        assert_eq!(lazy.cross_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.cross_start(&mut ui, "1"), ITEM as f32);
        count.set(3);
        ui.wait_for_idle();
        assert_eq!(lazy.cross_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.cross_start(&mut ui, "1"), ITEM as f32);
        assert_eq!(lazy.cross_start(&mut ui, "2"), (ITEM * 2) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridTest.kt fullSpanItem_scrollPast_withGaps_atStart
#[test]
fn a_full_line_scrolled_past_with_a_gap_at_the_start_and_back() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM * 2), (0, 0), move |scope| {
            scope.item(move || along(lazy, ITEM, "0"));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                along(lazy, ITEM, "1")
            });
            scope.items(4, move |index| along(lazy, ITEM, index + 2));
        });
        scroll_by(&mut ui, state, ITEM);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
        scroll_by(&mut ui, state, ITEM);
        assert_eq!(lazy.main_start(&mut ui, "2"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "3"), 0.0);
        scroll_by(&mut ui, state, -ITEM);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "2"), ITEM as f32);
        assert_eq!(lazy.main_start(&mut ui, "3"), ITEM as f32);
        scroll_by(&mut ui, state, -ITEM);
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), ITEM as f32);
    }
}

const BIG: i32 = 100;

fn dp(px: i32) -> Dp {
    Dp(px as f32)
}

/// An `axisSize` grid of `lanes` with a state kept, `tweak` having its way with the arguments.
fn big_grid(
    lazy: Lazy,
    lanes: usize,
    (cross, main): (i32, i32),
    initial: (usize, i32),
    tweak: impl Fn(Args) -> Args + 'static,
    content: impl Fn(LazyStaggeredGridScope) + Clone + 'static,
) -> (ComposeTestRule, LazyStaggeredGridState) {
    keeping(
        move |state| {
            tweak(Args::new(lazy, lanes, axis_size(lazy, cross, main).test_tag(GRID)).state(state))
        },
        initial,
        content,
    )
}

/// Boxes of `BIG` tagged by index.
fn boxes(count: usize) -> impl Fn(LazyStaggeredGridScope) + Clone + 'static {
    move |scope| {
        scope.items(count, |index| {
            Box!(modifier = Modifier.size(dp(BIG)).test_tag(index.to_string()));
        });
    }
}

fn long(lazy: Lazy, count: usize) -> impl Fn(LazyStaggeredGridScope) + Clone + 'static {
    move |scope| scope.items(count, move |index| along(lazy, BIG, index))
}

fn long_keyed(lazy: Lazy, count: usize) -> impl Fn(LazyStaggeredGridScope) + Clone + 'static {
    move |scope| scope.items_keyed(count, |index| index, move |index| along(lazy, BIG, index))
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt arrangement_addsSpacingInBothDirections
#[test]
fn an_arrangement_adds_spacing_both_ways() {
    for lazy in orientations() {
        let (mut ui, _) = big_grid(
            lazy,
            2,
            (BIG * 3, BIG * 5),
            (0, 0),
            |mut args| {
                args.main_spacing = BIG as f32;
                args.cross_spacing = (BIG / 2) as f32;
                args
            },
            long(lazy, 100),
        );
        let (cross, spacing) = (BIG * 5 / 4, BIG / 2);
        assert_bounds(&mut ui, lazy, "0", (0, 0), (cross, BIG));
        assert_bounds(&mut ui, lazy, "1", (cross + spacing, 0), (cross, BIG));
        assert_bounds(&mut ui, lazy, "2", (0, BIG * 2), (cross, BIG));
        assert_bounds(&mut ui, lazy, "3", (cross + spacing, BIG * 2), (cross, BIG));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt arrangement_lastItem_noSpacingMainAxis
#[test]
fn no_spacing_follows_the_last_item() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (0, 0),
            |mut args| {
                args.main_spacing = BIG as f32;
                args
            },
            long(lazy, 100),
        );
        let _ = run(&mut ui, async move { state.scroll_to_item(100, 0).await });
        assert_at(&mut ui, lazy, "98", 0, BIG * 4);
        assert_at(&mut ui, lazy, "99", BIG, BIG * 4);
    }
}

fn long_double(lazy: Lazy, count: usize) -> impl Fn(LazyStaggeredGridScope) + Clone + 'static {
    move |scope| scope.items(count, move |index| along(lazy, BIG * 2, index))
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt negativeSpacing_itemsVisible
#[test]
fn a_negative_spacing_overlaps_items() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (0, 0),
            |mut args| {
                args.main_spacing = -BIG as f32;
                args
            },
            long_double(lazy, 100),
        );
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_size(&mut ui, "0"), (BIG * 2) as f32);
        assert_eq!(lazy.main_start(&mut ui, "2"), BIG as f32);
        scroll_by(&mut ui, state, BIG);
        assert_eq!(lazy.main_start(&mut ui, "0"), -BIG as f32);
        assert_eq!(lazy.main_start(&mut ui, "2"), 0.0);
        assert_eq!(state.first_visible_item_index(), 2);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt negativeSpacing_withContentPadding_itemsVisible
#[test]
fn a_negative_spacing_overlaps_items_with_padding() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (0, 0),
            move |mut args| {
                args.main_spacing = -BIG as f32;
                args.padding(lazy.padding(dp(BIG), 0.dp(), 0.dp(), 0.dp()))
            },
            long_double(lazy, 100),
        );
        assert_eq!(lazy.main_start(&mut ui, "0"), BIG as f32);
        assert_eq!(lazy.main_start(&mut ui, "2"), (BIG * 2) as f32);
        scroll_by(&mut ui, state, BIG);
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "2"), BIG as f32);
        assert_eq!(state.first_visible_item_index(), 2);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt negativeSpacingLargerThanItem_itemsVisible
#[test]
fn a_negative_spacing_larger_than_an_item_stacks_them() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (2, 0),
            |mut args| {
                args.main_spacing = -(BIG + BIG / 2) as f32;
                args
            },
            boxes(8),
        );
        for index in 0..8 {
            assert_eq!(lazy.main_start(&mut ui, &index.to_string()), 0.0, "{index}");
        }
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt nonStartCrossAxisArrangement
#[test]
fn lanes_are_placed_by_the_arrangement() {
    for lazy in orientations() {
        let (mut ui, _) = big_grid(
            lazy,
            1,
            (BIG * 5, BIG * 5),
            (0, 0),
            |mut args| {
                args.cells = StaggeredGridCells::FixedSize(dp(BIG * 2));
                args.cross_center = true;
                args
            },
            boxes(10),
        );
        assert_eq!(lazy.cross_start(&mut ui, "0"), (BIG / 2) as f32);
        assert_eq!(lazy.cross_size(&mut ui, "0"), (BIG * 2) as f32);
        assert_eq!(lazy.cross_start(&mut ui, "1"), (BIG * 2 + BIG / 2) as f32);
        assert_eq!(lazy.cross_size(&mut ui, "1"), (BIG * 2) as f32);
    }
}

/// `PaddingValues(mainAxis, crossAxis)`.
fn padded(lazy: Lazy, main: Dp, cross: Dp) -> PaddingValues {
    lazy.padding(main, main, cross, cross)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPadding_isApplied
#[test]
fn content_padding_is_applied() {
    for lazy in orientations() {
        let (mut ui, _) = big_grid(
            lazy,
            2,
            (BIG * 4, BIG * 5),
            (0, 0),
            move |args| args.padding(padded(lazy, dp(BIG * 2), dp(BIG))),
            long(lazy, 4),
        );
        assert_bounds(&mut ui, lazy, "0", (BIG, BIG * 2), (BIG, BIG));
        assert_bounds(&mut ui, lazy, "1", (BIG * 2, BIG * 2), (BIG, BIG));
        lazy.scroll_main_axis_by(&mut ui, GRID, (BIG * 100) as f32);
        assert_bounds(&mut ui, lazy, "2", (BIG, BIG * 2), (BIG, BIG));
        assert_bounds(&mut ui, lazy, "3", (BIG * 2, BIG * 2), (BIG, BIG));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPadding_scrollPosition_setAndReportedCorrectly
#[test]
fn content_padding_doesnt_count_in_the_position() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 4, BIG * 5),
            (10, 0),
            move |args| args.padding(padded(lazy, dp(BIG * 2), dp(BIG))),
            long(lazy, 100),
        );
        assert_eq!(state.first_visible_item_index(), 10);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        assert_at(&mut ui, lazy, "10", BIG, BIG * 2);
        assert_at(&mut ui, lazy, "11", BIG * 2, BIG * 2);
        scroll_by(&mut ui, state, BIG * 2);
        assert_at(&mut ui, lazy, "10", BIG, 0);
        assert_at(&mut ui, lazy, "11", BIG * 2, 0);
        assert_eq!(state.first_visible_item_index(), 14);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPadding_reportedInLayoutInfo
#[test]
fn content_padding_is_in_layout_info() {
    for lazy in orientations() {
        let (_ui, state) = big_grid(
            lazy,
            2,
            (BIG * 4, BIG * 5),
            (10, 0),
            move |args| args.padding(padded(lazy, dp(BIG * 2), dp(BIG))),
            long(lazy, 100),
        );
        let info = state.layout_info();
        assert_eq!(info.before_content_padding, BIG * 2);
        assert_eq!(info.after_content_padding, BIG * 2);
        assert_eq!(info.viewport_start_offset, -BIG * 2);
        assert_eq!(info.viewport_end_offset, BIG * 3);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPadding_itemsAreDisplayedInPaddingArea
#[test]
fn items_show_in_the_padding() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 4, BIG * 5),
            (0, 0),
            move |args| args.padding(padded(lazy, dp(BIG * 2), dp(BIG))),
            long(lazy, 10),
        );
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG * 2) as f32);
        scroll_by(&mut ui, state, BIG * 2 + BIG / 2);
        assert_eq!(lazy.main_start(&mut ui, "0"), (-BIG / 2) as f32);
        scroll_by(&mut ui, state, BIG * 10);
        assert_eq!(lazy.main_start(&mut ui, "9"), (BIG * 2) as f32);
        scroll_by(&mut ui, state, -(BIG * 2 + BIG / 2));
        assert_eq!(lazy.main_start(&mut ui, "9"), (BIG * 4 + BIG / 2) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPadding_largePadding_itemsAreDisplayedCorrectly
#[test]
fn a_padding_larger_than_the_grid_keeps_the_last_items() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (0, 0),
            move |args| args.padding(padded(lazy, dp(BIG * 6), 0.dp())),
            long(lazy, 10),
        );
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG * 6) as f32);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        scroll_by(&mut ui, state, BIG * 20);
        assert!(!ui.on_node_with_tag("8").is_displayed());
        assert_eq!(state.first_visible_item_index(), 8);
        // Being the last item in the lane, it stays.
        assert_eq!(state.first_visible_item_scroll_offset(), BIG * 8);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPadding_zeroItems
#[test]
fn no_items_leave_the_padding() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            let args =
                Args::new(lazy, 2, Modifier.test_tag(GRID)).padding(padded(lazy, 10.dp(), 2.dp()));
            Grid(args, |_| {});
        });
        ui.wait_for_idle();
        assert_eq!(lazy.main_size(&mut ui, GRID), 20.0);
        assert_eq!(lazy.cross_size(&mut ui, GRID), 4.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPadding_zeroHeightItem
#[test]
fn items_of_no_size_leave_the_padding() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            Box!(modifier = lazy.cross_axis_size(dp(BIG * 2)), {
                let args = Args::new(lazy, 2, Modifier.test_tag(GRID)).padding(padded(
                    lazy,
                    10.dp(),
                    2.dp(),
                ));
                Grid(args, move |scope| scope.items(2, move |index| along(lazy, 0, index)));
            });
        });
        ui.wait_for_idle();
        assert_eq!(lazy.main_size(&mut ui, GRID), 20.0);
        assert_eq!(lazy.cross_size(&mut ui, GRID), (BIG * 2) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt scrollsCorrectlyWithKeyAndLargeMainAxisContentPadding
#[test]
fn keys_and_a_large_padding_scroll_on() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (0, 0),
            move |args| args.padding(padded(lazy, dp(BIG * 2), 0.dp())),
            long_keyed(lazy, 1000),
        );
        for _ in 0..10 {
            scroll_by(&mut ui, state, BIG * 20);
        }
        assert_eq!(state.first_visible_item_index(), 400);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt contentPaddingIncludedInLayoutMeasurement
#[test]
fn a_grid_measures_with_its_padding() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            Box!(modifier = axis_size(lazy, BIG * 2, BIG * 10), {
                let args = Args::new(lazy, 2, Modifier.test_tag(GRID)).padding(padded(
                    lazy,
                    dp(BIG * 2),
                    0.dp(),
                ));
                Grid(args, long_keyed(lazy, 4));
            });
        });
        ui.wait_for_idle();
        assert_eq!(lazy.main_size(&mut ui, GRID), (BIG * 6) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt afterContentPaddingWithSmallScrolls
#[test]
fn small_scrolls_keep_the_padding_after() {
    for lazy in orientations() {
        let kept = state_cell();
        let keep = kept.clone();
        let mut ui = rule(move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            keep.set(Some(state));
            Box!(modifier = axis_size(lazy, BIG * 2, BIG * 4), {
                let args = Args::new(lazy, 2, Modifier.test_tag(GRID))
                    .padding(lazy.padding(0.dp(), dp(BIG / 2), 0.dp(), 0.dp()))
                    .state(state);
                Grid(args, move |scope| {
                    scope.items_keyed(
                        20,
                        |index| index,
                        move |index| {
                            let size = if index == 0 || index == 19 { BIG / 2 } else { BIG * 2 };
                            along(lazy, size, index)
                        },
                    );
                });
            });
        });
        let state = state_of(&kept);
        scroll_by(&mut ui, state, BIG * 30);
        scroll_by(&mut ui, state, -5);
        scroll_by(&mut ui, state, BIG / 2);
        assert_eq!(lazy.main_start(&mut ui, "19"), (BIG * 3) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt twoElements_inOneItem_placedToTheEnd
#[test]
fn two_layouts_in_an_item_are_at_the_end_reversed() {
    for lazy in orientations() {
        let (mut ui, _) = big_grid(lazy, 2, (BIG * 2, BIG * 4), (0, 0), reversed, |scope| {
            scope.item(|| {
                Box!(modifier = Modifier.size(dp(BIG * 2)).test_tag("0"));
                Box!(modifier = Modifier.size(dp(BIG)).test_tag("1"));
            });
        });
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG * 2) as f32);
        assert_eq!(lazy.main_size(&mut ui, "0"), (BIG * 2) as f32);
        assert_eq!(lazy.main_start(&mut ui, "1"), (BIG * 3) as f32);
        assert_eq!(lazy.main_size(&mut ui, "1"), BIG as f32);
    }
}

fn reversed(mut args: Args) -> Args {
    args.reverse = true;
    args
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt items_positioned_AtTheEnd
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt initialScrollPosition_IsZero
#[test]
fn reversed_items_are_at_the_end() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(lazy, 2, (BIG * 2, BIG * 4), (0, 0), reversed, boxes(4));
        assert_at(&mut ui, lazy, "0", 0, BIG * 3);
        assert_at(&mut ui, lazy, "1", BIG, BIG * 3);
        assert_at(&mut ui, lazy, "2", 0, BIG * 2);
        assert_at(&mut ui, lazy, "3", BIG, BIG * 2);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt scrollForwards_doesNothing
#[test]
fn reversed_a_scroll_the_wrong_way_does_nothing() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(lazy, 2, (BIG * 2, BIG * 4), (0, 0), reversed, boxes(4));
        // Kotlin's swipes by `itemSize`, against the content's end here (D491).
        lazy.scroll_main_axis_by(&mut ui, GRID, -BIG as f32);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG * 3) as f32);
        assert_eq!(lazy.main_start(&mut ui, "1"), (BIG * 3) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt scrollBackwards_halfItem
#[test]
fn reversed_half_an_item_scrolled() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(lazy, 2, (BIG * 2, BIG * 2), (0, 0), reversed, boxes(6));
        lazy.scroll_main_axis_by(&mut ui, GRID, (BIG / 2) as f32);
        assert_eq!(state.first_visible_item_index(), 0);
        let scrolled = state.first_visible_item_scroll_offset();
        assert!(scrolled > 0);
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG + scrolled) as f32);
        assert_eq!(lazy.main_start(&mut ui, "1"), (BIG + scrolled) as f32);
        assert_eq!(lazy.main_start(&mut ui, "2"), scrolled as f32);
        assert_eq!(lazy.main_start(&mut ui, "3"), scrolled as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt scrollBackwards_toTheEnd
#[test]
fn reversed_scrolled_to_the_end() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(lazy, 2, (BIG * 2, BIG * 2), (0, 0), reversed, boxes(6));
        lazy.scroll_main_axis_by(&mut ui, GRID, (BIG * 10) as f32);
        assert_eq!(state.first_visible_item_index(), 2);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        assert_eq!(lazy.main_start(&mut ui, "4"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "5"), 0.0);
    }
}

fn rtl_reversed(lazy: Lazy) -> (ComposeTestRule, LazyStaggeredGridState) {
    let kept = state_cell();
    let keep = kept.clone();
    let ui = rule(move || {
        let keep = keep.clone();
        CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
            || {
                let state = remember_lazy_staggered_grid_state(0, 0);
                keep.set(Some(state));
                let modifier = axis_size(lazy, BIG * 2, BIG * 2).test_tag(GRID);
                Grid(reversed(Args::new(lazy, 2, modifier).state(state)), boxes(6));
            },
        );
    });
    let state = state_of(&kept);
    (ui, state)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt rtl_itemsAreReversed
#[test]
fn right_to_left_reversed_items() {
    for lazy in orientations() {
        let (mut ui, _) = rtl_reversed(lazy);
        if lazy.vertical {
            assert_at(&mut ui, lazy, "0", BIG, BIG);
            assert_at(&mut ui, lazy, "1", 0, BIG);
        } else {
            // Reversed twice.
            assert_at(&mut ui, lazy, "0", 0, 0);
            assert_at(&mut ui, lazy, "1", BIG, 0);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt rtl_scrollToTheEnd
#[test]
fn right_to_left_reversed_scrolled_to_the_end() {
    for lazy in orientations() {
        let (mut ui, state) = rtl_reversed(lazy);
        lazy.scroll_main_axis_by(&mut ui, GRID, (BIG * 10) as f32);
        assert_eq!(state.first_visible_item_index(), 2);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        if lazy.vertical {
            assert_at(&mut ui, lazy, "4", BIG, 0);
            assert_at(&mut ui, lazy, "5", 0, 0);
        } else {
            assert_at(&mut ui, lazy, "4", 0, BIG);
            assert_at(&mut ui, lazy, "5", BIG, BIG);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt remeasure_onReverseLayoutChange
#[test]
fn a_change_of_reversal_lays_out_again() {
    for lazy in orientations() {
        let reverse = mutable_state_of(false);
        let read = reverse.handle();
        let (mut ui, state) = keeping(
            move |state| {
                let mut args = Args::new(lazy, 2, axis_size(lazy, BIG * 2, BIG * 2)).state(state);
                args.reverse = read.get();
                args
            },
            (0, 0),
            boxes(6),
        );
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
        reverse.set(true);
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        assert_eq!(lazy.main_start(&mut ui, "0"), BIG as f32);
        assert_eq!(lazy.main_start(&mut ui, "1"), BIG as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt contentPadding_isNotReversed
#[test]
fn reversed_the_padding_stays_where_it_is() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (0, 0),
            move |args| reversed(args.padding(lazy.padding(dp(BIG), dp(BIG * 2), 0.dp(), 0.dp()))),
            boxes(6),
        );
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG * 2) as f32);
        assert_eq!(lazy.main_start(&mut ui, "1"), (BIG * 2) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt contentPadding_isPreserved
#[test]
fn reversed_the_padding_after_is_kept() {
    for lazy in orientations() {
        let (mut ui, state) = big_grid(
            lazy,
            2,
            (BIG * 2, BIG * 5),
            (0, 0),
            move |args| reversed(args.padding(lazy.padding(0.dp(), dp(BIG * 2), 0.dp(), 0.dp()))),
            boxes(6),
        );
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG * 2) as f32);
        assert_eq!(lazy.main_start(&mut ui, "1"), (BIG * 2) as f32);
    }
}
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridReverseLayoutTest.kt initialPosition_isPreserved
#[test]
fn reversed_an_initial_offset_is_kept() {
    for lazy in orientations() {
        let (mut ui, state) =
            big_grid(lazy, 2, (BIG * 2, BIG * 2), (0, BIG / 2), reversed, boxes(6));
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), BIG / 2);
        assert_eq!(lazy.main_start(&mut ui, "0"), (BIG + BIG / 2) as f32);
        assert_eq!(lazy.main_start(&mut ui, "1"), (BIG + BIG / 2) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLayoutInfoTest.kt contentTypeIsCorrect
#[test]
fn layout_info_has_the_content_types() {
    for lazy in orientations() {
        let (_ui, state) = keeping(
            move |state| Args::new(lazy, 1, Modifier.size(30.dp())).state(state),
            (0, 0),
            |scope| {
                scope.items_with_span(
                    2,
                    |index| index,
                    |_| StaggeredGridItemSpan::SingleLane,
                    |_| {
                        Box!(modifier = Modifier.size(10.dp()));
                    },
                );
                scope.item(|| {
                    Box!(modifier = Modifier.size(10.dp()));
                });
            },
        );
        let types: Vec<_> =
            state.layout_info().visible_items_info.iter().map(|it| it.content_type).collect();
        assert!(types[0].is_some() && types[1].is_some() && types[0] != types[1]);
        assert_eq!(types[2], None);
    }
}

/// `itemPairs`.
fn item_pairs(state: LazyStaggeredGridState) -> Vec<(usize, IntOffset)> {
    state.layout_info().visible_items_info.iter().map(|it| (it.index, it.offset)).collect()
}

fn short_two_lanes(lazy: Lazy, initial: (usize, i32)) -> (ComposeTestRule, LazyStaggeredGridState) {
    keeping(
        move |state| Args::new(lazy, 2, axis_size(lazy, ITEM * 2, ITEM + ITEM / 2)).state(state),
        initial,
        move |scope| scope.items(100, move |index| along(lazy, ITEM, index)),
    )
}

fn four_from(lazy: Lazy, first: usize, main: i32) -> Vec<(usize, IntOffset)> {
    vec![
        (first, at(lazy, 0, main)),
        (first + 1, at(lazy, ITEM, main)),
        (first + 2, at(lazy, 0, main + ITEM)),
        (first + 3, at(lazy, ITEM, main + ITEM)),
    ]
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLayoutInfoTest.kt updatedSynchronouslyDuringScroll_smallScrollForward
#[test]
fn layout_info_follows_a_small_scroll_forward() {
    for lazy in orientations() {
        let (mut ui, state) = short_two_lanes(lazy, (0, 0));
        scroll_by(&mut ui, state, 10);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 10);
        assert_eq!(item_pairs(state), four_from(lazy, 0, -10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLayoutInfoTest.kt updatedSynchronouslyDuringScroll_smallScrollBackward
#[test]
fn layout_info_follows_a_small_scroll_backward() {
    for lazy in orientations() {
        let (mut ui, state) = short_two_lanes(lazy, (0, ITEM / 2));
        scroll_by(&mut ui, state, -10);
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), ITEM / 2 - 10);
        assert_eq!(item_pairs(state), four_from(lazy, 0, -(ITEM / 2 - 10)));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLayoutInfoTest.kt updatedSynchronouslyDuringScroll_largeScrollForward
#[test]
fn layout_info_follows_a_large_scroll_forward() {
    for lazy in orientations() {
        let (mut ui, state) = short_two_lanes(lazy, (0, 0));
        scroll_by(&mut ui, state, ITEM * 3);
        assert_eq!(item_pairs(state), four_from(lazy, 6, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLayoutInfoTest.kt updatedSynchronouslyDuringScroll_largeScrollBackward
#[test]
fn layout_info_follows_a_large_scroll_backward() {
    for lazy in orientations() {
        let (mut ui, state) = short_two_lanes(lazy, (6, 0));
        scroll_by(&mut ui, state, -ITEM * 3);
        assert_eq!(item_pairs(state), four_from(lazy, 0, 0));
    }
}

/// Reads `layout_info` in a scope of its own, as Kotlin's `snapshotFlow`.
#[composable]
fn LayoutInfoReader(state: LazyStaggeredGridState, vertical: bool, noted: Rc<Cell<i32>>) {
    let info = state.layout_info();
    let offset = info.visible_items_info.first().map_or(IntOffset::new(0, 0), |it| it.offset);
    noted.set(if vertical { offset.y } else { offset.x });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridLayoutInfoTest.kt snapshotFlowIsNotifiedAboutNewOffsetOnSmallScrolls
#[test]
fn a_reader_of_layout_info_hears_of_a_small_scroll() {
    for lazy in orientations() {
        let first = Rc::new(Cell::new(0));
        let noted = first.clone();
        let kept = state_cell();
        let keep = kept.clone();
        let mut ui = rule(move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            keep.set(Some(state));
            Grid(Args::new(lazy, 1, Modifier.size(15.dp())).state(state), |scope| {
                scope.items(100, |_| {
                    Box!(modifier = Modifier.size(10.dp()));
                });
            });
            LayoutInfoReader(state, lazy.vertical, noted.clone());
        });
        scroll_by(&mut ui, state_of(&kept), 1);
        ui.wait_for_idle();
        assert_eq!(first.get(), -1);
    }
}

/// `TestContent` of `LazyStaggeredGridScrollTest`: two lanes, an item of 1 or 2 sizes by its index,
/// item 50 a full line.
fn scroll_content(
    lazy: Lazy,
    container: i32,
    before: i32,
    after: i32,
) -> (ComposeTestRule, LazyStaggeredGridState) {
    big_grid(
        lazy,
        2,
        (BIG * 2, container),
        (0, 0),
        move |args| args.padding(lazy.padding(dp(before), dp(after), 0.dp(), 0.dp())),
        move |scope| {
            scope.items_with_span(
                100,
                |_| (),
                |index| {
                    if index == 50 {
                        StaggeredGridItemSpan::FullLine
                    } else {
                        StaggeredGridItemSpan::SingleLane
                    }
                },
                move |index| along(lazy, BIG * (index as i32 % 2 + 1), index),
            );
        },
    )
}

fn scroll_to(ui: &mut ComposeTestRule, state: LazyStaggeredGridState, index: usize, offset: i32) {
    let _ = run(ui, async move { state.scroll_to_item(index, offset).await });
}

fn position(state: LazyStaggeredGridState) -> (usize, i32) {
    (state.first_visible_item_index(), state.first_visible_item_scroll_offset())
}

fn main_of(lazy: Lazy, offset: IntOffset) -> i32 {
    if lazy.vertical { offset.y } else { offset.x }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt setupWorks
#[test]
fn a_grid_starts_at_its_first_item() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        assert_eq!(position(state), (0, 0));
        assert!(ui.on_node_with_tag("0").is_displayed());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_byIndexAndOffset_outsideBounds
#[test]
fn a_scroll_to_an_item_out_of_view() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 10, 10);
        assert_eq!(position(state), (10, 10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_byIndexAndOffset_inBounds
#[test]
fn a_scroll_to_an_item_in_view() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 2, 10);
        assert_eq!(position(state), (1, 110));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_byIndexAndOffset_inBounds_secondLane
#[test]
fn a_scroll_to_an_item_in_view_in_the_second_lane() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 4, 10);
        assert_eq!(position(state), (3, 10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_byIndexAndNegativeOffset
#[test]
fn a_scroll_to_an_item_with_a_negative_offset() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 4, -10);
        assert_eq!(position(state), (1, BIG * 2 - 10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_offsetLargerThanItem
#[test]
fn a_scroll_with_an_offset_larger_than_the_item() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 10, BIG * 2);
        assert_eq!(position(state), (13, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_beyondFirstItem
#[test]
fn a_scroll_before_the_first_item_stops_there() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 10, 0);
        scroll_to(&mut ui, state, 0, -10);
        assert_eq!(position(state), (0, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_beyondLastItem
#[test]
fn a_scroll_past_the_last_item_stops_there() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 99, BIG * 3);
        let info = state.layout_info();
        let last = info.visible_items_info.last().expect("laid out");
        assert_eq!(last.index, 99);
        assert_eq!(main_of(lazy, last.offset), BIG * 3);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_beyondItemCount
#[test]
fn a_scroll_past_the_count_stops_at_the_last() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 420, 0);
        let info = state.layout_info();
        let last = info.visible_items_info.last().expect("laid out");
        assert_eq!(last.index, 99);
        assert_eq!(main_of(lazy, last.offset), BIG * 3);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt canScrollForward
#[test]
fn at_the_start_it_can_only_scroll_forward() {
    for lazy in orientations() {
        let (_ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        assert_eq!(state.first_visible_item_scroll_offset(), 0);
        assert!(state.can_scroll_forward());
        assert!(!state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt canScrollBackward
#[test]
fn at_the_end_it_can_only_scroll_backward() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 99, 0);
        let info = state.layout_info();
        assert_eq!(
            main_of(lazy, info.visible_items_info.last().expect("laid out").offset),
            BIG * 3
        );
        assert!(!state.can_scroll_forward());
        assert!(state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt canScrollForwardAndBackward
#[test]
fn in_the_middle_it_can_scroll_both_ways() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 10, 0);
        assert_eq!(state.first_visible_item_index(), 10);
        assert!(state.can_scroll_forward());
        assert!(state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollToItem_fullSpan
#[test]
fn a_scroll_to_before_a_full_line() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 0, 0);
        scroll_to(&mut ui, state, 49, 10);
        assert_eq!(position(state), (49, 10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt canScrollForwardAndBackward_afterSmallScrollFromStart
#[test]
fn a_small_scroll_from_the_start_and_back() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG + BIG / 2, 0, 0);
        let delta = BIG / 3;
        scroll_by(&mut ui, state, delta);
        assert_eq!(state.first_visible_item_scroll_offset(), delta);
        assert!(state.can_scroll_forward());
        assert!(state.can_scroll_backward());
        scroll_by(&mut ui, state, -delta);
        assert!(state.can_scroll_forward());
        assert!(!state.can_scroll_backward());
    }
}

fn from_the_end(lazy: Lazy, after: i32) {
    let (mut ui, state) = scroll_content(lazy, BIG * 2 + BIG / 2, 0, after);
    let delta = -(BIG / 3);
    scroll_to(&mut ui, state, 100, 0);
    assert!(!state.can_scroll_forward());
    assert!(state.can_scroll_backward());
    scroll_by(&mut ui, state, delta);
    assert!(state.can_scroll_forward());
    assert!(state.can_scroll_backward());
    scroll_by(&mut ui, state, -delta);
    assert!(!state.can_scroll_forward());
    assert!(state.can_scroll_backward());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt canScrollForwardAndBackward_afterSmallScrollFromEnd
#[test]
fn a_small_scroll_from_the_end_and_back() {
    for lazy in orientations() {
        from_the_end(lazy, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt canScrollForwardAndBackward_afterSmallScrollFromEnd_withContentPadding
#[test]
fn a_small_scroll_from_the_end_and_back_with_padding() {
    for lazy in orientations() {
        from_the_end(lazy, 2);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt scrollBy_emptyItemWithSpacing
#[test]
fn a_scroll_past_an_empty_item_with_spacing() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(
            move |state| {
                let mut args = Args::new(lazy, 1, Modifier.size(dp(BIG))).state(state);
                args.main_spacing = 10.0;
                args
            },
            (0, 0),
            |scope| {
                scope.item(|| {
                    Box!(modifier = Modifier.size(dp(BIG)).test_tag("0"));
                });
                scope.item(|| {});
                scope.item(|| {
                    Box!(modifier = Modifier.size(dp(BIG)).test_tag("2"));
                });
            },
        );
        // Two spacings of 10: item 2 is at the edge after 20.
        scroll_by(&mut ui, state, 20);
        scroll_by(&mut ui, state, 20);
        assert_eq!(lazy.main_start(&mut ui, "2"), (BIG - 20) as f32);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridScrollTest.kt overScrollingBackShouldIgnoreBeforeContentPadding
#[test]
fn a_scroll_back_past_the_start_ignores_the_padding() {
    for lazy in orientations() {
        let (mut ui, state) = scroll_content(lazy, BIG * 5, 5, 0);
        scroll_by(&mut ui, state, BIG);
        let consumed = run(&mut ui, async move { state.scroll_by(-(BIG as f32 + 10.0)).await });
        assert_eq!(consumed.expect("not interrupted"), -BIG as f32);
    }
}

/// `TestContent` of `LazyStaggeredGridAnimatedScrollTest`.
fn animated_content(lazy: Lazy, spacing: i32) -> (ComposeTestRule, LazyStaggeredGridState) {
    big_grid(
        lazy,
        2,
        (BIG * 2, BIG * 5),
        (0, 0),
        move |mut args| {
            args.main_spacing = spacing as f32;
            args
        },
        move |scope| {
            scope.items_with_span(
                100,
                |_| (),
                |index| {
                    if index == 50 {
                        StaggeredGridItemSpan::FullLine
                    } else {
                        StaggeredGridItemSpan::SingleLane
                    }
                },
                move |index| along(lazy, BIG, index),
            );
        },
    )
}

fn animate_to(ui: &mut ComposeTestRule, state: LazyStaggeredGridState, index: usize, offset: i32) {
    let _ = run(ui, async move { state.animate_scroll_to_item(index, offset).await });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollBy
#[test]
fn an_animated_scroll_by() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        let _ = run(&mut ui, async move {
            state.animate_scroll_by(320.0, compose_animation::spring()).await
        });
        assert_eq!(position(state), (6, 20));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_positiveOffset
#[test]
fn an_animated_scroll_with_an_offset() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        animate_to(&mut ui, state, 10, 10);
        assert_eq!(position(state), (10, 10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_positiveOffset_largerThanItem
#[test]
fn an_animated_scroll_with_an_offset_larger_than_the_item() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        animate_to(&mut ui, state, 10, 150);
        assert_eq!(position(state), (12, 50));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_negativeOffset
#[test]
fn an_animated_scroll_with_a_negative_offset() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        animate_to(&mut ui, state, 10, -10);
        assert_eq!(position(state), (8, BIG - 10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_beforeFirstItem
#[test]
fn an_animated_scroll_before_the_first_item() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        scroll_to(&mut ui, state, 10, 0);
        animate_to(&mut ui, state, 0, -10);
        assert_eq!(position(state), (0, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_afterLastItem
#[test]
fn an_animated_scroll_past_the_last_item() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        animate_to(&mut ui, state, 100, 0);
        assert_eq!(position(state), (91, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_toFullSpan
#[test]
fn an_animated_scroll_to_a_full_line() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        animate_to(&mut ui, state, 50, 10);
        assert_eq!(position(state), (50, 10));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_toFullSpan_andBack
#[test]
fn an_animated_scroll_to_a_full_line_and_back() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        animate_to(&mut ui, state, 50, 10);
        animate_to(&mut ui, state, 45, 0);
        assert_eq!(position(state), (44, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItemWithOffsetLargerThanItemSize_forward
#[test]
fn an_animated_scroll_forward_with_a_large_negative_offset() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        animate_to(&mut ui, state, 20, -BIG * 3);
        assert_eq!(position(state), (14, 0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItemWithOffsetLargerThanItemSize_backward
#[test]
fn an_animated_scroll_backward_with_a_large_offset() {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, 0);
        scroll_to(&mut ui, state, 20, 0);
        animate_to(&mut ui, state, 0, BIG * 3);
        assert_eq!(position(state), (6, 0));
    }
}

/// `assertSpringAnimation`: a frame at a time, within a pixel of the spring from one to the other.
fn assert_spring(spacing: i32, to: (usize, i32), from: (usize, i32)) {
    for lazy in orientations() {
        let (mut ui, state) = animated_content(lazy, spacing);
        if from != (0, 0) {
            scroll_to(&mut ui, state, from.0, from.1);
        }
        ui.wait_for_idle();
        assert_eq!(position(state), from);
        let step = BIG + spacing;
        let start = (from.0 as i32 / 2 * step + from.1) as f32;
        let end = (to.0 as i32 / 2 * step + to.1) as f32;
        ui.set_auto_advance(false);
        let expected: Rc<std::cell::RefCell<Vec<f32>>> = Rc::default();
        let into = expected.clone();
        crate::scroll_support::launch(async move {
            compose_animation::animate(start, end, 0.0, compose_animation::spring(), |value, _| {
                into.borrow_mut().push(value)
            })
            .await;
        });
        crate::scroll_support::launch(async move {
            let _ = state.animate_scroll_to_item(to.0, to.1).await;
        });
        let mut frames = 0;
        loop {
            ui.advance_time_by_frame();
            frames += 1;
            let Some(value) = expected.borrow().last().copied() else { continue };
            let actual = state.first_visible_item_index() as i32 / 2 * step
                + state.first_visible_item_scroll_offset();
            assert!(
                (actual - value.round() as i32).abs() <= 1,
                "frame {frames}: at {actual}, the spring at {value} (from {start} to {end})"
            );
            if !state.is_scroll_in_progress() || frames > 500 {
                break;
            }
        }
        assert_eq!(position(state), to);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_inBounds
#[test]
fn an_animated_scroll_in_view_follows_a_spring() {
    assert_spring(0, (2, 0), (0, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_inBounds_withOffset
#[test]
fn an_animated_scroll_in_view_with_an_offset_follows_a_spring() {
    assert_spring(0, (2, BIG / 2), (0, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_outOfBounds
#[test]
fn an_animated_scroll_out_of_view_follows_a_spring() {
    assert_spring(0, (10, 0), (0, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_firstItem
#[test]
fn an_animated_scroll_to_the_first_item_follows_a_spring() {
    assert_spring(0, (0, 0), (10, 10));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_firstItem_toOffset
#[test]
fn an_animated_scroll_to_the_first_item_with_an_offset_follows_a_spring() {
    assert_spring(0, (0, 10), (10, 10));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_outOfBounds_withSpacing
#[test]
fn an_animated_scroll_with_spacing_follows_a_spring() {
    assert_spring(10, (20, 0), (0, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_outOfBounds_withNegativeSpacing
#[test]
fn an_animated_scroll_with_negative_spacing_follows_a_spring() {
    assert_spring(-10, (20, 0), (0, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_backwards_withSpacing
#[test]
fn an_animated_scroll_back_with_spacing_follows_a_spring() {
    assert_spring(10, (0, 0), (20, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridAnimatedScrollTest.kt animateScrollToItem_backwards_withNegativeSpacing
#[test]
fn an_animated_scroll_back_with_negative_spacing_follows_a_spring() {
    assert_spring(-10, (0, 0), (20, 0));
}

#[test]
fn a_very_long_scroll_lands_with_the_items_in_their_lanes() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM * 2), (0, 0), move |scope| {
            scope.items(10_000, move |index| along(lazy, ITEM, index));
        });
        scroll_by(&mut ui, state, ITEM * 2000);
        assert_eq!(position(state), (4000, 0));
        assert_at(&mut ui, lazy, "4000", 0, 0);
        assert_at(&mut ui, lazy, "4001", ITEM, 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridArrangementsTest.kt spacedByWithAlignment
#[test]
fn lanes_spaced_and_aligned_to_the_end() {
    for lazy in orientations() {
        let (mut ui, _) = big_grid(
            lazy,
            1,
            (BIG * 5, BIG * 5),
            (0, 0),
            |mut args| {
                args.cells = StaggeredGridCells::FixedSize(dp(BIG * 2));
                args.cross_spacing = (BIG / 2) as f32;
                args.cross_end = true;
                args
            },
            boxes(10),
        );
        assert_eq!(lazy.cross_start(&mut ui, "0"), (BIG / 2) as f32);
        assert_eq!(lazy.cross_size(&mut ui, "0"), (BIG * 2) as f32);
        assert_eq!(lazy.cross_start(&mut ui, "1"), (BIG * 3) as f32);
        assert_eq!(lazy.cross_size(&mut ui, "1"), (BIG * 2) as f32);
    }
}

#[test]
fn lanes_share_what_is_left_from_the_first() {
    for lazy in orientations() {
        let (mut ui, _) = big_grid(
            lazy,
            3,
            (100, 500),
            (0, 0),
            |args| args,
            move |scope| {
                scope.items(3, move |index| along(lazy, 10, index));
            },
        );
        assert_eq!(lazy.cross_size(&mut ui, "0"), 34.0);
        assert_eq!(lazy.cross_size(&mut ui, "1"), 33.0);
        assert_eq!(lazy.cross_size(&mut ui, "2"), 33.0);
        assert_eq!(lazy.cross_start(&mut ui, "2"), 67.0);
    }
}

#[test]
fn a_fixed_size_too_large_is_one_lane_of_the_room() {
    for lazy in orientations() {
        let (mut ui, _) = big_grid(
            lazy,
            1,
            (100, 500),
            (0, 0),
            |mut args| {
                args.cells = StaggeredGridCells::FixedSize(dp(BIG * 2));
                args
            },
            move |scope| scope.item(move || along(lazy, 10, "0")),
        );
        assert_eq!(lazy.cross_size(&mut ui, "0"), 100.0);
    }
}

/// A full line whose lanes ended apart, scrolled past and back a step at a time: its gaps count
/// going up as they did going down.
#[test]
fn going_back_over_a_full_line_counts_its_gaps() {
    for lazy in orientations() {
        let (mut ui, state) = keeping(two_lanes(lazy, ITEM), (0, 0), move |scope| {
            scope.item(move || along(lazy, ITEM * 3, "0"));
            scope.item(move || along(lazy, ITEM, "1"));
            scope.item_with_span((), StaggeredGridItemSpan::FullLine, move || {
                along(lazy, ITEM, "2")
            });
            scope.items(10, move |index| along(lazy, ITEM, index + 3));
        });
        scroll_by(&mut ui, state, ITEM * 6);
        scroll_by(&mut ui, state, -ITEM * 3);
        assert_eq!(lazy.main_start(&mut ui, "2"), 0.0);
        scroll_by(&mut ui, state, -ITEM);
        assert_eq!(lazy.main_start(&mut ui, "0"), (-ITEM * 2) as f32);
        assert_eq!(lazy.main_start(&mut ui, "2"), ITEM as f32);
    }
}

#[test]
fn more_lanes_after_a_resize_keep_the_position() {
    for lazy in orientations() {
        let lanes = mutable_state_of(2usize);
        let read = lanes.handle();
        let (mut ui, state) = keeping(
            move |state| {
                let cross = ITEM * read.get() as i32;
                Args::new(lazy, read.get(), axis_size(lazy, cross, ITEM * 2)).state(state)
            },
            (0, 0),
            move |scope| scope.items(30, move |index| along(lazy, ITEM, index)),
        );
        scroll_by(&mut ui, state, ITEM * 2);
        assert_eq!(state.first_visible_item_index(), 4);
        lanes.set(3);
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 4);
        assert_at(&mut ui, lazy, "4", 0, 0);
        assert_at(&mut ui, lazy, "5", ITEM, 0);
        assert_at(&mut ui, lazy, "6", ITEM * 2, 0);
        assert_at(&mut ui, lazy, "7", 0, ITEM);
        scroll_by(&mut ui, state, -ITEM);
        assert_eq!(lazy.main_start(&mut ui, "4"), ITEM as f32);
    }
}
