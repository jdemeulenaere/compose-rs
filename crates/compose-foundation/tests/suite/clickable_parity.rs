//! `clickable` against Compose's `ClickableTest`: semantics, clicks, the interactions it emits
//! (presses delayed under scrollables, hover, focus, keys), and when it makes its indication's
//! node.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    ClickableOptions, FocusRequester, Indication, IndicationNodeFactory, Interaction,
    InteractionSource, Key, LOCAL_INDICATION, ModifierNode, MutableInteractionSource, Offset, Role,
    TAP_INDICATION_DELAY, UnitExt,
};

use crate::interaction_helpers::*;

const TAG: &str = "myClickable";

fn options(source: &MutableInteractionSource) -> ClickableOptions {
    ClickableOptions {
        interaction_source: Some(source.clone()),
        indication: Indication::NONE,
        ..ClickableOptions::DEFAULT
    }
}

/// A 100 x 50 clickable emitting into `source`, with no indication.
#[composable]
fn Target(source: MutableInteractionSource) {
    Box!(
        modifier = Modifier
            .size((100.dp(), 50.dp()))
            .clickable_with(options(&source), || {})
            .test_tag(TAG)
    );
}

// ---- Semantics ---------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, defaultSemantics
#[test]
fn a_clickable_is_enabled_with_a_click_action_and_no_role() {
    let mut ui = compose_test(|| {
        Box!({ BasicText!("ClickableText", modifier = Modifier.test_tag(TAG).clickable(|| {})) });
    });
    let node = ui.on_node_with_tag(TAG).fetch_semantics_node();
    assert_eq!(node.config.role_of(), None);
    assert!(!node.config.is_disabled() && node.config.is_clickable());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, disabledSemantics
#[test]
fn a_disabled_clickable_is_not_enabled_and_keeps_its_click_action() {
    let mut ui = compose_test(|| {
        let options = ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT };
        Box!({
            BasicText!(
                "ClickableText",
                modifier = Modifier.test_tag(TAG).clickable_with(options.clone(), || {})
            )
        });
    });
    let node = ui.on_node_with_tag(TAG).fetch_semantics_node();
    assert_eq!(node.config.role_of(), None);
    assert!(node.config.is_disabled() && node.config.is_clickable());
}

/// The click action of a disabled clickable calls its callback, as Compose's: refusing a
/// disabled node's actions is the platform bridges' (D1255).
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, applySemantics
#[test]
fn a_disabled_clickable_s_click_action_calls_its_callback() {
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    let mut ui = compose_test(move || {
        let count = count.clone();
        let options = ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT };
        Box!(
            modifier = Modifier
                .size(10.dp())
                .test_tag(TAG)
                .clickable_with(options, move || count.set(count.get() + 1))
        );
    });
    ui.on_node_with_tag(TAG).perform_semantics_action(&compose_ui::SemanticsActions::ON_CLICK);
    assert_eq!(clicks.get(), 1);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, semanticsInvalidation
#[test]
fn the_semantics_follow_enabled_and_the_role() {
    let enabled = mutable_state_of(true);
    let role = mutable_state_of(Some(Role::Button));
    let (enabled_handle, role_handle) = (enabled.handle(), role.handle());
    let mut ui = compose_test(move || {
        let options = ClickableOptions {
            enabled: enabled_handle.get(),
            role: role_handle.get(),
            ..ClickableOptions::DEFAULT
        };
        Box!({
            BasicText!(
                "ClickableText",
                modifier = Modifier.test_tag(TAG).clickable_with(options.clone(), || {})
            )
        });
    });
    let config = |ui: &mut ComposeTestRule| ui.on_node_with_tag(TAG).fetch_semantics_node().config;
    assert_eq!(config(&mut ui).role_of(), Some(Role::Button));
    role.set(None);
    ui.wait_for_idle();
    assert_eq!(config(&mut ui).role_of(), None);
    assert!(!config(&mut ui).is_disabled());
    enabled.set(false);
    ui.wait_for_idle();
    assert!(config(&mut ui).is_disabled() && config(&mut ui).is_clickable());
}

/// `on_click_label` is the click action's label.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, movableContentWithSubcomposition_updatingSemanticsShouldNotCrash
#[test]
fn the_click_label_is_in_the_semantics_and_follows_changes() {
    let label = mutable_state_of(false);
    let handle = label.handle();
    let mut ui = compose_test(move || {
        let options = ClickableOptions {
            on_click_label: Some(handle.get().to_string().into()),
            ..ClickableOptions::DEFAULT
        };
        Box!(modifier = Modifier.size(10.dp()).clickable_with(options, || {}).test_tag(TAG));
    });
    let label_of = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag(TAG)
            .fetch_semantics_node()
            .config
            .click_label()
            .map(|label| label.to_string())
    };
    assert_eq!(label_of(&mut ui).as_deref(), Some("false"));
    label.set(true);
    ui.wait_for_idle();
    assert_eq!(label_of(&mut ui).as_deref(), Some("true"));
}

