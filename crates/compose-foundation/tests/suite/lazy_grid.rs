//! Grids at large, as Compose's `LazyGridTest`, in both orientations. Density 1, so its dps are
//! pixels. Left: the lookahead tests (no lookahead), `customOverscroll` (feature:overscroll),
//! `onlyOneInitialMeasurePass` (composing for
//! a measurement takes rounds here, D125), `passingNegativeItemsCountIsNotAllowed` (a count is a
//! `usize`) and `recomposingWithNewComposedModifierObjectIsNotCausingRemeasure`: a grid recomposed
//! at its call site has new items (a closure can't be compared as Compose compares its content
//! lambda), so it measures again.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{
    Constraints, Dp, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Offset, UnitExt,
};

use crate::lazy_support::{Lazy, bounds, no_padding};
use crate::scroll_support::run;

const GRID: &str = "LazyGridTag";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

/// Compose's tests' `LazyGrid` with what these need of its arguments.
#[derive(Clone)]
struct Args {
    lazy: Lazy,
    cells: GridCells,
    modifier: Modifier,
    state: Option<LazyGridState>,
    padding: PaddingValues,
    main_spaced: f32,
    cross_spaced: f32,
    enabled: bool,
}

impl Args {
    fn new(lazy: Lazy, cells: GridCells, modifier: Modifier) -> Args {
        Args {
            lazy,
            cells,
            modifier,
            state: None,
            padding: no_padding(),
            main_spaced: 0.0,
            cross_spaced: 0.0,
            enabled: true,
        }
    }
}

fn Grid(args: Args, content: impl Fn(LazyGridScope) + 'static) {
    let content = Rc::new(content);
    let remembered = remember_lazy_grid_state(0, 0);
    let state = args.state.unwrap_or(remembered);
    let (vertical_spacing, horizontal_spacing) = if args.lazy.vertical {
        (args.main_spaced, args.cross_spaced)
    } else {
        (args.cross_spaced, args.main_spaced)
    };
    if args.lazy.vertical {
        LazyVerticalGrid!(
            args.cells.clone(),
            modifier = args.modifier.clone(),
            state = state,
            content_padding = args.padding,
            vertical_arrangement = Arrangement::spaced_by(Dp(vertical_spacing)),
            horizontal_arrangement = Arrangement::spaced_by(Dp(horizontal_spacing)),
            user_scroll_enabled = args.enabled,
            move |scope: LazyGridScope| content(scope)
        );
    } else {
        LazyHorizontalGrid!(
            args.cells.clone(),
            modifier = args.modifier.clone(),
            state = state,
            content_padding = args.padding,
            vertical_arrangement = Arrangement::spaced_by(Dp(vertical_spacing)),
            horizontal_arrangement = Arrangement::spaced_by(Dp(horizontal_spacing)),
            user_scroll_enabled = args.enabled,
            move |scope: LazyGridScope| content(scope)
        );
    }
}

fn fixed(cells: usize) -> GridCells {
    GridCells::Fixed(cells)
}

/// `axisSize(crossAxis, mainAxis)`.
fn axis_size(lazy: Lazy, cross: f32, main: f32) -> Modifier {
    lazy.sized(Dp(main), Dp(cross))
}

fn displayed(ui: &mut ComposeTestRule, tag: &str) -> bool {
    ui.on_node_with_tag(tag).is_displayed()
}

/// `count` items tagged from 1, each `main` long along the axis.
fn tagged(scope: LazyGridScope, lazy: Lazy, count: usize, main: f32) {
    scope.items(count, move |index| {
        Spacer(lazy.main_axis_size(Dp(main)).test_tag((index + 1).to_string()));
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, lazyGridShowsOneItem
#[test]
fn a_grid_shows_one_item() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            Grid(Args::new(lazy, fixed(3), Modifier), |scope| {
                scope.item(|| Spacer(Modifier.size(10.dp()).test_tag("item")));
            });
        });
        assert!(displayed(&mut ui, "item"));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, lazyGridShowsOneLine
#[test]
fn a_grid_shows_one_line() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            Grid(Args::new(lazy, fixed(3), axis_size(lazy, 300.0, 100.0)), move |scope| {
                tagged(scope, lazy, 5, 101.0);
            });
        });
        assert!(displayed(&mut ui, "1") && displayed(&mut ui, "2") && displayed(&mut ui, "3"));
        ui.on_node_with_tag("4").assert_does_not_exist();
        ui.on_node_with_tag("5").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, lazyGridShowsSecondLineOnScroll
#[test]
fn a_scroll_shows_a_grids_second_line() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let modifier = lazy.main_axis_size(100.dp()).test_tag(GRID);
            Grid(Args::new(lazy, fixed(3), modifier), move |scope| tagged(scope, lazy, 9, 101.0));
        });
        lazy.scroll_main_axis_by(&mut ui, GRID, 50.0);
        for tag in ["4", "5", "6"] {
            assert!(displayed(&mut ui, tag), "{tag}");
        }
        for tag in ["7", "8", "9"] {
            assert!(!displayed(&mut ui, tag), "{tag}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, lazyGridScrollHidesFirstLine
#[test]
fn a_scroll_hides_a_grids_first_line() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let modifier = lazy.main_axis_size(200.dp()).test_tag(GRID);
            Grid(Args::new(lazy, fixed(3), modifier), move |scope| tagged(scope, lazy, 9, 101.0));
        });
        lazy.scroll_main_axis_by(&mut ui, GRID, 103.0);
        for tag in ["1", "2", "3"] {
            assert!(!displayed(&mut ui, tag), "{tag}");
        }
        for tag in ["4", "5", "6", "7", "8", "9"] {
            assert!(displayed(&mut ui, tag), "{tag}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, adaptiveLazyGridFillsAllCrossAxisSize
#[test]
fn adaptive_cells_fill_the_room_across() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let cells = GridCells::Adaptive(130.dp());
            Grid(Args::new(lazy, cells, axis_size(lazy, 300.0, 100.0)), move |scope| {
                tagged(scope, lazy, 5, 101.0);
            });
        });
        assert_eq!(lazy.cross_start(&mut ui, "1"), 0.0);
        assert_eq!(lazy.cross_start(&mut ui, "2"), 150.0);
        ui.on_node_with_tag("3").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, adaptiveLazyGridAtLeastOneSlot
#[test]
fn adaptive_cells_are_at_least_one() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let cells = GridCells::Adaptive(301.dp());
            Grid(Args::new(lazy, cells, axis_size(lazy, 300.0, 100.0)), move |scope| {
                tagged(scope, lazy, 3, 101.0);
            });
        });
        assert!(displayed(&mut ui, "1"));
        ui.on_node_with_tag("2").assert_does_not_exist();
    }
}

