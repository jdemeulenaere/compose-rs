//! What the ports of Compose's interaction tests share: recording a source's interactions (as
//! their `scope.launch { interactions.collect { interactions.add(it) } }`), and touch and
//! mouse input at a node (as `performTouchInput` and `performMouseInput`).
//!
//! These are the suite's own, written before compose-test's input injection landed
//! (`perform_touch_input`, D900–D905): the ports can move onto it.

use std::cell::RefCell;
use std::rc::Rc;

use compose_test::ComposeTestRule;
use compose_ui::{
    Interaction, InteractionSource, Offset, PointerEvent, PointerId, PointerType, Rect,
};
use compose_ui::{SemanticsActions, SemanticsProperties};

/// The interactions a source emitted since `record` started collecting them.
pub(crate) type Recorded = Rc<RefCell<Vec<Interaction>>>;

/// Collects `source`'s interactions from now on, as a test's collecting coroutine.
pub(crate) fn record(source: &InteractionSource) -> Recorded {
    let recorded = Recorded::default();
    let sink = recorded.clone();
    let interactions = source.interactions();
    compose_runtime::platform::launch(async move {
        interactions.collect(move |interaction| sink.borrow_mut().push(interaction)).await
    });
    recorded
}

/// The recorded interactions' kinds, in order: `"Press"`, `"Release"`, `"Cancel"`, `"Enter"`,
/// `"Exit"`, `"Focus"`, `"Unfocus"`, `"Start"`, `"Stop"`, `"DragCancel"`, `"Custom"`.
pub(crate) fn kinds(recorded: &Recorded) -> Vec<&'static str> {
    use compose_ui::{DragInteraction, FocusInteraction, HoverInteraction, PressInteraction};
    recorded
        .borrow()
        .iter()
        .map(|interaction| match interaction {
            Interaction::Press(PressInteraction::Press(_)) => "Press",
            Interaction::Press(PressInteraction::Release(_)) => "Release",
            Interaction::Press(PressInteraction::Cancel(_)) => "Cancel",
            Interaction::Hover(HoverInteraction::Enter(_)) => "Enter",
            Interaction::Hover(HoverInteraction::Exit(_)) => "Exit",
            Interaction::Focus(FocusInteraction::Focus(_)) => "Focus",
            Interaction::Focus(FocusInteraction::Unfocus(_)) => "Unfocus",
            Interaction::Drag(DragInteraction::Start(_)) => "Start",
            Interaction::Drag(DragInteraction::Stop(_)) => "Stop",
            Interaction::Drag(DragInteraction::Cancel(_)) => "DragCancel",
            Interaction::Custom(_) => "Custom",
        })
        .collect()
}

/// Whether the interaction at `end` ends the one at `start` (a release or cancellation of that
/// press, an exit of that hover, an unfocus of that focus).
pub(crate) fn ends(recorded: &Recorded, start: usize, end: usize) -> bool {
    use compose_ui::{FocusInteraction, HoverInteraction, PressInteraction};
    let recorded = recorded.borrow();
    match (&recorded[start], &recorded[end]) {
        (
            Interaction::Press(PressInteraction::Press(press)),
            Interaction::Press(PressInteraction::Release(ended) | PressInteraction::Cancel(ended)),
        ) => press == ended,
        (
            Interaction::Hover(HoverInteraction::Enter(enter)),
            Interaction::Hover(HoverInteraction::Exit(ended)),
        ) => enter == ended,
        (
            Interaction::Focus(FocusInteraction::Focus(focus)),
            Interaction::Focus(FocusInteraction::Unfocus(ended)),
        ) => focus == ended,
        _ => false,
    }
}

/// The bounds of the node tagged `tag`, in the root.
pub(crate) fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

/// The center of the node tagged `tag`.
pub(crate) fn center(ui: &mut ComposeTestRule, tag: &str) -> Offset {
    let bounds = bounds(ui, tag);
    Offset::new((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0)
}

const FINGER: PointerId = PointerId(7);

fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(FINGER, PointerType::Touch)
}

/// A finger down at `at`, as `down(at)`.
pub(crate) fn touch_down(ui: &mut ComposeTestRule, at: Offset) {
    ui.host().send_pointer(touch(PointerEvent::press(at)));
    ui.wait_for_idle();
}

/// The finger moved to `to`, as `moveTo(to)`.
pub(crate) fn touch_move(ui: &mut ComposeTestRule, to: Offset) {
    ui.host().send_pointer(touch(PointerEvent::moved(to, true)));
    ui.wait_for_idle();
}