// ---- Clicks ------------------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, click
#[test]
fn each_tap_clicks() {
    let counter = Rc::new(Cell::new(0));
    let count = counter.clone();
    let mut ui = compose_test(move || {
        let count = count.clone();
        Box!({
            let count = count.clone();
            BasicText!(
                "ClickableText",
                modifier = Modifier.test_tag(TAG).clickable(move || count.set(count.get() + 1))
            )
        });
    });
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!(counter.get(), 1);
    touch_click(&mut ui, at);
    assert_eq!(counter.get(), 2);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, clickOnChildBasicText
#[test]
fn a_tap_on_a_child_text_clicks_the_parent() {
    let counter = Rc::new(Cell::new(0));
    let count = counter.clone();
    let mut ui = compose_test(move || {
        let count = count.clone();
        Column!(modifier = Modifier.clickable(move || count.set(count.get() + 1)), {
            BasicText!("Foo");
            BasicText!("Bar");
        });
    });
    ui.on_node_with_text("Foo").perform_click();
    assert_eq!(counter.get(), 1);
    ui.on_node_with_text("Bar").perform_click();
    assert_eq!(counter.get(), 2);
}

// ---- Presses -----------------------------------------------------------------------------------

/// Outside a scrollable, a press is emitted at once, its release naming it.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_noScrollableContainer
#[test]
fn a_press_outside_a_scrollable_is_emitted_at_once() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || Target(given.clone()));
    let recorded = record(&source);
    ui.wait_for_idle();
    assert!(recorded.borrow().is_empty());
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press"]);
    touch_up(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert!(ends(&recorded, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_immediateRelease_noScrollableContainer
#[test]
fn an_immediate_release_outside_a_scrollable_emits_the_press_and_its_release() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || Target(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert!(ends(&recorded, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_immediateCancel_noScrollableContainer
#[test]
fn an_immediate_cancel_outside_a_scrollable_emits_the_press_and_its_cancel() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || Target(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    touch_cancel(&mut ui);
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
    assert!(ends(&recorded, 0, 1));
}

/// A draggable before the clickable waits for its slop: the press waits, and a drag before the
/// delay shows none.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_immediateDrag_insideDraggable
#[test]
fn a_drag_before_the_delay_inside_a_draggable_emits_no_press() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let state = remember_draggable_state!(|_| {});
        Box!(
            modifier = Modifier
                .size((100.dp(), 50.dp()))
                .draggable(state, Orientation::Horizontal, true, false, |_| {}, |_| {})
                .clickable_with(options(&given), || {})
                .test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let bounds = bounds(&mut ui, TAG);
    let (left, right) = (bounds.left + 1.0, bounds.right - 1.0);
    let y = (bounds.top + bounds.bottom) / 2.0;
    touch_down(&mut ui, Offset::new(left, y));
    touch_move(&mut ui, Offset::new(right, y));
    ui.advance_time_by(TAP_INDICATION_DELAY);
    assert!(recorded.borrow().is_empty(), "{:?}", kinds(&recorded));
}

/// A clickable in a vertical scroll, 100 x 50.
#[composable]
fn ScrolledTarget(source: MutableInteractionSource) {
    let state = remember_scroll_state(0);
    Box!(modifier = Modifier.vertical_scroll(state), { Target(source.clone()) });
}

/// Inside a scrollable, a press waits `TAP_INDICATION_DELAY` before it's emitted.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_scrollableContainer
#[test]
fn a_press_inside_a_scrollable_waits_the_tap_delay() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(TAP_INDICATION_DELAY / 2);
    assert!(recorded.borrow().is_empty());
    ui.advance_time_by(TAP_INDICATION_DELAY / 2);
    assert_eq!(kinds(&recorded), ["Press"]);
    touch_up(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert!(ends(&recorded, 0, 1));
}

/// Released before the delay: the press and its release are emitted together.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_immediateRelease_scrollableContainer
#[test]
fn a_release_before_the_delay_inside_a_scrollable_emits_both_at_once() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_click(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert!(ends(&recorded, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_immediateCancel_scrollableContainer
#[test]
fn a_cancel_before_the_delay_inside_a_scrollable_emits_nothing() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    touch_cancel(&mut ui);
    ui.advance_time_by(TAP_INDICATION_DELAY);
    assert!(recorded.borrow().is_empty());
}

/// A scroll before the delay: no press is ever shown.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_immediateDrag_scrollableContainer
#[test]
fn a_scroll_before_the_delay_emits_no_press() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let bounds = bounds(&mut ui, TAG);
    let x = (bounds.left + bounds.right) / 2.0;
    touch_down(&mut ui, Offset::new(x, bounds.bottom - 1.0));
    touch_move(&mut ui, Offset::new(x, bounds.top + 1.0));
    ui.advance_time_by(TAP_INDICATION_DELAY);
    assert!(recorded.borrow().is_empty(), "{:?}", kinds(&recorded));
}

/// A scroll after the delay cancels the press shown.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_dragAfterTimeout_scrollableContainer
#[test]
fn a_scroll_after_the_delay_cancels_the_press() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let bounds = bounds(&mut ui, TAG);
    let x = (bounds.left + bounds.right) / 2.0;
    touch_down(&mut ui, Offset::new(x, bounds.bottom - 1.0));
    ui.advance_time_by(TAP_INDICATION_DELAY);
    assert_eq!(kinds(&recorded), ["Press"]);
    touch_move(&mut ui, Offset::new(x, bounds.top + 1.0));
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
    assert!(ends(&recorded, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_cancelledGesture_scrollableContainer
#[test]
fn a_cancel_after_the_delay_cancels_the_press() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(TAP_INDICATION_DELAY);
    assert_eq!(kinds(&recorded), ["Press"]);
    touch_cancel(&mut ui);
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
    assert!(ends(&recorded, 0, 1));
}

/// Presses and releases faster than the delay each emit a press and its release, in order.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_scrollableContainer_fastSuccessivePressesAndReleases
#[test]
fn fast_taps_inside_a_scrollable_each_emit_their_press_and_release() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    for _ in 0..3 {
        touch_click(&mut ui, at);
        ui.advance_time_by(Duration::from_millis(10));
    }
    ui.advance_time_by(TAP_INDICATION_DELAY);
    assert_eq!(kinds(&recorded), ["Press", "Release", "Press", "Release", "Press", "Release"]);
    assert!(ends(&recorded, 0, 1) && ends(&recorded, 2, 3) && ends(&recorded, 4, 5));
}

/// Released just before the delay ends: still the press and its release, once.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_scrollableContainer_releaseJustBeforeTapDelayFinishes
#[test]
fn a_release_just_before_the_delay_ends_emits_one_press_and_release() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    ui.advance_time_by(TAP_INDICATION_DELAY - Duration::from_millis(1));
    touch_up(&mut ui, at);
    ui.advance_time_by(TAP_INDICATION_DELAY);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    assert!(ends(&recorded, 0, 1));
}

/// A mouse press in a scrollable isn't delayed: scrollables don't drag with a mouse.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_hover_and_press_scrollableContainer
#[test]
fn a_mouse_press_inside_a_scrollable_is_emitted_at_once() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || ScrolledTarget(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    mouse_move(&mut ui, at);
    mouse_press(&mut ui, at);
    ui.advance_time_by(TAP_INDICATION_DELAY / 2);
    assert_eq!(kinds(&recorded), ["Enter", "Press"]);
    ui.advance_time_by(TAP_INDICATION_DELAY / 2);
    assert_eq!(kinds(&recorded), ["Enter", "Press"]);
    mouse_release(&mut ui, at);
    mouse_exit(&mut ui);
    assert_eq!(kinds(&recorded), ["Enter", "Press", "Release", "Exit"]);
    assert!(ends(&recorded, 1, 2) && ends(&recorded, 0, 3));
}

/// Leaving mid-press cancels the press.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_resetWhenDisposed
#[test]
fn a_clickable_removed_mid_press_cancels_it() {
    let source = MutableInteractionSource::new();
    let shown = mutable_state_of(true);
    let (given, handle) = (source.clone(), shown.handle());
    let mut ui = compose_test(move || {
        let given = given.clone();
        Box!({
            if handle.get() {
                Target(given.clone());
            }
        });
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press"]);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
    assert!(ends(&recorded, 0, 1));
}

/// Reused mid-press, the press is cancelled.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_resetWhenReused
#[test]
fn a_clickable_reused_mid_press_cancels_it() {
    let source = MutableInteractionSource::new();
    let key = mutable_state_of(true);
    let (given, handle) = (source.clone(), key.handle());
    let mut ui = compose_test(move || {
        let given = given.clone();
        Box!({
            let given = given.clone();
            ReusableContent(handle.get(), move || Target(given));
        });
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Press"]);
    key.set(false);
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Press", "Cancel"]);
    assert!(ends(&recorded, 0, 1));
}

// ---- Hover and focus ---------------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_hover
#[test]
fn a_mouse_over_the_clickable_emits_a_hover() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || Target(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    mouse_move(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Enter"]);
    mouse_move(&mut ui, Offset::new(300.0, 300.0));
    assert_eq!(kinds(&recorded), ["Enter", "Exit"]);
    assert!(ends(&recorded, 0, 1));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_hover_and_press
#[test]
fn a_mouse_click_emits_its_press_inside_the_hover() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || Target(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    mouse_move(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(50));
    mouse_press(&mut ui, at);
    mouse_release(&mut ui, at);
    ui.advance_time_by(Duration::from_millis(50));
    mouse_exit(&mut ui);
    assert_eq!(kinds(&recorded), ["Enter", "Press", "Release", "Exit"]);
    assert!(ends(&recorded, 1, 2) && ends(&recorded, 0, 3));
}

/// A release and the mouse leaving the window between two frames: the release, then the exit,
/// in the order they happened, as Compose's emissions launched in order (one queue, D1057).
#[test]
fn a_release_then_an_exit_in_one_frame_emit_in_that_order() {
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || Target(given.clone()));
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    mouse_move(&mut ui, at);
    mouse_press(&mut ui, at);
    ui.host().send_pointer(compose_ui::PointerEvent::release(at));
    ui.host().send_pointer_left();
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Enter", "Press", "Release", "Exit"]);
    assert!(!source.is_pressed() && !source.is_hovered());
}

/// Disabled, the clickable emits no hover; enabled again, it does.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, noHover_whenDisabled
#[test]
fn a_disabled_clickable_emits_no_hover() {
    let source = MutableInteractionSource::new();
    let enabled = mutable_state_of(true);
    let (given, handle) = (source.clone(), enabled.handle());
    let mut ui = compose_test(move || {
        let options = ClickableOptions { enabled: handle.get(), ..options(&given) };
        Box!(
            modifier =
                Modifier.size((100.dp(), 50.dp())).clickable_with(options, || {}).test_tag(TAG)
        );
    });
    let recorded = record(&source);
    let at = center(&mut ui, TAG);
    mouse_move(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Enter"]);
    mouse_exit(&mut ui);
    recorded.borrow_mut().clear();
    enabled.set(false);
    ui.wait_for_idle();
    mouse_move(&mut ui, at);
    assert!(recorded.borrow().is_empty());
    mouse_exit(&mut ui);
    enabled.set(true);
    ui.wait_for_idle();
    mouse_move(&mut ui, at);
    assert_eq!(kinds(&recorded), ["Enter"]);
}

/// Focused (compose-rs is always in Compose's keyboard mode, D1052), the clickable emits a
/// focus; cleared, its unfocus.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, interactionSource_focus_inKeyboardMode
#[test]
fn focusing_the_clickable_emits_a_focus_and_clearing_it_an_unfocus() {
    let source = MutableInteractionSource::new();
    let requester = FocusRequester::new();
    let given = source.clone();
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier
                .size(10.dp())
                .focus_requester(requester)
                .clickable_with(options(&given), || {})
                .test_tag(TAG)
        );
    });
    let recorded = record(&source);
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Focus"]);
    ui.host().with_tree(|tree| tree.clear_focus());
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Focus", "Unfocus"]);
    assert!(ends(&recorded, 0, 1));
}

/// Disabled while focused, the clickable loses the focus and can't take it again (compose-rs is
/// always in Compose's keyboard mode, as the test's forced input mode).
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, noFocus_whenDisabled
#[test]
fn a_clickable_disabled_while_focused_loses_the_focus() {
    let enabled = mutable_state_of(true);
    let focused = Rc::new(Cell::new(false));
    let requester = FocusRequester::new();
    let (handle, seen) = (enabled.handle(), focused.clone());
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        let options = ClickableOptions { enabled: handle.get(), ..ClickableOptions::DEFAULT };
        BasicText!(
            "ClickableText",
            modifier = Modifier
                .test_tag(TAG)
                .focus_requester(requester)
                .on_focus_changed(move |focus: compose_ui::FocusState| seen.set(focus.has_focus()))
                .clickable_with(options, || {})
        );
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert!(focused.get());
    enabled.set(false);
    ui.wait_for_idle();
    assert!(!focused.get());
    requester.request_focus();
    ui.wait_for_idle();
    assert!(!focused.get());
}

/// A clickable inside a disabled one takes the focus.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, disabledParentClickable_doesNotCrashOnFocusEvent
#[test]
fn a_clickable_inside_a_disabled_one_takes_the_focus() {
    let requester = FocusRequester::new();
    let mut ui = compose_test(move || {
        let options = ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT };
        Box!(modifier = Modifier.size(100.dp()).clickable_with(options, || {}), {
            Box!(
                modifier = Modifier
                    .size(10.dp())
                    .focus_requester(requester)
                    .test_tag(TAG)
                    .clickable(|| {})
            );
        });
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag(TAG).assert_is_focused();
}

/// A new source while focused: the old one's focus ends, as Compose's `FocusableNode.update`.
#[test]
fn a_new_source_while_focused_ends_the_old_ones_focus() {
    let first = MutableInteractionSource::new();
    let source = mutable_state_of(first.clone());
    let requester = FocusRequester::new();
    let handle = source.handle();
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier
                .size(10.dp())
                .focus_requester(requester)
                .clickable_with(options(&handle.get()), || {})
        );
    });
    let recorded = record(&first);
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Focus"]);
    source.set(MutableInteractionSource::new());
    ui.wait_for_idle();
    assert_eq!(kinds(&recorded), ["Focus", "Unfocus"]);
    assert!(ends(&recorded, 0, 1));
}

/// A key that doesn't click emits nothing.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, otherKey_doesNotEmitInteraction
#[test]
fn a_key_that_does_not_click_emits_nothing() {
    let source = MutableInteractionSource::new();
    let requester = FocusRequester::new();
    let given = source.clone();
    let mut ui = compose_test(move || {
        let given = given.clone();
        Box!(modifier = Modifier.padding(10.dp()), {
            Box!(
                modifier = Modifier
                    .size(10.dp())
                    .focus_requester(requester)
                    .clickable_with(options(&given), || {})
            );
        });
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    let recorded = record(&source);
    ui.press_key(Key::Backspace);
    assert!(recorded.borrow().is_empty());
}

/// Enter, Space and the D-pad's center press and click; the release names the press.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableParameterizedKeyInputTest.kt, clickWithKey
#[test]
fn each_clicking_key_presses_and_clicks() {
    for key in [Key::Enter, Key::NumPadEnter, Key::Space, Key::DirectionCenter] {
        let source = MutableInteractionSource::new();
        let requester = FocusRequester::new();
        let clicks = Rc::new(Cell::new(0));
        let (given, count) = (source.clone(), clicks.clone());
        let mut ui = compose_test(move || {
            let count = count.clone();
            Box!(
                modifier = Modifier
                    .size(10.dp())
                    .focus_requester(requester)
                    .clickable_with(options(&given), move || count.set(count.get() + 1))
            );
        });
        assert!(requester.request_focus());
        ui.wait_for_idle();
        let recorded = record(&source);
        ui.press_key(key);
        assert_eq!(clicks.get(), 1, "{key:?}");
        assert_eq!(kinds(&recorded), ["Press", "Release"], "{key:?}");
        assert!(ends(&recorded, 0, 1));
    }
}

// ---- The indication's node ---------------------------------------------------------------------

/// What `TestIndication` saw: its nodes made (with their sources), detached, and the
/// interactions they collected.
#[derive(Default)]
struct Seen {
    sources: RefCell<Vec<InteractionSource>>,
    detached: Cell<u32>,
    interactions: RefCell<Vec<Interaction>>,
}

impl Seen {
    fn created(&self) -> usize {
        self.sources.borrow().len()
    }
}

/// A factory recording what its nodes see, as Compose's `TestIndicationNodeFactory`: its node
/// collects its source from when it's made until it's detached (`on_detach`, not its drop: a
/// node replaced or reset is detached, as Compose's `undelegate`).
struct TestIndication(Rc<Seen>);

struct TestIndicationNode {
    seen: Rc<Seen>,
    job: Job,
}

impl ModifierNode for TestIndicationNode {
    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.job.cancel();
        self.seen.detached.set(self.seen.detached.get() + 1);
    }
}

impl IndicationNodeFactory for TestIndication {
    fn create(&self, interaction_source: InteractionSource) -> Box<dyn ModifierNode> {
        self.0.sources.borrow_mut().push(interaction_source.clone());
        let seen = self.0.clone();
        let interactions = interaction_source.interactions();
        let job = compose_runtime::platform::launch(async move {
            interactions
                .collect(move |interaction| seen.interactions.borrow_mut().push(interaction))
                .await
        });
        Box::new(TestIndicationNode { seen: self.0.clone(), job })
    }

    fn same(&self, other: &dyn IndicationNodeFactory) -> bool {
        other.downcast_ref::<TestIndication>().is_some_and(|other| Rc::ptr_eq(&self.0, &other.0))
    }
}

fn test_indication() -> (Rc<Seen>, Indication) {
    let seen = Rc::new(Seen::default());
    (seen.clone(), Indication::new(TestIndication(seen)))
}

fn seen_kinds(seen: &Seen) -> Vec<&'static str> {
    let recorded = Rc::new(RefCell::new(seen.interactions.borrow().clone()));
    kinds(&recorded)
}

/// Turning to `Indication::LOCAL` with a node made and no source: the new one is made at once, as
/// `recreateIndicationIfNeeded` (a node made wasn't lazily waiting any more).
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, recreateIndicationIfNeeded
#[test]
fn a_clickable_with_its_node_made_turning_local_makes_the_new_one_at_once() {
    let (explicit, explicit_indication) = test_indication();
    let (local_seen, local_indication) = test_indication();
    let local = mutable_state_of(false);
    let l = local.handle();
    let mut ui = compose_test(move || {
        let explicit = explicit_indication.clone();
        CompositionLocalProvider(LOCAL_INDICATION.provides(local_indication.clone()), move || {
            let options = ClickableOptions {
                indication: if l.get() { Indication::LOCAL } else { explicit.clone() },
                ..ClickableOptions::DEFAULT
            };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}).test_tag(TAG));
        });
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    touch_up(&mut ui, at);
    assert_eq!((explicit.created(), local_seen.created()), (1, 0));
    local.set(true);
    ui.wait_for_idle();
    assert_eq!(local_seen.created(), 1, "at once");
    assert_eq!(explicit.detached.get(), 1);
}

/// Reused in the pass an update changed its indication: the node isn't made for what the reuse
/// throws away at once.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onReset
#[test]
fn reused_as_it_turns_local_with_a_source_its_node_is_made_once() {
    let (seen, indication) = test_indication();
    let (flag, source) = (mutable_state_of(false), MutableInteractionSource::new());
    let f = flag.handle();
    let mut ui = compose_test(move || {
        let (indication, source) = (indication.clone(), source.clone());
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication), move || {
            let source = source.clone();
            ReusableContent(f.get(), move || {
                let options = ClickableOptions {
                    indication: if f.get() { Indication::LOCAL } else { Indication::NONE },
                    interaction_source: Some(source.clone()),
                    ..ClickableOptions::DEFAULT
                };
                Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}));
            });
        });
    });
    assert_eq!(seen.created(), 0);
    flag.set(true);
    ui.wait_for_idle();
    assert_eq!((seen.created(), seen.detached.get()), (1, 0));
}

#[test]
fn reused_with_a_new_source_its_node_is_made_once() {
    let (seen, indication) = test_indication();
    let flag = mutable_state_of(false);
    let (first, second) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let f = flag.handle();
    let mut ui = compose_test(move || {
        let (indication, first, second) = (indication.clone(), first.clone(), second.clone());
        ReusableContent(f.get(), move || {
            let options = ClickableOptions {
                indication: indication.clone(),
                interaction_source: Some(if f.get() { second.clone() } else { first.clone() }),
                ..ClickableOptions::DEFAULT
            };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}));
        });
    });
    assert_eq!(seen.created(), 1);
    flag.set(true);
    ui.wait_for_idle();
    assert_eq!((seen.created(), seen.detached.get()), (2, 1));
}