/// `count` items of `size` tagged from 1, in `cells`, spaced and padded as given, each expected
/// along or across the axis at `expected` with that size.
#[allow(clippy::too_many_arguments)]
fn assert_spaced(
    lazy: Lazy,
    cells: GridCells,
    modifier: Modifier,
    main_spaced: f32,
    cross_spaced: f32,
    padding: PaddingValues,
    count: usize,
    size: f32,
    along: bool,
    expected: &[f32],
) {
    let mut ui = rule(move || {
        let args = Args {
            padding,
            main_spaced,
            cross_spaced,
            ..Args::new(lazy, cells.clone(), modifier.clone())
        };
        Grid(args, move |scope| {
            scope.items(count, move |index| {
                Spacer(Modifier.size(Dp(size)).test_tag((index + 1).to_string()));
            });
        });
    });
    for (index, position) in expected.iter().enumerate() {
        let tag = (index + 1).to_string();
        assert!(displayed(&mut ui, &tag), "{tag}");
        let (start, length) = if along {
            (lazy.main_start(&mut ui, &tag), lazy.main_size(&mut ui, &tag))
        } else {
            (lazy.cross_start(&mut ui, &tag), lazy.cross_size(&mut ui, &tag))
        };
        assert_eq!((start, length), (*position, size), "{lazy:?} {tag}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, adaptiveLazyGridAppliesHorizontalSpacings
#[test]
fn adaptive_cells_are_spaced_across() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (10.0, 100.0);
        assert_spaced(
            lazy,
            GridCells::Adaptive(Dp(item)),
            axis_size(lazy, item * 3.0 + spacing * 2.0, item),
            0.0,
            spacing,
            no_padding(),
            3,
            item,
            false,
            &[0.0, item + spacing, item * 2.0 + spacing * 2.0],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, adaptiveLazyGridAppliesHorizontalSpacingsWithContentPaddings
#[test]
fn adaptive_cells_are_spaced_across_inside_the_padding() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (8.0, 40.0);
        assert_spaced(
            lazy,
            GridCells::Adaptive(Dp(item)),
            axis_size(lazy, item * 3.0 + spacing * 4.0, item),
            0.0,
            spacing,
            lazy.padding(0.dp(), 0.dp(), Dp(spacing), Dp(spacing)),
            3,
            item,
            false,
            &[spacing, item + spacing * 2.0, item * 2.0 + spacing * 3.0],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, adaptiveLazyGridAppliesVerticalSpacings
#[test]
fn adaptive_cells_are_spaced_along() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (4.0, 32.0);
        assert_spaced(
            lazy,
            GridCells::Adaptive(Dp(item)),
            axis_size(lazy, item, item * 3.0 + spacing * 2.0),
            spacing,
            0.0,
            no_padding(),
            3,
            item,
            true,
            &[0.0, item + spacing, item * 2.0 + spacing * 2.0],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, adaptiveLazyGridAppliesVerticalSpacingsWithContentPadding
#[test]
fn adaptive_cells_are_spaced_along_inside_the_padding() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (16.0, 72.0);
        assert_spaced(
            lazy,
            GridCells::Adaptive(Dp(item)),
            axis_size(lazy, item, item * 3.0 + spacing * 2.0),
            spacing,
            0.0,
            lazy.padding(Dp(spacing), Dp(spacing), 0.dp(), 0.dp()),
            3,
            item,
            true,
            &[spacing, spacing * 2.0 + item, spacing * 3.0 + item * 2.0],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, fixedLazyGridAppliesVerticalSpacings
#[test]
fn fixed_cells_are_spaced_along() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (24.0, 80.0);
        assert_spaced(
            lazy,
            fixed(2),
            axis_size(lazy, item, item * 2.0 + spacing),
            spacing,
            0.0,
            no_padding(),
            4,
            item,
            true,
            &[0.0, 0.0, spacing + item, spacing + item],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, fixedLazyGridAppliesHorizontalSpacings
#[test]
fn fixed_cells_are_spaced_across() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (15.0, 30.0);
        assert_spaced(
            lazy,
            fixed(2),
            axis_size(lazy, item * 2.0 + spacing, item * 2.0),
            0.0,
            spacing,
            no_padding(),
            4,
            item,
            false,
            &[0.0, spacing + item, 0.0, spacing + item],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, fixedLazyGridAppliesVerticalSpacingsWithContentPadding
#[test]
fn fixed_cells_are_spaced_along_inside_the_padding() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (30.0, 77.0);
        assert_spaced(
            lazy,
            fixed(2),
            axis_size(lazy, item, item * 2.0 + spacing),
            spacing,
            0.0,
            lazy.padding(Dp(spacing), Dp(spacing), 0.dp(), 0.dp()),
            4,
            item,
            true,
            &[spacing, spacing, spacing * 2.0 + item, spacing * 2.0 + item],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, fixedLazyGridAppliesHorizontalSpacingsWithContentPadding
#[test]
fn fixed_cells_are_spaced_across_inside_the_padding() {
    for lazy in Lazy::grids() {
        let (spacing, item) = (22.0, 44.0);
        assert_spaced(
            lazy,
            fixed(2),
            axis_size(lazy, item * 2.0 + spacing * 3.0, item * 2.0),
            0.0,
            spacing,
            lazy.padding(0.dp(), 0.dp(), Dp(spacing), Dp(spacing)),
            4,
            item,
            false,
            &[spacing, spacing * 2.0 + item, spacing, spacing * 2.0 + item],
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, usedWithArray, usedWithArrayIndexed
#[test]
fn a_grids_items_can_be_an_array() {
    for lazy in Lazy::grids() {
        let item = 15.0;
        let mut ui = rule(move || {
            let modifier = lazy.cross_axis_size(Dp(item * 2.0));
            Grid(Args::new(lazy, fixed(2), modifier), move |scope| {
                scope.items_list(vec!["1", "2", "3", "4"], move |text: &&str| {
                    Spacer(lazy.main_axis_size(Dp(item)).test_tag(text.to_string()));
                });
            });
        });
        let cells = [("1", 0.0, 0.0), ("2", 0.0, item), ("3", item, 0.0), ("4", item, item)];
        for (tag, main, cross) in cells {
            assert_eq!(lazy.main_start(&mut ui, tag), main, "{tag}");
            assert_eq!(lazy.cross_start(&mut ui, tag), cross, "{tag}");
        }
        let mut ui = rule(move || {
            let modifier = lazy.cross_axis_size(Dp(item * 2.0));
            Grid(Args::new(lazy, fixed(2), modifier), move |scope| {
                scope.items_indexed(vec!["1", "2", "3", "4"], move |index, text: &&str| {
                    Spacer(lazy.main_axis_size(Dp(item)).test_tag(format!("{index}*{text}")));
                });
            });
        });
        let cells =
            [("0*1", 0.0, 0.0), ("1*2", 0.0, item), ("2*3", item, 0.0), ("3*4", item, item)];
        for (tag, main, cross) in cells {
            assert_eq!(lazy.main_start(&mut ui, tag), main, "{tag}");
            assert_eq!(lazy.cross_start(&mut ui, tag), cross, "{tag}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, changeItemsCountAndScrollImmediately
#[test]
fn a_scroll_right_after_the_count_shrank_sees_it() {
    for lazy in Lazy::grids() {
        let count = mutable_state_of(100usize);
        let read_count = count.handle();
        let composed = Rc::new(RefCell::new(Vec::new()));
        let noted = composed.clone();
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            let noted = noted.clone();
            let args = Args {
                state: Some(handle),
                ..Args::new(lazy, fixed(1), lazy.main_axis_size(10.dp()))
            };
            let count = read_count.get();
            Grid(args, move |scope| {
                let noted = noted.clone();
                scope.items(count, move |index| {
                    noted.borrow_mut().push(index);
                    Box!(modifier = Modifier.size(20.dp()), {});
                });
            });
        });
        composed.borrow_mut().clear();
        count.set(10);
        let _ = run(&mut ui, async move { handle.scroll_to_item(50, 0).await });
        assert!(composed.borrow().iter().all(|item| *item < 10), "{:?}", composed.borrow());
        assert_eq!(state.first_visible_item_index(), 9);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, maxIntElements
#[test]
fn a_grid_of_as_many_items_as_an_int_holds() {
    for lazy in Lazy::grids() {
        let item = 15.0;
        let max = i32::MAX as usize;
        let state = LazyGridState::new(max - 3, 0);
        let handle = *state;
        let mut ui = rule(move || {
            let args = Args {
                state: Some(handle),
                ..Args::new(lazy, fixed(2), Modifier.size(Dp(item * 2.0)))
            };
            Grid(args, move |scope| {
                scope.items(max, move |index| {
                    Box!(modifier = Modifier.size(Dp(item)).test_tag(index.to_string()), {});
                });
            });
        });
        let cells = [(max - 3, 0.0, 0.0), (max - 2, 0.0, item), (max - 1, item, 0.0)];
        for (index, main, cross) in cells {
            let tag = index.to_string();
            assert_eq!(lazy.main_start(&mut ui, &tag), main, "{tag}");
            assert_eq!(lazy.cross_start(&mut ui, &tag), cross, "{tag}");
        }
        ui.on_node_with_tag(&max.to_string()).assert_does_not_exist();
        ui.on_node_with_tag("0").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, maxIntElements_withKey_startInMiddle
#[test]
fn a_keyed_grid_of_as_many_items_starts_in_the_middle() {
    for lazy in Lazy::grids() {
        let item = 15.0;
        let max = i32::MAX as usize;
        let state = LazyGridState::new(max / 2, 0);
        let handle = *state;
        let mut ui = rule(move || {
            let args =
                Args { state: Some(handle), ..Args::new(lazy, fixed(1), Modifier.size(Dp(item))) };
            Grid(args, move |scope| {
                scope.items_keyed(
                    max,
                    |index| index,
                    move |index| {
                        Box!(modifier = Modifier.size(Dp(item)).test_tag(index.to_string()), {});
                    },
                );
            });
        });
        assert_eq!(lazy.main_start(&mut ui, &(max / 2).to_string()), 0.0);
    }
}

fn five_of_30(lazy: Lazy, enabled: bool, state: Option<LazyGridState>) -> ComposeTestRule {
    rule(move || {
        let args = Args {
            enabled,
            state,
            ..Args::new(lazy, fixed(1), Modifier.size(90.dp()).test_tag(GRID))
        };
        Grid(args, |scope| {
            scope.items(5, |index| Spacer(Modifier.size(30.dp()).test_tag(index.to_string())));
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, pointerInputScrollingIsAllowedWhenUserScrollingIsEnabled, pointerInputScrollingIsDisallowedWhenUserScrollingIsDisabled
#[test]
fn drags_scroll_a_grid_only_with_user_scrolling_on() {
    for lazy in Lazy::grids() {
        for (enabled, expected) in [(true, 0.0), (false, 30.0)] {
            let mut ui = five_of_30(lazy, enabled, None);
            lazy.drag_main_axis_by(&mut ui, GRID, 30.0);
            assert_eq!(lazy.main_start(&mut ui, "1"), expected, "{lazy:?} {enabled}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, programmaticScrollingIsAllowedWhenUserScrollingIsDisabled
#[test]
fn the_state_scrolls_a_grid_with_user_scrolling_off() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = five_of_30(lazy, false, Some(handle));
        let _ = run(&mut ui, async move { handle.scroll_by(30.0).await });
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, semanticScrollingIsDisallowedWhenUserScrollingIsDisabled
#[test]
fn semantics_dont_scroll_a_grid_with_user_scrolling_off() {
    for lazy in Lazy::grids() {
        let mut ui = five_of_30(lazy, false, None);
        let tree = ui.on_node_with_tag(GRID).fetch_semantics_snapshot();
        assert!(tree.config.get(&compose_ui::SemanticsActions::SCROLL_BY).is_none());
        let range = if lazy.vertical {
            tree.config.get(&compose_ui::SemanticsProperties::VERTICAL_SCROLL_AXIS_RANGE)
        } else {
            tree.config.get(&compose_ui::SemanticsProperties::HORIZONTAL_SCROLL_AXIS_RANGE)
        };
        assert!(range.is_some(), "a read-only range");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, rtl
#[test]
fn a_grid_lays_out_right_to_left() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            CompositionLocalProvider(
                LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
                move || {
                    Grid(Args::new(lazy, fixed(3), lazy.cross_axis_size(30.dp())), move |scope| {
                        scope.items(3, move |index| {
                            Box!(
                                modifier = lazy.main_axis_size(1.dp()).test_tag(index.to_string()),
                                {}
                            );
                        });
                    });
                },
            );
        });
        let tags = if lazy.vertical { ["0", "1", "2"] } else { ["2", "1", "0"] };
        assert_eq!(lazy.cross_start(&mut ui, tags[0]), 20.0, "{lazy:?}");
        assert_eq!(lazy.cross_start(&mut ui, tags[1]), 10.0, "{lazy:?}");
        assert_eq!(lazy.cross_start(&mut ui, tags[2]), 0.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, withMissingItems
#[test]
fn an_empty_item_takes_its_cell() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            let args = Args {
                state: Some(handle),
                ..Args::new(lazy, fixed(2), lazy.main_axis_size(31.dp()))
            };
            Grid(args, move |scope| {
                scope.items(9, move |index| {
                    if index != 3 {
                        Box!(
                            modifier = lazy.main_axis_size(30.dp()).test_tag(index.to_string()),
                            {}
                        );
                    }
                });
            });
        });
        assert!(displayed(&mut ui, "0") && displayed(&mut ui, "1") && displayed(&mut ui, "2"));
        let _ = run(&mut ui, async move { handle.scroll_to_item(3, 0).await });
        assert!(!displayed(&mut ui, "0") && !displayed(&mut ui, "1"));
        assert!(displayed(&mut ui, "2") && displayed(&mut ui, "4") && displayed(&mut ui, "5"));
        ui.on_node_with_tag("6").assert_does_not_exist();
        ui.on_node_with_tag("7").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, withZeroSizedFirstItem_shouldKeepItemOnSizeChange
#[test]
fn a_first_item_growing_from_nothing_shows() {
    for lazy in Lazy::grids() {
        let size = mutable_state_of(0.0f32);
        let read = size.handle();
        let mut ui = rule(move || {
            let args = Args {
                padding: PaddingValues::all(10.dp()),
                ..Args::new(lazy, fixed(1), Modifier.test_tag("mainList").size(300.dp()))
            };
            Grid(args, move |scope| {
                scope.item(move || {
                    Spacer(Modifier.test_tag("firstItem").size(Dp(read.get())));
                });
                scope.items(9, |index| {
                    Box!(modifier = Modifier.test_tag(index.to_string()), {
                        BasicText!("{}", index);
                    });
                });
            });
        });
        assert!(!displayed(&mut ui, "firstItem"));
        size.set(20.0);
        ui.wait_for_idle();
        assert!(displayed(&mut ui, "firstItem"));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, customGridCells
#[test]
fn a_grids_cells_can_be_your_own() {
    struct OneToTwo;
    impl CrossAxisCells for OneToTwo {
        fn calculate_cross_axis_cell_sizes(
            &self,
            _: compose_ui::Density,
            available: i32,
            spacing: i32,
        ) -> Vec<i32> {
            let column = (available - spacing) / 3;
            vec![column, column * 2]
        }
    }
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let cells = GridCells::Custom(Rc::new(OneToTwo));
            Grid(Args::new(lazy, cells, axis_size(lazy, 300.0, 100.0)), move |scope| {
                tagged(scope, lazy, 5, 101.0);
            });
        });
        assert_eq!((lazy.cross_start(&mut ui, "1"), lazy.cross_size(&mut ui, "1")), (0.0, 100.0));
        assert_eq!((lazy.cross_start(&mut ui, "2"), lazy.cross_size(&mut ui, "2")), (100.0, 200.0));
        ui.on_node_with_tag("3").assert_does_not_exist();
        ui.on_node_with_tag("4").assert_does_not_exist();
        ui.on_node_with_tag("5").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, laysOutRtlCorrectlyWithLargerContainer
#[test]
fn a_grid_lays_out_right_to_left_in_a_larger_container() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            CompositionLocalProvider(
                LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
                move || {
                    Grid(Args::new(lazy, fixed(2), axis_size(lazy, 110.0, 250.0)), move |scope| {
                        scope.items(4, move |index| {
                            let label = (index + 1).to_string();
                            Box!(modifier = Modifier.size(50.dp()).test_tag(label), {});
                        });
                    });
                },
            );
        });
        if lazy.vertical {
            // 2 1
            // 4 3
            assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
            assert_eq!(lazy.cross_start(&mut ui, "1"), 55.0);
        } else {
            // 3 1
            // 4 2
            assert_eq!(lazy.cross_start(&mut ui, "1"), 0.0);
            assert_eq!(lazy.main_start(&mut ui, "1"), 200.0);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, scrollDuringMeasure, scrollInLaunchedEffect
#[test]
fn a_grid_scrolled_from_an_effect_is_there() {
    for lazy in Lazy::grids() {
        for in_constraints in [false, true] {
            let mut ui = rule(move || {
                let body = move || {
                    let state = remember_lazy_grid_state(0, 0);
                    let args = Args {
                        state: Some(state),
                        ..Args::new(lazy, fixed(2), axis_size(lazy, 40.0, 100.0))
                    };
                    Grid(args, move |scope| {
                        scope.items(20, move |index| {
                            Box!(
                                modifier = axis_size(lazy, 20.0, 20.0).test_tag(index.to_string()),
                                {}
                            );
                        });
                    });
                    LaunchedEffect(state, async move {
                        let _ = state.scroll_to_item(10, 0).await;
                    });
                };
                if in_constraints {
                    BoxWithConstraints!(move |_| body());
                } else {
                    body();
                }
            });
            ui.wait_for_idle();
            assert_eq!(lazy.main_start(&mut ui, "10"), 0.0, "{lazy:?} {in_constraints}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, changedLinesRemeasuredCorrectly
#[test]
fn lines_changing_are_measured_again() {
    for lazy in Lazy::grids() {
        let flag = mutable_state_of(false);
        let read = flag.handle();
        let mut ui = rule(move || {
            let split = read.get();
            Grid(Args::new(lazy, fixed(2), axis_size(lazy, 60.0, 100.0)), move |scope| {
                let whole = |scope: LazyGridItemSpanScope| GridItemSpan(scope.max_line_span());
                let main = lazy.main_axis_size(32.dp());
                let (first, second, target) = (main.clone(), main.clone(), main.clone());
                scope.item_with_span(whole, "first", move || {
                    Box!(modifier = first.clone(), {});
                });
                if split {
                    let (second, target) = (second.clone(), target.clone());
                    scope.item(move || Box!(modifier = second.clone(), {}));
                    scope.item(move || Box!(modifier = target.clone().test_tag("target"), {}));
                } else {
                    let (second, target) = (second.clone(), target.clone());
                    scope.item_with_span(whole, "second", move || {
                        Box!(modifier = second.clone(), {});
                    });
                    scope.item_with_span(whole, "third", move || {
                        Box!(modifier = target.clone().test_tag("target"), {});
                    });
                }
            });
        });
        ui.wait_for_idle();
        flag.set(true);
        ui.wait_for_idle();
        assert_eq!(lazy.cross_size(&mut ui, "target"), 30.0);
        assert_eq!(lazy.main_start(&mut ui, "target"), 32.0);
        assert_eq!(lazy.cross_start(&mut ui, "target"), 30.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, spacingsLargerThanLayoutSize
#[test]
fn a_spacing_larger_than_the_room_leaves_empty_cells() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let args =
                Args { cross_spaced: 5.0, ..Args::new(lazy, fixed(2), axis_size(lazy, 0.0, 50.0)) };
            Grid(args, |scope| {
                scope.items(2, |index| {
                    Spacer(Modifier.size(50.dp()).test_tag((index + 1).to_string()));
                });
            });
        });
        for tag in ["1", "2"] {
            ui.on_node_with_tag(tag).assert_exists();
            assert_eq!(lazy.cross_size(&mut ui, tag), 0.0, "{lazy:?} {tag}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, itemsComposedInOrderDuringAnimatedScroll
#[test]
fn an_animated_scroll_composes_items_in_order() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let previous = Rc::new(Cell::new(-1i64));
        let seen = previous.clone();
        let mut ui = rule(move || {
            let seen = seen.clone();
            let args =
                Args { state: Some(handle), ..Args::new(lazy, fixed(1), Modifier.size(30.dp())) };
            Grid(args, move |scope| {
                let seen = seen.clone();
                scope.items(500, move |index| {
                    assert!(index as i64 > seen.get(), "{index} after {}", seen.get());
                    seen.set(index as i64);
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            });
        });
        let _ = run(&mut ui, async move { handle.animate_scroll_to_item(500, 0).await });
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, fillingFullSize_nextItemIsNotComposed
#[test]
fn a_grid_filled_doesnt_compose_the_next_item() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            let args = Args {
                state: Some(handle),
                ..Args::new(lazy, fixed(1), lazy.main_axis_size(5.dp()).test_tag(GRID))
            };
            Grid(args, |scope| {
                scope.items(3, |index| {
                    Box!(modifier = Modifier.size(5.dp()).test_tag(index.to_string()), {});
                });
            });
        });
        for index in 0..3 {
            assert!(displayed(&mut ui, &index.to_string()));
            ui.on_node_with_tag(&(index + 1).to_string()).assert_does_not_exist();
            let _ = run(&mut ui, async move { handle.scroll_by(5.0).await });
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, changingMaxSpansCount
#[test]
fn a_grids_cells_can_change_with_spans_at_the_line_start() {
    for lazy in Lazy::grids() {
        let expanded = mutable_state_of(true);
        let read = expanded.handle();
        let mut ui = rule(move || {
            let sized = Modifier.layout(move |scope, measurable, _: Constraints| {
                let cross = if read.get() { 300 } else { 100 };
                let main = 301;
                let (width, height) = if lazy.vertical { (cross, main) } else { (main, cross) };
                let placeable = scope.measure(measurable, Constraints::fixed(width, height));
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            });
            Row!({
                let args =
                    Args::new(lazy, GridCells::Adaptive(100.dp()), sized.clone().test_tag(GRID));
                Grid(args, |scope| {
                    scope.items_with_span(
                        100,
                        |span, index| {
                            if index == 0 || index == 5 {
                                GridItemSpan(span.max_line_span())
                            } else {
                                GridItemSpan(1)
                            }
                        },
                        |_| 0,
                        |index| {
                            Box!(modifier = Modifier.size(100.dp()).test_tag(index.to_string()), {
                            });
                        },
                    );
                });
            });
        });
        ui.wait_for_idle();
        expanded.set(false);
        ui.wait_for_idle();
        expanded.set(true);
        ui.wait_for_idle();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, fixedSizeCell_forcesFixedSize
#[test]
fn fixed_size_cells_are_that_size() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let cells = GridCells::FixedSize(200.dp());
            Grid(Args::new(lazy, cells, axis_size(lazy, 500.0, 500.0)), |scope| {
                scope.items(10, |index| {
                    Box!(modifier = Modifier.size(100.dp()).test_tag(index.to_string()), {});
                });
            });
        });
        assert_eq!((lazy.cross_start(&mut ui, "0"), lazy.cross_size(&mut ui, "0")), (0.0, 200.0));
        assert_eq!((lazy.cross_start(&mut ui, "1"), lazy.cross_size(&mut ui, "1")), (200.0, 200.0));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, fixedSizeCell_smallContainer_matchesContainer
#[test]
fn a_fixed_size_cell_larger_than_the_room_is_the_room() {
    for lazy in Lazy::grids() {
        let mut ui = rule(move || {
            let cells = GridCells::FixedSize(200.dp());
            Grid(Args::new(lazy, cells, axis_size(lazy, 100.0, 500.0)), |scope| {
                scope.items(10, |index| {
                    Box!(modifier = Modifier.size(100.dp()).test_tag(index.to_string()), {});
                });
            });
        });
        for tag in ["0", "1"] {
            assert_eq!(
                (lazy.cross_start(&mut ui, tag), lazy.cross_size(&mut ui, tag)),
                (0.0, 100.0)
            );
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, withZeroSizedFirstItem
#[test]
fn a_swipe_back_past_a_zero_sized_first_item_consumes_nothing() {
    for lazy in Lazy::grids() {
        let consumed = Rc::new(Cell::new(Offset::ZERO));
        let noted = consumed.clone();
        let mut ui = rule(move || {
            let connection = Rc::new(ConsumedConnection(noted.clone()));
            let modifier = Modifier.size(300.dp()).test_tag("mainList").nested_scroll(connection);
            let args = Args {
                padding: PaddingValues::all(10.dp()),
                ..Args::new(lazy, fixed(1), modifier)
            };
            Grid(args, |scope| {
                scope.item(|| Spacer(Modifier.size(0.dp())));
                scope.items(9, |index| {
                    Box!(modifier = Modifier.test_tag(index.to_string()), {
                        BasicText!("{}", index);
                    });
                });
            });
        });
        // Back: towards the start, at the start.
        lazy.drag_main_axis_by(&mut ui, "mainList", -100.0);
        assert_eq!(consumed.get(), Offset::ZERO, "{lazy:?}");
    }
}

struct ConsumedConnection(Rc<Cell<Offset>>);

impl compose_ui::NestedScrollConnection for ConsumedConnection {
    fn on_post_scroll(
        &self,
        consumed: Offset,
        _available: Offset,
        _source: compose_ui::NestedScrollSource,
    ) -> Offset {
        self.0.set(self.0.get() + consumed);
        Offset::ZERO
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, scrollingALotDoesntCauseLazyLayoutRecomposition
#[test]
fn a_jump_doesnt_recompose_around_the_grid() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let compositions = Rc::new(Cell::new(0));
        let noted = compositions.clone();
        let mut ui = rule(move || {
            noted.set(noted.get() + 1);
            let args =
                Args { state: Some(handle), ..Args::new(lazy, fixed(1), Modifier.size(100.dp())) };
            Grid(args, |scope| scope.items(1000, |_| Spacer(Modifier.size(100.dp()))));
        });
        ui.wait_for_idle();
        assert_eq!(compositions.get(), 1);
        let _ = run(&mut ui, async move { handle.scroll_to_item(100, 0).await });
        assert_eq!(compositions.get(), 1);
    }
}

/// A scrollable around a grid of 100 px, `count` items of `size`: what it's dragged, in a state of its
/// own, is what the grid leaves.
fn nested_grid(lazy: Lazy, count: usize, size: f32) -> (ComposeTestRule, Rc<Cell<f32>>) {
    let dragged = Rc::new(Cell::new(0.0f32));
    let into = dragged.clone();
    let outer = scrollable_state(move |delta| {
        into.set(into.get() + delta);
        delta
    });
    let orientation = if lazy.vertical {
        compose_foundation::gestures::Orientation::Vertical
    } else {
        compose_foundation::gestures::Orientation::Horizontal
    };
    let ui = rule(move || {
        let outer = outer.clone();
        Box!(modifier = Modifier.scrollable(outer, orientation), {
            let modifier = Modifier.required_size(100.dp()).test_tag(GRID);
            Grid(Args::new(lazy, fixed(1), modifier), move |scope| {
                scope.items(count, move |index| {
                    Spacer(Modifier.required_size(Dp(size)).test_tag(index.to_string()));
                });
            });
        });
    });
    (ui, dragged)
}

/// A touch down at `at` along the axis (10 across), moved by `by` past the slop, and up with no
/// fling, as these tests' `performTouchInput`.
fn nested_drag(ui: &mut ComposeTestRule, lazy: Lazy, at: f32, by: f32) {
    let along = |amount: f32| {
        if lazy.vertical { Offset::new(0.0, amount) } else { Offset::new(amount, 0.0) }
    };
    let mut finger = crate::scroll_support::Touch::down(
        ui,
        along(at) + if lazy.vertical { Offset::new(10.0, 0.0) } else { Offset::new(0.0, 10.0) },
    );
    finger.move_by(ui, along(by + 8.0f32.copysign(by)));
    finger.advance_event_time(ui, 3000);
    finger.up(ui);
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyNestedScrollingTest.kt, verticalGrid_nestedScrollingBackwardInitially
#[test]
fn a_drag_back_at_a_grids_start_goes_to_the_parent() {
    for lazy in Lazy::grids() {
        let (mut ui, dragged) = nested_grid(lazy, 3, 50.0);
        nested_drag(&mut ui, lazy, 10.0, 100.0);
        assert_eq!(dragged.get(), 100.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyNestedScrollingTest.kt, verticalGrid_nestedScrollingBackwardOnceWeScrolledForwardPreviously
#[test]
fn a_drag_back_at_a_grids_start_after_a_scroll_goes_to_the_parent() {
    for lazy in Lazy::grids() {
        let (mut ui, dragged) = nested_grid(lazy, 3, 50.0);
        lazy.drag_main_axis_by(&mut ui, GRID, 20.0);
        lazy.drag_main_axis_by(&mut ui, GRID, -21.0);
        dragged.set(0.0);
        nested_drag(&mut ui, lazy, 90.0, 20.0);
        assert_eq!(dragged.get(), 20.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyNestedScrollingTest.kt, verticalGrid_nestedScrollingForwardWhenTheFullContentIsInitiallyVisible
#[test]
fn a_drag_on_a_grid_with_room_to_spare_goes_to_the_parent() {
    for lazy in Lazy::grids() {
        let (mut ui, dragged) = nested_grid(lazy, 2, 40.0);
        nested_drag(&mut ui, lazy, 10.0, -20.0);
        assert_eq!(dragged.get(), -20.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyNestedScrollingTest.kt, verticalGrid_nestedScrollingForwardWhenScrolledToTheEnd
#[test]
fn a_drag_on_at_a_grids_end_goes_to_the_parent() {
    for lazy in Lazy::grids() {
        let (mut ui, dragged) = nested_grid(lazy, 3, 50.0);
        lazy.drag_main_axis_by(&mut ui, GRID, 55.0);
        dragged.set(0.0);
        nested_drag(&mut ui, lazy, 10.0, -20.0);
        assert_eq!(dragged.get(), -20.0, "{lazy:?}");
    }
}

/// A vertical grid reporting `cross_axis_cell_count` as its item composes.
fn reporting(
    cells: GridCells,
    modifier: Modifier,
) -> (ComposeTestRule, Rc<Cell<usize>>, Rc<Cell<usize>>) {
    let reported = Rc::new(Cell::new(0));
    let composed = Rc::new(Cell::new(0));
    let (noted, counted) = (reported.clone(), composed.clone());
    let ui = rule(move || {
        let (noted, counted) = (noted.clone(), counted.clone());
        LazyVerticalGrid!(
            cells.clone(),
            modifier = modifier.clone(),
            move |scope: LazyGridScope| {
                let (noted, counted) = (noted.clone(), counted.clone());
                let items = scope.item_scope();
                scope.item(move || {
                    counted.set(counted.get() + 1);
                    noted.set(items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            }
        );
    });
    (ui, reported, composed)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_availableInItemScope_fixedGrid
#[test]
fn an_item_knows_the_cells_of_a_fixed_grid() {
    let (_ui, reported, _) = reporting(fixed(3), Modifier.size(100.dp()));
    assert_eq!(reported.get(), 3);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_availableInItemScope_horizontalGrid
#[test]
fn an_item_knows_the_cells_of_a_horizontal_grid() {
    let reported = Rc::new(Cell::new(0));
    let noted = reported.clone();
    let _ui = rule(move || {
        let noted = noted.clone();
        LazyHorizontalGrid!(
            fixed(4),
            modifier = Modifier.size(100.dp()),
            move |scope: LazyGridScope| {
                let (noted, items) = (noted.clone(), scope.item_scope());
                scope.item(move || {
                    noted.set(items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            }
        );
    });
    assert_eq!(reported.get(), 4);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_availableInItemScope_fixedSizeGrid
#[test]
fn an_item_knows_the_cells_of_a_fixed_size_grid() {
    let width = mutable_state_of(200.0f32);
    let read = width.handle();
    let reported = Rc::new(Cell::new(0));
    let noted = reported.clone();
    let mut ui = rule(move || {
        let noted = noted.clone();
        LazyVerticalGrid!(
            GridCells::FixedSize(60.dp()),
            modifier = Modifier.width(Dp(read.get())).height(200.dp()),
            move |scope: LazyGridScope| {
                let (noted, items) = (noted.clone(), scope.item_scope());
                scope.item(move || {
                    noted.set(items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            }
        );
    });
    assert_eq!(reported.get(), 3);
    width.set(40.0);
    ui.wait_for_idle();
    assert_eq!(reported.get(), 1, "falls back to one");
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_recomposesWhenColumnCountChanges: not its first composition's count, the item knowing the cells only once measured here (D1451)
#[test]
fn an_item_recomposes_only_when_the_cells_change() {
    let width = mutable_state_of(220.0f32);
    let read = width.handle();
    let reported = Rc::new(Cell::new(0));
    let composed = Rc::new(Cell::new(0));
    let (noted, counted) = (reported.clone(), composed.clone());
    let mut ui = rule(move || {
        let (noted, counted) = (noted.clone(), counted.clone());
        LazyVerticalGrid!(
            GridCells::Adaptive(100.dp()),
            // Read in the measurement: Compose's content lambda is one value, where a new closure at
            // the call site would compose its items again whatever they read.
            modifier = Modifier.layout(move |scope, measurable, _: Constraints| {
                let constraints = Constraints::fixed(read.get() as i32, 200);
                let placeable = scope.measure(measurable, constraints);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            }),
            move |scope: LazyGridScope| {
                let (noted, counted, items) = (noted.clone(), counted.clone(), scope.item_scope());
                scope.item(move || {
                    counted.set(counted.get() + 1);
                    let cells = items.cross_axis_cell_count();
                    noted.set(cells);
                    Box!(
                        modifier =
                            Modifier.height(Dp(cells as f32 * 10.0)).test_tag("adaptiveItem"),
                        {}
                    );
                });
            }
        );
    });
    assert_eq!(reported.get(), 2);
    assert_eq!(bounds(&mut ui, "adaptiveItem").height(), 20.0);
    let first = composed.get();
    width.set(250.0);
    ui.wait_for_idle();
    assert_eq!((reported.get(), composed.get()), (2, first), "the same cells");
    width.set(420.0);
    ui.wait_for_idle();
    assert_eq!((reported.get(), composed.get()), (4, first + 1));
    assert_eq!(bounds(&mut ui, "adaptiveItem").height(), 40.0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_doesNotRecomposeOnScroll
#[test]
fn a_scroll_doesnt_recompose_an_item_for_the_cells() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let seen: Rc<RefCell<std::collections::HashMap<usize, usize>>> = Rc::default();
    let first_composed = Rc::new(Cell::new(0));
    let (noted, counted) = (seen.clone(), first_composed.clone());
    let mut ui = rule(move || {
        let (noted, counted) = (noted.clone(), counted.clone());
        LazyVerticalGrid!(
            fixed(2),
            state = handle,
            modifier = Modifier.size(100.dp()),
            move |scope: LazyGridScope| {
                let (noted, counted, items) = (noted.clone(), counted.clone(), scope.item_scope());
                scope.items(50, move |index| {
                    if index == 0 {
                        counted.set(counted.get() + 1);
                    }
                    noted.borrow_mut().insert(index, items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(50.dp()), {});
                });
            }
        );
    });
    assert_eq!(seen.borrow()[&0], 2);
    let composed = first_composed.get();
    let _ = run(&mut ui, async move { handle.scroll_by(10.0).await });
    let info = handle.layout_info();
    assert!(info.visible_items_info.iter().any(|item| item.index == 0), "item 0 still shows");
    assert_eq!(first_composed.get(), composed);
    let _ = run(&mut ui, async move { handle.scroll_by(60.0).await });
    assert_eq!((seen.borrow()[&4], seen.borrow()[&5]), (2, 2));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_withCustomItemSpansAndStickyHeader
#[test]
fn every_kind_of_item_knows_the_lines_cells() {
    let columns = mutable_state_of(4usize);
    let read = columns.handle();
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let counts = Rc::new(RefCell::new([0usize; 4]));
    let noted = counts.clone();
    let mut ui = rule(move || {
        let noted = noted.clone();
        LazyVerticalGrid!(
            fixed(read.get()),
            state = handle,
            modifier = Modifier.size(200.dp()),
            move |scope: LazyGridScope| {
                let note = |at: usize| {
                    let (noted, items) = (noted.clone(), scope.item_scope());
                    move || {
                        noted.borrow_mut()[at] = items.cross_axis_cell_count();
                        Box!(modifier = Modifier.size(10.dp()), {});
                    }
                };
                let header = note(0);
                scope.sticky_header(move |_| header());
                scope.item_with_span(|_| GridItemSpan(2), "double", note(1));
                scope.item_with_span(|_| GridItemSpan(1), "single", note(2));
                scope.item_with_span(
                    |line: LazyGridItemSpanScope| GridItemSpan(line.max_line_span()),
                    "full",
                    note(3),
                );
            }
        );
    });
    assert_eq!(*counts.borrow(), [4; 4]);
    assert_eq!(state.layout_info().max_span, 4);
    columns.set(6);
    ui.wait_for_idle();
    assert_eq!(*counts.borrow(), [6; 4]);
    assert_eq!(state.layout_info().max_span, 6);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, spansCalculationDoesntCrash
#[test]
fn spans_of_many_intervals_can_be_walked_to_the_end() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyVerticalGrid!(
            fixed(2),
            state = handle,
            modifier = Modifier.size(100.dp()),
            |scope: LazyGridScope| {
                for _ in 0..100 {
                    scope.item_with_span(
                        |line: LazyGridItemSpanScope| GridItemSpan(line.max_line_span()),
                        "whole",
                        || Box!(modifier = Modifier.fill_max_width().height(1.dp()), {}),
                    );
                    scope.items(10, |_| {
                        Box!(modifier = Modifier.fill_max_width().height(1.dp()), {})
                    });
                }
            }
        );
    });
    let total = state.layout_info().total_items_count;
    let _ = run(&mut ui, async move { handle.scroll_to_item(total - 1, 0).await });
}

/// An item drawing a square far larger than it, `color`, in a node at `z`.
fn painting(color: compose_ui::Color, z: f32, lazy: Lazy) {
    let size = axis_size(lazy, 6.0, 2.0);
    Box!(
        modifier = size.z_index(z).draw_behind(move |scope| {
            scope.draw_rect(
                compose_ui::Rect::new(-10.0, -10.0, 10.0, 10.0),
                compose_ui::Paint::fill(color),
            );
        }),
        {}
    );
}

/// The colors drawn, in order, walking into items' recordings.
fn colors_in_order(display: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    for line in display.lines() {
        for (name, code) in [("blue", "#FF0000FF"), ("green", "#FF00FF00"), ("red", "#FFFF0000")] {
            if line.contains(code) {
                found.push(name);
            }
        }
    }
    found
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, zIndexOnItemAffectsDrawingOrder
#[test]
fn a_grid_items_z_index_orders_it_among_the_items() {
    use compose_ui::Color;
    for lazy in Lazy::grids() {
        let ui = rule(move || {
            Grid(Args::new(lazy, fixed(1), Modifier.size(6.dp())), move |scope| {
                scope.items(3, move |index| {
                    let (color, z) = match index {
                        0 => (Color::BLUE, 0.0),
                        1 => (Color::GREEN, 1.0),
                        _ => (Color::RED, 0.0),
                    };
                    painting(color, z, lazy);
                });
            });
        });
        assert_eq!(colors_in_order(&ui.dump_display_list()), ["blue", "red", "green"], "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, zIndexOnItemAffectsDrawingOrder
#[test]
fn a_list_items_z_index_orders_it_among_the_items() {
    use compose_ui::Color;
    for lazy in Lazy::grids() {
        let ui = rule(move || {
            crate::lazy_support::LazyColumnOrRow(
                lazy,
                Modifier.size(6.dp()),
                remember_lazy_list_state(0, 0),
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items(3, move |index| {
                        let (color, z) = match index {
                            0 => (Color::BLUE, 0.0),
                            1 => (Color::GREEN, 1.0),
                            _ => (Color::RED, 0.0),
                        };
                        painting(color, z, lazy);
                    });
                },
            );
        });
        assert_eq!(colors_in_order(&ui.dump_display_list()), ["blue", "red", "green"], "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridTest.kt, zIndexOnItemAffectsDrawingOrder: the order follows a z that changes
#[test]
fn an_items_z_index_can_change() {
    use compose_ui::Color;
    let lazy = Lazy::column();
    let raised = mutable_state_of(0.0f32);
    let read = raised.handle();
    let mut ui = rule(move || {
        Grid(Args::new(lazy, fixed(1), Modifier.size(6.dp())), move |scope| {
            scope.items(2, move |index| {
                let (color, z) =
                    if index == 0 { (Color::BLUE, read.get()) } else { (Color::RED, 0.0) };
                painting(color, z, lazy);
            });
        });
    });
    assert_eq!(colors_in_order(&ui.dump_display_list()), ["blue", "red"]);
    raised.set(1.0);
    ui.wait_for_idle();
    assert_eq!(colors_in_order(&ui.dump_display_list()), ["red", "blue"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_availableInItemScope_adaptiveGrid
#[test]
fn an_item_knows_the_cells_of_an_adaptive_grid_through_its_padding() {
    let padding = mutable_state_of(0.0f32);
    let read = padding.handle();
    let reported = Rc::new(Cell::new(0));
    let noted = reported.clone();
    let mut ui = rule(move || {
        let noted = noted.clone();
        LazyVerticalGrid!(
            GridCells::Adaptive(100.dp()),
            modifier = Modifier.size((350.dp(), 200.dp())),
            content_padding = PaddingValues::symmetric(Dp(read.get()), 0.dp()),
            horizontal_arrangement = Arrangement::spaced_by(10.dp()),
            move |scope: LazyGridScope| {
                let (noted, items) = (noted.clone(), scope.item_scope());
                scope.item(move || {
                    noted.set(items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            }
        );
    });
    assert_eq!(reported.get(), 3);
    padding.set(20.0);
    ui.wait_for_idle();
    assert_eq!(reported.get(), 2);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_recomposesWhenFixedCellsCountChanges
#[test]
fn an_item_follows_a_fixed_grids_cells() {
    let columns = mutable_state_of(1usize);
    let read = columns.handle();
    let reported = Rc::new(Cell::new(0));
    let noted = reported.clone();
    let mut ui = rule(move || {
        let noted = noted.clone();
        LazyVerticalGrid!(
            fixed(read.get()),
            modifier = Modifier.size(200.dp()),
            move |scope: LazyGridScope| {
                let (noted, items) = (noted.clone(), scope.item_scope());
                scope.item(move || {
                    noted.set(items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            }
        );
    });
    assert_eq!(reported.get(), 1);
    for count in [3, 1] {
        columns.set(count);
        ui.wait_for_idle();
        assert_eq!(reported.get(), count);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_recomposesWhenRowCountChanges_horizontalGrid
#[test]
fn an_item_follows_a_horizontal_adaptive_grids_rows() {
    let height = mutable_state_of(220i32);
    let read = height.handle();
    let reported = Rc::new(Cell::new(0));
    let composed = Rc::new(Cell::new(0));
    let (noted, counted) = (reported.clone(), composed.clone());
    let mut ui = rule(move || {
        let (noted, counted) = (noted.clone(), counted.clone());
        LazyHorizontalGrid!(
            GridCells::Adaptive(100.dp()),
            modifier = Modifier.layout(move |scope, measurable, _: Constraints| {
                let placeable = scope.measure(measurable, Constraints::fixed(200, read.get()));
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            }),
            move |scope: LazyGridScope| {
                let (noted, counted, items) = (noted.clone(), counted.clone(), scope.item_scope());
                scope.item(move || {
                    counted.set(counted.get() + 1);
                    noted.set(items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            }
        );
    });
    assert_eq!(reported.get(), 2);
    let first = composed.get();
    height.set(350);
    ui.wait_for_idle();
    assert_eq!((reported.get(), composed.get()), (3, first + 1));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_preservedWhenItemsUpdate
#[test]
fn an_item_keeps_the_cells_through_new_items() {
    let count = mutable_state_of(2usize);
    let columns = mutable_state_of(3usize);
    let (read_count, read_columns) = (count.handle(), columns.handle());
    let seen: Rc<RefCell<std::collections::BTreeMap<usize, usize>>> = Rc::default();
    let first_item: Rc<RefCell<Vec<usize>>> = Rc::default();
    let (noted, observed) = (seen.clone(), first_item.clone());
    let mut ui = rule(move || {
        let (noted, observed) = (noted.clone(), observed.clone());
        let items_count = read_count.get();
        LazyVerticalGrid!(
            fixed(read_columns.get()),
            modifier = Modifier.size(200.dp()),
            move |scope: LazyGridScope| {
                let (noted, observed, items) =
                    (noted.clone(), observed.clone(), scope.item_scope());
                scope.items_keyed(
                    items_count,
                    |index| index,
                    move |index| {
                        let cells = items.cross_axis_cell_count();
                        if index == 0 && observed.borrow().last() != Some(&cells) {
                            observed.borrow_mut().push(cells);
                        }
                        noted.borrow_mut().insert(index, cells);
                        Box!(modifier = Modifier.size(10.dp()), {});
                    },
                );
            }
        );
    });
    assert_eq!(*first_item.borrow().last().unwrap(), 3);
    count.set(4);
    ui.wait_for_idle();
    assert!(first_item.borrow().iter().skip(1).all(|cells| *cells == 3), "never reset");
    assert!(seen.borrow().values().all(|cells| *cells == 3), "{:?}", seen.borrow());
    assert_eq!(seen.borrow().len(), 4);
    columns.set(5);
    ui.wait_for_idle();
    assert!(seen.borrow().values().all(|cells| *cells == 5), "{:?}", seen.borrow());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemScopeImpl.kt, crossAxisCellCount: at least 1, so what divides by it can
#[test]
fn an_items_first_composition_can_divide_by_the_cells() {
    let reported = Rc::new(Cell::new(usize::MAX));
    let noted = reported.clone();
    let _ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let noted = noted.clone();
        LazyVerticalGrid!(
            GridCells::Adaptive(10.dp()),
            modifier = Modifier.size(100.dp()),
            move |scope: LazyGridScope| {
                let (noted, items) = (noted.clone(), scope.item_scope());
                scope.item(move || {
                    let cells = items.cross_axis_cell_count();
                    noted.set(noted.get().min(cells));
                    let width = 100 / cells;
                    Box!(modifier = Modifier.width(Dp(width as f32)), {});
                });
            }
        );
    });
    assert!(reported.get() >= 1);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGrid.kt, `remember(columns, arrangement)`, GridSlotCache: the cells aren't asked for again with the same way to work them out
#[test]
fn stable_cells_of_your_own_are_asked_once() {
    struct Counted(Rc<Cell<usize>>);
    impl CrossAxisCells for Counted {
        fn calculate_cross_axis_cell_sizes(
            &self,
            _: compose_ui::Density,
            available: i32,
            _: i32,
        ) -> Vec<i32> {
            self.0.set(self.0.get() + 1);
            vec![available]
        }
    }
    let asked = Rc::new(Cell::new(0));
    let cells = GridCells::Custom(Rc::new(Counted(asked.clone())));
    let tick = mutable_state_of(0);
    let read = tick.handle();
    let mut ui = rule(move || {
        read.get();
        LazyVerticalGrid!(
            cells.clone(),
            modifier = Modifier.size(100.dp()),
            |scope: LazyGridScope| {
                scope.items(3, |_| Spacer(Modifier.height(10.dp())));
            }
        );
    });
    assert_eq!(asked.get(), 1);
    for value in 1..4 {
        tick.set(value);
        ui.wait_for_idle();
    }
    assert_eq!(asked.get(), 1, "a caller's recomposition asks nothing");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridDsl.kt, `GridCells`: at least one cell
#[test]
#[should_panic(expected = "at least one cell")]
fn cells_of_your_own_are_at_least_one() {
    struct Nothing;
    impl CrossAxisCells for Nothing {
        fn calculate_cross_axis_cell_sizes(
            &self,
            _: compose_ui::Density,
            _: i32,
            _: i32,
        ) -> Vec<i32> {
            Vec::new()
        }
    }
    let _ui = rule(|| {
        LazyVerticalGrid!(
            GridCells::Custom(Rc::new(Nothing)),
            modifier = Modifier.size(100.dp()),
            |scope: LazyGridScope| scope.item(|| {})
        );
    });
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridDsl.kt, GridCells.FixedSize: `requirePrecondition(size > 0.dp)`
#[test]
#[should_panic(expected = "Provided size should be larger than zero.")]
fn a_fixed_size_cell_of_nothing_panics() {
    let _ui = rule(|| {
        LazyVerticalGrid!(
            GridCells::FixedSize(0.dp()),
            modifier = Modifier.size(100.dp()),
            |scope: LazyGridScope| scope.item(|| {})
        );
    });
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridDsl.kt, GridCells.FixedSize: as many as fit with the spacing between them
#[test]
fn fixed_size_cells_count_the_spacing_between_them() {
    for (room, expected) in [(305, 2), (320, 3)] {
        let reported = Rc::new(Cell::new(0));
        let noted = reported.clone();
        let _ui = rule(move || {
            let noted = noted.clone();
            LazyVerticalGrid!(
                GridCells::FixedSize(100.dp()),
                modifier = Modifier.size((Dp(room as f32), 200.dp())),
                horizontal_arrangement = Arrangement::spaced_by(10.dp()),
                move |scope: LazyGridScope| {
                    let (noted, items) = (noted.clone(), scope.item_scope());
                    scope.item(move || noted.set(items.cross_axis_cell_count()));
                }
            );
        });
        assert_eq!(reported.get(), expected, "in {room}");
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGrid.kt, `rememberLazyGridSlots`: `slots.sizes` and `positions` at least 0 for a cell across nothing
#[test]
fn an_item_across_cells_arranged_over_themselves_is_at_least_nothing() {
    let mut ui = rule(|| {
        LazyVerticalGrid!(
            fixed(2),
            modifier = Modifier.size((10.dp(), 50.dp())),
            horizontal_arrangement = Arrangement::spaced_by(30.dp()),
            |scope: LazyGridScope| {
                scope.items_with_span(
                    2,
                    |line, _| GridItemSpan(line.max_line_span()),
                    |_| 0,
                    |index| {
                        Spacer(Modifier.fill_max_width().height(5.dp()).test_tag(index.to_string()))
                    },
                );
            }
        );
    });
    assert!(bounds(&mut ui, "0").width() >= 0.0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridLayoutInfoTest.kt, the reversed configurations with no items
#[test]
fn an_empty_reversed_grid_says_it_is() {
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let _ui = rule(move || {
        LazyVerticalGrid!(
            fixed(2),
            modifier = Modifier.size(100.dp()),
            state = handle,
            reverse_layout = true,
            |_: LazyGridScope| {}
        );
    });
    assert!(state.layout_info().reverse_layout);
}

/// `Fixed(1)` lines at a spacing of -50: even ones of 100 px, advancing 50; odd ones of 10 px, smaller
/// than the spacing, advancing nothing (`mainAxisSizeWithSpacings`, at least 0).
fn overlapping_grid(
    first: usize,
    offset: i32,
    before: f32,
    height: f32,
    beyond: bool,
) -> (ComposeTestRule, Owned<LazyGridState>) {
    use compose_ui::platform::Optimization;
    use_optimizations(Optimizations::NONE.with(Optimization::GRID_LINES_BEYOND, beyond));
    let state = LazyGridState::new(first, offset);
    let handle = *state;
    let ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        LazyVerticalGrid!(
            fixed(1),
            modifier = Modifier.required_size((50.dp(), Dp(height))),
            state = handle,
            content_padding = PaddingValues { top: Dp(before), ..PaddingValues::ZERO },
            vertical_arrangement = Arrangement::spaced_by(-50.dp()),
            |scope: LazyGridScope| {
                scope.items(40, |index| {
                    let main = if index.is_multiple_of(2) { 100.dp() } else { 10.dp() };
                    Box!(modifier = Modifier.size((50.dp(), main)).test_tag(index.to_string()), {});
                });
            }
        );
    });
    (ui, state)
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridMeasuredLine.kt, mainAxisSizeWithSpacings: `coerceAtLeast(0)`, stepping back in LazyGridMeasure.kt
#[test]
fn stepping_back_over_a_line_smaller_than_the_spacing_takes_nothing() {
    let (mut ui, state) = overlapping_grid(6, -10, 0.0, 150.0, false);
    ui.wait_for_idle();
    // Line 5, 10 px, takes none of the 10 px; line 4, 100 px, has 50 with the spacing: 40 px into it.
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (4, 40)
    );
    assert_eq!(Lazy::column().main_start(&mut ui, "4"), -40.0);
}

// Compose: the same, laying lines out one after the other
#[test]
fn lines_smaller_than_the_spacing_advance_nothing() {
    let (mut ui, _state) = overlapping_grid(0, 0, 0.0, 150.0, false);
    let lazy = Lazy::column();
    for (index, at) in [(0, 0.0), (1, 50.0), (2, 50.0), (3, 100.0), (4, 100.0)] {
        assert_eq!(lazy.main_start(&mut ui, &index.to_string()), at, "{index}");
    }
}

// Compose: the same, for lines laid out beyond those in view
#[test]
fn lines_beyond_those_in_view_smaller_than_the_spacing_advance_nothing() {
    let (mut ui, _state) = overlapping_grid(0, 0, 0.0, 150.0, true);
    let lazy = Lazy::column();
    // Line 4 starts at 100 and ends the view at 150; beyond it, 5 at 150 and 6 with it.
    assert_eq!(lazy.main_start(&mut ui, "5"), 150.0);
    assert_eq!(lazy.main_start(&mut ui, "6"), 150.0);
}

// Compose: the same, for lines laid out beyond those before the view
#[test]
fn lines_before_those_in_view_smaller_than_the_spacing_advance_nothing() {
    let (mut ui, _state) = overlapping_grid(6, 0, 0.0, 150.0, true);
    let lazy = Lazy::column();
    // Line 4 walks back to -50 (its 50 with the spacing): beyond it, 3 at -50 and 2 at -100.
    assert_eq!(lazy.main_start(&mut ui, "4"), -50.0);
    assert_eq!(lazy.main_start(&mut ui, "3"), -50.0);
    assert_eq!(lazy.main_start(&mut ui, "2"), -100.0);
}

// Compose: the same, for the lines beyond the last in view: padding before larger than the view
#[test]
fn lines_after_the_last_in_view_count_their_size_with_the_spacing() {
    let (mut ui, _state) = overlapping_grid(1, 0, 200.0, 150.0, true);
    let lazy = Lazy::column();
    // Item 1, 10 px, at the padding's end: at 0 with the spacing, so the walk goes on to item 2.
    assert_eq!(lazy.main_start(&mut ui, "1"), 200.0);
    assert_eq!(lazy.main_start(&mut ui, "2"), 200.0);
    // Past those in view, the same rule: 100 px is 50 more, 10 px is none.
    assert_eq!(lazy.main_start(&mut ui, "3"), 250.0);
    assert_eq!(lazy.main_start(&mut ui, "4"), 250.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridItemScopeImpl.kt, crossAxisCellCount: at least 1 with cells of your own too, which nothing estimates here
#[test]
fn an_item_can_divide_by_cells_of_your_own() {
    struct Two;
    impl CrossAxisCells for Two {
        fn calculate_cross_axis_cell_sizes(
            &self,
            _: compose_ui::Density,
            room: i32,
            _: i32,
        ) -> Vec<i32> {
            vec![room / 2, room / 2]
        }
    }
    let reported = Rc::new(Cell::new(0));
    let noted = reported.clone();
    let _ui = rule(move || {
        let noted = noted.clone();
        LazyVerticalGrid!(
            GridCells::Custom(Rc::new(Two)),
            modifier = Modifier.size(100.dp()),
            move |scope: LazyGridScope| {
                let (noted, items) = (noted.clone(), scope.item_scope());
                scope.item(move || {
                    let cells = items.cross_axis_cell_count();
                    noted.set(cells);
                    Box!(modifier = Modifier.width(Dp((100 / cells) as f32)), {});
                });
            }
        );
    });
    assert_eq!(reported.get(), 2);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridSpanTest.kt, crossAxisCellCount_recomposesWhenColumnCountChanges: `compositionCount` of 1, in a grid filling the room around, where the estimate is right
#[test]
fn an_item_in_a_grid_filling_the_room_knows_the_cells_at_once() {
    let composed = Rc::new(Cell::new(0));
    let reported = Rc::new(Cell::new(0));
    let (counted, noted) = (composed.clone(), reported.clone());
    let _ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
        let (counted, noted) = (counted.clone(), noted.clone());
        LazyVerticalGrid!(
            GridCells::Adaptive(100.dp()),
            modifier = Modifier.fill_max_size(),
            move |scope: LazyGridScope| {
                let (counted, noted, items) = (counted.clone(), noted.clone(), scope.item_scope());
                scope.item(move || {
                    counted.set(counted.get() + 1);
                    noted.set(items.cross_axis_cell_count());
                    Box!(modifier = Modifier.size(10.dp()), {});
                });
            }
        );
    });
    assert_eq!((reported.get(), composed.get()), (4, 1));
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGrid.kt, `measuredLineProvider.childConstraints`: with new items a prefetched item's measured in the cells as they are
#[test]
fn an_item_prefetched_after_new_spans_is_measured_in_its_new_cell() {
    let wide = mutable_state_of(false);
    let read = wide.handle();
    let widths: Rc<RefCell<Vec<i32>>> = Rc::default();
    let noted = widths.clone();
    let state = LazyGridState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        let noted = noted.clone();
        let wide = read.get();
        LazyVerticalGrid!(
            fixed(2),
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = handle,
            move |scope: LazyGridScope| {
                let noted = noted.clone();
                scope.items_with_span(
                    40,
                    move |span, index| {
                        GridItemSpan(if wide && index == 10 { span.max_line_span() } else { 1 })
                    },
                    |_| 0,
                    move |index| {
                        let noted = noted.clone();
                        let sized =
                            Modifier.layout(move |scope, measurable, constraints: Constraints| {
                                if index == 10 {
                                    noted.borrow_mut().push(constraints.max_width);
                                }
                                let placeable = scope.measure(measurable, constraints);
                                scope.place(&placeable, 0, 0);
                                scope.layout(placeable.width, placeable.height)
                            });
                        Spacer(sized.height(50.dp()));
                    },
                );
            }
        );
    });
    ui.wait_for_idle();
    // New spans, then a scroll to 140: lines 2 to 4 in view, item 10 on line 5 only a request.
    wide.set(true);
    ui.wait_for_idle();
    let later = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let _ = run(&mut ui, async move { handle.scroll_by(140.0).await });
    assert_eq!(handle.layout_info().visible_items_info.last().unwrap().index, 9);
    while ui.host().prefetch(later) {
        ui.wait_for_idle();
    }
    assert_eq!(*widths.borrow(), vec![100], "item 10 once, across the line it fills");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/grid/LazyGridMeasuredItem.kt place
#[test]
fn reversed_an_items_layouts_end_together() {
    for lazy in Lazy::grids() {
        let content = |scope: LazyGridScope| {
            scope.item(|| {
                Box!(modifier = Modifier.size(200.dp()).test_tag("0"));
                Box!(modifier = Modifier.size(100.dp()).test_tag("1"));
            });
        };
        let mut ui = rule(move || {
            let modifier = axis_size(lazy, 200.0, 400.0);
            if lazy.vertical {
                LazyVerticalGrid!(
                    fixed(2),
                    modifier = modifier,
                    reverse_layout = true,
                    move |scope: LazyGridScope| content(scope)
                );
            } else {
                LazyHorizontalGrid!(
                    fixed(2),
                    modifier = modifier,
                    reverse_layout = true,
                    move |scope: LazyGridScope| content(scope)
                );
            }
        });
        assert_eq!(lazy.main_start(&mut ui, "0"), 200.0);
        assert_eq!(lazy.main_size(&mut ui, "0"), 200.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), 300.0);
        assert_eq!(lazy.main_size(&mut ui, "1"), 100.0);
    }
}

/// `layout_info` read where the grid is called: it goes idle. Its content runs again with new
/// closures, so new items, which say nothing new: Compose's lambda is memoized (D1471).
#[test]
fn reading_layout_info_where_the_grid_is_called_goes_idle() {
    let reads = Rc::new(Cell::new(0));
    let counted = reads.clone();
    let owned = LazyGridState::new(0, 0);
    let state = *owned;
    let mut ui = rule(move || {
        let _ = state.layout_info();
        counted.set(counted.get() + 1);
        LazyVerticalGrid!(
            fixed(2),
            modifier = Modifier.size(300.dp()),
            state = state,
            |scope: LazyGridScope| {
                scope.items(60, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    ui.wait_for_idle();
    assert!(reads.get() < 8, "{} compositions", reads.get());
}
