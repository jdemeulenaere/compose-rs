//! Predictive back in a display, against Navigation 3's `NavDisplayPredictiveBackTest`, its scenes
//! moved by hand during a gesture and handed to the pop's animation when it's done.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_navigation::*;
use compose_navigation_event::{
    DirectNavigationEventInput, LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER, NavigationEvent,
};
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

use crate::nav_support::*;

fn event(progress: f32) -> BackEventCompat {
    BackEventCompat {
        swipe_edge: NavigationEvent::EDGE_LEFT,
        progress,
        touch_x: 0.1,
        touch_y: 0.1,
        ..Default::default()
    }
}

fn counting(keys: &[&'static str]) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(keys.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || NavDisplay!(stack, entry_provider = counter_entry));
    (ui, owner)
}

/// Counters set in both entries, then back by what `gesture` does.
fn restored_by(gesture: impl FnOnce(&mut ComposeTestRule)) {
    let (mut ui, stack) = counting(&[FIRST]);
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 0);
    counter(FIRST).set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 2").assert_is_displayed();
    stack.push(SECOND);
    ui.wait_for_idle();
    assert_eq!(counter(SECOND).get(), 0);
    counter(SECOND).set(4);
    ui.wait_for_idle();
    ui.on_node_with_text("second: 4").assert_is_displayed();
    gesture(&mut ui);
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 2, "restored");
    ui.on_node_with_text("first: 2").assert_is_displayed();
    assert_eq!(stack.len(), 1);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayPredictiveBackTest.kt, testStateIsRestoredOnBackPressedPredictiveBack
#[test]
fn a_gesture_through_the_host_restores_what_s_under_it() {
    restored_by(|ui| {
        ui.host().on_back_started(event(0.1));
        ui.host().on_back_progressed(event(0.5));
        ui.wait_for_idle();
        ui.host().on_back_pressed();
    });
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayPredictiveBackTest.kt, testStateIsRestoredOnNavEventPredictiveBack
#[test]
fn a_gesture_through_an_input_restores_what_s_under_it() {
    thread_local! {
        static INPUT: DirectNavigationEventInput = DirectNavigationEventInput::new();
    }
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let dispatcher = LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.current().unwrap();
        remember(move || INPUT.with(|input| dispatcher.add_input(input)));
        NavDisplay!(stack, entry_provider = counter_entry);
    });
    ui.wait_for_idle();
    counter(FIRST).set(2);
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    counter(SECOND).set(4);
    ui.wait_for_idle();
    let nav_event = |progress| NavigationEvent {
        swipe_edge: NavigationEvent::EDGE_LEFT,
        progress,
        touch_x: 0.1,
        touch_y: 0.1,
        ..Default::default()
    };
    INPUT.with(|input| {
        input.back_started(nav_event(0.1));
        input.back_progressed(nav_event(0.5));
    });
    ui.wait_for_idle();
    INPUT.with(|input| input.back_completed());
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 2);
    ui.on_node_with_text("first: 2").assert_is_displayed();
}

#[composable]
fn Clickable(key: &'static str, clicks: Rc<Cell<u32>>) {
    Box!(modifier = Modifier.fill_max_size().clickable(move || clicks.set(clicks.get() + 1)), {
        BasicText!(key)
    });
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayPredictiveBackTest.kt, verifyZIndexAfterInterruptedBackNavigation
#[test]
fn a_pop_interrupted_and_done_again_keeps_what_leaves_on_top() {
    let (clicks_first, clicks_second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let (first, second) = (clicks_first.clone(), clicks_second.clone());
    let mut ui = compose_test(move || {
        let (first, second) = (first.clone(), second.clone());
        NavDisplay!(
            stack,
            entry_provider = move |key: &'static str| {
                let clicks = if key == FIRST { first.clone() } else { second.clone() };
                NavEntry::new(key, move |key| Clickable!(key, clicks.clone()))
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_exists();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_exists();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by(Duration::from_millis(100));
    stack.push(SECOND);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_exists();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by(Duration::from_millis(100));
    ui.on_node_with_text(SECOND).perform_click();
    assert_eq!(clicks_first.get(), 0);
    assert!(clicks_second.get() > 0);
    ui.set_auto_advance(true);
}

/// The scenes move by the gesture's progress, not only by what they start at.
#[test]
fn a_gesture_moves_both_scenes_by_its_progress() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            predictive_pop_transition_spec =
                |_: AnimatedContentTransitionScope<SceneRef<&'static str>>, _: i32| {
                    slide_in_horizontally_with(tween(100).easing(LINEAR_EASING), |width| -width / 2)
                        .together_with(slide_out_horizontally_with(
                            tween(100).easing(LINEAR_EASING),
                            |width| width / 2,
                        ))
                },
            entry_provider = filling_entry
        );
    });
    ui.wait_for_idle();
    ui.host().on_back_started(event(0.5));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    let width = ui.on_root().fetch_semantics_node().bounds.width();
    assert_eq!(left(&mut ui, FIRST), -width / 4.0);
    assert_eq!(left(&mut ui, SECOND), width / 4.0);
    ui.host().on_back_progressed(event(0.0));
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, FIRST), -width / 2.0);
    assert_eq!(left(&mut ui, SECOND), 0.0);
}