/// A new source: the old node detached, the new one made, then attached, as Compose's `update`
/// undelegates and makes the next.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, recreateIndicationIfNeeded
#[test]
fn a_replaced_indication_goes_before_the_next_is_made() {
    struct Node(Rc<RefCell<Vec<&'static str>>>);
    impl ModifierNode for Node {
        fn on_attach(&mut self, _: &mut compose_ui::NodeContext<'_>) {
            self.0.borrow_mut().push("attach");
        }
        fn on_detach(&mut self, _: &mut compose_ui::NodeContext<'_>) {
            self.0.borrow_mut().push("detach");
        }
    }
    struct Factory(Rc<RefCell<Vec<&'static str>>>);
    impl IndicationNodeFactory for Factory {
        fn create(&self, _: InteractionSource) -> Box<dyn ModifierNode> {
            self.0.borrow_mut().push("create");
            Box::new(Node(self.0.clone()))
        }
        fn same(&self, _: &dyn IndicationNodeFactory) -> bool {
            true
        }
    }
    let log = Rc::new(RefCell::new(Vec::new()));
    let flag = mutable_state_of(false);
    let (first, second) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let (f, l) = (flag.handle(), log.clone());
    let indication = Indication::new(Factory(l));
    let mut ui = compose_test(move || {
        let options = ClickableOptions {
            indication: indication.clone(),
            interaction_source: Some(if f.get() { second.clone() } else { first.clone() }),
            ..ClickableOptions::DEFAULT
        };
        Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}));
    });
    assert_eq!(*log.borrow(), ["create", "attach"]);
    log.borrow_mut().clear();
    flag.set(true);
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["detach", "create", "attach"]);
}

