//! A scene doing its own transitions with what's inside it: the same scene key, so a display doesn't
//! animate it, against Navigation 3's `TestNestedTransitionScenes.kt`, the part of `AnimatedTest` and
//! of `NavDisplayScreenshotTest` that use it, through bounds rather than screenshots. A gesture's
//! bounds are the goldens': their clone predates `NavDisplay`'s move to `DeferredAnimatedContent`.

use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_navigation::ContentKey;
use compose_navigation::*;
use compose_navigation_event::{
    NavigationBackHandler, NavigationEvent, NavigationEventTransitionState,
    remember_navigation_event_state,
};
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_of};
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

use crate::nav_support::*;

type Key = &'static str;

const CARD: &str = "card";
const CARD_PADDING: i32 = 48;
const DURATION: u32 = 300;
const WIDTH: i32 = 800;
const HEIGHT: i32 = 800;

fn card_entry(key: Key) -> NavEntry<Key> {
    NavEntry::new(key, |key| {
        Box!(
            modifier = Modifier.fill_max_size().test_tag(key),
            content_alignment = Alignment::CENTER,
            {
                Shown!(key);
            }
        );
    })
    .with_metadata(NavMetadata::new().with(CARD, true))
}

/// Keeps every card of the end of the stack in one scene, as `CardStackScene`.
struct CardStackScene {
    entries: Vec<NavEntry<Key>>,
    previous_entries: Vec<NavEntry<Key>>,
    duration: u32,
    on_back: compose_runtime::Callback<dyn Fn()>,
}

impl Scene<Key> for CardStackScene {
    fn key(&self) -> ContentKey {
        ContentKey::new(())
    }

    fn entries(&self) -> &[NavEntry<Key>] {
        &self.entries
    }

    fn previous_entries(&self) -> &[NavEntry<Key>] {
        &self.previous_entries
    }

    fn content(&self) {
        Cards(self.entries.clone(), self.duration, self.on_back.clone());
    }
}

fn card_stack_strategy(duration: u32) -> SceneStrategyRef<Key> {
    scene_strategy(move |scope: &SceneStrategyScope<Key>, entries: &[NavEntry<Key>]| {
        let start = entries
            .iter()
            .rposition(|entry| !entry.metadata().contains_named(CARD))
            .map_or(0, |last_other| last_other + 1);
        if start == entries.len() {
            return None;
        }
        let scene: SceneRef<Key> = Rc::new(CardStackScene {
            entries: entries[start..].to_vec(),
            previous_entries: entries[..entries.len() - 1].to_vec(),
            duration,
            on_back: scope.on_back(),
        });
        Some(scene)
    })
}

#[composable(non_restartable)]
fn Cards(entries: Vec<NavEntry<Key>>, duration: u32, on_back: compose_runtime::Callback<dyn Fn()>) {
    // What's shown: what's popped stays until it's animated out.
    let shown = remember(mutable_state_list_of::<NavEntry<Key>>);
    {
        let entries = entries.clone();
        let keys = entries.iter().map(|entry| entry.content_key().clone()).collect::<Vec<_>>();
        LaunchedEffect(format!("{keys:?}"), async move {
            for entry in entries {
                if !shown.contains(&entry) {
                    shown.push(entry);
                }
            }
        });
    }
    let state = remember_navigation_event_state!(());
    let enabled = entries.len() > 1;
    NavigationBackHandler!(
        state.clone(),
        is_back_enabled = enabled,
        on_back_completed = move || on_back()
    );
    let transition_state = state.transition_state();
    Layout!(
        modifier = Modifier.fill_max_size(),
        measure_policy = measure_policy(|scope, measurables, constraints| {
            let padding = scope.density().round_to_px(CARD_PADDING.dp());
            let placeables: Vec<_> =
                measurables.iter().map(|&card| scope.measure(card, constraints)).collect();
            for (index, placeable) in placeables.iter().enumerate() {
                scope.place(placeable, 0, padding * index as i32);
            }
            let first = placeables.first();
            scope.layout(
                first.map_or(0, |card| card.width),
                first.map_or(0, |card| card.height + padding * placeables.len() as i32),
            )
        }),
        {
            let count = shown.len();
            for (index, entry) in shown.iter_cloned().enumerate() {
                let visible = entries.contains(&entry);
                key(entry.content_key().clone(), || {
                    let of_the_last = if index == count - 1 {
                        transition_state
                    } else {
                        NavigationEventTransitionState::Idle
                    };
                    let (popped, leaving) = (entry.clone(), shown);
                    AnimatedCard(
                        entry.clone(),
                        visible,
                        of_the_last,
                        compose_runtime::callback(move || {
                            leaving.retain(|other| *other != popped);
                        }),
                        duration,
                    );
                });
            }
        }
    );
}

