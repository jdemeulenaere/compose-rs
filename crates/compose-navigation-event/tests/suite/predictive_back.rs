//! `BackHandler` and `PredictiveBackHandler`, on the host's navigation events, against AndroidX's
//! `BackHandlerTest` and `PredictiveBackHandlerTest`.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_lifecycle::*;
use compose_navigation_event::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::*;

use crate::support::panic_text;

type Log = Rc<RefCell<Vec<&'static str>>>;

fn fake_event() -> BackEventCompat {
    BackEventCompat {
        touch_x: 0.1,
        touch_y: 0.1,
        swipe_edge: NavigationEvent::EDGE_LEFT,
        ..Default::default()
    }
}

fn logging(log: &Log, name: &'static str) -> impl Fn() + 'static {
    let log = log.clone();
    move || log.borrow_mut().push(name)
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/BackHandlerTest.kt, testBackHandler
#[test]
fn back_reaches_a_back_handler() {
    let log = Log::default();
    let on_back = logging(&log, "back");
    let on_back = Rc::new(on_back);
    let mut ui = compose_test(move || {
        let on_back = on_back.clone();
        BackHandler!(on_back = move || on_back());
    });
    ui.wait_for_idle();
    assert!(ui.host().on_back_pressed());
    assert_eq!(*log.borrow(), ["back"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/BackHandlerTest.kt, testBackHandlerDisabledChild
#[test]
fn a_disabled_back_handler_leaves_it_to_the_one_before() {
    let log = Log::default();
    let (parent, child) = (Rc::new(logging(&log, "parent")), Rc::new(logging(&log, "child")));
    let mut ui = compose_test(move || {
        let (parent, child) = (parent.clone(), child.clone());
        BackHandler!(on_back = move || parent());
        BackHandler!(enabled = false, on_back = move || child());
    });
    ui.wait_for_idle();
    assert!(ui.host().on_back_pressed());
    assert_eq!(*log.borrow(), ["parent"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/BackHandlerTest.kt, testBackHandlerOnBackChanged
#[test]
fn a_back_handler_takes_a_new_closure() {
    let log = Log::default();
    let name = mutable_state_of("initial");
    let (read, logged) = (name.handle(), log.clone());
    let mut ui = compose_test(move || {
        let (name, logged) = (read.get(), logged.clone());
        BackHandler!(on_back = move || logged.borrow_mut().push(name));
    });
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    name.set("changed");
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    assert_eq!(*log.borrow(), ["initial", "changed"]);
}

/// `content` under `lifecycle`, above a handler that was there first.
fn under_lifecycle(
    lifecycle: Lifecycle,
    below: NavigationEventHandler,
    content: impl Fn() + 'static,
) -> ComposeTestRule {
    let content = Rc::new(content);
    compose_test(move || {
        let (below, content) = (below.clone(), content.clone());
        let dispatcher = LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.current().unwrap();
        DisposableEffect(0, move |scope| {
            dispatcher.add_handler(&below);
            scope.on_dispose(move || below.remove())
        });
        CompositionLocalProvider(LOCAL_LIFECYCLE_OWNER.provides(lifecycle.clone()), move || {
            content()
        });
    })
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/BackHandlerTest.kt, testBackHandlerLifecycle
#[test]
fn a_back_handler_keeps_its_place_through_a_lifecycle() {
    let owner = TestLifecycleOwner::new(LifecycleState::Resumed);
    let intercepted = Rc::new(Cell::new(false));
    let seen = intercepted.clone();
    let below = TestNavigationEventHandler::new();
    let mut ui = under_lifecycle(owner.lifecycle(), (*below).clone(), move || {
        let seen = seen.clone();
        BackHandler!(on_back = move || seen.set(true));
    });
    ui.wait_for_idle();
    owner.set_current_state(LifecycleState::Created);
    ui.wait_for_idle();
    owner.set_current_state(LifecycleState::Resumed);
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    assert!(intercepted.get());
    assert_eq!(below.on_back_completed_invocations(), 0);
}

/// Stopped, it takes nothing: `isEnabled` follows the lifecycle.
#[test]
fn a_back_handler_of_a_stopped_lifecycle_takes_nothing() {
    let owner = TestLifecycleOwner::new(LifecycleState::Resumed);
    let intercepted = Rc::new(Cell::new(false));
    let seen = intercepted.clone();
    let below = TestNavigationEventHandler::new();
    let mut ui = under_lifecycle(owner.lifecycle(), (*below).clone(), move || {
        let seen = seen.clone();
        BackHandler!(on_back = move || seen.set(true));
    });
    ui.wait_for_idle();
    owner.set_current_state(LifecycleState::Created);
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    assert!(!intercepted.get());
    assert_eq!(below.on_back_completed_invocations(), 1);
}

/// A rule of `PredictiveBackHandler`s logging as `content` says.
fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = compose_test(content);
    ui.wait_for_idle();
    ui
}

/// The handler of most tests: logs its start, collects, logs its end.
#[composable(non_restartable)]
fn Collecting(log: Log, name: &'static str, enabled: bool) {
    PredictiveBackHandler!(
        enabled = enabled,
        on_back = move |progress: BackProgress| {
            let log = log.clone();
            async move {
                log.borrow_mut().push(name);
                let _ = progress.collect(|_| {}).await;
            }
        }
    );
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testHandleOnStart
#[test]
fn a_gesture_starts_the_task() {
    let log = Log::default();
    let logged = log.clone();
    let mut ui = rule(move || Collecting(logged.clone(), "start", true));
    ui.host().on_back_started(fake_event());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start"]);
}

/// Collects, and logs when it's over.
#[composable(non_restartable)]
fn Completing(counter: Rc<Cell<u32>>) {
    PredictiveBackHandler!(
        on_back = move |progress: BackProgress| {
            let counter = counter.clone();
            async move {
                if progress.collect(|_| {}).await.is_ok() {
                    counter.set(counter.get() + 1);
                }
            }
        }
    );
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testHandleOnComplete
// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testHandleOnCompleteWithDispatcher
#[test]
fn a_completed_gesture_ends_the_collection_and_so_does_a_press() {
    let counter = Rc::new(Cell::new(0));
    let counted = counter.clone();
    let mut ui = rule(move || Completing(counted.clone()));
    ui.host().on_back_started(fake_event());
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    assert_eq!(counter.get(), 1);
    // A press with no gesture.
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    assert_eq!(counter.get(), 2);
}

/// In a test owner, with an input.
fn owned(
    content: impl Fn() + 'static,
) -> (ComposeTestRule, TestNavigationEventDispatcherOwner, DirectNavigationEventInput) {
    let owner = TestNavigationEventDispatcherOwner::new();
    let input = DirectNavigationEventInput::new();
    let dispatcher = owner.navigation_event_dispatcher();
    dispatcher.add_input(&input);
    let content = Rc::new(content);
    let ui = rule(move || {
        let content = content.clone();
        CompositionLocalProvider(
            LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.provides(Some(dispatcher.clone())),
            move || content(),
        );
    });
    (ui, owner, input)
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testDisabledBackHandler
#[test]
fn a_disabled_predictive_handler_leaves_it_to_the_fallback() {
    let (log, enabled) = (Log::default(), mutable_state_of(true));
    let (logged, read) = (log.clone(), enabled.handle());
    let (mut ui, owner, input) = owned(move || Collecting(logged.clone(), "on_back", read.get()));
    let gesture = || {
        input.back_started(NavigationEvent::default());
        input.back_completed();
    };
    gesture();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["on_back"]);
    enabled.set(false);
    ui.wait_for_idle();
    gesture();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["on_back"]);
    assert_eq!(owner.on_back_completed_fallback_invocations(), 1);
    enabled.set(true);
    ui.wait_for_idle();
    gesture();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["on_back", "on_back"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testPredictiveBackHandlerDisabledBeforeStart
#[test]
fn a_handler_disabled_just_before_a_gesture_still_starts() {
    let log = Log::default();
    let (count, started, cancelled) =
        (mutable_state_of(2), Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let (read, logged, started_seen, cancelled_seen) =
        (count.handle(), log.clone(), started.clone(), cancelled.clone());
    let (mut ui, _owner, input) = owned(move || {
        let (logged, started, cancelled) =
            (logged.clone(), started_seen.clone(), cancelled_seen.clone());
        PredictiveBackHandler!(
            enabled = read.get() > 1,
            on_back = move |progress: BackProgress| {
                let (logged, started, cancelled) =
                    (logged.clone(), started.clone(), cancelled.clone());
                async move {
                    if read.get() <= 1 {
                        started.set(true);
                    }
                    match progress.collect(|_| {}).await {
                        Ok(()) => logged.borrow_mut().push("on_back"),
                        Err(_) => cancelled.set(true),
                    }
                }
            }
        );
    });
    // The count written, and the gesture before a pass applies it.
    count.set(1);
    input.back_started(NavigationEvent::default());
    ui.wait_for_idle();
    assert!(started.get());
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["on_back"]);
    assert!(!cancelled.get());
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testPredictiveBackHandlerDisabledAfterStart
#[test]
fn a_handler_disabled_just_after_a_gesture_still_completes() {
    let log = Log::default();
    let (count, started) = (mutable_state_of(2), Rc::new(Cell::new(false)));
    let (read, logged, seen) = (count.handle(), log.clone(), started.clone());
    let (mut ui, _owner, input) = owned(move || {
        let (logged, seen) = (logged.clone(), seen.clone());
        PredictiveBackHandler!(
            enabled = read.get() > 1,
            on_back = move |progress: BackProgress| {
                let (logged, seen) = (logged.clone(), seen.clone());
                async move {
                    if read.get() <= 1 {
                        seen.set(true);
                    }
                    let _ = progress.collect(|_| {}).await;
                    logged.borrow_mut().push("on_back");
                }
            }
        );
    });
    input.back_started(NavigationEvent::default());
    compose_runtime::platform::run_ready();
    count.set(1);
    ui.wait_for_idle();
    assert!(!started.get());
    input.back_completed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["on_back"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testNoCollection
#[test]
fn a_task_that_does_not_collect_panics() {
    let text = panic_text(|| {
        let mut ui = rule(|| {
            PredictiveBackHandler!(on_back = |_progress: BackProgress| async {});
        });
        ui.host().on_back_started(fake_event());
        ui.host().on_back_pressed();
        ui.wait_for_idle();
    });
    assert!(text.contains("You must collect the progress flow"), "{text}");
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testRestartCancelsPreviousAsyncJob
#[test]
fn a_new_gesture_ends_the_last_one_s_task() {
    let log = Log::default();
    let logged = log.clone();
    let mut ui = rule(move || {
        let logged = logged.clone();
        PredictiveBackHandler!(
            on_back = move |progress: BackProgress| {
                let logged = logged.clone();
                async move {
                    logged.borrow_mut().push("start");
                    let _ = progress.collect(|_| {}).await;
                    delay(Duration::from_millis(300)).await;
                    logged.borrow_mut().push("async");
                }
            }
        );
    });
    ui.host().on_back_started(fake_event());
    ui.host().on_back_pressed();
    compose_runtime::platform::run_ready();
    ui.host().on_back_started(fake_event());
    ui.host().on_back_pressed();
    ui.wait_until(|_| log.borrow().len() >= 3);
    assert_eq!(*log.borrow(), ["start", "start", "async"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testChildBackHandler
// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testSiblingBackHandlers
#[test]
fn the_last_predictive_handler_composed_takes_it() {
    let log = Log::default();
    let logged = log.clone();
    let mut ui = rule(move || {
        Collecting(logged.clone(), "first", true);
        Collecting(logged.clone(), "second", true);
    });
    ui.host().on_back_started(fake_event());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["second"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testDisabledChildBackHandler
// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testDisabledSiblingBackHandlers
#[test]
fn a_disabled_predictive_handler_leaves_it_to_the_one_before() {
    let log = Log::default();
    let logged = log.clone();
    let mut ui = rule(move || {
        Collecting(logged.clone(), "first", true);
        Collecting(logged.clone(), "second", false);
    });
    ui.host().on_back_started(fake_event());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["first"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testBackHandlerOnBackChanged
#[test]
fn a_predictive_handler_takes_a_new_closure() {
    let (log, name) = (Log::default(), mutable_state_of("first"));
    let (logged, read) = (log.clone(), name.handle());
    let mut ui = rule(move || Collecting(logged.clone(), read.get(), true));
    ui.host().on_back_started(fake_event());
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    name.set("second");
    ui.wait_for_idle();
    ui.host().on_back_started(fake_event());
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["first", "second"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testLifecycleChange
#[test]
fn a_predictive_handler_keeps_its_place_through_a_lifecycle() {
    let owner = TestLifecycleOwner::new(LifecycleState::Resumed);
    let log = Log::default();
    let logged = log.clone();
    let below = TestNavigationEventHandler::new();
    let mut ui = under_lifecycle(owner.lifecycle(), (*below).clone(), move || {
        Collecting(logged.clone(), "intercepted", true);
    });
    ui.wait_for_idle();
    owner.set_current_state(LifecycleState::Created);
    ui.wait_for_idle();
    owner.set_current_state(LifecycleState::Resumed);
    ui.wait_for_idle();
    ui.host().on_back_started(fake_event());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["intercepted"]);
    assert_eq!(below.on_back_started_invocations(), 0);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testHandleOnProgress
#[test]
fn a_gesture_s_progress_reaches_the_task() {
    let result = Rc::new(RefCell::new(Vec::new()));
    let seen = result.clone();
    let mut ui = rule(move || {
        let seen = seen.clone();
        PredictiveBackHandler!(
            on_back = move |progress: BackProgress| {
                let seen = seen.clone();
                async move {
                    let _ = progress.collect(|event| seen.borrow_mut().push(event.progress)).await;
                }
            }
        );
    });
    ui.host().on_back_started(fake_event());
    for progress in [0.25, 0.5, 0.75] {
        ui.host().on_back_progressed(BackEventCompat { progress, ..fake_event() });
    }
    ui.wait_for_idle();
    assert_eq!(*result.borrow(), [0.25, 0.5, 0.75]);
}

/// Logs what a gesture does, and "onBack cancelled" if it's given up.
#[composable(non_restartable)]
fn Catching(log: Rc<RefCell<Vec<String>>>) {
    PredictiveBackHandler!(
        on_back = move |progress: BackProgress| {
            let log = log.clone();
            async move {
                log.borrow_mut().push("start".to_string());
                let seen = log.clone();
                match progress.collect(|_| seen.borrow_mut().push("progress".to_string())).await {
                    Ok(()) => log.borrow_mut().push("completed".to_string()),
                    Err(cancelled) => log.borrow_mut().push(cancelled.to_string()),
                }
            }
        }
    );
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testHandleOnCancelled
#[test]
fn a_gesture_given_up_is_an_error_of_the_collection() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let logged = log.clone();
    let mut ui = rule(move || Catching(logged.clone()));
    ui.host().on_back_started(fake_event());
    ui.host().on_back_progressed(fake_event());
    ui.wait_for_idle();
    ui.host().on_back_cancelled();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start", "progress", "onBack cancelled"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testHandleOnCancelledWithoutCatch
#[test]
fn a_gesture_given_up_ends_a_task_that_goes_on_waiting() {
    let log = Log::default();
    let logged = log.clone();
    let mut ui = rule(move || {
        let logged = logged.clone();
        PredictiveBackHandler!(
            on_back = move |progress: BackProgress| {
                let logged = logged.clone();
                async move {
                    logged.borrow_mut().push("start");
                    let seen = logged.clone();
                    let _ = progress.collect(|_| seen.borrow_mut().push("progress")).await;
                    // What a cancelled coroutine's next suspension doesn't come back from.
                    delay(Duration::from_millis(1)).await;
                    logged.borrow_mut().push("completed");
                }
            }
        );
    });
    ui.host().on_back_started(fake_event());
    ui.host().on_back_progressed(fake_event());
    ui.wait_for_idle();
    ui.host().on_back_cancelled();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start", "progress"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testRestartCancelledBack
#[test]
fn a_gesture_after_one_given_up_completes() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let logged = log.clone();
    let mut ui = rule(move || Catching(logged.clone()));
    ui.host().on_back_started(fake_event());
    ui.host().on_back_progressed(fake_event());
    ui.wait_for_idle();
    ui.host().on_back_cancelled();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start", "progress", "onBack cancelled"]);
    log.borrow_mut().clear();
    ui.host().on_back_started(fake_event());
    ui.host().on_back_progressed(fake_event());
    ui.host().on_back_progressed(fake_event());
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start", "progress", "progress", "completed"]);
}

// Compose: activity/activity-compose/src/androidTest/java/androidx/activity/compose/PredictiveBackHandlerTest.kt, testOnBackCancelledCancelsAsyncJob
#[test]
fn a_gesture_given_up_ends_what_the_task_waits_for() {
    let (log, started) = (Log::default(), Rc::new(Cell::new(false)));
    let (logged, seen) = (log.clone(), started.clone());
    let mut ui = rule(move || {
        let (logged, seen) = (logged.clone(), seen.clone());
        PredictiveBackHandler!(
            on_back = move |progress: BackProgress| {
                let (logged, seen) = (logged.clone(), seen.clone());
                async move {
                    logged.borrow_mut().push("start");
                    seen.set(true);
                    delay(Duration::from_millis(300)).await;
                    logged.borrow_mut().push("async");
                    let _ = progress.collect(|_| {}).await;
                }
            }
        );
    });
    ui.host().on_back_started(fake_event());
    ui.wait_until(|_| started.get());
    ui.host().on_back_cancelled();
    ui.wait_for_idle();
    std::thread::sleep(Duration::from_millis(350));
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start"]);
}

/// Where it goes in the layers: a dialog's first, and the gesture goes to it to the end.
#[test]
fn a_dialog_s_predictive_handler_goes_first() {
    let log = Log::default();
    let logged = log.clone();
    let mut ui = rule(move || {
        Collecting(logged.clone(), "under", true);
        let logged = logged.clone();
        Dialog!(
            on_dismiss_request = || {},
            content = move || {
                Collecting(logged.clone(), "dialog", true);
            }
        );
    });
    ui.host().on_back_started(fake_event());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["dialog"]);
}

/// A handler leaving during its gesture ends its task.
#[test]
fn a_handler_leaving_during_its_gesture_ends_its_task() {
    let (log, show) = (Log::default(), mutable_state_of(true));
    let (logged, read) = (log.clone(), show.handle());
    let mut ui = rule(move || Shown!(read, logged.clone()));
    ui.host().on_back_started(fake_event());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start"]);
    show.set(false);
    ui.wait_for_idle();
    ui.host().on_back_pressed();
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["start"]);
    assert!(!ui.host().has_enabled_back_handlers());
}

#[composable]
fn Shown(show: MutableState<bool>, log: Log) {
    if show.get() {
        PredictiveBackHandler!(
            on_back = move |progress: BackProgress| {
                let log = log.clone();
                async move {
                    log.borrow_mut().push("start");
                    if progress.collect(|_| {}).await.is_ok() {
                        log.borrow_mut().push("completed");
                    }
                }
            }
        );
    }
}
