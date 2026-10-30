//! In a composition, against AndroidX's `NavigationEventHandlerTest`,
//! `NavigationEventDispatcherOwnerTest` and `RememberNavigationEventStateTest`.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_navigation_event::*;
use compose_runtime::*;
use compose_test::*;

use crate::support::*;

#[derive(Clone, PartialEq, Debug)]
struct TestInfo(i32);

type Log = Rc<RefCell<Vec<&'static str>>>;

fn in_child(parent: NavigationEventDispatcher, content: impl FnOnce()) {
    let child = remember_navigation_event_dispatcher_owner!(parent = Some(parent));
    CompositionLocalProvider(
        LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(child)),
        content,
    );
}

/// An owner, with an input added.
fn owner_and_input() -> (TestNavigationEventDispatcherOwner, DirectNavigationEventInput) {
    let owner = TestNavigationEventDispatcherOwner::new();
    let input = DirectNavigationEventInput::new();
    owner.navigation_event_dispatcher().add_input(&input);
    (owner, input)
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, infoHandler_whenInfoChanges_updatesGlobalHistoryState
#[test]
fn a_handler_s_info_reaches_the_history() {
    let (owner, _input) = owner_and_input();
    let current = mutable_state_of(1);
    let dispatcher = owner.navigation_event_dispatcher();
    let (read, parent) = (current.handle(), dispatcher.clone());
    let mut ui = compose_test(move || {
        let parent = parent.clone();
        in_child(parent, || {
            let id = read.get();
            let back = if id == 2 { vec![TestInfo(1)] } else { vec![] };
            let state = remember_navigation_event_state!(TestInfo(id), back_info = back);
            NavigationEventHandler!(state, on_back_completed = || {});
        });
    });
    ui.wait_for_idle();
    let merged = |infos: &[i32]| -> Vec<NavigationEventInfo> {
        infos.iter().map(|id| NavigationEventInfo::new(TestInfo(*id))).collect()
    };
    assert_eq!(dispatcher.history().merged_history(), merged(&[1]));
    current.set(2);
    ui.wait_for_idle();
    assert_eq!(dispatcher.history().merged_history(), merged(&[1, 2]));
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, handler_whenStateIsReused_throwsException
// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/RememberNavigationEventStateTest.kt, rememberNavigationEventState_whenAddingToMultipleHandlers_throws
#[test]
fn a_state_with_two_handlers_panics() {
    let (owner, _input) = owner_and_input();
    let dispatcher = owner.navigation_event_dispatcher();
    let text = panic_text(move || {
        let mut ui = compose_test(move || {
            let parent = dispatcher.clone();
            in_child(parent, || {
                let state = remember_navigation_event_state!(TestInfo(1));
                NavigationEventHandler!(state.clone(), on_back_completed = || {});
                NavigationEventHandler!(state, on_back_completed = || {});
            });
        });
        ui.wait_for_idle();
    });
    assert!(text.contains("is already registered with a NavigationEventHandler"), "{text}");
}

/// A handler logging `name`, of a state of its own.
#[composable]
fn Logging(log: Log, name: &'static str, enabled: bool) {
    let state = remember_navigation_event_state!(TestInfo(-1));
    let (cancelled, completed) = (log.clone(), log);
    NavigationEventHandler!(
        state,
        is_back_enabled = enabled,
        on_back_cancelled = move || cancelled.borrow_mut().push("cancelled"),
        on_back_completed = move || completed.borrow_mut().push(name)
    );
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenEventsAreDispatched_invokesCorrectLambdas
#[test]
fn a_handler_runs_the_closures_of_a_gesture_s_end() {
    let (owner, input) = owner_and_input();
    let log = Log::default();
    let (dispatcher, logged) = (owner.navigation_event_dispatcher(), log.clone());
    let mut ui = compose_test(move || {
        let (parent, logged) = (dispatcher.clone(), logged.clone());
        in_child(parent, move || Logging!(logged.clone(), "completed", true));
    });
    ui.wait_for_idle();
    input.back_started(NavigationEvent::default());
    input.back_progressed(NavigationEvent::default());
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["completed"]);
    log.borrow_mut().clear();
    input.back_started(NavigationEvent::default());
    input.back_cancelled();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["cancelled"]);
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenDisabled_invokesFallbackInsteadOfHandler
// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenEnabledStateChanges_togglesHandlerCorrectly
#[test]
fn a_handler_disabled_leaves_it_to_the_fallback() {
    let (owner, input) = owner_and_input();
    let (log, enabled) = (Log::default(), mutable_state_of(true));
    let (dispatcher, logged, read) =
        (owner.navigation_event_dispatcher(), log.clone(), enabled.handle());
    let mut ui = compose_test(move || {
        let (parent, logged) = (dispatcher.clone(), logged.clone());
        in_child(parent, move || Logging!(logged.clone(), "handler", read.get()));
    });
    ui.wait_for_idle();
    let gesture = || {
        input.back_started(NavigationEvent::default());
        input.back_completed();
    };
    gesture();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["handler"]);
    assert_eq!(owner.on_back_completed_fallback_invocations(), 0);
    enabled.set(false);
    log.borrow_mut().clear();
    ui.wait_for_idle();
    gesture();
    ui.wait_for_idle();
    assert!(log.borrow().is_empty());
    assert_eq!(owner.on_back_completed_fallback_invocations(), 1);
    enabled.set(true);
    ui.wait_for_idle();
    gesture();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["handler"]);
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenLambdaChanges_invokesNewLambda
#[test]
fn a_handler_takes_a_new_closure() {
    let (owner, input) = owner_and_input();
    let (log, name) = (Log::default(), mutable_state_of("first"));
    let (dispatcher, logged, read) =
        (owner.navigation_event_dispatcher(), log.clone(), name.handle());
    let mut ui = compose_test(move || {
        let (parent, logged) = (dispatcher.clone(), logged.clone());
        in_child(parent, move || Logging!(logged.clone(), read.get(), true));
    });
    ui.wait_for_idle();
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["first"]);
    name.set("second");
    ui.wait_for_idle();
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["first", "second"]);
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenNested_invokesOnlyInnermost
// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenSiblingsExist_invokesLastComposed
#[test]
fn the_last_handler_composed_takes_it() {
    let (owner, input) = owner_and_input();
    let log = Log::default();
    let (dispatcher, logged) = (owner.navigation_event_dispatcher(), log.clone());
    let mut ui = compose_test(move || {
        let (parent, logged) = (dispatcher.clone(), logged.clone());
        in_child(parent, move || {
            Logging!(logged.clone(), "first", true);
            Logging!(logged.clone(), "second", true);
        });
    });
    ui.wait_for_idle();
    input.back_started(NavigationEvent::default());
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["second"]);
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenNestedChildIsDisabled_invokesParent
// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventHandlerTest.kt, lambdaHandler_whenLastSiblingIsDisabled_invokesPrevious
#[test]
fn a_last_handler_disabled_leaves_it_to_the_one_before() {
    let (owner, input) = owner_and_input();
    let log = Log::default();
    let (dispatcher, logged) = (owner.navigation_event_dispatcher(), log.clone());
    let mut ui = compose_test(move || {
        let (parent, logged) = (dispatcher.clone(), logged.clone());
        in_child(parent, move || {
            Logging!(logged.clone(), "first", true);
            Logging!(logged.clone(), "second", false);
        });
    });
    ui.wait_for_idle();
    input.back_started(NavigationEvent::default());
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["first"]);
}