/// With a source given, `LOCAL_INDICATION`'s node is made at once.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_interactionSource_eagerlyCreated
#[test]
fn with_a_source_the_local_indication_is_made_at_once() {
    let (seen, indication) = test_indication();
    let source = MutableInteractionSource::new();
    let _ui = compose_test(move || {
        let (indication, source) = (indication.clone(), source.clone());
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication), move || {
            let options = ClickableOptions {
                interaction_source: Some(source.clone()),
                ..ClickableOptions::DEFAULT
            };
            Box!(modifier = Modifier.size(10.dp()).clickable_with(options, || {}));
        });
    });
    assert_eq!(seen.created(), 1);
}

/// Turning to `Indication::LOCAL`, it reads the local then, as `updateCommon`'s
/// `onObservedReadsChanged`.
#[test]
fn a_clickable_turning_to_the_local_indication_shows_it() {
    let (seen, indication) = test_indication();
    let local = mutable_state_of(false);
    let l = local.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), move || {
            let options = ClickableOptions {
                indication: if l.get() { Indication::LOCAL } else { Indication::NONE },
                ..ClickableOptions::DEFAULT
            };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}).test_tag(TAG));
        });
    });
    local.set(true);
    ui.wait_for_idle();
    assert_eq!(seen.created(), 0, "without a source, at the first press");
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(seen.created(), 1);
    assert_eq!(seen_kinds(&seen), ["Press"]);
}

