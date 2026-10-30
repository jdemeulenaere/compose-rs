//! `ResultEventBus` and `ResultEffect`, against Navigation 3's `ResultEventBusTest`,
//! `ResultEffectTest` and `ResultEventBusAndroidTest`. `ResultEventBusNavEntryDecoratorTest` needs
//! `NavDisplay`, and comes with it.

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::BasicText;
use compose_navigation::*;
use compose_runtime::*;
use compose_test::*;

type Log = Rc<RefCell<Vec<String>>>;

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusTest.kt, testGetResultFlow
#[test]
fn a_result_sent_before_it_s_listened_to_waits() {
    let bus = ResultEventBus::new();
    bus.send_result_to("testKey", "testResult".to_string());
    let log = Log::default();
    let (given, logged) = (bus.clone(), log.clone());
    let mut ui = compose_test(move || {
        let logged = logged.clone();
        ResultEffect!(
            result_key = Some("testKey".to_string()),
            result_event_bus = given.clone(),
            on_result = move |result: String| {
                logged.borrow_mut().push(result);
                async {}
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["testResult"]);
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusTest.kt, testRemoveResult
#[test]
fn a_result_removed_is_heard_by_nobody() {
    let bus = ResultEventBus::new();
    bus.send_result_to("testKey", "test".to_string());
    bus.remove_result_of("testKey");
    let log = Log::default();
    let (given, logged) = (bus.clone(), log.clone());
    let mut ui = compose_test(move || {
        let logged = logged.clone();
        ResultEffect!(
            result_key = Some("testKey".to_string()),
            result_event_bus = given.clone(),
            on_result = move |result: String| {
                logged.borrow_mut().push(result);
                async {}
            }
        );
    });
    ui.wait_for_idle();
    assert!(log.borrow().is_empty());
    // Sent again, it's a new channel's, which the effect comes to.
    bus.send_result_to("testKey", "again".to_string());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["again"]);
}

/// Removing it ends the wait of what's listening, and a type's key goes the same way.
#[test]
fn removing_a_result_ends_its_listeners_wait() {
    let bus = ResultEventBus::new();
    let log = Log::default();
    let (given, logged) = (bus.clone(), log.clone());
    let mut ui = compose_test(move || {
        let logged = logged.clone();
        ResultEffect!(
            result_event_bus = given.clone(),
            on_result = move |result: String| {
                logged.borrow_mut().push(result);
                async {}
            }
        );
    });
    ui.wait_for_idle();
    bus.send_result("first".to_string());
    ui.wait_for_idle();
    bus.remove_result::<String>();
    ui.wait_for_idle();
    bus.send_result("second".to_string());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["first", "second"]);
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/result/ResultEffectTest.kt, testResultEffect_capturesUpdatedOnResult
#[test]
fn an_effect_hears_with_its_latest_closure() {
    let bus = ResultEventBus::new();
    let received = Rc::new(RefCell::new(String::new()));
    let state = mutable_state_of("initial");
    let (given, seen, read) = (bus.clone(), received.clone(), state.handle());
    let mut ui = compose_test(move || {
        let current = read.get();
        let seen = seen.clone();
        ResultEffect!(
            result_key = Some("testKey".to_string()),
            result_event_bus = given.clone(),
            on_result = move |result: String| {
                *seen.borrow_mut() = format!("{result} with state {current}");
                async {}
            }
        );
    });
    ui.wait_for_idle();
    bus.send_result_to("testKey", "event".to_string());
    ui.wait_for_idle();
    assert_eq!(*received.borrow(), "event with state initial");
    state.set("updated");
    ui.wait_for_idle();
    bus.send_result_to("testKey", "event".to_string());
    ui.wait_for_idle();
    assert_eq!(*received.borrow(), "event with state updated");
}

/// Each result is heard, in order, even several at a time, and a closure's wait holds up the next.
#[test]
fn an_effect_hears_each_result_in_order() {
    let bus = ResultEventBus::new();
    let log = Log::default();
    let (given, logged) = (bus.clone(), log.clone());
    let mut ui = compose_test(move || {
        let logged = logged.clone();
        ResultEffect!(
            result_event_bus = given.clone(),
            on_result = move |result: String| {
                let logged = logged.clone();
                async move {
                    logged.borrow_mut().push(format!("{result} start"));
                    delay(std::time::Duration::from_millis(10)).await;
                    logged.borrow_mut().push(format!("{result} end"));
                }
            }
        );
    });
    ui.wait_for_idle();
    bus.send_result("a".to_string());
    bus.send_result("b".to_string());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["a start"]);
    ui.advance_time_by(std::time::Duration::from_secs(1));
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["a start", "a end", "b start", "b end"]);
}

/// Past 64 waiting, what's sent is dropped, as `trySend` on a buffered channel.
#[test]
fn results_past_the_buffer_are_dropped() {
    let bus = ResultEventBus::new();
    for index in 0..100 {
        bus.send_result(index);
    }
    let count = Rc::new(std::cell::Cell::new(0));
    let (given, counted) = (bus.clone(), count.clone());
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        ResultEffect!(
            result_event_bus = given.clone(),
            on_result = move |_: i32| {
                counted.set(counted.get() + 1);
                async {}
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(count.get(), 64);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusAndroidTest.kt, testConflateAsStateDefault
#[test]
fn a_state_of_results_starts_at_its_default() {
    let bus = ResultEventBus::new();
    let mut ui = compose_test(move || {
        let state = bus.conflate_as_state("testResult".to_string());
        BasicText!("{}", state.get());
    });
    ui.wait_for_idle();
    ui.on_node_with_text("testResult").assert_is_displayed();
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/result/ResultEventBusAndroidTest.kt, testConflateAsState
#[test]
fn a_state_of_results_holds_the_latest() {
    let bus = ResultEventBus::new();
    let given = bus.clone();
    let mut ui = compose_test(move || {
        let state = given.conflate_as_state_of("testKey", "no result".to_string());
        BasicText!("{}", state.get());
    });
    bus.send_result_to("testKey", "testResult".to_string());
    ui.wait_for_idle();
    ui.on_node_with_text("testResult").assert_is_displayed();
    bus.send_result_to("testKey", "one".to_string());
    bus.send_result_to("testKey", "two".to_string());
    ui.wait_for_idle();
    ui.on_node_with_text("two").assert_is_displayed();
}

/// What provides a bus, and what reads the local one with none.
#[test]
fn the_local_bus_is_what_s_provided() {
    let log = Log::default();
    let bus = ResultEventBus::new();
    let (given, logged) = (bus.clone(), log.clone());
    let mut ui = compose_test(move || {
        let logged = logged.clone();
        CompositionLocalProvider(LocalResultEventBus::provides(given.clone()), move || {
            let logged = logged.clone();
            ResultEffect!(
                on_result = move |result: String| {
                    logged.borrow_mut().push(result);
                    async {}
                }
            );
        });
    });
    ui.wait_for_idle();
    bus.send_result("result".to_string());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["result"]);
}

#[test]
#[should_panic(expected = "No ResultEventBus has been provided")]
fn the_local_bus_with_none_panics() {
    let mut ui = compose_test(|| {
        let _ = LocalResultEventBus::current();
    });
    ui.wait_for_idle();
}

/// Through a decorator, as the decorator's tests do with `NavDisplay`: an entry's bus is the
/// decorator's.
#[test]
fn a_decorator_provides_its_bus_to_entries() {
    let bus = ResultEventBus::new();
    let log = Log::default();
    let (given, logged) = (bus.clone(), log.clone());
    let mut ui = compose_test(move || {
        let decorator = remember_result_event_bus_nav_entry_decorator!(given.clone());
        let logged = logged.clone();
        let entries = remember_decorated_nav_entries!(
            vec![1],
            vec![decorator],
            entry_provider = move |key: i32| {
                let logged = logged.clone();
                NavEntry::new(key, move |_| {
                    let logged = logged.clone();
                    ResultEffect!(
                        on_result = move |result: String| {
                            logged.borrow_mut().push(result);
                            async {}
                        }
                    );
                })
            }
        );
        entries[0].Content();
    });
    ui.wait_for_idle();
    bus.send_result("result".to_string());
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["result"]);
}
