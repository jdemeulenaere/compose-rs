//! Second phase F review: failing tests for compose-foundation's text field (docs/06-ui.md,
//! "Keyboard and text"). Each test states the input, what Jetpack Compose (Desktop) does, and
//! what happens instead. They use the fixed-advance measurer at density 1: 7 px per character
//! and 17.5 px per line at the default 14 sp, and a 2 px cursor.

#![allow(non_snake_case)]

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    IntSize, Key, KeyEvent, KeyEventType, KeyModifiers, Offset, PointerEvent, UnitExt,
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

/// The platform's shortcut modifier: Cmd on macOS, Ctrl elsewhere, as Compose Desktop's.
fn command() -> KeyModifiers {
    if cfg!(target_os = "macos") {
        KeyModifiers { meta: true, ..Default::default() }
    } else {
        KeyModifiers { ctrl: true, ..Default::default() }
    }
}

/// The word-motion modifier: Option on macOS, Ctrl elsewhere, as Compose Desktop's key
/// mappings (`KeyMapping.macos.kt`, `defaultKeyMapping`).
fn word() -> KeyModifiers {
    if cfg!(target_os = "macos") {
        KeyModifiers { alt: true, ..Default::default() }
    } else {
        KeyModifiers { ctrl: true, ..Default::default() }
    }
}

fn text_of(ui: &mut ComposeTestRule) -> String {
    let node = ui.on_node_with_tag("field").fetch_semantics_node();
    node.config.editable_text.as_deref().unwrap_or_default().to_string()
}

// ---- Keys ---------------------------------------------------------------------------------------

/// Down moves the cursor to the next line in a multi-line field, as Compose's
/// `TextFieldKeyInput` (`KeyCommand.DOWN`: `moveCursorDownByLine`, keeping the horizontal
/// position). In "ab\ncd", with the cursor at the start, Down then typing "X" gives
/// "ab\nXcd". Here Down and Up aren't handled at all (the key goes on to the ancestors), the
/// cursor stays on the first line, and the text becomes "Xab\ncd": a multi-line field can only
/// be moved through with Left and Right.
#[test]
fn down_moves_the_cursor_to_the_next_line() {
    let mut ui = editor("ab\ncd", false);
    ui.on_node_with_tag("field").request_focus();
    ui.press_key(Key::DirectionDown);
    ui.type_text("X");
    assert_eq!(text_of(&mut ui), "ab\nXcd");
}

