//! `request_scroll_to_item`: a jump at the next layout, which forgets the first visible item's
//! key (so items coming before don't move the position), updates the position at once,
//! measures the list again, and stops a scroll going on. Lists and grids.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, IntSize};

use crate::lazy_support::{Lazy, LazyColumnOrRow, LazyGrid, no_padding, px};
use crate::scroll_support::{launch, run};

const ITEM: i32 = 100;

fn visible_keys(state: &LazyListState) -> Vec<u64> {
    state.layout_info().visible_items_info.iter().map(|item| item.key).collect()
}

fn keys(keys: impl IntoIterator<Item = u32>) -> Vec<u64> {
    keys.into_iter().map(|key| hash_key(&key)).collect()
}

/// A list of `items` (keyed by value) in a box 2.5 items long, running `on_compose` in its
/// content each composition (Compose's `Snapshot.withoutReadObservation { … }` there).
fn keyed_list(
    lazy: Lazy,
    state: LazyListState,
    items: MutableState<Vec<u32>>,
    size: Dp,
    on_compose: impl Fn(LazyListState) + 'static,
) -> ComposeTestRule {
    let on_compose = Rc::new(on_compose);
    ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let list = items.get();
        let on_compose = on_compose.clone();
        LazyColumnOrRow(
            lazy,
            Modifier.size(size),
            state,
            no_padding(),
            move |scope: LazyListScope| {
                let list = list.clone();
                scope.items_list_keyed(
                    list,
                    |item: &u32| *item,
                    |_, _: &u32| {
                        Spacer(Modifier.size(px(ITEM)));
                    },
                );
                on_compose(state);
            },
        );
    })
}

fn two_and_a_half() -> Dp {
    px(ITEM * 5 / 2)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_withIndex0_itemsPrepended_scrollsToNewFirstItem
#[test]
fn a_request_for_the_first_item_follows_items_prepended() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let items = mutable_state_of((10..=15).collect::<Vec<u32>>());
        let handle = items.handle();
        let mut ui = keyed_list(lazy, *state, handle, two_and_a_half(), |state| {
            state.request_scroll_to_item(0, 0)
        });
        handle.set((0..=15).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(visible_keys(&state), keys([0, 1, 2]));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_withIndex0_scrollToEnd_isScrolledToEnd
#[test]
fn a_request_kept_while_the_data_stays_lets_a_fling_go_to_the_end() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((0..=15).collect::<Vec<u32>>());
        let mut ui = keyed_list(lazy, handle, items.handle(), two_and_a_half(), |state| {
            state.request_scroll_to_item(0, 0)
        });
        // A quick swipe towards the end, then the fling.
        let (from, to) = if lazy.vertical {
            (compose_ui::Offset::new(125.0, 245.0), compose_ui::Offset::new(125.0, 5.0))
        } else {
            (compose_ui::Offset::new(245.0, 125.0), compose_ui::Offset::new(5.0, 125.0))
        };
        crate::scroll_support::swipe(&mut ui, from, to, 48);
        ui.wait_for_idle();
        assert_ne!(state.first_visible_item_index(), 0);
        assert_eq!(visible_keys(&state), keys([13, 14, 15]));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_withFirstVisibleIndex_index0ItemRemoved_staysScrolledAtSameIndex
#[test]
fn a_request_for_the_same_index_stays_there_when_an_item_before_goes() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((0..=35).collect::<Vec<u32>>());
        let items_handle = items.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            let list = items_handle.get();
            LazyColumnOrRow(
                lazy,
                Modifier.size(two_and_a_half()),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list_keyed(
                        list.clone(),
                        |item: &u32| *item,
                        |_, _: &u32| {
                            Spacer(Modifier.size(px(ITEM)));
                        },
                    );
                },
            );
            SideEffect(move || {
                if !handle.is_scroll_in_progress() {
                    handle.request_scroll_to_item(handle.first_visible_item_index(), 0);
                }
            });
        });
        let _ = run(&mut ui, async move { handle.scroll_to_item(15, 0).await });
        items.set((1..=35).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 15);
        assert_eq!(visible_keys(&state), keys([16, 17, 18]));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_withFirstVisibleIndex_firstVisibleItemMoved_staysScrolledAtSameIndex