#[composable(non_restartable)]
fn AnimatedCard(
    entry: NavEntry<Key>,
    visible: bool,
    transition_state: NavigationEventTransitionState,
    on_no_longer_visible: compose_runtime::Callback<dyn Fn()>,
    duration: u32,
) {
    let seekable = remember(|| SeekableTransitionState::new(visible));
    let transition = remember_transition(&seekable, "cardStackScene");
    let last_edge = remember(|| mutable_state_of(NavigationEvent::EDGE_RIGHT));
    let held = seekable.clone();
    if let NavigationEventTransitionState::InProgress { latest_event, .. } = transition_state {
        last_edge.set(latest_event.swipe_edge);
        let progress = latest_event.progress;
        LaunchedEffect(progress, async move { held.seek_to_with(progress, false).await });
    } else {
        LaunchedEffect(visible, async move {
            held.animate_to(visible).await;
            if !visible {
                on_no_longer_visible();
            }
        });
    }
    transition.AnimatedVisibility(
        |visible: &bool| *visible,
        Modifier.fill_max_size(),
        None,
        slide_out_with(tween(duration), move |size: IntSize| {
            let x = if last_edge.get() == NavigationEvent::EDGE_RIGHT {
                -size.width
            } else {
                size.width
            };
            IntOffset::new(x, -(size.height / 8))
        }),
        move |_| entry.Content(),
    );
}

fn cards(duration: u32) -> (ComposeTestRule, OwnedStateList<Key>) {
    let owner = compose_runtime::snapshots::mutable_state_list_from([FIRST, SECOND, THIRD]);
    let stack = owner.handle();
    let ui = ComposeTestRule::sized(IntSize::new(WIDTH, HEIGHT), move || {
        let strategies = remember(move || vec![card_stack_strategy(duration)]);
        NavDisplay!(stack, scene_strategies = strategies, entry_provider = card_entry);
    });
    (ui, owner)
}

fn event(progress: f32) -> BackEventCompat {
    BackEventCompat {
        swipe_edge: NavigationEvent::EDGE_LEFT,
        progress,
        touch_x: 0.1,
        touch_y: 0.1,
        ..Default::default()
    }
}

fn gesture_to_half(ui: &mut ComposeTestRule) {
    ui.host().on_back_started(event(0.1));
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
}

fn origin(ui: &mut ComposeTestRule, tag: &str) -> (f32, f32) {
    let bounds = ui.on_node_with_tag(tag).fetch_semantics_node().bounds;
    (bounds.left, bounds.top)
}

/// How far a card's gone at `fraction` of its time, by a tween's curve.
fn moved(fraction: f32) -> (f32, f32) {
    let eased = FAST_OUT_SLOW_IN_EASING.transform(fraction);
    (WIDTH as f32 * eased, -(HEIGHT as f32 / 8.0) * eased)
}

fn assert_near(actual: (f32, f32), expected: (f32, f32), what: &str) {
    assert!(
        (actual.0 - expected.0).abs() <= 1.0 && (actual.1 - expected.1).abs() <= 1.0,
        "{what}: {actual:?}, not {expected:?}"
    );
}

