//! Review of foundation's scrolling milestone 1: a press stops any scroll (`startDragImmediately`),
//! a parent scrolling takes nothing a child leaves (`performRawScroll`), drags end with their
//! node, the dominant axis of a wheel turn, `can_scroll_*` as state.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::gestures::{LocalBoxFuture, ScrollBlock};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    IntSize, MutableInteractionSource, NestedScrollConnection, NestedScrollSource, Offset,
    PointerEvent, UnitExt,
};

use crate::scroll_support::{Touch, launch};

const TAG: &str = "scroller";
const CENTER: Offset = Offset::new(50.0, 50.0);

/// A state adding up its deltas, all of them consumed.
fn total_state() -> (DefaultScrollableState, Rc<Cell<f32>>) {
    let total = Rc::new(Cell::new(0.0f32));
    let into = total.clone();
    let state = scrollable_state(move |delta| {
        into.set(into.get() + delta);
        delta
    });
    (state, total)
}

/// A state taking nothing, always at an edge.
fn edge_state() -> DefaultScrollableState {
    scrollable_state(|_| 0.0)
}

fn sized(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(300, 300), content)
}

/// A wheel turn at the center of the 100 dp box.
fn wheel(ui: &mut ComposeTestRule, delta: Offset) {
    ui.host().send_pointer(PointerEvent::scroll(CENTER, delta));
    ui.wait_for_idle();
}

/// Runs a scroll of `priority` that never ends on `state`, until it's interrupted.
fn held_scroll(
    state: impl ScrollableState + 'static,
    priority: MutatePriority,
) -> Rc<RefCell<Option<Result<(), MutationInterrupted>>>> {
    let result: Rc<RefCell<Option<Result<(), MutationInterrupted>>>> = Rc::default();
    let into = result.clone();
    launch(async move {
        *into.borrow_mut() = Some(state.scroll(priority, |_| std::future::pending::<()>()).await);
    });
    result
}

/// The three scrollers, over a 100 dp box of `content`, as a state's scrolled pixels to read.
#[derive(Clone, Copy, Debug)]
enum Scroller {
    Scrollable,
    VerticalScroll,
    Lazy,
}

const SCROLLERS: [Scroller; 3] = [Scroller::Scrollable, Scroller::VerticalScroll, Scroller::Lazy];

/// What a test drives: the state, and how far it has scrolled.
struct Driven {
    state: Rc<dyn ScrollableState>,
    scrolled: Rc<dyn Fn() -> f32>,
    source: Option<MutableInteractionSource>,
}

/// `kind` filling a 100 dp box tagged [`TAG`], `inner` inside it, while `shown` is true (a lazy
/// list has `inner` as its first item, then items to scroll).
fn scroller(
    kind: Scroller,
    shown: MutableState<bool>,
    inner: impl Fn() + Clone + 'static,
) -> (ComposeTestRule, Driven) {
    match kind {
        Scroller::Scrollable => {
            let (state, total) = total_state();
            let source = MutableInteractionSource::new();
            let (given, source_given) = (state.clone(), source.clone());
            let ui = sized(move || {
                if shown.get() {
                    let inner = inner.clone();
                    Box!(
                        modifier = Modifier.test_tag(TAG).size(100.dp()).scrollable_with(
                            given.clone(),
                            Orientation::Vertical,
                            true,
                            false,
                            None,
                            Some(source_given.clone()),
                        ),
                        {
                            inner();
                        }
                    );
                }
            });
            let scrolled = Rc::new(move || total.get());
            (ui, Driven { state: Rc::new(state), scrolled, source: Some(source) })
        }
        Scroller::VerticalScroll => {
            let owner = Rc::new(ScrollState::new(0));
            let state = **owner;
            let keep = owner.clone();
            let ui = sized(move || {
                let _ = &keep;
                if shown.get() {
                    let inner = inner.clone();
                    Column!(
                        modifier = Modifier.test_tag(TAG).size(100.dp()).vertical_scroll(state),
                        {
                            inner();
                            Box!(modifier = Modifier.size((100.dp(), 1000.dp())));
                        }
                    );
                }
            });
            let scrolled = Rc::new(move || state.value() as f32);
            let source = Some(state.interaction_source());
            (ui, Driven { state: Rc::new(state), scrolled, source })
        }
        Scroller::Lazy => {
            let owner = Rc::new(LazyListState::new(0, 0));
            let state = **owner;
            let keep = owner.clone();
            let ui = sized(move || {
                let _ = &keep;
                if shown.get() {
                    let inner = inner.clone();
                    LazyColumn!(
                        modifier = Modifier.test_tag(TAG).size(100.dp()),
                        state = state,
                        move |scope: LazyListScope| {
                            let inner = inner.clone();
                            scope.item(inner);
                            scope.items(100, |_| Spacer(Modifier.size((100.dp(), 50.dp()))));
                        }
                    );
                }
            });
            let scrolled = Rc::new(move || state.first_visible_item_scroll_offset() as f32);
            let source = Some(state.interaction_source());
            (ui, Driven { state: Rc::new(state), scrolled, source })
        }
    }
}

