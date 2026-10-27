//! Keyed lazy list items, as Compose's `LazyCustomKeysTest`: laid out, their state moving with
//! them through removals, reorders and replacements, keys in the layout info, the position
//! following the first item's key, the key function called once. Density 1, so Compose's dps
//! are pixels (its items are 100 px).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, UnitExt};

const ITEM_SIZE: f32 = 100.0;

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

#[composable]
fn Item(tag: String) {
    Spacer(Modifier.test_tag(tag).size(ITEM_SIZE.dp()));
}

/// Compose's `assertItems`: the items tagged `tags` one under the other from the top, each
/// 100 px high.
fn assert_items(ui: &mut ComposeTestRule, tags: &[&str]) {
    for (index, tag) in tags.iter().enumerate() {
        let bounds = ui.on_node_with_tag(tag).fetch_semantics_node().bounds;
        assert_eq!(
            (bounds.top, bounds.height()),
            (ITEM_SIZE * index as f32, ITEM_SIZE),
            "{tag} in {tags:?}"
        );
    }
}

/// The keys of the items in view, as Compose's tests' `visibleKeys` (hashed, D170).
fn visible_keys(state: LazyListState) -> Vec<u64> {
    state.layout_info().visible_items_info.iter().map(|item| item.key).collect()
}

