//! `toggleable`, `tri_state_toggleable`, `selectable`, `focusable` and `hoverable` against
//! Compose's `ToggleableTest`, `SelectableTest`, `FocusableTest` and `HoverableTest`.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    ClickableOptions, FocusRequester, Indication, Key, KeyEvent, KeyEventType,
    MutableInteractionSource, Offset, ToggleableState, UnitExt,
};

use crate::interaction_helpers::*;

const TAG: &str = "target";

fn options(source: &MutableInteractionSource) -> ClickableOptions {
    ClickableOptions {
        interaction_source: Some(source.clone()),
        indication: Indication::NONE,
        ..ClickableOptions::DEFAULT
    }
}

fn key(ui: &mut ComposeTestRule, key: Key, kind: KeyEventType) {
    ui.host().send_key(KeyEvent { key, kind, modifiers: Default::default() });
    ui.wait_for_idle();
}

// ---- Toggleable ----------------------------------------------------------------------------------

/// Each of the three states is in the semantics, with a click action.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_defaultSemantics
#[test]
fn a_tri_state_toggleable_says_its_state() {
    let mut ui = compose_test(|| {
        Column!({
            for (state, tag) in [
                (ToggleableState::On, "on"),
                (ToggleableState::Off, "off"),
                (ToggleableState::Indeterminate, "indeterminate"),
            ] {
                Box!(modifier = Modifier.tri_state_toggleable(state, || {}).test_tag(tag), {
                    BasicText!("ToggleableText");
                });
            }
        });
    });
    for (state, tag) in [
        (ToggleableState::On, "on"),
        (ToggleableState::Off, "off"),
        (ToggleableState::Indeterminate, "indeterminate"),
    ] {
        let config = ui.on_node_with_tag(tag).fetch_semantics_snapshot().config;
        assert_eq!(config.toggleable_state_of(), Some(state));
        assert!(config.is_clickable() && !config.is_disabled());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_booleanOverload_defaultSemantics
#[test]
fn a_toggleable_says_on_or_off() {
    let mut ui = compose_test(|| {
        Column!({
            Box!(modifier = Modifier.toggleable(true, |_| {}).test_tag("checked"), {
                BasicText!("ToggleableText");
            });
            Box!(modifier = Modifier.toggleable(false, |_| {}).test_tag("unchecked"), {
                BasicText!("ToggleableText");
            });
        });
    });
    ui.on_node_with_tag("checked").assert_is_enabled().assert_is_on();
    ui.on_node_with_tag("unchecked").assert_is_enabled().assert_is_off();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_disabledSemantics
#[test]
fn a_disabled_toggleable_is_not_enabled() {
    let mut ui = compose_test(|| {
        let options = ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT };
        Box!(
            modifier = Modifier
                .tri_state_toggleable_with(ToggleableState::On, options.clone(), || {})
                .test_tag(TAG),
            { BasicText!("ToggleableText") }
        );
    });
    ui.on_node_with_tag(TAG).assert_is_not_enabled();
    assert!(ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config.is_clickable());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_toggle
#[test]
fn a_click_toggles() {
    let checked = mutable_state_of(true);
    let handle = checked.handle();
    let mut ui = compose_test(move || {
        Box!(
            modifier =
                Modifier.toggleable(handle.get(), move |value| handle.set(value)).test_tag(TAG),
            {
                BasicText!("ToggleableText");
            }
        );
    });
    ui.on_node_with_tag(TAG).perform_click();
    assert!(!checked.get());
    ui.on_node_with_tag(TAG).assert_is_off();
}

/// Disabled, a toggleable still takes its presses: the clickable around it doesn't click.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_toggle_consumedWhenDisabled
#[test]
fn a_disabled_toggleable_keeps_its_presses_from_the_parent() {
    let outer = Rc::new(Cell::new(0));
    let toggles = Rc::new(Cell::new(0));
    let enabled = mutable_state_of(false);
    let (outer_count, toggle_count, handle) = (outer.clone(), toggles.clone(), enabled.handle());
    let mut ui = compose_test(move || {
        let (outer_count, toggle_count) = (outer_count.clone(), toggle_count.clone());
        Box!(modifier = Modifier.clickable(move || outer_count.set(outer_count.get() + 1)), {
            let toggle_count = toggle_count.clone();
            let options = ClickableOptions { enabled: handle.get(), ..ClickableOptions::DEFAULT };
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .toggleable_with(true, options, move |_| toggle_count
                        .set(toggle_count.get() + 1))
                    .test_tag(TAG)
            );
        });
    });
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!((toggles.get(), outer.get()), (0, 0));
    enabled.set(true);
    ui.wait_for_idle();
    touch_click(&mut ui, at);
    assert_eq!((toggles.get(), outer.get()), (1, 0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_interactionSource_noScrollableContainer
#[test]
fn a_toggleable_emits_its_press() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(
            modifier =
                Modifier.size(20.dp()).toggleable_with(true, options(&given), |_| {}).test_tag(TAG)
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

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_interactionSource_scrollableContainer
#[test]
fn a_toggleable_in_a_scrollable_waits_the_tap_delay() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let state = remember_scroll_state(0);
        let given = given.clone();
        Box!(modifier = Modifier.vertical_scroll(state), {
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .toggleable_with(true, options(&given), |_| {})
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
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_interactionSource_resetWhenDisposed_noScrollableContainer
#[test]
fn a_toggleable_removed_mid_press_cancels_it() {
    let source = MutableInteractionSource::new();
    let shown = mutable_state_of(true);
    let (given, handle) = (source.clone(), shown.handle());
    let mut ui = compose_test(move || {
        let given = given.clone();
        Box!({
            if handle.get() {
                Box!(
                    modifier = Modifier
                        .size(20.dp())
                        .toggleable_with(true, options(&given), |_| {})
                        .test_tag(TAG)
                );
            }
        });
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
    assert!(ends(&recorded, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_interactionSource_hover
#[test]
fn a_toggleable_emits_its_hover() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || {
        Box!(
            modifier =
                Modifier.size(20.dp()).toggleable_with(true, options(&given), |_| {}).test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    mouse_move(&mut ui, at);
    mouse_exit(&mut ui);
    assert_eq!(kinds(&recorded), ["Enter", "Exit"]);
    assert!(ends(&recorded, 0, 1));
}

/// A focused toggleable, its source and its toggle count.
fn focused_toggleable(
    tri_state: bool,
) -> (ComposeTestRule, MutableInteractionSource, Rc<Cell<u32>>, OwnedState<bool>) {
    let source = MutableInteractionSource::new();
    let toggles = Rc::new(Cell::new(0));
    let requester = FocusRequester::new();
    let enabled = mutable_state_of(true);
    let (given, count, handle) = (source.clone(), toggles.clone(), enabled.handle());
    let mut ui = compose_test(move || {
        let count = count.clone();
        let options = ClickableOptions { enabled: handle.get(), ..options(&given) };
        let modifier = Modifier.size(20.dp()).focus_requester(requester);
        let modifier = if tri_state {
            modifier.tri_state_toggleable_with(ToggleableState::Off, options, move || {
                count.set(count.get() + 1)
            })
        } else {
            modifier.toggleable_with(false, options, move |_| count.set(count.get() + 1))
        };
        Box!(modifier = Modifier.padding(10.dp()), {
            Box!(modifier = modifier.clone().test_tag(TAG))
        });
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    (ui, source, toggles, enabled)
}

/// Enter, Space, NumPadEnter and the D-pad's center toggle when focused, each pressing.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_clickWithEnterKey
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_clickWithSpaceKey
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_clickWithNumPadEnterKey
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_clickWithDpadCenter
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_enterKey_emitsIndication
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_numPadEnterKey_emitsIndication
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_dpadCenter_emitsIndication
#[test]
fn clicking_keys_toggle_and_press() {
    for pressed in [Key::Enter, Key::Space, Key::NumPadEnter, Key::DirectionCenter] {
        let (mut ui, source, toggles, _enabled) = focused_toggleable(false);
        let recorded = record(&source);
        key(&mut ui, pressed, KeyEventType::KeyDown);
        assert_eq!(kinds(&recorded), ["Press"], "{pressed:?}");
        key(&mut ui, pressed, KeyEventType::KeyUp);
        assert_eq!(kinds(&recorded), ["Press", "Release"], "{pressed:?}");
        assert_eq!(toggles.get(), 1, "{pressed:?}");
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_clickWithEnterKey_triStateToggleable
#[test]
fn enter_clicks_a_tri_state_toggleable() {
    let (mut ui, _source, toggles, _enabled) = focused_toggleable(true);
    ui.press_key(Key::Enter);
    assert_eq!(toggles.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_otherKey_doesNotEmitIndication
#[test]
fn another_key_neither_toggles_nor_presses() {
    let (mut ui, source, toggles, _enabled) = focused_toggleable(false);
    let recorded = record(&source);
    ui.press_key(Key::Backspace);
    assert!(recorded.borrow().is_empty());
    assert_eq!(toggles.get(), 0);
}

/// Enter pressed twice emits two presses, each released.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_doubleEnterKey_emitsFurtherInteractions
#[test]
fn enter_twice_emits_two_presses() {
    let (mut ui, source, _toggles, _enabled) = focused_toggleable(false);
    let recorded = record(&source);
    ui.press_key(Key::Enter);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    assert_eq!(kinds(&recorded), ["Press", "Release", "Press"]);
    key(&mut ui, Key::Enter, KeyEventType::KeyUp);
    assert_eq!(kinds(&recorded), ["Press", "Release", "Press", "Release"]);
}

/// A held key's repeats emit no more presses.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_repeatKeyEvents_doNotEmitFurtherInteractions
#[test]
fn a_held_keys_repeats_emit_no_more_presses() {
    let (mut ui, source, _toggles, _enabled) = focused_toggleable(false);
    let recorded = record(&source);
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    assert_eq!(kinds(&recorded), ["Press"]);
    key(&mut ui, Key::Enter, KeyEventType::KeyUp);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
}

/// Disabled mid key press: the press is cancelled, and the key's release emits nothing.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleableTest_interruptedClick_emitsCancelIndication
#[test]
fn disabled_mid_key_press_the_press_is_cancelled() {
    let (mut ui, source, toggles, enabled) = focused_toggleable(false);
    let recorded = record(&source);
    key(&mut ui, Key::Enter, KeyEventType::KeyDown);
    assert_eq!(kinds(&recorded), ["Press"]);
    enabled.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag(TAG).assert_is_not_enabled();
    let presses = |recorded: &Recorded| {
        kinds(recorded)
            .into_iter()
            .filter(|kind| !matches!(*kind, "Focus" | "Unfocus"))
            .collect::<Vec<_>>()
    };
    assert_eq!(presses(&recorded), ["Press", "Cancel"]);
    key(&mut ui, Key::Enter, KeyEventType::KeyUp);
    assert_eq!(presses(&recorded), ["Press", "Cancel"]);
    assert_eq!(toggles.get(), 0);
}

/// The toggleables' modifiers compare as Compose's elements: equal for the same arguments, not
/// for another `enabled` or `state`.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleable_nullInteractionSource_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, toggleable_nonNullInteractionSource_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, triStateToggleable_nullInteractionSource_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/ToggleableTest.kt, triStateToggleable_nonNullInteractionSource_equality
#[test]
fn equal_arguments_make_equal_toggleables() {
    let on_value_change: Callback<dyn Fn(bool)> = callback(|_| {});
    let on_click: Callback<dyn Fn()> = callback(|| {});
    let source = MutableInteractionSource::new();
    for interaction_source in [None, Some(source.clone())] {
        let with = |enabled| ClickableOptions {
            enabled,
            interaction_source: interaction_source.clone(),
            ..ClickableOptions::DEFAULT
        };
        let toggle =
            |enabled| Modifier.toggleable_with(true, with(enabled), on_value_change.clone());
        assert!(toggle(true).same(&toggle(true)) && !toggle(true).same(&toggle(false)));
        let tri = |enabled, state| {
            Modifier.tri_state_toggleable_with(state, with(enabled), on_click.clone())
        };
        assert!(tri(true, ToggleableState::On).same(&tri(true, ToggleableState::On)));
        assert!(!tri(true, ToggleableState::On).same(&tri(false, ToggleableState::On)));
        assert!(!tri(true, ToggleableState::On).same(&tri(true, ToggleableState::Indeterminate)));
    }
}

// ---- Selectable ----------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt, selectable_defaultSemantics
#[test]
fn a_selectable_says_it_is_selected() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.selectable(false, || {}).test_tag(TAG), { BasicText!("Text") });
    });
    let config = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config;
    assert_eq!(config.selected_of(), Some(false));
    assert!(config.is_clickable() && !config.is_disabled());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt, selectable_defaultClicks
#[test]
fn a_click_selects() {
    let selected = mutable_state_of(false);
    let handle = selected.handle();
    let mut ui = compose_test(move || {
        Box!(
            modifier =
                Modifier.selectable(handle.get(), move || handle.set(!handle.get())).test_tag(TAG),
            {
                BasicText!("Text");
            }
        );
    });
    ui.on_node_with_tag(TAG).perform_click();
    assert!(selected.get());
    assert_eq!(
        ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config.selected_of(),
        Some(true)
    );
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt, selectable_noClicksNoChanges
#[test]
fn without_a_click_nothing_changes() {
    let selected = mutable_state_of(false);
    let handle = selected.handle();
    let mut ui = compose_test(move || {
        Box!(
            modifier =
                Modifier.selectable(handle.get(), move || handle.set(!handle.get())).test_tag(TAG),
            {
                BasicText!("Text");
            }
        );
    });
    assert_eq!(
        ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config.selected_of(),
        Some(false)
    );
    assert!(!selected.get());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt, selectable_clicks_noPropagationWhenDisabled
#[test]
fn a_disabled_selectable_keeps_its_presses_from_the_parent() {
    let outer = Rc::new(Cell::new(0));
    let selects = Rc::new(Cell::new(0));
    let (outer_count, select_count) = (outer.clone(), selects.clone());
    let mut ui = compose_test(move || {
        let (outer_count, select_count) = (outer_count.clone(), select_count.clone());
        Box!(modifier = Modifier.clickable(move || outer_count.set(outer_count.get() + 1)), {
            let select_count = select_count.clone();
            let options = ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT };
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .selectable_with(false, options, move || select_count
                        .set(select_count.get() + 1))
                    .test_tag(TAG)
            );
        });
    });
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!((selects.get(), outer.get()), (0, 0));
}

/// A selectable's interactions are a clickable's: its press, its hover, its keys.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt, selectableTest_interactionSource_noScrollableContainer
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt, selectableTest_interactionSource_hover
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/selection/SelectableTest.kt, selectableTest_enterKey_emitsIndication
#[test]
fn a_selectable_emits_its_press_hover_and_key_presses() {
    let source = MutableInteractionSource::new();
    let requester = FocusRequester::new();
    let clicks = Rc::new(Cell::new(0));
    let (given, count) = (source.clone(), clicks.clone());
    let mut ui = compose_test(move || {
        let count = count.clone();
        Box!(
            modifier = Modifier
                .size(20.dp())
                .focus_requester(requester)
                .selectable_with(false, options(&given), move || count.set(count.get() + 1))
                .test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    recorded.borrow_mut().clear();
    mouse_move(&mut ui, at);
    mouse_exit(&mut ui);
    assert_eq!(kinds(&recorded), ["Enter", "Exit"]);
    assert!(requester.request_focus());
    ui.wait_for_idle();
    recorded.borrow_mut().clear();
    ui.press_key(Key::DirectionCenter);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert_eq!(clicks.get(), 2);
}

// ---- Focusable -----------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_defaultSemantics
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_disabledSemantics
#[test]
fn a_focusable_is_focusable_and_a_disabled_one_is_not() {
    let mut ui = compose_test(|| {
        Column!({
            BasicText!("focusable", modifier = Modifier.test_tag("enabled").focusable());
            BasicText!(
                "not focusable",
                modifier = Modifier.test_tag("disabled").focusable_with(false, None)
            );
        });
    });
    assert!(ui.on_node_with_tag("enabled").fetch_semantics_snapshot().config.is_focusable());
    assert!(!ui.on_node_with_tag("disabled").fetch_semantics_snapshot().config.is_focusable());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_focusAcquire
#[test]
fn a_focusable_takes_and_loses_the_focus() {
    let (first, second) = (FocusRequester::new(), FocusRequester::new());
    let mut ui = compose_test(move || {
        Column!({
            BasicText!(
                "focusable",
                modifier = Modifier.test_tag(TAG).focus_requester(first).focusable()
            );
            BasicText!("other", modifier = Modifier.focus_requester(second).focusable());
        });
    });
    assert!(!ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config.is_focused());
    assert!(first.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag(TAG).assert_is_focused();
    assert!(second.request_focus());
    ui.wait_for_idle();
    assert!(!ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config.is_focused());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_interactionSource
#[test]
fn a_focusable_emits_its_focus_and_unfocus() {
    let source = MutableInteractionSource::new();
    let (first, second) = (FocusRequester::new(), FocusRequester::new());
    let given = source.clone();
    let mut ui = compose_test(move || {
        let given = given.clone();
        Column!({
            BasicText!(
                "focusable",
                modifier =
                    Modifier.focus_requester(first).focusable_with(true, Some(given.clone()))
            );
            BasicText!("other", modifier = Modifier.focus_requester(second).focusable());
        });
    });
    let recorded = record(&source);
    assert!(first.request_focus());
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Focus"]);
    assert!(second.request_focus());
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Focus", "Unfocus"]);
    assert!(ends(&recorded, 0, 1));
}

/// A focused focusable disabled, its modifier removed, or its node removed: its focus ends.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_interactionSource_resetWhenDisabled
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_interactionSource_resetWhenModifierDetached
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_interactionSource_resetWhenLayoutDetached
#[test]
fn a_focusables_focus_ends_when_disabled_or_removed() {
    for case in 0..3 {
        let source = MutableInteractionSource::new();
        let requester = FocusRequester::new();
        let on = mutable_state_of(true);
        let (given, handle) = (source.clone(), on.handle());
        let mut ui = compose_test(move || {
            let given = given.clone();
            Box!({
                match case {
                    0 => BasicText!(
                        "focusable",
                        modifier = Modifier
                            .focus_requester(requester)
                            .focusable_with(handle.get(), Some(given.clone()))
                    ),
                    1 => {
                        let focusable = if handle.get() {
                            Modifier.focusable_with(true, Some(given.clone()))
                        } else {
                            Modifier
                        };
                        BasicText!(
                            "focusable",
                            modifier = Modifier.focus_requester(requester).then(focusable)
                        )
                    }
                    _ => {
                        if handle.get() {
                            BasicText!(
                                "focusable",
                                modifier = Modifier
                                    .focus_requester(requester)
                                    .focusable_with(true, Some(given.clone()))
                            );
                        }
                    }
                }
            });
        });
        let recorded = record(&source);
        assert!(requester.request_focus());
        ui.wait_for_idle();
        assert_eq!(kinds(&recorded), ["Focus"], "case {case}");
        on.set(false);
        ui.wait_for_idle();
        assert_eq!(kinds(&recorded), ["Focus", "Unfocus"], "case {case}");
        assert!(ends(&recorded, 0, 1));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, focusable_equality
#[test]
fn equal_arguments_make_equal_focusables() {
    let source = MutableInteractionSource::new();
    let focusable = |enabled| Modifier.focusable_with(enabled, Some(source.clone()));
    assert!(focusable(true).same(&focusable(true)));
    assert!(!focusable(true).same(&focusable(false)));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/FocusableTest.kt, reusingFocusedItem_itemIsNotFocusedAnymore
#[test]
fn a_reused_focused_item_is_not_focused_any_more() {
    let requester = FocusRequester::new();
    let key_state = mutable_state_of(0);
    let handle = key_state.handle();
    let mut ui = compose_test(move || {
        ReusableContent(handle.get(), || {
            BasicText!(
                "focusable",
                modifier = Modifier.test_tag(TAG).focus_requester(requester).focusable()
            );
        });
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag(TAG).assert_is_focused();
    key_state.set(1);
    ui.wait_for_idle();
    assert!(!ui.on_node_with_tag(TAG).fetch_semantics_snapshot().config.is_focused());
}

// ---- Hoverable -----------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt, hoverableTest_equality
#[test]
fn equal_arguments_make_equal_hoverables() {
    let source = MutableInteractionSource::new();
    let hoverable = |enabled| Modifier.hoverable_with(source.clone(), enabled);
    assert!(hoverable(true).same(&hoverable(true)));
    assert!(!hoverable(true).same(&hoverable(false)));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt, hoverableTest_hovered
#[test]
fn collect_is_hovered_as_state_follows_the_mouse() {
    let source = MutableInteractionSource::new();
    let hovered = Rc::new(Cell::new(false));
    let (given, seen) = (source.clone(), hovered.clone());
    let mut ui = ComposeTestRule::sized(compose_ui::IntSize::new(300, 300), move || {
        Box!(modifier = Modifier.size(128.dp()).test_tag(TAG).hoverable(given.clone()));
        seen.set(given.collect_is_hovered_as_state().get());
    });
    for (at, expected) in [(64.0, true), (96.0, true), (129.0, false), (96.0, true)] {
        mouse_move(&mut ui, Offset::new(at, at));
        assert_eq!(hovered.get(), expected, "at {at}");
    }
}

/// The hover emitted into the source, its exit naming its enter; removed, reused or disabled
/// while hovered, the hover ends.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt, hoverableTest_interactionSource
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt, hoverableTest_interactionSource_resetWhenDisposed
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt, hoverableTest_interactionSource_resetWhenReused
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt, hoverableTest_interactionSource_resetWhenDisabled
#[test]
fn a_hoverables_hover_ends_when_exited_removed_reused_or_disabled() {
    for case in 0..4 {
        let source = MutableInteractionSource::new();
        let on = mutable_state_of(true);
        let (given, handle) = (source.clone(), on.handle());
        let mut ui = compose_test(move || {
            let given = given.clone();
            Box!({
                match case {
                    0 | 3 => Box!(
                        modifier = Modifier
                            .size(128.dp())
                            .test_tag(TAG)
                            .hoverable_with(given.clone(), case == 0 || handle.get())
                    ),
                    1 => {
                        if handle.get() {
                            Box!(
                                modifier =
                                    Modifier.size(128.dp()).test_tag(TAG).hoverable(given.clone())
                            );
                        }
                    }
                    _ => {
                        let given = given.clone();
                        ReusableContent(handle.get(), move || {
                            Box!(
                                modifier =
                                    Modifier.size(128.dp()).test_tag(TAG).hoverable(given.clone())
                            );
                        });
                    }
                }
            });
        });
        let recorded = record(&source);
        mouse_move(&mut ui, Offset::new(64.0, 64.0));
        assert_eq!(kinds(&recorded), ["Enter"], "case {case}");
        if case == 0 {
            mouse_move(&mut ui, Offset::new(300.0, 300.0));
        } else {
            on.set(false);
            ui.wait_for_idle();
        }
        assert_eq!(kinds(&recorded), ["Enter", "Exit"], "case {case}");
        assert!(ends(&recorded, 0, 1));
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/HoverableTest.kt, hoverableTest_interactionSource_dontHoverWhenDisabled
#[test]
fn a_disabled_hoverable_emits_nothing() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || {
        Box!(modifier = Modifier.size(128.dp()).hoverable_with(given.clone(), false));
    });
    let recorded = record(&source);
    mouse_move(&mut ui, Offset::new(64.0, 64.0));
    assert!(recorded.borrow().is_empty());
}
