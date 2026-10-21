//! Third phase F review: failing tests for compose-foundation's text field (docs/06-ui.md,
//! "Keyboard and text"). Each test states the input, what Jetpack Compose (Desktop) does, and
//! what happens instead. They use the fixed-advance measurer at density 1: 7 px per character
//! and 17.5 px per line at the default 14 sp.
//!
//! Checked and sound (no test kept): 300 seeds of 300 random keys (with every modifier),
//! shortcuts, taps, drags, multibyte, emoji, combining and right-to-left insertions, preedits,
//! commits and focus moves over two fields (one filtering its value), on the Skia measurer,
//! panic nowhere once a composition is committed before the cursor moves (see
//! `compose-skia/tests/review_f3.rs` for the case that does panic).

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    DrawOp, IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent, UnitExt,
};

#[composable]
fn Editor(initial: &'static str, single_line: bool) {
    let text = remember(|| mutable_state_of(String::from(initial)));
    BasicTextField!(
        value = text.get(),
        on_value_change = move |value| text.set(value),
        modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("field"),
        single_line = single_line,
    );
}

fn editor(initial: &'static str, single_line: bool) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(300, 100), move || Editor(initial, single_line))
}

/// A key down and up with `modifiers` held.
fn chord(ui: &mut ComposeTestRule, key: Key, modifiers: KeyModifiers) {
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyDown, modifiers });
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyUp, modifiers });
    ui.wait_for_idle();
}

fn text_of(ui: &mut ComposeTestRule, tag: &str) -> String {
    let node = ui.on_node_with_tag(tag).fetch_semantics_node();
    node.config.editable_text.as_deref().unwrap_or_default().to_string()
}

/// The text of the paragraph the field draws.
fn drawn_text(ui: &mut ComposeTestRule) -> String {
    let list = ui.host().display_list();
    let handle = list
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawParagraph(handle, _) => Some(*handle),
            _ => None,
        })
        .expect("a paragraph");
    ui.host().with_tree(|tree| tree.text_measurer_ref().text(handle).to_string())
}

fn click(ui: &mut ComposeTestRule, x: f32, y: f32) {
    ui.host().send_pointer(PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(x, y)));
}

// ---- Input method composition -------------------------------------------------------------

/// A click in the field while an input method is composing (the desktop delivers clicks during
/// a composition: winit doesn't end it) moves the cursor. In Compose the click ends the
/// composition (`TextFieldValue(text, TextRange(offset))` has no composition) and what is shown
/// is what the field holds. Here the cursor moves (`move_cursor`, or the tap in
/// `on_pointer_event`) without the paragraph being laid out again, though the shown text
/// (`display_text`: the composition in place of the selection) changed: with "hello" and "ni"
/// composed at the end, a click after the "h" leaves the paragraph drawing "helloni" while the
/// field's text is "hniello", the underline and the cursor measured in the stale paragraph.
/// (With multibyte text the stale offsets fall inside characters and the measurer panics: see
/// `compose-skia/tests/review_f3.rs`.) The click's offset is also taken from the paragraph
/// holding the composition and used as an offset into the text without it.
#[test]
fn a_click_during_a_composition_draws_what_the_field_holds() {
    let mut ui = editor("hello", true);
    ui.on_node_with_tag("field").request_focus();
    ui.press_key(Key::MoveEnd);
    ui.host().send_preedit("ni", Some((2, 2)));
    ui.wait_for_idle();
    assert_eq!(drawn_text(&mut ui), "helloni");
    // After the "h".
    click(&mut ui, 7.0, 8.0);
    ui.wait_for_idle();
    let shown = text_of(&mut ui, "field");
    assert_eq!(drawn_text(&mut ui), shown, "the field draws a stale paragraph");
}

