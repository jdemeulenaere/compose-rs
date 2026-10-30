//! `combined_clickable` against Compose's `CombinedClickableTest`: long and double clicks by
//! touch and keys, their timing, haptics, what a long click consumes, and its interactions.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    ClickableOptions, FocusRequester, HapticFeedback, HapticFeedbackType, Indication, Key,
    KeyEvent, KeyEventType, LOCAL_HAPTIC_FEEDBACK, MutableInteractionSource, Offset,
    PointerEventPass, PointerEventType, SemanticsActions, UnitExt, ViewConfiguration,
};

use crate::interaction_helpers::*;

const TAG: &str = "myClickable";
/// As `longPressTimeoutMillis` plus Compose's `longClick()` margin.
const LONG_CLICK: Duration = Duration::from_millis(500);
/// Compose's test `doubleClick()` delay between the clicks: between the minimum and the timeout.
const DOUBLE_TAP_DELAY: Duration = Duration::from_millis(170);
/// Past the double-tap timeout.
const PAST_DOUBLE_TAP_TIMEOUT: Duration = Duration::from_millis(400);

type Counter = Rc<Cell<u32>>;

fn counter() -> (Counter, Callback<dyn Fn()>) {
    let count = Counter::default();
    let counting = count.clone();
    (count, callback(move || counting.set(counting.get() + 1)))
}

/// A 100 x 100 combined clickable with these callbacks, on a manual clock.
fn combined(
    on_long_click: Option<Callback<dyn Fn()>>,
    on_double_click: Option<Callback<dyn Fn()>>,
    on_click: Callback<dyn Fn()>,
) -> ComposeTestRule {
    ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier = Modifier
                .size(100.dp())
                .combined_clickable(
                    on_long_click.clone(),
                    on_double_click.clone(),
                    on_click.clone()
                )
                .test_tag(TAG)
        );
    })
}

fn long_click(ui: &mut ComposeTestRule, at: Offset) {
    touch_down(ui, at);
    ui.advance_time_by(LONG_CLICK);
    touch_up(ui, at);
}

fn double_click(ui: &mut ComposeTestRule, at: Offset) {
    touch_click(ui, at);
    ui.advance_time_by(DOUBLE_TAP_DELAY);
    touch_click(ui, at);
}

