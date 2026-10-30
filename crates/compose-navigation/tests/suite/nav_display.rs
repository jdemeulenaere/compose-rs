//! `NavDisplay`, against Navigation 3's `NavDisplayTest`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::SizeTransform;
use compose_foundation::layout::*;
use compose_foundation::{LazyColumn, LazyListScope};
use compose_navigation::*;
use compose_navigation_event::{
    DirectNavigationEventInput, LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER, NavigationEvent,
    TestNavigationEventDispatcherOwner, remember_navigation_event_dispatcher_owner,
};
use compose_runtime::snapshots::{OwnedStateList, mutable_state_list_from};
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Alignment, BackHandler, DialogProperties, Modifier, UnitExt};

use crate::nav_support::*;

fn panic_text(body: impl FnOnce()) -> String {
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
        .expect_err("it should have panicked");
    match payload.downcast::<String>() {
        Ok(text) => *text,
        Err(payload) => payload.downcast::<&str>().map(|text| text.to_string()).unwrap_or_default(),
    }
}

/// A display of `keys`, its stack the test's, showing each key.
fn showing(keys: &[&'static str]) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(keys.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || NavDisplay!(stack, entry_provider = text_entry));
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testContentShown
#[test]
fn a_display_shows_its_entry() {
    let (mut ui, _stack) = showing(&[FIRST]);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testContentChanged
#[test]
fn a_key_added_shows_instead() {
    let (mut ui, stack) = showing(&[FIRST]);
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_does_not_exist();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testDialog
#[test]
fn a_dialog_shows_over_the_entry_before_it() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = |key: &'static str| {
                let entry = text_entry(key);
                if key == SECOND {
                    entry.with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
                } else {
                    entry
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testOnBack
#[test]
fn back_pops_the_stack() {
    let (mut ui, stack) = showing(&[FIRST]);
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
    assert_eq!(stack.len(), 1);
    assert!(!ui.host().on_back_pressed(), "nothing's before the last");
}

/// A display of counters: each entry's own, saved, by its key.
fn counting(keys: &[&'static str]) -> (ComposeTestRule, OwnedStateList<&'static str>) {
    let owner = mutable_state_list_from(keys.to_vec());
    let stack = owner.handle();
    let ui = compose_test(move || NavDisplay!(stack, entry_provider = counter_entry));
    (ui, owner)
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testStateOfInactiveContentIsRestoredWhenWeGoBackToIt
#[test]
fn what_an_entry_saved_comes_back_with_it() {
    let (mut ui, stack) = counting(&[FIRST]);
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 0);
    counter(FIRST).set(1);
    stack.push(SECOND);
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 1);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testStateIsRemovedOnPop
#[test]
fn what_a_popped_entry_saved_is_forgotten() {
    let (mut ui, stack) = counting(&[FIRST]);
    ui.wait_for_idle();
    counter(FIRST).set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 2").assert_is_displayed();
    stack.push(SECOND);
    ui.wait_for_idle();
    assert_eq!(counter(SECOND).get(), 0);
    counter(SECOND).set(4);
    ui.wait_for_idle();
    ui.on_node_with_text("second: 4").assert_is_displayed();
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 2, "restored");
    ui.on_node_with_text("first: 2").assert_is_displayed();
    stack.push(SECOND);
    ui.wait_for_idle();
    assert_eq!(counter(SECOND).get(), 0, "forgotten");
    ui.on_node_with_text("second: 0").assert_is_displayed();
}

thread_local! {
    static REGISTRIES: RefCell<Vec<(&'static str, SaveableStateRegistry)>> =
        const { RefCell::new(Vec::new()) };
}

fn note_registry(name: &'static str) {
    let registry = LOCAL_SAVEABLE_STATE_REGISTRY.current().expect("a registry");
    REGISTRIES.with_borrow_mut(|registries| registries.push((name, registry)));
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testIndividualSavedStateRegistries
#[test]
fn each_entry_has_a_registry_of_its_own() {
    REGISTRIES.with_borrow_mut(Vec::clear);
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        note_registry("main");
        NavDisplay!(stack, entry_provider = |key: &'static str| NavEntry::new(key, note_registry));
    });
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    let of = |name: &str| {
        REGISTRIES.with_borrow(|all| {
            all.iter().find(|(n, _)| *n == name).map(|(_, registry)| registry.clone()).unwrap()
        })
    };
    assert!(!of("main").same(&of(FIRST)));
    assert!(!of(SECOND).same(&of(FIRST)));
    assert!(of(FIRST).same(&of(FIRST)));
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, swappingOutMultipleBackStacks
#[test]
fn another_back_stack_can_be_shown() {
    let (one, two, three) = (
        mutable_state_list_from([FIRST]),
        mutable_state_list_from([SECOND]),
        mutable_state_list_from([THIRD]),
    );
    let stacks = [one.handle(), two.handle(), three.handle()];
    let state = mutable_state_of(0);
    let read = state.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(stacks[read.get()], entry_provider = text_entry);
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    state.set(1);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    state.set(2);
    ui.wait_for_idle();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    stacks[2].push(FOURTH);
    ui.wait_for_idle();
    ui.on_node_with_text(FOURTH).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testInitEmptyBackstackThrows
#[test]
fn a_display_of_nothing_panics() {
    let text = panic_text(|| {
        let (mut ui, _stack) = showing(&[]);
        ui.wait_for_idle();
    });
    assert!(text.contains("NavDisplay backstack cannot be empty"), "{text}");
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testPopToEmptyBackstackThrows
#[test]
fn a_display_popped_to_nothing_panics() {
    let text = panic_text(|| {
        let (mut ui, stack) = showing(&[FIRST]);
        ui.wait_for_idle();
        ui.on_node_with_text(FIRST).assert_is_displayed();
        stack.clear();
        ui.wait_for_idle();
    });
    assert!(text.contains("NavDisplay backstack cannot be empty"), "{text}");
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testPopAddInSameFrame
#[test]
fn a_pop_and_an_add_in_one_frame_show_what_s_added() {
    let (mut ui, stack) = showing(&[FIRST]);
    ui.wait_for_idle();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
    stack.remove(1);
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testPopAddInCenterInSameFrame
#[test]
fn an_add_in_the_middle_and_a_pop_in_one_frame_show_the_last() {
    let (mut ui, stack) = showing(&[FIRST, THIRD, FOURTH]);
    ui.wait_for_idle();
    ui.on_node_with_text(FOURTH).assert_is_displayed();
    stack.insert(1, SECOND);
    stack.remove(3);
    ui.wait_for_idle();
    ui.on_node_with_text(THIRD).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testDuplicateKeyStateIsShared
#[test]
fn a_key_on_the_stack_twice_shares_its_state() {
    let (mut ui, stack) = counting(&[FIRST]);
    ui.wait_for_idle();
    counter(FIRST).set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 2").assert_is_displayed();
    stack.push(FIRST);
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 2);
    ui.on_node_with_text("first: 2").assert_is_displayed();
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 2);
    ui.on_node_with_text("first: 2").assert_is_displayed();
}

#[composable(non_restartable)]
fn OuterWithNested(nested: compose_runtime::snapshots::SnapshotStateList<&'static str>) {
    Column!({
        Counter!(FIRST);
        NavDisplay!(
            nested,
            entry_provider = |key: &'static str| NavEntry::new(key, |_| Counter!("nested"))
        );
    });
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testDuplicateKeyStateNestedStateIsCorrect
#[test]
fn a_nested_display_with_the_same_key_has_its_own_state() {
    let (outer, nested) = (mutable_state_list_from([FIRST]), mutable_state_list_from([FIRST]));
    let (stack, inner) = (outer.handle(), nested.handle());
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            entry_provider =
                move |key: &'static str| NavEntry::new(key, move |_| OuterWithNested(inner))
        );
    });
    ui.wait_for_idle();
    assert_eq!(counter(FIRST).get(), 0);
    counter(FIRST).set(2);
    ui.wait_for_idle();
    ui.on_node_with_text("first: 2").assert_is_displayed();
    assert_eq!(counter("nested").get(), 0);
    counter("nested").set(1);
    ui.wait_for_idle();
    ui.on_node_with_text("nested: 1").assert_is_displayed();
    ui.on_node_with_text("first: 2").assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testMultiPaneDisplay
#[test]
fn two_panes_show_two_entries_and_a_single_one_replaces_them() {
    let owner = mutable_state_list_from(["left", "right"]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![two_pane_strategy()],
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text("left").assert_is_displayed();
    ui.on_node_with_text("right").assert_is_displayed();
    stack.clear();
    stack.push("single");
    ui.wait_for_idle();
    ui.on_node_with_text("left").assert_does_not_exist();
    ui.on_node_with_text("right").assert_does_not_exist();
    ui.on_node_with_text("single").assert_is_displayed();
}

#[composable(non_restartable)]
fn InnerDisplay(inner: compose_runtime::snapshots::SnapshotStateList<&'static str>) {
    NavDisplay!(
        inner,
        on_back = Some(|| {}),
        entry_provider = |key: &'static str| NavEntry::new(key, |_| Shown!("inner"))
    );
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testNestedNavDisplayPredictiveBackStartWithSingleEntryDoesNotCrash
#[test]
fn a_gesture_past_a_nested_display_of_one_entry_goes_on_to_the_outer() {
    let (outer, inner) =
        (mutable_state_list_from([FIRST, SECOND]), mutable_state_list_from([THIRD]));
    let (stack, nested) = (outer.handle(), inner.handle());
    let owner = TestNavigationEventDispatcherOwner::new();
    let input = DirectNavigationEventInput::new();
    let dispatcher = owner.navigation_event_dispatcher();
    dispatcher.add_input(&input);
    let mut ui = compose_test(move || {
        let child = remember_navigation_event_dispatcher_owner!(parent = Some(dispatcher.clone()));
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(child)),
            || {
                NavDisplay!(
                    stack,
                    on_back = Some(|| {}),
                    entry_provider = move |key: &'static str| {
                        if key == FIRST {
                            NavEntry::new(key, |_| Shown!("home"))
                        } else {
                            NavEntry::new(key, move |_| InnerDisplay(nested))
                        }
                    }
                );
            },
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text("inner").assert_is_displayed();
    ui.on_node_with_text("home").assert_does_not_exist();
    input.back_started(NavigationEvent { progress: 0.1, ..Default::default() });
    ui.wait_for_idle();
    input.back_progressed(NavigationEvent { progress: 0.5, ..Default::default() });
    ui.wait_for_idle();
    input.back_completed();
    ui.wait_for_idle();
    ui.on_node_with_text("inner").assert_is_displayed();
    ui.on_node_with_text("home").assert_does_not_exist();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testSceneStrategiesThenFirstStrategy
#[test]
fn the_first_strategy_with_a_scene_makes_it() {
    let mut ui = compose_test(|| {
        NavDisplay!(
            vec![FIRST, SECOND],
            scene_strategies = vec![two_pane_strategy(), scene_strategy(SinglePaneSceneStrategy)],
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testSceneStrategiesThenChainedStrategy
#[test]
fn a_strategy_with_no_scene_leaves_it_to_the_next() {
    let mut ui = compose_test(|| {
        NavDisplay!(
            vec![FIRST],
            scene_strategies = vec![two_pane_strategy(), scene_strategy(SinglePaneSceneStrategy)],
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
}

type Popped = Rc<RefCell<Vec<ContentKey>>>;

fn noting_pops(popped: &Popped) -> NavEntryDecorator<&'static str> {
    let popped = popped.clone();
    NavEntryDecorator::with_on_pop(
        move |key| popped.borrow_mut().push(key),
        |entry: NavEntry<&'static str>| entry.Content(),
    )
}

fn key_of(key: &'static str) -> ContentKey {
    text_entry(key).content_key().clone()
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testOnPopCalledWhenSwappingBackStacksWithSharedDecorator
#[test]
fn another_back_stack_pops_the_last_ones_entries_once_they_ve_left() {
    let popped = Popped::default();
    let decorator = noting_pops(&popped);
    let (one, two) = (mutable_state_list_from([FIRST]), mutable_state_list_from([SECOND]));
    let stacks = [one.handle(), two.handle()];
    let state = mutable_state_of(0);
    let read = state.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stacks[read.get()],
            on_back = Some(|| {}),
            entry_decorators = vec![decorator.clone()],
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    assert!(popped.borrow().is_empty());
    stacks[0].push(THIRD);
    ui.wait_for_idle();
    assert!(popped.borrow().is_empty());
    state.set(1);
    ui.wait_for_idle();
    let popped = popped.borrow();
    assert_eq!(popped.len(), 2);
    assert!(popped.contains(&key_of(FIRST)));
    assert!(popped.contains(&key_of(THIRD)));
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testOnPopCalledWhenSwappingBackStacksWithMultipleDecorator
#[test]
fn a_stack_hidden_pops_only_what_s_taken_off_it() {
    let (popped_one, popped_two) = (Popped::default(), Popped::default());
    let (decorator_one, decorator_two) = (noting_pops(&popped_one), noting_pops(&popped_two));
    let (one, two) = (mutable_state_list_from([FIRST]), mutable_state_list_from([SECOND]));
    let (stack_one, stack_two) = (one.handle(), two.handle());
    let state = mutable_state_of(0);
    let read = state.handle();
    let mut ui = compose_test(move || {
        let entries_one = remember_decorated_nav_entries!(
            stack_one,
            vec![decorator_one.clone()],
            entry_provider = text_entry
        );
        let entries_two = remember_decorated_nav_entries!(
            stack_two,
            vec![decorator_two.clone()],
            entry_provider = text_entry
        );
        let entries = if read.get() == 0 { entries_one } else { entries_two };
        if !entries.is_empty() {
            NavDisplayOfEntries!(entries, on_back = || {});
        }
    });
    ui.wait_for_idle();
    state.set(1);
    ui.wait_for_idle();
    assert!(popped_one.borrow().is_empty(), "only hidden");
    ui.on_node_with_text(SECOND).assert_is_displayed();
    stack_one.remove(0);
    ui.wait_for_idle();
    assert_eq!(*popped_one.borrow(), [key_of(FIRST)]);
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/ui/NavDisplayTest.kt, testInterruptPopWithPop
#[test]
fn a_pop_interrupted_by_another_settles_on_the_last() {
    let owner = mutable_state_list_from([FIRST, SECOND, THIRD]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![two_pane_strategy()],
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(2);
    ui.advance_time_by(Duration::from_millis(50));
    stack.remove(1);
    ui.advance_time_by(Duration::from_millis(50));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
}

/// Scenes that have left are forgotten, or what they showed would stay excluded from later ones.
#[test]
fn a_scene_settled_on_excludes_nothing_for_those_that_left() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![two_pane_strategy()],
            entry_provider = text_entry
        );
    });
    ui.wait_for_idle();
    stack.remove(1);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    stack.push(THIRD);
    ui.wait_for_idle();
    ui.on_node_with_text(FIRST).assert_is_displayed();
    ui.on_node_with_text(THIRD).assert_is_displayed();
    ui.on_node_with_text(SECOND).assert_does_not_exist();
}

/// A dialog dismissed goes back by the latest `on_back`, its scene not made again for it.
#[test]
fn a_dialog_dismissed_goes_back_by_the_latest_on_back() {
    let told = Rc::new(RefCell::new(Vec::new()));
    let tick = mutable_state_of(0);
    let (read, logged) = (tick.handle(), told.clone());
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let (value, logged) = (read.get(), logged.clone());
        NavDisplay!(
            stack,
            on_back = Some(move || logged.borrow_mut().push(value)),
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = |key: &'static str| {
                let entry = text_entry(key);
                if key == SECOND {
                    entry.with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
                } else {
                    entry
                }
            }
        );
    });
    ui.wait_for_idle();
    tick.set(1);
    ui.wait_for_idle();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert_eq!(*told.borrow(), [1]);
}

#[composable]
fn HandlingBack(key: &'static str, backs: Rc<Cell<u32>>) {
    BackHandler!(on_back = move || backs.set(backs.get() + 1));
    Shown!(key);
}

/// A screen's own `BackHandler` goes before the display's.
#[test]
fn a_screen_s_back_handler_goes_first() {
    let backs = Rc::new(Cell::new(0));
    let seen = backs.clone();
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        NavDisplay!(
            stack,
            entry_provider = move |key: &'static str| {
                let seen = seen.clone();
                if key == SECOND {
                    NavEntry::new(key, move |key| HandlingBack!(key, seen.clone()))
                } else {
                    text_entry(key)
                }
            }
        );
    });
    ui.wait_for_idle();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert_eq!(backs.get(), 1);
    assert_eq!(stack.len(), 2);
}

/// An entry leaving is created at most, so its `BackHandler` takes nothing while it animates out.
#[test]
fn a_leaving_entry_s_back_handler_takes_nothing() {
    let backs = Rc::new(Cell::new(0));
    let seen = backs.clone();
    let owner = mutable_state_list_from([FIRST, SECOND, THIRD]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        NavDisplay!(
            stack,
            entry_provider = move |key: &'static str| {
                let seen = seen.clone();
                if key == THIRD {
                    NavEntry::new(key, move |key| HandlingBack!(key, seen.clone()))
                } else {
                    text_entry(key)
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.remove(2);
    ui.advance_time_by(Duration::from_millis(100));
    ui.on_node_with_text(THIRD).assert_exists();
    assert!(ui.host().on_back_pressed());
    assert_eq!(backs.get(), 0, "the display took it");
    assert_eq!(stack.len(), 1);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
}

#[composable(non_restartable)]
fn ListOfDisplays() {
    LazyColumn!(|scope: LazyListScope| {
        scope.items(2, move |index| {
            let key: &'static str = if index == 0 { "inner one" } else { "inner two" };
            Box!(modifier = Modifier.size(50.dp()), {
                NavDisplay!(vec![key], on_back = Some(|| {}), entry_provider = text_entry);
            });
        });
    });
}

/// A display in each item of a list in an entry of a display: each its own, and shown again after
/// going away and back.
#[test]
fn displays_in_a_list_in_a_display() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            entry_provider = |key: &'static str| {
                if key == FIRST {
                    NavEntry::new(key, |_| ListOfDisplays())
                } else {
                    text_entry(key)
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_text("inner one").assert_is_displayed();
    ui.on_node_with_text("inner two").assert_is_displayed();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text("inner one").assert_does_not_exist();
    stack.remove(1);
    ui.wait_for_idle();
    ui.on_node_with_text("inner one").assert_is_displayed();
    ui.on_node_with_text("inner two").assert_is_displayed();
}

/// The guide's shape: a stack in what calls the display, a provider of a stack's handle. A push
/// shows what's on the stack once, whatever's under it.
#[composable]
fn GuideShape(renders: Rc<Cell<u32>>) {
    let back_stack = remember_nav_back_stack(["first".to_string()]);
    STACK.set(Some(back_stack));
    let renders = renders.clone();
    NavDisplay!(
        back_stack,
        entry_provider = move |key: String| {
            let renders = renders.clone();
            NavEntry::new(key, move |key: String| {
                if key == "first" {
                    renders.set(renders.get() + 1);
                }
                let shown = key.clone();
                compose_foundation::BasicText!("{shown}");
            })
        }
    );
}

thread_local! {
    static STACK: Cell<Option<NavBackStack<String>>> = const { Cell::new(None) };
}

// Compose: navigation3/navigation3-ui/src/androidDeviceTest/kotlin/androidx/navigation3/scene/usecases/AnimateOverlaySceneTest.kt, testNestedBottomSheet
#[test]
fn a_push_doesn_t_show_again_what_s_on_the_stack() {
    let renders = Rc::new(Cell::new(0));
    let given = renders.clone();
    let mut ui = compose_test(move || GuideShape!(given.clone()));
    ui.wait_for_idle();
    assert_eq!(renders.get(), 1);
    STACK.get().unwrap().push("second".to_string());
    ui.wait_for_idle();
    assert_eq!(renders.get(), 1);
    STACK.get().unwrap().push("third".to_string());
    ui.wait_for_idle();
    assert_eq!(renders.get(), 1);
    STACK.get().unwrap().remove(2);
    STACK.get().unwrap().remove(1);
    ui.wait_for_idle();
    assert_eq!(renders.get(), 2, "shown again, having left");
}

/// A screen under a dialog isn't shown again by what comes and goes over it.
#[test]
fn what_s_under_a_dialog_isn_t_shown_again() {
    let renders = Rc::new(Cell::new(0));
    let counted = renders.clone();
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        NavDisplay!(
            stack,
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = move |key: &'static str| {
                let counted = counted.clone();
                let entry = NavEntry::new(key, move |key| {
                    if key == FIRST {
                        counted.set(counted.get() + 1);
                    }
                    Shown!(key);
                });
                if key == FIRST {
                    entry
                } else {
                    entry.with_metadata(DialogSceneStrategy::dialog(DialogProperties::default()))
                }
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(renders.get(), 1);
    stack.push(SECOND);
    ui.wait_for_idle();
    assert_eq!(renders.get(), 1);
    stack.remove(1);
    ui.wait_for_idle();
    assert_eq!(renders.get(), 1);
}

/// An entry pushing as it composes pushes once: nothing shows it again.
#[test]
fn an_entry_pushing_in_composition_pushes_once() {
    let owner = mutable_state_list_from([FIRST]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            entry_provider = move |key: &'static str| {
                NavEntry::new(key, move |key| {
                    if key == FIRST {
                        stack.push(SECOND);
                    }
                    Shown!(key);
                })
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(stack.len(), 2);
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

/// A dialog's properties come from its entry's metadata: one that doesn't dismiss on back keeps it.
#[test]
fn a_dialog_s_properties_are_its_metadata_s() {
    let owner = mutable_state_list_from([FIRST, SECOND]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            scene_strategies = vec![scene_strategy(DialogSceneStrategy)],
            entry_provider = |key: &'static str| {
                let entry = text_entry(key);
                if key == SECOND {
                    entry.with_metadata(DialogSceneStrategy::dialog(DialogProperties {
                        dismiss_on_back_press: false,
                        ..DialogProperties::default()
                    }))
                } else {
                    entry
                }
            }
        );
    });
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    assert_eq!(stack.len(), 2);
    ui.on_node_with_text(SECOND).assert_is_displayed();
}

#[composable]
fn Sized(side: i32) {
    Box!(modifier = Modifier.size(side.dp()));
}

/// `content_alignment` places what's smaller than the largest shown.
#[test]
fn a_display_aligns_its_content() {
    let owner = mutable_state_list_from([150]);
    let stack = owner.handle();
    let mut ui = compose_test(move || {
        NavDisplay!(
            stack,
            modifier = Modifier.test_tag("display"),
            content_alignment = Alignment::CENTER,
            entry_provider = |side: i32| NavEntry::new(side, move |_| Tagged!(side))
        );
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    stack.push(50);
    ui.advance_time_by(Duration::from_millis(100));
    let bounds = ui.on_node_with_tag("50").fetch_semantics_snapshot().bounds;
    assert_eq!((bounds.left, bounds.top), (50.0, 50.0));
}

#[composable]
fn Tagged(side: i32) {
    Box!(modifier = Modifier.size(side.dp()).test_tag(format!("{side}")));
}

/// `size_transform`: with one, the display's size animates; without, it's the largest at once.
#[test]
fn a_display_s_size_follows_its_size_transform() {
    fn size_during(transform: Option<SizeTransform>) -> f32 {
        let owner = mutable_state_list_from([50]);
        let stack = owner.handle();
        let held = transform;
        let mut ui = compose_test(move || {
            NavDisplay!(
                stack,
                modifier = Modifier.test_tag("display"),
                size_transform = held.clone(),
                entry_provider = |side: i32| NavEntry::new(side, move |_| Sized!(side))
            );
        });
        ui.wait_for_idle();
        ui.set_auto_advance(false);
        stack.push(150);
        ui.advance_time_by(Duration::from_millis(100));
        ui.on_node_with_tag("display").fetch_semantics_snapshot().bounds.width()
    }
    assert_eq!(size_during(None), 150.0);
    let animated = size_during(Some(compose_animation::size_transform(false, |_, _| {
        compose_animation::tween(1000).into()
    })));
    assert!(animated > 50.0 && animated < 150.0, "{animated}");
}

#[composable]
fn Labelled(
    stack: compose_runtime::snapshots::SnapshotStateList<&'static str>,
    label: MutableState<&'static str>,
) {
    let label = label.get();
    NavDisplay!(
        stack,
        entry_provider = move |key: &'static str| {
            NavEntry::new(key, move |key| {
                compose_foundation::BasicText!("{key} {label}");
            })
        }
    );
}

/// Another provider's entries are its own: keys that stay keep theirs only under the same provider.
/// As in Kotlin, it's asked when the keys change.
#[test]
fn a_new_provider_makes_the_entries_of_the_next_keys() {
    let owner = mutable_state_list_from([FIRST]);
    let (stack, label) = (owner.handle(), mutable_state_of("old"));
    let read = label.handle();
    let mut ui = compose_test(move || Labelled!(stack, read));
    ui.wait_for_idle();
    ui.on_node_with_text("first old").assert_exists();
    label.set("new");
    ui.wait_for_idle();
    ui.on_node_with_text("first old").assert_exists();
    stack.push(SECOND);
    ui.wait_for_idle();
    ui.on_node_with_text("second new").assert_exists();
    stack.remove(1);
    ui.wait_for_idle();
    ui.on_node_with_text("first new").assert_exists();
}