/// The finger up at `at`, as `up()`.
pub(crate) fn touch_up(ui: &mut ComposeTestRule, at: Offset) {
    ui.host().send_pointer(touch(PointerEvent::release(at)));
    ui.wait_for_idle();
}

/// The touch cancelled by the system, as `cancel()`.
pub(crate) fn touch_cancel(ui: &mut ComposeTestRule) {
    ui.host().with_tree(|tree| tree.cancel_pointer(FINGER));
    ui.wait_for_idle();
}

/// A tap at `at`: down and up, as `click(at)`.
pub(crate) fn touch_click(ui: &mut ComposeTestRule, at: Offset) {
    touch_down(ui, at);
    touch_up(ui, at);
}

/// The mouse moved to `to` without a button, as `moveTo` (its hover entering what it's over).
pub(crate) fn mouse_move(ui: &mut ComposeTestRule, to: Offset) {
    ui.host().send_pointer(PointerEvent::moved(to, false));
    ui.wait_for_idle();
}

/// The mouse's primary button pressed at `at`, as `press()`.
pub(crate) fn mouse_press(ui: &mut ComposeTestRule, at: Offset) {
    ui.host().send_pointer(PointerEvent::press(at));
    ui.wait_for_idle();
}

/// The mouse's primary button released at `at`, as `release()`.
pub(crate) fn mouse_release(ui: &mut ComposeTestRule, at: Offset) {
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
}

/// The mouse left the window, as `exit()`.
pub(crate) fn mouse_exit(ui: &mut ComposeTestRule) {
    ui.host().send_pointer_left();
    ui.wait_for_idle();
}

/// The semantics the clickable ports read, from Compose's typed keys (D1250): what Compose's
/// matchers check (`hasClickAction`, `isEnabled`, `isFocused`, the role, the state).
pub(crate) trait SemanticsReads {
    fn is_clickable(&self) -> bool;
    fn is_long_clickable(&self) -> bool;
    fn is_disabled(&self) -> bool;
    fn role_of(&self) -> Option<compose_ui::Role>;
    /// Focused, as `assertIsFocused`.
    fn is_focused(&self) -> bool;
    /// Focusable, as `isFocusable` (its `Focused` key is defined).
    fn is_focusable(&self) -> bool;
    fn selected_of(&self) -> Option<bool>;
    fn toggleable_state_of(&self) -> Option<compose_ui::ToggleableState>;
    fn click_label(&self) -> Option<compose_ui::SharedString>;
    fn long_click_label(&self) -> Option<compose_ui::SharedString>;
    fn progress_of(&self) -> Option<compose_ui::ProgressBarRangeInfo>;
    fn content_descriptions(&self) -> Vec<String>;
}

impl SemanticsReads for compose_ui::SemanticsConfiguration {
    fn is_clickable(&self) -> bool {
        self.contains(&SemanticsActions::ON_CLICK)
    }
    fn is_long_clickable(&self) -> bool {
        self.contains(&SemanticsActions::ON_LONG_CLICK)
    }
    fn is_disabled(&self) -> bool {
        self.contains(&SemanticsProperties::DISABLED)
    }
    fn role_of(&self) -> Option<compose_ui::Role> {
        self.get(&SemanticsProperties::ROLE).copied()
    }
    fn is_focused(&self) -> bool {
        self.get(&SemanticsProperties::FOCUSED).copied().unwrap_or(false)
    }
    fn is_focusable(&self) -> bool {
        self.contains(&SemanticsProperties::FOCUSED)
    }
    fn selected_of(&self) -> Option<bool> {
        self.get(&SemanticsProperties::SELECTED).copied()
    }
    fn toggleable_state_of(&self) -> Option<compose_ui::ToggleableState> {
        self.get(&SemanticsProperties::TOGGLEABLE_STATE).copied()
    }
    fn click_label(&self) -> Option<compose_ui::SharedString> {
        self.get(&SemanticsActions::ON_CLICK).and_then(|action| action.label.clone())
    }
    fn long_click_label(&self) -> Option<compose_ui::SharedString> {
        self.get(&SemanticsActions::ON_LONG_CLICK).and_then(|action| action.label.clone())
    }
    fn progress_of(&self) -> Option<compose_ui::ProgressBarRangeInfo> {
        self.get(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO).copied()
    }
    fn content_descriptions(&self) -> Vec<String> {
        self.get(&SemanticsProperties::CONTENT_DESCRIPTION)
            .map(|list| list.iter().map(|text| text.to_string()).collect())
            .unwrap_or_default()
    }
}