/// ... at once with a source given.
#[test]
fn a_clickable_turning_to_the_local_indication_shows_it_at_once_with_a_source() {
    let (seen, indication) = test_indication();
    let local = mutable_state_of(false);
    let l = local.handle();
    let source = MutableInteractionSource::new();
    let mut ui = compose_test(move || {
        let source = source.clone();
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), move || {
            let options = ClickableOptions {
                indication: if l.get() { Indication::LOCAL } else { Indication::NONE },
                interaction_source: Some(source.clone()),
                ..ClickableOptions::DEFAULT
            };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}));
        });
    });
    assert_eq!(seen.created(), 0);
    local.set(true);
    ui.wait_for_idle();
    assert_eq!(seen.created(), 1);
}

/// Local, then explicit, then local: what it read before isn't kept across.
#[test]
fn a_clickable_back_to_the_local_indication_reads_it_again() {
    let (first, first_indication) = test_indication();
    let (second, second_indication) = test_indication();
    let (explicit, explicit_indication) = test_indication();
    let which = mutable_state_of(0);
    let w = which.handle();
    let source = MutableInteractionSource::new();
    let mut ui = compose_test(move || {
        let local = if w.get() == 2 { second_indication.clone() } else { first_indication.clone() };
        let explicit = explicit_indication.clone();
        let source = source.clone();
        CompositionLocalProvider(LOCAL_INDICATION.provides(local), move || {
            let options = ClickableOptions {
                indication: if w.get() == 1 { explicit.clone() } else { Indication::LOCAL },
                interaction_source: Some(source.clone()),
                ..ClickableOptions::DEFAULT
            };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}));
        });
    });
    assert_eq!(first.created(), 1);
    which.set(1);
    ui.wait_for_idle();
    assert_eq!(explicit.created(), 1);
    which.set(2);
    ui.wait_for_idle();
    assert_eq!(second.created(), 1);
    assert_eq!(first.created(), 1);
}

/// With no source, the local indication's node is made at the first press, with a source of
/// the clickable's own, and sees that press.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_noInteractionSource_lazilyCreated_pointerInput
#[test]
fn without_a_source_the_local_indication_is_made_at_the_first_press() {
    let (seen, indication) = test_indication();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            Box!(modifier = Modifier.padding(10.dp()), {
                Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag(TAG));
            });
        });
    });
    assert_eq!(seen.created(), 0);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(seen.created(), 1);
    assert_eq!(seen_kinds(&seen), ["Press"]);
}

/// ... or at its focus.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_noInteractionSource_lazilyCreated_focus
#[test]
fn without_a_source_the_local_indication_is_made_at_the_focus() {
    let (seen, indication) = test_indication();
    let requester = FocusRequester::new();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            Box!(modifier = Modifier.size(20.dp()).focus_requester(requester).clickable(|| {}));
        });
    });
    assert_eq!(seen.created(), 0);
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert_eq!(seen.created(), 1);
    assert_eq!(seen_kinds(&seen), ["Focus"]);
}

/// ... or at a key a focused child didn't take.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_noInteractionSource_lazilyCreated_keyInput
#[test]
fn without_a_source_the_local_indication_is_made_at_a_key_from_a_child() {
    let (seen, indication) = test_indication();
    let requester = FocusRequester::new();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            Box!(modifier = Modifier.padding(10.dp()).focusable(), {
                Box!(modifier = Modifier.clickable(|| {}), {
                    Box!(modifier = Modifier.size(10.dp()).focus_requester(requester).focusable());
                });
            });
        });
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert_eq!(seen.created(), 0, "the child is focused, not the clickable");
    ui.host().send_key(compose_ui::KeyEvent {
        key: Key::Enter,
        kind: compose_ui::KeyEventType::KeyDown,
        modifiers: Default::default(),
    });
    ui.wait_for_idle();
    assert_eq!(seen.created(), 1);
    assert_eq!(seen_kinds(&seen), ["Press"]);
}

/// A new local indication before the node is made: nothing is made until the press, then the
/// new one.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_changingIndication_beforeCreation
#[test]
fn a_new_local_indication_before_the_node_is_made_is_made_lazily() {
    let (first_seen, first) = test_indication();
    let (second_seen, second) = test_indication();
    let indication = mutable_state_of(first);
    let handle = indication.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(handle.get()), || {
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag(TAG));
        });
    });
    indication.set(second);
    ui.wait_for_idle();
    assert_eq!((first_seen.created(), second_seen.created()), (0, 0));
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!((first_seen.created(), second_seen.created()), (0, 1));
    assert_eq!(seen_kinds(&second_seen), ["Press"]);
}

