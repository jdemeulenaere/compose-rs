//! `Modifier::animate_item` on lazy grids: moves between cells and out of view, across lines and
//! spans, fades in and out. What's the list's alone is in `lazy_animate_item.rs`.

#![allow(non_snake_case)]

use compose_animation::AnimationSpec;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, Dp, IntSize, UnitExt};

use crate::lazy_animate_support::*;

const ITEM: i32 = 40;
const ITEM2: i32 = 24;
const SPACING: i32 = 8;
const GRID_TAG: &str = "grid";

fn dp(px: i32) -> Dp {
    px.dp()
}

fn f(px: i32) -> f32 {
    px as f32
}

#[derive(Clone, Copy, Debug)]
struct Params {
    vertical: bool,
    reverse: bool,
}

const PARAMS: [Params; 4] = [
    Params { vertical: true, reverse: false },
    Params { vertical: false, reverse: false },
    Params { vertical: true, reverse: true },
    Params { vertical: false, reverse: true },
];

#[derive(Clone, Copy)]
struct GridOptions {
    cells: usize,
    max_size: i32,
    min_size: i32,
    start_index: usize,
    spacing: i32,
}

impl GridOptions {
    fn new(cells: usize, max_size: i32) -> GridOptions {
        GridOptions { cells, max_size, min_size: 0, start_index: 0, spacing: 0 }
    }
}

fn lazy_grid(
    params: Params,
    options: GridOptions,
    state: LazyGridState,
    content: impl Fn(LazyGridScope) + Clone + 'static,
) {
    let GridOptions { cells, max_size, min_size, spacing, .. } = options;
    let cross = dp(cells as i32 * ITEM + (cells as i32 - 1) * spacing);
    let modifier = if params.vertical {
        Modifier.width(cross).height_in(dp(min_size), dp(max_size))
    } else {
        Modifier.height(cross).width_in(dp(min_size), dp(max_size))
    }
    .test_tag(GRID_TAG);
    if params.vertical {
        let arrangement: VerticalArrangement = if spacing != 0 {
            Arrangement::spaced_by(dp(spacing)).into()
        } else if params.reverse {
            Arrangement::BOTTOM
        } else {
            Arrangement::TOP
        };
        LazyVerticalGrid!(
            GridCells::Fixed(cells),
            modifier = modifier,
            state = state,
            reverse_layout = params.reverse,
            vertical_arrangement = arrangement,
            horizontal_arrangement = Arrangement::spaced_by(dp(spacing)),
            content = content
        );
    } else {
        let arrangement: HorizontalArrangement = if spacing != 0 {
            Arrangement::spaced_by(dp(spacing)).into()
        } else if params.reverse {
            Arrangement::END
        } else {
            Arrangement::START
        };
        LazyHorizontalGrid!(
            GridCells::Fixed(cells),
            modifier = modifier,
            state = state,
            reverse_layout = params.reverse,
            horizontal_arrangement = arrangement,
            vertical_arrangement = Arrangement::spaced_by(dp(spacing)),
            content = content
        );
    }
}

fn item(
    scope: &LazyGridItemScope,
    params: Params,
    tag: i32,
    size: i32,
    spec: Option<AnimationSpec>,
) {
    let base = match spec {
        Some(spec) => Modifier.animate_item_with(scope, None, Some(spec), None),
        None => Modifier,
    };
    let sized = if params.vertical { base.height(dp(size)) } else { base.width(dp(size)) };
    let sized = if params.vertical { sized.fill_max_width() } else { sized.fill_max_height() };
    Box!(modifier = sized.test_tag(tag.to_string()));
}

struct Fixture {
    ui: ComposeTestRule,
    list: OwnedState<Vec<i32>>,
    state: LazyGridState,
}

