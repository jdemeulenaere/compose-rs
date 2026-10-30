//! Grids at large, as Compose's `LazyGridTest`, in both orientations. Density 1, so its dps are
//! pixels. Left: the lookahead tests (no lookahead), `customOverscroll` (feature:overscroll),
//! `zIndexOnItemAffectsDrawingOrder` (a test of pixels), `onlyOneInitialMeasurePass` (composing for
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
        let tree = ui.on_node_with_tag(GRID).fetch_semantics_node();
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
