//! Lazy lists at large, as Compose's `LazyListTest`: the forms of items, what's shown and
//! composed as the list scrolls, wrapping content, filling the parent, data changes, the state
//! after scrolls, and what is (not) recomposed or redrawn. Both orientations; density 1, so
//! Compose's dps are pixels.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, UnitExt};

use crate::lazy_support::{Lazy, LazyColumnOrRow, is_displayed, no_padding};
use crate::scroll_support::{Touch, run};

const LIST: &str = "LazyListTag";
const FIRST: &str = "firstItemTag";

fn orientations() -> [Lazy; 2] {
    [Lazy::column(), Lazy::row()]
}

/// A test rule for `content` in a big root, at parity (no item kept or prefetched beyond
/// Compose's).
fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyListShowsCombinedItems
#[test]
fn single_list_and_indexed_items_show_together() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                lazy.main_axis_size(200.dp()).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    let item_scope = scope.item_scope();
                    scope.item(move || {
                        Spacer(
                            lazy.main_axis_size(40.dp())
                                .then(lazy.fill_parent_max_cross_axis(&item_scope))
                                .test_tag("itemTestTag"),
                        );
                    });
                    scope.items_list(vec!["1", "2"], move |item_scope, item: &&str| {
                        let tag = item.to_string();
                        Spacer(
                            lazy.main_axis_size(40.dp())
                                .then(lazy.fill_parent_max_cross_axis(&item_scope))
                                .test_tag(tag),
                        );
                    });
                    scope.items_indexed(vec![3, 4, 5], move |item_scope, index, item: &i32| {
                        Spacer(
                            lazy.main_axis_size(41.dp())
                                .then(lazy.fill_parent_max_cross_axis(&item_scope))
                                .test_tag(format!("{index}-{item}")),
                        );
                    });
                },
            );
        });
        for tag in ["itemTestTag", "1", "2", "0-3", "1-4"] {
            assert!(is_displayed(&mut ui, tag, LIST), "{lazy:?}: {tag} shows");
        }
        ui.on_node_with_tag("2-5").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyListAllowEmptyListItems
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyListAllowsNullableItems
#[test]
fn empty_lists_and_absent_items_are_items_too() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                |scope: LazyListScope| {
                    scope.items_list(Vec::<u32>::new(), |_, _: &u32| {});
                    scope.item(|| Spacer(Modifier.size(10.dp()).test_tag("itemTag")));
                },
            );
        });
        assert!(is_displayed(&mut ui, "itemTag", LIST));
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                lazy.main_axis_size(200.dp()).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list(vec![Some("1"), None, Some("3")], move |item_scope, item| {
                        let tag = item.unwrap_or("nullTestTag").to_string();
                        Spacer(
                            lazy.main_axis_size(101.dp())
                                .then(lazy.fill_parent_max_cross_axis(&item_scope))
                                .test_tag(tag),
                        );
                    });
                },
            );
        });
        assert!(is_displayed(&mut ui, "1", LIST));
        assert!(is_displayed(&mut ui, "nullTestTag", LIST));
        ui.on_node_with_tag("3").assert_does_not_exist();
    }
}