fn fixture_of(
    params: Params,
    options: GridOptions,
    list: Vec<i32>,
    content: impl Fn(&LazyGridScope, Vec<i32>) + Clone + 'static,
) -> Fixture {
    let list = mutable_state_of(list);
    let handle = list.handle();
    let state = LazyGridState::new(options.start_index, 0);
    let state_handle = *state;
    let ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let items = handle.get();
        let content = content.clone();
        lazy_grid(params, options, state_handle, move |scope| {
            content(&scope, items.clone());
        });
    });
    Fixture { ui, list, state: state_handle }
}

fn plain_items(params: Params) -> impl Fn(&LazyGridScope, Vec<i32>) + Clone {
    move |scope: &LazyGridScope, list: Vec<i32>| {
        let item_scope = scope.item_scope();
        scope.items_list_keyed(
            list,
            |it: &i32| *it,
            move |it: &i32| item(&item_scope, params, *it, ITEM, Some(anim_spec())),
        );
    }
}

fn plain(params: Params, options: GridOptions, list: Vec<i32>) -> Fixture {
    fixture_of(params, options, list, plain_items(params))
}

impl Fixture {
    fn set_list(&mut self, list: Vec<i32>) {
        let handle = self.list.handle();
        self.ui.run_on_ui_thread(move |_| handle.set(list));
    }
}

/// Compose's `assertPositions`, each `(tag, cross, main)`: `main` from the other end when reversed.
fn assert_positions_of(
    ui: &mut ComposeTestRule,
    params: Params,
    expected: &[(i32, f32, f32)],
    fraction: f32,
) {
    let (list_x, list_y) = position_of(ui, GRID_TAG);
    let (list_width, list_height) = size_of(ui, GRID_TAG);
    let container = if params.vertical { list_height } else { list_width };
    for &(tag, cross, main) in expected {
        let name = tag.to_string();
        let (x, y) = position_of(ui, &name);
        let (width, height) = size_of(ui, &name);
        let (actual_cross, actual_main, size) = if params.vertical {
            (x - list_x, y - list_y, height)
        } else {
            (y - list_y, x - list_x, width)
        };
        let wanted_main = main.round() as i32;
        let wanted_main = if params.reverse { container - size - wanted_main } else { wanted_main };
        assert_eq!(
            (actual_cross, actual_main),
            (cross.round() as i32, wanted_main),
            "item {tag} (across, along) at fraction {fraction}, {params:?}"
        );
    }
}

