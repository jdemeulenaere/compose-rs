//! `Modifier::animate_item` on lazy staggered grids: moves between lanes and out of view, full
//! lines, fades in and out. What's the animator's alone is in `lazy_animate_item.rs`.

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
    state: LazyStaggeredGridState,
    content: impl Fn(LazyStaggeredGridScope) + Clone + 'static,
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
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(cells),
            modifier = modifier,
            state = state,
            reverse_layout = params.reverse,
            vertical_item_spacing = dp(spacing),
            horizontal_arrangement = Arrangement::spaced_by(dp(spacing)),
            content = content
        );
    } else {
        LazyHorizontalStaggeredGrid!(
            StaggeredGridCells::Fixed(cells),
            modifier = modifier,
            state = state,
            reverse_layout = params.reverse,
            horizontal_item_spacing = dp(spacing),
            vertical_arrangement = Arrangement::spaced_by(dp(spacing)),
            content = content
        );
    }
}

fn item(params: Params, tag: i32, size: i32, spec: Option<AnimationSpec>) {
    let scope = LazyStaggeredGridItemScope;
    let base = match spec {
        Some(spec) => Modifier.animate_item_with(&scope, None, Some(spec), None),
        None => Modifier,
    };
    let sized = if params.vertical { base.height(dp(size)) } else { base.width(dp(size)) };
    let sized = if params.vertical { sized.fill_max_width() } else { sized.fill_max_height() };
    Box!(modifier = sized.test_tag(tag.to_string()));
}

struct Fixture {
    ui: ComposeTestRule,
    list: OwnedState<Vec<i32>>,
    state: LazyStaggeredGridState,
}