/// Four 101 px items in a 200 px box, as Compose's scrolling tests.
fn four_items(lazy: Lazy) -> ComposeTestRule {
    rule(move || {
        Box!(modifier = lazy.main_axis_size(200.dp()), {
            LazyColumnOrRow(
                lazy,
                Modifier.test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list(vec!["1", "2", "3", "4"], move |item_scope, item: &&str| {
                        let tag = item.to_string();
                        Spacer(
                            lazy.main_axis_size(101.dp())
                                .then(lazy.fill_parent_max_cross_axis(&item_scope))
                                .test_tag(tag),
                        );
                    });
                },
            );
        });
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyList_noBeyondBoundItemsCount_OnlyVisibleItemsAdded
#[test]
fn only_the_items_in_view_are_composed() {
    for lazy in orientations() {
        let mut ui = four_items(lazy);
        assert!(is_displayed(&mut ui, "1", LIST) && is_displayed(&mut ui, "2", LIST));
        ui.on_node_with_tag("3").assert_does_not_exist();
        ui.on_node_with_tag("4").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyListScrollToShowItems123
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyListScrollToHideFirstItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyListScrollToShowItems234
#[test]
fn scrolling_shows_the_items_coming_in_and_hides_those_going_out() {
    for (distance, shown, hidden) in [
        (50.0, &["1", "2", "3"][..], &["4"][..]),
        (105.0, &["2", "3"][..], &["1"][..]),
        (150.0, &["2", "3", "4"][..], &["1"][..]),
    ] {
        for lazy in orientations() {
            let mut ui = four_items(lazy);
            lazy.scroll_main_axis_by(&mut ui, LIST, distance);
            for tag in shown {
                assert!(is_displayed(&mut ui, tag, LIST), "{lazy:?} +{distance}: {tag} shows");
            }
            for tag in hidden {
                assert!(!is_displayed(&mut ui, tag, LIST), "{lazy:?} +{distance}: {tag} hidden");
            }
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, lazyListWrapsContent
#[test]
fn a_list_wraps_its_content() {
    for lazy in orientations() {
        let same = mutable_state_of(true);
        let same_handle = same.handle();
        let mut ui = rule(move || {
            let same = same_handle.get();
            let column = |content: Box<dyn Fn()>| {
                if lazy.vertical {
                    Column!({ content() });
                } else {
                    Row!({ content() });
                }
            };
            column(Box::new(move || {
                LazyColumnOrRow(
                    lazy,
                    Modifier.test_tag(LIST),
                    remember_lazy_list_state!(),
                    no_padding(),
                    move |scope: LazyListScope| {
                        scope.item(|| Spacer(Modifier.size(50.dp()).test_tag("inside")));
                        scope.item(move || {
                            Spacer(Modifier.size(if same { 50.dp() } else { 70.dp() }));
                        });
                    },
                );
                Spacer(Modifier.size(50.dp()).test_tag("outside"));
            }));
        });
        let check = |ui: &mut ComposeTestRule, main: f32, cross: f32| {
            let list = crate::lazy_support::bounds(ui, LIST);
            let (main_end, cross_end) =
                if lazy.vertical { (list.bottom, list.right) } else { (list.right, list.bottom) };
            assert_eq!((list.left, list.top, main_end, cross_end), (0.0, 0.0, main, cross));
        };
        assert!(is_displayed(&mut ui, "outside", "outside"));
        check(&mut ui, 100.0, 50.0);
        same.set(false);
        ui.wait_for_idle();
        check(&mut ui, 120.0, 70.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, compositionsAreDisposed_whenNodesAreScrolledOff
#[test]
fn compositions_are_disposed_when_their_items_scroll_off() {
    for lazy in orientations() {
        let composed = Rc::new(Cell::new(0usize));
        let disposed = Rc::new(Cell::new(0usize));
        let (into_composed, into_disposed) = (composed.clone(), disposed.clone());
        let mut ui = rule(move || {
            let (composed, disposed) = (into_composed.clone(), into_disposed.clone());
            Box!(modifier = lazy.main_axis_size(300.dp()).test_tag(LIST), {
                let (composed, disposed) = (composed.clone(), disposed.clone());
                LazyColumnOrRow(
                    lazy,
                    Modifier.fill_max_size(),
                    remember_lazy_list_state!(),
                    no_padding(),
                    move |scope: LazyListScope| {
                        let (composed, disposed) = (composed.clone(), disposed.clone());
                        scope.items(50, move |index| {
                            let (composed, disposed) = (composed.clone(), disposed.clone());
                            DisposableEffect(index, move |effects| {
                                composed.set(composed.get() + 1);
                                effects.on_dispose(move || disposed.set(disposed.get() + 1))
                            });
                            Spacer(lazy.main_axis_size(31.dp()));
                        });
                    },
                );
            });
        });
        // Ten 31 px items in 300.
        assert_eq!((composed.get(), disposed.get()), (10, 0), "{lazy:?}");
        let (from, to) = if lazy.vertical {
            (compose_ui::Offset::new(50.0, 250.0), compose_ui::Offset::new(50.0, 50.0))
        } else {
            (compose_ui::Offset::new(250.0, 50.0), compose_ui::Offset::new(50.0, 50.0))
        };
        crate::scroll_support::swipe(&mut ui, from, to, 200);
        ui.wait_for_idle();
        assert!(composed.get() > 10, "more items composed after the swipe");
        assert!(disposed.get() > 0, "items scrolled off were let go");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, whenItemsAreInitiallyCreatedWith0SizeWeCanScrollWhenTheyExpanded
#[test]
fn an_item_growing_from_nothing_can_be_scrolled_to() {
    for lazy in orientations() {
        let has_size = mutable_state_of(false);
        let handle = has_size.handle();
        let mut ui = rule(move || {
            let cross =
                if lazy.vertical { Modifier.fill_max_width() } else { Modifier.fill_max_height() };
            LazyColumnOrRow(
                lazy,
                cross.then(lazy.main_axis_size(100.dp())).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list(vec![1, 2, 3], move |item_scope, item: &i32| {
                        let fill = lazy.fill_parent_max_cross_axis(&item_scope);
                        if *item == 3 {
                            let size = if handle.get() { 60.dp() } else { 0.dp() };
                            Spacer(fill.test_tag("third").then(lazy.main_axis_size(size)));
                        } else {
                            Spacer(fill.then(lazy.main_axis_size(60.dp())));
                        }
                    });
                },
            );
        });
        lazy.scroll_main_axis_by(&mut ui, LIST, 21.0);
        ui.on_node_with_tag("third").assert_exists();
        assert!(!is_displayed(&mut ui, "third", LIST));
        has_size.set(true);
        ui.wait_for_idle();
        lazy.scroll_main_axis_by(&mut ui, LIST, 10.0);
        assert!(is_displayed(&mut ui, "third", LIST), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemFillingParentWidth
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemFillingParentHeight
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemFillingParentSize
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemFillingParentWidthFraction
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemFillingParentHeightFraction
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemFillingParentSizeFraction
#[test]
fn items_fill_the_parent_or_a_fraction_of_it() {
    type Fill = fn(&LazyItemScope) -> Modifier;
    let cases: [(Fill, (f32, f32)); 6] = [
        (|scope| Modifier.fill_parent_max_width(scope).height(50.dp()), (100.0, 50.0)),
        (|scope| Modifier.width(50.dp()).fill_parent_max_height(scope), (50.0, 150.0)),
        (|scope| Modifier.fill_parent_max_size(scope), (100.0, 150.0)),
        (|scope| Modifier.fill_parent_max_width_with(scope, 0.7).height(50.dp()), (70.0, 50.0)),
        (|scope| Modifier.width(50.dp()).fill_parent_max_height_with(scope, 0.3), (50.0, 45.0)),
        (|scope| Modifier.fill_parent_max_size_with(scope, 0.5), (50.0, 75.0)),
    ];
    for lazy in orientations() {
        for (fill, size) in cases {
            let mut ui = rule(move || {
                LazyColumnOrRow(
                    lazy,
                    Modifier.required_size((100.dp(), 150.dp())),
                    remember_lazy_list_state!(),
                    no_padding(),
                    move |scope: LazyListScope| {
                        scope.items_list(vec![0], move |item_scope, _: &i32| {
                            Spacer(fill(&item_scope).test_tag(FIRST));
                        });
                    },
                );
            });
            let item = crate::lazy_support::bounds(&mut ui, FIRST);
            assert_eq!((item.width(), item.height()), size, "{lazy:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemFillingParentSizeParentResized
#[test]
fn an_item_filling_the_parent_follows_its_size() {
    for lazy in orientations() {
        let size = mutable_state_of(100.0f32);
        let handle = size.handle();
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(handle.get().dp()),
                remember_lazy_list_state!(),
                no_padding(),
                |scope: LazyListScope| {
                    scope.items_list(vec![0], |item_scope, _: &i32| {
                        Spacer(Modifier.fill_parent_max_size(&item_scope).test_tag(FIRST));
                    });
                },
            );
        });
        size.set(150.0);
        ui.wait_for_idle();
        let item = crate::lazy_support::bounds(&mut ui, FIRST);
        assert_eq!((item.width(), item.height()), (150.0, 150.0));
    }
}

/// `items` of 20 px in a 100 px list.
fn twenty_px_items(
    lazy: Lazy,
    state: LazyListState,
    items: MutableState<Vec<u32>>,
) -> ComposeTestRule {
    rule(move || {
        let list = items.get();
        LazyColumnOrRow(
            lazy,
            Modifier.required_size(100.dp()).test_tag(LIST),
            state,
            no_padding(),
            move |scope: LazyListScope| {
                scope.items_list(list.clone(), |_, item: &u32| {
                    Spacer(Modifier.required_size(20.dp()).test_tag(item.to_string()));
                });
            },
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, whenNotAnymoreAvailableItemWasDisplayed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, whenFewDisplayedItemsWereRemoved
#[test]
fn when_the_items_shown_go_the_list_shows_its_last_ones() {
    for (count, scroll, after, first_shown) in [(30, 300.0, 10, "6"), (10, 100.0, 8, "4")] {
        for lazy in orientations() {
            let items = mutable_state_of((1..=count).collect::<Vec<u32>>());
            let state = LazyListState::new(0, 0);
            let mut ui = twenty_px_items(lazy, *state, items.handle());
            lazy.scroll_main_axis_by(&mut ui, LIST, scroll);
            items.set((1..=after).collect());
            ui.wait_for_idle();
            assert_eq!(lazy.main_start(&mut ui, first_shown), 0.0, "{lazy:?}: {count} to {after}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, whenItemsBecameEmpty
#[test]
fn an_emptied_list_has_no_size_and_no_items() {
    for lazy in orientations() {
        let items = mutable_state_of((1..=10).collect::<Vec<u32>>());
        let handle = items.handle();
        let mut ui = rule(move || {
            let list = handle.get();
            LazyColumnOrRow(
                lazy,
                Modifier
                    .size_in(compose_ui::SizeSpec::bounds(0.dp(), 0.dp(), 100.dp(), 100.dp()))
                    .test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list(list.clone(), |_, item: &u32| {
                        Spacer(Modifier.required_size(20.dp()).test_tag(item.to_string()));
                    });
                },
            );
        });
        lazy.scroll_main_axis_by(&mut ui, LIST, 20.0);
        items.set(Vec::new());
        ui.wait_for_idle();
        let list = crate::lazy_support::bounds(&mut ui, LIST);
        assert_eq!((list.width(), list.height()), (0.0, 0.0));
        ui.on_node_with_tag("1").assert_does_not_exist();
        ui.on_node_with_tag("2").assert_does_not_exist();
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scrollBackAndForth
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, tryToScrollBackwardWhenAlreadyOnTop
#[test]
fn scrolling_back_to_the_start_stops_there() {
    for lazy in orientations() {
        let items = mutable_state_of((1..=20).collect::<Vec<u32>>());
        let state = LazyListState::new(0, 0);
        let mut ui = twenty_px_items(lazy, *state, items.handle());
        lazy.scroll_main_axis_by(&mut ui, LIST, 100.0);
        lazy.scroll_main_axis_by(&mut ui, LIST, -100.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
        lazy.scroll_main_axis_by(&mut ui, LIST, -50.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0);
        assert_eq!(lazy.main_start(&mut ui, "5"), 80.0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, contentOfNotStableItemsIsNotRecomposedDuringScroll
#[test]
fn items_are_not_recomposed_by_a_scroll() {
    for lazy in orientations() {
        let counts = Rc::new(RefCell::new([0u32; 2]));
        let into = counts.clone();
        let mut ui = rule(move || {
            let counts = into.clone();
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(100.dp()).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    let counts = counts.clone();
                    scope.items_list(vec![1u32, 2], move |_, item: &u32| {
                        counts.borrow_mut()[*item as usize - 1] += 1;
                        Spacer(Modifier.required_size(75.dp()));
                    });
                },
            );
        });
        assert_eq!(*counts.borrow(), [1, 1]);
        lazy.scroll_main_axis_by(&mut ui, LIST, 50.0);
        assert_eq!(*counts.borrow(), [1, 1], "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, stateUpdatedAfterScroll
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, stateUpdatedAfterScrollWithinTheSameItem
#[test]
fn the_state_follows_a_scroll() {
    for (distance, expected) in [(30.0, (1, 10)), (10.0, (0, 10))] {
        for lazy in orientations() {
            let state = LazyListState::new(0, 0);
            let items = mutable_state_of((1..=20).collect::<Vec<u32>>());
            let mut ui = twenty_px_items(lazy, *state, items.handle());
            assert_eq!(state.first_visible_item_index(), 0);
            lazy.scroll_main_axis_by(&mut ui, LIST, distance);
            let position =
                (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
            assert_eq!(position, expected, "{lazy:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scroll_makeListSmaller_scroll
#[test]
fn a_scroll_after_the_list_shrank_works() {
    for lazy in orientations() {
        let items = mutable_state_of((1..=100).collect::<Vec<u32>>());
        let handle = items.handle();
        let mut ui = rule(move || {
            let list = handle.get();
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(100.dp()).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list(list.clone(), |_, item: &u32| {
                        Spacer(Modifier.required_size(10.dp()).test_tag(item.to_string()));
                    });
                },
            );
        });
        lazy.scroll_main_axis_by(&mut ui, LIST, 300.0);
        items.set((1..=11).collect());
        ui.wait_for_idle();
        lazy.scroll_main_axis_by(&mut ui, LIST, -10.0);
        assert!(is_displayed(&mut ui, "1", LIST), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, initialScrollIsApplied
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, snapToItemIndex
#[test]
fn the_initial_position_and_a_jump_are_applied() {
    for lazy in orientations() {
        let state = LazyListState::new(2, 10);
        let items = mutable_state_of((0..=20).collect::<Vec<u32>>());
        let mut ui = twenty_px_items(lazy, *state, items.handle());
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            (2, 10)
        );
        assert_eq!(lazy.main_start(&mut ui, "2"), -10.0);
        let handle = *state;
        let _ = run(&mut ui, async move { handle.scroll_to_item(3, 10).await });
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            (3, 10)
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemsAreNotRedrawnDuringScroll_noBeyondBoundItemsCount
#[test]
fn items_are_not_redrawn_by_a_scroll() {
    for lazy in orientations() {
        let draws = Rc::new(RefCell::new([0u32; 6]));
        let into = draws.clone();
        let mut ui = rule(move || {
            let draws = into.clone();
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(100.dp()).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    let draws = draws.clone();
                    scope.items(21, move |index| {
                        let draws = draws.clone();
                        Spacer(Modifier.required_size(20.dp()).draw_behind(move |_| {
                            if let Some(count) = draws.borrow_mut().get_mut(index) {
                                *count += 1;
                            }
                        }));
                    });
                },
            );
        });
        lazy.scroll_main_axis_by(&mut ui, LIST, 10.0);
        // Items 0 to 5: each drawn once (item 5 when it came into view).
        assert_eq!(*draws.borrow(), [1; 6], "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemInvalidationIsNotCausingAnotherItemToRedraw
#[test]
fn an_item_drawn_again_does_not_draw_its_neighbour() {
    for lazy in orientations() {
        let draws = Rc::new(RefCell::new([0u32; 2]));
        let into = draws.clone();
        let read = mutable_state_of(false);
        let read_handle = read.handle();
        let mut ui = rule(move || {
            let draws = into.clone();
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(100.dp()).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    let draws = draws.clone();
                    scope.items(2, move |index| {
                        let draws = draws.clone();
                        Spacer(Modifier.required_size(50.dp()).draw_behind(move |_| {
                            draws.borrow_mut()[index] += 1;
                            if index == 1 {
                                let _ = read_handle.get();
                            }
                        }));
                    });
                },
            );
        });
        read.set(true);
        ui.wait_for_idle();
        assert_eq!(*draws.borrow(), [1, 2], "{lazy:?}");
    }
}

/// Scrolls `state` by `distance` as Compose's tests' `state.scrollBy(…)` (a `scroll { scrollBy }`
/// run to its end).
fn scroll_state_by(ui: &mut ComposeTestRule, state: LazyListState, distance: f32) {
    let _ = run(ui, async move { state.scroll_by(distance).await });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, notVisibleAnymoreItemNotAffectingCrossAxisSize
#[test]
fn an_item_scrolled_out_no_longer_sizes_the_cross_axis() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                lazy.main_axis_size(29.dp()).test_tag(LIST),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items(2, move |index| {
                        let (cross, main) = if index == 0 { (30, 29) } else { (20, 30) };
                        Spacer(
                            lazy.cross_axis_size(cross.dp()).then(lazy.main_axis_size(main.dp())),
                        );
                    });
                },
            );
        });
        scroll_state_by(&mut ui, handle, 30.0);
        assert_eq!(lazy.cross_size(&mut ui, LIST), 20.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemStillVisibleAfterOverscrollIsAffectingCrossAxisSize
#[test]
fn an_item_still_in_view_at_the_end_sizes_the_cross_axis() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                lazy.main_axis_size(52.5.dp()).test_tag(LIST),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items(3, move |index| {
                        let (cross, main) = [(30.0, 15.0), (20.0, 15.0), (20.0, 30.0)][index];
                        Spacer(
                            lazy.cross_axis_size(cross.dp()).then(lazy.main_axis_size(main.dp())),
                        );
                    });
                },
            );
        });
        scroll_state_by(&mut ui, handle, 30.0);
        assert_eq!(lazy.cross_size(&mut ui, LIST), 30.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, usedWithArray
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, usedWithArrayIndexed
#[test]
fn items_of_an_array_are_laid_out_one_after_the_other() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier,
                remember_lazy_list_state!(),
                no_padding(),
                |scope: LazyListScope| {
                    scope.items_list(vec!["1", "2", "3"], |_, item: &&str| {
                        Spacer(Modifier.required_size(15.dp()).test_tag(item.to_string()));
                    });
                },
            );
        });
        for (tag, start) in [("1", 0.0), ("2", 15.0), ("3", 30.0)] {
            assert_eq!(lazy.main_start(&mut ui, tag), start, "{lazy:?}");
        }
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier,
                remember_lazy_list_state!(),
                no_padding(),
                |scope: LazyListScope| {
                    scope.items_indexed(vec!["1", "2", "3"], |_, index, item: &&str| {
                        Spacer(Modifier.required_size(15.dp()).test_tag(format!("{index}*{item}")));
                    });
                },
            );
        });
        for (tag, start) in [("0*1", 0.0), ("1*2", 15.0), ("2*3", 30.0)] {
            assert_eq!(lazy.main_start(&mut ui, tag), start, "{lazy:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, changeItemsCountAndScrollImmediately
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, changeItemsCountAndScrollImmediately
#[test]
fn a_scroll_right_after_the_count_shrank_sees_the_new_count() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let count = mutable_state_of(100usize);
        let count_handle = count.handle();
        let composed = Rc::new(RefCell::new(Vec::new()));
        let into = composed.clone();
        let mut ui = rule(move || {
            let into = into.clone();
            let count = count_handle.get();
            LazyColumnOrRow(
                lazy,
                lazy.fill_max_cross_axis().then(lazy.main_axis_size(10.dp())),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    let into = into.clone();
                    scope.items(count, move |index| {
                        into.borrow_mut().push(index);
                        Box!(modifier = Modifier.size(20.dp()), {});
                    });
                },
            );
        });
        composed.borrow_mut().clear();
        count.set(10);
        let _ = run(&mut ui, async move { handle.scroll_to_item(50, 0).await });
        assert!(
            composed.borrow().iter().all(|&index| index < 10),
            "{lazy:?}: {:?}",
            composed.borrow()
        );
        assert_eq!(state.first_visible_item_index(), 9, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, overscrollingBackwardFromNotTheFirstPosition
#[test]
fn a_drag_back_past_the_start_stops_at_the_first_item() {
    for lazy in orientations() {
        let mut ui = rule(move || {
            Box!(modifier = Modifier.test_tag("container").size(50.dp()), {
                LazyColumnOrRow(
                    lazy,
                    Modifier.test_tag(LIST),
                    remember_lazy_list_state!(2, 5),
                    no_padding(),
                    move |scope: LazyListScope| {
                        scope.items(100, move |index| {
                            Box!(
                                modifier = lazy
                                    .fill_max_cross_axis()
                                    .then(lazy.main_axis_size(10.dp()))
                                    .test_tag(index.to_string()),
                                {}
                            );
                        });
                    },
                );
            });
        });
        let along = |amount: f32| {
            if lazy.vertical {
                compose_ui::Offset::new(0.0, amount)
            } else {
                compose_ui::Offset::new(amount, 0.0)
            }
        };
        // One drag (not a swipe's several) from the center, overscrolling backward.
        let center = ui.on_node_with_tag(LIST).fetch_semantics_node().center();
        let mut finger = Touch::down(&mut ui, center);
        finger.move_by(&mut ui, along(-8.0));
        finger.move_by(&mut ui, along(150.0));
        finger.up(&mut ui);
        ui.wait_for_idle();
        assert_eq!(lazy.main_size(&mut ui, LIST), 50.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "0"), 0.0, "{lazy:?}");
        assert_eq!(lazy.main_start(&mut ui, "4"), 40.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, initialScrollPositionIsCorrectWhenItemsAreLoadedAsynchronously
#[test]
fn the_initial_position_waits_for_items_loaded_later() {
    for lazy in orientations() {
        let state = LazyListState::new(2, 10);
        let handle = *state;
        let count = mutable_state_of(0usize);
        let count_handle = count.handle();
        let mut ui = rule(move || {
            let count = count_handle.get();
            LazyColumnOrRow(
                lazy,
                Modifier.fill_max_size(),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items(count, |_| Box!(modifier = Modifier.size(20.dp()), {}));
                },
            );
        });
        count.set(100);
        ui.wait_for_idle();
        let position = (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
        assert_eq!(position, (2, 10), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, animateScrollToItemDoesNotScrollPastItem
#[test]
fn an_animated_scroll_to_an_item_never_goes_past_it() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let target = Rc::new(Cell::new(0usize));
        let reverse = Rc::new(Cell::new(false));
        let (seen_target, seen_reverse) = (target.clone(), reverse.clone());
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.fill_max_size(),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(2500, |_| Box!(modifier = Modifier.size(100.dp()), {}));
                },
            );
            let first = handle.first_visible_item_index();
            if seen_reverse.get() {
                assert!(
                    first >= seen_target.get(),
                    "{lazy:?}: at {first}, past {}",
                    seen_target.get()
                );
            } else {
                assert!(
                    first <= seen_target.get(),
                    "{lazy:?}: at {first}, past {}",
                    seen_target.get()
                );
            }
        });
        for (to, back) in [500, 800, 1500, 1600, 1800]
            .map(|to| (to, false))
            .into_iter()
            .chain([1600, 1500, 800, 500, 0].map(|to| (to, true)))
        {
            target.set(to);
            reverse.set(back);
            let _ = run(&mut ui, async move { handle.animate_scroll_to_item(to, 0).await });
            let position =
                (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
            assert_eq!(position, (to, 0), "{lazy:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, animateScrollToTheLastItemWhenItemsAreLargerThenTheScreen
#[test]
fn an_animated_scroll_reaches_a_last_item_larger_than_the_list() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                lazy.sized(100.dp(), 150.dp()),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(20, |_| Box!(modifier = Modifier.size(150.dp()), {}));
                },
            );
        });
        for start in [0, 5, 12] {
            run(&mut ui, async move {
                let _ = handle.scroll_to_item(start, 0).await;
                let _ = handle.animate_scroll_to_item(19, 0).await;
            });
            let position =
                (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
            assert_eq!(position, (19, 0), "{lazy:?} from {start}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, maxIntElements
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, maxIntElements_withKey_startInMiddle
#[test]
fn a_list_of_i32_max_items_works() {
    const MAX: usize = i32::MAX as usize;
    for lazy in orientations() {
        let state = LazyListState::new(MAX - 3, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(45.dp()),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(MAX, |index| {
                        Box!(modifier = Modifier.size(15.dp()).test_tag(index.to_string()), {})
                    });
                },
            );
        });
        for (index, start) in [(MAX - 3, 0.0), (MAX - 2, 15.0), (MAX - 1, 30.0)] {
            assert_eq!(lazy.main_start(&mut ui, &index.to_string()), start, "{lazy:?}");
        }
        ui.on_node_with_tag(&MAX.to_string()).assert_does_not_exist();
        ui.on_node_with_tag("0").assert_does_not_exist();

        let state = LazyListState::new(MAX / 2, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.required_size(15.dp()),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items_keyed(
                        MAX,
                        |index| index,
                        |index| {
                            Box!(modifier = Modifier.size(15.dp()).test_tag(index.to_string()), {});
                        },
                    );
                },
            );
        });
        assert_eq!(lazy.main_start(&mut ui, &(MAX / 2).to_string()), 0.0, "{lazy:?}");
    }
}

/// Five 30 px items in a 90 px list, as Compose's user scrolling tests.
fn five_thirty_px_items(lazy: Lazy, state: LazyListState) -> ComposeTestRule {
    rule(move || {
        LazyColumnOrRow(
            lazy,
            lazy.main_axis_size(90.dp()).test_tag(LIST),
            state,
            no_padding(),
            |scope: LazyListScope| {
                scope.items(5, |index| Spacer(Modifier.size(30.dp()).test_tag(index.to_string())));
            },
        );
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scrollingByExactlyTheItemSize_switchesTheFirstVisibleItem
#[test]
fn scrolling_by_exactly_an_item_makes_the_next_one_first() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let mut ui = five_thirty_px_items(lazy, *state);
        scroll_state_by(&mut ui, *state, 30.0);
        assert!(!is_displayed(&mut ui, "0", LIST), "{lazy:?}");
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            (1, 0)
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, pointerInputScrollingIsAllowedWhenUserScrollingIsEnabled
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, pointerInputScrollingIsDisallowedWhenUserScrollingIsDisabled
#[test]
fn a_drag_scrolls_only_with_user_scrolling_on() {
    for lazy in orientations() {
        for disabled in [false, true] {
            let lazy = Lazy { user_scroll_disabled: disabled, ..lazy };
            let state = LazyListState::new(0, 0);
            let mut ui = five_thirty_px_items(lazy, *state);
            lazy.drag_main_axis_by(&mut ui, LIST, 30.0);
            let expected = if disabled { 30.0 } else { 0.0 };
            assert_eq!(lazy.main_start(&mut ui, "1"), expected, "{lazy:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, programmaticScrollingIsAllowedWhenUserScrollingIsDisabled
#[test]
fn the_state_scrolls_with_user_scrolling_off() {
    for lazy in orientations() {
        let lazy = Lazy { user_scroll_disabled: true, ..lazy };
        let state = LazyListState::new(0, 0);
        let mut ui = five_thirty_px_items(lazy, *state);
        scroll_state_by(&mut ui, *state, 30.0);
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, semanticScrollingIsDisallowedWhenUserScrollingIsDisabled
#[test]
fn semantics_scrolling_is_not_offered_with_user_scrolling_off() {
    for lazy in orientations() {
        let lazy = Lazy { user_scroll_disabled: true, ..lazy };
        let state = LazyListState::new(0, 0);
        let mut ui = five_thirty_px_items(lazy, *state);
        let node = ui.on_node_with_tag(LIST).fetch_semantics_node();
        let action = compose_ui::SemanticsAction::ScrollBy { x: 30.0, y: 30.0 };
        assert!(!ui.host().perform_semantics_action(node.id, action), "{lazy:?}");
        // But the read-only scroll range stays.
        let range = if lazy.vertical {
            node.config.vertical_scroll_axis_range
        } else {
            node.config.horizontal_scroll_axis_range
        };
        assert!(range.is_some(), "{lazy:?}");
        assert_eq!(state.first_visible_item_index(), 0);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, withMissingItems
#[test]
fn an_item_emitting_nothing_takes_no_room() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                lazy.main_axis_size(31.dp()).test_tag(LIST),
                handle,
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(4, |index| {
                        if index != 1 {
                            Box!(modifier = Modifier.size(30.dp()).test_tag(index.to_string()), {});
                        }
                    });
                },
            );
        });
        assert!(is_displayed(&mut ui, "0", LIST) && is_displayed(&mut ui, "2", LIST), "{lazy:?}");
        let _ = run(&mut ui, async move { handle.scroll_to_item(1, 0).await });
        assert!(!is_displayed(&mut ui, "0", LIST), "{lazy:?}");
        assert!(is_displayed(&mut ui, "2", LIST) && is_displayed(&mut ui, "3", LIST), "{lazy:?}");
    }
}

/// Sums what the scrolls inside it consumed.
struct Consumed(Cell<compose_ui::Offset>);

impl compose_ui::NestedScrollConnection for Consumed {
    fn on_post_scroll(
        &self,
        consumed: compose_ui::Offset,
        _available: compose_ui::Offset,
        _source: compose_ui::NestedScrollSource,
    ) -> compose_ui::Offset {
        self.0.set(self.0.get() + consumed);
        compose_ui::Offset::ZERO
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, withZeroSizedFirstItem_shouldNotConsumedDrag
#[test]
fn a_zero_sized_first_item_does_not_consume_a_drag_back() {
    for lazy in orientations() {
        let connection = Rc::new(Consumed(Cell::new(compose_ui::Offset::ZERO)));
        let parent: Rc<dyn compose_ui::NestedScrollConnection> = connection.clone();
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.test_tag("mainList").nested_scroll(parent.clone()),
                remember_lazy_list_state!(),
                PaddingValues::all(10.dp()),
                |scope: LazyListScope| {
                    scope.item(|| Spacer(Modifier.size(0.dp())));
                    scope.items(10, |_| {
                        Box!(modifier = Modifier.fill_max_width().height(20.dp()), {})
                    });
                },
            );
        });
        // Compose's `swipeDown()`: from the top to the bottom, in 200 ms.
        let bounds = crate::lazy_support::bounds(&mut ui, "mainList");
        let x = (bounds.left + bounds.right) / 2.0;
        let start = compose_ui::Offset::new(x, bounds.top);
        let end = compose_ui::Offset::new(x, bounds.bottom);
        crate::scroll_support::swipe(&mut ui, start, end, 200);
        ui.wait_for_idle();
        assert_eq!(connection.0.get(), compose_ui::Offset::ZERO, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, withZeroSizedFirstItem_shouldKeepItemOnSizeChange
#[test]
fn a_zero_sized_first_item_shows_when_it_grows() {
    for lazy in orientations() {
        let size = mutable_state_of(0.dp());
        let handle = size.handle();
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.test_tag("mainList"),
                remember_lazy_list_state!(),
                PaddingValues::all(10.dp()),
                move |scope: LazyListScope| {
                    scope.item(move || Spacer(Modifier.test_tag("firstItem").size(handle.get())));
                    scope.items(10, |_| {
                        Box!(modifier = Modifier.fill_max_width().height(20.dp()), {})
                    });
                },
            );
        });
        assert!(!is_displayed(&mut ui, "firstItem", "mainList"), "{lazy:?}");
        size.set(20.dp());
        ui.wait_for_idle();
        assert!(is_displayed(&mut ui, "firstItem", "mainList"), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scrollingToItemDoesntCauseLazyLayoutRecomposition
#[test]
fn scrolling_to_an_item_does_not_recompose_the_list() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let composed = Rc::new(Cell::new(0));
        let into = composed.clone();
        let mut ui = rule(move || {
            into.set(into.get() + 1);
            LazyColumnOrRow(lazy, Modifier, handle, no_padding(), |scope: LazyListScope| {
                scope.items(1000, |_| Spacer(Modifier.size(10.dp())));
            });
        });
        assert_eq!(composed.get(), 1);
        let _ = run(&mut ui, async move { handle.scroll_to_item(100, 0).await });
        assert_eq!(composed.get(), 1, "{lazy:?}");
        assert_eq!(state.first_visible_item_index(), 100);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scrollingDoesntCauseItemRecomposition
#[test]
fn scrolling_composes_each_item_once() {
    for lazy in orientations() {
        let compositions: Rc<RefCell<std::collections::HashMap<usize, u32>>> = Rc::default();
        let more_than_once = Rc::new(Cell::new(0));
        let (into, disposed) = (compositions.clone(), more_than_once.clone());
        let mut ui = rule(move || {
            let (into, disposed) = (into.clone(), disposed.clone());
            LazyColumnOrRow(
                lazy,
                lazy.main_axis_size(100.dp()).test_tag(LIST),
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    let (into, disposed) = (into.clone(), disposed.clone());
                    scope.items(1000, move |index| {
                        Spacer(Modifier.size(5.dp()));
                        let counted = into.clone();
                        SideEffect(move || *counted.borrow_mut().entry(index).or_insert(0) += 1);
                        let (counts, disposed) = (into.clone(), disposed.clone());
                        DisposableEffect(index, move |scope| {
                            scope.on_dispose(move || {
                                if counts.borrow_mut().remove(&index).is_some_and(|count| count > 1)
                                {
                                    disposed.set(disposed.get() + 1);
                                }
                            })
                        });
                    });
                },
            );
        });
        // Ten items, half the list.
        lazy.scroll_main_axis_by(&mut ui, LIST, 250.0);
        assert_eq!(more_than_once.get(), 0, "{lazy:?}");
        assert!(
            compositions.borrow().values().all(|&count| count == 1),
            "{lazy:?}: {:?}",
            compositions.borrow()
        );
    }
}

/// Twenty 30 px items filling the list across, in a 100 px list, as Compose's scroll-in-effect
/// tests.
fn thirty_px_items(lazy: Lazy, state: LazyListState) {
    LazyColumnOrRow(
        lazy,
        lazy.main_axis_size(100.dp()).then(lazy.fill_max_cross_axis()),
        state,
        no_padding(),
        move |scope: LazyListScope| {
            scope.items(20, move |index| {
                Box!(
                    modifier = lazy
                        .main_axis_size(30.dp())
                        .then(lazy.fill_max_cross_axis())
                        .test_tag(index.to_string()),
                    {}
                );
            });
        },
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scrollDuringMeasure
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scrollInLaunchedEffect
#[test]
fn a_scroll_in_a_launched_effect_applies() {
    for lazy in orientations() {
        for during_measure in [true, false] {
            let mut ui = rule(move || {
                let content = move || {
                    let state = remember_lazy_list_state!();
                    thirty_px_items(lazy, state);
                    LaunchedEffect(state, async move {
                        let _ = state.scroll_to_item(10, 0).await;
                    });
                };
                if during_measure {
                    BoxWithConstraints!(move |_scope: BoxWithConstraintsScope| content());
                } else {
                    content();
                }
            });
            assert_eq!(
                lazy.main_start(&mut ui, "10"),
                0.0,
                "{lazy:?}, in measure: {during_measure}"
            );
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, scrollByOnInitiallyNotAttachedState
#[test]
fn a_scroll_of_a_state_not_yet_shown_applies_when_shown() {
    for lazy in orientations() {
        let shown = mutable_state_of(false);
        let handle = shown.handle();
        let mut ui = rule(move || {
            let state = remember_lazy_list_state!();
            if handle.get() {
                thirty_px_items(lazy, state);
            }
            LaunchedEffect(state, async move {
                let _ = state.scroll_by(30.0).await;
            });
        });
        shown.set(true);
        ui.wait_for_idle();
        assert_eq!(lazy.main_start(&mut ui, "1"), 0.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemsComposedInOrderDuringAnimatedScroll
#[test]
fn an_animated_scroll_composes_items_in_order() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let previous = Rc::new(Cell::new(None::<usize>));
        let seen = previous.clone();
        let mut ui = rule(move || {
            let seen = seen.clone();
            LazyColumnOrRow(
                lazy,
                Modifier.size(30.dp()),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    let seen = seen.clone();
                    scope.items(500, move |index| {
                        if let Some(before) = seen.get() {
                            assert!(index > before, "item {index} composed after {before}");
                        }
                        seen.set(Some(index));
                        Box!(modifier = Modifier.size(10.dp()), {});
                    });
                },
            );
        });
        let _ = run(&mut ui, async move { handle.animate_scroll_to_item(500, 0).await });
        assert_eq!(state.first_visible_item_index(), 497, "{lazy:?}");
    }
}

/// A layout measuring its content with `constraints` (a state), as the `Layout`s of Compose's
/// tests.
fn measured_with(constraints: MutableState<compose_ui::Constraints>, content: impl Fn() + 'static) {
    compose_ui::Layout(
        Modifier,
        compose_ui::measure_policy(move |scope, measurables, _| {
            let constraints = constraints.get();
            let placeable = scope.measure(measurables[0], constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        }),
        callback(content),
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, increasingConstraintsWhenParentMaxSizeIsUsed_correctlyMaintainsThePosition
#[test]
fn growing_constraints_keep_the_position_of_items_filling_the_parent() {
    for lazy in orientations() {
        let state = LazyListState::new(1, 10);
        let handle = *state;
        let constraints = mutable_state_of(compose_ui::Constraints::fixed(100, 100));
        let given = constraints.handle();
        let mut ui = rule(move || {
            measured_with(given, move || {
                LazyColumnOrRow(lazy, Modifier, handle, no_padding(), |scope: LazyListScope| {
                    let item_scope = scope.item_scope();
                    scope.items(3, move |_| {
                        Box!(modifier = Modifier.fill_parent_max_size(&item_scope), {})
                    });
                });
            });
        });
        constraints.set(compose_ui::Constraints::fixed(500, 500));
        ui.wait_for_idle();
        let position = (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
        assert_eq!(position, (1, 10), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, usingFillParentMaxSizeOnInfinityConstraintsIsIgnored
#[test]
fn filling_an_unbounded_parent_is_ignored() {
    for lazy in orientations() {
        let infinite = compose_ui::Constraints::INFINITY;
        let constraints = if lazy.vertical {
            compose_ui::Constraints::new(0, infinite, 0, 100)
        } else {
            compose_ui::Constraints::new(0, 100, 0, infinite)
        };
        let constraints = mutable_state_of(constraints);
        let given = constraints.handle();
        let mut ui = rule(move || {
            measured_with(given, move || {
                LazyColumnOrRow(
                    lazy,
                    Modifier,
                    remember_lazy_list_state!(),
                    no_padding(),
                    |scope: LazyListScope| {
                        let item_scope = scope.item_scope();
                        scope.items(1, move |_| {
                            Box!(
                                modifier = Modifier
                                    .fill_parent_max_size_with(&item_scope, 0.95)
                                    .test_tag("item"),
                                {}
                            );
                        });
                    },
                );
            });
        });
        assert_eq!(lazy.main_size(&mut ui, "item"), 95.0, "{lazy:?}");
        assert_eq!(lazy.cross_size(&mut ui, "item"), 0.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, fillingFullSize_nextItemIsNotComposed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, fillingFullSize_crossAxisSizeOfVisibleItemIsUsed
#[test]
fn an_item_filling_the_list_is_alone_in_it() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                lazy.main_axis_size(5.dp()).test_tag(LIST),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    let item_scope = scope.item_scope();
                    scope.items(5, move |index| {
                        Box!(
                            modifier = lazy
                                .fill_parent_max_main_axis(&item_scope)
                                .then(lazy.cross_axis_size((index as f32).dp()))
                                .test_tag(index.to_string()),
                            {}
                        );
                    });
                },
            );
        });
        for index in 0..5 {
            ui.on_node_with_tag(&index.to_string()).assert_exists();
            ui.on_node_with_tag(&(index + 1).to_string()).assert_does_not_exist();
            assert_eq!(lazy.cross_size(&mut ui, LIST), index as f32, "{lazy:?}");
            scroll_state_by(&mut ui, handle, 5.0);
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, layoutModifierIsNotRepeated
#[test]
fn a_layout_modifier_on_the_list_applies_once() {
    for lazy in orientations() {
        let enabled = mutable_state_of(true);
        let handle = enabled.handle();
        let mut ui = rule(move || {
            let lazy = Lazy { user_scroll_disabled: !handle.get(), ..lazy };
            let modifier = lazy
                .main_axis_size(90.dp())
                .layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let (x, y) = if lazy.vertical { (0, 30) } else { (30, 0) };
                    scope.place(&placeable, x, y);
                    scope.layout(placeable.width, placeable.height)
                })
                .test_tag(LIST);
            LazyColumnOrRow(
                lazy,
                modifier,
                remember_lazy_list_state!(),
                no_padding(),
                |scope: LazyListScope| {
                    scope.items(5, |index| {
                        Spacer(Modifier.size(30.dp()).test_tag(index.to_string()))
                    });
                },
            );
        });
        assert_eq!(lazy.main_start(&mut ui, LIST), 30.0, "{lazy:?}");
        enabled.set(false);
        ui.wait_for_idle();
        assert_eq!(lazy.main_start(&mut ui, LIST), 30.0, "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, recreatingContentLambdaTriggersItemRecomposition
#[test]
fn a_new_content_lambda_recomposes_the_items() {
    for lazy in orientations() {
        let count = mutable_state_of(0);
        let handle = count.handle();
        let mut ui = rule(move || {
            let count = handle.get();
            LazyColumnOrRow(
                lazy,
                Modifier,
                remember_lazy_list_state!(),
                no_padding(),
                move |scope: LazyListScope| {
                    scope.item(move || BasicText!(format!("Count {count}")));
                },
            );
        });
        ui.on_node_with_text("Count 0").assert_exists();
        count.set(1);
        ui.wait_for_idle();
        ui.on_node_with_text("Count 1").assert_exists();
    }
}

/// Swipes the node tagged `tag` towards the start of `vertical`'s axis or back, over its
/// length in 200 ms, as Compose's `swipeUp`, `swipeLeft` and their opposites.
fn swipe_along(ui: &mut ComposeTestRule, tag: &str, vertical: bool, forward: bool) {
    let rect = crate::lazy_support::bounds(ui, tag);
    let (center_x, center_y) = ((rect.left + rect.right) / 2.0, (rect.top + rect.bottom) / 2.0);
    let (from, to) = if vertical {
        (
            compose_ui::Offset::new(center_x, rect.bottom - 1.0),
            compose_ui::Offset::new(center_x, rect.top),
        )
    } else {
        (
            compose_ui::Offset::new(rect.right - 1.0, center_y),
            compose_ui::Offset::new(rect.left, center_y),
        )
    };
    let (from, to) = if forward { (from, to) } else { (to, from) };
    crate::scroll_support::swipe(ui, from, to, 200);
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, whenInnerListIsReused_makeSureFlingUsesCorrectStateForCancellation
#[test]
fn a_reused_inner_list_flings_its_own_state() {
    for lazy in orientations() {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let inner = Lazy { vertical: !lazy.vertical, ..lazy };
        let mut ui = rule(move || {
            LazyColumnOrRow(
                lazy,
                Modifier.size(50.dp()),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items(20, move |main| {
                        LazyColumnOrRow(
                            inner,
                            Modifier.test_tag(format!("main={main}")),
                            remember_lazy_list_state!(),
                            no_padding(),
                            move |scope: LazyListScope| {
                                scope.items(100, move |item| {
                                    Box!(
                                        modifier = Modifier
                                            .size(10.dp())
                                            .test_tag(format!("main={main} item={item}")),
                                        {}
                                    );
                                });
                            },
                        );
                    });
                },
            );
        });
        // Item 0 out of view.
        let _ = run(&mut ui, async move { handle.animate_scroll_to_item(3, 0).await });
        assert_eq!(state.first_visible_item_index(), 3, "{lazy:?}");
        for main in 5..=7 {
            let (list, first) = (format!("main={main}"), format!("main={main} item=0"));
            swipe_along(&mut ui, &list, inner.vertical, true);
            assert!(!is_displayed(&mut ui, &first, &list), "{lazy:?}: {list}");
            // And back to the start, the fling going the whole way.
            swipe_along(&mut ui, &list, inner.vertical, false);
            assert!(is_displayed(&mut ui, &first, &list), "{lazy:?}: {list}");
        }
    }
}
