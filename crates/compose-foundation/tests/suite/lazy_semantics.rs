//! Semantics of lazy layouts (`IndexForKey` and `CollectionInfo`), as Compose's
//! `lazy/list/LazySemanticsTest`, `lazy/grid/LazySemanticsTest`, and
//! `staggeredgrid/LazyStaggeredGridSemanticTest`, plus `Pager` and typed key edge cases.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::lazy::layout::get_default_lazy_layout_key;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{
    CollectionInfo, Dp, IntSize, Modifier, SemanticsActions, SemanticsProperties, UnitExt,
};

const N: usize = 20;
const STAGGERED_ITEM_COUNT: usize = 60;
const LIST_TAG: &str = "LazyList";
const GRID_TAG: &str = "LazyGrid";
const STAGGERED_TAG: &str = "LazyStaggeredGrid";
const PAGER_TAG: &str = "Pager";

fn rule(size: IntSize, content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(size, content)
}

fn tag(index: usize) -> String {
    format!("tag_{index}")
}

fn key(index: usize) -> String {
    format!("key_{index}")
}

fn scroll_to_key_semantics(ui: &mut ComposeTestRule, node_tag: &str, needle: &'static str) {
    let snapshot = ui.on_node_with_tag(node_tag).fetch_semantics_snapshot();
    let index_for_key = snapshot
        .config
        .get(&SemanticsProperties::INDEX_FOR_KEY)
        .expect("node should expose IndexForKey semantics");
    let owned = needle.to_string();
    let index = (index_for_key.0)(&owned);
    let literal_index = (index_for_key.0)(&needle);
    assert_eq!(index, literal_index, "String and &'static str keys should resolve identically");
    assert_ne!(index, -1, "key {needle:?} should be found");
    ui.on_node_with_tag(node_tag).perform_semantics_action_with(
        &SemanticsActions::SCROLL_TO_INDEX,
        |action| {
            assert!(action(index));
        },
    );
    ui.wait_for_idle();
}

fn index_for_key<K: 'static>(ui: &mut ComposeTestRule, node_tag: &str, needle: &K) -> i32 {
    let snapshot = ui.on_node_with_tag(node_tag).fetch_semantics_snapshot();
    let index_for_key = snapshot
        .config
        .get(&SemanticsProperties::INDEX_FOR_KEY)
        .expect("node should expose IndexForKey semantics");
    (index_for_key.0)(needle)
}

fn assert_range_bounds(
    ui: &mut ComposeTestRule,
    first_shown: usize,
    last_shown: usize,
    count: usize,
) {
    for index in 0..count {
        let mut node = ui.on_node_with_tag(&tag(index));
        if (first_shown..=last_shown).contains(&index) {
            node.assert_is_displayed();
        } else {
            node.assert_does_not_exist();
        }
    }
}

fn check_visibility_key_indexing(ui: &mut ComposeTestRule, list_tag: &str) {
    assert_range_bounds(ui, 0, 3, N);
    assert_eq!(index_for_key(ui, list_tag, &"missing_key"), -1);
    assert_eq!(index_for_key(ui, list_tag, &10usize), -1);

    // Scroll forward to index 10 -> items 10..=13 are visible.
    scroll_to_key_semantics(ui, list_tag, "key_10");
    assert_range_bounds(ui, 10, 13, N);

    // Scroll to the last item (index 19) -> items 16..=19 are visible.
    scroll_to_key_semantics(ui, list_tag, "key_19");
    assert_range_bounds(ui, 16, 19, N);
}

fn SpacerItem(index: usize) {
    Spacer(Modifier.required_size(30.dp()).test_tag(tag(index)));
}

// ---- lazy/list/LazySemanticsTest.kt (8 tests) --------------------------------------------------

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, itemSemantics_column
#[test]
fn item_list_semantics_are_set_correctly() {
    let mut ui = rule(IntSize::new(500, 500), || {
        LazyColumn!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            |scope: LazyListScope| {
                for index in 0..N {
                    scope.item_keyed(key(index), move || SpacerItem(index));
                }
            }
        );
    });
    check_visibility_key_indexing(&mut ui, LIST_TAG);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, itemsSemantics_column