/// Shift+Tab in a multi-line field moves focus back in Compose: its key mapping gives Tab
/// with Shift no command (only a plain Tab is `KeyCommand.TAB`, which types a tab), so the
/// event isn't consumed and focus moves to the previous field. Here the field types a tab for
/// Shift+Tab too, so keyboard focus can enter a multi-line field but never leave it: the first
/// field stays unfocused and the second holds "\t".
#[test]
fn shift_tab_leaves_a_multi_line_field() {
    #[composable]
    fn Two() {
        let first = remember(|| mutable_state_of(String::new()));
        let second = remember(|| mutable_state_of(String::new()));
        compose_foundation::layout::Column!({
            BasicTextField!(
                value = first.get(),
                on_value_change = move |value| first.set(value),
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("first"),
            );
            BasicTextField!(
                value = second.get(),
                on_value_change = move |value| second.set(value),
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("second"),
            );
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), Two);
    ui.on_node_with_tag("second").request_focus();
    chord(&mut ui, Key::Tab, KeyModifiers { shift: true, ..Default::default() });
    ui.on_node_with_tag("second").assert_text_equals(&[""]);
    ui.on_node_with_tag("first").assert_is_focused();
}

/// Word motion: Option+Left on macOS, Ctrl+Left elsewhere, moves to the start of the previous
/// word in Compose (`KeyCommand.LEFT_WORD`), so from the end of "hello world" it lands before
/// "world" and "X" typed there gives "hello Xworld". Here the modifier is ignored and the
/// cursor moves one character: "hello worlXd". (Option/Ctrl+Backspace, which deletes a word in
/// Compose, deletes one character the same way.)
#[test]
fn word_left_moves_to_the_previous_word() {
    let mut ui = editor("hello world", true);
    ui.on_node_with_tag("field").request_focus();
    ui.press_key(Key::MoveEnd);
    chord(&mut ui, Key::DirectionLeft, word());
    ui.type_text("X");
    assert_eq!(text_of(&mut ui), "hello Xworld");
}

/// Undo: Cmd+Z (Ctrl+Z elsewhere) undoes the last edit in Compose's text field
/// (`KeyCommand.UNDO`, `TextFieldKeyInput`'s `UndoManager`), so typing "abc" on the keyboard
/// and pressing it leaves the field empty again (keys typed within five seconds are one undo
/// step). Here the shortcut isn't handled (nor Cmd+Shift+Z, redo) and the text stays "abc".
#[test]
fn command_z_undoes_typing() {
    let mut ui = editor("", true);
    ui.on_node_with_tag("field").request_focus();
    for c in ['a', 'b', 'c'] {
        // As the desktop host types a key: down, its committed text, up.
        ui.host().send_key(KeyEvent::down(Key::Character(c)));
        ui.host().send_text(&c.to_string());
        ui.host().send_key(KeyEvent::up(Key::Character(c)));
        ui.wait_for_idle();
    }
    assert_eq!(text_of(&mut ui), "abc");
    chord(&mut ui, Key::Character('z'), command());
    assert_eq!(text_of(&mut ui), "");
}

// ---- Focus --------------------------------------------------------------------------------------

/// A field that loses focus drops its selection in Compose: `CoreTextField`'s focus callback
/// calls `manager.deselect()` when focus goes, which collapses a selection to its end. So
/// selecting all of "hello" (Cmd/Ctrl+A), tabbing to the next field and back, then typing "X"
/// gives "helloX". Here the selection outlives the focus (hidden while unfocused, back when
/// focus returns), and the "X" replaces the whole text: "X".
#[test]
fn losing_focus_drops_the_selection() {
    #[composable]
    fn Two() {
        let first = remember(|| mutable_state_of(String::from("hello")));
        let second = remember(|| mutable_state_of(String::new()));
        compose_foundation::layout::Column!({
            BasicTextField!(
                value = first.get(),
                on_value_change = move |value| first.set(value),
                modifier = Modifier.size_wh(200.dp(), 40.dp()).test_tag("field"),
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
    ui.on_node_with_tag("field").request_focus();
    chord(&mut ui, Key::Character('a'), command());
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("second").assert_is_focused();
    chord(&mut ui, Key::Tab, KeyModifiers { shift: true, ..Default::default() });
    ui.on_node_with_tag("field").assert_is_focused();
    ui.type_text("X");
    assert_eq!(text_of(&mut ui), "helloX");
}

// ---- The pointer --------------------------------------------------------------------------------

/// Dragging the mouse across text selects it in Compose's text field (`TextFieldSelectionManager`
/// `mouseSelectionObserver`: a press puts the cursor, a drag extends the selection), and the
/// docs promise it ("selects with Shift and the pointer"). Pressing before "hello" and dragging
/// to after it in "hello world", then typing "X", replaces the word: "X world". Here the field
/// ignores moves: the release just puts the cursor after "hello", and the text becomes
/// "helloX world".
#[test]
fn dragging_across_text_selects_it() {
    let mut ui = editor("hello world", true);
    ui.on_node_with_tag("field").request_focus();
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(1.0, 8.0)));
    for x in [10.0, 20.0, 30.0, 35.0] {
        host.send_pointer(PointerEvent::moved(Offset::new(x, 8.0), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(35.0, 8.0)));
    ui.wait_for_idle();
    ui.type_text("X");
    assert_eq!(text_of(&mut ui), "X world");
}

/// Events can arrive before the next frame (the desktop delivers a batch of them, then draws).
/// After typing, the field has no paragraph until it's measured again, so a tap then is only
/// recorded (`pending_tap`) and resolved at the next measurement, which moves the cursor but
/// not the selection's anchor. Typing "hello world", then (before a frame) tapping after the
/// "h", then typing "X" gives "hXello world" in Compose: a tap collapses the selection. Here
/// the anchor stays at the end, the tap leaves "ello world" selected, and "X" replaces it:
/// "hX".
#[test]
fn a_tap_right_after_typing_does_not_select() {
    let mut ui = editor("", true);
    ui.on_node_with_tag("field").request_focus();
    let host = ui.host();
    host.send_text("hello world");
    // Between "h" and "e".
    host.send_pointer(PointerEvent::press(Offset::new(7.0, 8.0)));
    host.send_pointer(PointerEvent::release(Offset::new(7.0, 8.0)));
    ui.wait_for_idle();
    ui.type_text("X");
    assert_eq!(text_of(&mut ui), "hXello world");
}

// ---- Scrolling ------------------------------------------------------------------------------------

/// The bounds of the last rectangle drawn (the focused field's cursor) in the root, following
/// translations.
fn last_rect(list: &compose_ui::platform::DisplayList) -> compose_ui::Rect {
    use compose_ui::{Rect, platform::DrawOp};
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut last = None;
    for op in &list.ops {
        match op {
            DrawOp::Translate(offset) => {
                x += offset.x;
                y += offset.y;
            }
            DrawOp::DrawRect(rect, _) => {
                last = Some(Rect::new(rect.left + x, rect.top + y, rect.right + x, rect.bottom + y))
            }
            _ => {}
        }
    }
    last.expect("a cursor")
}

/// A multi-line field taller than its bounds scrolls vertically in Compose to keep the cursor
/// in view (`TextFieldScrollerPosition` with `Orientation.Vertical`, `Modifier.textFieldScroll`),
/// as the single-line one scrolls sideways. In a 40 px field (two lines of 17.5 px), typing
/// four lines leaves the cursor on the fourth line, which Compose scrolls into the field. Here
/// only single-line fields scroll: the text is clipped at the field's bottom and the cursor is
/// drawn at y = 52.5..70, below the field (0..40), out of sight while typing.
#[test]
fn a_multi_line_field_keeps_its_cursor_in_view() {
    let mut ui = editor("", false);
    ui.on_node_with_tag("field").perform_text_input("one\ntwo\nthree\nfour");
    let field = ui.on_node_with_tag("field").fetch_semantics_node().bounds;
    let cursor = last_rect(&ui.host().display_list());
    assert!(
        cursor.top >= field.top && cursor.bottom <= field.bottom,
        "the cursor {cursor:?} is outside the field {field:?}"
    );
}