/// A new local indication once the node is made: the old node goes, the new one is made at once.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_changingIndication_afterCreation
#[test]
fn a_new_local_indication_after_the_node_is_made_replaces_it_at_once() {
    let (first_seen, first) = test_indication();
    let (second_seen, second) = test_indication();
    let indication = mutable_state_of(first);
    let handle = indication.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(handle.get()), || {
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag(TAG));
        });
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(first_seen.created(), 1);
    touch_up(&mut ui, at);
    indication.set(second);
    ui.wait_for_idle();
    assert_eq!(second_seen.created(), 1);
    assert_eq!(first_seen.detached.get(), 1);
    touch_down(&mut ui, at);
    assert_eq!(seen_kinds(&second_seen), ["Press"]);
    assert_eq!(seen_kinds(&first_seen), ["Press", "Release"], "the old node collects no more");
}

/// A new provider of it the node's observation can't see, a function `provides_computed` gives:
/// the content composes again, the node stays and keeps what it read, as Compose's, whose
/// `onObservedReadsChanged` only an observed state calls and whose `compositionLocalMap` setter
/// only auto-invalidates (D1184). The local's new value through a state reaches it, above.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, onObservedReadsChanged
#[test]
fn a_new_computed_local_indication_doesn_t_reach_a_kept_clickable() {
    let (first_seen, first) = test_indication();
    let (second_seen, second) = test_indication();
    let which = mutable_state_of(false);
    let handle = which.handle();
    let mut ui = compose_test(move || {
        let chosen = if handle.get() { second.clone() } else { first.clone() };
        // A new function each time: the provider's content composes again.
        let provided = LOCAL_INDICATION.provides_computed(move |_| chosen.clone());
        CompositionLocalProvider(provided, || {
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag(TAG));
        });
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    touch_up(&mut ui, at);
    assert_eq!((first_seen.created(), second_seen.created()), (1, 0));
    which.set(true);
    ui.wait_for_idle();
    touch_down(&mut ui, at);
    assert_eq!((first_seen.created(), second_seen.created()), (1, 0));
    assert_eq!(seen_kinds(&first_seen), ["Press", "Release", "Press"]);
}

/// Its node is a delegate of the clickable: its own observation, its own handle, told when what
/// it reads changes.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, initializeIndicationAndInteractionSourceIfNeeded
#[test]
fn an_indication_s_node_is_a_delegate_with_its_own_observation() {
    struct Observes(Rc<Cell<u32>>, MutableState<i32>);
    impl ModifierNode for Observes {
        fn as_observer(&mut self) -> Option<&mut dyn compose_ui::ObserverModifierNode> {
            Some(self)
        }
        fn on_attach(&mut self, context: &mut compose_ui::NodeContext<'_>) {
            let read = self.1;
            context.observe_reads(move || read.get());
        }
    }
    impl compose_ui::ObserverModifierNode for Observes {
        fn on_observed_reads_changed(&mut self, _: &mut compose_ui::NodeContext<'_>) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct Factory(Rc<Cell<u32>>, MutableState<i32>);
    impl IndicationNodeFactory for Factory {
        fn create(&self, _: InteractionSource) -> Box<dyn ModifierNode> {
            Box::new(Observes(self.0.clone(), self.1))
        }
        fn same(&self, _: &dyn IndicationNodeFactory) -> bool {
            true
        }
    }
    let told = Rc::new(Cell::new(0));
    let state = mutable_state_of(0);
    let source = MutableInteractionSource::new();
    let (t, s) = (told.clone(), state.handle());
    let mut ui = compose_test(move || {
        let options = ClickableOptions {
            indication: Indication::new(Factory(t.clone(), s)),
            interaction_source: Some(source.clone()),
            ..ClickableOptions::DEFAULT
        };
        Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}));
    });
    state.set(1);
    ui.wait_for_idle();
    assert_eq!(told.get(), 1);
}

/// A clickable modifier made outside the provider (outside composition, or before the
/// provider in it) and used inside it shows the provider's indication: the local is read where
/// the clickable's node is, as Compose's node reads `LocalIndication` (`useLocalIndication`,
/// D1056).
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, ClickableNode.onObservedReadsChanged (currentValueOf(LocalIndication))
#[test]
fn a_clickable_made_outside_the_provider_shows_the_providers_indication() {
    let (seen, indication) = test_indication();
    let made_outside = Modifier.size(20.dp()).clickable(|| {}).test_tag("outside");
    let mut ui = compose_test(move || {
        let made_before = Modifier.size(20.dp()).clickable(|| {}).test_tag("before");
        let made_outside = made_outside.clone();
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), move || {
            Column!({
                Box!(modifier = made_outside.clone());
                Box!(modifier = made_before.clone());
            });
        });
    });
    for (made, tag) in [(1, "outside"), (2, "before")] {
        let at = center(&mut ui, tag);
        touch_down(&mut ui, at);
        touch_up(&mut ui, at);
        assert_eq!(seen.created(), made, "{tag}");
    }
    assert_eq!(seen_kinds(&seen), ["Press", "Release", "Press", "Release"]);
}

/// A new source: the node goes and a new one is made for it.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_changingInteractionSourceToAnotherInteractionSource
#[test]
fn a_new_source_makes_the_node_again() {
    let (seen, indication) = test_indication();
    let source = mutable_state_of(MutableInteractionSource::new());
    let handle = source.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            let options = ClickableOptions {
                interaction_source: Some(handle.get()),
                ..ClickableOptions::DEFAULT
            };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}).test_tag(TAG));
        });
    });
    assert_eq!(seen.created(), 1);
    let new = MutableInteractionSource::new();
    source.set(new.clone());
    ui.wait_for_idle();
    assert_eq!((seen.created(), seen.detached.get()), (2, 1));
    assert!(seen.sources.borrow()[1] == *new.as_source());
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(seen_kinds(&seen), ["Press"]);
}

/// A source given where there was none: the node is made at once.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_changingInteractionSourceFromNull
#[test]
fn a_source_given_where_there_was_none_makes_the_node_at_once() {
    let (seen, indication) = test_indication();
    let source: OwnedState<Option<MutableInteractionSource>> = mutable_state_of(None);
    let handle = source.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            let options =
                ClickableOptions { interaction_source: handle.get(), ..ClickableOptions::DEFAULT };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}).test_tag(TAG));
        });
    });
    assert_eq!(seen.created(), 0);
    source.set(Some(MutableInteractionSource::new()));
    ui.wait_for_idle();
    assert_eq!(seen.created(), 1);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(seen_kinds(&seen), ["Press"]);
}

