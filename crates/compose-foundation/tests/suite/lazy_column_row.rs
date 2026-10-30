//! `LazyColumn` and `LazyRow` specifics, as Compose's `LazyColumnTest` and `LazyRowTest`:
//! compositions disposed with their data or the list, data changes, cross-axis alignment, a
//! fling stopped by a press, recomposition order, nested rows, right to left;
//! and the lists' and grids' `fling_behavior`. Density 1, so Compose's dps are pixels.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::gestures::{FlingBehavior, LocalBoxFuture, ScrollScope};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Offset, UnitExt};

use crate::lazy_support::{Lazy, bounds};
use crate::scroll_support::{Touch, run, swipe};

const LIST: &str = "LazyListTag";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

// Review of D880: a list leaving under a finger ends the drag's scroll, as `vertical_scroll`'s and
// `Modifier.scrollable`'s nodes do; the state, kept by its owner, doesn't stay scrolling.
#[test]
fn a_drag_ends_when_its_list_leaves() {
    let owner = LazyListState::new(0, 0);
    let state = *owner;
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = rule(move || {
        if handle.get() {
            LazyColumn!(
                modifier = Modifier.test_tag(LIST).size(300.dp()),
                state = state,
                |scope: LazyListScope| {
                    scope.items(100, |_| Spacer(Modifier.height(50.dp())));
                }
            );
        }
    });
    let mut finger = Touch::down(&mut ui, Offset::new(100.0, 200.0));
    finger.move_by(&mut ui, Offset::new(0.0, -50.0));
    assert!(state.is_scroll_in_progress(), "dragging");
    shown.set(false);
    ui.wait_for_idle();
    assert!(!state.is_scroll_in_progress());
    finger.up(&mut ui);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, compositionsAreDisposed_whenDataIsChanged
#[test]
fn compositions_are_disposed_when_the_data_changes() {
    let (composed, disposed) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let part2 = mutable_state_of(false);
    let handle = part2.handle();
    let (into, out) = (composed.clone(), disposed.clone());
    let mut ui = rule(move || {
        let data: Vec<i32> = if handle.get() { (4..=5).collect() } else { (1..=3).collect() };
        let (into, out) = (into.clone(), out.clone());
        LazyColumn!(
            modifier = Modifier.test_tag(LIST).fill_max_size(),
            move |scope: LazyListScope| {
                let (into, out) = (into.clone(), out.clone());
                scope.items_list(data.clone(), move |_, _: &i32| {
                    let (into, out) = (into.clone(), out.clone());
                    DisposableEffect(NeverEqual, move |scope| {
                        into.set(into.get() + 1);
                        scope.on_dispose(move || out.set(out.get() + 1))
                    });
                    Spacer(Modifier.height(50.dp()));
                });
            }
        );
    });
    assert_eq!(composed.get(), 3, "not all items were composed");
    composed.set(0);
    part2.set(true);
    ui.wait_for_idle();
    assert_eq!(composed.get(), 2, "no items composed after the data change");
    assert_eq!(disposed.get(), 3, "compositions were leaked");
}

/// A key never equal to itself, as Compose's tests' `NeverEqualObject`: its effect runs again
/// each recomposition.
struct NeverEqual;

impl ComposeArg for NeverEqual {
    fn same(&self, _: &Self) -> bool {
        false
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, compositionsAreDisposed_whenLazyListIsDisposed
#[test]
fn compositions_are_disposed_with_the_list() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let disposed = Rc::new(RefCell::new([false; 2]));
    let out = disposed.clone();
    let mut ui = rule(move || {
        if handle.get() {
            let out = out.clone();
            LazyColumn!(modifier = Modifier.fill_max_size(), move |scope: LazyListScope| {
                let out = out.clone();
                scope.items(2, move |index| {
                    Box!(modifier = Modifier.required_size(100.dp()), {});
                    let out = out.clone();
                    DisposableEffect((), move |scope| {
                        scope.on_dispose(move || out.borrow_mut()[index] = true)
                    });
                });
            });
        }
    });
    assert_eq!(*disposed.borrow(), [false, false], "an item was disposed at once");
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(*disposed.borrow(), [true, true], "an item was not disposed");
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, removeItemsTest
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, changingDataTest
#[test]
fn removed_and_changed_items_come_and_go() {
    let count = mutable_state_of(3usize);
    let handle = count.handle();
    let mut ui = rule(move || {
        let count = handle.get();
        LazyColumn!(modifier = Modifier.test_tag("List"), move |scope: LazyListScope| {
            scope.items_list((0..count).collect::<Vec<_>>(), |_, item: &usize| {
                BasicText!(item.to_string())
            });
        });
    });
    for remaining in (0..=3).rev() {
        count.set(remaining);
        ui.wait_for_idle();
        for index in 0..3 {
            let found = ui.count_nodes(&has_text(&index.to_string()));
            assert_eq!(found, usize::from(index < remaining), "{index} with {remaining} items");
        }
    }

    let data = mutable_state_of(vec![1, 2, 3]);
    let handle = data.handle();
    let mut ui = rule(move || {
        let data = handle.get();
        LazyColumn!(modifier = Modifier.test_tag("List"), move |scope: LazyListScope| {
            scope.items_list(data.clone(), |_, item: &i32| BasicText!(item.to_string()));
        });
    });
    for shown in [vec![1, 2, 3], vec![4, 5, 6, 7, 8], vec![3, 4]] {
        data.set(shown.clone());
        ui.wait_for_idle();
        for index in 1..=8 {
            let found = ui.count_nodes(&has_text(&index.to_string()));
            assert_eq!(found, usize::from(shown.contains(&index)), "{index} in {shown:?}");
        }
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, lazyColumnAlignmentCenterHorizontally
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, lazyColumnAlignmentStart
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, lazyColumnAlignmentEnd
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyRowTest.kt, lazyRowAlignmentCenterVertically
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyRowTest.kt, lazyRowAlignmentTop
// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyRowTest.kt, lazyRowAlignmentBottom
#[test]
fn items_align_across_the_list() {
    let two_items = |scope: LazyListScope| {
        scope.items_list(vec![1, 2], |_, item: &i32| {
            let (size, tag) = if *item == 1 { (50, "first") } else { (70, "second") };
            Spacer(Modifier.size(size.dp()).test_tag(tag));
        });
    };
    for (alignment, first, second) in [
        (Alignment::CENTER_HORIZONTALLY, 25.0, 15.0),
        (Alignment::START, 0.0, 0.0),
        (Alignment::END, 50.0, 30.0),
    ] {
        let mut ui = rule(move || {
            LazyColumn!(
                modifier = Modifier.test_tag(LIST).width(100.dp()),
                horizontal_alignment = alignment,
                content = two_items
            );
        });
        let list = bounds(&mut ui, LIST);
        assert_eq!((list.left, list.right), (0.0, 100.0));
        let (a, b) = (bounds(&mut ui, "first"), bounds(&mut ui, "second"));
        assert_eq!(
            ((a.left, a.top), (b.left, b.top)),
            ((first, 0.0), (second, 50.0)),
            "{alignment:?}"
        );
    }
    for (alignment, first, second) in [
        (Alignment::CENTER_VERTICALLY, 25.0, 15.0),
        (Alignment::TOP, 0.0, 0.0),
        (Alignment::BOTTOM, 50.0, 30.0),
    ] {
        let mut ui = rule(move || {
            LazyRow!(
                modifier = Modifier.test_tag(LIST).height(100.dp()),
                vertical_alignment = alignment,
                content = two_items
            );
        });
        let list = bounds(&mut ui, LIST);
        assert_eq!((list.top, list.bottom), (0.0, 100.0));
        let (a, b) = (bounds(&mut ui, "first"), bounds(&mut ui, "second"));
        assert_eq!(
            ((a.left, a.top), (b.left, b.top)),
            ((0.0, first), (50.0, second)),
            "{alignment:?}"
        );
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, flingAnimationStopsOnFingerDown
#[test]
fn a_press_stops_a_fling() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyColumn!(
            modifier = Modifier.required_size(100.dp()).test_tag(LIST),
            state = handle,
            |scope: LazyListScope| {
                scope.items_list((1..=20).collect::<Vec<i32>>(), |_, item: &i32| {
                    Spacer(Modifier.required_size(20.dp()).test_tag(item.to_string()));
                });
            }
        );
    });
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 0)
    );
    ui.set_auto_advance(false);
    // Compose's `swipeUp()`: from the bottom to the top in 200 ms.
    let list = bounds(&mut ui, LIST);
    let x = (list.left + list.right) / 2.0;
    swipe(&mut ui, Offset::new(x, list.bottom - 1.0), Offset::new(x, list.top), 200);
    ui.advance_time_by(std::time::Duration::from_millis(100));
    let interrupted = (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
    assert!(interrupted.0 != 0 && interrupted.1 != 0, "{interrupted:?}");
    let center = Offset::new(x, (list.top + list.bottom) / 2.0);
    let _finger = Touch::down(&mut ui, center);
    ui.advance_time_by(std::time::Duration::from_millis(100));
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        interrupted
    );
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, removalWithMutableStateListOf
#[test]
fn an_item_removed_from_a_state_list_goes() {
    let items = mutable_state_list_of::<&'static str>();
    items.set(vec!["1", "2", "3"]);
    let handle = items.handle();
    let mut ui = rule(move || {
        let items = handle.get();
        LazyColumn!(move |scope: LazyListScope| {
            scope.items_list(items.clone(), |_, item: &&str| {
                Spacer(Modifier.size(15.dp()).test_tag(item.to_string()))
            });
        });
    });
    items.update(|items| items[..items.len() - 1].to_vec());
    ui.wait_for_idle();
    ui.on_node_with_tag("1").assert_exists();
    ui.on_node_with_tag("2").assert_exists();
    ui.on_node_with_tag("3").assert_does_not_exist();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, recompositionOrder
#[test]
fn an_item_recomposes_once_with_the_list_around_it() {
    let (outer, inner) = (mutable_state_of(0), mutable_state_of(0));
    let (outer_handle, inner_handle) = (outer.handle(), inner.handle());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let into = seen.clone();
    let mut ui = rule(move || {
        let outer = outer_handle.get();
        let into = into.clone();
        LazyColumn!(move |scope: LazyListScope| {
            let into = into.clone();
            scope.items(1, move |_| {
                into.borrow_mut().push((outer, inner_handle.get()));
                Box!(modifier = Modifier.fill_max_size(), {});
            });
        });
    });
    inner.set(1);
    outer.set(1);
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), vec![(0, 0), (1, 1)]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyColumnTest.kt, wrappedNestedLazyRowDisplayCorrectContent
#[test]
fn a_nested_row_shows_its_item_after_a_jump() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        LazyColumn!(
            modifier = Modifier.size(20.dp()).test_tag(LIST),
            state = handle,
            |scope: LazyListScope| {
                scope.items(100, |index| {
                    LazyRow!(move |row: LazyListScope| {
                        row.items(1, move |_| {
                            BasicText!(index.to_string(), modifier = Modifier.size(21.dp()))
                        });
                    });
                });
            }
        );
    });
    for item in 1..=10 {
        let _ = run(&mut ui, async move { handle.scroll_to_item(item, 0).await });
        assert_eq!(ui.count_nodes(&has_text(&item.to_string())), 1, "item {item}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyRowTest.kt, scrollsLeftInRtl
#[test]
fn a_row_right_to_left_scrolls_leftwards() {
    let lazy = Lazy::row();
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = rule(move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
            Box!(modifier = Modifier.width(100.dp()), {
                LazyRow!(
                    modifier = Modifier.test_tag(LIST),
                    state = handle,
                    |scope: LazyListScope| {
                        let item_scope = scope.item_scope();
                        scope.items(4, move |index| {
                            Spacer(
                                Modifier
                                    .width(101.dp())
                                    .fill_parent_max_height(&item_scope)
                                    .test_tag(index.to_string()),
                            );
                        });
                    }
                );
            });
        });
    });
    // The finger moving right: right to left, forward.
    lazy.drag_main_axis_by(&mut ui, LIST, -150.0);
    assert_eq!(state.first_visible_item_index(), 1);
    assert!(state.first_visible_item_scroll_offset() > 0);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/list/LazyRowTest.kt, laysOutRtlCorrectlyWithLargerRow
#[test]
fn a_row_right_to_left_starts_at_its_right() {
    let mut ui = rule(|| {
        Column!({
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
                LazyRow!(modifier = Modifier.size((300.dp(), 100.dp())), |scope: LazyListScope| {
                    scope.items(3, |index| {
                        BasicText!(
                            index.to_string(),
                            modifier = Modifier.size(50.dp()).test_tag(index.to_string())
                        );
                    });
                });
            });
        });
    });
    let first = bounds(&mut ui, "0");
    assert_eq!((first.left, first.top), (250.0, 0.0));
}

