//! Twelfth review round: `FocusRequester`, Tab skipping out-of-view nodes, and interaction
//! sources, against Jetpack Compose. Each test states the input, what Compose does, and what
//! happens instead.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{mutable_state_of, remember};
use compose_test::*;
use compose_ui::ClickableOptions;
use compose_ui::{
    FocusRequester, IntSize, Key, MutableInteractionSource, Offset, PointerEvent, UnitExt,
};

/// Input: one remembered `FocusRequester`, attached to box "b" (last in a column) while
/// `late` is true and, once it's false, to box "a" (first in the column, a node composed in the
/// same recomposition that removes "b"); then a click on a button calls `request_focus()`.
///
/// Compose: the requester follows its modifier: the recomposition adds "a"'s
/// `FocusRequesterModifierNode` and removes "b"'s, so the request focuses "a".
///
/// Here: "a" is inserted first and attaches the requester, then the removal of "b" runs its
/// node's `on_detach`, which detaches the same requester, so it's attached to nothing:
/// `request_focus()` does nothing. Moving the focus target up the screen (an "edit" row
/// replacing a read-only one above it) loses the requester.
#[test]
fn a_requester_moved_to_an_earlier_node_still_focuses() {
    let late = mutable_state_of(true);
    let handle = late.handle();
    let mut ui = compose_test(move || {
        let requester = remember(FocusRequester::new);
        let target = requester;
        Column!({
            if !handle.get() {
                Box!(
                    modifier =
                        Modifier.size(20.dp()).focus_requester(requester).focusable().test_tag("a")
                );
            }
            Box!(
                modifier = Modifier.size(20.dp()).test_tag("button").clickable(move || {
                    target.request_focus();
                })
            );
            if handle.get() {
                Box!(
                    modifier =
                        Modifier.size(20.dp()).focus_requester(requester).focusable().test_tag("b")
                );
            }
        });
    });
    late.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("button").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_tag("a").assert_is_focused();
}

/// Input: one `FocusRequester` on two boxes, "a" and "b"; "a" is then removed and the
/// requester asked to focus.
///
/// Compose: a requester keeps a list of its nodes (`focusRequesterNodes`) and focuses the first
/// one that takes it, so with "a" gone it focuses "b".
///
/// Here: the requester remembers only the last node whose chain was refreshed, and removing
/// "a" clears it altogether: `request_focus()` returns false and nothing is focused.
#[test]
fn a_requester_on_two_nodes_survives_removing_one() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let requester = FocusRequester::new();
    let attached = requester;
    let mut ui = compose_test(move || {
        Column!({
            if handle.get() {
                Box!(
                    modifier =
                        Modifier.size(20.dp()).focus_requester(attached).focusable().test_tag("a")
                );
            }
            Box!(
                modifier =
                    Modifier.size(20.dp()).focus_requester(attached).focusable().test_tag("b")
            );
        });
    });
    shown.set(false);
    ui.wait_for_idle();
    assert!(requester.request_focus(), "the requester is still attached to b");
    ui.wait_for_idle();
    ui.on_node_with_tag("b").assert_is_focused();
}

/// Input: a column of a clickable "before", a 20 dp box holding a box moved out of it by
/// `Modifier.offset(30.dp, 0.dp)` around a 20 dp clickable "badge" (drawn and clickable to the
/// 20 dp box's right: nothing clips it), and a clickable "after". "before" is focused and Tab pressed.
///
/// Compose: the one-dimensional search goes to the next placed focus target in order: the
/// badge, placed (outside its parent's bounds, which don't matter).
///
/// Here: Tab steps over nodes out of any ancestor's bounds unless the focus is inside that
/// ancestor, meant for a lazy list's items beyond its viewport; the badge is out of the bounds
/// of its offset parent (whose bounds are taken before the offset) and the focus can never be
/// inside that parent, so Tab goes to "after" and the badge is
/// never reachable from the keyboard.
#[test]
fn tab_reaches_a_clickable_offset_out_of_its_parent() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        Column!({
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag("before"));
            Box!(modifier = Modifier.size(20.dp()), {
                Box!(modifier = Modifier.offset(30.dp(), 0.dp()), {
                    Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag("badge"));
                });
            });
            Box!(modifier = Modifier.size(20.dp()).clickable(|| {}).test_tag("after"));
        });
    });
    ui.on_node_with_tag("before").request_focus().assert_is_focused();
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("badge").assert_is_focused();
}

