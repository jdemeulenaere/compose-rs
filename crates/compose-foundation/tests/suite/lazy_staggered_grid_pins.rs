//! Pinning a staggered grid's items, as Compose's `LazyStaggeredGridPinnableContainerTest`. Items
//! that stop being wanted leave between frames here (D575), so its `waitUntil`s run those requests.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, PinnableContainer, PinnedHandle, UnitExt};

use crate::scroll_support::run;

type Composed = Rc<RefCell<BTreeSet<usize>>>;
type Container = Rc<RefCell<Option<Rc<dyn PinnableContainer>>>>;

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(1000, 1000), content)
}

/// `Item`, noting it's composed.
fn Item(index: usize, composed: Composed) {
    Box!(modifier = Modifier.size(10.dp()).test_tag(index.to_string()));
    DisposableEffect(index, move |effects| {
        composed.borrow_mut().insert(index);
        let composed = composed.clone();
        effects.on_dispose(move || {
            composed.borrow_mut().remove(&index);
        })
    });
}

/// Compose's `rule.waitUntil { … }`: the requests between frames run until `done`.
fn wait_until(ui: &mut ComposeTestRule, done: impl Fn() -> bool) {
    let later = std::time::Instant::now() + std::time::Duration::from_secs(60);
    for _ in 0..100 {
        ui.wait_for_idle();
        if done() {
            return;
        }
        ui.host().prefetch(later);
    }
    panic!("never true");
}

fn has(composed: &Composed, index: usize) -> bool {
    composed.borrow().contains(&index)
}

fn missing(composed: &Composed, index: usize) -> impl Fn() -> bool + use<> {
    let composed = composed.clone();
    move || !composed.borrow().contains(&index)
}

struct Pins {
    ui: ComposeTestRule,
    state: LazyStaggeredGridState,
    composed: Composed,
    container: Container,
}

impl Pins {
    fn pin(&mut self) -> PinnedHandle {
        let handle = self.container.borrow().as_ref().expect("composed").pin();
        self.ui.wait_for_idle();
        handle
    }

    fn scroll_to(&mut self, index: usize) {
        let state = self.state;
        let _ = run(&mut self.ui, async move { state.scroll_to_item(index, 0).await });
    }

    fn assert_placed_out_of_view(&mut self, tag: &str) {
        let mut node = self.ui.on_node_with_tag(tag);
        assert!(!node.is_displayed(), "{tag} is out of view");
        node.assert_exists();
    }
}

/// A grid of one lane, `main` items long, of `count` items, the container of item `pinned` kept.
fn pins(
    pinned: usize,
    main: i32,
    initial: usize,
    count: compose_runtime::MutableState<usize>,
) -> Pins {
    let owned = LazyStaggeredGridState::new(initial, 0);
    let state = *owned;
    std::mem::forget(owned);
    let composed: Composed = Rc::default();
    let container: Container = Rc::default();
    let (noted, kept) = (composed.clone(), container.clone());
    let mut ui = rule(move || {
        let (noted, kept) = (noted.clone(), kept.clone());
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            modifier = Modifier.size((10 * main).dp()),
            state = state,
            move |scope: LazyStaggeredGridScope| {
                let (noted, kept) = (noted.clone(), kept.clone());
                scope.items(count.get(), move |index| {
                    if index == pinned {
                        *kept.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                    }
                    Item(index, noted.clone());
                });
            }
        );
    });
    ui.wait_for_idle();
    Pins { ui, state, composed, container }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinnedItemIsComposedAndPlacedWhenScrolledOut