/// A fling that records its velocity and scrolls by nothing.
struct RecordingFling {
    velocity: Cell<Option<f32>>,
}

impl FlingBehavior for RecordingFling {
    fn perform_fling<'a>(
        &'a self,
        _scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        self.velocity.set(Some(initial_velocity));
        Box::pin(async { 0.0 })
    }
}

// Compose's lists' and grids' `flingBehavior` parameter (`LazyColumn(flingBehavior = …)`), as
// `ScrollableTest`'s `scrollable_flingBehaviourCalled` does for `Modifier.scrollable`.
#[test]
fn a_list_and_a_grid_fling_with_the_behavior_given() {
    for grid in [false, true] {
        let fling = Rc::new(RecordingFling { velocity: Cell::new(None) });
        let given: Rc<dyn FlingBehavior> = fling.clone();
        let mut ui = rule(move || {
            let behavior = Some(given.clone());
            let modifier = Modifier.size(100.dp()).test_tag(LIST);
            if grid {
                LazyVerticalGrid!(
                    GridCells::Fixed(2),
                    modifier = modifier,
                    fling_behavior = behavior,
                    |scope: LazyGridScope| {
                        scope.items(100, |_| Spacer(Modifier.height(20.dp())));
                    }
                );
            } else {
                LazyColumn!(
                    modifier = modifier,
                    fling_behavior = behavior,
                    |scope: LazyListScope| {
                        scope.items(100, |_| Spacer(Modifier.size(20.dp())));
                    }
                );
            }
        });
        let list = bounds(&mut ui, LIST);
        let x = (list.left + list.right) / 2.0;
        swipe(&mut ui, Offset::new(x, list.bottom - 1.0), Offset::new(x, list.top), 100);
        ui.wait_for_idle();
        let velocity = fling.velocity.get().expect("the fling behavior given wasn't asked");
        // Towards the content's end (the finger up): a positive velocity in the state's terms.
        assert!(velocity > 0.0, "grid: {grid}, {velocity}");
        assert!(ui.on_node_with_tag(LIST).is_displayed());
    }
}
