//! `MutableInteractionSource`: the interactions stream, `emit` and `try_emit`, and the
//! `collect_is_*_as_state` states.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    DragInteraction, DragStart, Focus, FocusInteraction, Interaction, MutableInteractionSource,
    Offset, Press, PressInteraction,
};

use crate::interaction_helpers::{kinds, record};

/// A rule whose content collects one of `source`'s states, and the state it collects.
fn collecting(
    source: &MutableInteractionSource,
    collect: fn(&compose_ui::InteractionSource) -> State<bool>,
) -> (ComposeTestRule, Rc<Cell<Option<State<bool>>>>) {
    let state = Rc::new(Cell::new(None));
    let (given, seen) = (source.clone(), state.clone());
    let ui = compose_test(move || seen.set(Some(collect(&given))));
    (ui, state)
}

fn value(state: &Rc<Cell<Option<State<bool>>>>) -> bool {
    state.get().expect("composed").get()
}

/// Emitting from a coroutine: the collector gets each interaction, in order.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt, emittingInteractionsInOrder
#[test]
fn interactions_reach_a_collector_in_the_order_emitted() {
    struct First;
    struct Second;
    struct Third;
    let mut ui = compose_test(|| {});
    let source = MutableInteractionSource::new();
    let recorded = record(&source);
    let (first, second, third) =
        (Interaction::custom(First), Interaction::custom(Second), Interaction::custom(Third));
    let emitter = source.clone();
    let (a, b) = (first.clone(), second.clone());
    CoroutineScope::new().launch(async move {
        emitter.emit(a).await;
        emitter.emit(b).await;
    });
    ui.wait_for_idle();
    assert_eq!(*recorded.borrow(), vec![first.clone(), second.clone()]);
    let emitter = source.clone();
    let c = third.clone();
    CoroutineScope::new().launch(async move { emitter.emit(c).await });
    ui.wait_for_idle();
    assert_eq!(*recorded.borrow(), vec![first, second, third]);
}

/// `try_emit` succeeds at once, and a collector that hasn't run keeps the latest 16, as
/// Compose's buffer (`extraBufferCapacity = 16`, dropping the oldest).
#[test]
fn try_emit_succeeds_and_a_slow_collector_keeps_the_latest_sixteen() {
    let mut ui = compose_test(|| {});
    let source = MutableInteractionSource::new();
    let recorded = record(&source);
    let presses: Vec<Press> =
        (0..20).map(|index| Press::new(Offset::new(index as f32, 0.0))).collect();
    for press in &presses {
        assert!(source.try_emit(PressInteraction::Press(*press)));
    }
    ui.wait_for_idle();
    let expected: Vec<Interaction> =
        presses[4..].iter().map(|press| PressInteraction::Press(*press).into()).collect();
    assert_eq!(*recorded.borrow(), expected);
}