/// The source taken away: the node is made again at once, with the clickable's own source.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_changingInteractionSourceToNull
#[test]
fn the_source_taken_away_makes_the_node_again_with_its_own() {
    let (seen, indication) = test_indication();
    let given = MutableInteractionSource::new();
    let source = mutable_state_of(Some(given.clone()));
    let handle = source.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            let options =
                ClickableOptions { interaction_source: handle.get(), ..ClickableOptions::DEFAULT };
            Box!(modifier = Modifier.size(20.dp()).clickable_with(options, || {}).test_tag(TAG));
        });
    });
    assert_eq!(seen.created(), 1);
    source.set(None);
    ui.wait_for_idle();
    assert_eq!((seen.created(), seen.detached.get()), (2, 1));
    assert!(seen.sources.borrow()[1] != *given.as_source());
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(seen_kinds(&seen), ["Press"]);
}

/// Reused mid-press, the node and the clickable's own source go; the next press makes new ones.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_nullInteractionSource_resetWhenReused_pressed
#[test]
fn reused_mid_press_the_node_and_its_own_source_are_made_again_at_the_next_press() {
    let (seen, indication) = test_indication();
    let key = mutable_state_of(true);
    let handle = key.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            Box!({
                ReusableContent(handle.get(), || {
                    Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag(TAG));
                });
            });
        });
    });
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(seen.created(), 1);
    assert_eq!(seen_kinds(&seen), ["Press"]);
    seen.interactions.borrow_mut().clear();
    key.set(false);
    ui.wait_for_idle();
    assert_eq!((seen.created(), seen.detached.get()), (1, 1));
    touch_up(&mut ui, at);
    touch_down(&mut ui, at);
    assert_eq!((seen.created(), seen.detached.get()), (2, 1));
    let sources = seen.sources.borrow();
    assert!(sources[0] != sources[1], "a new source of its own");
    assert_eq!(seen_kinds(&seen), ["Press"]);
}

/// Reused while focused, the same: the next focus makes new ones.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, localIndication_nullInteractionSource_resetWhenReused_focused
#[test]
fn reused_while_focused_the_node_and_its_own_source_are_made_again_at_the_next_focus() {
    let (seen, indication) = test_indication();
    let key = mutable_state_of(true);
    let requester = FocusRequester::new();
    let handle = key.handle();
    let mut ui = compose_test(move || {
        CompositionLocalProvider(LOCAL_INDICATION.provides(indication.clone()), || {
            Box!({
                ReusableContent(handle.get(), || {
                    Box!(
                        modifier =
                            Modifier.size(20.dp()).focus_requester(requester).clickable(|| {})
                    );
                });
            });
        });
    });
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert_eq!(seen.created(), 1);
    assert_eq!(seen_kinds(&seen), ["Focus"]);
    seen.interactions.borrow_mut().clear();
    key.set(false);
    ui.wait_for_idle();
    assert_eq!((seen.created(), seen.detached.get()), (1, 1));
    assert!(requester.request_focus());
    ui.wait_for_idle();
    assert_eq!((seen.created(), seen.detached.get()), (2, 1));
    let sources = seen.sources.borrow();
    assert!(sources[0] != sources[1]);
    assert_eq!(seen_kinds(&seen), ["Focus"]);
}

/// With a factory given, the same rules as the local one: made at once with a source, at the
/// first press without.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, indicationNodeFactory_interactionSource_eagerlyCreated
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, indicationNodeFactory_noInteractionSource_lazilyCreated_pointerInput
#[test]
fn a_factory_given_is_made_as_the_local_one() {
    let (eager, indication) = test_indication();
    let source = MutableInteractionSource::new();
    let _ui = compose_test(move || {
        let options = ClickableOptions {
            interaction_source: Some(source.clone()),
            indication: indication.clone(),
            ..ClickableOptions::DEFAULT
        };
        Box!(modifier = Modifier.size(10.dp()).clickable_with(options, || {}));
    });
    assert_eq!(eager.created(), 1);
    let (lazy, indication) = test_indication();
    let mut ui = compose_test(move || {
        let options =
            ClickableOptions { indication: indication.clone(), ..ClickableOptions::DEFAULT };
        Box!(modifier = Modifier.size(10.dp()).clickable_with(options, || {}).test_tag(TAG));
    });
    assert_eq!(lazy.created(), 0);
    let at = center(&mut ui, TAG);
    touch_down(&mut ui, at);
    assert_eq!(lazy.created(), 1);
    assert_eq!(seen_kinds(&lazy), ["Press"]);
}

/// The default indication, Compose's debug one: black at 30 % pressed.
#[test]
fn the_default_indication_darkens_a_press() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag(TAG));
    });
    let at = center(&mut ui, TAG);
    assert!(!ui.dump_display_list().contains("#4D000000"));
    touch_down(&mut ui, at);
    assert!(ui.dump_display_list().contains("#4D000000"), "{}", ui.dump_display_list());
    touch_up(&mut ui, at);
    assert!(!ui.dump_display_list().contains("#4D000000"));
    mouse_move(&mut ui, at);
    assert!(ui.dump_display_list().contains("#1A000000"), "hovered: {}", ui.dump_display_list());
}

// ---- Equality ----------------------------------------------------------------------------------

fn clickable_modifier(options: ClickableOptions, on_click: &Callback<dyn Fn()>) -> Modifier {
    Modifier.clickable_with(options, on_click.clone())
}

/// The same arguments make an equal modifier (its node isn't updated); another `enabled`
/// doesn't, whatever the source and the indication.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, nullInteractionSource_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, nonNullInteractionSource_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, nullInteractionSourceNullIndication_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, nonNullInteractionSourceNullIndication_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, nullInteractionSourceNonNullIndicationNodeFactory_equality
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, nonNullInteractionSourceNonNullIndicationNodeFactory_equality
#[test]
fn equal_arguments_make_equal_clickables() {
    let on_click: Callback<dyn Fn()> = callback(|| {});
    let source = MutableInteractionSource::new();
    let (_, factory) = test_indication();
    for interaction_source in [None, Some(source.clone())] {
        for indication in [Indication::LOCAL, Indication::NONE, factory.clone()] {
            let with = |enabled| ClickableOptions {
                enabled,
                interaction_source: interaction_source.clone(),
                indication: indication.clone(),
                ..ClickableOptions::DEFAULT
            };
            let same = clickable_modifier(with(true), &on_click)
                .same(&clickable_modifier(with(true), &on_click));
            let toggled = clickable_modifier(with(true), &on_click)
                .same(&clickable_modifier(with(false), &on_click));
            assert!(same && !toggled, "{interaction_source:?} {indication:?}");
        }
    }
}

// ---- Focus through semantics -------------------------------------------------------------------

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, focusUsingSemanticAction
#[test]
fn the_request_focus_action_focuses_the_clickable() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.size(10.dp()).test_tag(TAG).clickable(|| {}));
    });
    ui.on_node_with_tag(TAG).request_focus();
    ui.on_node_with_tag(TAG).assert_is_focused();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, focusUsingSemanticActionWhenNotEnabled