#[test]
fn a_request_for_the_same_index_stays_there_when_the_first_item_moves() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((0..=25).collect::<Vec<u32>>());
        let items_handle = items.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            let list = items_handle.get();
            LazyColumnOrRow(
                lazy,
                Modifier.size(two_and_a_half()),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list_keyed(
                        list.clone(),
                        |item: &u32| *item,
                        |_, _: &u32| {
                            Spacer(Modifier.size(px(ITEM)));
                        },
                    );
                },
            );
            SideEffect(move || handle.request_scroll_to_item(handle.first_visible_item_index(), 0));
        });
        let _ = run(&mut ui, async move { handle.scroll_to_item(5, 0).await });
        items.set((0..=4).chain(6..=25).chain([5]).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 5);
        assert_eq!(visible_keys(&state), keys([6, 7, 8]));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_withStartOfNewFirstItem_prependedItems_scrollsToStartOfNewFirstItem
#[test]
fn a_request_with_a_negative_offset_shows_the_start_of_a_new_first_item() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let lazy = Lazy { reverse_layout: true, ..lazy };
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((3..=30).collect::<Vec<u32>>());
        let items_handle = items.handle();
        let list_size = ITEM * 6 / 10;
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            let list = items_handle.get();
            let first = list.first().copied();
            LazyColumnOrRow(
                lazy,
                Modifier.size(px(list_size)),
                handle,
                no_padding(),
                move |scope: LazyListScope| {
                    scope.items_list_keyed(
                        list.clone(),
                        |item: &u32| *item,
                        |_, _: &u32| {
                            Spacer(Modifier.size(px(ITEM)));
                        },
                    );
                },
            );
            SideEffect(move || {
                let first_key =
                    handle.layout_info().visible_items_info.first().map(|item| item.key);
                let changed = first_key != first.map(|first| hash_key(&first));
                if !handle.can_scroll_backward() && changed {
                    // The start of the first item.
                    handle.request_scroll_to_item(1, -list_size);
                }
            });
        });
        let _ = run(&mut ui, async move { handle.scroll_to_item(0, 0).await });
        items.set((0..=30).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), ITEM - list_size);
        assert_eq!(visible_keys(&state), keys([0]));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_firstVisibleItemIndexUpdatesInstantly
#[test]
fn a_request_updates_the_position_at_once() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let items = mutable_state_of((0..=30).collect::<Vec<u32>>());
        let _ui = keyed_list(lazy, *state, items.handle(), two_and_a_half(), |_| {});
        state.request_scroll_to_item(30, 0);
        assert_eq!(state.first_visible_item_index(), 30);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_withoutChangingData_scrollsToRequestedIndex
#[test]
fn a_request_measures_the_list_again_by_itself() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let items = mutable_state_of((0..=30).collect::<Vec<u32>>());
        let mut ui = keyed_list(lazy, *state, items.handle(), two_and_a_half(), |_| {});
        state.request_scroll_to_item(0, ITEM * 30);
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 28);
        assert_eq!(visible_keys(&state), keys([28, 29, 30]));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, requestScrollToItem_whileScrolling_cancelsScroll
#[test]
fn a_request_stops_a_scroll_going_on() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((0..15).collect::<Vec<u32>>());
        let mut ui = keyed_list(lazy, handle, items.handle(), px(ITEM), |_| {});
        let cancelled = Rc::new(Cell::new(false));
        let flag = cancelled.clone();
        launch(async move {
            /// Says it was dropped (the scroll cancelled), as Compose's `finally`.
            struct OnDrop(Rc<Cell<bool>>);
            impl Drop for OnDrop {
                fn drop(&mut self) {
                    self.0.set(true);
                }
            }
            let _ = handle
                .scroll(MutatePriority::Default, move |_| async move {
                    let _guard = OnDrop(flag);
                    std::future::pending::<()>().await;
                })
                .await;
        });
        ui.wait_for_idle();
        assert!(state.is_scroll_in_progress());
        state.request_scroll_to_item(0, 0);
        ui.wait_for_idle();
        assert!(cancelled.get(), "the scroll was stopped");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyListRequestScrollTest.kt, moveFirstVisibleItem_afterScrollRequestIsProcessed_maintainsScrollByFirstVisibleItemKey