/// The two under the top stay where they are, which is a test that they weren't moved by a display's
/// transition either, as the scene's key doesn't change.
fn assert_under_in_place(ui: &mut ComposeTestRule, cards: usize) {
    let (first, second) = (origin(ui, FIRST), origin(ui, SECOND));
    assert_eq!(second.0, first.0);
    assert_eq!(second.1 - first.1, CARD_PADDING as f32);
    // What's taller than its parent is centred on it, a padding for each of the two cards.
    assert_eq!(first, (0.0, -(CARD_PADDING as f32) * cards as f32 / 2.0));
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testNestedPredictiveBackAnimations
#[test]
fn a_card_goes_by_the_gesture_in_a_scene_that_doesn_t_animate() {
    let (mut ui, stack) = cards(DURATION);
    ui.wait_for_idle();
    for key in [FIRST, SECOND, THIRD] {
        ui.on_node_with_text(key).assert_is_displayed();
    }
    gesture_to_half(&mut ui);
    ui.host().on_back_progressed(event(0.5));
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_exists();
    ui.on_node_with_text(THIRD).assert_is_not_displayed();
    assert_eq!(stack.len(), 3, "a gesture is only that, until it's done");
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_does_not_exist();
    assert_eq!(stack.len(), 2);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNestedPredictiveBackDuringGestureBack
#[test]
fn a_card_is_where_its_gesture_has_it() {
    let (mut ui, _stack) = cards(DURATION);
    ui.wait_for_idle();
    let top = origin(&mut ui, THIRD);
    // What's taller than its parent is centred on it, as a padded stack of full cards is.
    assert_eq!(top, (0.0, 2.0 * CARD_PADDING as f32 - 1.5 * CARD_PADDING as f32));
    gesture_to_half(&mut ui);
    assert_under_in_place(&mut ui, 3);
    let (x, y) = moved(0.5);
    assert_near(origin(&mut ui, THIRD), (top.0 + x, top.1 + y), "at half the gesture");
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNestedPredictiveBackAnimationPostBackPressed
#[test]
fn a_card_goes_on_from_where_its_gesture_left_it() {
    let (mut ui, _stack) = cards(500);
    ui.wait_for_idle();
    let top = origin(&mut ui, THIRD);
    gesture_to_half(&mut ui);
    let at_the_end_of_the_gesture = origin(&mut ui, THIRD);
    ui.set_auto_advance(false);
    ui.host().on_back_pressed();
    // Upstream's 25 ms, and a few more frames for it to be on its way.
    ui.advance_time_by(Duration::from_millis(100));
    // Not a blank screen, nor back at the start.
    assert_under_in_place(&mut ui, 3);
    let now = origin(&mut ui, THIRD);
    assert!(now.0 >= at_the_end_of_the_gesture.0, "{now:?} from {at_the_end_of_the_gesture:?}");
    assert!(now.0 < WIDTH as f32, "{now:?}: it's still leaving");
    assert!(now.1 <= at_the_end_of_the_gesture.1);
    assert!(now.1 > top.1 - HEIGHT as f32 / 8.0);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNestedPredictiveBackAnimationCompleted
#[test]
fn a_gesture_done_leaves_the_cards_under_it() {
    let (mut ui, stack) = cards(DURATION);
    ui.wait_for_idle();
    gesture_to_half(&mut ui);
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    ui.on_node_with_tag(THIRD).assert_does_not_exist();
    assert_under_in_place(&mut ui, 2);
    assert_eq!(stack.len(), 2);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNestedPopAnimationsDuringPop
#[test]
fn a_card_popped_leaves_over_its_duration() {
    let (mut ui, _stack) = cards(DURATION);
    ui.wait_for_idle();
    let top = origin(&mut ui, THIRD);
    ui.set_auto_advance(false);
    ui.host().on_back_pressed();
    ui.advance_time_by(Duration::from_millis(u64::from(DURATION) / 2));
    assert_under_in_place(&mut ui, 3);
    // A back press isn't from the left edge: it goes the default way.
    // The frames that a change of target takes, at most half its time.
    let now = origin(&mut ui, THIRD);
    let (at_least, at_most) = (moved(0.37), moved(0.5));
    assert!(-now.0 >= at_least.0 && -now.0 <= at_most.0, "{now:?}");
    assert!(now.1 - top.1 <= at_least.1 && now.1 - top.1 >= at_most.1, "{now:?}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag(THIRD).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayScreenshotTest.kt, testNestedPopAnimationCompleted
#[test]
fn a_card_popped_leaves_the_cards_under_it() {
    let (mut ui, stack) = cards(DURATION);
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    ui.on_node_with_tag(THIRD).assert_does_not_exist();
    assert_under_in_place(&mut ui, 2);
    assert_eq!(stack.len(), 2);
}
