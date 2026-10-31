//! Staggered grids' keys and `request_scroll_to_item`, as Compose's
//! `LazyStaggeredGridCustomKeysTest` and `LazyStaggeredGridRequestScrollTest`, at density 1.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, UnitExt};

use crate::lazy_support::Lazy;
use crate::scroll_support::{launch, run};

const ITEM: i32 = 100;
const COLUMNS: usize = 2;

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

#[composable]
fn Item(tag: String) {
    Spacer(Modifier.test_tag(tag).size(100.dp()));
}

/// `Item(remember { "$id" })`: its tag from when it was composed.
fn remembered(id: i32) {
    let tag = remember(|| Rc::new(id.to_string()));
    Item((*tag).clone());
}

/// `assertItems`: two columns from the top, each `ITEM` high.
fn assert_items(ui: &mut ComposeTestRule, tags: &[&str]) {
    for (index, tag) in tags.iter().enumerate() {
        let bounds = ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds;
        let top = (index / COLUMNS) as i32 * ITEM;
        assert_eq!((bounds.top, bounds.height()), (top as f32, ITEM as f32), "{tag} in {tags:?}");
    }
}

fn visible_keys(state: LazyStaggeredGridState) -> Vec<u64> {
    state.layout_info().visible_items_info.iter().map(|item| item.key).collect()
}

fn keys(ids: impl IntoIterator<Item = i32>) -> Vec<u64> {
    ids.into_iter().map(|id| hash_key(&id)).collect()
}

/// A column grid of `COLUMNS` over `ids`, whatever a test does in its scope.
fn keyed_grid(
    ids: Vec<i32>,
    content: impl Fn(LazyStaggeredGridScope, Vec<i32>) + Clone + 'static,
) -> (ComposeTestRule, compose_runtime::OwnedState<Vec<i32>>) {
    let list = mutable_state_of(ids);
    let read = list.handle();
    let ui = rule(move || {
        let ids = read.get();
        let content = content.clone();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(COLUMNS),
            move |scope: LazyStaggeredGridScope| content(scope, ids.clone())
        );
    });
    (ui, list)
}

