//! The dispatcher against AndroidX's `NavigationEventDispatcherTest`.

use std::cell::Cell;
use std::rc::Rc;

use compose_navigation_event::*;

use crate::support::*;

use NavigationEventDispatcher as Dispatcher;
use TestNavigationEventHandler as TestHandler;

const OVERLAY: i32 = Dispatcher::PRIORITY_OVERLAY;
const DEFAULT: i32 = Dispatcher::PRIORITY_DEFAULT;

/// (started, progressed, completed, cancelled) of back.
fn back(handler: &TestHandler) -> (u32, u32, u32, u32) {
    (
        handler.on_back_started_invocations(),
        handler.on_back_progressed_invocations(),
        handler.on_back_completed_invocations(),
        handler.on_back_cancelled_invocations(),
    )
}

/// The same of forward.
fn forward(handler: &TestHandler) -> (u32, u32, u32, u32) {
    (
        handler.on_forward_started_invocations(),
        handler.on_forward_progressed_invocations(),
        handler.on_forward_completed_invocations(),
        handler.on_forward_cancelled_invocations(),
    )
}

/// A dispatcher with `handler`, and an input.
fn with(handler: &TestHandler) -> (Dispatcher, TestInput) {
    let dispatcher = Dispatcher::new();
    dispatcher.add_handler(handler);
    let input = TestInput::new();
    dispatcher.add_input(&input);
    (dispatcher, input)
}

/// A parent and child, each with a handler.
fn parent_and_child() -> (Dispatcher, Dispatcher, TestHandler, TestHandler) {
    let parent = Dispatcher::new();
    let child = Dispatcher::with_parent(&parent);
    let (parent_handler, child_handler) = (TestHandler::new(), TestHandler::new());
    parent.add_handler(&parent_handler);
    child.add_handler(&child_handler);
    (parent, child, parent_handler, child_handler)
}

fn added_to(dispatcher: &Dispatcher) -> TestInput {
    let input = TestInput::new();
    dispatcher.add_input(&input);
    input
}