#[test]
fn a_disabled_clickable_cannot_be_focused() {
    let mut ui = compose_test(|| {
        let options = ClickableOptions { enabled: false, ..ClickableOptions::DEFAULT };
        Box!(modifier = Modifier.size(10.dp()).test_tag(TAG).clickable_with(options, || {}));
    });
    let node = ui.on_node_with_tag(TAG).node_id();
    assert!(
        !ui.host().perform_semantics_action(node, &compose_ui::SemanticsActions::REQUEST_FOCUS)
    );
    assert!(!ui.on_node_with_tag(TAG).fetch_semantics_node().config.is_focused());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, focusUsingSemanticActionWhileChangingEnabledParam
#[test]
fn focus_follows_enabled() {
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut ui = compose_test(move || {
        let options = ClickableOptions { enabled: handle.get(), ..ClickableOptions::DEFAULT };
        Box!(modifier = Modifier.size(10.dp()).test_tag(TAG).clickable_with(options, || {}));
    });
    ui.on_node_with_tag(TAG).request_focus();
    ui.on_node_with_tag(TAG).assert_is_focused();
    enabled.set(false);
    ui.host().with_tree(|tree| tree.clear_focus());
    ui.wait_for_idle();
    let node = ui.on_node_with_tag(TAG).node_id();
    assert!(
        !ui.host().perform_semantics_action(node, &compose_ui::SemanticsActions::REQUEST_FOCUS)
    );
    assert!(!ui.on_node_with_tag(TAG).fetch_semantics_node().config.is_focused());
    enabled.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag(TAG).request_focus();
    ui.on_node_with_tag(TAG).assert_is_focused();
}

// ---- In a scroll -------------------------------------------------------------------------------

/// A click in a scrolled container clicks, by touch and by mouse.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, clickableInScrollContainerWithMouse
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ClickableTest.kt, clickableInScrollContainerWithTouch
#[test]
fn a_clickable_in_a_scroll_clicks_by_touch_and_by_mouse() {
    let counter = Rc::new(Cell::new(0));
    let count = counter.clone();
    let mut ui = compose_test(move || {
        let count = count.clone();
        let state = remember_scroll_state(0);
        Column!(modifier = Modifier.size(100.dp()).vertical_scroll(state), {
            let count = count.clone();
            Box!(
                modifier = Modifier
                    .size((100.dp(), 50.dp()))
                    .clickable(move || count.set(count.get() + 1))
                    .test_tag(TAG)
            );
            Box!(modifier = Modifier.size((100.dp(), 200.dp())));
        });
    });
    let at = center(&mut ui, TAG);
    mouse_press(&mut ui, at);
    mouse_release(&mut ui, at);
    assert_eq!(counter.get(), 1);
    touch_click(&mut ui, at);
    assert_eq!(counter.get(), 2);
}

// ---- The extended touch bounds -----------------------------------------------------------------

/// A press on a clickable smaller than the minimum touch target (48 dp) may stray that far
/// around it before it's cancelled, as Compose's `isOutOfBounds(size, extendedTouchPadding)`:
/// a 20 dp clickable keeps a press 10 dp out (within its 14 dp of padding) and clicks, and
/// cancels one 20 dp out.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, getExtendedTouchPadding
#[test]
fn a_press_strays_within_the_minimum_touch_target_before_it_is_cancelled() {
    let clicks = Rc::new(Cell::new(0));
    let source = MutableInteractionSource::new();
    let (count, given) = (clicks.clone(), source.clone());
    let mut ui = compose_test(move || {
        let (count, given) = (count.clone(), given.clone());
        Box!(modifier = Modifier.padding(50.dp()), {
            let count = count.clone();
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .clickable_with(options(&given), move || count.set(count.get() + 1))
                    .test_tag(TAG)
            );
        });
    });
    let recorded = record(&source);
    let density = ui.density().density;
    let at = center(&mut ui, TAG);
    let bounds = bounds(&mut ui, TAG);
    // 10 dp to the right of the clickable: inside the extended bounds.
    let near = Offset::new(bounds.right + 10.0 * density, at.y);
    touch_down(&mut ui, at);
    touch_move(&mut ui, near);
    touch_up(&mut ui, near);
    assert_eq!(clicks.get(), 1);
    assert_eq!(kinds(&recorded), ["Press", "Release"]);
    // 20 dp to the right: past them, cancelled.
    let far = Offset::new(bounds.right + 20.0 * density, at.y);
    touch_down(&mut ui, at);
    touch_move(&mut ui, far);
    touch_up(&mut ui, far);
    assert_eq!(clicks.get(), 1);
    assert_eq!(kinds(&recorded), ["Press", "Release", "Press", "Cancel"]);
}

/// The minimum touch target is the view configuration's `minimum_touch_target_size`, each side
/// its own: with 100 by 20 dp, a 20 dp clickable keeps a press 30 dp to its right, and cancels
/// one 10 dp below it.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/Clickable.kt, getExtendedTouchPadding
#[test]
fn the_touch_target_is_the_view_configuration_s() {
    let clicks = Rc::new(Cell::new(0));
    let source = MutableInteractionSource::new();
    let (count, given) = (clicks.clone(), source.clone());
    let mut ui = compose_test(move || {
        let (count, given) = (count.clone(), given.clone());
        Box!(modifier = Modifier.padding(60.dp()), {
            let count = count.clone();
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .clickable_with(options(&given), move || count.set(count.get() + 1))
                    .test_tag(TAG)
            );
        });
    });
    ui.host().set_view_configuration(Some(compose_ui::ViewConfiguration {
        minimum_touch_target_size: compose_ui::DpSize::new(100.dp(), 20.dp()),
        ..compose_ui::ViewConfiguration::default()
    }));
    ui.wait_for_idle();
    let recorded = record(&source);
    let density = ui.density().density;
    let at = center(&mut ui, TAG);
    let bounds = bounds(&mut ui, TAG);
    let right = Offset::new(bounds.right + 30.0 * density, at.y);
    touch_down(&mut ui, at);
    touch_move(&mut ui, right);
    touch_up(&mut ui, right);
    assert_eq!(clicks.get(), 1, "within the target's width (Android's 48 dp would cancel)");
    let below = Offset::new(at.x, bounds.bottom + 10.0 * density);
    touch_down(&mut ui, at);
    touch_move(&mut ui, below);
    touch_up(&mut ui, below);
    assert_eq!(clicks.get(), 1, "past the target's height (Android's 48 dp would keep it)");
    assert_eq!(kinds(&recorded), ["Press", "Release", "Press", "Cancel"]);
}
