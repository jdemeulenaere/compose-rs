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
    let width = ui.on_root().fetch_semantics_snapshot().bounds.width();
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
    ui.on_node_with_text(text).fetch_semantics_snapshot().bounds.left
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
    let exit_y = ui.on_node_with_text(SECOND).fetch_semantics_snapshot().bounds.top;
    assert!(exit_x > 0.0, "exit shouldn't jump back: {exit_x}");
    assert!(enter_x < 0.0, "enter shouldn't jump back: {enter_x}");
    ui.advance_time_by(Duration::from_millis(100));
    let exit_y_later = ui.on_node_with_text(SECOND).fetch_semantics_snapshot().bounds.top;
    assert!(exit_y_later > exit_y, "the pop's own takes over: {exit_y_later} {exit_y}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
}

type Scope = AnimatedContentTransitionScope<SceneRef<&'static str>>;

fn alphas(ui: &mut ComposeTestRule) -> Vec<f32> {
    let mut alphas: Vec<f32> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
            _ => None,
        })
        .collect();
    alphas.sort_by(f32::total_cmp);
    alphas
}

fn scales(ui: &mut ComposeTestRule) -> Vec<f32> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::Transform { scale, .. } => Some(scale.0),
            _ => None,
        })
        .collect()
}

/// An entry's own predictive transition goes before the display's.
#[test]
fn an_entry_s_predictive_transition_goes_first() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            predictive_pop_transition_spec = |_: Scope, _: i32| {
                EnterTransition::NONE.together_with(slide_out_horizontally_with(
                    tween(100).easing(LINEAR_EASING),
                    |width| -width / 2,
                ))
            },
            entry_provider = |key: &'static str| {
                let entry = filling_entry(key);
                if key == SECOND {
                    entry.with_metadata(NavDisplay::predictive_pop_transition_spec(
                        |_: Scope, edge: i32| {
                            let sign = if edge == NavigationEvent::EDGE_LEFT { 1 } else { -1 };
                            Some(EnterTransition::NONE.together_with(slide_out_horizontally_with(
                                tween(100).easing(LINEAR_EASING),
                                move |width| sign * width / 2,
                            )))
                        },
                    ))
                } else {
                    entry
                }
            }
        );
    });
    ui.wait_for_idle();
    let width = ui.on_root().fetch_semantics_snapshot().bounds.width();
    ui.host().on_back_started(event(0.5));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, SECOND), width / 4.0, "the entry's, by the edge");
}

/// The display's own default: the entering scene fades along a spring, read linearly, and the
/// leaving one scales to 0.7, by progress.
#[test]
fn a_gesture_fades_and_scales_by_default() {
    let (mut ui, _stack) = {
        let owner = mutable_state_list_from([FIRST, SECOND]);
        let stack = owner.handle();
        (compose_test(move || NavDisplay!(stack, entry_provider = filling_entry)), owner)
    };
    ui.wait_for_idle();
    ui.host().on_back_started(event(0.5));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    assert_eq!(alphas(&mut ui), [0.5], "only what enters fades, linearly");
    assert_eq!(scales(&mut ui), [0.85], "halfway to 0.7");
}

/// A spec that isn't a spring is read along its easing.
#[test]
fn a_gesture_follows_its_spec_s_easing() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            predictive_pop_transition_spec = |_: Scope, _: i32| {
                slide_in_horizontally_with(tween(100), |width| -width)
                    .together_with(ExitTransition::NONE)
            },
            entry_provider = filling_entry
        );
    });
    ui.wait_for_idle();
    let width = ui.on_root().fetch_semantics_snapshot().bounds.width();
    ui.host().on_back_started(event(0.5));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    let eased = FAST_OUT_SLOW_IN_EASING.transform(0.5);
    assert_eq!(left(&mut ui, FIRST), (-width * (1.0 - eased)).round());
}

/// A fade in and out, a scale in: each goes the way it goes, from the start to the end.
#[test]
fn a_gesture_moves_each_part_of_its_transform_its_way() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            predictive_pop_transition_spec = |_: Scope, _: i32| {
                let linear = || tween(100).easing(LINEAR_EASING);
                (fade_in_with(linear(), 0.0)
                    + scale_in_with(linear(), 0.5, TransformOrigin::CENTER))
                .together_with(fade_out_with(linear(), 0.0))
            },
            entry_provider = filling_entry
        );
    });
    ui.wait_for_idle();
    ui.host().on_back_started(event(0.25));
    ui.host().on_back_progressed(event(0.25));
    ui.wait_for_idle();
    assert_eq!(alphas(&mut ui), [0.25, 0.75]);
    assert_eq!(scales(&mut ui), [0.625], "a quarter of the way from 0.5 to 1");
}

