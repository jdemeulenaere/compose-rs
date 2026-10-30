//! A display's transitions, against Navigation 3's `AnimatedTest`. `testSharedElement` waits for
//! shared elements, and `testNestedPredictiveBackAnimations` for its `CardStackSceneStrategy`, a
//! scene seeking its own transitions: `nav_predictive_back.rs` has a display's own.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_lifecycle::*;
use compose_navigation::*;
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

use crate::nav_support::*;

/// `DEFAULT_TRANSITION_DURATION_MILLISECOND`, in `NavDisplay.android.kt`.
const DEFAULT: u64 = 700;

fn millis(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

thread_local! {
    static LIFECYCLES: RefCell<HashMap<&'static str, Lifecycle>> = RefCell::default();
}

/// What's under `key`'s lifecycle now.
fn state_of(key: &'static str) -> LifecycleState {
    LIFECYCLES.with_borrow(|all| all[key].current_state())
}

#[composable]
fn Noting(key: &'static str) {
    let lifecycle = LOCAL_LIFECYCLE_OWNER.current();
    LIFECYCLES.with_borrow_mut(|all| all.insert(key, lifecycle));
    BasicTextOf(key);
}

#[composable]
fn BasicTextOf(key: &'static str) {
    Box!({ Shown!(key) });
}

/// An entry noting its lifecycle, as most of the tests'.
fn noting_entry(key: &'static str) -> NavEntry<&'static str> {
    NavEntry::new(key, |key| Noting!(key))
}

fn noting(keys: &[&'static str]) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(keys.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || NavDisplay!(stack, entry_provider = noting_entry));
    (ui, owner)
}

type TransitionScope = AnimatedContentTransitionScope<SceneRef<&'static str>>;

/// Short enough that a test past it is past no default transition.
const SHORT: u32 = DEFAULT as u32 / 5;

fn past_short() -> Duration {
    millis(u64::from(SHORT) * 3 / 2)
}

#[derive(Clone, Copy)]
enum Kind {
    Push,
    Pop,
}

/// A display of `keys` where `custom_key` has a transition of its own lasting `duration`.
fn custom(
    keys: &[&'static str],
    custom_key: &'static str,
    kind: Kind,
    duration: u32,
) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(keys.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || {
        NavDisplay!(
            stack,
            entry_provider = move |key: &'static str| {
                let entry = text_entry(key);
                if key != custom_key {
                    return entry;
                }
                let spec = move |_: TransitionScope| {
                    Some(ContentTransform::new(
                        fade_in_with(tween(duration), 0.0),
                        fade_out_with(tween(duration), 0.0),
                    ))
                };
                entry.with_metadata(match kind {
                    Kind::Push => NavDisplay::transition_spec(spec),
                    Kind::Pop => NavDisplay::pop_transition_spec(spec),
                })
            }
        );
    });
    (ui, owner)
}

static LOCAL_PROVIDED: CompositionLocal<bool> = CompositionLocal::new("LocalProvided", || false);

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testNavigateAnimations
#[test]
fn going_forward_starts_both_then_resumes_the_new() {
    let (mut ui, stack) = noting(&[FIRST]);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert_eq!(state_of(FIRST), LifecycleState::Resumed);
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(millis(DEFAULT / 2));
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Started);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Resumed);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testNavigateInterruptedAnimations
#[test]
fn a_key_added_during_a_transition_interrupts_it() {
    let (mut ui, stack) = noting(&[FIRST]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(millis(DEFAULT / 3));
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Started);
    stack.push(THIRD);
    ui.advance_time_by(millis(DEFAULT / 20));
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Started);
    ui.on_node_with_text(THIRD).assert_exists();
    assert_eq!(state_of(THIRD), LifecycleState::Started);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_exists();
    assert_eq!(state_of(THIRD), LifecycleState::Resumed);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPopInterruptedAnimations
#[test]
fn a_pop_during_a_pop_interrupts_it() {
    let (mut ui, stack) = noting(&[FIRST, SECOND, THIRD]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(2);
    ui.advance_time_by(millis(DEFAULT / 3));
    ui.on_node_with_text(THIRD).assert_exists();
    assert_eq!(state_of(THIRD), LifecycleState::Created);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Started);
    stack.remove(1);
    ui.advance_time_by(millis(DEFAULT / 20));
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Created);
    ui.on_node_with_text(THIRD).assert_exists();
    assert_eq!(state_of(THIRD), LifecycleState::Created);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_exists();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_does_not_exist();
    assert_eq!(state_of(FIRST), LifecycleState::Resumed);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPopNavigateInterruptedAnimations
#[test]
fn a_key_added_during_a_pop_interrupts_it() {
    let (mut ui, stack) = noting(&[FIRST, SECOND]);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by(millis(DEFAULT / 3));
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Created);
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    stack.push(THIRD);
    ui.advance_time_by(millis(DEFAULT / 20));
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Created);
    ui.on_node_with_text(THIRD).assert_exists();
    assert_eq!(state_of(THIRD), LifecycleState::Started);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_exists();
    assert_eq!(state_of(THIRD), LifecycleState::Resumed);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testNavigateAnimationsImmutableBackStack
