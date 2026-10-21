//! Third phase F review: failing tests for compose-material3's `TextField` (docs/06-ui.md).
//! Each test states the input, what Jetpack Compose's material3 (Desktop) does, and what
//! happens instead.
//!
//! Checked and sound (no test kept): `TextFieldLayout` against Compose's
//! `TextFieldMeasurePolicy` for the paddings (16 dp at the sides; 8 dp above and below with a
//! label, 16 dp without), the 280 x 56 dp minimum, the label at the top padding with the text
//! under it, a single line centered without a label, a multi-line text at the top padding, and
//! the placeholder at the text's place.

#![allow(non_snake_case)]

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Modifier, Offset, PointerEvent};

/// The bounds of the field's inner text (its focus target), in the root; leaves it focused.
fn focus_inner_text(ui: &mut ComposeTestRule) -> compose_ui::Rect {
    ui.on_node(Matcher::new("editable", |config| config.editable)).request_focus();
    ui.host().with_tree(|tree| tree.bounds_in_root(tree.focused().expect("focused")))
}

#[composable]
fn Field() {
    let text = remember(|| mutable_state_of(String::from("hello")));
    MaterialTheme!({
        TextField!(
            value = text.get(),
            on_value_change = move |value| text.set(value),
            modifier = Modifier.test_tag("field"),
            single_line = true,
        );
    });
}

/// Events arrive in batches between frames on the desktop. A tap in the container's padding
/// puts the cursor at the nearest offset (D102), and it does so right after typing too in
/// Compose (the tap is handled against the layout of the text being shown). Typing " world"
/// at the end of "hello", then (before the next frame) tapping in the left padding, then
/// typing "!" gives "!hello world". Here the typing dropped the paragraph, and the
/// decoration's tap (`on_focus_tap`) is ignored when there is none: unlike a tap on the text
/// itself (resolved at the next measurement, `pending_tap`), it is lost, and the text becomes
/// "hello world!".
#[test]
fn a_tap_in_the_padding_right_after_typing_moves_the_cursor() {
    let mut ui = compose_test(Field);
    let input = focus_inner_text(&mut ui);
    ui.wait_for_idle();
    ui.press_key(compose_ui::Key::MoveEnd);
    // " world", then a tap left of the text, before a frame.
    ui.host().send_text(" world");
    let at = Offset::new(input.left - 8.0, (input.top + input.bottom) / 2.0);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
    ui.type_text("!");
    ui.on_node_with_tag("field").assert_text_equals(&["!hello world"]);
}

/// The control: with a frame between the typing and the tap, the tap moves the cursor.
#[test]
fn a_tap_in_the_padding_after_a_frame_moves_the_cursor() {
    let mut ui = compose_test(Field);
    let input = focus_inner_text(&mut ui);
    ui.wait_for_idle();
    ui.press_key(compose_ui::Key::MoveEnd);
    ui.host().send_text(" world");
    ui.wait_for_idle();
    let at = Offset::new(input.left - 8.0, (input.top + input.bottom) / 2.0);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
    ui.type_text("!");
    ui.on_node_with_tag("field").assert_text_equals(&["!hello world"]);
}