/// During a gesture, what's going back to is under what's there: a click takes the current one.
#[test]
fn a_gesture_keeps_what_s_there_on_top() {
    let (clicks_first, clicks_second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let (first, second) = (clicks_first.clone(), clicks_second.clone());
    let mut ui = compose_test(move || {
        let (first, second) = (first.clone(), second.clone());
        NavDisplay!(
            stack,
            predictive_pop_transition_spec =
                |_: Scope, _: i32| { EnterTransition::NONE.together_with(ExitTransition::NONE) },
            entry_provider = move |key: &'static str| {
                let clicks = if key == FIRST { first.clone() } else { second.clone() };
                NavEntry::new(key, move |key| Clickable!(key, clicks.clone()))
            }
        );
    });
    ui.wait_for_idle();
    ui.host().on_back_started(event(0.5));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).perform_click();
    assert_eq!(clicks_first.get(), 0);
    assert_eq!(clicks_second.get(), 1);
}

#[composable]
fn Tagged(key: &'static str) {
    Box!(modifier = Modifier.fill_max_size().test_tag(key), { Shown!(key) });
}

fn tagged_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |key| Tagged!(key))
}

fn left_of(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds.left
}

/// The gesture's half way at the end of the steps upstream's tests make.
fn half_a_gesture(ui: &mut ComposeTestRule, edge: i32) {
    let event = |progress| BackEventCompat { swipe_edge: edge, ..event(progress) };
    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
}

fn slide_in(by: impl Fn(i32) -> i32 + 'static) -> EnterTransition {
    slide_in_horizontally_with(spring(), by)
}

fn slide_out(by: impl Fn(i32) -> i32 + 'static) -> ExitTransition {
    slide_out_horizontally_with(spring(), by)
}

/// `edge`'s test: `first` under `second`, each moved by `spec`.
fn sliding_by_gesture(
    edge: i32,
    spec: impl Fn(Scope, i32) -> ContentTransform + Copy + 'static,
) -> (f32, f32) {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 2400), move || {
        NavDisplay!(
            stack,
            transition_spec = |_: Scope| {
                slide_in(|width| width / 2).together_with(slide_out(|width| -width / 2))
            },
            predictive_pop_transition_spec = spec,
            entry_provider = tagged_entry
        );
    });
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    half_a_gesture(&mut ui, edge);
    (left_of(&mut ui, FIRST), left_of(&mut ui, SECOND))
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNavDisplayPredictiveBackAnimations
#[test]
fn a_gesture_slides_both_scenes_by_its_progress() {
    let at = sliding_by_gesture(NavigationEvent::EDGE_LEFT, |_, _| {
        slide_in(|width| -width / 2).together_with(slide_out(|width| width / 2))
    });
    // A spring has no curve to follow, so they go linearly: half of a half.
    assert_eq!(at, (-270.0, 270.0));
}