#[test]
fn items_list_semantics_are_set_correctly() {
    let mut ui = rule(IntSize::new(500, 500), || {
        LazyColumn!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            |scope: LazyListScope| {
                scope.items_keyed(N, key, SpacerItem);
            }
        );
    });
    check_visibility_key_indexing(&mut ui, LIST_TAG);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, itemSemantics_row
#[test]
fn item_row_semantics_are_set_correctly() {
    let mut ui = rule(IntSize::new(500, 500), || {
        LazyRow!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            |scope: LazyListScope| {
                for index in 0..N {
                    scope.item_keyed(key(index), move || SpacerItem(index));
                }
            }
        );
    });
    check_visibility_key_indexing(&mut ui, LIST_TAG);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, itemsSemantics_row
#[test]
fn items_row_semantics_are_set_correctly() {
    let mut ui = rule(IntSize::new(500, 500), || {
        LazyRow!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            |scope: LazyListScope| {
                scope.items_keyed(N, key, SpacerItem);
            }
        );
    });
    check_visibility_key_indexing(&mut ui, LIST_TAG);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, hasCorrectCollectionInfoInLazyColumn
#[test]
fn item_list_collection_info_is_set_correctly() {
    let mut ui = rule(IntSize::new(500, 500), || {
        LazyColumn!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            |scope: LazyListScope| {
                for index in 0..N {
                    scope.item_keyed(key(index), move || SpacerItem(index));
                }
            }
        );
    });
    let snapshot = ui.on_node_with_tag(LIST_TAG).fetch_semantics_snapshot();
    assert_eq!(
        snapshot.config.get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: N as i32, column_count: 1 })
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, hasCorrectCollectionInfoInLazyRow
#[test]
fn item_row_collection_info_is_set_correctly() {
    let mut ui = rule(IntSize::new(500, 500), || {
        LazyRow!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            |scope: LazyListScope| {
                for index in 0..N {
                    scope.item_keyed(key(index), move || SpacerItem(index));
                }
            }
        );
    });
    let snapshot = ui.on_node_with_tag(LIST_TAG).fetch_semantics_snapshot();
    assert_eq!(
        snapshot.config.get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 1, column_count: N as i32 })
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, hasCorrectCollectionInfoAfterLazyColumnItemsUpdate
#[test]
fn item_list_collection_info_is_updated_when_items_update() {
    let count = mutable_state_of(N);
    let handle = count.handle();
    let mut ui = rule(IntSize::new(500, 500), move || {
        let items = handle.get();
        LazyColumn!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            move |scope: LazyListScope| {
                for index in 0..items {
                    scope.item_keyed(key(index), move || SpacerItem(index));
                }
            }
        );
    });
    assert_eq!(
        ui.on_node_with_tag(LIST_TAG)
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: N as i32, column_count: 1 })
    );

    count.set(N * 2);
    ui.wait_for_idle();
    assert_eq!(
        ui.on_node_with_tag(LIST_TAG)
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: (N * 2) as i32, column_count: 1 })
    );

    count.set(0);
    ui.wait_for_idle();
    assert_eq!(
        ui.on_node_with_tag(LIST_TAG)
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 0, column_count: 1 })
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazySemanticsTest.kt, hasCorrectCollectionInfoAfterLazyRowItemsUpdate
#[test]
fn item_row_collection_info_is_updated_when_items_update() {
    let count = mutable_state_of(N);
    let handle = count.handle();
    let mut ui = rule(IntSize::new(500, 500), move || {
        let items = handle.get();
        LazyRow!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            move |scope: LazyListScope| {
                for index in 0..items {
                    scope.item_keyed(key(index), move || SpacerItem(index));
                }
            }
        );
    });
    assert_eq!(
        ui.on_node_with_tag(LIST_TAG)
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 1, column_count: N as i32 })
    );

    count.set(N * 2);
    ui.wait_for_idle();
    assert_eq!(
        ui.on_node_with_tag(LIST_TAG)
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 1, column_count: (N * 2) as i32 })
    );

    count.set(0);
    ui.wait_for_idle();
    assert_eq!(
        ui.on_node_with_tag(LIST_TAG)
            .fetch_semantics_snapshot()
            .config
            .get(&SemanticsProperties::COLLECTION_INFO),
        Some(&CollectionInfo { row_count: 1, column_count: 0 })
    );
}