#[test]
fn a_new_list_with_one_more_key_animates_to_it() {
    let state = mutable_state_of(1);
    let read = state.handle();
    let mut ui = compose_test(move || {
        let stack = if read.get() == 1 { vec![FIRST] } else { vec![FIRST, SECOND] };
        NavDisplay!(stack, entry_provider = noting_entry);
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert_eq!(state_of(FIRST), LifecycleState::Resumed);
    ui.set_auto_advance(false);
    state.set(2);
    ui.advance_time_by(millis(DEFAULT / 2));
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Started);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Resumed);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPopAnimationsImmutableBackStack
#[test]
fn a_new_list_with_one_key_less_animates_to_it() {
    let state = mutable_state_of(1);
    let read = state.handle();
    let mut ui = compose_test(move || {
        let stack = if read.get() == 1 { vec![FIRST, SECOND] } else { vec![FIRST] };
        NavDisplay!(stack, entry_provider = noting_entry);
    });
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(false);
    state.set(2);
    ui.advance_time_by(millis(DEFAULT / 2));
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testNavigateAnimationsCustom
#[test]
fn an_entry_s_transition_replaces_the_display_s() {
    let (mut ui, stack) = custom(&[FIRST], SECOND, Kind::Push, DEFAULT as u32 * 2);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(millis(DEFAULT * 3 / 2));
    ui.on_node_with_text(FIRST).assert_exists();
    ui.on_node_with_text(SECOND).assert_exists();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_exists();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPopAnimationsCustom
#[test]
fn an_entry_s_pop_transition_replaces_the_display_s() {
    let (mut ui, stack) = custom(&[FIRST, SECOND], SECOND, Kind::Pop, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPopMultiple
#[test]
fn a_pop_of_several_uses_the_last_one_s_transition() {
    let (mut ui, stack) = custom(&[FIRST, SECOND, THIRD], THIRD, Kind::Pop, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(2);
    stack.remove(1);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPopNavigate
#[test]
fn a_pop_and_an_add_at_once_isn_t_a_pop() {
    let (mut ui, stack) = custom(&[FIRST, SECOND], THIRD, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    stack.push(THIRD);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testCentrePop
#[test]
fn a_pop_in_the_middle_changes_nothing_shown() {
    let (mut ui, stack) = custom(&[FIRST, SECOND, THIRD], THIRD, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testCentreNavigate
#[test]
fn an_add_in_the_middle_changes_nothing_shown() {
    let (mut ui, stack) = custom(&[FIRST, THIRD], THIRD, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.insert(1, SECOND);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testCentrePopAndEndPop
#[test]
fn a_pop_in_the_middle_and_at_the_end_isn_t_a_pop() {
    let (mut ui, stack) = custom(&[FIRST, SECOND, THIRD, FOURTH], THIRD, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(FOURTH).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(3);
    stack.remove(1);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.on_node_with_text(FOURTH).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testCentrePopAndEndNavigate
#[test]
fn a_pop_in_the_middle_and_an_add_at_the_end_go_forward() {
    let (mut ui, stack) = custom(&[FIRST, SECOND, THIRD], FOURTH, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    stack.push(FOURTH);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_does_not_exist();
    ui.on_node_with_text(FOURTH).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testCentreNavigateAndEndPop
#[test]
fn an_add_in_the_middle_and_a_pop_at_the_end_isn_t_a_pop() {
    let (mut ui, stack) = custom(&[FIRST, THIRD, FOURTH], THIRD, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(FOURTH).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(2);
    stack.insert(1, SECOND);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.on_node_with_text(FOURTH).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testCentreNavigateAndEndNavigate
#[test]
fn an_add_in_the_middle_and_at_the_end_go_forward() {
    let (mut ui, stack) = custom(&[FIRST, THIRD], FOURTH, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.insert(1, SECOND);
    stack.push(FOURTH);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_does_not_exist();
    ui.on_node_with_text(FOURTH).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testSameStack
#[test]
fn a_pop_and_the_same_add_change_nothing() {
    let (mut ui, stack) = custom(&[FIRST, SECOND], SECOND, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    stack.push(SECOND);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPoppedEntryIsAnimated
#[test]
fn a_popped_entry_stays_shown_while_it_animates() {
    let (mut ui, stack) = noting(&[FIRST, SECOND]);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPoppedEntryIsWrapped
#[test]
fn a_popped_entry_stays_decorated_while_it_animates() {
    let wrapped = Rc::new(Cell::new(false));
    let seen = wrapped.clone();
    let decorator = NavEntryDecorator::new(|entry: NavEntry<&'static str>| {
        CompositionLocalProvider(LOCAL_PROVIDED.provides(true), move || entry.Content());
    });
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        NavDisplay!(
            stack,
            entry_decorators = vec![decorator.clone()],
            entry_provider = move |key: &'static str| {
                let seen = seen.clone();
                NavEntry::new(key, move |key| {
                    if key == SECOND {
                        seen.set(LOCAL_PROVIDED.current());
                    }
                    Shown!(key);
                })
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(wrapped.get());
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testDuplicateLastEntry
#[test]
fn a_key_added_again_at_the_end_animates_to_it() {
    let (mut ui, stack) = custom(&[FIRST, SECOND, THIRD], SECOND, Kind::Push, SHORT);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.set_auto_advance(false);
    stack.push(SECOND);
    ui.advance_time_by(past_short());
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(THIRD).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testPopLifecycle
#[test]
fn a_pop_caps_what_leaves_at_created() {
    let (mut ui, stack) = noting(&[FIRST, SECOND]);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert_eq!(state_of(SECOND), LifecycleState::Resumed);
    ui.set_auto_advance(false);
    stack.remove(1);
    ui.advance_time_by(millis(DEFAULT / 2));
    ui.on_node_with_text(FIRST).assert_exists();
    assert_eq!(state_of(FIRST), LifecycleState::Started);
    ui.on_node_with_text(SECOND).assert_exists();
    assert_eq!(state_of(SECOND), LifecycleState::Created);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    assert_eq!(state_of(FIRST), LifecycleState::Resumed);
    ui.on_node_with_text(SECOND).assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testSwapStack
#[test]
fn another_stack_animates_with_its_entry_s_transition() {
    let (one, two) = (mutable_state_list_from([FIRST]), mutable_state_list_from([SECOND]));
    let stacks = [one.handle(), two.handle()];
    let state = mutable_state_of(0);
    let read = state.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stacks[read.get()],
            entry_provider = |key: &'static str| {
                let entry = text_entry(key);
                if key == SECOND {
                    entry.with_metadata(NavDisplay::transition_spec(|_: TransitionScope| {
                        Some(ContentTransform::new(
                            slide_in_horizontally_with(tween(300), |width| width / 2),
                            slide_out_horizontally_with(tween(300), |width| -width / 2),
                        ))
                    }))
                } else {
                    entry
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.set_auto_advance(false);
    state.set(1);
    ui.advance_time_by(millis(150));
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testLocalAnimatedContentScopeDoesNotThrowInOverlays
#[test]
fn the_animated_scope_can_be_read_in_an_overlay() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = |key: &'static str| {
                if key == SECOND {
                    NavEntry::new(key, |key| {
                        let _ = LOCAL_NAV_ANIMATED_CONTENT_SCOPE.current();
                        Shown!(key);
                    })
                    .with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
                } else {
                    text_entry(key)
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/AnimatedTest.kt, testInterruptedConsecutivePop
#[test]
fn a_pop_interrupted_by_another_stays_a_pop() {
    let (forward, pop) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (counted_forward, counted_pop) = (forward.clone(), pop.clone());
    let owner = mutable_state_list_from([FIRST, SECOND, THIRD]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let (counted_forward, counted_pop) = (counted_forward.clone(), counted_pop.clone());
        NavDisplay!(
            stack,
            transition_spec = move |scope: TransitionScope| {
                counted_forward.set(counted_forward.get() + 1);
                default_transition_spec()(scope)
            },
            pop_transition_spec = move |scope: TransitionScope| {
                counted_pop.set(counted_pop.get() + 1);
                default_pop_transition_spec()(scope)
            },
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    forward.set(0);
    pop.set(0);
    stack.remove(2);
    ui.set_auto_advance(false);
    ui.advance_time_by(millis(DEFAULT / 2));
    stack.remove(1);
    ui.advance_time_by(millis(16));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(forward.get(), 0, "not a transition forward");
    assert!(pop.get() >= 2, "{}", pop.get());
}