fn by_id(scope: LazyStaggeredGridScope, ids: Vec<i32>) {
    scope.items_list_keyed(ids, |id: &i32| *id, |id: &i32| remembered(*id));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt itemsWithKeysAreLaidOutCorrectly
#[test]
fn keyed_items_are_laid_out() {
    let (mut ui, _list) = keyed_grid(vec![0, 1, 2], by_id);
    assert_items(&mut ui, &["0", "1", "2"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt removing_statesAreMoved
#[test]
fn removing_an_item_moves_the_states_of_the_others() {
    let (mut ui, list) = keyed_grid(vec![0, 1, 2], by_id);
    list.set(vec![0, 2]);
    ui.wait_for_idle();
    assert_items(&mut ui, &["0", "2"]);
}

fn reordering(content: impl Fn(LazyStaggeredGridScope, Vec<i32>) + Clone + 'static) {
    let (mut ui, list) = keyed_grid(vec![0, 1, 2], content);
    list.set(vec![0, 2, 1]);
    ui.wait_for_idle();
    assert_items(&mut ui, &["0", "2", "1"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt reordering_statesAreMoved_list
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt reordering_statesAreMoved_array
#[test]
fn reordering_a_list_moves_the_states() {
    reordering(by_id);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt reordering_statesAreMoved_list_indexed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt reordering_statesAreMoved_array_indexed
#[test]
fn reordering_an_indexed_list_moves_the_states() {
    reordering(|scope, ids| {
        scope.items_indexed_keyed(ids, |_, id: &i32| *id, |_, id: &i32| remembered(*id));
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt reordering_statesAreMoved_itemsWithCount
#[test]
fn reordering_items_by_count_moves_the_states() {
    reordering(|scope, ids| {
        let tags = ids.clone();
        scope.items_keyed(ids.len(), move |index| ids[index], move |index| remembered(tags[index]));
    });
}

/// A tag from a counter, `remember { counter++ }`.
fn counted(counter: Rc<Cell<i32>>) {
    let tag = remember(|| {
        let tag = counter.get();
        counter.set(tag + 1);
        Rc::new(tag.to_string())
    });
    Item((*tag).clone());
}

fn with_counter(after: Vec<i32>, tags: &[&str]) {
    let counter = Rc::new(Cell::new(0));
    let (mut ui, list) = keyed_grid(vec![0, 1, 2], move |scope, ids| {
        let counter = counter.clone();
        scope.items_list_keyed(ids, |id: &i32| *id, move |_| counted(counter.clone()));
    });
    list.set(after);
    ui.wait_for_idle();
    assert_items(&mut ui, tags);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt fullyReplacingTheList
#[test]
fn replacing_every_item_starts_new_states() {
    with_counter(vec![3, 4, 5, 6], &["3", "4", "5", "6"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt keepingOneItem
#[test]
fn keeping_one_item_keeps_its_state() {
    with_counter(vec![1], &["1"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt keepingOneItemAndAddingMore
#[test]
fn keeping_one_item_and_adding_more() {
    with_counter(vec![1, 3], &["1", "3"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt mixingKeyedItemsAndNot
#[test]
fn keyed_items_after_one_without_a_key() {
    reordering(|scope, ids| {
        let first = ids[0];
        scope.item(move || Item(first.to_string()));
        scope.items_list_keyed(ids[1..].to_vec(), |id: &i32| *id, |id: &i32| remembered(*id));
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt updatingTheDataSetIsCorrectlyApplied
#[test]
fn a_new_data_set_is_applied() {
    let list = mutable_state_of(Vec::<i32>::new());
    let read = list.handle();
    let mut ui = rule(move || {
        LaunchedEffect((), async move { read.set(vec![4, 1, 3]) });
        let ids = read.get();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(COLUMNS),
            modifier = Modifier.fill_max_size(),
            move |scope: LazyStaggeredGridScope| {
                scope.items_list_keyed(
                    ids.clone(),
                    |id: &i32| *id,
                    |id: &i32| Item(id.to_string()),
                );
            }
        );
    });
    ui.wait_for_idle();
    assert_items(&mut ui, &["4", "1", "3"]);
    list.set(vec![2, 4, 6, 1, 3, 5]);
    ui.wait_for_idle();
    assert_items(&mut ui, &["2", "4", "6", "1", "3", "5"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt reordering_usingMutableStateListOf
#[test]
fn a_move_within_the_list_moves_the_state() {
    let (mut ui, list) = keyed_grid(vec![0, 1, 2], by_id);
    list.set(vec![0, 2, 1]);
    ui.wait_for_idle();
    assert_items(&mut ui, &["0", "2", "1"]);
}

/// A grid in a box of `2.5` items with a state, keyed by value.
fn small_grid(
    lanes: usize,
    ids: Vec<i32>,
    initial: usize,
) -> (ComposeTestRule, LazyStaggeredGridState, compose_runtime::OwnedState<Vec<i32>>) {
    let list = mutable_state_of(ids);
    let read = list.handle();
    let kept: Rc<Cell<Option<LazyStaggeredGridState>>> = Rc::default();
    let keep = kept.clone();
    let ui = rule(move || {
        let state = remember_lazy_staggered_grid_state(initial, 0);
        keep.set(Some(state));
        let ids = read.get();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(lanes),
            modifier = Modifier.size(250.dp()),
            state = state,
            move |scope: LazyStaggeredGridScope| by_id(scope, ids.clone())
        );
    });
    let state = kept.get().expect("composed");
    (ui, state, list)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt keysInLazyListItemInfoAreCorrect
#[test]
fn layout_info_has_the_keys() {
    let (_ui, state, _list) = small_grid(COLUMNS, vec![0, 1, 2], 0);
    assert_eq!(visible_keys(state), keys([0, 1, 2]));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt keysInLazyListItemInfoAreCorrectAfterReordering
#[test]
fn layout_info_has_the_keys_after_a_reorder() {
    let (mut ui, state, list) = small_grid(COLUMNS, vec![0, 1, 2], 0);
    list.set(vec![0, 2, 1]);
    ui.wait_for_idle();
    assert_eq!(visible_keys(state), keys([0, 2, 1]));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt addingItemsBeforeWithoutKeysIsMaintainingTheIndex
#[test]
fn items_added_before_without_keys_keep_the_index() {
    let list = mutable_state_of((10..=15).collect::<Vec<i32>>());
    let read = list.handle();
    let kept: Rc<Cell<Option<LazyStaggeredGridState>>> = Rc::default();
    let keep = kept.clone();
    let mut ui = rule(move || {
        let state = remember_lazy_staggered_grid_state(0, 0);
        keep.set(Some(state));
        let ids = read.get();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(COLUMNS),
            modifier = Modifier.size(250.dp()),
            state = state,
            move |scope: LazyStaggeredGridScope| {
                scope.items_list(ids.clone(), |id: &i32| remembered(*id));
            }
        );
    });
    list.set((0..=15).collect());
    ui.wait_for_idle();
    assert_eq!(kept.get().expect("composed").first_visible_item_index(), 0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt addingItemsBeforeKeepingThisItemFirst
#[test]
fn items_added_before_keep_the_first_item_first() {
    let (mut ui, state, list) = small_grid(COLUMNS, (10..=15).collect(), 0);
    list.set((0..=15).collect());
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 10);
    assert_eq!(visible_keys(state), keys(10..=15));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt addingItemsRightAfterKeepingThisItemFirst
#[test]
fn items_added_right_after_keep_the_first_item_first() {
    let (mut ui, state, list) = small_grid(1, (0..=5).chain(10..=15).collect(), 5);
    list.set((0..=15).collect());
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 5);
    assert_eq!(visible_keys(state), keys(5..=7));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt addingItemsBeforeWhileCurrentItemIsNotInTheBeginning
#[test]
fn items_added_before_a_first_item_further_on() {
    let (mut ui, state, list) = small_grid(COLUMNS, (10..=30).collect(), 10);
    list.set((0..=30).collect());
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 20);
    assert_eq!(visible_keys(state), keys(20..=25));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt removingTheCurrentItemMaintainsTheIndex
#[test]
fn removing_the_first_item_keeps_the_index() {
    let (mut ui, state, list) = small_grid(COLUMNS, (0..=20).collect(), 8);
    list.set((0..=20).filter(|id| *id != 8).collect());
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 8);
    assert_eq!(visible_keys(state), keys(9..=14));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridCustomKeysTest.kt keysLambdaIsCalledOnlyOnce
#[test]
fn the_key_function_is_called_once() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let _ui = rule(move || {
        let counted = counted.clone();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            move |scope: LazyStaggeredGridScope| {
                let counted = counted.clone();
                scope.items_keyed(
                    1,
                    move |_| {
                        counted.set(counted.get() + 1);
                        0
                    },
                    |_| Item("item".to_string()),
                );
            }
        );
    });
    assert_eq!(calls.get(), 1);
}

/// A grid of `Lazy` keyed by value, running `on_compose` in its content, `cells` across.
fn requesting(
    lazy: Lazy,
    cells: StaggeredGridCells,
    size: compose_ui::Dp,
    ids: Vec<i32>,
    on_compose: impl Fn(LazyStaggeredGridState) + 'static,
) -> (ComposeTestRule, LazyStaggeredGridState, compose_runtime::OwnedState<Vec<i32>>) {
    let on_compose = Rc::new(on_compose);
    let list = mutable_state_of(ids);
    let read = list.handle();
    let kept: Rc<Cell<Option<LazyStaggeredGridState>>> = Rc::default();
    let keep = kept.clone();
    let ui = rule(move || {
        let state = remember_lazy_staggered_grid_state(0, 0);
        keep.set(Some(state));
        let ids = read.get();
        let on_compose = on_compose.clone();
        let content = move |scope: LazyStaggeredGridScope| {
            by_id(scope, ids.clone());
            compose_runtime::snapshots::Snapshot::without_read_observation(|| on_compose(state));
        };
        if lazy.vertical {
            LazyVerticalStaggeredGrid!(
                cells.clone(),
                modifier = Modifier.size(size),
                state = state,
                content = content
            );
        } else {
            LazyHorizontalStaggeredGrid!(
                cells.clone(),
                modifier = Modifier.size(size),
                state = state,
                content = content
            );
        }
    });
    let state = kept.get().expect("composed");
    (ui, state, list)
}

fn three() -> StaggeredGridCells {
    StaggeredGridCells::Fixed(3)
}

fn orientations() -> [Lazy; 2] {
    [Lazy::column(), Lazy::row()]
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_withIndex0_itemsPrepended_scrollsToNewFirstItem
#[test]
fn a_request_for_index_0_shows_items_prepended() {
    for lazy in orientations() {
        let (mut ui, state, list) =
            requesting(lazy, three(), 250.dp(), (10..=15).collect(), |state| {
                state.request_scroll_to_item(0, 0)
            });
        list.set((0..=15).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(visible_keys(state), keys(0..=8));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_withIndex0_scrollToEnd_isScrolledToEnd
#[test]
fn a_request_in_the_content_leaves_a_scroll_with_no_new_data_alone() {
    for lazy in orientations() {
        let (mut ui, state, _list) =
            requesting(lazy, three(), 250.dp(), (0..=14).collect(), |state| {
                state.request_scroll_to_item(0, 0)
            });
        let _ = run(&mut ui, async move { state.scroll_by(5000.0).await });
        assert_ne!(state.first_visible_item_index(), 0);
        assert_eq!(visible_keys(state), keys(6..=14));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_withStartOfNewFirstItem_prependedItems_scrollsToStartOfNewFirstItem
#[test]
fn a_request_with_a_negative_offset_shows_the_start_of_a_new_first_item() {
    for lazy in orientations() {
        let list_size = ITEM * 6 / 10;
        let list = mutable_state_of((3..=30).collect::<Vec<i32>>());
        let read = list.handle();
        let kept: Rc<Cell<Option<LazyStaggeredGridState>>> = Rc::default();
        let keep = kept.clone();
        let mut ui = rule(move || {
            let state = remember_lazy_staggered_grid_state(0, 0);
            keep.set(Some(state));
            let ids = read.get();
            let first = ids.first().copied();
            let modifier = Modifier.size(compose_ui::Dp(list_size as f32));
            let cells = StaggeredGridCells::FixedSize(compose_ui::Dp(ITEM as f32));
            let content = move |scope: LazyStaggeredGridScope| by_id(scope, ids.clone());
            if lazy.vertical {
                LazyVerticalStaggeredGrid!(
                    cells,
                    modifier = modifier,
                    state = state,
                    reverse_layout = true,
                    content = content
                );
            } else {
                LazyHorizontalStaggeredGrid!(
                    cells,
                    modifier = modifier,
                    state = state,
                    reverse_layout = true,
                    content = content
                );
            }
            SideEffect(move || {
                let first_key = state.layout_info().visible_items_info.first().map(|item| item.key);
                let changed = first_key != first.map(|first| hash_key(&first));
                if !state.can_scroll_backward() && changed {
                    state.request_scroll_to_item(1, -list_size);
                }
            });
        });
        let state = kept.get().expect("composed");
        let _ = run(&mut ui, async move { state.scroll_to_item(0, 0).await });
        list.set((0..=30).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 0);
        assert_eq!(state.first_visible_item_scroll_offset(), ITEM - list_size);
        assert_eq!(visible_keys(state), keys([0]));
    }
}

/// A grid whose caller's `SideEffect` asks to stay at the first visible index.
fn staying(
    lazy: Lazy,
    ids: Vec<i32>,
    only_at_rest: bool,
) -> (ComposeTestRule, LazyStaggeredGridState, compose_runtime::OwnedState<Vec<i32>>) {
    let list = mutable_state_of(ids);
    let read = list.handle();
    let kept: Rc<Cell<Option<LazyStaggeredGridState>>> = Rc::default();
    let keep = kept.clone();
    let ui = rule(move || {
        let state = remember_lazy_staggered_grid_state(0, 0);
        keep.set(Some(state));
        let ids = read.get();
        let content = move |scope: LazyStaggeredGridScope| by_id(scope, ids.clone());
        if lazy.vertical {
            LazyVerticalStaggeredGrid!(
                three(),
                modifier = Modifier.size(250.dp()),
                state = state,
                content = content
            );
        } else {
            LazyHorizontalStaggeredGrid!(
                three(),
                modifier = Modifier.size(250.dp()),
                state = state,
                content = content
            );
        }
        SideEffect(move || {
            if !only_at_rest || !state.is_scroll_in_progress() {
                state.request_scroll_to_item(state.first_visible_item_index(), 0);
            }
        });
    });
    let state = kept.get().expect("composed");
    (ui, state, list)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_withFirstVisibleIndex_firstVisibleItemMoved_staysScrolledAtSameIndex
#[test]
fn a_request_for_the_first_index_stays_there_when_its_item_moves() {
    for lazy in orientations() {
        let (mut ui, state, list) = staying(lazy, (0..=23).collect(), false);
        let _ = run(&mut ui, async move { state.scroll_to_item(6, 0).await });
        assert_eq!(state.first_visible_item_index(), 6);
        list.set((0..=5).chain(7..=23).chain([6]).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 6);
        assert_eq!(visible_keys(state), keys(7..=15));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_withFirstVisibleIndex_index0ItemRemoved_staysScrolledAtSameIndex
#[test]
fn a_request_for_the_first_index_stays_there_when_the_first_item_goes() {
    for lazy in orientations() {
        let (mut ui, state, list) = staying(lazy, (0..=35).collect(), true);
        let _ = run(&mut ui, async move { state.scroll_to_item(15, 0).await });
        list.set((1..=35).collect());
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 15);
        assert_eq!(visible_keys(state), keys(16..=24));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_firstVisibleItemIndexUpdatesInstantly
#[test]
fn a_request_updates_the_first_index_at_once() {
    for lazy in orientations() {
        let (_ui, state, _list) = requesting(
            lazy,
            StaggeredGridCells::FixedSize(100.dp()),
            250.dp(),
            (0..=30).collect(),
            |_| {},
        );
        state.request_scroll_to_item(30, 0);
        assert_eq!(state.first_visible_item_index(), 30);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_withoutChangingData_scrollsToRequestedIndex
#[test]
fn a_request_measures_again_by_itself() {
    for lazy in orientations() {
        let (mut ui, state, _list) =
            requesting(lazy, three(), 250.dp(), (0..=29).collect(), |_| {});
        state.request_scroll_to_item(0, ITEM * 30);
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), 21);
        assert_eq!(visible_keys(state), keys(21..=29));
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt requestScrollToItem_whileScrolling_cancelsScroll
#[test]
fn a_request_stops_a_scroll_going_on() {
    for lazy in orientations() {
        let (mut ui, state, _list) = requesting(
            lazy,
            StaggeredGridCells::FixedSize(100.dp()),
            100.dp(),
            (0..15).collect(),
            |_| {},
        );
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
            let _ = state
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
        assert!(cancelled.get());
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridRequestScrollTest.kt moveFirstVisibleItem_afterScrollRequestIsProcessed_maintainsScrollByFirstVisibleItemKey
#[test]
fn a_first_item_moving_after_the_request_is_followed_by_its_key() {
    for lazy in orientations() {
        let (mut ui, state, list) =
            requesting(lazy, three(), 250.dp(), (10..=20).collect(), |state| {
                if state.first_visible_item_index() == 0 {
                    state.request_scroll_to_item(0, 0);
                }
            });
        list.set((0..=20).collect());
        ui.wait_for_idle();
        let _ = run(&mut ui, async move { state.scroll_to_item(3, 0).await });
        let moved: Vec<i32> = (0..=2).chain(4..=20).chain([3]).collect();
        let size = moved.len();
        list.set(moved);
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), size - 9);
        assert_eq!(visible_keys(state), keys((13..=20).chain([3])));
    }
}