/// A field losing focus while an input method is composing keeps the composed text in
/// Compose: the composition is part of the field's value, and `TextFieldDelegate.onBlur`
/// commits it (`copy(composition = null)`), so composing "にほん" and tabbing to the next field
/// leaves "にほん" in the first. Here losing focus drops the composition (`end_composition` in
/// `on_focus_changed`): the composed text vanishes and the application never hears it. On the
/// desktop the input method also keeps composing (nothing tells it focus moved), so the next
/// key shows the whole composition in the newly focused field instead.
#[test]
fn losing_focus_keeps_the_composed_text() {
    #[composable]
    fn Two() {
        let first = remember(|| mutable_state_of(String::new()));
        let second = remember(|| mutable_state_of(String::new()));
        compose_foundation::layout::Column!({
            BasicTextField!(
                value = first.get(),
                on_value_change = move |value| first.set(value),
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("first"),
                single_line = true,
            );
            BasicTextField!(
                value = second.get(),
                on_value_change = move |value| second.set(value),
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("second"),
                single_line = true,
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Two);
    ui.on_node_with_tag("first").request_focus();
    ui.host().send_preedit("にほん", Some((9, 9)));
    ui.wait_for_idle();
    assert_eq!(text_of(&mut ui, "first"), "にほん");
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("second").assert_is_focused();
    assert_eq!(text_of(&mut ui, "first"), "にほん", "the composed text was dropped");
}

// ---- Keys -----------------------------------------------------------------------------------

/// End goes to the end of the line: on a hard line ending in a space, after that space
/// (Compose's `KeyCommand.LINE_END`: `getLineEnd(line, visibleEnd = false)` up to the line
/// break). In "ab \ncd", End from the start then "X" gives "ab X\ncd". Here `line_edge` takes
/// any line ending in a space for a soft wrap and steps back over it, so the cursor stops
/// before the space: "abX \ncd". (Shift+End leaves the space out of the selection too.)
#[test]
fn end_goes_after_a_trailing_space_before_a_new_line() {
    let mut ui = editor("ab \ncd", false);
    ui.on_node_with_tag("field").request_focus();
    ui.press_key(Key::MoveEnd);
    ui.type_text("X");
    assert_eq!(text_of(&mut ui, "field"), "ab X\ncd");
}

/// On macOS, Ctrl+A moves to the start of the line in Compose Desktop, as in every Cocoa text
/// field (`KeyMapping.macos.kt`: Ctrl+A is `LINE_START`, the Emacs-like shortcuts; select all is
/// Cmd+A only). In "hello" with the cursor at the end, Ctrl+A then "X" gives "Xhello". Here
/// Ctrl counts as the shortcut modifier on macOS too, so Ctrl+A selects everything and the "X"
/// replaces the whole text: "X".
#[test]
#[cfg(target_os = "macos")]
fn control_a_moves_to_the_line_start_on_macos() {
    let mut ui = editor("hello", true);
    ui.on_node_with_tag("field").request_focus();
    ui.press_key(Key::MoveEnd);
    chord(&mut ui, Key::Character('a'), KeyModifiers { ctrl: true, ..Default::default() });
    ui.type_text("X");
    assert_eq!(text_of(&mut ui, "field"), "Xhello");
}

// ---- The pointer ----------------------------------------------------------------------------

/// After a double-click selects a word, dragging extends the selection by whole words, and
/// the double-clicked word stays selected whichever way the pointer goes (Compose's
/// `mouseSelectionObserver` with `SelectionAdjustment.Word` for two clicks). Double-clicking
/// "world" in "hello world" and dragging back into "hello" selects "hello world", and "X" then
/// replaces it all: "X". Here the drag keeps the word's start as the anchor and moves the
/// cursor by characters, so dragging back deselects the double-clicked word: "llo " is
/// selected and the text becomes "heXworld".
#[test]
fn a_drag_after_a_double_click_extends_by_words() {
    let mut ui = editor("hello world", true);
    ui.on_node_with_tag("field").request_focus();
    let host = ui.host();
    // Inside "world" (x = 56..84), twice.
    host.send_pointer(PointerEvent::press(Offset::new(66.0, 8.0)));
    host.send_pointer(PointerEvent::release(Offset::new(66.0, 8.0)));
    host.send_pointer(PointerEvent::press(Offset::new(66.0, 8.0)));
    // Back to after "he".
    for x in [50.0, 30.0, 14.0] {
        host.send_pointer(PointerEvent::moved(Offset::new(x, 8.0), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(14.0, 8.0)));
    ui.wait_for_idle();
    ui.type_text("X");
    assert_eq!(text_of(&mut ui, "field"), "X");
}

/// A triple-click selects the whole line (paragraph) in Compose's text field (the click count
/// of `mouseSelection`: one click places the cursor, two select a word, three a paragraph,
/// `SelectionAdjustment.Paragraph`). Triple-clicking in "hello world" then typing "X" gives
/// "X". Here the third click is a plain tap (the double-click took `last_tap`), which collapses
/// the word selection: the text becomes "hello worXld".
#[test]
fn a_triple_click_selects_the_line() {
    let mut ui = editor("hello world", true);
    ui.on_node_with_tag("field").request_focus();
    for _ in 0..3 {
        click(&mut ui, 66.0, 8.0);
    }
    ui.wait_for_idle();
    ui.type_text("X");
    assert_eq!(text_of(&mut ui, "field"), "X");
}