#[test]
fn after_a_request_the_position_follows_the_first_items_key_again() {
    for lazy in [Lazy::column(), Lazy::row()] {
        let state = LazyListState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((10..=20).collect::<Vec<u32>>());
        let items_handle = items.handle();
        let mut ui = keyed_list(lazy, handle, items_handle, two_and_a_half(), |state| {
            if state.first_visible_item_index() == 0 {
                state.request_scroll_to_item(0, 0);
            }
        });
        items.set((0..=20).collect());
        ui.wait_for_idle();
        let _ = run(&mut ui, async move { handle.scroll_to_item(1, 0).await });
        items.set([0].into_iter().chain(2..=20).chain([1]).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 21 - 3);
        assert_eq!(visible_keys(&state), keys([19, 20, 1]));
    }
}

/// A grid of `items` (keyed by value) in two cells, 2.5 lines long.
fn keyed_grid(
    lazy: Lazy,
    state: LazyGridState,
    items: MutableState<Vec<u32>>,
    on_compose: impl Fn(LazyGridState) + 'static,
) -> ComposeTestRule {
    let on_compose = Rc::new(on_compose);
    ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let list = items.get();
        let on_compose = on_compose.clone();
        LazyGrid(
            lazy,
            2,
            lazy.sized(two_and_a_half(), px(ITEM * 2)),
            state,
            no_padding(),
            move |scope: LazyGridScope| {
                scope.items_list_keyed(
                    list.clone(),
                    |item: &u32| *item,
                    |_: &u32| {
                        Spacer(Modifier.size(px(ITEM)));
                    },
                );
                on_compose(state);
            },
        );
    })
}

fn grid_keys(state: &LazyGridState) -> Vec<u64> {
    state.layout_info().visible_items_info.iter().map(|item| item.key).collect()
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, requestScrollToItem_withIndex0_itemsPrepended_scrollsToNewFirstItem
#[test]
fn a_grid_request_for_the_first_item_follows_items_prepended() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let items = mutable_state_of((10..=25).collect::<Vec<u32>>());
        let handle = items.handle();
        let mut ui = keyed_grid(lazy, *state, handle, |state| state.request_scroll_to_item(0, 0));
        handle.set((0..=25).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(grid_keys(&state), keys(0..6));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, requestScrollToItem_firstVisibleItemIndexUpdatesInstantly
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, requestScrollToItem_withoutChangingData_scrollsToRequestedIndex
#[test]
fn a_grid_request_updates_the_position_at_once_and_lays_out_again() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let items = mutable_state_of((0..60).collect::<Vec<u32>>());
        let mut ui = keyed_grid(lazy, *state, items.handle(), |_| {});
        state.request_scroll_to_item(30, 0);
        assert_eq!(state.first_visible_item_index(), 30);
        ui.wait_for_idle();
        state.request_scroll_to_item(0, ITEM * 30);
        ui.wait_for_idle();
        // At the end: the last three lines.
        assert_eq!(state.first_visible_item_index(), 54);
        assert_eq!(grid_keys(&state), keys(54..60));
    }
}