/// Input: a clickable box with a `MutableInteractionSource`, focused, then disabled by a
/// recomposition (`enabled` read from a state that's set to false).
///
/// Compose: the disabled clickable can't keep the focus; it loses it and its interaction
/// source gets the `FocusInteraction.Unfocus`, so `collectIsFocusedAsState` turns false.
///
/// Here: the tree applies the recomposition's new modifier, finds the focus target can't take
/// focus and tells it so; the clickable publishes its press and focus to the interaction
/// source at once (`publish_press`, unlike `release_later` on removal), writing a state while
/// composition applies its changes: debug builds panic ("state written ... during
/// composition"). A NavigationBarItem or any `clickable_with` disabled while focused
/// (a "Submit" button disabled once clicked from the keyboard) crashes.
#[test]
fn disabling_a_focused_clickable_with_a_source_unfocuses_it() {
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let mut ui = compose_test(move || {
        Box!(
            modifier = Modifier.size(20.dp()).test_tag("box").clickable_with(
                ClickableOptions {
                    enabled: handle.get(),
                    indication: Color::RED.into(),
                    interaction_source: Some(given.clone()),
                    ..ClickableOptions::DEFAULT
                },
                || {}
            )
        );
    });
    ui.on_node_with_tag("box").request_focus().assert_is_focused();
    ui.wait_for_idle();
    assert!(source.is_focused());
    enabled.set(false);
    ui.wait_for_idle();
    assert!(!source.is_focused());
}

/// Groups digits by four with a space, as a card number field's `VisualTransformation`.
struct CardGrouping;

struct CardMapping;

impl OffsetMapping for CardMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        if offset <= 4 { offset } else { offset + 1 }
    }
    fn transformed_to_original(&self, offset: usize) -> usize {
        if offset <= 4 { offset } else { offset - 1 }
    }
}

impl VisualTransform for CardGrouping {
    fn filter(&self, text: &str) -> TransformedText {
        let shown = if text.len() > 4 {
            format!("{} {}", &text[..4], &text[4..])
        } else {
            text.to_string()
        };
        TransformedText { text: shown, offset_mapping: std::rc::Rc::new(CardMapping) }
    }
}

/// Input: a single-line field holding "12345678", shown "1234 5678" by a visual transformation
/// inserting a space after four digits (with its `OffsetMapping`). A double-click on the "6"
/// (fixed-advance measurer: 7 px per character), then "X" typed.
///
/// Compose: the double-click's `SelectionAdjustment.Word` takes the word boundary from the
/// text layout, which is the transformed text's ("5678", transformed 5..9), and maps it back
/// with `transformedToOriginal`: original 4..8 is selected, and "X" makes "1234X".
///
/// Here: the tap's offset is mapped back, but the word is then looked up in the original text
/// ("12345678", one word), so the whole number is selected and "X" makes "X". (Ctrl+Left and
/// Right step by the original text's words too.)
#[test]
fn a_double_click_selects_a_word_of_the_transformed_text() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), || {
        let text = remember(|| mutable_state_of(String::from("12345678")));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            single_line = true,
            visual_transformation = VisualTransformation::new(CardGrouping)
        );
    });
    ui.on_node_with_tag("field").request_focus();
    // The "6" is the transformed text's seventh character: x = 42..49.
    for _ in 0..2 {
        ui.host().send_pointer(PointerEvent::press(Offset::new(45.0, 8.0)));
        ui.host().send_pointer(PointerEvent::release(Offset::new(45.0, 8.0)));
    }
    ui.wait_for_idle();
    ui.type_text("X");
    let node = ui.on_node_with_tag("field").fetch_semantics_node();
    assert_eq!(
        node.config.get(&compose_ui::SemanticsProperties::EDITABLE_TEXT).cloned().as_deref(),
        Some("1234 X")
    );
}
