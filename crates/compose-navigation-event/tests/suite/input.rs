//! Inputs and the history against AndroidX's `NavigationEventInputTest`,
//! `DirectNavigationEventInputTest` and `NavigationEventHistoryTest`.

use std::cell::RefCell;
use std::rc::Rc;

use compose_navigation_event::*;

use crate::support::*;

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnBackStarted_withoutDispatcher_shouldReturnSilently
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnBackProgressed_withoutDispatcher_shouldReturnSilently
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnBackCancelled_withoutDispatcher_shouldReturnSilently
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnBackCompleted_withoutDispatcher_shouldReturnSilently
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnForwardStarted_withoutDispatcher_shouldReturnSilently
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnForwardProgressed_withoutDispatcher_shouldReturnSilently
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnForwardCancelled_withoutDispatcher_shouldReturnSilently
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, dispatchOnForwardCompleted_withoutDispatcher_shouldReturnSilently
#[test]
fn an_input_with_no_dispatcher_does_nothing() {
    let input = TestInput::new();
    input.back_started();
    input.back_progressed();
    input.back_cancelled();
    input.back_completed();
    input.forward_started();
    input.forward_progressed();
    input.forward_cancelled();
    input.forward_completed();
}

struct EnabledCalls(Rc<RefCell<Vec<(&'static str, bool)>>>);

impl NavigationEventInputCallbacks for EnabledCalls {
    fn on_has_enabled_handlers_changed(&self, value: bool) {
        self.0.borrow_mut().push(("any", value));
    }

    fn on_has_enabled_back_handlers_changed(&self, value: bool) {
        self.0.borrow_mut().push(("back", value));
    }

    fn on_has_enabled_forward_handlers_changed(&self, value: bool) {
        self.0.borrow_mut().push(("forward", value));
    }
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, hasEnabledHandlers_callbacksTriggeredCorrectly
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventInputTest.kt, hasEnabledHandlers_propertiesUpdatedCorrectly
#[test]
fn an_input_hears_of_enabled_handlers_by_direction() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let input = NavigationEventInput::new(EnabledCalls(calls.clone()));
    let dispatcher = NavigationEventDispatcher::new();
    dispatcher.add_input(&input);
    assert_eq!(*calls.borrow(), [("any", false), ("back", false), ("forward", false)]);
    assert!(!input.has_enabled_handlers());
    calls.borrow_mut().clear();
    let handler = TestNavigationEventHandler::enabled(true, false);
    dispatcher.add_handler(&handler);
    assert_eq!(*calls.borrow(), [("any", true), ("back", true)]);
    assert!(input.has_enabled_handlers() && input.has_enabled_back_handlers());
    assert!(!input.has_enabled_forward_handlers());
    calls.borrow_mut().clear();
    handler.set_forward_enabled(true);
    assert_eq!(*calls.borrow(), [("forward", true)]);
    assert!(input.has_enabled_forward_handlers());
    calls.borrow_mut().clear();
    handler.set_back_enabled(false);
    assert_eq!(*calls.borrow(), [("back", false)]);
    calls.borrow_mut().clear();
    handler.remove();
    assert_eq!(*calls.borrow(), [("any", false), ("forward", false)]);
    assert!(!input.has_enabled_handlers());
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, backStarted_afterConnected_shouldWork
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, backProgressed_afterConnected_shouldWork
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, backCancelled_afterConnected_shouldWork
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, backCompleted_afterConnected_shouldWork
#[test]
fn a_direct_input_sends_back_gestures() {
    let dispatcher = NavigationEventDispatcher::new();
    let handler = TestNavigationEventHandler::new();
    dispatcher.add_handler(&handler);
    let input = DirectNavigationEventInput::new();
    dispatcher.add_input(&input);
    input.back_started(NavigationEvent::default());
    assert_eq!(handler.on_back_started_invocations(), 1);
    input.back_progressed(NavigationEvent::default());
    assert_eq!(handler.on_back_progressed_invocations(), 1);
    input.back_cancelled();
    assert_eq!(handler.on_back_cancelled_invocations(), 1);
    input.back_completed();
    assert_eq!(handler.on_back_completed_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, forwardStarted_afterConnected_shouldWork
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, forwardProgressed_afterConnected_shouldWork
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, forwardCancelled_afterConnected_shouldWork
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/DirectNavigationEventInputTest.kt, forwardCompleted_afterConnected_shouldWork
#[test]
fn a_direct_input_sends_forward_gestures() {
    let dispatcher = NavigationEventDispatcher::new();
    let handler = TestNavigationEventHandler::new();
    dispatcher.add_handler(&handler);
    let input = DirectNavigationEventInput::new();
    dispatcher.add_input(&input);
    input.forward_started(NavigationEvent::default());
    assert_eq!(handler.on_forward_started_invocations(), 1);
    input.forward_progressed(NavigationEvent::default());
    assert_eq!(handler.on_forward_progressed_invocations(), 1);
    input.forward_cancelled();
    assert_eq!(handler.on_forward_cancelled_invocations(), 1);
    input.forward_completed();
    assert_eq!(handler.on_forward_completed_invocations(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt, emptyConstructor_createsEmptyState
#[test]
fn an_empty_history_is_at_minus_one() {
    let history = NavigationEventHistory::default();
    assert!(history.merged_history().is_empty());
    assert_eq!(history.current_index(), -1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt, partitionConstructor_currentOnly_createsCorrectState
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt, partitionConstructor_currentAndBack_createsCorrectState
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt, partitionConstructor_currentAndForward_createsCorrectState
// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt, partitionConstructor_fullStack_createsCorrectState
#[test]
fn a_history_merges_back_current_and_forward() {
    let (a, b, c) = (home("A"), home("B"), home("C"));
    let only = NavigationEventHistory::new(a.clone(), vec![], vec![]);
    assert_eq!(only.merged_history(), std::slice::from_ref(&a));
    assert_eq!(only.current_index(), 0);
    let with_back = NavigationEventHistory::new(b.clone(), vec![a.clone()], vec![]);
    assert_eq!(with_back.merged_history(), [a.clone(), b.clone()]);
    assert_eq!(with_back.current_index(), 1);
    let with_forward = NavigationEventHistory::new(a.clone(), vec![], vec![b.clone()]);
    assert_eq!(with_forward.merged_history(), [a.clone(), b.clone()]);
    assert_eq!(with_forward.current_index(), 0);
    let full = NavigationEventHistory::new(b.clone(), vec![a.clone()], vec![c.clone()]);
    assert_eq!(full.merged_history(), [a, b, c]);
    assert_eq!(full.current_index(), 1);
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt, equals_and_hashCode_contract
#[test]
fn histories_are_equal_by_their_places_and_index() {
    let (a, b) = (home("A"), home("B"));
    let first = NavigationEventHistory::new(b.clone(), vec![a.clone()], vec![]);
    assert_eq!(first, NavigationEventHistory::new(b.clone(), vec![a.clone()], vec![]));
    assert_ne!(first, NavigationEventHistory::new(a.clone(), vec![], vec![b]));
    assert_ne!(first, NavigationEventHistory::new(a, vec![], vec![]));
}

// Compose: navigationevent/navigationevent/src/commonTest/kotlin/androidx/navigationevent/NavigationEventHistoryTest.kt, toString_containsAllProperties
#[test]
fn a_history_prints_its_places_and_index() {
    let history = NavigationEventHistory::new(home("B"), vec![home("A")], vec![home("C")]);
    let text = format!("{history:?}");
    assert!(text.contains("currentIndex=1"), "{text}");
    assert!(text.contains(r#"mergedHistory=[Home("A"), Home("B"), Home("C")]"#), "{text}");
}

/// What takes the place of a class to extend: a value, compared and read back, with a title and a
/// URL.
#[test]
fn an_info_is_any_value() {
    assert_eq!(home("A"), home("A"));
    assert_ne!(home("A"), home("B"));
    assert_ne!(home("A"), details("A"));
    assert_eq!(home("A").downcast_ref::<Home>(), Some(&Home("A")));
    assert_eq!(home("A").downcast_ref::<Details>(), None);
    assert_eq!(NavigationEventInfo::none(), NavigationEventInfo::none());
    let titled = home("A").with_title("Title").with_url("https://example.com/");
    assert_eq!(titled.title(), Some("Title"));
    assert_eq!(titled.url(), Some("https://example.com/"));
    assert_ne!(titled, home("A"));
}