/// `state` through the trait object the driven scroller gives.
struct ByRef(Rc<dyn ScrollableState>);

impl ScrollableState for ByRef {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        self.0.scroll_boxed(priority, block)
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.0.dispatch_raw_delta(delta)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.0.is_scroll_in_progress()
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/LazyListState.kt, canScrollForward and canScrollBackward are states, false before the first layout
#[test]
fn what_reads_can_scroll_is_composed_again_when_it_changes() {
    let owner = LazyListState::new(0, 0);
    let state = *owner;
    let seen: Rc<RefCell<Vec<(bool, bool)>>> = Rc::default();
    let into = seen.clone();
    let mut ui = sized(move || {
        into.borrow_mut().push((state.can_scroll_forward(), state.can_scroll_backward()));
        LazyColumn!(modifier = Modifier.size(100.dp()), state = state, |scope: LazyListScope| {
            scope.items(4, |_| Spacer(Modifier.size((100.dp(), 50.dp()))));
        });
    });
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), vec![(false, false), (true, false)]);
    state.dispatch_raw_delta(10.0);
    ui.wait_for_idle();
    assert_eq!(seen.borrow().last(), Some(&(true, true)));
    state.dispatch_raw_delta(1000.0);
    ui.wait_for_idle();
    assert_eq!(seen.borrow().last(), Some(&(false, true)));
    let compositions = seen.borrow().len();
    state.dispatch_raw_delta(-10.0);
    ui.wait_for_idle();
    state.dispatch_raw_delta(-10.0);
    ui.wait_for_idle();
    assert_eq!(seen.borrow().len(), compositions + 1, "only a change composes it again");
}