fn fixture_of(
    params: Params,
    options: GridOptions,
    list: Vec<i32>,
    content: impl Fn(&LazyStaggeredGridScope, Vec<i32>) + Clone + 'static,
) -> Fixture {
    let list = mutable_state_of(list);
    let handle = list.handle();
    let state = LazyStaggeredGridState::new(options.start_index, 0);
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

fn plain_items(params: Params) -> impl Fn(&LazyStaggeredGridScope, Vec<i32>) + Clone {
    move |scope: &LazyStaggeredGridScope, list: Vec<i32>| {
        scope.items_list_keyed(
            list,
            |it: &i32| *it,
            move |it: &i32| item(params, *it, ITEM, Some(anim_spec())),
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

fn scroll_grid(ui: &mut ComposeTestRule, state: LazyStaggeredGridState, by: i32) {
    ui.run_on_ui_thread(move |_| state.dispatch_raw_delta(by as f32));
    ui.host().frame();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, reorderTwoItems
#[test]
fn two_items_of_one_lane_swap() {
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, reorderTwoByTwoItems
#[test]
fn four_items_of_two_lanes_change_places_both_ways() {
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, reorderTwoItems_layoutInfoHasFinalPositions
#[test]
fn a_staggered_layout_info_has_the_final_positions_at_once() {
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, reorderFirstAndLastItems
#[test]
fn the_first_and_last_of_a_lane_swap() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(1, ITEM * 5), vec![0, 1, 2, 3, 4]);
        fixture.set_list(vec![4, 1, 2, 3, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[
                    (0, 0.0, f(ITEM * 4) * fraction),
                    (1, 0.0, f(ITEM)),
                    (2, 0.0, f(ITEM * 2)),
                    (3, 0.0, f(ITEM * 3)),
                    (4, 0.0, f(ITEM * 4) - f(ITEM * 4) * fraction),
                ],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, itemSizeChangeAnimatesNextItems
#[test]
fn an_item_of_a_lane_growing_moves_the_next_ones() {
    for params in PARAMS {
        let size = mutable_state_of(ITEM);
        let size_handle = size.handle();
        let state = LazyStaggeredGridState::new(0, 0);
        let handle = *state;
        let mut ui = paused(IntSize::new(1000, 1000), move || {
            let _ = &state;
            let grown = size_handle.get();
            let options = GridOptions { min_size: ITEM * 5, ..GridOptions::new(1, ITEM * 5) };
            lazy_grid(params, options, handle, move |scope| {
                scope.items_list_keyed(
                    vec![0, 1, 2],
                    |it: &i32| *it,
                    move |it: &i32| {
                        let main = if *it == 1 { grown } else { ITEM };
                        item(params, *it, main, Some(anim_spec()));
                    },
                );
            });
        });
        ui.run_on_ui_thread(|_| size.set(ITEM * 2));
        on_animation_frame(&mut ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0, 0.0), (1, 0.0, f(ITEM)), (2, 0.0, f(ITEM * 2) + f(ITEM) * fraction)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, multipleChildrenPerItem
#[test]
fn each_child_of_a_staggered_item_moves() {
    for params in PARAMS {
        let content = move |scope: &LazyStaggeredGridScope, list: Vec<i32>| {
            scope.items_list_keyed(
                list,
                |it: &i32| *it,
                move |it: &i32| {
                    item(params, *it, ITEM, Some(anim_spec()));
                    item(params, *it + 1, ITEM, Some(anim_spec()));
                },
            );
        };
        let mut fixture = fixture_of(params, GridOptions::new(1, ITEM * 2), vec![0, 2], content);
        assert_positions_of(
            &mut fixture.ui,
            params,
            &[(0, 0.0, 0.0), (1, 0.0, 0.0), (2, 0.0, f(ITEM)), (3, 0.0, f(ITEM))],
            0.0,
        );
        fixture.set_list(vec![2, 0]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let (up, down) = (f(ITEM) * fraction, f(ITEM) - f(ITEM) * fraction);
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0, up), (1, 0.0, up), (2, 0.0, down), (3, 0.0, down)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, moveItemToTheBottomOutsideOfBounds
#[test]
fn a_staggered_item_moved_past_the_end_stays_composed_while_it_leaves() {
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, moveItemToTheTopOutsideOfBounds
#[test]
fn a_staggered_item_moved_past_the_start_stays_composed_while_it_leaves() {
    for params in PARAMS {
        let options = GridOptions { start_index: 6, ..GridOptions::new(2, ITEM * 2) };
        let mut fixture = plain(params, options, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        fixture.set_list(vec![0, 8, 2, 3, 4, 5, 6, 7, 1, 9]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item1 = -f(ITEM) + f(ITEM * 2) * fraction;
            let item8 = f(ITEM) - f(ITEM * 2) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[(1, 0.0, item1), (6, 0.0, 0.0), (7, f(ITEM), 0.0), (8, 0.0, item8)],
                |main| main > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, moveFirstItemToEndCausingAllItemsToAnimate_withSpacing
#[test]
fn the_first_item_moved_to_the_end_moves_all_of_the_lanes_with_spacing() {
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, animatingItemWithPreviousIndexLargerThanTheNewItemCount
#[test]
fn an_item_at_an_index_past_the_new_count_still_moves_in() {
    for params in PARAMS {
        let mut fixture =
            plain(params, GridOptions::new(1, ITEM * 2), vec![0, 1, 2, 3, 4, 5, 6, 7]);
        fixture.set_list(vec![0, 6]);
        let grid = ITEM * 2;
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let item6 = f(grid) - f(grid - ITEM) * fraction;
            assert_in_view(ui, params, fraction, &[(0, 0.0, 0.0), (6, 0.0, item6)], |at| {
                at < f(grid)
            });
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, moveTwoItemsToTheTopOutsideOfBounds_itemsOfDifferentLanes
#[test]
fn items_of_two_lanes_moved_past_the_start_stay_in_their_lanes() {
    for params in PARAMS {
        let options = GridOptions { start_index: 2, ..GridOptions::new(2, ITEM * 2) };
        let mut fixture = plain(params, options, vec![0, 1, 2, 3, 4, 5]);
        assert_positions_of(
            &mut fixture.ui,
            params,
            &[(2, 0.0, 0.0), (3, f(ITEM), 0.0), (4, 0.0, f(ITEM)), (5, f(ITEM), f(ITEM))],
            0.0,
        );
        fixture.set_list(vec![4, 5, 2, 3, 0, 1]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            let coming = -f(ITEM) + f(ITEM * 2) * fraction;
            let leaving = f(ITEM) - f(ITEM * 2) * fraction;
            assert_in_view(
                ui,
                params,
                fraction,
                &[
                    (0, 0.0, coming),
                    (1, f(ITEM), coming),
                    (2, 0.0, 0.0),
                    (3, f(ITEM), 0.0),
                    (4, 0.0, leaving),
                    (5, f(ITEM), leaving),
                ],
                |at| at > -f(ITEM),
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, moveTwoItemsToTheBottomOutsideOfBounds_itemsOfDifferentLanes
#[test]
fn items_of_two_lanes_moved_past_the_end_stay_in_their_lanes() {
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, noAnimationWhenScrollForwardBySmallOffset
#[test]
fn a_small_scroll_of_a_staggered_grid_animates_nothing() {
    for params in PARAMS {
        let mut fixture =
            plain(params, GridOptions::new(1, ITEM * 3), vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let state = fixture.state;
        scroll_grid(&mut fixture.ui, state, ITEM / 2);
        let half = f(ITEM / 2);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0, -half), (1, 0.0, half), (2, 0.0, half * 3.0), (3, 0.0, half * 5.0)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, noAnimationWhenScrollForwardByLargeOffset_multipleCells
#[test]
fn a_large_scroll_of_a_staggered_grid_animates_nothing() {
    for params in PARAMS {
        let list: Vec<i32> = (0..18).collect();
        let mut fixture = plain(params, GridOptions::new(3, ITEM * 3), list);
        let state = fixture.state;
        scroll_grid(&mut fixture.ui, state, ITEM * 5 / 2);
        let half = f(ITEM / 2);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(6, 0.0, -half), (7, f(ITEM), -half), (9, 0.0, half), (12, 0.0, half * 3.0)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, noAnimationWhenScrolledToOtherPosition
#[test]
fn a_jump_of_a_staggered_grid_animates_nothing() {
    for params in PARAMS {
        let mut fixture =
            plain(params, GridOptions::new(1, ITEM * 3), vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let state = fixture.state;
        fixture.ui.run_on_ui_thread(move |_| state.request_scroll_to_item(0, ITEM / 2));
        let half = f(ITEM / 2);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            assert_positions_of(
                ui,
                params,
                &[(0, 0.0, -half), (1, 0.0, half), (2, 0.0, half * 3.0), (3, 0.0, half * 5.0)],
                fraction,
            );
        });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, scrollIsAffectingItemsMovingWithinViewport
#[test]
fn a_scroll_moves_the_staggered_items_moving_in_view() {
    for params in PARAMS {
        let mut fixture = plain(params, GridOptions::new(1, ITEM * 2), vec![0, 1, 2, 3]);
        let state = fixture.state;
        fixture.set_list(vec![0, 2, 1, 3]);
        on_animation_frame(&mut fixture.ui, |ui, fraction| {
            if fraction == 0.0 {
                scroll_grid(ui, state, SPACING);
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, columnCountChange
#[test]
fn a_change_of_lanes_leaves_the_first_where_it_is() {
    let params = PARAMS[0];
    let cells = mutable_state_of(2_usize);
    let handle = cells.handle();
    let state = LazyStaggeredGridState::new(0, 0);
    let state_handle = *state;
    let mut ui = paused(IntSize::new(1000, 1000), move || {
        let _ = &state;
        let count = handle.get();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(count),
            modifier = Modifier.size((dp(ITEM * 2), dp(ITEM))).test_tag(GRID_TAG),
            state = state_handle,
            move |scope: LazyStaggeredGridScope| {
                scope.items_keyed(
                    10,
                    |it: usize| it,
                    move |it| {
                        item(params, it as i32, ITEM, Some(anim_spec()));
                    },
                );
            }
        );
    });
    ui.run_on_ui_thread(|_| cells.set(1));
    on_animation_frame(&mut ui, |ui, fraction| {
        // Item 1 is out of the one lane: its node is gone here, where Compose's keeps its last
        // position unplaced (D1472).
        assert_positions_of(ui, params, &[(0, 0.0, 0.0)], fraction);
        ui.on_node_with_tag("1").assert_does_not_exist();
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemPlacementAnimationTest.kt, animationWhenTryingToStayInTheStart
#[test]
fn staying_at_the_start_of_a_staggered_grid_over_a_new_first_item_animates() {
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

/// Colored items for the fades, two lanes by default.
struct FadeFixture {
    ui: ComposeTestRule,
    list: OwnedState<Vec<Color>>,
}

fn fade_grid(
    cells: usize,
    colors: Vec<Color>,
    fade_in: bool,
    fade_out: bool,
    wrap: bool,
) -> FadeFixture {
    let list = mutable_state_of(colors);
    let handle = list.handle();
    let ui = paused(IntSize::new(1000, 1000), move || {
        let colors = handle.get();
        let width = dp(ITEM * cells as i32);
        let modifier = if wrap {
            Modifier.width(width).wrap_content_height().test_tag(GRID_TAG)
        } else {
            Modifier.size((width, dp(ITEM * 2))).test_tag(GRID_TAG)
        };
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(cells),
            modifier = modifier,
            move |scope: LazyStaggeredGridScope| {
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
            }
        );
    });
    FadeFixture { ui, list }
}

impl FadeFixture {
    fn set(&mut self, colors: Vec<Color>) {
        let handle = self.list.handle();
        self.ui.run_on_ui_thread(move |_| handle.set(colors));
    }
}

fn assert_cells(
    ui: &mut ComposeTestRule,
    cells: i32,
    fraction: f32,
    expected: &[Option<(Color, f32)>],
) {
    for (cell, wanted) in expected.iter().enumerate() {
        let x = (cell as i32 % cells * ITEM + ITEM / 2) as f32;
        let y = (cell as i32 / cells * ITEM + ITEM / 2) as f32;
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

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemAppearanceAnimationTest.kt, oneItemAdded
#[test]
fn a_staggered_item_added_fades_in() {
    let mut fixture = fade_grid(1, Vec::new(), true, false, false);
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_cells(ui, 1, fraction, &[Some((Color::BLACK, fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemAppearanceAnimationTest.kt, noAnimationForInitialList
#[test]
fn a_staggered_grids_first_items_show_at_once() {
    let mut fixture = fade_grid(1, vec![Color::BLACK], true, false, false);
    assert_cells(&mut fixture.ui, 1, 0.0, &[Some((Color::BLACK, 1.0))]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemAppearanceAnimationTest.kt, oneExistTwoAdded
#[test]
fn only_the_staggered_items_added_fade_in() {
    let mut fixture = fade_grid(1, vec![Color::BLACK], true, false, false);
    fixture.set(vec![Color::BLACK, Color::RED]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_cells(ui, 1, fraction, &[Some((Color::BLACK, 1.0)), Some((Color::RED, fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemDisappearanceAnimationTest.kt, oneItemRemoved
#[test]
fn a_staggered_item_removed_fades_out_after_it_left_the_composition() {
    let mut fixture = fade_grid(1, vec![Color::BLACK], false, true, false);
    fixture.set(Vec::new());
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        assert_cells(ui, 1, fraction, &[Some((Color::BLACK, 1.0 - fraction))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemDisappearanceAnimationTest.kt, wholeLineRemoved
#[test]
fn a_whole_line_of_lanes_removed_fades_out() {
    let mut fixture = fade_grid(2, vec![Color::BLACK, Color::GREEN], false, true, false);
    fixture.set(Vec::new());
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let alpha = 1.0 - fraction;
        assert_cells(ui, 2, fraction, &[Some((Color::BLACK, alpha)), Some((Color::GREEN, alpha))]);
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridItemDisappearanceAnimationTest.kt, itemsBeingRemovedAreAffectingTheContainerSizeForTheDurationOfAnimation
#[test]
fn a_staggered_item_fading_out_keeps_the_room_it_took() {
    let mut fixture = fade_grid(1, vec![Color::BLACK, Color::RED], false, true, true);
    assert_eq!(size_of(&mut fixture.ui, GRID_TAG).1, ITEM * 2);
    fixture.set(vec![Color::BLACK]);
    on_animation_frame(&mut fixture.ui, |ui, fraction| {
        let wanted = if fraction < 1.0 { ITEM * 2 } else { ITEM };
        assert_eq!(size_of(ui, GRID_TAG).1, wanted, "fraction {fraction}");
    });
}

/// A lane's items don't make the others of its line move.
#[test]
fn a_shorter_item_doesnt_stop_another_lane_from_moving() {
    for params in PARAMS {
        let content = move |scope: &LazyStaggeredGridScope, list: Vec<i32>| {
            scope.items_list_keyed(
                list,
                |it: &i32| *it,
                move |it: &i32| {
                    let size = if *it == 0 { ITEM2 } else { ITEM };
                    item(params, *it, size, Some(anim_spec()));
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