type Held = Rc<RefCell<Option<NavigationEventDispatcher>>>;

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_asChild_whenInComposition_thenCreatesChildDispatcher
#[test]
fn a_remembered_owner_is_a_child_of_the_local_one() {
    let parent = TestNavigationEventDispatcherOwner::new();
    let held = Held::default();
    let (dispatcher, seen) = (parent.navigation_event_dispatcher(), held.clone());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(dispatcher.clone())),
            move || *seen.borrow_mut() = Some(remember_navigation_event_dispatcher_owner!()),
        );
    });
    ui.wait_for_idle();
    let child = held.borrow().clone().unwrap();
    assert_ne!(child, parent.navigation_event_dispatcher());
    let handler = TestNavigationEventHandler::new();
    child.add_handler(&handler);
    let input = DirectNavigationEventInput::new();
    child.add_input(&input);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
    assert_eq!(parent.on_back_completed_fallback_invocations(), 0);
}

#[composable]
fn OwnerWhile(show: MutableState<bool>, seen: Held) {
    if show.get() {
        *seen.borrow_mut() = Some(remember_navigation_event_dispatcher_owner!());
    }
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_asChild_whenRemovedFromComposition_thenIsDisposed
#[test]
fn a_remembered_owner_leaving_is_disposed() {
    let parent = TestNavigationEventDispatcherOwner::new();
    let (held, show) = (Held::default(), mutable_state_of(true));
    let (dispatcher, seen, read) =
        (parent.navigation_event_dispatcher(), held.clone(), show.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(dispatcher.clone())),
            move || OwnerWhile!(read, seen.clone()),
        );
    });
    ui.wait_for_idle();
    show.set(false);
    ui.wait_for_idle();
    let child = held.borrow().clone().unwrap();
    assert_ne!(child, parent.navigation_event_dispatcher());
    let text = panic_text(|| child.add_input(&DirectNavigationEventInput::new()));
    assert!(text.contains("has already been disposed"), "{text}");
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_asChild_whenEnabledStateChanges_thenUpdatesDispatcher
#[test]
fn a_remembered_owner_disabled_dispatches_nothing() {
    let parent = TestNavigationEventDispatcherOwner::new();
    let (held, enabled) = (Held::default(), mutable_state_of(true));
    let (dispatcher, seen, read) =
        (parent.navigation_event_dispatcher(), held.clone(), enabled.handle());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(dispatcher.clone())),
            move || {
                *seen.borrow_mut() =
                    Some(remember_navigation_event_dispatcher_owner!(enabled = read.get()));
            },
        );
    });
    ui.wait_for_idle();
    enabled.set(false);
    ui.wait_for_idle();
    let child = held.borrow().clone().unwrap();
    let handler = TestNavigationEventHandler::new();
    child.add_handler(&handler);
    let input = DirectNavigationEventInput::new();
    child.add_input(&input);
    input.back_completed();
    assert!(!child.is_enabled());
    assert!(!handler.is_back_enabled());
    assert_eq!(handler.on_back_completed_invocations(), 0);
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_whenChildRecomposes_thenParentIsNotDisposed
#[test]
fn a_child_replaced_leaves_its_parent() {
    let (parent_held, child_held, configuration) =
        (Held::default(), Held::default(), mutable_state_of(1));
    let (seen_parent, seen_child, read) =
        (parent_held.clone(), child_held.clone(), configuration.handle());
    let mut ui = compose_test(move || {
        let parent = remember_navigation_event_dispatcher_owner!(parent = None);
        *seen_parent.borrow_mut() = Some(parent.clone());
        let seen_child = seen_child.clone();
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(parent)),
            move || Configured!(read, seen_child.clone()),
        );
    });
    ui.wait_for_idle();
    configuration.set(2);
    ui.wait_for_idle();
    let parent = parent_held.borrow().clone().unwrap();
    let handler = TestNavigationEventHandler::new();
    parent.add_handler(&handler);
    let input = DirectNavigationEventInput::new();
    parent.add_input(&input);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
    let child = child_held.borrow().clone().unwrap();
    let text = panic_text(|| child.add_input(&DirectNavigationEventInput::new()));
    assert!(text.contains("has already been disposed"), "{text}");
}

