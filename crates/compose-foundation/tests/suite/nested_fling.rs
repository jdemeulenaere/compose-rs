//! Flings handed on: a scrollable flings with what one inside it left (`onPostFling`), a fling
//! whose node left hands on all it had, and the connections around hear of it.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::gestures::{FlingBehavior, LocalBoxFuture, ScrollScope};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, NestedScrollConnection, NestedScrollSource, Offset, UnitExt};

use crate::scroll_support::swipe;

fn sized(content: impl Fn() + 'static) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(300, 300), content)
}

/// Ten 200 dp boxes in a scrolling column.
#[composable]
fn Boxes(state: ScrollState) {
    Column!(modifier = Modifier.vertical_scroll(state), {
        for _ in 0..10 {
            Box!(modifier = Modifier.size(200.dp()));
        }
    });
}

/// What a connection heard.
#[derive(Default)]
struct Heard {
    pre_side_effect: Cell<f32>,
    pre_fling: Cell<Option<f32>>,
    post_fling: Cell<Option<(f32, f32)>>,
    /// Counts the fling deltas only once this one heard its fling end.
    after: RefCell<Option<Rc<Heard>>>,
}

impl NestedScrollConnection for Heard {
    fn on_pre_scroll(&self, available: Offset, source: NestedScrollSource) -> Offset {
        let counting =
            self.after.borrow().as_ref().is_none_or(|after| after.post_fling.get().is_some());
        if source == NestedScrollSource::SideEffect && counting {
            self.pre_side_effect.set(self.pre_side_effect.get() + available.y);
        }
        Offset::ZERO
    }

    fn on_pre_fling(&self, available: Offset) -> Offset {
        self.pre_fling.set(Some(available.y));
        Offset::ZERO
    }

    fn on_post_fling(&self, consumed: Offset, available: Offset) -> Offset {
        self.post_fling.set(Some((consumed.y, available.y)));
        Offset::ZERO
    }
}

/// Scrolls 100 px, as the state's way, and gives back all it got.
#[derive(Default)]
struct ScrollsAHundred {
    velocities: RefCell<Vec<f32>>,
}

impl FlingBehavior for ScrollsAHundred {
    fn perform_fling<'a>(
        &'a self,
        scope: ScrollScope,
        initial_velocity: f32,
    ) -> LocalBoxFuture<'a, f32> {
        self.velocities.borrow_mut().push(initial_velocity);
        Box::pin(async move {
            scope.scroll_by(100.0);
            initial_velocity
        })
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedFling_parentShouldFlingWithVelocityLeft
#[test]
fn a_parent_flings_with_the_velocity_a_child_left() {
    let owner = ScrollState::new(1000);
    let column = *owner;
    let top = Rc::new(Heard::default());
    let middle = Rc::new(Heard::default());
    *top.after.borrow_mut() = Some(middle.clone());
    let fling = Rc::new(ScrollsAHundred::default());
    let parent = scrollable_state(|_| 0.0);
    let (around, inner, behavior) = (top.clone(), middle.clone(), fling.clone());
    let mut ui = sized(move || {
        let behavior: Rc<dyn FlingBehavior> = behavior.clone();
        let (inner, parent) = (inner.clone(), parent.clone());
        Box!(
            modifier = Modifier.nested_scroll(around.clone()).scrollable_with(
                parent,
                Orientation::Vertical,
                true,
                false,
                Some(behavior),
                None
            ),
            {
                Box!(modifier = Modifier.nested_scroll(inner.clone()), {
                    Boxes(column);
                });
            }
        );
    });
    swipe(&mut ui, Offset::new(100.0, 10.0), Offset::new(100.0, 290.0), 100);
    ui.wait_for_idle();
    assert_eq!(column.value(), 0, "the column is at its start");
    let (_, left) = middle.post_fling.get().expect("the column's fling ended");
    assert_ne!(left, 0.0, "with velocity left");
    assert_eq!(*fling.velocities.borrow(), vec![left], "the parent got it");
    assert_eq!(top.pre_side_effect.get(), 100.0, "its fling's deltas go through its parents");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedFling_parentShouldFlingWithVelocityLeft_whenInnerDisappears
#[test]
fn a_fling_whose_node_left_hands_on_its_velocity() {
    let owner = ScrollState::new(0);
    let column = *owner;
    let heard = Rc::new(Heard::default());
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let around = heard.clone();
    let mut ui = sized(move || {
        Box!(modifier = Modifier.size(300.dp()).nested_scroll(around.clone()), {
            if handle.get() {
                Boxes(column);
            }
        });
    });
    ui.set_auto_advance(false);
    swipe(&mut ui, Offset::new(100.0, 290.0), Offset::new(100.0, 10.0), 100);
    assert!(column.is_scroll_in_progress(), "flinging");
    ui.advance_time_by_frame();
    shown.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(!column.is_scroll_in_progress(), "it ended at its next delta");
    let preflung = heard.pre_fling.get().expect("it started");
    let (consumed, available) = heard.post_fling.get().expect("the parents heard");
    assert_eq!(consumed + available, preflung);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedFling_shouldCancelWhenHitTheBounds_ifRemoved
#[test]
fn a_fling_whose_node_left_sends_no_more_deltas() {
    let owner = ScrollState::new(1000);
    let column = *owner;
    let heard = Rc::new(Heard::default());
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let around = heard.clone();
    let mut ui = sized(move || {
        Box!(modifier = Modifier.nested_scroll(around.clone()), {
            if handle.get() {
                Boxes(column);
            }
        });
    });
    ui.set_auto_advance(false);
    swipe(&mut ui, Offset::new(100.0, 10.0), Offset::new(100.0, 290.0), 100);
    let read = heard.clone();
    ui.advance_time_until(move |_| read.pre_side_effect.get().abs() > 280.0);
    shown.set(false);
    ui.advance_time_by_frame();
    heard.pre_side_effect.set(0.0);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(heard.pre_side_effect.get(), 0.0);
}

/// Takes all a scroll leaves, as what's around a list that goes on.
#[derive(Default)]
struct TakesTheRest {
    side_effect: Cell<f32>,
}

impl NestedScrollConnection for TakesTheRest {
    fn on_post_scroll(&self, _: Offset, available: Offset, source: NestedScrollSource) -> Offset {
        if source == NestedScrollSource::SideEffect {
            self.side_effect.set(self.side_effect.get() + available.y);
        }
        available
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollableTest.kt, scrollable_nestedFling_shouldContinueSendingDeltasWhenHitBounds
#[test]
fn a_fling_at_its_bounds_goes_on_sending_deltas() {
    let owner = ScrollState::new(1000);
    let column = *owner;
    let connection = Rc::new(TakesTheRest::default());
    let around = connection.clone();
    let mut ui = sized(move || {
        Box!(modifier = Modifier.nested_scroll(around.clone()), {
            Boxes(column);
        });
    });
    ui.set_auto_advance(false);
    swipe(&mut ui, Offset::new(100.0, 10.0), Offset::new(100.0, 290.0), 100);
    ui.advance_time_until(move |_| column.value() == 0);
    connection.side_effect.set(0.0);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_ne!(connection.side_effect.get(), 0.0);
}