/// What's popped goes the way of `edge`, and what's under it doesn't move.
fn by_edge(_: Scope, edge: i32) -> ContentTransform {
    EnterTransition::NONE.together_with(if edge == NavigationEvent::EDGE_LEFT {
        slide_out(|width| width / 2)
    } else {
        slide_out(|width| -width / 2)
    })
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNavDisplayPredictiveBackAnimationsRightSwipeEdge
#[test]
fn a_gesture_from_the_right_edge_slides_left() {
    assert_eq!(sliding_by_gesture(NavigationEvent::EDGE_RIGHT, by_edge), (0.0, -270.0));
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNavDisplayPredictiveBackAnimationsLeftSwipeEdge
#[test]
fn an_entry_s_gesture_slides_by_its_metadata_from_the_edge() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(1080, 2400), move || {
        NavDisplay!(
            stack,
            transition_spec = |_: Scope| {
                slide_in(|width| width / 2).together_with(slide_out(|width| -width / 2))
            },
            entry_provider = |key: &'static str| {
                let entry = tagged_entry(key);
                if key == SECOND {
                    entry.with_metadata(NavDisplay::predictive_pop_transition_spec(
                        |scope: Scope, edge: i32| Some(by_edge(scope, edge)),
                    ))
                } else {
                    entry
                }
            }
        );
    });
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    half_a_gesture(&mut ui, NavigationEvent::EDGE_LEFT);
    assert_eq!((left_of(&mut ui, FIRST), left_of(&mut ui, SECOND)), (0.0, 270.0));
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testSharedElementCorrectlyDisplayedOnPredictiveBack
#[test]
fn a_shared_entry_stays_unscaled_in_the_overlay_on_predictive_back_and_expands_on_pop() {
    const RED_FILL: Color = Color::from_rgba(255, 77, 77, 255);
    const BLUE_FILL: Color = Color::from_rgba(51, 51, 255, 255);

    #[composable]
    fn ColoredBox(text: &'static str, fill: Color) {
        Box!(
            modifier = Modifier.fill_max_size().background(fill).test_tag(text),
            content_alignment = Alignment::CENTER,
            { BasicText!(text, modifier = Modifier.size(50.dp())) }
        );
    }

    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let seen_scope: Rc<std::cell::RefCell<Option<SharedTransitionScope>>> = Rc::default();
    let scope_in = seen_scope.clone();

    let mut ui = ComposeTestRule::sized(IntSize::new(1280, 800), move || {
        let scope_out = scope_in.clone();
        SharedTransitionLayout!(move |scope: SharedTransitionScope| {
            *scope_out.borrow_mut() = Some(scope.clone());
            let back_stack = stack;
            let list_detail = remember_list_detail_scene_strategy();
            NavDisplay!(
                back_stack,
                modifier = Modifier.test_tag("NavHostTag"),
                on_back = Some(move || {
                    back_stack.remove(back_stack.len() - 1);
                }),
                scene_strategies = vec![list_detail],
                shared_transition_scope = Some(scope),
                entry_provider =
                    entry_provider(|provider: &mut EntryProviderScope<&'static str>| {
                        provider
                            .entry(FIRST, |_| ColoredBox!(FIRST, RED_FILL))
                            .with_metadata(ListDetailScene::list_pane());
                        provider
                            .entry(SECOND, |_| ColoredBox!(SECOND, BLUE_FILL))
                            .with_metadata(ListDetailScene::detail_pane());
                    })
            );
        });
    });

    ui.wait_for_idle();
    // Before predictive back: 40/60 split across 1280x800 (`FIRST` 0..512, `SECOND` 512..1280).
    let first_before = ui.on_node_with_tag(FIRST).fetch_semantics_node().bounds_in_root();
    let second_before = ui.on_node_with_tag(SECOND).fetch_semantics_node().bounds_in_root();
    assert_eq!(first_before, Rect::new(0.0, 0.0, 512.0, 800.0));
    assert_eq!(second_before, Rect::new(512.0, 0.0, 1280.0, 800.0));

    // Start predictive back to progress 0.2 (outgoing scene scales by 1.0 - 0.2 * 0.3 = 0.94
    // around center (640, 400), while `FIRST`'s `SharedEntryInSceneNavEntryDecorator` sets
    // `permitTransformDuringDeferredTransition = false` so `FIRST` renders in the overlay unscaled).
    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.2));
    ui.wait_for_idle();

    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();

    // `SECOND` (no match in `SinglePaneScene`) stays in `ListDetailScene` and is scaled by 0.94
    // around (640, 400): width = 768 * 0.94 = 721.92, height = 800 * 0.94 = 752.0.
    let second_during = ui.on_node_with_tag(SECOND).fetch_semantics_node().bounds_in_root();
    assert!((second_during.width() - 768.0 * 0.94).abs() < 1.5, "{second_during:?}");
    assert!((second_during.height() - 800.0 * 0.94).abs() < 1.5, "{second_during:?}");

    // Check the display list: `RED_FILL` (`FIRST`) is drawn last in the `SharedTransitionLayout`
    // overlay at full 512x800 (unscaled), while `ListDetailScene` has a 0.94 scale transform.
    let ops = ui.host().display_list().ops;
    assert!(
        ops.iter().any(|op| matches!(
            op,
            compose_ui::platform::DrawOp::Transform { scale: (sx, sy), .. }
                if (*sx - 0.94).abs() < 0.01 && (*sy - 0.94).abs() < 0.01
        )),
        "outgoing scene scaled by 0.94: {ops:?}"
    );
    let red_rects: Vec<Rect> = ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::DrawRect(rect, paint) if paint.color == RED_FILL => {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        red_rects,
        vec![Rect::new(0.0, 0.0, 512.0, 800.0)],
        "shared entry drawn once in overlay at unscaled 512x800"
    );

    // Completing predictive back hands `FIRST` off to `SinglePaneScene` and animates its bounds
    // from 512x800 out to the full 1280x800 display.
    ui.set_auto_advance(false);
    ui.host().on_back_pressed();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    assert!(seen_scope.borrow().as_ref().unwrap().is_transition_active());
    let first_mid = ui.on_node_with_tag(FIRST).fetch_semantics_node().bounds_in_root();
    assert!(
        first_mid.width() > 512.0 && first_mid.width() < 1280.0,
        "animating between 512 and 1280: {first_mid:?}"
    );

    ui.set_auto_advance(true);
    ui.wait_for_idle();
    let first_after = ui.on_node_with_tag(FIRST).fetch_semantics_node().bounds_in_root();
    assert_eq!(first_after, Rect::new(0.0, 0.0, 1280.0, 800.0));
}