#[composable]
fn Configured(configuration: MutableState<i32>, seen: Held) {
    if configuration.get() == 1 {
        *seen.borrow_mut() = Some(remember_navigation_event_dispatcher_owner!());
    } else {
        remember_navigation_event_dispatcher_owner!();
    }
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_whenParentChanges_thenOldDispatcherIsDisposed
#[test]
fn another_parent_disposes_the_old_child() {
    let (first, second) =
        (TestNavigationEventDispatcherOwner::new(), TestNavigationEventDispatcherOwner::new());
    let which = mutable_state_of(0);
    let held = Held::default();
    let parents = [first.navigation_event_dispatcher(), second.navigation_event_dispatcher()];
    let (read, seen) = (which.handle(), held.clone());
    let mut ui = compose_test(move || {
        let index = read.get();
        let seen = seen.clone();
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(parents[index].clone())),
            move || {
                let child = remember_navigation_event_dispatcher_owner!();
                if index == 0 {
                    *seen.borrow_mut() = Some(child);
                }
            },
        );
    });
    ui.wait_for_idle();
    which.set(1);
    ui.wait_for_idle();
    let child = held.borrow().clone().unwrap();
    let text = panic_text(|| child.add_input(&DirectNavigationEventInput::new()));
    assert!(text.contains("has already been disposed"), "{text}");
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_asRoot_whenNoParent_thenCreatesRootDispatcher
// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_asRoot_whenEnabledStateChanges_thenUpdatesDispatcher
#[test]
fn a_remembered_root_dispatches_while_enabled() {
    let (held, enabled) = (Held::default(), mutable_state_of(true));
    let (seen, read) = (held.clone(), enabled.handle());
    let mut ui = compose_test(move || {
        *seen.borrow_mut() =
            Some(remember_navigation_event_dispatcher_owner!(enabled = read.get(), parent = None));
    });
    ui.wait_for_idle();
    let root = held.borrow().clone().unwrap();
    let handler = TestNavigationEventHandler::new();
    root.add_handler(&handler);
    let input = DirectNavigationEventInput::new();
    root.add_input(&input);
    input.back_completed();
    assert!(root.is_enabled());
    assert_eq!(handler.on_back_completed_invocations(), 1);
    enabled.set(false);
    ui.wait_for_idle();
    input.back_completed();
    assert!(!root.is_enabled());
    assert_eq!(handler.on_back_completed_invocations(), 1);
}