#[test]
fn a_pinned_item_stays_composed_and_placed_scrolled_out() {
    let count = mutable_state_of(100usize);
    let mut pins = pins(1, 2, 0, count.handle());
    let _handle = pins.pin();
    assert!(has(&pins.composed, 1));
    pins.scroll_to(3);
    wait_until(&mut pins.ui, missing(&pins.composed, 0));
    assert!(has(&pins.composed, 1));
    pins.assert_placed_out_of_view("1");
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt itemsBetweenPinnedAndCurrentVisibleAreNotComposed
#[test]
fn items_between_a_pinned_one_and_those_in_view_leave() {
    let count = mutable_state_of(100usize);
    let mut pins = pins(1, 2, 0, count.handle());
    let _handle = pins.pin();
    pins.scroll_to(4);
    // Those a round composed too many of leave between frames too (D575).
    let composed = pins.composed.clone();
    wait_until(&mut pins.ui, move || {
        (0..4).filter(|it| composed.borrow().contains(it)).count() == 1
    });
    for (index, expected) in [(0, false), (1, true), (2, false), (3, false), (4, true)] {
        assert_eq!(has(&pins.composed, index), expected, "{index}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinnedItemAfterVisibleOnesIsComposedAndPlacedWhenScrolledOut
#[test]
fn a_pinned_item_after_those_in_view_stays() {
    let count = mutable_state_of(100usize);
    let mut pins = pins(4, 2, 0, count.handle());
    pins.scroll_to(4);
    wait_until(&mut pins.ui, missing(&pins.composed, 1));
    let _handle = pins.pin();
    assert!(has(&pins.composed, 5));
    pins.scroll_to(0);
    let composed = pins.composed.clone();
    wait_until(&mut pins.ui, move || [2, 3, 5].iter().all(|it| !composed.borrow().contains(it)));
    for (index, expected) in [(0, true), (1, true), (2, false), (3, false), (4, true), (5, false)] {
        assert_eq!(has(&pins.composed, index), expected, "{index}");
    }
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinnedItemCanBeUnpinned
#[test]
fn a_pinned_item_can_be_unpinned() {
    let count = mutable_state_of(100usize);
    let mut pins = pins(1, 2, 0, count.handle());
    let handle = pins.pin();
    pins.scroll_to(3);
    wait_until(&mut pins.ui, missing(&pins.composed, 0));
    handle.release();
    wait_until(&mut pins.ui, missing(&pins.composed, 1));
    pins.ui.on_node_with_tag("1").assert_does_not_exist();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinnedItemIsStillPinnedWhenReorderedAndNotVisibleAnymore
#[test]
fn a_pinned_item_reordered_out_of_view_stays_pinned() {
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    let composed: Composed = Rc::default();
    let container: Container = Rc::default();
    let list = mutable_state_of(vec![0usize, 1, 2, 3, 4]);
    let read = list.handle();
    let (noted, kept) = (composed.clone(), container.clone());
    let mut ui = rule(move || {
        let (noted, kept) = (noted.clone(), kept.clone());
        let ids = read.get();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            modifier = Modifier.size(30.dp()),
            state = state,
            move |scope: LazyStaggeredGridScope| {
                let (noted, kept) = (noted.clone(), kept.clone());
                scope.items_list_keyed(
                    ids.clone(),
                    |id: &usize| *id,
                    move |id: &usize| {
                        if *id == 2 {
                            *kept.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                        }
                        Item(*id, noted.clone());
                    },
                );
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(composed.borrow().iter().copied().collect::<Vec<_>>(), [0, 1, 2]);
    let _handle = container.borrow().as_ref().expect("composed").pin();
    ui.wait_for_idle();
    list.set(vec![0, 3, 4, 1, 2]);
    wait_until(&mut ui, missing(&composed, 1));
    assert_eq!(composed.borrow().iter().copied().collect::<Vec<_>>(), [0, 2, 3, 4]);
    ui.on_node_with_tag("2").assert_exists();
}

/// A grid of one lane whose state changes.
fn with_a_new_state(
    pinned: usize,
    initial: usize,
) -> (ComposeTestRule, compose_runtime::OwnedState<LazyStaggeredGridState>, Composed, Container) {
    let owned = LazyStaggeredGridState::new(initial, 0);
    let state = mutable_state_of(*owned);
    std::mem::forget(owned);
    let read = state.handle();
    let composed: Composed = Rc::default();
    let container: Container = Rc::default();
    let (noted, kept) = (composed.clone(), container.clone());
    let mut ui = rule(move || {
        let (noted, kept) = (noted.clone(), kept.clone());
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            modifier = Modifier.size(20.dp()),
            state = read.get(),
            move |scope: LazyStaggeredGridScope| {
                let (noted, kept) = (noted.clone(), kept.clone());
                scope.items(100, move |index| {
                    if index == pinned {
                        *kept.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                    }
                    Item(index, noted.clone());
                });
            }
        );
    });
    ui.wait_for_idle();
    (ui, state, composed, container)
}

fn fresh_state() -> LazyStaggeredGridState {
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    state
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt unpinnedWhenLazyStaggeredGridStateChanges
#[test]
fn a_new_state_unpins() {
    let (mut ui, state, composed, container) = with_a_new_state(2, 2);
    let _handle = container.borrow().as_ref().expect("composed").pin();
    ui.wait_for_idle();
    assert!(has(&composed, 3));
    let current = state.get();
    let _ = run(&mut ui, async move { current.scroll_to_item(0, 0).await });
    wait_until(&mut ui, missing(&composed, 3));
    assert!(has(&composed, 2));
    state.set(fresh_state());
    wait_until(&mut ui, missing(&composed, 2));
    ui.on_node_with_tag("2").assert_does_not_exist();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinAfterLazyStaggeredGridStateChange
#[test]
fn a_pin_after_a_new_state_holds() {
    let (mut ui, state, composed, container) = with_a_new_state(0, 0);
    state.set(fresh_state());
    ui.wait_for_idle();
    let _handle = container.borrow().as_ref().expect("composed").pin();
    ui.wait_for_idle();
    assert!(has(&composed, 1));
    let current = state.get();
    let _ = run(&mut ui, async move { current.scroll_to_item(2, 0).await });
    wait_until(&mut ui, missing(&composed, 1));
    assert!(has(&composed, 0));
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt itemsArePinnedBasedOnGlobalIndexes
#[test]
fn a_pin_goes_by_the_index_in_the_grid() {
    let owned = LazyStaggeredGridState::new(3, 0);
    let state = *owned;
    let composed: Composed = Rc::default();
    let container: Container = Rc::default();
    let (noted, kept) = (composed.clone(), container.clone());
    let mut ui = rule(move || {
        let (noted, kept) = (noted.clone(), kept.clone());
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            modifier = Modifier.size(20.dp()),
            state = state,
            move |scope: LazyStaggeredGridScope| {
                for index in 0..100 {
                    let (noted, kept) = (noted.clone(), kept.clone());
                    scope.item(move || {
                        if index == 3 {
                            *kept.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                        }
                        Item(index, noted.clone());
                    });
                }
            }
        );
    });
    ui.wait_for_idle();
    let _handle = container.borrow().as_ref().expect("composed").pin();
    ui.wait_for_idle();
    assert!(has(&composed, 4));
    let _ = run(&mut ui, async move { state.scroll_to_item(6, 0).await });
    wait_until(&mut ui, missing(&composed, 4));
    assert!(has(&composed, 3));
    assert!(!ui.on_node_with_tag("3").is_displayed());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinnedItemIsRemovedWhenNotVisible
#[test]
fn a_pinned_item_removed_out_of_view_leaves() {
    let count = mutable_state_of(10usize);
    let mut pins = pins(3, 2, 3, count.handle());
    let _handle = pins.pin();
    assert!(has(&pins.composed, 4));
    pins.scroll_to(0);
    wait_until(&mut pins.ui, missing(&pins.composed, 4));
    count.set(3);
    wait_until(&mut pins.ui, missing(&pins.composed, 3));
    pins.ui.on_node_with_tag("3").assert_does_not_exist();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinnedItemIsRemovedWhenVisible
#[test]
fn a_pinned_item_removed_in_view_leaves() {
    let composed: Composed = Rc::default();
    let container: Container = Rc::default();
    let list = mutable_state_of(vec![0usize, 1, 2]);
    let read = list.handle();
    let (noted, kept) = (composed.clone(), container.clone());
    let mut ui = rule(move || {
        let (noted, kept) = (noted.clone(), kept.clone());
        let ids = read.get();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            modifier = Modifier.size(20.dp()),
            move |scope: LazyStaggeredGridScope| {
                let (noted, kept) = (noted.clone(), kept.clone());
                scope.items_list(ids.clone(), move |id: &usize| {
                    if *id == 1 {
                        *kept.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                    }
                    Item(*id, noted.clone());
                });
            }
        );
    });
    ui.wait_for_idle();
    let _handle = container.borrow().as_ref().expect("composed").pin();
    ui.wait_for_idle();
    list.set(vec![0, 2]);
    wait_until(&mut ui, missing(&composed, 1));
    ui.on_node_with_tag("1").assert_does_not_exist();
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinnedMultipleTimes
#[test]
fn a_pin_counts() {
    let count = mutable_state_of(100usize);
    let mut pins = pins(1, 2, 0, count.handle());
    let mut handles = vec![pins.pin(), pins.pin()];
    handles.push(pins.pin());
    assert!(has(&pins.composed, 0));
    pins.scroll_to(3);
    wait_until(&mut pins.ui, missing(&pins.composed, 0));
    while let Some(handle) = handles.pop() {
        pins.ui.wait_for_idle();
        assert!(has(&pins.composed, 1));
        handle.release();
    }
    wait_until(&mut pins.ui, missing(&pins.composed, 1));
}

struct Parent(Rc<Cell<bool>>);

impl PinnableContainer for Parent {
    fn pin(&self) -> PinnedHandle {
        self.0.set(true);
        let pinned = self.0.clone();
        PinnedHandle::new(move || pinned.set(false))
    }
}

fn under(
    parents: Vec<Rc<dyn PinnableContainer>>,
    which: compose_runtime::MutableState<usize>,
    container: Container,
) -> ComposeTestRule {
    let mut ui = rule(move || {
        let kept = container.clone();
        CompositionLocalProvider(
            compose_ui::LOCAL_PINNABLE_CONTAINER.provides(Some(parents[which.get()].clone())),
            move || {
                let kept = kept.clone();
                LazyVerticalStaggeredGrid!(
                    StaggeredGridCells::Fixed(1),
                    modifier = Modifier.size(20.dp()),
                    move |scope: LazyStaggeredGridScope| {
                        let kept = kept.clone();
                        scope.item(move || {
                            *kept.borrow_mut() = compose_ui::LOCAL_PINNABLE_CONTAINER.current();
                            Box!(modifier = Modifier.size(10.dp()));
                        });
                    }
                );
            },
        );
    });
    ui.wait_for_idle();
    ui
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt pinningIsPropagatedToParentContainer
#[test]
fn a_pin_goes_to_the_parent() {
    let pinned = Rc::new(Cell::new(false));
    let parent: Rc<dyn PinnableContainer> = Rc::new(Parent(pinned.clone()));
    let which = mutable_state_of(0usize);
    let container: Container = Rc::default();
    let mut ui = under(vec![parent], which.handle(), container.clone());
    let handle = container.borrow().as_ref().expect("composed").pin();
    ui.wait_for_idle();
    assert!(pinned.get());
    handle.release();
    ui.wait_for_idle();
    assert!(!pinned.get());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridPinnableContainerTest.kt parentContainerChange_pinningIsMaintained
#[test]
fn a_new_parent_takes_over_the_pin() {
    let (one, other) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let first: Rc<dyn PinnableContainer> = Rc::new(Parent(one.clone()));
    let second: Rc<dyn PinnableContainer> = Rc::new(Parent(other.clone()));
    let which = mutable_state_of(0usize);
    let container: Container = Rc::default();
    let mut ui = under(vec![first, second], which.handle(), container.clone());
    let _handle = container.borrow().as_ref().expect("composed").pin();
    ui.wait_for_idle();
    assert!(one.get());
    assert!(!other.get());
    which.set(1);
    ui.wait_for_idle();
    assert!(!one.get());
    assert!(other.get());
}

// Compose: compose/foundation/foundation/integration-tests/lazy-tests/src/androidTest/kotlin/androidx/compose/foundation/lazy/staggeredgrid/LazyStaggeredGridContentPaddingTest.kt pinnedItemWorksIsPlacedOnceInContentPadding
#[test]
fn an_item_focused_in_the_padding_is_placed_once() {
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    let mut ui = rule(move || {
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            modifier = Modifier.size((200.dp(), 400.dp())),
            state = state,
            content_padding = PaddingValues { top: 100.dp(), ..PaddingValues::ZERO },
            |scope: LazyStaggeredGridScope| {
                scope.item(|| {
                    Box!(modifier = Modifier.height(100.dp()).test_tag("field").focusable());
                });
                scope.items(10, |index| {
                    Spacer(Modifier.height(100.dp()).test_tag(index.to_string()));
                });
            }
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    let _ = run(&mut ui, async move { state.scroll_by(50.0).await });
    let top = ui.on_node_with_tag("0").fetch_semantics_snapshot().bounds.top;
    assert_eq!(top, 150.0);
    let _ = run(&mut ui, async move { state.scroll_by(300.0).await });
    assert_eq!(ui.count_nodes(&has_test_tag("field")), 1);
}

fn focusable_grid(count: usize) -> (ComposeTestRule, LazyStaggeredGridState) {
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    let ui = rule(move || {
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(1),
            modifier = Modifier.size(100.dp()),
            state = state,
            move |scope: LazyStaggeredGridScope| {
                scope.items(count, |index| {
                    Box!(
                        modifier =
                            Modifier.height(20.dp()).test_tag(format!("row {index}")).focusable()
                    );
                });
            }
        );
    });
    (ui, state)
}

fn rows_placed(ui: &mut ComposeTestRule) -> usize {
    (0..1000).filter(|index| ui.count_nodes(&has_test_tag(&format!("row {index}"))) == 1).count()
}

/// A focus search leaving the grid asks for the next item beyond its bounds (D477), which takes the
/// focus and is scrolled to; nothing beyond those in view stays placed after.
#[test]
fn tab_lays_out_the_next_item_beyond_the_grid() {
    let (mut ui, state) = focusable_grid(1000);
    assert_eq!(rows_placed(&mut ui), 5);
    for _ in 0..7 {
        ui.press_key(compose_ui::Key::Tab);
    }
    ui.on_node_with_tag("row 6").assert_is_focused();
    assert_eq!(state.first_visible_item_index(), 2);
    assert_eq!(rows_placed(&mut ui), 5);
}

/// `ScrollToIndex`, as much of `LazyStaggeredGridSemanticTest` as there's no `IndexForKey` in.
#[test]
fn a_service_scrolls_to_an_index() {
    let (mut ui, state) = focusable_grid(100);
    ui.on_node(has_scroll_to_index_action()).perform_semantics_action_with(
        &compose_ui::SemanticsActions::SCROLL_TO_INDEX,
        |scroll| {
            scroll(30);
        },
    );
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 30);
    ui.on_node_with_tag("row 30").assert_exists();
    ui.on_node_with_tag("row 29").assert_does_not_exist();
}
