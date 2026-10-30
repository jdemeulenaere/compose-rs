//! Saveable state in lazy items, across recreating the host and scrolling back: scenarios of
//! Compose's `LazyLayoutStateRestorationTest`, rewritten on `LazyColumn`, for `LazyLayout` isn't
//! public here (lazy-layout-public).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, UnitExt};

/// What each item got from `remember_saveable`, by the value it starts from.
type Told = Rc<RefCell<Vec<i32>>>;

fn counter_item(index: usize, told: Told, next: Rc<[Cell<i32>; 3]>) {
    let value = remember_saveable(move || {
        let value = next[index].get();
        next[index].set(value + 1);
        value
    });
    told.borrow_mut()[index] = value;
    Box!(modifier = Modifier.size(1.dp()));
}

fn counters() -> Rc<[Cell<i32>; 3]> {
    Rc::new([Cell::new(1), Cell::new(10), Cell::new(100)])
}

fn big_rule() -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(300, 300), || {})
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStateRestorationTest.kt, visibleItemsStateRestored
#[test]
fn the_items_in_view_get_their_state_back() {
    let told: Told = Rc::new(RefCell::new(vec![0; 3]));
    let (given, next) = (told.clone(), counters());
    let mut tester = StateRestorationTester::new();
    let mut ui = big_rule();
    tester.set_content(&mut ui, move || {
        let (given, next) = (given.clone(), next.clone());
        LazyColumn!(move |scope: LazyListScope| {
            let (given, next) = (given.clone(), next.clone());
            scope.items(3, move |index| counter_item(index, given.clone(), next.clone()));
        });
    });
    ui.wait_for_idle();
    assert_eq!(*told.borrow(), [1, 10, 100]);
    *told.borrow_mut() = vec![0; 3];
    tester.emulate_save_and_restore(&mut ui);
    ui.wait_for_idle();
    assert_eq!(*told.borrow(), [1, 10, 100]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStateRestorationTest.kt, stateRestoredWhenUsedWithCustomKeys
#[test]
fn keyed_items_get_their_state_back() {
    let told: Told = Rc::new(RefCell::new(vec![0; 3]));
    let (given, next) = (told.clone(), counters());
    let mut tester = StateRestorationTester::new();
    let mut ui = big_rule();
    tester.set_content(&mut ui, move || {
        let (given, next) = (given.clone(), next.clone());
        LazyColumn!(move |scope: LazyListScope| {
            let (given, next) = (given.clone(), next.clone());
            scope.items_keyed(
                3,
                |index| format!("{index}"),
                move |index| counter_item(index, given.clone(), next.clone()),
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(*told.borrow(), [1, 10, 100]);
    *told.borrow_mut() = vec![0; 3];
    tester.emulate_save_and_restore(&mut ui);
    ui.wait_for_idle();
    assert_eq!(*told.borrow(), [1, 10, 100]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStateRestorationTest.kt, stateRestoredWhenUsedWithCustomKeysAfterReordering
#[test]
fn keyed_items_reordered_get_their_state_back() {
    let told: Told = Rc::new(RefCell::new(vec![0; 3]));
    let (given, next) = (told.clone(), counters());
    let order = mutable_state_of(vec![0usize, 1, 2]);
    let read = order.handle();
    let mut tester = StateRestorationTester::new();
    let mut ui = big_rule();
    tester.set_content(&mut ui, move || {
        let (given, next) = (given.clone(), next.clone());
        let order = read.get();
        let keys = order.clone();
        LazyColumn!(move |scope: LazyListScope| {
            let (given, next, keys, order) =
                (given.clone(), next.clone(), keys.clone(), order.clone());
            scope.items_keyed(
                order.len(),
                move |position| format!("{}", keys[position]),
                move |position| counter_item(order[position], given.clone(), next.clone()),
            );
        });
    });
    ui.wait_for_idle();
    order.set(vec![1, 2, 0]);
    ui.wait_for_idle();
    assert_eq!(*told.borrow(), [1, 10, 100]);
    *told.borrow_mut() = vec![0; 3];
    tester.emulate_save_and_restore(&mut ui);
    ui.wait_for_idle();
    assert_eq!(*told.borrow(), [1, 10, 100]);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStateRestorationTest.kt, itemsStateRestoredWhenWeScrolledBackToIt
#[test]
fn an_item_scrolled_back_to_gets_its_state_back() {
    let (next, told, disposed) =
        (Rc::new(Cell::new(1)), Rc::new(Cell::new(0)), Rc::new(Cell::new(false)));
    let (counting, telling, gone) = (next.clone(), told.clone(), disposed.clone());
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (counting, telling, gone) = (counting.clone(), telling.clone(), gone.clone());
        LazyColumn!(
            modifier = Modifier.size(30.dp()),
            state = handle,
            move |scope: LazyListScope| {
                let (counting, telling, gone) = (counting.clone(), telling.clone(), gone.clone());
                scope.items(2, move |index| {
                    if index == 0 {
                        let counting = counting.clone();
                        telling.set(remember_saveable(move || {
                            let value = counting.get();
                            counting.set(value + 1);
                            value
                        }));
                        let gone = gone.clone();
                        DisposableEffect((), move |scope| {
                            let gone = gone.clone();
                            scope.on_dispose(move || gone.set(true))
                        });
                    }
                    Box!(modifier = Modifier.size(30.dp()));
                });
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(told.get(), 1);
    handle.request_scroll_to_item(1, 0);
    ui.wait_for_idle();
    assert!(disposed.get());
    told.set(0);
    handle.request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    assert_eq!(told.get(), 1);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStateRestorationTest.kt, nestedLazy_itemsStateRestoredWhenWeScrolledBackToIt
#[test]
fn a_nested_item_scrolled_back_to_gets_its_state_back() {
    let (next, told, disposed) =
        (Rc::new(Cell::new(1)), Rc::new(Cell::new(0)), Rc::new(Cell::new(false)));
    let (counting, telling, gone) = (next.clone(), told.clone(), disposed.clone());
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (counting, telling, gone) = (counting.clone(), telling.clone(), gone.clone());
        LazyColumn!(
            modifier = Modifier.size(30.dp()),
            state = handle,
            move |scope: LazyListScope| {
                let (counting, telling, gone) = (counting.clone(), telling.clone(), gone.clone());
                scope.items(2, move |index| {
                    if index == 0 {
                        let (counting, telling, gone) =
                            (counting.clone(), telling.clone(), gone.clone());
                        LazyColumn!(
                            modifier = Modifier.size(30.dp()),
                            move |scope: LazyListScope| {
                                let (counting, telling, gone) =
                                    (counting.clone(), telling.clone(), gone.clone());
                                scope.items(1, move |_| {
                                    let counting = counting.clone();
                                    telling.set(remember_saveable(move || {
                                        let value = counting.get();
                                        counting.set(value + 1);
                                        value
                                    }));
                                    let gone = gone.clone();
                                    DisposableEffect((), move |scope| {
                                        let gone = gone.clone();
                                        scope.on_dispose(move || gone.set(true))
                                    });
                                    Box!(modifier = Modifier.size(30.dp()));
                                });
                            }
                        );
                    } else {
                        Box!(modifier = Modifier.size(30.dp()));
                    }
                });
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(told.get(), 1);
    handle.request_scroll_to_item(1, 0);
    ui.wait_for_idle();
    assert!(disposed.get());
    told.set(0);
    handle.request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    assert_eq!(told.get(), 1);
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/layout/LazyLayoutStateRestorationTest.kt, itemsStateRestoredOnlyForVisibleItemsWhenStateSaved_100items
#[test]
fn of_a_hundred_items_those_in_view_when_saved_get_their_state_back() {
    let told: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(vec![0; 100]));
    let to_use = Rc::new(Cell::new(1));
    let (given, using) = (told.clone(), to_use.clone());
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut tester = StateRestorationTester::new();
    let mut ui = big_rule();
    tester.set_content(&mut ui, move || {
        let (given, using) = (given.clone(), using.clone());
        LazyColumn!(
            modifier = Modifier.size(300.dp()),
            state = handle,
            move |scope: LazyListScope| {
                let (given, using) = (given.clone(), using.clone());
                scope.items(100, move |index| {
                    let using = using.clone();
                    given.borrow_mut()[index] = remember_saveable(move || using.get());
                    Box!(modifier = Modifier.size(30.dp()));
                });
            }
        );
    });
    ui.wait_for_idle();
    // Every item seen once.
    for first in (0..=90).step_by(10) {
        handle.request_scroll_to_item(first, 0);
        ui.wait_for_idle();
    }
    assert_eq!(*told.borrow(), vec![1; 100]);
    *told.borrow_mut() = vec![0; 100];
    to_use.set(2);
    tester.emulate_save_and_restore(&mut ui);
    ui.wait_for_idle();
    assert_eq!(told.borrow()[90..100], vec![1; 10][..], "in view when saved");
    handle.request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    assert_eq!(told.borrow()[0..10], vec![1; 10][..], "their state stays with the holder");
}