/// A key filling the screen, its text at its top left.
#[composable]
fn Filling(key: &'static str) {
    Box!(modifier = Modifier.fill_max_size(), { BasicText!(key) });
}

fn filling_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |key| Filling!(key))
}

fn left(ui: &mut ComposeTestRule, text: &str) -> f32 {
    ui.on_node_with_text(text).fetch_semantics_node().bounds.left
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayPredictiveBackTest.kt, testPredictiveBackDuringForwardAnimation
#[test]
fn a_gesture_during_a_forward_animation_takes_it_and_gives_it_back() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let linear = || tween(1000).easing(LINEAR_EASING);
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            transition_spec =
                move |scope: AnimatedContentTransitionScope<SceneRef<&'static str>>| {
                    scope
                        .slide_into_container_with(SlideDirection::Start, linear(), |offset| offset)
                        .together_with(scope.slide_out_of_container_with(
                            SlideDirection::Start,
                            linear(),
                            |offset| offset,
                        ))
                },
            pop_transition_spec =
                move |scope: AnimatedContentTransitionScope<SceneRef<&'static str>>| {
                    scope
                        .slide_into_container_with(SlideDirection::End, linear(), |offset| offset)
                        .together_with(scope.slide_out_of_container_with(
                            SlideDirection::End,
                            linear(),
                            |offset| offset,
                        ))
                },
            predictive_pop_transition_spec =
                move |scope: AnimatedContentTransitionScope<SceneRef<&'static str>>, _: i32| {
                    scope
                        .slide_into_container_with(SlideDirection::End, linear(), |offset| offset)
                        .together_with(scope.slide_out_of_container_with(
                            SlideDirection::End,
                            linear(),
                            |offset| offset,
                        ))
                },
            entry_provider = filling_entry
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_exists();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(Duration::from_millis(500));
    let second_at_half = left(&mut ui, SECOND);
    assert!(second_at_half > 0.0, "{second_at_half}");
    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.9));
    ui.advance_time_by(Duration::from_millis(500));
    let second_at_gesture = left(&mut ui, SECOND);
    let first_at_gesture = left(&mut ui, FIRST);
    assert!(second_at_gesture > second_at_half, "{second_at_gesture} {second_at_half}");
    assert!(first_at_gesture < 0.0, "{first_at_gesture}");
    ui.host().on_back_cancelled();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_exists();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    assert_eq!(left(&mut ui, SECOND), 0.0);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayPredictiveBackTest.kt, testPredictiveBackHandoff
#[test]
fn a_gesture_completed_hands_its_scenes_to_the_pop_where_they_are() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            pop_transition_spec = |_: AnimatedContentTransitionScope<SceneRef<&'static str>>| {
                slide_in_horizontally_with(tween(1000).easing(LINEAR_EASING), |width| -width / 2)
                    .together_with(slide_out_vertically_with(
                        tween(1000).easing(LINEAR_EASING),
                        |height| height / 2,
                    ))
            },
            predictive_pop_transition_spec =
                |_: AnimatedContentTransitionScope<SceneRef<&'static str>>, _: i32| {
                    slide_in_horizontally_with(tween(100).easing(LINEAR_EASING), |width| -width / 2)
                        .together_with(slide_out_horizontally_with(
                            tween(100).easing(LINEAR_EASING),
                            |width| width / 2,
                        ))
                },
            entry_provider = filling_entry
        );
    });
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.host().on_back_started(event(0.5));
    ui.host().on_back_progressed(event(0.5));
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let (enter_x, exit_x) = (left(&mut ui, FIRST), left(&mut ui, SECOND));
    assert!(exit_x > 0.0, "exit at half: {exit_x}");
    assert!(enter_x < 0.0, "enter at half: {enter_x}");
    ui.host().on_back_pressed();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let (enter_x, exit_x) = (left(&mut ui, FIRST), left(&mut ui, SECOND));
    let exit_y = ui.on_node_with_text(SECOND).fetch_semantics_node().bounds.top;
    assert!(exit_x > 0.0, "exit shouldn't jump back: {exit_x}");
    assert!(enter_x < 0.0, "enter shouldn't jump back: {enter_x}");
    ui.advance_time_by(Duration::from_millis(100));
    let exit_y_later = ui.on_node_with_text(SECOND).fetch_semantics_node().bounds.top;
    assert!(exit_y_later > exit_y, "the pop's own takes over: {exit_y_later} {exit_y}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
}