// ---- lazy/grid/LazySemanticsTest.kt (2 tests) --------------------------------------------------

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazySemanticsTest.kt, itemSemantics_verticalGrid
#[test]
fn item_grid_semantics_are_set_correctly() {
    for vertical in [true, false] {
        let mut ui = rule(IntSize::new(500, 500), move || {
            let content = |scope: LazyGridScope| {
                for index in 0..N {
                    scope.item_keyed(key(index), move || SpacerItem(index));
                }
            };
            if vertical {
                LazyVerticalGrid!(
                    GridCells::Fixed(1),
                    modifier = Modifier.required_size(100.dp()).test_tag(GRID_TAG),
                    move |scope: LazyGridScope| content(scope)
                );
            } else {
                LazyHorizontalGrid!(
                    GridCells::Fixed(1),
                    modifier = Modifier.required_size(100.dp()).test_tag(GRID_TAG),
                    move |scope: LazyGridScope| content(scope)
                );
            }
        });
        check_visibility_key_indexing(&mut ui, GRID_TAG);
        assert_eq!(
            ui.on_node_with_tag(GRID_TAG)
                .fetch_semantics_snapshot()
                .config
                .get(&SemanticsProperties::COLLECTION_INFO),
            Some(&CollectionInfo { row_count: -1, column_count: -1 })
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/grid/LazySemanticsTest.kt, itemsSemantics_verticalGrid
#[test]
fn items_grid_semantics_are_set_correctly() {
    for vertical in [true, false] {
        let mut ui = rule(IntSize::new(500, 500), move || {
            let content = |scope: LazyGridScope| {
                scope.items_keyed(N, key, SpacerItem);
            };
            if vertical {
                LazyVerticalGrid!(
                    GridCells::Fixed(1),
                    modifier = Modifier.required_size(100.dp()).test_tag(GRID_TAG),
                    move |scope: LazyGridScope| content(scope)
                );
            } else {
                LazyHorizontalGrid!(
                    GridCells::Fixed(1),
                    modifier = Modifier.required_size(100.dp()).test_tag(GRID_TAG),
                    move |scope: LazyGridScope| content(scope)
                );
            }
        });
        check_visibility_key_indexing(&mut ui, GRID_TAG);
    }
}

// ---- lazy/staggeredgrid/LazyStaggeredGridSemanticTest.kt (2 tests, both orientations) ----------

fn check_staggered_semantics(ui: &mut ComposeTestRule) {
    assert_range_bounds(ui, 0, 5, STAGGERED_ITEM_COUNT);
    assert_eq!(index_for_key(ui, STAGGERED_TAG, &"missing"), -1);

    scroll_to_key_semantics(ui, STAGGERED_TAG, "key_30");
    assert_range_bounds(ui, 30, 35, STAGGERED_ITEM_COUNT);

    scroll_to_key_semantics(ui, STAGGERED_TAG, "key_59");
    assert_range_bounds(ui, 54, 59, STAGGERED_ITEM_COUNT);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridSemanticTest.kt, semantics_item
#[test]
fn staggered_item_semantics_are_set_correctly() {
    for vertical in [true, false] {
        let mut ui = rule(IntSize::new(500, 500), move || {
            let main_axis_size = Dp(100.0 * 3.0 - 1.0);
            let cross_axis_size = Dp(100.0 * 2.0);
            let content = |scope: LazyStaggeredGridScope| {
                for index in 0..STAGGERED_ITEM_COUNT {
                    scope.item_keyed(key(index), move || {
                        Spacer(Modifier.required_size(100.dp()).test_tag(tag(index)));
                    });
                }
            };
            if vertical {
                LazyVerticalStaggeredGrid!(
                    StaggeredGridCells::Fixed(2),
                    modifier = Modifier
                        .width(cross_axis_size)
                        .height(main_axis_size)
                        .test_tag(STAGGERED_TAG),
                    move |scope: LazyStaggeredGridScope| content(scope)
                );
            } else {
                LazyHorizontalStaggeredGrid!(
                    StaggeredGridCells::Fixed(2),
                    modifier = Modifier
                        .height(cross_axis_size)
                        .width(main_axis_size)
                        .test_tag(STAGGERED_TAG),
                    move |scope: LazyStaggeredGridScope| content(scope)
                );
            }
        });
        check_staggered_semantics(&mut ui);
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridSemanticTest.kt, semantics_items
#[test]
fn staggered_items_semantics_are_set_correctly() {
    for vertical in [true, false] {
        let mut ui = rule(IntSize::new(500, 500), move || {
            let main_axis_size = Dp(100.0 * 3.0 - 1.0);
            let cross_axis_size = Dp(100.0 * 2.0);
            let content = |scope: LazyStaggeredGridScope| {
                scope.items_keyed(STAGGERED_ITEM_COUNT, key, |index| {
                    Spacer(Modifier.required_size(100.dp()).test_tag(tag(index)));
                });
            };
            if vertical {
                LazyVerticalStaggeredGrid!(
                    StaggeredGridCells::Fixed(2),
                    modifier = Modifier
                        .width(cross_axis_size)
                        .height(main_axis_size)
                        .test_tag(STAGGERED_TAG),
                    move |scope: LazyStaggeredGridScope| content(scope)
                );
            } else {
                LazyHorizontalStaggeredGrid!(
                    StaggeredGridCells::Fixed(2),
                    modifier = Modifier
                        .height(cross_axis_size)
                        .width(main_axis_size)
                        .test_tag(STAGGERED_TAG),
                    move |scope: LazyStaggeredGridScope| content(scope)
                );
            }
        });
        check_staggered_semantics(&mut ui);
    }
}

// ---- Pager & typed KeyProvider semantics coverage ----------------------------------------------

#[test]
fn pager_index_for_key_typed_and_disabled_scroll() {
    for (vertical, enabled) in [(false, true), (true, true), (false, false)] {
        let mut ui = rule(IntSize::new(200, 200), move || {
            let state = remember_pager_state!(0, 0.0, || 20);
            let key_fn = pager_key(|page| format!("page_{page}"));
            let content = |_, page| {
                Spacer(Modifier.fill_max_size().test_tag(tag(page)));
            };
            if vertical {
                VerticalPager!(
                    state,
                    modifier = Modifier.fill_max_size().test_tag(PAGER_TAG),
                    user_scroll_enabled = enabled,
                    key = Some(key_fn),
                    page_content = content
                );
            } else {
                HorizontalPager!(
                    state,
                    modifier = Modifier.fill_max_size().test_tag(PAGER_TAG),
                    user_scroll_enabled = enabled,
                    key = Some(key_fn),
                    page_content = content
                );
            }
        });
        // Even with user_scroll_enabled = false, IndexForKey is present while ScrollToIndex is absent.
        let snapshot = ui.on_node_with_tag(PAGER_TAG).fetch_semantics_snapshot();
        assert!(snapshot.config.contains(&SemanticsProperties::INDEX_FOR_KEY));
        assert_eq!(snapshot.config.contains(&SemanticsActions::SCROLL_TO_INDEX), enabled);
        assert_eq!(index_for_key(&mut ui, PAGER_TAG, &"page_12"), 12);
        assert_eq!(index_for_key(&mut ui, PAGER_TAG, &"page_99".to_string()), -1);
        assert_eq!(index_for_key(&mut ui, PAGER_TAG, &12usize), -1);

        if enabled {
            ui.on_node_with_tag(PAGER_TAG).perform_scroll_to_key("page_12");
            ui.on_node_with_tag(&tag(12)).assert_is_displayed();
            ui.on_node_with_tag(&tag(0)).assert_does_not_exist();
        }
    }
}

#[test]
fn pager_raw_u64_key_fallback_and_called_once() {
    let calls = Rc::new(Cell::new(0usize));
    let noted = calls.clone();
    let mut ui = rule(IntSize::new(200, 200), move || {
        let state = remember_pager_state!(0, 0.0, || 5);
        let noted = noted.clone();
        let raw_key: Rc<dyn Fn(usize) -> u64> = Rc::new(move |page| {
            noted.set(noted.get() + 1);
            (page as u64 + 1) * 100
        });
        HorizontalPager!(
            state,
            modifier = Modifier.fill_max_size().test_tag(PAGER_TAG),
            key = Some(raw_key),
            page_content = |_, page| {
                Spacer(Modifier.fill_max_size().test_tag(tag(page)));
            }
        );
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(index_for_key(&mut ui, PAGER_TAG, &300u64), 2);
    assert_eq!(index_for_key(&mut ui, PAGER_TAG, &300i32), -1);
    ui.on_node_with_tag(PAGER_TAG).perform_scroll_to_key(400u64);
    ui.on_node_with_tag(&tag(3)).assert_is_displayed();
}

#[derive(Clone, PartialEq, Eq)]
struct SameHashA(i32);

impl Hash for SameHashA {
    fn hash<H: Hasher>(&self, state: &mut H) {
        42u64.hash(state);
    }
}

#[derive(Clone, PartialEq, Eq)]
struct SameHashB(i32);

impl Hash for SameHashB {
    fn hash<H: Hasher>(&self, state: &mut H) {
        42u64.hash(state);
    }
}

#[test]
fn index_for_key_uses_exact_types_even_with_equal_hashes_and_handles_unkeyed_and_updates() {
    let offset = mutable_state_of(0usize);
    let handle = offset.handle();
    let mut ui = rule(IntSize::new(200, 200), move || {
        let shift = handle.get();
        LazyColumn!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST_TAG),
            move |scope: LazyListScope| {
                scope.item_keyed(SameHashA(1), move || {
                    Spacer(Modifier.required_size(30.dp()).test_tag("a"));
                });
                scope.items(2, |i| {
                    Spacer(Modifier.required_size(30.dp()).test_tag(format!("unkeyed_{i}")));
                });
                scope.items_keyed(
                    4,
                    move |i| i + shift,
                    move |i| {
                        Spacer(Modifier.required_size(30.dp()).test_tag(format!("dyn_{i}")));
                    },
                );
            }
        );
    });

    // Different types with identical hashes (and the raw u64 hash itself) do not match SameHashA(1).
    let raw_hash = compose_runtime::hash_key(&SameHashA(1));
    assert_eq!(raw_hash, compose_runtime::hash_key(&SameHashB(1)));
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &SameHashA(1)), 0);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &SameHashB(1)), -1);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &SameHashA(2)), -1);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &raw_hash), -1);

    // Unkeyed items match Compose's getDefaultLazyLayoutKey(index), never user integers/strings.
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &get_default_lazy_layout_key(1)), 1);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &get_default_lazy_layout_key(2)), 2);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &99usize), -1);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &"1"), -1);

    // Typed keyed interval matches exact usize type, not i32 or u64.
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &0usize), 3);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &3usize), 6);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &0i32), -1);

    // Dynamic key update is reflected immediately.
    offset.set(100);
    ui.wait_for_idle();
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &0usize), -1);
    assert_eq!(index_for_key(&mut ui, LIST_TAG, &102usize), 5);
}