/// A grid of `items` (keyed by value) in three cells, 2.5 items square, as Compose's grid
/// request tests: `on_compose` runs in its content each composition, `after` after it (a
/// `SideEffect` there).
fn keyed_grid_of_three(
    lazy: Lazy,
    state: LazyGridState,
    items: MutableState<Vec<u32>>,
    on_compose: impl Fn(LazyGridState) + 'static,
    after: impl Fn(LazyGridState) + 'static,
) -> ComposeTestRule {
    let (on_compose, after) = (Rc::new(on_compose), Rc::new(after));
    ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let list = items.get();
        let on_compose = on_compose.clone();
        LazyGrid(
            lazy,
            3,
            Modifier.size(two_and_a_half()),
            state,
            no_padding(),
            move |scope: LazyGridScope| {
                scope.items_list_keyed(
                    list.clone(),
                    |item: &u32| *item,
                    |_: &u32| Spacer(Modifier.size(px(ITEM))),
                );
                on_compose(state);
            },
        );
        let after = after.clone();
        SideEffect(move || after(state));
    })
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, requestScrollToItem_withIndex0_scrollToEnd_isScrolledToEnd
#[test]
fn a_grid_request_kept_while_the_data_stays_lets_a_fling_go_to_the_end() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let items = mutable_state_of((0..=14).collect::<Vec<u32>>());
        let mut ui = keyed_grid_of_three(
            lazy,
            *state,
            items.handle(),
            |state| state.request_scroll_to_item(0, 0),
            |_| {},
        );
        // A quick swipe towards the end, then the fling.
        let (from, to) = if lazy.vertical {
            (compose_ui::Offset::new(125.0, 245.0), compose_ui::Offset::new(125.0, 5.0))
        } else {
            (compose_ui::Offset::new(245.0, 125.0), compose_ui::Offset::new(5.0, 125.0))
        };
        crate::scroll_support::swipe(&mut ui, from, to, 48);
        ui.wait_for_idle();
        assert_ne!(state.first_visible_item_index(), 0, "{lazy:?}");
        assert_eq!(grid_keys(&state), keys(6..=14), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, requestScrollToItem_withFirstVisibleIndex_firstVisibleItemMoved_staysScrolledAtSameIndex
#[test]
fn a_grid_request_for_the_same_index_stays_there_when_the_first_item_moves() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((0..=23).collect::<Vec<u32>>());
        let mut ui = keyed_grid_of_three(
            lazy,
            handle,
            items.handle(),
            |_| {},
            |state| state.request_scroll_to_item(state.first_visible_item_index(), 0),
        );
        let _ = run(&mut ui, async move { handle.scroll_to_item(6, 0).await });
        assert_eq!(state.first_visible_item_index(), 6, "{lazy:?}");
        items.set((0..=5).chain(7..=23).chain([6]).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 6, "{lazy:?}");
        assert_eq!(grid_keys(&state), keys(7..=15), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, requestScrollToItem_withFirstVisibleIndex_index0ItemRemoved_staysScrolledAtSameIndex
#[test]
fn a_grid_request_for_the_same_index_stays_there_when_an_item_before_goes() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((0..=35).collect::<Vec<u32>>());
        let mut ui = keyed_grid_of_three(
            lazy,
            handle,
            items.handle(),
            |_| {},
            |state| {
                if !state.is_scroll_in_progress() {
                    state.request_scroll_to_item(state.first_visible_item_index(), 0);
                }
            },
        );
        let _ = run(&mut ui, async move { handle.scroll_to_item(15, 0).await });
        items.set((1..=35).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 15, "{lazy:?}");
        assert_eq!(grid_keys(&state), keys(16..=24), "{lazy:?}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, requestScrollToItem_whileScrolling_cancelsScroll
#[test]
fn a_grid_request_stops_a_scroll_going_on() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((0..15).collect::<Vec<u32>>());
        let items_handle = items.handle();
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            let list = items_handle.get();
            LazyGrid(
                lazy,
                1,
                Modifier.size(px(ITEM)),
                handle,
                no_padding(),
                move |scope: LazyGridScope| {
                    scope.items_list_keyed(
                        list.clone(),
                        |item: &u32| *item,
                        |_: &u32| Spacer(Modifier.size(px(ITEM))),
                    );
                },
            );
        });
        let cancelled = Rc::new(Cell::new(false));
        let flag = cancelled.clone();
        launch(async move {
            /// Says it was dropped (the scroll cancelled), as Compose's `finally`.
            struct OnDrop(Rc<Cell<bool>>);
            impl Drop for OnDrop {
                fn drop(&mut self) {
                    self.0.set(true);
                }
            }
            let _ = handle
                .scroll(MutatePriority::Default, move |_| async move {
                    let _guard = OnDrop(flag);
                    std::future::pending::<()>().await;
                })
                .await;
        });
        ui.wait_for_idle();
        assert!(state.is_scroll_in_progress(), "{lazy:?}");
        state.request_scroll_to_item(0, 0);
        ui.wait_for_idle();
        assert!(cancelled.get(), "{lazy:?}: the scroll was stopped");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazyGridRequestScrollTest.kt, moveFirstVisibleItem_afterScrollRequestIsProcessed_maintainsScrollByFirstVisibleItemKey
#[test]
fn after_a_grid_request_the_position_follows_the_first_items_key_again() {
    for lazy in Lazy::grids() {
        let state = LazyGridState::new(0, 0);
        let handle = *state;
        let items = mutable_state_of((10..=20).collect::<Vec<u32>>());
        let mut ui = keyed_grid_of_three(
            lazy,
            handle,
            items.handle(),
            |state| {
                if state.first_visible_item_index() == 0 {
                    state.request_scroll_to_item(0, 0);
                }
            },
            |_| {},
        );
        items.set((0..=20).collect());
        ui.wait_for_idle();
        let _ = run(&mut ui, async move { handle.scroll_to_item(3, 0).await });
        items.set([0, 1, 2].into_iter().chain(4..=20).chain([3]).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 21 - 9, "{lazy:?}");
        assert_eq!(grid_keys(&state), keys((13..=20).chain([3])), "{lazy:?}");
    }
}