fn keys(ids: &[i32]) -> Vec<u64> {
    ids.iter().map(hash_key).collect()
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, itemsWithKeysAreLaidOutCorrectly
#[test]
fn keyed_items_are_laid_out() {
    let mut ui = rule(|| {
        LazyColumn!(|scope: LazyListScope| {
            scope.items_list_keyed(
                vec![0, 1, 2],
                |id: &i32| *id,
                |_, id: &i32| Item(id.to_string()),
            );
        });
    });
    assert_items(&mut ui, &["0", "1", "2"]);
}

/// A list of `ids` the test changes, its items remembering their tags.
fn remembering(ids: Vec<i32>) -> (ComposeTestRule, OwnedState<Vec<i32>>) {
    let list = mutable_state_of(ids);
    let handle = list.handle();
    let ui = rule(move || {
        let ids = handle.get();
        LazyColumn!(move |scope: LazyListScope| {
            scope.items_list_keyed(
                ids.clone(),
                |id: &i32| *id,
                |_, id: &i32| {
                    let id = *id;
                    Item(remember(move || id.to_string()));
                },
            );
        });
    });
    (ui, list)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, removing_statesAreMoved
#[test]
fn a_removal_moves_the_states_after_it() {
    let (mut ui, list) = remembering(vec![0, 1, 2]);
    list.set(vec![0, 2]);
    ui.wait_for_idle();
    assert_items(&mut ui, &["0", "2"]);
}

/// Compose's `testReordering`: three items, the last two swapped, the states moving with them.
fn reordering(content: impl Fn(LazyListScope, Rc<Vec<i32>>) + 'static) {
    let list = mutable_state_of(Rc::new(vec![0, 1, 2]));
    let handle = list.handle();
    let content = Rc::new(content);
    let mut ui = rule(move || {
        let ids = handle.get();
        let content = content.clone();
        LazyColumn!(move |scope: LazyListScope| content(scope, ids.clone()));
    });
    list.set(Rc::new(vec![0, 2, 1]));
    ui.wait_for_idle();
    assert_items(&mut ui, &["0", "2", "1"]);
}

fn remembered_item(id: i32) {
    Item(remember(move || id.to_string()));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, reordering_statesAreMoved_list
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, reordering_statesAreMoved_array
#[test]
fn a_reorder_moves_the_states_of_a_list() {
    reordering(|scope, ids| {
        scope.items_list_keyed((*ids).clone(), |id: &i32| *id, |_, id: &i32| remembered_item(*id));
    });
    reordering(|scope, ids| {
        let array: [i32; 3] = [ids[0], ids[1], ids[2]];
        scope.items_list_keyed(array, |id: &i32| *id, |_, id: &i32| remembered_item(*id));
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, reordering_statesAreMoved_list_indexed
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, reordering_statesAreMoved_array_indexed
#[test]
fn a_reorder_moves_the_states_of_an_indexed_list() {
    reordering(|scope, ids| {
        scope.items_indexed_keyed(
            (*ids).clone(),
            |_, id: &i32| *id,
            |_, _, id: &i32| remembered_item(*id),
        );
    });
    reordering(|scope, ids| {
        let array: [i32; 3] = [ids[0], ids[1], ids[2]];
        scope.items_indexed_keyed(array, |_, id: &i32| *id, |_, _, id: &i32| remembered_item(*id));
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, reordering_statesAreMoved_itemsWithCount
#[test]
fn a_reorder_moves_the_states_of_counted_items() {
    reordering(|scope, ids| {
        let keys = ids.clone();
        scope.items_keyed(
            ids.len(),
            move |index| keys[index],
            move |index| remembered_item(ids[index]),
        );
    });
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, mixingKeyedItemsAndNot
#[test]
fn a_reorder_moves_the_states_after_an_unkeyed_item() {
    reordering(|scope, ids| {
        let first = ids[0];
        scope.item(move || Item(first.to_string()));
        scope.items_list_keyed(
            ids[1..].to_vec(),
            |id: &i32| *id,
            |_, id: &i32| remembered_item(*id),
        );
    });
}

/// A list of `ids` whose items remember a count of the compositions so far as their tags.
fn counting(ids: Vec<i32>) -> (ComposeTestRule, OwnedState<Vec<i32>>) {
    let list = mutable_state_of(ids);
    let handle = list.handle();
    let counter = Rc::new(Cell::new(0));
    let ui = rule(move || {
        let (ids, counter) = (handle.get(), counter.clone());
        LazyColumn!(move |scope: LazyListScope| {
            let counter = counter.clone();
            scope.items_list_keyed(
                ids.clone(),
                |id: &i32| *id,
                move |_, _: &i32| {
                    let counter = counter.clone();
                    Item(remember(move || counter.replace(counter.get() + 1).to_string()));
                },
            );
        });
    });
    (ui, list)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, fullyReplacingTheList
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, keepingOneItem
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, keepingOneItemAndAddingMore
#[test]
fn replaced_items_get_new_states_and_kept_ones_keep_theirs() {
    for (after, expected) in [
        (vec![3, 4, 5, 6], vec!["3", "4", "5", "6"]),
        (vec![1], vec!["1"]),
        (vec![1, 3], vec!["1", "3"]),
    ] {
        let (mut ui, list) = counting(vec![0, 1, 2]);
        list.set(after);
        ui.wait_for_idle();
        assert_items(&mut ui, &expected);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, updatingTheDataSetIsCorrectlyApplied
#[test]
fn data_set_from_an_effect_and_then_changed_is_applied() {
    let list = mutable_state_of(Vec::<i32>::new());
    let handle = list.handle();
    let mut ui = rule(move || {
        LaunchedEffect((), async move { handle.set(vec![4, 1, 3]) });
        let ids = handle.get();
        LazyColumn!(modifier = Modifier.fill_max_size(), move |scope: LazyListScope| {
            scope.items_list_keyed(ids.clone(), |id: &i32| *id, |_, id: &i32| Item(id.to_string()));
        });
    });
    assert_items(&mut ui, &["4", "1", "3"]);
    list.set(vec![2, 4, 6, 1, 3, 5]);
    ui.wait_for_idle();
    assert_items(&mut ui, &["2", "4", "6", "1", "3", "5"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, reordering_usingMutableStateListOf
#[test]
fn a_reorder_of_a_state_list_moves_the_states() {
    let list = mutable_state_list_of::<i32>();
    list.set(vec![0, 1, 2]);
    let handle = list.handle();
    let mut ui = rule(move || {
        let ids = handle.get();
        LazyColumn!(move |scope: LazyListScope| {
            scope.items_list_keyed(ids.clone(), |id: &i32| *id, |_, id: &i32| remembered_item(*id));
        });
    });
    list.update(|ids| {
        let mut ids = ids.clone();
        let moved = ids.remove(1);
        ids.push(moved);
        ids
    });
    ui.wait_for_idle();
    assert_items(&mut ui, &["0", "2", "1"]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, keysInLazyListItemInfoAreCorrect
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, keysInLazyListItemInfoAreCorrectAfterReordering
#[test]
fn the_layout_info_has_the_items_keys() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let list = mutable_state_of(vec![0, 1, 2]);
    let ids = list.handle();
    let mut ui = rule(move || {
        let ids = ids.get();
        LazyColumn!(state = handle, move |scope: LazyListScope| {
            scope.items_list_keyed(ids.clone(), |id: &i32| *id, |_, id: &i32| remembered_item(*id));
        });
    });
    assert_eq!(visible_keys(*state), keys(&[0, 1, 2]));
    list.set(vec![0, 2, 1]);
    ui.wait_for_idle();
    assert_eq!(visible_keys(*state), keys(&[0, 2, 1]));
}

/// Items of `ids` in a list of 250 px, keyed or not, from `first`.
fn from(
    first: usize,
    ids: Vec<i32>,
    keyed: bool,
) -> (ComposeTestRule, Owned<LazyListState>, OwnedState<Vec<i32>>) {
    let state = LazyListState::new(first, 0);
    let handle = *state;
    let list = mutable_state_of(ids);
    let ids = list.handle();
    let ui = rule(move || {
        let ids = ids.get();
        LazyColumn!(
            modifier = Modifier.size((ITEM_SIZE * 2.5).dp()),
            state = handle,
            move |scope: LazyListScope| {
                if keyed {
                    scope.items_list_keyed(
                        ids.clone(),
                        |id: &i32| *id,
                        |_, id: &i32| remembered_item(*id),
                    );
                } else {
                    scope.items_list(ids.clone(), |_, id: &i32| remembered_item(*id));
                }
            }
        );
    });
    (ui, state, list)
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, addingItemsBeforeWithoutKeysIsMaintainingTheIndex
#[test]
fn items_added_before_unkeyed_ones_keep_the_index() {
    let (mut ui, state, list) = from(0, (10..=15).collect(), false);
    list.set((0..=15).collect());
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, addingItemsBeforeKeepingThisItemFirst
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, addingItemsRightAfterKeepingThisItemFirst
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, addingItemsBeforeWhileCurrentItemIsNotInTheBeginning
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, removingTheCurrentItemMaintainsTheIndex
/// The first visible index, the items before and after, the first index and keys expected.
type FollowCase = (usize, Vec<i32>, Vec<i32>, usize, [i32; 3]);

#[test]
fn the_first_item_is_followed_by_key() {
    let cases: [FollowCase; 4] = [
        (0, (10..=15).collect(), (0..=15).collect(), 10, [10, 11, 12]),
        (5, (0..=5).chain(10..=15).collect(), (0..=15).collect(), 5, [5, 6, 7]),
        (10, (10..=30).collect(), (0..=30).collect(), 20, [20, 21, 22]),
        (5, (0..=20).collect(), (0..=20).filter(|id| *id != 5).collect(), 5, [6, 7, 8]),
    ];
    for (first, before, after, index, shown) in cases {
        let (mut ui, state, list) = from(first, before, true);
        list.set(after);
        ui.wait_for_idle();
        assert_eq!(state.first_visible_item_index(), index, "from {first}");
        assert_eq!(visible_keys(*state), keys(&shown), "from {first}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyCustomKeysTest.kt, keysLambdaIsCalledOnlyOnce
#[test]
fn the_key_function_is_called_once() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let _ui = rule(move || {
        let counted = counted.clone();
        LazyColumn!(move |scope: LazyListScope| {
            let counted = counted.clone();
            scope.items_keyed(
                1,
                move |_| {
                    counted.set(counted.get() + 1);
                    0
                },
                |_| Item("item".to_string()),
            );
        });
    });
    assert_eq!(calls.get(), 1);
}

// Review of D867: a key function reading state the content lambda doesn't read sees it change,
// as Compose's key map (a `derivedStateOf`) does.
#[test]
fn keys_read_from_state_follow_it() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let ids = mutable_state_of(vec![0, 1, 2]);
    let ids_handle = ids.handle();
    let mut ui = rule(move || {
        LazyColumn!(state = handle, move |scope: LazyListScope| {
            scope.items_keyed(
                3,
                move |index| ids_handle.get()[index],
                move |index| remembered_item(ids_handle.get()[index]),
            );
        });
    });
    assert_eq!(visible_keys(*state), keys(&[0, 1, 2]));
    ids.set(vec![0, 2, 1]);
    ui.wait_for_idle();
    assert_eq!(visible_keys(*state), keys(&[0, 2, 1]));
    assert_items(&mut ui, &["0", "2", "1"]);
}