// The same through a grid, whose measurement is its own.
#[test]
fn what_reads_a_grids_can_scroll_is_composed_again_when_it_changes() {
    let owner = LazyGridState::new(0, 0);
    let state = *owner;
    let seen: Rc<RefCell<Vec<(bool, bool)>>> = Rc::default();
    let into = seen.clone();
    let mut ui = sized(move || {
        into.borrow_mut().push((state.can_scroll_forward(), state.can_scroll_backward()));
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size(100.dp()),
            state = state,
            |scope: LazyGridScope| {
                scope.items(8, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), vec![(false, false), (true, false)]);
    state.dispatch_raw_delta(1000.0);
    ui.wait_for_idle();
    assert_eq!(seen.borrow().last(), Some(&(false, true)));
}

// An empty list can't scroll, and says so again after having items.
#[test]
fn a_list_that_empties_can_no_longer_scroll() {
    let owner = LazyListState::new(0, 0);
    let state = *owner;
    let count = mutable_state_of(4usize);
    let handle = count.handle();
    let mut ui = sized(move || {
        let count = handle.get();
        LazyColumn!(
            modifier = Modifier.size(100.dp()),
            state = state,
            move |scope: LazyListScope| {
                scope.items(count, |_| Spacer(Modifier.size((100.dp(), 50.dp()))));
            }
        );
    });
    ui.wait_for_idle();
    assert!(state.can_scroll_forward());
    count.set(0);
    ui.wait_for_idle();
    assert!(!state.can_scroll_forward() && !state.can_scroll_backward());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AbstractScrollableNode.kt, startDragImmediately is the scroll logic's shouldScrollImmediately: isScrollInProgress
#[test]
fn a_press_stops_a_scroll_going_on_and_drags_at_once() {
    for kind in SCROLLERS {
        let shown = mutable_state_of(true);
        let (mut ui, driven) = scroller(kind, shown.handle(), || {});
        let result = held_scroll(ByRef(driven.state.clone()), MutatePriority::Default);
        ui.wait_for_idle();
        assert!(driven.state.is_scroll_in_progress(), "{kind:?}");
        let start = (driven.scrolled)();
        let mut finger = Touch::down(&mut ui, CENTER);
        ui.wait_for_idle();
        assert_eq!(
            *result.borrow(),
            Some(Err(MutationInterrupted)),
            "{kind:?}: the press stops it"
        );
        // Less than the touch slop: the drag has started.
        finger.move_by(&mut ui, Offset::new(0.0, -3.0));
        ui.wait_for_idle();
        assert_ne!((driven.scrolled)(), start, "{kind:?}: no slop to pass");
        assert!(driven.state.is_scroll_in_progress(), "{kind:?}: dragging");
        finger.up(&mut ui);
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AbstractScrollableNode.kt, CanDragCalculation comes before startDragImmediately: a mouse doesn't drag, so it stops nothing
#[test]
fn a_mouse_press_stops_nothing() {
    for kind in SCROLLERS {
        let shown = mutable_state_of(true);
        let (mut ui, driven) = scroller(kind, shown.handle(), || {});
        let result = held_scroll(ByRef(driven.state.clone()), MutatePriority::Default);
        ui.wait_for_idle();
        ui.on_node_with_tag(TAG).perform_mouse_input(|mouse| {
            mouse.move_to(Offset::new(50.0, 50.0));
            mouse.press(MouseButton::Primary);
        });
        assert_eq!(*result.borrow(), None, "{kind:?}: it goes on");
        assert!(driven.state.is_scroll_in_progress(), "{kind:?}");
        ui.on_node_with_tag(TAG).perform_mouse_input(|mouse| mouse.release(MouseButton::Primary));
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt, ScrollingLogic.performRawScroll takes nothing while the state is scrolling
#[test]
fn a_parent_scrolling_takes_nothing_a_child_leaves() {
    for kind in SCROLLERS {
        let shown = mutable_state_of(true);
        let edge = edge_state();
        let inner = edge.clone();
        let (mut ui, driven) = scroller(kind, shown.handle(), move || {
            Box!(
                modifier = Modifier.size(100.dp()).scrollable(inner.clone(), Orientation::Vertical)
            );
        });
        let start = (driven.scrolled)();
        // Unheld, it does take it (the test can tell).
        wheel(&mut ui, Offset::new(0.0, 10.0));
        ui.wait_for_idle();
        let moved = (driven.scrolled)();
        assert_ne!(moved, start, "{kind:?}: what the child leaves scrolls the parent");
        let result = held_scroll(ByRef(driven.state.clone()), MutatePriority::Default);
        ui.wait_for_idle();
        wheel(&mut ui, Offset::new(0.0, 10.0));
        ui.wait_for_idle();
        assert_eq!((driven.scrolled)(), moved, "{kind:?}: its own scroll goes on");
        assert_eq!(*result.borrow(), None, "{kind:?}");
    }
}

// A node leaving under a finger ends its drag's scroll and tells its source, each of the three
// (the state lives on with its owner).
#[test]
fn a_drag_ends_when_its_node_leaves() {
    for kind in SCROLLERS {
        let shown = mutable_state_of(true);
        let (mut ui, driven) = scroller(kind, shown.handle(), || {});
        let mut finger = Touch::down(&mut ui, CENTER);
        finger.move_by(&mut ui, Offset::new(0.0, -50.0));
        assert!(driven.state.is_scroll_in_progress(), "{kind:?}: dragging");
        let source = driven.source.clone().expect("a source");
        assert!(source.is_dragged(), "{kind:?}");
        shown.set(false);
        ui.wait_for_idle();
        assert!(!driven.state.is_scroll_in_progress(), "{kind:?}: the scroll ended");
        assert!(!source.is_dragged(), "{kind:?}: the source heard");
        finger.up(&mut ui);
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt, a drag released emits DragInteraction.Stop, one cancelled (the gesture cancelled, the node gone) DragInteraction.Cancel
#[test]
fn a_drag_tells_its_source_how_it_ended() {
    for kind in SCROLLERS {
        // Released.
        let shown = mutable_state_of(true);
        let (mut ui, driven) = scroller(kind, shown.handle(), || {});
        let source = driven.source.clone().expect("a source");
        let recorded = crate::interaction_helpers::record(&source);
        let mut finger = Touch::down(&mut ui, CENTER);
        finger.move_by(&mut ui, Offset::new(0.0, -50.0));
        finger.up(&mut ui);
        ui.wait_for_idle();
        assert_eq!(crate::interaction_helpers::kinds(&recorded), ["Start", "Stop"], "{kind:?}");

        // The gesture cancelled.
        let recorded = crate::interaction_helpers::record(&source);
        ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(Offset::new(0.0, -50.0));
            touch.cancel();
        });
        ui.wait_for_idle();
        assert_eq!(
            crate::interaction_helpers::kinds(&recorded),
            ["Start", "DragCancel"],
            "{kind:?}: cancelled"
        );
        assert!(!source.is_dragged(), "{kind:?}");

        // The node gone.
        let recorded = crate::interaction_helpers::record(&source);
        let mut finger = Touch::down(&mut ui, CENTER);
        finger.move_by(&mut ui, Offset::new(0.0, -50.0));
        shown.set(false);
        ui.wait_for_idle();
        assert_eq!(
            crate::interaction_helpers::kinds(&recorded),
            ["Start", "DragCancel"],
            "{kind:?}: removed"
        );
        assert!(!source.is_dragged(), "{kind:?}");
        finger.up(&mut ui);
    }
}

// A scroller in a list's item, released when its item leaves for good: a drag going on in it ends,
// its state not left scrolling.
#[test]
fn a_drag_ends_when_its_item_is_released() {
    let outer_owner = LazyListState::new(0, 0);
    let outer = *outer_owner;
    let (inner, _) = total_state();
    let given = inner.clone();
    let mut ui = sized(move || {
        let given = given.clone();
        LazyColumn!(
            modifier = Modifier.size(100.dp()),
            state = outer,
            move |scope: LazyListScope| {
                let given = given.clone();
                scope.items(100, move |index| {
                    let modifier = Modifier.size((100.dp(), 100.dp()));
                    if index == 0 {
                        Box!(modifier = modifier.scrollable(given.clone(), Orientation::Vertical));
                    } else {
                        Box!(modifier = modifier);
                    }
                });
            }
        );
    });
    let mut finger = Touch::down(&mut ui, CENTER);
    finger.move_by(&mut ui, Offset::new(0.0, -50.0));
    assert!(inner.is_scroll_in_progress(), "dragging");
    outer.request_scroll_to_item(50, 0);
    ui.wait_for_idle();
    assert_eq!(outer.first_visible_item_index(), 50);
    assert!(!inner.is_scroll_in_progress(), "the item left, the drag with it");
    finger.up(&mut ui);
}

// The same scroller on every item, the slots of the items leaving reused by those coming: the
// node stays, and its reset ends the drag it had (the states are the items' own).
#[test]
fn a_drag_ends_when_its_node_is_reset_for_another_item() {
    let outer_owner = LazyListState::new(0, 0);
    let outer = *outer_owner;
    let states: Vec<(DefaultScrollableState, Rc<Cell<f32>>)> =
        (0..100).map(|_| total_state()).collect();
    let first = states[0].0.clone();
    let states = Rc::new(states);
    let resets = Rc::new(Cell::new(0));
    let counted = resets.clone();
    let mut ui = sized(move || {
        let (states, counted) = (states.clone(), counted.clone());
        LazyColumn!(
            modifier = Modifier.size(100.dp()),
            state = outer,
            move |scope: LazyListScope| {
                let (states, counted) = (states.clone(), counted.clone());
                scope.items(100, move |index| {
                    let counted = counted.clone();
                    DisposableEffect(index, move |scope| {
                        counted.set(counted.get() + 1);
                        scope.on_dispose(|| {})
                    });
                    let state = states[index].0.clone();
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 100.dp()))
                            .scrollable(state, Orientation::Vertical)
                    );
                });
            }
        );
    });
    let mut finger = Touch::down(&mut ui, CENTER);
    finger.move_by(&mut ui, Offset::new(0.0, -50.0));
    assert!(first.is_scroll_in_progress(), "dragging");
    for index in 1..6 {
        outer.request_scroll_to_item(index, 0);
        ui.wait_for_idle();
    }
    assert!(!first.is_scroll_in_progress(), "the item left, the drag with it");
    finger.up(&mut ui);
}

// Each scroller in a list's item, kept deactivated when the item leaves (no item of its content
// type comes to reuse it): nothing updates the node, so its reset is what ends the drag. On
// reuse by another state, the element's update ends it first.
#[test]
fn a_drag_ends_when_its_node_is_deactivated() {
    for kind in SCROLLERS {
        let outer_owner = LazyListState::new(0, 0);
        let outer = *outer_owner;
        let (scrollable, _) = total_state();
        let column = ScrollState::new(0);
        let lazy = LazyListState::new(0, 0);
        let (column_state, lazy_state) = (*column, *lazy);
        let inner: Rc<dyn ScrollableState> = match kind {
            Scroller::Scrollable => Rc::new(scrollable.clone()),
            Scroller::VerticalScroll => Rc::new(column_state),
            Scroller::Lazy => Rc::new(lazy_state),
        };
        let mut ui = sized(move || {
            let scrollable = scrollable.clone();
            LazyColumn!(
                modifier = Modifier.size(100.dp()),
                state = outer,
                move |scope: LazyListScope| {
                    let scrollable = scrollable.clone();
                    scope.items_with(
                        100,
                        |index| index == 0,
                        move |index| {
                            let size = Modifier.size((100.dp(), 100.dp()));
                            if index != 0 {
                                Box!(modifier = size);
                                return;
                            }
                            match kind {
                                Scroller::Scrollable => Box!(
                                    modifier =
                                        size.scrollable(scrollable.clone(), Orientation::Vertical)
                                ),
                                Scroller::VerticalScroll => {
                                    Column!(modifier = size.vertical_scroll(column_state), {
                                        Box!(modifier = Modifier.size((100.dp(), 1000.dp())));
                                    });
                                }
                                Scroller::Lazy => LazyColumn!(
                                    modifier = size,
                                    state = lazy_state,
                                    |scope: LazyListScope| {
                                        scope.items(100, |_| {
                                            Spacer(Modifier.size((100.dp(), 50.dp())))
                                        });
                                    }
                                ),
                            }
                        },
                    );
                }
            );
        });
        let mut finger = Touch::down(&mut ui, CENTER);
        finger.move_by(&mut ui, Offset::new(0.0, -50.0));
        assert!(inner.is_scroll_in_progress(), "{kind:?}: dragging");
        for index in 1..6 {
            outer.request_scroll_to_item(index, 0);
            ui.wait_for_idle();
        }
        assert!(!inner.is_scroll_in_progress(), "{kind:?}: the item left, the drag with it");
        finger.up(&mut ui);
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/lazy/LazySemantics.kt, the scrollBy action returns true whatever it's asked
#[test]
fn a_lists_scroll_by_is_handled_with_nothing_to_do() {
    let owner = LazyListState::new(0, 0);
    let state = *owner;
    let mut ui = sized(move || {
        LazyColumn!(
            modifier = Modifier.test_tag(TAG).size(100.dp()),
            state = state,
            |scope: LazyListScope| {
                scope.items(100, |_| Spacer(Modifier.size((100.dp(), 50.dp()))));
            }
        );
    });
    let node = ui.on_node_with_tag(TAG).fetch_semantics_node();
    assert!(ui.host().perform_semantics_action_with(
        node.id,
        &compose_ui::SemanticsActions::SCROLL_BY,
        |scroll| scroll(0.0, 0.0)
    ));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedDiagonalScroll_mouseWheel_triggersOnAngle
#[test]
fn a_diagonal_wheel_turn_scrolls_the_scrollable_of_its_dominant_axis() {
    let (vertical, down) = total_state();
    let (horizontal, across) = total_state();
    let mut ui = sized(move || {
        let horizontal = horizontal.clone();
        Box!(
            modifier = Modifier.size(100.dp()).scrollable(vertical.clone(), Orientation::Vertical),
            {
                Box!(
                    modifier = Modifier
                        .size(100.dp())
                        .scrollable(horizontal.clone(), Orientation::Horizontal)
                );
            }
        );
    });
    wheel(&mut ui, Offset::new(-100.0, -50.0));
    assert!(across.get() > 0.0, "mostly horizontal");
    assert_eq!(down.get(), 0.0);
    across.set(0.0);
    down.set(0.0);
    wheel(&mut ui, Offset::new(-10.0, -50.0));
    assert!(down.get() > 0.0, "mostly vertical");
    assert_eq!(across.get(), 0.0);
}

// The same on one scrollable: the other axis's dominant turn doesn't move it.
#[test]
fn a_turn_dominated_by_the_other_axis_does_not_scroll() {
    let (state, total) = total_state();
    let given = state.clone();
    let mut ui = sized(move || {
        Box!(modifier = Modifier.size(100.dp()).scrollable(given.clone(), Orientation::Vertical));
    });
    wheel(&mut ui, Offset::new(-100.0, -50.0));
    assert_eq!(total.get(), 0.0);
    // Equal: vertical, as `abs(y) >= abs(x)`.
    wheel(&mut ui, Offset::new(-50.0, -50.0));
    assert_eq!(total.get(), 50.0);
}

// Mouse and touch are separate pointers: a finger lifting while the mouse is held ends the drag.
#[test]
fn a_finger_lifting_does_not_hand_the_drag_to_a_held_mouse() {
    for kind in SCROLLERS {
        let shown = mutable_state_of(true);
        let (mut ui, driven) = scroller(kind, shown.handle(), || {});
        let mut finger = Touch::down(&mut ui, CENTER);
        finger.move_by(&mut ui, Offset::new(0.0, -50.0));
        assert!(driven.state.is_scroll_in_progress(), "{kind:?}");
        // By the host: the harness's mouse ends a touch gesture, as Compose's dispatcher does.
        ui.host().send_pointer(PointerEvent::press(CENTER));
        finger.advance_event_time(&mut ui, 100);
        finger.up(&mut ui);
        ui.wait_for_idle();
        let lifted = (driven.scrolled)();
        ui.host().send_pointer(PointerEvent::moved(CENTER + Offset::new(0.0, -30.0), true));
        ui.wait_for_idle();
        assert_eq!((driven.scrolled)(), lifted, "{kind:?}: the mouse doesn't drag it");
        assert!(!driven.state.is_scroll_in_progress(), "{kind:?}: the drag ended");
        ui.host().send_pointer(PointerEvent::release(CENTER + Offset::new(0.0, -30.0)));
    }
}

/// An app's own state: its `scroll` runs the block only after `wait` (what a state may do).
struct LateState {
    inner: DefaultScrollableState,
    wait: Rc<Cell<bool>>,
}

impl ScrollableState for LateState {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        let wait = self.wait.clone();
        Box::pin(async move {
            if wait.get() {
                compose_runtime::with_frame_nanos(|_| ()).await;
            }
            self.inner.scroll_boxed(priority, block).await
        })
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.inner.dispatch_raw_delta(delta)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.inner.is_scroll_in_progress()
    }
}

impl ComposeArg for LateRc {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
struct LateRc(Rc<LateState>);

impl ScrollableState for LateRc {
    fn scroll_boxed<'a>(
        &'a self,
        priority: MutatePriority,
        block: ScrollBlock<'a>,
    ) -> LocalBoxFuture<'a, Result<(), MutationInterrupted>> {
        self.0.scroll_boxed(priority, block)
    }

    fn dispatch_raw_delta(&self, delta: f32) -> f32 {
        self.0.dispatch_raw_delta(delta)
    }

    fn is_scroll_in_progress(&self) -> bool {
        self.0.is_scroll_in_progress()
    }
}

// A state that runs its block a frame late still gets the turn: the wheel's a scroll in a task.
#[test]
fn a_wheel_turn_on_an_apps_state_is_not_lost_when_its_block_runs_later() {
    let (inner, total) = total_state();
    let state = LateRc(Rc::new(LateState { inner, wait: Rc::new(Cell::new(true)) }));
    let given = state.clone();
    let mut ui = sized(move || {
        Box!(modifier = Modifier.size(100.dp()).scrollable(given.clone(), Orientation::Vertical));
    });
    wheel(&mut ui, Offset::new(0.0, -10.0));
    assert_eq!(total.get(), 10.0, "applied when the block ran");
    assert!(!state.is_scroll_in_progress());
}

// A delta's consumer panicking in a wheel's scroll leaves the state's mutex free: the task's
// future goes with the panic.
#[test]
fn a_panic_in_a_wheel_turn_on_an_apps_state_leaves_its_mutex_free() {
    let panics = Rc::new(Cell::new(false));
    let armed = panics.clone();
    let inner = scrollable_state(move |delta| {
        assert!(!armed.get(), "the delta's consumer panics");
        delta
    });
    let state = LateRc(Rc::new(LateState { inner, wait: Rc::new(Cell::new(false)) }));
    let given = state.clone();
    let mut ui = sized(move || {
        Box!(modifier = Modifier.size(100.dp()).scrollable(given.clone(), Orientation::Vertical));
    });
    panics.set(true);
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.host().send_pointer(PointerEvent::scroll(CENTER, Offset::new(0.0, -10.0)));
    }));
    assert!(unwound.is_err());
    panics.set(false);
    assert!(!state.is_scroll_in_progress(), "its scroll ended");
}

/// Takes everything before the scroll while `greedy`.
struct PreScroll {
    greedy: Cell<bool>,
    in_progress: RefCell<Option<Box<dyn Fn() -> bool>>>,
    /// `is_scroll_in_progress` when it heard the fling.
    at_pre_fling: Cell<Option<bool>>,
}

impl NestedScrollConnection for PreScroll {
    fn on_pre_scroll(&self, available: Offset, _: NestedScrollSource) -> Offset {
        if self.greedy.get() { available } else { Offset::ZERO }
    }

    fn on_pre_fling(&self, _: Offset) -> LocalBoxFuture<'_, Offset> {
        let in_progress = self.in_progress.borrow();
        self.at_pre_fling.set(in_progress.as_ref().map(|in_progress| in_progress()));
        Box::pin(async { Offset::ZERO })
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt, performScroll calls scrollBy with what pre-scroll left, zero too, so lastScrolled* follow
#[test]
fn a_scroll_the_pre_scroll_took_whole_notes_no_direction() {
    let (state, _) = total_state();
    let connection = Rc::new(PreScroll {
        greedy: Cell::new(false),
        in_progress: RefCell::default(),
        at_pre_fling: Cell::new(None),
    });
    let (given, around) = (state.clone(), connection.clone());
    let mut ui = sized(move || {
        Box!(
            modifier = Modifier
                .size(100.dp())
                .nested_scroll(around.clone())
                .scrollable(given.clone(), Orientation::Vertical)
        );
    });
    wheel(&mut ui, Offset::new(0.0, -10.0));
    assert!(state.last_scrolled_forward());
    connection.greedy.set(true);
    wheel(&mut ui, Offset::new(0.0, -10.0));
    assert!(!state.last_scrolled_forward() && !state.last_scrolled_backward());
}

// D880: the drag's scroll has ended by the time what follows it runs (the connections hearing of
// the fling, the fling's own scroll), whatever order the executor runs tasks in.
#[test]
fn a_drags_scroll_has_ended_when_its_fling_is_told() {
    let (state, _) = total_state();
    let connection = Rc::new(PreScroll {
        greedy: Cell::new(false),
        in_progress: RefCell::default(),
        at_pre_fling: Cell::new(None),
    });
    let read = state.clone();
    *connection.in_progress.borrow_mut() = Some(Box::new(move || read.is_scroll_in_progress()));
    let (given, around) = (state.clone(), connection.clone());
    let mut ui = sized(move || {
        Box!(
            modifier = Modifier
                .size(100.dp())
                .nested_scroll(around.clone())
                .scrollable(given.clone(), Orientation::Vertical)
        );
    });
    let mut finger = Touch::down(&mut ui, CENTER);
    finger.move_by(&mut ui, Offset::new(0.0, -50.0));
    finger.up(&mut ui);
    assert_eq!(connection.at_pre_fling.get(), Some(false));
}