#[composable]
fn RootWhile(show: MutableState<bool>, seen: Held) {
    if show.get() {
        *seen.borrow_mut() = Some(remember_navigation_event_dispatcher_owner!(parent = None));
    }
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_asRoot_whenRemovedFromComposition_thenIsDisposed
#[test]
fn a_remembered_root_leaving_is_disposed() {
    let (held, show) = (Held::default(), mutable_state_of(true));
    let (seen, read) = (held.clone(), show.handle());
    let mut ui = compose_test(move || RootWhile!(read, seen.clone()));
    ui.wait_for_idle();
    show.set(false);
    ui.wait_for_idle();
    let root = held.borrow().clone().unwrap();
    let text = panic_text(|| root.add_input(&DirectNavigationEventInput::new()));
    assert!(text.contains("has already been disposed"), "{text}");
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/NavigationEventDispatcherOwnerTest.kt, rememberNavigationEventDispatcherOwner_asRoot_whenNoExplicitlyNullParent_thenThrows
#[test]
fn a_remembered_owner_with_no_parent_panics_unless_it_says_so() {
    let text = panic_text(|| {
        let mut ui = compose_test(|| {
            CompositionLocalProvider(
                LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(None),
                || {
                    remember_navigation_event_dispatcher_owner!();
                },
            );
        });
        ui.wait_for_idle();
    });
    assert!(
        text.contains(
            "No NavigationEventDispatcherOwner provided in LocalNavigationEventDispatcherOwner"
        ),
        "{text}"
    );
}

type HeldState = Rc<RefCell<Option<NavigationEventState<TestInfo>>>>;

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/RememberNavigationEventStateTest.kt, rememberNavigationEventState_whenInfoChanges_updatesStateFields
#[test]
fn a_state_s_fields_follow_the_info() {
    let (current, held) = (mutable_state_of(1), HeldState::default());
    let (read, seen) = (current.handle(), held.clone());
    let mut ui = compose_test(move || {
        let id = read.get();
        let (back, forward) =
            if id == 2 { (vec![TestInfo(1)], vec![TestInfo(3)]) } else { (vec![], vec![]) };
        *seen.borrow_mut() = Some(remember_navigation_event_state!(
            TestInfo(id),
            back_info = back,
            forward_info = forward
        ));
    });
    ui.wait_for_idle();
    let state = held.borrow().clone().unwrap();
    assert_eq!(state.current_info(), TestInfo(1));
    assert!(state.back_info().is_empty() && state.forward_info().is_empty());
    current.set(2);
    ui.wait_for_idle();
    assert_eq!(state.current_info(), TestInfo(2));
    assert_eq!(state.back_info(), [TestInfo(1)]);
    assert_eq!(state.forward_info(), [TestInfo(3)]);
}

// Compose: navigationevent/navigationevent-compose/src/androidDeviceTest/kotlin/androidx/navigationevent/compose/RememberNavigationEventStateTest.kt, rememberNavigationEventState_transitionState_reflectsGestureLifecycle
#[test]
fn a_state_s_transition_follows_the_gesture() {
    let (owner, input) = owner_and_input();
    let held = HeldState::default();
    let (dispatcher, seen) = (owner.navigation_event_dispatcher(), held.clone());
    let mut ui = compose_test(move || {
        let (parent, seen) = (dispatcher.clone(), seen.clone());
        in_child(parent, move || {
            let state = remember_navigation_event_state!(TestInfo(1));
            *seen.borrow_mut() = Some(state.clone());
            NavigationEventHandler!(state);
        });
    });
    ui.wait_for_idle();
    let state = held.borrow().clone().unwrap();
    use NavigationEventTransitionState::*;
    assert_eq!(state.transition_state(), Idle);
    input.back_started(NavigationEvent::default());
    assert!(matches!(state.transition_state(), InProgress { .. }));
    input.back_progressed(NavigationEvent::default());
    assert!(matches!(state.transition_state(), InProgress { .. }));
    input.back_cancelled();
    assert_eq!(state.transition_state(), Idle);
    input.back_started(NavigationEvent::default());
    assert!(matches!(state.transition_state(), InProgress { .. }));
    input.back_completed();
    assert_eq!(state.transition_state(), Idle);
}

/// `NavigationBackHandler` and `NavigationForwardHandler` take their direction alone.
#[test]
fn back_and_forward_handlers_take_their_direction() {
    let (owner, input) = owner_and_input();
    let log = Log::default();
    let (dispatcher, logged) = (owner.navigation_event_dispatcher(), log.clone());
    let mut ui = compose_test(move || {
        let (parent, logged) = (dispatcher.clone(), logged.clone());
        in_child(parent, move || {
            let state = remember_navigation_event_state!(TestInfo(1));
            let (back, forward) = (logged.clone(), logged.clone());
            NavigationBackHandler!(
                state,
                on_back_completed = move || back.borrow_mut().push("back")
            );
            let state = remember_navigation_event_state!(TestInfo(2));
            NavigationForwardHandler!(
                state,
                on_forward_completed = move || forward.borrow_mut().push("forward")
            );
        });
    });
    ui.wait_for_idle();
    input.back_completed();
    input.forward_completed();
    assert_eq!(*log.borrow(), ["back", "forward"]);
}