// ---- Semantics ---------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClickSemantics
#[test]
fn a_long_click_is_a_semantics_action() {
    let (long_clicks, on_long_click) = counter();
    let (_, on_click) = counter();
    let mut ui = combined(Some(on_long_click), None, on_click);
    let config = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config;
    assert!(config.is_long_clickable() && !config.is_disabled());
    ui.on_node_with_tag(TAG).perform_semantics_action(&SemanticsActions::ON_LONG_CLICK);
    assert_eq!(long_clicks.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, changingLongClickSemantics
#[test]
fn the_long_click_action_follows_the_callback() {
    let long_clicks = Counter::default();
    let on_long_click: OwnedState<Option<u8>> = mutable_state_of(None);
    let handle = on_long_click.handle();
    let counting = long_clicks.clone();
    let noop = callback(|| {});
    let counted = callback(move || counting.set(counting.get() + 1));
    let mut ui = compose_test(move || {
        let long = match handle.get() {
            None => None,
            Some(0) => Some(noop.clone()),
            Some(_) => Some(counted.clone()),
        };
        Box!(modifier = Modifier.size(10.dp()).combined_clickable(long, None, || {}).test_tag(TAG));
    });
    let long_clickable = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config.is_long_clickable()
    };
    assert!(!long_clickable(&mut ui));
    on_long_click.set(Some(0));
    ui.wait_for_idle();
    assert!(long_clickable(&mut ui));
    ui.on_node_with_tag(TAG).perform_semantics_action(&SemanticsActions::ON_LONG_CLICK);
    assert_eq!(long_clicks.get(), 0);
    on_long_click.set(Some(1));
    ui.wait_for_idle();
    ui.on_node_with_tag(TAG).perform_semantics_action(&SemanticsActions::ON_LONG_CLICK);
    assert_eq!(long_clicks.get(), 1);
    on_long_click.set(None);
    ui.wait_for_idle();
    assert!(!long_clickable(&mut ui));
}

/// `on_long_click_label` is the long-click action's label.
#[test]
fn the_long_click_label_is_in_the_semantics() {
    let mut ui = compose_test(|| {
        let options = ClickableOptions {
            on_long_click_label: Some("Edit".into()),
            ..ClickableOptions::DEFAULT
        };
        Box!(
            modifier = Modifier
                .size(10.dp())
                .combined_clickable_with(options, Some(callback(|| {})), None, || {})
                .test_tag(TAG)
        );
    });
    let config = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config;
    assert_eq!(config.long_click_label().as_deref(), Some("Edit"));
}

// ---- Clicks and long clicks --------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick
#[test]
fn holding_long_clicks() {
    let (long_clicks, on_long_click) = counter();
    let mut ui = combined(Some(on_long_click), None, callback(|| {}));
    let at = center(&mut ui, TAG);
    long_click(&mut ui, at);
    assert_eq!(long_clicks.get(), 1);
    long_click(&mut ui, at);
    assert_eq!(long_clicks.get(), 2);
}

/// A long click vibrates with `LongPress`; a shorter press doesn't.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_hapticFeedbackEnabled
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_hapticFeedbackDisabled
#[test]
fn a_long_click_vibrates_unless_haptics_are_off() {
    struct Recording(Rc<RefCell<Vec<HapticFeedbackType>>>);
    impl HapticFeedback for Recording {
        fn perform_haptic_feedback(&self, kind: HapticFeedbackType) {
            self.0.borrow_mut().push(kind);
        }
    }
    for enabled in [true, false] {
        let performed = Rc::new(RefCell::new(Vec::new()));
        let haptics: Rc<dyn HapticFeedback> = Rc::new(Recording(performed.clone()));
        let (long_clicks, on_long_click) = counter();
        let mut ui = ComposeTestRule::with_manual_clock(move || {
            let on_long_click = on_long_click.clone();
            CompositionLocalProvider(LOCAL_HAPTIC_FEEDBACK.provides(haptics.clone()), move || {
                let options = ClickableOptions {
                    haptic_feedback_enabled: enabled,
                    ..ClickableOptions::DEFAULT
                };
                Box!(
                    modifier = Modifier
                        .size(100.dp())
                        .combined_clickable_with(options, Some(on_long_click.clone()), None, || {})
                        .test_tag(TAG)
                );
            });
        });
        let at = center(&mut ui, TAG);
        touch_down(&mut ui, at);
        ui.advance_time_by(Duration::from_millis(100));
        touch_up(&mut ui, at);
        assert_eq!(long_clicks.get(), 0);
        assert!(performed.borrow().is_empty());
        touch_down(&mut ui, at);
        ui.advance_time_by(Duration::from_millis(1000));
        assert_eq!(long_clicks.get(), 1);
        let expected = if enabled { vec![HapticFeedbackType::LongPress] } else { vec![] };
        assert_eq!(*performed.borrow(), expected, "haptics enabled: {enabled}");
    }
}

fn key(ui: &mut ComposeTestRule, key: Key, kind: KeyEventType) {
    ui.host().send_key(KeyEvent { key, kind, modifiers: Default::default() });
    ui.wait_for_idle();
}

/// A focused combined clickable on a manual clock, with long and short click counters.
fn focused_combined() -> (ComposeTestRule, Counter, Counter) {
    let (clicks, on_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let requester = FocusRequester::new();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier = Modifier.size(10.dp()).focus_requester(requester).combined_clickable(
                Some(on_long_click.clone()),
                None,
                on_click.clone()
            )
        );
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    (ui, clicks, long_clicks)
}

/// Holding Enter long-clicks, then holding the D-pad's center long-clicks again.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClickWithEnterKeyThenDPadCenter_triggersListenerTwice
#[test]
fn holding_enter_then_the_dpad_center_long_clicks_twice() {
    let (mut ui, clicks, long_clicks) = focused_combined();
    for pressed in [Key::Enter, Key::DirectionCenter] {
        key(&mut ui, pressed, KeyEventType::KeyDown);
        ui.advance_time_by(LONG_CLICK);
        key(&mut ui, pressed, KeyEventType::KeyUp);
    }
    assert_eq!((long_clicks.get(), clicks.get()), (2, 0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClickWithEnterKeyConcurrentlyWithDPadCenter_triggersListenerForEach
#[test]
fn two_keys_held_together_each_long_click() {
    let (mut ui, clicks, long_clicks) = focused_combined();
    let half = LONG_CLICK / 2;
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    ui.advance_time_by(half);
    key(&mut ui, Key::DirectionCenter, KeyEventType::KeyDown);
    ui.advance_time_by(half);
    key(&mut ui, Key::Enter, KeyEventType::KeyUp);
    ui.advance_time_by(half);
    key(&mut ui, Key::DirectionCenter, KeyEventType::KeyUp);
    assert_eq!((long_clicks.get(), clicks.get()), (2, 0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClickWithEnterKeyConcurrentlyWithShortClickDPadCenter_triggersListenerForEach
#[test]
fn a_key_held_long_and_another_short_click_each() {
    let (mut ui, clicks, long_clicks) = focused_combined();
    let half = LONG_CLICK / 2;
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    ui.advance_time_by(half);
    key(&mut ui, Key::DirectionCenter, KeyEventType::KeyDown);
    ui.advance_time_by(half);
    key(&mut ui, Key::Enter, KeyEventType::KeyUp);
    key(&mut ui, Key::DirectionCenter, KeyEventType::KeyUp);
    assert_eq!((long_clicks.get(), clicks.get()), (1, 1));
}

/// What a parent recorded of the events: their kind and whether they were consumed.
type Events = Rc<RefCell<Vec<(PointerEventType, bool)>>>;

fn recording(events: &Events) -> Modifier {
    let events = events.clone();
    Modifier.pointer_input((), move |scope| {
        let events = events.clone();
        async move {
            loop {
                let change = scope.await_pointer_event(PointerEventPass::Main).await;
                if !matches!(change.kind, PointerEventType::Enter | PointerEventType::Exit) {
                    events.borrow_mut().push((change.kind, change.is_consumed()));
                }
            }
        }
    })
}

/// After a long click, the moves and the release are consumed; before it, a move isn't.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_consumesEventsAfterLongClick
#[test]
fn after_a_long_click_the_moves_and_the_release_are_consumed() {
    let events = Events::default();
    let (long_clicks, on_long_click) = counter();
    let seen = events.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier = recording(&seen)
                .size(100.dp())
                .combined_clickable(Some(on_long_click.clone()), None, || {})
                .test_tag(TAG)
        );
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    touch_move(&mut ui, at + Offset::new(1.0, 1.0));
    assert_eq!(long_clicks.get(), 0);
    assert_eq!(
        *events.borrow(),
        [(PointerEventType::Press, true), (PointerEventType::Move, false)]
    );
    events.borrow_mut().clear();
    ui.advance_time_by(LONG_CLICK);
    touch_move(&mut ui, at + Offset::new(2.0, 2.0));
    touch_up(&mut ui, at + Offset::new(2.0, 2.0));
    assert_eq!(long_clicks.get(), 1);
    assert_eq!(
        *events.borrow(),
        [(PointerEventType::Move, true), (PointerEventType::Release, true)]
    );
}

/// Even out of its bounds, after a long click.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_consumesEventsAfterLongClick_outOfBounds
#[test]
fn after_a_long_click_moves_out_of_bounds_are_consumed_too() {
    let events = Events::default();
    let (long_clicks, on_long_click) = counter();
    let seen = events.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier = recording(&seen)
                .size(100.dp())
                .combined_clickable(Some(on_long_click.clone()), None, || {})
                .test_tag(TAG)
        );
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(LONG_CLICK);
    assert_eq!(long_clicks.get(), 1);
    events.borrow_mut().clear();
    let outside = at + Offset::new(200.0, 200.0);
    touch_move(&mut ui, outside);
    touch_up(&mut ui, outside);
    assert_eq!(
        *events.borrow(),
        [(PointerEventType::Move, true), (PointerEventType::Release, true)]
    );
}

/// A child consuming the release after a long click: the next click still clicks.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_childConsumesUp_handlesNextClick
#[test]
fn a_child_taking_the_release_after_a_long_click_leaves_the_next_click() {
    let (clicks, on_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let consume = Rc::new(Cell::new(false));
    let consuming = consume.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let consuming = consuming.clone();
        Box!(
            modifier = Modifier
                .size(100.dp())
                .combined_clickable(Some(on_long_click.clone()), None, on_click.clone())
                .pointer_input((), move |scope| {
                    let consuming = consuming.clone();
                    async move {
                        loop {
                            let change = scope.await_pointer_event(PointerEventPass::Main).await;
                            if consuming.get() {
                                change.consume();
                            }
                        }
                    }
                })
                .test_tag(TAG)
        );
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(LONG_CLICK);
    assert_eq!((long_clicks.get(), clicks.get()), (1, 0));
    consume.set(true);
    touch_up(&mut ui, at);
    consume.set(false);
    touch_click(&mut ui, at);
    assert_eq!((long_clicks.get(), clicks.get()), (1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, click_withLongClick
#[test]
fn a_tap_clicks_and_holding_long_clicks() {
    let (clicks, on_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let mut ui = combined(Some(on_long_click), None, on_click);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!((clicks.get(), long_clicks.get()), (1, 0));
    long_click(&mut ui, at);
    assert_eq!((clicks.get(), long_clicks.get()), (1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, click_afterLongClick
#[test]
fn a_tap_after_a_long_click_clicks() {
    let (clicks, on_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let mut ui = combined(Some(on_long_click), None, on_click);
    let at = center(&mut ui, TAG);
    long_click(&mut ui, at);
    assert_eq!((clicks.get(), long_clicks.get()), (0, 1));
    touch_click(&mut ui, at);
    assert_eq!((clicks.get(), long_clicks.get()), (1, 1));
}

// ---- Double clicks -----------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, click_withDoubleClick
#[test]
fn a_tap_waits_out_the_double_click_timeout_and_two_double_click() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let mut ui = combined(None, Some(on_double_click), on_click);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!(clicks.get(), 0, "waiting for a second tap");
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    assert_eq!((clicks.get(), doubles.get()), (1, 0));
    double_click(&mut ui, at);
    assert_eq!((clicks.get(), doubles.get()), (1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, click_withDoubleClick_andLongClick
#[test]
fn clicks_double_clicks_and_long_clicks_together() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let mut ui = combined(Some(on_long_click), Some(on_double_click), on_click);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    assert_eq!((clicks.get(), doubles.get(), long_clicks.get()), (1, 0, 0));
    double_click(&mut ui, at);
    assert_eq!((clicks.get(), doubles.get(), long_clicks.get()), (1, 1, 0));
    long_click(&mut ui, at);
    assert_eq!((clicks.get(), doubles.get(), long_clicks.get()), (1, 1, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, doubleClick_withinTimeout_aboveMinimumDuration
#[test]
fn a_second_tap_within_the_timeout_double_clicks_at_once() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let mut ui = combined(None, Some(on_double_click), on_click);
    let at = center(&mut ui, TAG);
    double_click(&mut ui, at);
    assert_eq!((clicks.get(), doubles.get()), (0, 1));
}

/// A second tap sooner than the minimum is ignored: the first is a click, once the timeout ends.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, doubleClick_withinTimeout_belowMinimumDuration
#[test]
fn a_second_tap_sooner_than_the_minimum_is_ignored() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let mut ui = combined(None, Some(on_double_click), on_click);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(20));
    touch_click(&mut ui, at);
    assert_eq!((clicks.get(), doubles.get()), (0, 0));
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    assert_eq!((clicks.get(), doubles.get()), (1, 0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, doubleClick_outsideTimeout
#[test]
fn a_second_tap_after_the_timeout_is_a_click_of_its_own() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let mut ui = combined(None, Some(on_double_click), on_click);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!(clicks.get(), 0);
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    assert_eq!(clicks.get(), 1);
    touch_click(&mut ui, at);
    assert_eq!(clicks.get(), 1);
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    assert_eq!((clicks.get(), doubles.get()), (2, 0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, doubleClick_secondClickIsALongClick
#[test]
fn a_held_second_tap_long_clicks_and_cancels_the_double_click() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let mut ui = combined(Some(on_long_click), Some(on_double_click), on_click);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    ui.advance_time_by(DOUBLE_TAP_DELAY);
    touch_down(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(1000));
    assert_eq!((clicks.get(), doubles.get(), long_clicks.get()), (0, 0, 1));
}

/// A cancelled second tap cancels the click the first was waiting for.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, doubleClick_pointerInputCanceled_cancelsPendingTap
#[test]
fn a_cancelled_gesture_cancels_the_pending_click() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let mut ui = combined(None, Some(on_double_click), on_click);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    touch_down(&mut ui, at);
    touch_cancel(&mut ui);
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    assert_eq!((clicks.get(), doubles.get()), (0, 0));
}

// ---- Interactions ------------------------------------------------------------------------------

fn options(source: &MutableInteractionSource, enabled: bool) -> ClickableOptions {
    ClickableOptions {
        enabled,
        interaction_source: Some(source.clone()),
        indication: Indication::NONE,
        ..ClickableOptions::DEFAULT
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, interactionSource_noScrollableContainer
#[test]
fn a_press_is_emitted_and_released() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier = Modifier
                .size(100.dp())
                .combined_clickable_with(options(&given, true), None, None, || {})
                .test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press"]);
    touch_up(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert!(ends(&recorded, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, interactionSource_scrollableContainer
#[test]
fn a_press_inside_a_scrollable_waits_the_tap_delay() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let given = given.clone();
        let state = remember_scroll_state(0);
        Box!(modifier = Modifier.vertical_scroll(state), {
            Box!(
                modifier = Modifier
                    .size(100.dp())
                    .combined_clickable_with(options(&given, true), None, None, || {})
                    .test_tag(TAG)
            );
        });
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(compose_ui::TAP_INDICATION_DELAY / 2);
    assert!(recorded.borrow().is_empty());
    ui.advance_time_by(compose_ui::TAP_INDICATION_DELAY / 2);
    assert_eq!(kinds(&recorded), ["Press"]);
    touch_up(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
}

/// A long click recomposing with a new callback keeps the press until the release.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_interactionSource_continuesTrackingPressAfterLambdasChange
#[test]
fn a_new_long_click_callback_keeps_the_press() {
    let source = MutableInteractionSource::new();
    let generation = mutable_state_of(0);
    let (given, handle) = (source.clone(), generation.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        // A new callback each generation; the first moves to the next.
        let current = handle.get();
        let long = callback(move || handle.set(current + 1));
        Box!(
            modifier = Modifier
                .size(100.dp())
                .combined_clickable_with(options(&given, true), Some(long), None, || {})
                .test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(1000));
    ui.advance_time_by_frame();
    assert_eq!(generation.get(), 1);
    assert_eq!(kinds(&recorded), ["Press"]);
    touch_up(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert!(ends(&recorded, 0, 1));
}

/// The long click going away mid-press cancels the press.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_interactionSource_cancelsIfLongClickBecomesNull
#[test]
fn the_long_click_going_away_cancels_the_press() {
    let source = MutableInteractionSource::new();
    let has_long = mutable_state_of(true);
    let (given, handle) = (source.clone(), has_long.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let long = handle.get().then(|| callback(move || handle.set(false)));
        Box!(
            modifier = Modifier
                .size(100.dp())
                .combined_clickable_with(options(&given, true), long, None, || {})
                .test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(kinds(&recorded), ["Press"]);
    ui.advance_time_by(Duration::from_millis(1000));
    ui.advance_time_by_frame();
    assert!(!has_long.get());
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
    assert!(ends(&recorded, 0, 1));
}

/// Disabled mid-press, the press is cancelled.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, longClick_interactionSource_cancelsIfBecomesDisabled
#[test]
fn disabled_mid_press_the_press_is_cancelled() {
    let source = MutableInteractionSource::new();
    let enabled = mutable_state_of(true);
    let (given, handle) = (source.clone(), enabled.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let long = callback(move || handle.set(false));
        Box!(
            modifier = Modifier
                .size(100.dp())
                .combined_clickable_with(options(&given, handle.get()), Some(long), None, || {})
                .test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(1000));
    ui.advance_time_by_frame();
    assert!(!enabled.get());
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
}

/// Disabled, nothing clicks, and the presses are still taken from the parent.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, click_withDoubleClick_andLongClick_disabled
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, clicks_consumedWhenDisabled
#[test]
fn disabled_nothing_clicks_and_the_parent_gets_nothing() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let (outer, on_outer) = counter();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let options = ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT };
        let (on_long_click, on_double_click, on_click) =
            (on_long_click.clone(), on_double_click.clone(), on_click.clone());
        Box!(modifier = Modifier.combined_clickable(Some(on_outer.clone()), None, || {}), {
            Box!(
                modifier = Modifier
                    .size(100.dp())
                    .combined_clickable_with(
                        options.clone(),
                        Some(on_long_click.clone()),
                        Some(on_double_click.clone()),
                        on_click.clone()
                    )
                    .test_tag(TAG)
            );
        });
    });
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    double_click(&mut ui, at);
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    long_click(&mut ui, at);
    assert_eq!((clicks.get(), doubles.get(), long_clicks.get(), outer.get()), (0, 0, 0, 0));
}

/// Disabled mid-gesture: the pending click doesn't happen.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/CombinedClickableTest.kt, click_withDoubleClick_andLongClick_disabledMidGesture
#[test]
fn disabled_mid_gesture_nothing_clicks() {
    let (clicks, on_click) = counter();
    let (doubles, on_double_click) = counter();
    let (long_clicks, on_long_click) = counter();
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let options = ClickableOptions { enabled: handle.get(), ..ClickableOptions::DEFAULT };
        Box!(
            modifier = Modifier
                .size(100.dp())
                .combined_clickable_with(
                    options,
                    Some(on_long_click.clone()),
                    Some(on_double_click.clone()),
                    on_click.clone()
                )
                .test_tag(TAG)
        );
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    enabled.set(false);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(1000));
    touch_up(&mut ui, at);
    ui.advance_time_by(PAST_DOUBLE_TAP_TIMEOUT);
    assert_eq!((clicks.get(), doubles.get(), long_clicks.get()), (0, 0, 0));
}

// ---- LOCAL_VIEW_CONFIGURATION ------------------------------------------------------------------

/// Android's view configuration with other timeouts, for the host (what its nodes read as
/// `currentValueOf(LocalViewConfiguration)`).
fn timeouts(long_press: u64, double_tap: u64, double_tap_min: u64) -> ViewConfiguration {
    ViewConfiguration {
        long_press_timeout_millis: long_press,
        double_tap_timeout_millis: double_tap,
        double_tap_min_time_millis: double_tap_min,
        ..ViewConfiguration::default()
    }
}

/// A long click waits the view configuration's `long_press_timeout_millis`, by touch.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, CombinedClickableNode's longPressTimeoutMillis
#[test]
fn a_long_click_waits_the_view_configuration_s_timeout() {
    let (long_clicks, on_long_click) = counter();
    let mut ui = combined(Some(on_long_click), None, callback(|| {}));
    ui.host().set_view_configuration(Some(timeouts(1000, 300, 40)));
    // Nodes take it as the host's composition provides it, as `LocalViewConfiguration`.
    ui.advance_time_by_frame();
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(999));
    assert_eq!(long_clicks.get(), 0, "Android's 400 ms isn't the timeout");
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(1));
    assert_eq!(long_clicks.get(), 1);
    touch_up(&mut ui, at);
}

/// A key held long clicks after the view configuration's timeout.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onClickKeyDownEvent
#[test]
fn a_key_held_waits_the_view_configuration_s_timeout() {
    let (mut ui, clicks, long_clicks) = focused_combined();
    ui.host().set_view_configuration(Some(timeouts(1000, 300, 40)));
    // Nodes take it as the host's composition provides it, as `LocalViewConfiguration`.
    ui.advance_time_by_frame();
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(999));
    assert_eq!(long_clicks.get(), 0);
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(1));
    assert_eq!(long_clicks.get(), 1);
    key(&mut ui, Key::Enter, KeyEventType::KeyUp);
    assert_eq!(clicks.get(), 0);
}

/// A tap waits the view configuration's `double_tap_timeout_millis` for a second one before it
/// clicks, by touch.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, CombinedClickableNode's doubleTapTimeoutMillis
#[test]
fn a_tap_waits_the_view_configuration_s_double_tap_timeout() {
    let (clicks, on_click) = counter();
    let (double_clicks, on_double_click) = counter();
    let mut ui = combined(None, Some(on_double_click), on_click);
    ui.host().set_view_configuration(Some(timeouts(400, 1000, 40)));
    // Nodes take it as the host's composition provides it, as `LocalViewConfiguration`.
    ui.advance_time_by_frame();
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    ui.advance_time_by_ignoring_frame_duration(Duration::from_millis(999));
    assert_eq!(clicks.get(), 0, "Android's 300 ms isn't the timeout");
    touch_click(&mut ui, at);
    assert_eq!((clicks.get(), double_clicks.get()), (0, 1), "still within the timeout");
    touch_click(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(1000));
    assert_eq!((clicks.get(), double_clicks.get()), (1, 1));
}

/// A second tap sooner than the view configuration's `double_tap_min_time_millis` is ignored.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, CombinedClickableNode's doubleTapMinTimeMillis
#[test]
fn a_second_tap_sooner_than_the_view_configuration_s_minimum_is_ignored() {
    let (clicks, on_click) = counter();
    let (double_clicks, on_double_click) = counter();
    let mut ui = combined(None, Some(on_double_click), on_click);
    ui.host().set_view_configuration(Some(timeouts(400, 300, 200)));
    // Nodes take it as the host's composition provides it, as `LocalViewConfiguration`.
    ui.advance_time_by_frame();
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(100));
    touch_click(&mut ui, at);
    assert_eq!(double_clicks.get(), 0, "Android's 40 ms isn't the minimum");
    ui.advance_time_by(Duration::from_millis(300));
    assert_eq!((clicks.get(), double_clicks.get()), (1, 0));
}

/// Keys wait the view configuration's double-tap times too: a second click sooner than the
/// minimum makes the first a click of its own; past it, within the timeout, it's a double click;
/// past the timeout, a click.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onClickKeyUpEvent
#[test]
fn keys_wait_the_view_configuration_s_double_tap_times() {
    let (clicks, on_click) = counter();
    let (double_clicks, on_double_click) = counter();
    let requester = FocusRequester::new();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier = Modifier.size(10.dp()).focus_requester(requester).combined_clickable(
                None,
                Some(on_double_click.clone()),
                on_click.clone()
            )
        );
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    ui.host().set_view_configuration(Some(timeouts(400, 1000, 200)));
    // Nodes take it as the host's composition provides it, as `LocalViewConfiguration`.
    ui.advance_time_by_frame();
    let press = |ui: &mut ComposeTestRule| {
        key(ui, Key::Enter, KeyEventType::KeyDown);
        key(ui, Key::Enter, KeyEventType::KeyUp);
    };
    press(&mut ui);
    ui.advance_time_by(Duration::from_millis(100));
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    assert_eq!(clicks.get(), 1, "sooner than the minimum: the first is a click of its own");
    key(&mut ui, Key::Enter, KeyEventType::KeyUp);
    ui.advance_time_by(Duration::from_millis(500));
    press(&mut ui);
    assert_eq!((clicks.get(), double_clicks.get()), (1, 1), "within the timeout");
    press(&mut ui);
    ui.advance_time_by(Duration::from_millis(1000));
    assert_eq!((clicks.get(), double_clicks.get()), (2, 1), "past the timeout, a click");
}