/// `assert_positions_of` for those `in_view`; the others aren't displayed.
fn assert_in_view(
    ui: &mut ComposeTestRule,
    params: Params,
    fraction: f32,
    items: &[(i32, f32, f32)],
    in_view: impl Fn(f32) -> bool,
) {
    let mut expected = Vec::new();
    for &(tag, cross, main) in items {
        if in_view(main) {
            expected.push((tag, cross, main));
        } else {
            ui.on_node_with_tag(&tag.to_string()).assert_is_not_displayed();
        }
    }
    assert_positions_of(ui, params, &expected, fraction);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, reorderTwoItems
#[test]
fn two_items_of_one_column_swap() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(1, ITEM * 2), vec![0, 1]);
        fixture.set_list(vec![1, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0, f(ITEM) * fraction), (1, 0.0, f(ITEM) - f(ITEM) * fraction)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, reorderTwoByTwoItems
#[test]
fn four_items_of_two_lines_change_places_both_ways() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(2, ITEM * 2), vec![0, 1, 2, 3]);
        assert_positions_of(
            &mut fixture.ui,
            params,
            &[(0, 0.0, 0.0), (1, f(ITEM), 0.0), (2, 0.0, f(ITEM)), (3, f(ITEM), f(ITEM))],
            0.0,
        );
        fixture.set_list(vec![3, 2, 1, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let (up, down) = (f(ITEM) * fraction, f(ITEM) - f(ITEM) * fraction);
            assert_positions_of(
                ui,
                params,
                &[(0, up, up), (1, down, up), (2, up, down), (3, down, down)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, reorderTwoItems_layoutInfoHasFinalPositions
#[test]
fn a_grids_layout_info_has_the_final_positions_at_once() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(2, ITEM * 2), vec![0, 1, 2, 3]);
        fixture.set_list(vec![3, 2, 1, 0]);
        let state = fixture.state;
        on_animation_frame(&mut fixture.ui, |_, _| {
            let items: Vec<(usize, i32)> = state
                .layout_info()
                .visible_items_info
                .iter()
                .map(|item| {
                    let main = if params.vertical { item.offset.y } else { item.offset.x };
                    (item.index, main)
                })
                .collect();
            assert_eq!(items, vec![(0, 0), (1, 0), (2, ITEM), (3, ITEM)], "{params:?}");
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, moveFirstItemToEndCausingAllItemsToAnimate
#[test]
fn the_first_item_moved_to_the_end_moves_all_cells() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(2, ITEM * 3), vec![0, 1, 2, 3, 4]);
        fixture.set_list(vec![1, 2, 3, 4, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let one = f(ITEM) * fraction;
            assert_positions_of(
                ui,
                params,
                &[
                    (0, 0.0, f(ITEM * 2) * fraction),
                    (1, f(ITEM) - one, 0.0),
                    (2, one, f(ITEM) - one),
                    (3, f(ITEM) - one, f(ITEM)),
                    (4, one, f(ITEM * 2) - one),
                ],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, itemSizeChangeAnimatesNextItems
#[test]
fn a_line_growing_moves_the_next_ones() {
    for params in PARAMS {
        let size = mutable_state_of(ITEM);
        let size_handle = size.handle();
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = paused(IntSize::new(1000, 1000), move || {
            let _ = &state;
            let grown = size_handle.get();
            let options = GridOptions { min_size: ITEM * 5, ..GridOptions::new(1, ITEM * 5) };
            lazy_grid(params, options, handle, move |scope| {
                let item_scope = scope.item_scope();
                scope.items_list_keyed(
                    vec![0, 1, 2],
                    |it: &i32| *it,
                    move |it: &i32| {
                        let main = if *it == 1 { grown } else { ITEM };
                        item(&item_scope, params, *it, main, Some(anim_spec()));
                    },
                );
            });
        });
        ui.run_on_ui_thread(|_| size.set(ITEM * 2));
        on_animation_frame(&mut ui, |ui, fraction| {
            let third = f(ITEM * 2) + f(ITEM) * fraction;
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0, 0.0), (1, 0.0, f(ITEM)), (2, 0.0, third)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, onlyItemsWithModifierAnimates
#[test]
fn only_the_cells_with_the_modifier_animate() {
    for params in PARAMS {
        let content = move |scope: &LazyGridScope, list: Vec<i32>| {
            let item_scope = scope.item_scope();
            scope.items_list_keyed(
                list,
                |it: &i32| *it,
                move |it: &i32| {
                    let spec = (*it == 1 || *it == 3).then(anim_spec);
                    item(&item_scope, params, *it, ITEM, spec);
                },
            );
        };
        let mut fixture =
            fixture_of(params, GridOptions::new(1, ITEM * 5), vec![0, 1, 2, 3, 4], content);
        fixture.set_list(vec![1, 2, 3, 4, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, 0.0, f(ITEM * 4)),
                    (1, 0.0, f(ITEM) - f(ITEM) * fraction),
                    (2, 0.0, f(ITEM)),
                    (3, 0.0, f(ITEM * 3) - f(ITEM) * fraction),
                    (4, 0.0, f(ITEM * 3)),
                ],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, moveItemToTheBottomOutsideOfBounds
#[test]
fn a_cell_moved_past_the_end_stays_composed_while_it_leaves() {
    for params in PARAMS {
        let mut fixture =
            plain(params, GridOptions::new(2, ITEM * 2), vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        fixture.set_list(vec![0, 8, 2, 3, 4, 5, 6, 7, 1, 9]);
        let grid = ITEM * 2;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = f(grid) * fraction;
            let item8 = f(grid) - f(grid) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[(0, 0.0, 0.0), (1, f(ITEM), item1), (8, f(ITEM), item8)],
                |main| main < f(grid),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, moveItemToTheTopOutsideOfBounds
#[test]
fn a_cell_moved_past_the_start_stays_composed_while_it_leaves() {
    for params in PARAMS {
        let options = GridOptions { start_index: 6, ..GridOptions::new(2, ITEM * 2) };
        let mut fixture = plain(params, options, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_positions_of(
            &mut fixture.ui,
            params,
            &[(6, 0.0, 0.0), (7, f(ITEM), 0.0), (8, 0.0, f(ITEM)), (9, f(ITEM), f(ITEM))],
            0.0,
        );
        fixture.set_list(vec![0, 8, 2, 3, 4, 5, 6, 7, 1, 9]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = -f(ITEM) + f(ITEM * 2) * fraction;
            let item8 = f(ITEM) - f(ITEM * 2) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (1, 0.0, item1),
                    (6, 0.0, 0.0),
                    (7, f(ITEM), 0.0),
                    (8, 0.0, item8),
                    (9, f(ITEM), f(ITEM)),
                ],
                |main| main > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, moveFirstItemToEndCausingAllItemsToAnimate_withSpacing
#[test]
fn the_first_item_moved_to_the_end_moves_all_cells_with_spacing() {
    for params in PARAMS {
        let step = f(ITEM + SPACING);
        let options = GridOptions { spacing: SPACING, ..GridOptions::new(2, ITEM * 2 + SPACING) };
        let mut fixture = plain(params, options, vec![0, 1, 2, 3]);
        fixture.set_list(vec![1, 2, 3, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let one = step * fraction;
            assert_positions_of(
                ui,
                params,
                &[(0, one, one), (1, step - one, 0.0), (2, one, step - one), (3, step - one, step)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, animatingItemsWithPreviousIndexLargerThanTheNewItemCount
#[test]
fn items_at_an_index_past_the_new_count_still_move_in() {
    for params in PARAMS {
        let mut fixture =
            plain(params, GridOptions::new(2, ITEM * 2), vec![0, 1, 2, 3, 4, 5, 6, 7]);
        fixture.set_list(vec![0, 2, 4, 6]);
        let grid = ITEM * 2;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let main = f(grid) - f(grid - ITEM) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (0, 0.0, 0.0),
                    (2, f(ITEM) * fraction, f(ITEM) * (1.0 - fraction)),
                    (4, 0.0, main),
                    (6, f(ITEM), main),
                ],
                |at| at < f(grid),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, animatingItemsWithPreviousIndexLargerThanTheNewItemCount_differentSpans
#[test]
fn items_of_other_spans_at_an_index_past_the_new_count_still_move_in() {
    for params in PARAMS {
        let content = move |scope: &LazyGridScope, list: Vec<i32>| {
            let item_scope = scope.item_scope();
            scope.items_list_keyed_with_span(
                list,
                |it: &i32| *it,
                |span_scope: LazyGridItemSpanScope, it: &i32| {
                    GridItemSpan(if *it == 6 { span_scope.max_line_span() } else { 1 })
                },
                |_: &i32| (),
                move |it: &i32| item(&item_scope, params, *it, ITEM, Some(anim_spec())),
            );
        };
        let mut fixture =
            fixture_of(params, GridOptions::new(2, ITEM * 2), vec![0, 1, 2, 3, 4, 5, 6], content);
        fixture.set_list(vec![0, 4, 6]);
        let grid = ITEM * 2;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (0, 0.0, 0.0),
                    (4, f(ITEM), f(grid) - f(grid) * fraction),
                    (6, 0.0, f(grid + ITEM) - f(grid) * fraction),
                ],
                |at| at < f(grid),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, moveTwoItemsToTheTopOutsideOfBounds_cellsOfTheSameLine
#[test]
fn cells_of_a_line_moved_past_the_start_stay_on_one_line() {
    for params in PARAMS {
        let options = GridOptions { start_index: 3, ..GridOptions::new(2, ITEM * 2) };
        let mut fixture = plain(params, options, vec![0, 1, 2, 3, 4, 5]);
        fixture.set_list(vec![4, 5, 2, 3, 0, 1]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let in_from_start = -f(ITEM) + f(ITEM * 2) * fraction;
            let out_to_start = f(ITEM) - f(ITEM * 2) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (0, 0.0, in_from_start),
                    (1, f(ITEM), in_from_start),
                    (2, 0.0, 0.0),
                    (3, f(ITEM), 0.0),
                    (4, 0.0, out_to_start),
                    (5, f(ITEM), out_to_start),
                ],
                |at| at > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, moveTwoItemsToTheBottomOutsideOfBounds_cellsOfTheSameLine
#[test]
fn cells_of_a_line_moved_past_the_end_stay_on_one_line() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(2, ITEM * 2), vec![0, 1, 2, 3, 4, 5]);
        fixture.set_list(vec![0, 1, 4, 5, 2, 3]);
        let grid = ITEM * 2;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let leaving = f(ITEM) + f(ITEM) * fraction;
            let coming = f(grid) - f(ITEM) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (0, 0.0, 0.0),
                    (1, f(ITEM), 0.0),
                    (2, 0.0, leaving),
                    (3, f(ITEM), leaving),
                    (4, 0.0, coming),
                    (5, f(ITEM), coming),
                ],
                |at| at < f(grid),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, noAnimationWhenScrollForwardByLargeOffset_multipleCells
#[test]
fn a_large_scroll_of_a_grid_animates_nothing() {
    for params in PARAMS {
        let list: Vec<i32> = (0..18).collect();
        let mut fixture = plain(params, GridOptions::new(3, ITEM * 3), list);
        let state = fixture.state;
        fixture.ui.run_on_ui_thread(move |_| state.dispatch_raw_delta((ITEM * 5 / 2) as f32));
        fixture.ui.host().frame();
        let half = f(ITEM / 2);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (6, 0.0, -half),
                    (7, f(ITEM), -half),
                    (9, 0.0, half),
                    (12, 0.0, half * 3.0),
                    (15, 0.0, half * 5.0),
                ],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, noAnimationWhenParentSizeShrinks
#[test]
fn a_grid_shrinking_animates_nothing() {
    for params in PARAMS {
        let size = mutable_state_of(ITEM * 3);
        let handle = size.handle();
        let state = LazyGridState::new(0, 0);
        let state_handle = *state;
        let mut ui = paused(IntSize::new(1000, 1000), move || {
            let _ = &state;
            let options = GridOptions::new(1, handle.get());
            lazy_grid(params, options, state_handle, move |scope| {
                plain_items(params)(&scope, vec![0, 1, 2]);
            });
        });
        ui.run_on_ui_thread(|_| size.set(ITEM * 2));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(ui, params, &[(0, 0.0, 0.0), (1, 0.0, f(ITEM))], fraction);
            ui.on_node_with_tag("2").assert_is_not_displayed();
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, scrollIsAffectingItemsMovingWithinViewport
#[test]
fn a_scroll_moves_the_cells_moving_in_view() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(1, ITEM * 2), vec![0, 1, 2, 3]);
        let state = fixture.state;
        fixture.set_list(vec![0, 2, 1, 3]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            if fraction == 0.0 {
                ui.run_on_ui_thread(move |_| state.dispatch_raw_delta(SPACING as f32));
                ui.host().frame();
            }
            let scrolled = f(SPACING);
            assert_positions_of(
                ui,
                params,
                &[
                    (0, 0.0, -scrolled),
                    (1, 0.0, f(ITEM) - scrolled + f(ITEM) * fraction),
                    (2, 0.0, f(ITEM * 2) - scrolled - f(ITEM) * fraction),
                ],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemPlacementAnimationTest.kt, animationWhenTryingToStayInTheStart
#[test]
fn staying_at_the_start_of_a_grid_over_a_new_first_item_animates() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(1, ITEM * 3), vec![1, 2, 3, 4]);
        let state = fixture.state;
        let handle = fixture.list.handle();
        fixture.ui.run_on_ui_thread(move |_| {
            handle.set(vec![0, 1, 2, 3]);
            state.request_scroll_to_item(0, 0);
        });
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, 0.0, 0.0),
                    (1, 0.0, f(ITEM) * fraction),
                    (2, 0.0, f(ITEM) + f(ITEM) * fraction),
                ],
                fraction,
            );
        });
    }
}

/// Colored cells for the fades: two to a line, the first two lines in view.
struct FadeFixture {
    ui: ComposeTestRule,
    list: OwnedState<Vec<Color>>,
}

fn fade_grid(colors: Vec<Color>, fade_in: bool, fade_out: bool, wrap: bool) -> FadeFixture {
    let list = mutable_state_of(colors);
    let handle = list.handle();
    let ui = paused(IntSize::new(1000, 1000), move || {
        let colors = handle.get();
        let modifier = if wrap {
            Modifier.width(dp(ITEM * 2)).wrap_content_height().test_tag(GRID_TAG)
        } else {
            Modifier.size((dp(ITEM * 2), dp(ITEM * 2))).test_tag(GRID_TAG)
        };
        LazyVerticalGrid!(GridCells::Fixed(2), modifier = modifier, move |scope: LazyGridScope| {
            let item_scope = scope.item_scope();
            scope.items_list_keyed(
                colors.clone(),
                |color: &Color| color.argb(),
                move |color: &Color| {
                    let modifier = Modifier.animate_item_with(
                        &item_scope,
                        fade_in.then(anim_spec),
                        None,
                        fade_out.then(anim_spec),
                    );
                    Box!(modifier = modifier.height(dp(ITEM)).background(*color));
                },
            );
        });
    });
    FadeFixture { ui, list }
}

impl FadeFixture {
    fn set(&mut self, colors: Vec<Color>) {
        let handle = self.list.handle();
        self.ui.run_on_ui_thread(move |_| handle.set(colors));
    }
}

fn assert_cells(ui: &mut ComposeTestRule, fraction: f32, expected: &[Option<(Color, f32)>]) {
    for (cell, wanted) in expected.iter().enumerate() {
        let x = (cell as i32 % 2 * ITEM + ITEM / 2) as f32;
        let y = (cell as i32 / 2 * ITEM + ITEM / 2) as f32;
        let color = color_at(ui, x, y);
        match wanted {
            Some((wanted, alpha)) => assert!(
                color.is(*wanted, *alpha),
                "cell {cell} at fraction {fraction}: {color:?}, wanted {wanted:?} at {alpha}"
            ),
            None => assert!(color.is_transparent(), "cell {cell} at {fraction}: {color:?}"),
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemAppearanceAnimationTest.kt, oneItemAdded
#[test]
fn a_cell_added_fades_in() {
    let mut fixture = fade_grid(Vec::new(), true, false, false);
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_cells(ui, fraction, &[Some((Color::BLACK, fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemAppearanceAnimationTest.kt, noAnimationForInitialList
#[test]
fn a_grids_first_cells_show_at_once() {
    let mut fixture = fade_grid(vec![Color::BLACK], true, false, false);
    assert_cells(&mut fixture.ui, 0.0, &[Some((Color::BLACK, 1.0))]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemAppearanceAnimationTest.kt, oneExistTwoAdded
#[test]
fn only_the_cells_added_fade_in() {
    let mut fixture = fade_grid(vec![Color::BLACK], true, false, false);
    fixture.set(vec![Color::BLACK, Color::RED, Color::GREEN]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_cells(
            ui,
            fraction,
            &[
                Some((Color::BLACK, 1.0)),
                Some((Color::RED, fraction)),
                Some((Color::GREEN, fraction)),
            ],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemDisappearanceAnimationTest.kt, oneItemRemoved
#[test]
fn a_cell_removed_fades_out_after_it_left_the_composition() {
    let mut fixture = fade_grid(vec![Color::BLACK], false, true, false);
    fixture.set(Vec::new());
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_cells(ui, fraction, &[Some((Color::BLACK, 1.0 - fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemDisappearanceAnimationTest.kt, threeExistTwoRemoved
#[test]
fn only_the_cells_removed_fade_out() {
    let mut fixture = fade_grid(vec![Color::BLACK, Color::RED, Color::GREEN], false, true, false);
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let alpha = 1.0 - fraction;
        assert_cells(
            ui,
            fraction,
            &[Some((Color::BLACK, 1.0)), Some((Color::RED, alpha)), Some((Color::GREEN, alpha))],
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemDisappearanceAnimationTest.kt, itemsBeingRemovedAreAffectingTheContainerSizeForTheDurationOfAnimation
#[test]
fn a_cell_fading_out_keeps_the_room_it_took() {
    let mut fixture = fade_grid(vec![Color::BLACK, Color::RED, Color::GREEN], false, true, true);
    assert_eq!(size_of(&mut fixture.ui, GRID_TAG).1, ITEM * 2);
    fixture.set(vec![Color::BLACK, Color::RED]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let wanted = if fraction < 1.0 { ITEM * 2 } else { ITEM };
        assert_eq!(size_of(ui, GRID_TAG).1, wanted, "fraction {fraction}");
    });
}

/// Cells of a line move each by itself, whatever the line's size.
#[test]
fn a_shorter_cell_doesnt_stop_a_cell_of_its_line_from_moving() {
    for params in PARAMS {
        let content = move |scope: &LazyGridScope, list: Vec<i32>| {
            let item_scope = scope.item_scope();
            scope.items_list_keyed(
                list,
                |it: &i32| *it,
                move |it: &i32| {
                    let size = if *it == 0 { ITEM2 } else { ITEM };
                    item(&item_scope, params, *it, size, Some(anim_spec()));
                },
            );
        };
        let mut fixture = fixture_of(params, GridOptions::new(2, ITEM * 2), vec![0, 1], content);
        fixture.set_list(vec![1, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, f(ITEM) * fraction, 0.0), (1, f(ITEM) - f(ITEM) * fraction, 0.0)],
                fraction,
            );
        });
    }
}

/// Right to left, a vertical grid's cells go the other way across it, and moves with them.
#[test]
fn cells_right_to_left_swap_on_screen() {
    let list = mutable_state_of(vec![0, 1]);
    let handle = list.handle();
    let params = PARAMS[0];
    let state = LazyGridState::new(0, 0);
    let state_handle = *state;
    let mut ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let items = handle.get();
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_LAYOUT_DIRECTION.provides(compose_ui::LayoutDirection::Rtl),
            || {
                let items = items.clone();
                lazy_grid(params, GridOptions::new(2, ITEM), state_handle, move |scope| {
                    plain_items(params)(&scope, items.clone());
                });
            },
        );
    });
    assert_positions_of(&mut ui, params, &[(0, f(ITEM), 0.0), (1, 0.0, 0.0)], 0.0);
    ui.run_on_ui_thread(|_| list.set(vec![1, 0]));
    on_animation_frame(&mut ui, |ui, fraction| {
        assert_positions_of(
            ui,
            params,
            &[(0, f(ITEM) - f(ITEM) * fraction, 0.0), (1, f(ITEM) * fraction, 0.0)],
            fraction,
        );
    });
}