/// A collector sees only what's emitted after it started collecting.
#[test]
fn a_collector_starts_with_what_comes_after_it() {
    let mut ui = compose_test(|| {});
    let source = MutableInteractionSource::new();
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    let recorded = record(&source);
    ui.wait_for_idle();
    assert!(recorded.borrow().is_empty());
    source.try_emit(FocusInteraction::Focus(Focus::new()));
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Focus"]);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt, isDragged
#[test]
fn collect_is_dragged_as_state_follows_a_drag() {
    let source = MutableInteractionSource::new();
    let (mut ui, dragged) = collecting(&source, |source| source.collect_is_dragged_as_state());
    assert!(!value(&dragged));
    let start = DragStart::new();
    source.try_emit(DragInteraction::Start(start));
    ui.wait_for_idle();
    assert!(value(&dragged));
    source.try_emit(DragInteraction::Stop(start));
    ui.wait_for_idle();
    assert!(!value(&dragged));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt, isDragged_multipleDrags
#[test]
fn collect_is_dragged_as_state_stays_true_until_every_drag_ends() {
    let source = MutableInteractionSource::new();
    let (mut ui, dragged) = collecting(&source, |source| source.collect_is_dragged_as_state());
    let (first, second) = (DragStart::new(), DragStart::new());
    source.try_emit(DragInteraction::Start(first));
    ui.wait_for_idle();
    assert!(value(&dragged));
    source.try_emit(DragInteraction::Start(second));
    ui.wait_for_idle();
    assert!(value(&dragged));
    source.try_emit(DragInteraction::Stop(first));
    ui.wait_for_idle();
    assert!(value(&dragged));
    source.try_emit(DragInteraction::Cancel(second));
    ui.wait_for_idle();
    assert!(!value(&dragged));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt, isFocused
#[test]
fn collect_is_focused_as_state_follows_a_focus() {
    let source = MutableInteractionSource::new();
    let (mut ui, focused) = collecting(&source, |source| source.collect_is_focused_as_state());
    assert!(!value(&focused));
    let focus = Focus::new();
    source.try_emit(FocusInteraction::Focus(focus));
    ui.wait_for_idle();
    assert!(value(&focused));
    source.try_emit(FocusInteraction::Unfocus(focus));
    ui.wait_for_idle();
    assert!(!value(&focused));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt, isFocused_multipleFocuses
#[test]
fn collect_is_focused_as_state_stays_true_until_every_focus_ends() {
    let source = MutableInteractionSource::new();
    let (mut ui, focused) = collecting(&source, |source| source.collect_is_focused_as_state());
    let (first, second) = (Focus::new(), Focus::new());
    source.try_emit(FocusInteraction::Focus(first));
    source.try_emit(FocusInteraction::Focus(second));
    ui.wait_for_idle();
    assert!(value(&focused));
    source.try_emit(FocusInteraction::Unfocus(first));
    ui.wait_for_idle();
    assert!(value(&focused));
    source.try_emit(FocusInteraction::Unfocus(second));
    ui.wait_for_idle();
    assert!(!value(&focused));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt, isPressed
#[test]
fn collect_is_pressed_as_state_follows_a_press() {
    let source = MutableInteractionSource::new();
    let (mut ui, pressed) = collecting(&source, |source| source.collect_is_pressed_as_state());
    assert!(!value(&pressed));
    let press = Press::new(Offset::ZERO);
    source.try_emit(PressInteraction::Press(press));
    ui.wait_for_idle();
    assert!(value(&pressed));
    source.try_emit(PressInteraction::Release(press));
    ui.wait_for_idle();
    assert!(!value(&pressed));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/InteractionSourceTest.kt, isPressed_multiplePresses
#[test]
fn collect_is_pressed_as_state_stays_true_until_every_press_ends() {
    let source = MutableInteractionSource::new();
    let (mut ui, pressed) = collecting(&source, |source| source.collect_is_pressed_as_state());
    let (first, second) = (Press::new(Offset::ZERO), Press::new(Offset::ZERO));
    source.try_emit(PressInteraction::Press(first));
    ui.wait_for_idle();
    assert!(value(&pressed));
    source.try_emit(PressInteraction::Press(second));
    ui.wait_for_idle();
    assert!(value(&pressed));
    source.try_emit(PressInteraction::Release(first));
    ui.wait_for_idle();
    assert!(value(&pressed));
    source.try_emit(PressInteraction::Cancel(second));
    ui.wait_for_idle();
    assert!(!value(&pressed));
}

/// compose-rs's own `is_pressed`: every press since the source was made, a state read.
#[test]
fn is_pressed_reads_every_press_since_the_source_was_made() {
    let mut ui = compose_test(|| {});
    let source = MutableInteractionSource::new();
    let press = Press::new(Offset::ZERO);
    source.try_emit(PressInteraction::Press(press));
    ui.wait_for_idle();
    assert!(source.is_pressed());
    source.try_emit(PressInteraction::Cancel(press));
    assert!(!source.is_pressed());
    // A release of a press it never saw ends nothing.
    source.try_emit(PressInteraction::Press(press));
    source.try_emit(PressInteraction::Release(Press::new(Offset::ZERO)));
    assert!(source.is_pressed());
}