const DISPOSED: &str = "has already been disposed";

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onBackStarted_sendsEventToHandler
#[test]
fn back_started_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.back_started();
    assert_eq!(back(&handler), (1, 0, 0, 0));
    assert_eq!(forward(&handler).0, 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onBackProgressed_withStart_sendsEventToHandler
#[test]
fn back_progress_after_a_start_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.back_started();
    input.back_progressed();
    assert_eq!(back(&handler), (1, 1, 0, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onBackProgressed_withoutStart_ignoresProgress
#[test]
fn back_progress_with_no_start_is_ignored() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.back_progressed();
    assert_eq!(back(&handler), (0, 0, 0, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onBackCompleted_sendsEventToHandler
#[test]
fn back_completed_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.back_completed();
    assert_eq!(back(&handler), (0, 0, 1, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onBackCancelled_sendsEventToHandler
#[test]
fn back_cancelled_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.back_cancelled();
    assert_eq!(back(&handler), (0, 0, 0, 1));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, removeHandler_duringInProgressBackNavigation_sendsCancellation
#[test]
fn a_handler_removed_during_its_back_gesture_hears_it_cancelled() {
    let at_cancel = Rc::new(Cell::new(0));
    let seen = at_cancel.clone();
    let handler = TestHandler::on_back_cancelled(move |handler| {
        seen.set(handler.on_back_started_invocations());
    });
    let (_dispatcher, input) = with(&handler);
    input.back_started();
    assert_eq!(handler.on_back_started_invocations(), 1);
    handler.remove();
    assert_eq!(handler.on_back_cancelled_invocations(), 1);
    assert_eq!(at_cancel.get(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_handlerDisablesBack_doesNotSendCancellation
#[test]
fn a_handler_disabling_back_as_it_starts_still_completes() {
    let handler = TestHandler::on_back_started(|handler, _| handler.set_back_enabled(false));
    let (_dispatcher, input) = with(&handler);
    input.back_started();
    input.back_completed();
    assert_eq!(back(&handler), (1, 0, 1, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_handlerRemovesItselfOnBackStarted_sendsCancellation
#[test]
fn a_handler_removing_itself_as_back_starts_hears_it_cancelled() {
    let at_start = Rc::new(Cell::new(u32::MAX));
    let seen = at_start.clone();
    let handler = TestHandler::on_back_started(move |handler, _| {
        seen.set(handler.on_back_cancelled_invocations());
        handler.remove();
    });
    let (_dispatcher, input) = with(&handler);
    input.back_started();
    assert_eq!(handler.on_back_started_invocations(), 1);
    assert_eq!(handler.on_back_cancelled_invocations(), 1);
    assert_eq!(at_start.get(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_interleavedBackGestures_onlyProcessesOriginatingInput_completed
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_interleavedBackGestures_onlyProcessesOriginatingInput_cancelled
#[test]
fn interleaved_back_gestures_stay_with_the_input_that_started() {
    for completed in [true, false] {
        let (dispatcher, handler, input1) = dispatcher_with_handler();
        let input2 = added_to(&dispatcher);
        input1.back_started();
        input2.back_started();
        input1.back_progressed();
        input2.back_progressed();
        if completed {
            input1.back_completed();
            input2.back_completed();
            assert_eq!(back(&handler), (1, 1, 1, 0));
        } else {
            input1.back_cancelled();
            input2.back_cancelled();
            assert_eq!(back(&handler), (1, 1, 0, 1));
        }
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_conflictingGesturesFromSameInput_backWins_completed
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_conflictingGesturesFromSameInput_backWins_cancelled
#[test]
fn back_started_first_wins_over_forward_from_one_input() {
    for completed in [true, false] {
        let (_dispatcher, handler, input) = dispatcher_with_handler();
        input.back_started();
        input.forward_started();
        input.back_progressed();
        input.forward_progressed();
        if completed {
            input.back_completed();
            input.forward_completed();
            assert_eq!(back(&handler), (1, 1, 1, 0));
        } else {
            input.back_cancelled();
            input.forward_cancelled();
            assert_eq!(back(&handler), (1, 1, 0, 1));
        }
        assert_eq!(forward(&handler), (0, 0, 0, 0));
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onForwardStarted_sendsEventToHandler
#[test]
fn forward_started_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.forward_started();
    assert_eq!(forward(&handler), (1, 0, 0, 0));
    assert_eq!(back(&handler).0, 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onForwardProgressed_withStart_sendsEventToHandler
#[test]
fn forward_progress_after_a_start_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.forward_started();
    input.forward_progressed();
    assert_eq!(forward(&handler), (1, 1, 0, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onForwardProgressed_withoutStart_ignoresProgress
#[test]
fn forward_progress_with_no_start_is_ignored() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.forward_progressed();
    assert_eq!(forward(&handler), (0, 0, 0, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onForwardCompleted_sendsEventToHandler
#[test]
fn forward_completed_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.forward_completed();
    assert_eq!(forward(&handler), (0, 0, 1, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onForwardCancelled_sendsEventToHandler
#[test]
fn forward_cancelled_reaches_the_handler() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.forward_cancelled();
    assert_eq!(forward(&handler), (0, 0, 0, 1));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, removeHandler_duringInProgressForwardNavigation_sendsCancellation
#[test]
fn a_handler_removed_during_its_forward_gesture_hears_it_cancelled() {
    let at_cancel = Rc::new(Cell::new(0));
    let seen = at_cancel.clone();
    let handler = TestHandler::on_forward_cancelled(move |handler| {
        seen.set(handler.on_forward_started_invocations());
    });
    let (_dispatcher, input) = with(&handler);
    input.forward_started();
    assert_eq!(handler.on_forward_started_invocations(), 1);
    handler.remove();
    assert_eq!(handler.on_forward_cancelled_invocations(), 1);
    assert_eq!(at_cancel.get(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_handlerDisablesForward_doesNotSendCancellation
#[test]
fn a_handler_disabling_forward_as_it_starts_still_completes() {
    let handler = TestHandler::on_forward_started(|handler, _| handler.set_forward_enabled(false));
    let (_dispatcher, input) = with(&handler);
    input.forward_started();
    input.forward_completed();
    assert_eq!(forward(&handler), (1, 0, 1, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_handlerRemovesItselfOnForwardStarted_sendsCancellation
#[test]
fn a_handler_removing_itself_as_forward_starts_hears_it_cancelled() {
    let at_start = Rc::new(Cell::new(u32::MAX));
    let seen = at_start.clone();
    let handler = TestHandler::on_forward_started(move |handler, _| {
        seen.set(handler.on_forward_cancelled_invocations());
        handler.remove();
    });
    let (_dispatcher, input) = with(&handler);
    input.forward_started();
    assert_eq!(handler.on_forward_started_invocations(), 1);
    assert_eq!(handler.on_forward_cancelled_invocations(), 1);
    assert_eq!(at_start.get(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_interleavedForwardGestures_onlyProcessesOriginatingInput_completed
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_interleavedForwardGestures_onlyProcessesOriginatingInput_cancelled
#[test]
fn interleaved_forward_gestures_stay_with_the_input_that_started() {
    for completed in [true, false] {
        let (dispatcher, handler, input1) = dispatcher_with_handler();
        let input2 = added_to(&dispatcher);
        input1.forward_started();
        input2.forward_started();
        input1.forward_progressed();
        input2.forward_progressed();
        if completed {
            input1.forward_completed();
            input2.forward_completed();
            assert_eq!(forward(&handler), (1, 1, 1, 0));
        } else {
            input1.forward_cancelled();
            input2.forward_cancelled();
            assert_eq!(forward(&handler), (1, 1, 0, 1));
        }
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_conflictingGesturesFromSameInput_forwardWins_completed
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_conflictingGesturesFromSameInput_forwardWins_cancelled
#[test]
fn forward_started_first_wins_over_back_from_one_input() {
    for completed in [true, false] {
        let (_dispatcher, handler, input) = dispatcher_with_handler();
        input.forward_started();
        input.back_started();
        input.forward_progressed();
        input.back_progressed();
        if completed {
            input.forward_completed();
            input.back_completed();
            assert_eq!(forward(&handler), (1, 1, 1, 0));
        } else {
            input.forward_cancelled();
            input.back_cancelled();
            assert_eq!(forward(&handler), (1, 1, 0, 1));
        }
        assert_eq!(back(&handler), (0, 0, 0, 0));
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, setEnabled_duringInProgressNavigation_doesNotSendCancellation
#[test]
fn disabling_during_a_gesture_does_not_cancel_it() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.back_started();
    handler.set_back_enabled(false);
    assert_eq!(handler.on_back_cancelled_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_duringInProgressNavigation_ignoresNewHandlerForCurrentEvent
#[test]
fn a_handler_added_during_a_gesture_waits_for_the_next() {
    let (dispatcher, first, input) = dispatcher_with_handler();
    input.back_started();
    let second = TestHandler::new();
    dispatcher.add_handler(&second);
    input.back_completed();
    assert_eq!(back(&first), (1, 0, 1, 0));
    assert_eq!(back(&second), (0, 0, 0, 0));
    input.back_started();
    input.back_completed();
    assert_eq!(back(&first), (1, 0, 1, 0));
    assert_eq!(back(&second), (1, 0, 1, 0));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withNoEnabledHandlers_invokesBackFallback
#[test]
fn back_with_no_enabled_handler_runs_the_fallback() {
    let called = Rc::new(Cell::new(false));
    let told = called.clone();
    let dispatcher = Dispatcher::with_back_fallback(move || told.set(true));
    let handler = TestHandler::new();
    dispatcher.add_handler(&handler);
    let input = added_to(&dispatcher);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
    assert!(!called.get());
    handler.set_back_enabled(false);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
    assert!(called.get());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withNoEnabledHandlers_doesNotInvokeBackFallbackForForward
#[test]
fn forward_does_not_run_the_back_fallback() {
    let called = Rc::new(Cell::new(false));
    let told = called.clone();
    let dispatcher = Dispatcher::with_back_fallback(move || told.set(true));
    let handler = TestHandler::enabled(true, false);
    dispatcher.add_handler(&handler);
    let input = added_to(&dispatcher);
    input.forward_completed();
    assert_eq!(handler.on_forward_completed_invocations(), 0);
    assert!(!called.get());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withNoEnabledHandlers_invokesForwardFallback
#[test]
fn forward_with_no_enabled_handler_runs_its_fallback() {
    let called = Rc::new(Cell::new(false));
    let told = called.clone();
    let dispatcher =
        Dispatcher::with_fallbacks(Rc::new(|| {}), Some(Rc::new(move || told.set(true))));
    let handler = TestHandler::new();
    dispatcher.add_handler(&handler);
    let input = added_to(&dispatcher);
    input.forward_completed();
    assert_eq!(handler.on_forward_completed_invocations(), 1);
    assert!(!called.get());
    handler.set_forward_enabled(false);
    input.forward_completed();
    assert_eq!(handler.on_forward_completed_invocations(), 1);
    assert!(called.get());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withNoEnabledHandlers_doesNotInvokeForwardFallbackForBack
#[test]
fn back_does_not_run_the_forward_fallback() {
    let called = Rc::new(Cell::new(false));
    let told = called.clone();
    let dispatcher =
        Dispatcher::with_fallbacks(Rc::new(|| {}), Some(Rc::new(move || told.set(true))));
    let handler = TestHandler::enabled(false, true);
    dispatcher.add_handler(&handler);
    let input = added_to(&dispatcher);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 0);
    assert!(!called.get());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withOverlayHandler_prioritizesOverlay
#[test]
fn an_overlay_goes_before_a_default_handler() {
    let dispatcher = Dispatcher::new();
    let (overlay, normal) = (TestHandler::new(), TestHandler::new());
    dispatcher.add_handler_with_priority(&overlay, OVERLAY);
    dispatcher.add_handler_with_priority(&normal, DEFAULT);
    added_to(&dispatcher).back_completed();
    assert_eq!(overlay.on_back_completed_invocations(), 1);
    assert_eq!(normal.on_back_completed_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withDisabledOverlay_invokesDefaultHandler
#[test]
fn a_disabled_overlay_leaves_it_to_the_default_handler() {
    let dispatcher = Dispatcher::new();
    let (overlay, normal) = (TestHandler::new(), TestHandler::new());
    dispatcher.add_handler_with_priority(&overlay, OVERLAY);
    dispatcher.add_handler_with_priority(&normal, DEFAULT);
    overlay.set_back_enabled(false);
    added_to(&dispatcher).back_completed();
    assert_eq!(overlay.on_back_completed_invocations(), 0);
    assert_eq!(normal.on_back_completed_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_toSecondDispatcher_throwsException
#[test]
fn a_handler_of_a_dispatcher_can_not_join_another() {
    let handler = TestHandler::new();
    let first = Dispatcher::new();
    first.add_handler(&handler);
    let second = Dispatcher::new();
    let text = panic_text(|| second.add_handler(&handler));
    assert!(text.contains("is already registered with a dispatcher"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_withAlreadyRegisteredDispatcher_ignoresCall
#[test]
fn a_handler_added_twice_is_added_once() {
    let handler = TestHandler::new();
    let dispatcher = Dispatcher::new();
    dispatcher.add_handler(&handler);
    dispatcher.add_handler(&handler);
    added_to(&dispatcher).back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_withInvalidPriority_throwsException
#[test]
fn a_handler_of_an_unknown_priority_panics() {
    let dispatcher = Dispatcher::new();
    let handler = TestHandler::new();
    let text = panic_text(|| dispatcher.add_handler_with_priority(&handler, -99));
    assert!(text.contains("Unsupported priority value: -99"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_multipleOverlays_prioritizesLastAdded
#[test]
fn the_last_overlay_added_goes_first() {
    let dispatcher = Dispatcher::new();
    let (first, second, normal) = (TestHandler::new(), TestHandler::new(), TestHandler::new());
    dispatcher.add_handler_with_priority(&normal, DEFAULT);
    dispatcher.add_handler_with_priority(&first, OVERLAY);
    dispatcher.add_handler_with_priority(&second, OVERLAY);
    added_to(&dispatcher).back_completed();
    assert_eq!(second.on_back_completed_invocations(), 1);
    assert_eq!(first.on_back_completed_invocations(), 0);
    assert_eq!(normal.on_back_completed_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withNoHandlers_invokesBackFallback
#[test]
fn back_with_no_handler_runs_the_fallback() {
    let called = Rc::new(Cell::new(false));
    let told = called.clone();
    let dispatcher = Dispatcher::with_back_fallback(move || told.set(true));
    added_to(&dispatcher).back_completed();
    assert!(called.get());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, fallback_propagatesToChildAndClearsOnDispose
#[test]
fn a_child_has_its_parent_s_back_fallback_until_disposed() {
    let fallback: OnBackCompletedFallback = Rc::new(|| {});
    let parent = Dispatcher::with_fallbacks(fallback.clone(), None);
    let child = Dispatcher::with_parent(&parent);
    assert!(Rc::ptr_eq(&child.on_back_completed_fallback().unwrap(), &fallback));
    child.dispose();
    assert!(child.on_back_completed_fallback().is_none());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, forwardFallback_propagatesToChildAndClearsOnDispose
#[test]
fn a_child_has_its_parent_s_forward_fallback_until_disposed() {
    let fallback: OnForwardCompletedFallback = Rc::new(|| {});
    let parent = Dispatcher::with_fallbacks(Rc::new(|| {}), Some(fallback.clone()));
    let child = Dispatcher::with_parent(&parent);
    assert!(Rc::ptr_eq(&child.on_forward_completed_fallback().unwrap(), &fallback));
    child.dispose();
    assert!(child.on_forward_completed_fallback().is_none());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, setEnabled_onDisabledHandler_reEnablesEventReceiving
#[test]
fn a_handler_enabled_again_takes_events_again() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    handler.set_back_enabled(false);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 0);
    handler.set_back_enabled(true);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_withoutStart_ignoresProgress
#[test]
fn progress_with_no_start_is_ignored_both_ways() {
    let (_dispatcher, handler, input) = dispatcher_with_handler();
    input.back_progressed();
    input.forward_progressed();
    assert_eq!(handler.on_back_progressed_invocations(), 0);
    assert_eq!(handler.on_forward_progressed_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_removedAndReadded_actsAsNew
#[test]
fn a_handler_removed_and_added_again_takes_events_again() {
    let (dispatcher, handler, input) = dispatcher_with_handler();
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
    handler.remove();
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
    dispatcher.add_handler(&handler);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 2);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addInput_onAdd_callsOnAttach
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addInput_twice_callsOnAttachOnce
#[test]
fn an_input_added_hears_it_once() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    dispatcher.add_input(&input);
    assert_eq!(input.heard.added.get(), 1);
    assert_eq!(*input.heard.dispatcher.borrow(), Some(dispatcher));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addInput_allPriorities_invokesCallbacksInOrder
#[test]
fn an_input_of_any_priority_hears_in_one_order() {
    let dispatcher = Dispatcher::new();
    let expected =
        ["on_added", "on_history_changed", "on_has_enabled_handlers_changed", "on_removed"];
    let input = TestInput::new();
    dispatcher.remove_input(&input);
    assert!(input.heard.order.borrow().is_empty());
    dispatcher.add_input(&input);
    dispatcher.remove_input(&input);
    assert_eq!(*input.heard.order.borrow(), expected);
    input.heard.order.borrow_mut().clear();
    dispatcher.add_input_with_priority(&input, DEFAULT);
    dispatcher.remove_input(&input);
    assert_eq!(*input.heard.order.borrow(), expected);
    input.heard.order.borrow_mut().clear();
    dispatcher.add_input_with_priority(&input, OVERLAY);
    dispatcher.remove_input(&input);
    assert_eq!(*input.heard.order.borrow(), expected);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addInput_toSecondDispatcher_throwsException
#[test]
fn an_input_of_a_dispatcher_can_not_join_another() {
    let first = Dispatcher::new();
    let input = added_to(&first);
    let second = Dispatcher::new();
    let text = panic_text(|| second.add_input(&input));
    assert!(text.contains("is already added to dispatcher"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addInput_withAlreadyRegisteredDispatcher_ignoresCall
#[test]
fn an_input_added_twice_does_not_panic() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    dispatcher.add_input(&input);
    assert_eq!(input.heard.added.get(), 1);
    dispatcher.remove_input(&input);
    assert_eq!(input.heard.removed.get(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, removeInput_onRemove_callsOnDetach
#[test]
fn an_input_removed_hears_it() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    dispatcher.remove_input(&input);
    assert_eq!(input.heard.removed.get(), 1);
    assert!(input.heard.dispatcher.borrow().is_none());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, removeInput_whenNotRegistered_doesNotCallOnDetach
#[test]
fn an_input_not_added_hears_no_removal() {
    let dispatcher = Dispatcher::new();
    let input = TestInput::new();
    dispatcher.remove_input(&input);
    assert_eq!(input.heard.removed.get(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispose_onCall_detachesInputs
#[test]
fn a_dispatcher_disposed_removes_its_inputs() {
    let dispatcher = Dispatcher::new();
    let (first, second) = (added_to(&dispatcher), added_to(&dispatcher));
    dispatcher.dispose();
    for input in [&first, &second] {
        assert_eq!(input.heard.removed.get(), 1);
        assert!(input.heard.dispatcher.borrow().is_none());
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addInput_onDisposedDispatcher_throws
#[test]
fn an_input_added_to_a_disposed_dispatcher_panics() {
    let dispatcher = Dispatcher::new();
    dispatcher.dispose();
    let text = panic_text(|| dispatcher.add_input(&TestInput::new()));
    assert!(text.contains("This NavigationEventDispatcher has already been disposed"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, removeInput_onDisposedDispatcher_throws
#[test]
fn an_input_removed_from_a_disposed_dispatcher_panics() {
    let dispatcher = Dispatcher::new();
    dispatcher.dispose();
    let text = panic_text(|| dispatcher.remove_input(&TestInput::new()));
    assert!(text.contains("This NavigationEventDispatcher has already been disposed"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, onHasEnabledHandlerChanged_onHandlerChange_notifiesInput
#[test]
fn an_input_hears_of_enabled_handlers() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    assert_eq!(input.enabled(), [false]);
    let handler = TestHandler::new();
    dispatcher.add_handler(&handler);
    assert_eq!(input.enabled(), [false, true]);
    handler.set_back_enabled(false);
    handler.set_forward_enabled(false);
    assert_eq!(input.enabled(), [false, true, false]);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, onInfoChanged_onStateChanges_notifiesInputCorrectly
#[test]
fn an_input_hears_of_the_history() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    assert_eq!(input.heard.history_changes.get(), 1);
    assert_eq!(*input.heard.history.borrow(), NavigationEventHistory::default());
    let a = home("A");
    let handler_a = TestHandler::with_info(a.clone(), vec![], vec![]);
    dispatcher.add_handler(&handler_a);
    assert_eq!(input.heard.history_changes.get(), 2);
    assert_eq!(
        *input.heard.history.borrow(),
        NavigationEventHistory::new(a.clone(), vec![], vec![])
    );
    let b = details("B");
    let handler_b = TestHandler::with_info(b.clone(), vec![a.clone()], vec![]);
    dispatcher.add_handler(&handler_b);
    assert_eq!(input.heard.history_changes.get(), 3);
    assert_eq!(
        *input.heard.history.borrow(),
        NavigationEventHistory::new(b.clone(), vec![a.clone()], vec![])
    );
    let updated = details("B_updated");
    handler_b.set_info(updated.clone(), vec![a.clone()], vec![b.clone()]);
    assert_eq!(input.heard.history_changes.get(), 4);
    assert_eq!(
        *input.heard.history.borrow(),
        NavigationEventHistory::new(updated, vec![a.clone()], vec![b])
    );
    handler_b.remove();
    assert_eq!(input.heard.history_changes.get(), 5);
    assert_eq!(*input.heard.history.borrow(), NavigationEventHistory::new(a, vec![], vec![]));
    dispatcher.remove_input(&input);
    dispatcher.add_handler(&TestHandler::with_info(home("C"), vec![], vec![]));
    assert_eq!(input.heard.history_changes.get(), 5);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, onInfoChanged_onInfoIsUpdated_isTriggeredOnlyOnActualChange
#[test]
fn an_input_hears_of_the_history_only_when_it_changes() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    let (current, back, forward) = (home("current1"), details("back1"), details("forward1"));
    let handler =
        TestHandler::with_info(current.clone(), vec![back.clone()], vec![forward.clone()]);
    dispatcher.add_handler(&handler);
    assert_eq!(input.heard.history_changes.get(), 2);
    handler.set_info(current, vec![back.clone()], vec![forward.clone()]);
    assert_eq!(input.heard.history_changes.get(), 2);
    let current = home("current2");
    handler.set_info(current.clone(), vec![back], vec![forward.clone()]);
    assert_eq!(input.heard.history_changes.get(), 3);
    let back = details("back2");
    handler.set_info(current.clone(), vec![back.clone()], vec![forward]);
    assert_eq!(input.heard.history_changes.get(), 4);
    handler.set_info(current, vec![back], vec![details("forward2")]);
    assert_eq!(input.heard.history_changes.get(), 5);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, onHasEnabledHandlerChanged_afterInputRemoved_doesNotNotify
#[test]
fn an_input_removed_hears_no_more() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    dispatcher.add_handler(&TestHandler::new());
    assert_eq!(input.enabled(), [false, true]);
    dispatcher.remove_input(&input);
    dispatcher.add_handler(&TestHandler::new());
    assert_eq!(input.enabled(), [false, true]);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispose_onParent_detachesChildInputs
#[test]
fn a_parent_disposed_removes_its_child_s_inputs() {
    let parent = Dispatcher::new();
    let child = Dispatcher::with_parent(&parent);
    let (parent_input, child_input) = (added_to(&parent), added_to(&child));
    parent.dispose();
    for input in [&parent_input, &child_input] {
        assert_eq!(input.heard.removed.get(), 1);
        assert!(input.heard.dispatcher.borrow().is_none());
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, onHasEnabledCallbacksChanged_notifiesInputsByPriority
#[test]
fn inputs_hear_of_their_priority_s_handlers() {
    let dispatcher = Dispatcher::new();
    let unspecified = added_to(&dispatcher);
    let (default, overlay) = (TestInput::new(), TestInput::new());
    dispatcher.add_input_with_priority(&default, DEFAULT);
    dispatcher.add_input_with_priority(&overlay, OVERLAY);
    dispatcher.add_handler_with_priority(&TestHandler::enabled(true, false), OVERLAY);
    assert_eq!(default.enabled(), [false]);
    assert_eq!(overlay.enabled(), [false, true]);
    assert_eq!(unspecified.enabled(), [false, true]);
    dispatcher.add_handler_with_priority(&TestHandler::enabled(true, false), DEFAULT);
    assert_eq!(default.enabled(), [false, true]);
    assert_eq!(overlay.enabled(), [false, true]);
    assert_eq!(unspecified.enabled(), [false, true]);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, onHasEnabledHandlerChanged_unspecifiedInput_notifiedOnlyOnAggregateChange
#[test]
fn an_input_of_no_priority_hears_only_when_all_change() {
    let dispatcher = Dispatcher::new();
    let unspecified = added_to(&dispatcher);
    assert_eq!(unspecified.enabled(), [false]);
    let default = TestHandler::enabled(true, true);
    dispatcher.add_handler_with_priority(&default, DEFAULT);
    assert_eq!(unspecified.enabled(), [false, true]);
    let overlay = TestHandler::enabled(true, true);
    dispatcher.add_handler_with_priority(&overlay, OVERLAY);
    assert_eq!(unspecified.enabled(), [false, true]);
    default.remove();
    assert_eq!(unspecified.enabled(), [false, true]);
    overlay.remove();
    assert_eq!(unspecified.enabled(), [false, true, false]);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, removeInput_withPriority_stopsReceivingNotifications
#[test]
fn an_input_of_a_priority_removed_hears_no_more() {
    let dispatcher = Dispatcher::new();
    let overlay = TestInput::new();
    dispatcher.add_input_with_priority(&overlay, OVERLAY);
    let handler = TestHandler::new();
    dispatcher.add_handler_with_priority(&handler, OVERLAY);
    assert_eq!(overlay.enabled(), [false, true]);
    dispatcher.remove_input(&overlay);
    handler.set_back_enabled(false);
    assert_eq!(overlay.enabled(), [false, true]);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addInput_withInvalidPriority_throwsException
#[test]
fn an_input_of_an_unknown_priority_panics() {
    let dispatcher = Dispatcher::new();
    let input = TestInput::new();
    let text = panic_text(|| dispatcher.add_input_with_priority(&input, -99));
    assert!(text.contains("Unsupported priority value: -99"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, init_withParent_sharesHandlers
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_toParentThenChild_ordersLIFO
#[test]
fn a_child_shares_its_parent_s_handlers_the_last_first() {
    let (parent, _child, parent_handler, child_handler) = parent_and_child();
    added_to(&parent).back_started();
    assert_eq!(parent_handler.on_back_started_invocations(), 0);
    assert_eq!(child_handler.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, init_withoutParent_hasIndependentHandlers
#[test]
fn two_roots_do_not_share() {
    let (a, b) = (Dispatcher::new(), Dispatcher::new());
    let (handler_a, handler_b) = (TestHandler::new(), TestHandler::new());
    a.add_handler(&handler_a);
    b.add_handler(&handler_b);
    added_to(&a).back_started();
    assert_eq!(handler_a.on_back_started_invocations(), 1);
    assert_eq!(handler_b.on_back_started_invocations(), 0);
    added_to(&b).back_started();
    assert_eq!(handler_a.on_back_started_invocations(), 1);
    assert_eq!(handler_b.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_toChild_isDispatchedViaParent
#[test]
fn a_child_s_handler_takes_what_the_parent_s_input_sends() {
    let parent = Dispatcher::new();
    let child = Dispatcher::with_parent(&parent);
    let handler = TestHandler::new();
    child.add_handler(&handler);
    added_to(&parent).back_started();
    assert_eq!(handler.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_multipleDispatchers_prioritizesLastAdded
#[test]
fn of_siblings_the_last_handler_added_goes_first() {
    let parent = Dispatcher::new();
    let (child1, child2) = (Dispatcher::with_parent(&parent), Dispatcher::with_parent(&parent));
    let (parent_handler, handler1, handler2) =
        (TestHandler::new(), TestHandler::new(), TestHandler::new());
    parent.add_handler(&parent_handler);
    child2.add_handler(&handler2);
    child1.add_handler(&handler1);
    added_to(&parent).back_started();
    assert_eq!(parent_handler.on_back_started_invocations(), 0);
    assert_eq!(handler2.on_back_started_invocations(), 0);
    assert_eq!(handler1.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispose_onChild_parentStillReceivesEvents
#[test]
fn a_child_disposed_leaves_its_parent() {
    let (parent, child, parent_handler, child_handler) = parent_and_child();
    child.dispose();
    added_to(&parent).back_started();
    assert_eq!(parent_handler.on_back_started_invocations(), 1);
    assert_eq!(child_handler.on_back_started_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispose_onParent_cascadesAndDisablesChildren
#[test]
fn a_parent_disposed_disposes_its_child() {
    let parent = Dispatcher::new();
    let child = Dispatcher::with_parent(&parent);
    parent.dispose();
    for dispatcher in [&parent, &child] {
        let text = panic_text(|| dispatcher.add_input(&TestInput::new()));
        assert!(text.contains(DISPOSED), "{text}");
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispose_onGrandparent_cascadesAndDisablesHierarchy
#[test]
fn a_grandparent_disposed_disposes_them_all() {
    let grandparent = Dispatcher::new();
    let parent = Dispatcher::with_parent(&grandparent);
    let child = Dispatcher::with_parent(&parent);
    grandparent.dispose();
    for dispatcher in [&grandparent, &parent, &child] {
        let text = panic_text(|| dispatcher.add_input(&TestInput::new()));
        assert!(text.contains(DISPOSED), "{text}");
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispose_doesNotThrow_ConcurrentModificationException
#[test]
fn disposing_with_many_inputs_and_handlers_does_not_panic() {
    let dispatcher = Dispatcher::new();
    let (mut inputs, mut handlers) = (Vec::new(), Vec::new());
    for _ in 0..10 {
        let (input, handler) = (TestInput::new(), TestHandler::new());
        dispatcher.add_input(&input);
        dispatcher.add_handler(&handler);
        inputs.push(input);
        handlers.push(handler);
    }
    dispatcher.dispose();
    for input in &inputs {
        assert_eq!(input.heard.removed.get(), 1);
    }
    // Each is free to go on another.
    let other = Dispatcher::new();
    for handler in &handlers {
        other.add_handler(handler);
    }
}

/// A child dropped without `dispose`: nothing it had stays in the processor it shared, and its
/// parent hears as if it had been disposed (D845).
#[test]
fn a_child_dropped_leaves_nothing_behind() {
    let parent = Dispatcher::new();
    let input = added_to(&parent);
    let held = Rc::new(());
    let removed = Rc::new(Cell::new(0));
    // Not a `TestInput`, which holds on to its dispatcher.
    let child_input = DirectNavigationEventInput::with_callbacks(CountingRemovals(removed.clone()));
    let handler = TestHandler::new();
    {
        let child = Dispatcher::with_parent(&parent);
        child.add_input(&child_input);
        let kept = held.clone();
        let captured = TestHandler::on_back_started(move |_, _| {
            let _ = &kept;
        });
        child.add_handler(&captured);
        drop(captured);
        child.add_handler(&handler);
        assert!(input.has_enabled_back_handlers());
    }
    assert_eq!(Rc::strong_count(&held), 1, "the handler went with the child");
    assert_eq!(removed.get(), 1);
    assert!(!input.has_enabled_back_handlers());
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 0);
    // It can go elsewhere: nothing says it's registered.
    parent.add_handler(&handler);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
}

struct CountingRemovals(Rc<Cell<u32>>);

impl NavigationEventInputCallbacks for CountingRemovals {
    fn on_removed(&self) {
        self.0.set(self.0.get() + 1);
    }
}

/// One in its gesture is told, as by `dispose`.
#[test]
fn a_child_dropped_gives_up_its_handler_s_gesture() {
    let parent = Dispatcher::new();
    let input = added_to(&parent);
    let handler = TestHandler::new();
    {
        let child = Dispatcher::with_parent(&parent);
        child.add_handler(&handler);
        input.back_started();
    }
    assert_eq!(handler.on_back_cancelled_invocations(), 1);
    // And a new gesture starts, once its input's is over.
    input.back_cancelled();
    let next = TestHandler::new();
    parent.add_handler(&next);
    input.back_started();
    assert_eq!(next.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_whenTrue_dispatchesEvents
#[test]
fn an_enabled_dispatcher_dispatches() {
    let (dispatcher, handler, input) = dispatcher_with_handler();
    dispatcher.set_enabled(true);
    input.back_started();
    assert_eq!(handler.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_whenFalse_doesNotDispatchEvents
#[test]
fn a_disabled_dispatcher_does_not() {
    let (dispatcher, handler, input) = dispatcher_with_handler();
    dispatcher.set_enabled(false);
    input.back_started();
    assert_eq!(handler.on_back_started_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_parentDisabled_disablesChildDispatch
#[test]
fn a_parent_disabled_disables_its_child() {
    let (parent, child, parent_handler, child_handler) = parent_and_child();
    parent.set_enabled(false);
    added_to(&child).back_started();
    assert_eq!(parent_handler.on_back_started_invocations(), 0);
    assert_eq!(child_handler.on_back_started_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_childDisabled_doesNotDispatch
#[test]
fn a_child_disabled_does_not_dispatch() {
    let (_parent, child, parent_handler, child_handler) = parent_and_child();
    child.set_enabled(false);
    added_to(&child).back_started();
    assert_eq!(parent_handler.on_back_started_invocations(), 0);
    assert_eq!(child_handler.on_back_started_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_childDisabled_parentStillDispatches
#[test]
fn a_child_disabled_leaves_its_parent_dispatching() {
    let (parent, child, parent_handler, child_handler) = parent_and_child();
    child.set_enabled(false);
    added_to(&parent).back_started();
    assert_eq!(child_handler.on_back_started_invocations(), 0);
    assert_eq!(parent_handler.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_parentReEnabled_reEnablesChildDispatch
#[test]
fn a_parent_enabled_again_enables_its_child() {
    let (parent, child, parent_handler, child_handler) = parent_and_child();
    parent.set_enabled(false);
    let input = added_to(&child);
    input.back_completed();
    assert_eq!(child_handler.on_back_completed_invocations(), 0);
    parent.set_enabled(true);
    input.back_completed();
    assert_eq!(child_handler.on_back_completed_invocations(), 1);
    assert_eq!(parent_handler.on_back_completed_invocations(), 0);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_parentReEnabled_childHandlerReceivesEvents
#[test]
fn a_parent_enabled_again_gives_its_child_s_handler_events() {
    let (parent, _child, parent_handler, child_handler) = parent_and_child();
    parent.set_enabled(false);
    let input = added_to(&parent);
    input.back_completed();
    assert_eq!(parent_handler.on_back_completed_invocations(), 0);
    assert_eq!(child_handler.on_back_completed_invocations(), 0);
    parent.set_enabled(true);
    input.back_completed();
    assert_eq!(parent_handler.on_back_completed_invocations(), 0);
    assert_eq!(child_handler.on_back_completed_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_grandparentDisabled_disablesGrandchildDispatch
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, isEnabled_grandparentDisabled_grandchildHandlerDoesNotReceiveEvents
#[test]
fn a_grandparent_disabled_disables_them_all() {
    let grandparent = Dispatcher::new();
    let parent = Dispatcher::with_parent(&grandparent);
    let child = Dispatcher::with_parent(&parent);
    let handlers = [TestHandler::new(), TestHandler::new(), TestHandler::new()];
    grandparent.add_handler(&handlers[0]);
    parent.add_handler(&handlers[1]);
    child.add_handler(&handlers[2]);
    grandparent.set_enabled(false);
    added_to(&child).back_started();
    added_to(&grandparent).back_started();
    for handler in &handlers {
        assert_eq!(handler.on_back_started_invocations(), 0);
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, handlerIsEnabled_whenDispatcherDisabled_doesNotReceiveEvents
#[test]
fn a_handler_of_a_dispatcher_disabled_gets_nothing() {
    let (dispatcher, handler, input) = dispatcher_with_handler();
    input.back_started();
    assert_eq!(handler.on_back_started_invocations(), 1);
    dispatcher.set_enabled(false);
    input.back_started();
    assert_eq!(handler.on_back_started_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, handlerIsEnabled_whenDispatcherReEnabled_receivesEvents
#[test]
fn a_handler_of_a_dispatcher_enabled_again_gets_events() {
    let (dispatcher, handler, input) = dispatcher_with_handler();
    dispatcher.set_enabled(false);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 0);
    dispatcher.set_enabled(true);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, addHandler_onDisposedDispatcher_throws
#[test]
fn a_handler_added_to_a_disposed_dispatcher_panics() {
    let dispatcher = Dispatcher::new();
    dispatcher.dispose();
    let text = panic_text(|| dispatcher.add_handler(&TestHandler::new()));
    assert!(text.contains(DISPOSED), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, handleOnStarted_onDisposedDispatcher_throws
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, handleOnProgressed_onDisposedDispatcher_throws
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, handleOnCompleted_onDisposedDispatcher_throws
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, handleOnCancelled_onDisposedDispatcher_throws
#[test]
fn an_input_can_not_be_used_with_a_disposed_dispatcher() {
    let dispatcher = Dispatcher::new();
    dispatcher.dispose();
    let text = panic_text(|| {
        let input = added_to(&dispatcher);
        input.back_started();
    });
    assert!(text.contains(DISPOSED), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispose_onDisposedDispatcher_throws
#[test]
fn disposing_twice_panics() {
    let dispatcher = Dispatcher::new();
    dispatcher.dispose();
    let text = panic_text(|| dispatcher.dispose());
    assert!(text.contains(DISPOSED), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, setEnabled_onDisposedDispatcher_throws
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, setDisabled_onDisposedDispatcher_throws
#[test]
fn enabling_a_disposed_dispatcher_panics() {
    let dispatcher = Dispatcher::new();
    dispatcher.dispose();
    let text = panic_text(|| dispatcher.set_enabled(false));
    assert!(text.contains(DISPOSED), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onBackCompletedToUnimplementedHandler_throwsUnsupportedOperationException
#[test]
fn a_handler_with_no_on_back_completed_panics() {
    struct Unimplemented;
    impl NavigationEventCallbacks for Unimplemented {}
    let dispatcher = Dispatcher::new();
    dispatcher.add_handler(&NavigationEventHandler::new(
        NavigationEventInfo::none(),
        true,
        false,
        Unimplemented,
    ));
    let input = DirectNavigationEventInput::new();
    dispatcher.add_input(&input);
    let text = panic_text(|| input.back_completed());
    assert!(text.contains("must override 'onBackCompleted()' to handle the callback"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, dispatch_onForwardCompletedToUnimplementedHandler_throwsUnsupportedOperationException
#[test]
fn a_handler_with_no_on_forward_completed_panics() {
    struct Unimplemented;
    impl NavigationEventCallbacks for Unimplemented {}
    let dispatcher = Dispatcher::new();
    dispatcher.add_handler(&NavigationEventHandler::new(
        NavigationEventInfo::none(),
        false,
        true,
        Unimplemented,
    ));
    let input = DirectNavigationEventInput::new();
    dispatcher.add_input(&input);
    let text = panic_text(|| input.forward_completed());
    assert!(text.contains("must override 'onForwardCompleted()' to handle the callback"), "{text}");
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, transitionState_initialState_isIdle
#[test]
fn the_transition_state_starts_idle() {
    assert_eq!(Dispatcher::new().transition_state(), NavigationEventTransitionState::Idle);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, transitionState_onFullBackCompleted_emitsInProgressThenIdle
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, transitionState_onFullBackCancelled_returnsToIdle
#[test]
fn the_transition_state_follows_a_back_gesture() {
    use NavigationEventTransitionState::*;
    let (dispatcher, _handler, input) = dispatcher_with_handler();
    assert_eq!(dispatcher.transition_state(), Idle);
    let start = NavigationEvent { progress: 0.1, ..Default::default() };
    input.dispatch_on_back_started(start);
    let direction = NavigationEventTransitionState::TRANSITIONING_BACK;
    assert_eq!(dispatcher.transition_state(), InProgress { latest_event: start, direction });
    let progress = NavigationEvent { progress: 0.5, ..Default::default() };
    input.dispatch_on_back_progressed(progress);
    assert_eq!(dispatcher.transition_state(), InProgress { latest_event: progress, direction });
    input.dispatch_on_back_completed();
    assert_eq!(dispatcher.transition_state(), Idle);
    input.dispatch_on_back_started(start);
    assert!(matches!(dispatcher.transition_state(), InProgress { .. }));
    input.dispatch_on_back_cancelled();
    assert_eq!(dispatcher.transition_state(), Idle);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, transitionState_onFullForwardCompleted_emitsInProgressThenIdle
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, transitionState_onFullForwardCancelled_returnsToIdle
#[test]
fn the_transition_state_follows_a_forward_gesture() {
    use NavigationEventTransitionState::*;
    let (dispatcher, _handler, input) = dispatcher_with_handler();
    assert_eq!(dispatcher.transition_state(), Idle);
    let start = NavigationEvent { progress: 0.1, ..Default::default() };
    input.dispatch_on_forward_started(start);
    let direction = NavigationEventTransitionState::TRANSITIONING_FORWARD;
    assert_eq!(dispatcher.transition_state(), InProgress { latest_event: start, direction });
    let progress = NavigationEvent { progress: 0.5, ..Default::default() };
    input.dispatch_on_forward_progressed(progress);
    assert_eq!(dispatcher.transition_state(), InProgress { latest_event: progress, direction });
    input.dispatch_on_forward_completed();
    assert_eq!(dispatcher.transition_state(), Idle);
    input.dispatch_on_forward_started(start);
    assert!(matches!(dispatcher.transition_state(), InProgress { .. }));
    input.dispatch_on_forward_cancelled();
    assert_eq!(dispatcher.transition_state(), Idle);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, historyState_initialState_isEmpty
#[test]
fn the_history_starts_empty() {
    assert_eq!(Dispatcher::new().history(), NavigationEventHistory::default());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, historyState_onHandlerChanges_updatesHistoryStack
#[test]
fn the_history_follows_the_handlers() {
    let dispatcher = Dispatcher::new();
    let home_info = home("home");
    let home_handler = TestHandler::with_info(home_info.clone(), vec![], vec![]);
    dispatcher.add_handler(&home_handler);
    let at_home = NavigationEventHistory::new(home_info.clone(), vec![], vec![]);
    assert_eq!(dispatcher.history(), at_home);
    let details_info = details("details");
    let details_handler =
        TestHandler::with_info(details_info.clone(), vec![home_info.clone()], vec![]);
    dispatcher.add_handler(&details_handler);
    assert_eq!(
        dispatcher.history(),
        NavigationEventHistory::new(details_info, vec![home_info], vec![])
    );
    details_handler.remove();
    assert_eq!(dispatcher.history(), at_home);
    home_handler.remove();
    assert_eq!(dispatcher.history(), NavigationEventHistory::default());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, historyState_remainsStableDuringGestureLifecycle
#[test]
fn a_gesture_leaves_the_history_alone() {
    let dispatcher = Dispatcher::new();
    let input = added_to(&dispatcher);
    let info = home("home");
    dispatcher.add_handler(&TestHandler::with_info(info.clone(), vec![], vec![]));
    assert_eq!(input.heard.history_changes.get(), 2);
    assert_eq!(dispatcher.history().merged_history(), [info]);
    input.back_started();
    input.back_progressed();
    input.back_cancelled();
    input.back_completed();
    assert_eq!(input.heard.history_changes.get(), 2);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, historyState_onSetInfo_updatesStack
#[test]
fn set_info_updates_the_history() {
    let dispatcher = Dispatcher::new();
    let handler = TestHandler::with_info(home("A"), vec![], vec![]);
    dispatcher.add_handler(&handler);
    assert_eq!(dispatcher.history(), NavigationEventHistory::new(home("A"), vec![], vec![]));
    handler.set_info(details("B"), vec![home("A")], vec![]);
    assert_eq!(
        dispatcher.history(),
        NavigationEventHistory::new(details("B"), vec![home("A")], vec![])
    );
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, historyState_withCurrentAndForwardInfo_updatesStack
#[test]
fn the_history_has_forward_info() {
    let dispatcher = Dispatcher::new();
    let forward = vec![details("B"), details("C")];
    dispatcher.add_handler(&TestHandler::with_info(home("A"), vec![], forward.clone()));
    assert_eq!(dispatcher.history(), NavigationEventHistory::new(home("A"), vec![], forward));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventDispatcherTest.kt, historyState_withMergedBackInfo_combinesStacksInPriorityOrder
#[test]
fn back_info_combines_in_priority_order() {
    let dispatcher = Dispatcher::new();
    let (home_info, settings) = (home("home"), details("settings"));
    let default = TestHandler::with_info(settings.clone(), vec![home_info.clone()], vec![]);
    dispatcher.add_handler_with_priority(&default, DEFAULT);
    let in_settings =
        NavigationEventHistory::new(settings.clone(), vec![home_info.clone()], vec![]);
    assert_eq!(dispatcher.history(), in_settings);
    let profile = details("profile");
    let overlay = TestHandler::with_info(profile.clone(), vec![settings.clone()], vec![]);
    dispatcher.add_handler_with_priority(&overlay, OVERLAY);
    assert_eq!(
        dispatcher.history(),
        NavigationEventHistory::new(profile, vec![settings, home_info], vec![])
    );
    overlay.remove();
    assert_eq!(dispatcher.history(), in_settings);
}

/// Dialogs here are layers of overlays (D845): the deepest goes first, whatever the order.
#[test]
fn a_deeper_layer_goes_first() {
    let dispatcher = Dispatcher::new();
    let (deep, shallow, later, normal) =
        (TestHandler::new(), TestHandler::new(), TestHandler::new(), TestHandler::new());
    dispatcher.add_handler_in_layer(&normal, 0);
    dispatcher.add_handler_in_layer(&shallow, 1);
    dispatcher.add_handler_in_layer(&deep, 2);
    dispatcher.add_handler_in_layer(&later, 1);
    let input = added_to(&dispatcher);
    input.back_completed();
    assert_eq!(deep.on_back_completed_invocations(), 1);
    deep.remove();
    input.back_completed();
    assert_eq!(later.on_back_completed_invocations(), 1);
    assert_eq!(shallow.on_back_completed_invocations(), 0);
}
