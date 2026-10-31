//! What a frame tells `ComposeRsView` of the soft keyboard (`input_method.rs`, D250): the
//! focused field's `EditorInfo`, the input restarting when its options change, its action key,
//! and the content's requests to show or hide the keyboard.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_android::input_method::{
    CURSOR_ANCHOR_HEADER, CursorUpdates, InputConnections, LastTextInput, TextInputState,
    editor_info, ime_action, input_state_units, input_type, selected_text, take_text_input_state,
    text_after_cursor, text_before_cursor,
};
use compose_android::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{MutableState, OwnedState, mutable_state_of, remember};
use compose_ui::Dialog;
use compose_ui::{
    Density, EventContext, FocusTargetModifierNode, ImeAction, ImeOptions, IntSize, Key, KeyEvent,
    KeyInputModifierNode, KeyModifiers, KeyboardType, LOCAL_SOFTWARE_KEYBOARD_CONTROLLER,
    ModifierNode, ModifierNodeElement, Offset, PointerEvent, Rect, SoftwareKeyboardController,
    TextRange, UnitExt,
    platform::{ParagraphMetrics, UiHost},
};

type Controller = Rc<RefCell<Option<SoftwareKeyboardController>>>;

/// 8 px per UTF-16 unit, one 20 px line.
struct Fake;

impl TextBackend for Fake {
    fn layout(&mut self, _handle: u32, text: &str, _request: &[u8]) -> LaidOut {
        let units = text.encode_utf16().count() as i32;
        let metrics = ParagraphMetrics {
            size: IntSize::new(units * 8, 20),
            first_baseline: 16.0,
            last_baseline: 16.0,
            line_count: 1,
        };
        LaidOut { metrics, did_exceed_max_lines: false, max_intrinsic_width: None }
    }
    fn hit_test(&self, _handle: u32, position: Offset) -> usize {
        (position.x / 8.0).round().max(0.0) as usize
    }
    fn cursor(&self, _handle: u32, offset: usize) -> Rect {
        let x = offset as f32 * 8.0;
        Rect::new(x, 0.0, x, 20.0)
    }
    fn release(&mut self, _handle: u32) {}
}

/// Two single-line fields, one above the other (40 px each): an email whose action is `Next`,
/// and a field whose keyboard type `kind` sets, its action `Done`. `controller` gets the
/// content's keyboard controller.
fn host(kind: MutableState<KeyboardType>, controller: Controller) -> UiHost {
    let mut host = UiHost::new(
        IntSize::new(200, 200),
        Density::new(1.0),
        Box::new(AndroidTextMeasurer::new(Fake)),
    );
    host.set_content(move || {
        *controller.borrow_mut() = LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current();
        let first = remember(|| mutable_state_of(String::new()));
        let second = remember(|| mutable_state_of(String::new()));
        Column!({
            BasicTextField!(
                first.get(),
                move |new: String| first.set(new),
                modifier = Modifier.size((160.dp(), 40.dp())),
                keyboard_options = KeyboardOptions {
                    keyboard_type: KeyboardType::Email,
                    ime_action: ImeAction::Next,
                    ..KeyboardOptions::DEFAULT
                },
                single_line = true
            );
            BasicTextField!(
                second.get(),
                move |new: String| second.set(new),
                modifier = Modifier.size((160.dp(), 40.dp())),
                keyboard_options = KeyboardOptions {
                    keyboard_type: kind.get(),
                    ime_action: ImeAction::Done,
                    ..KeyboardOptions::DEFAULT
                },
                single_line = true
            );
        });
    });
    host.frame();
    host
}

fn click(host: &mut UiHost, x: f32, y: f32) {
    host.send_pointer(PointerEvent::press(Offset::new(x, y)));
    host.send_pointer(PointerEvent::release(Offset::new(x, y)));
}

#[test]
fn the_focused_fields_options_reach_the_keyboard_and_a_change_restarts_it() {
    let kind = mutable_state_of(KeyboardType::Text);
    let mut host = host(kind.handle(), Controller::default());
    let mut last = LastTextInput::default();
    assert_eq!(take_text_input_state(&host, &mut last), TextInputState::default());

    click(&mut host, 10.0, 10.0);
    host.frame();
    // The input starts for a field taking the focus (Kotlin starts it for the first field anyway).
    let state = take_text_input_state(&host, &mut last);
    assert!(state.active && state.restart, "{state:?}");
    let options = host.text_input_options().expect("the focused email field's options");
    assert_eq!(
        options,
        ImeOptions {
            single_line: true,
            keyboard_type: KeyboardType::Email,
            ime_action: ImeAction::Next,
            ..ImeOptions::DEFAULT
        }
    );
    let info = editor_info(options, false);
    assert_eq!(
        info.input_type,
        input_type::TYPE_CLASS_TEXT
            | input_type::TYPE_TEXT_VARIATION_EMAIL_ADDRESS
            | input_type::TYPE_TEXT_FLAG_AUTO_CORRECT
    );
    assert_eq!(info.ime_options & editor_info::IME_MASK_ACTION, editor_info::IME_ACTION_NEXT);

    // Nothing changed: no restart.
    host.frame();
    assert!(!take_text_input_state(&host, &mut last).restart);

    // The keyboard's Next moves the focus on, to a field of other options: the input restarts.
    assert!(host.perform_ime_action(ime_action(editor_info::IME_ACTION_NEXT)));
    host.frame();
    let state = take_text_input_state(&host, &mut last);
    assert!(state.active && state.restart, "{state:?}");
    assert_eq!(host.text_input_options().map(|options| options.ime_action), Some(ImeAction::Done));

    // The focused field's own options change: the input restarts too.
    kind.set(KeyboardType::Number);
    host.frame();
    let state = take_text_input_state(&host, &mut last);
    assert!(state.active && state.restart, "{state:?}");
    let info = editor_info(host.text_input_options().unwrap(), false);
    assert_eq!(info.input_type, input_type::TYPE_CLASS_NUMBER);
}

#[test]
fn done_hides_the_keyboard_and_the_controller_shows_it() {
    let kind = mutable_state_of(KeyboardType::Text);
    let controller = Controller::default();
    let mut host = host(kind.handle(), controller.clone());
    let mut last = LastTextInput::default();
    click(&mut host, 10.0, 50.0);
    host.frame();
    assert!(take_text_input_state(&host, &mut last).active);

    // The keyboard's Done, by default, hides it (and keeps the focus).
    assert!(host.perform_ime_action(ime_action(editor_info::IME_ACTION_DONE)));
    host.frame();
    let state = take_text_input_state(&host, &mut last);
    assert_eq!(state.keyboard, Some(false));
    assert!(state.active);
    // Taken: the next frame asks nothing.
    host.frame();
    assert_eq!(take_text_input_state(&host, &mut last).keyboard, None);

    // A hardware Enter in a single line performs the same action.
    host.send_key(compose_ui::KeyEvent {
        key: compose_ui::Key::Enter,
        kind: compose_ui::KeyEventType::KeyDown,
        modifiers: Default::default(),
    });
    host.frame();
    assert_eq!(take_text_input_state(&host, &mut last).keyboard, Some(false));

    // The content's controller shows it again.
    controller.borrow().as_ref().expect("the tree's controller").show();
    host.frame();
    assert_eq!(take_text_input_state(&host, &mut last).keyboard, Some(true));
}

// The keyboard's connection: UTF-16 numbers, as `ComposeRsInputConnection` passes them on.

/// A focused multi-line field and a connection to it, as a keyboard has one.
struct Keyboard {
    host: UiHost,
    connections: InputConnections,
    id: i64,
    value: OwnedState<String>,
    /// How many text changes the application heard.
    edits: Rc<Cell<u32>>,
    /// The options the frames saw, for `take_text_input_state`.
    options: LastTextInput,
}

impl Keyboard {
    fn with(initial: &str, read_only: bool, filter: fn(String) -> String) -> Keyboard {
        let value = mutable_state_of(initial.to_string());
        let edits = Rc::new(Cell::new(0));
        let mut host = UiHost::new(
            IntSize::new(400, 200),
            Density::new(1.0),
            Box::new(AndroidTextMeasurer::new(Fake)),
        );
        let (state, heard) = (value.handle(), edits.clone());
        host.set_content(move || {
            let heard = heard.clone();
            BasicTextField!(
                state.get(),
                move |new: String| {
                    heard.set(heard.get() + 1);
                    state.set(filter(new));
                },
                modifier = Modifier.size((300.dp(), 40.dp())),
                read_only = read_only
            );
        });
        host.frame();
        click(&mut host, 1.0, 1.0);
        host.frame();
        let mut connections = InputConnections::default();
        let id = connections.open(&host);
        let mut keyboard =
            Keyboard { host, connections, id, value, edits, options: LastTextInput::default() };
        keyboard.frame();
        keyboard
    }

    fn new(initial: &str) -> Keyboard {
        Keyboard::with(initial, false, |new| new)
    }

    /// A field holding `text`, the cursor after it, the keyboard having heard.
    fn typed(text: &str) -> Keyboard {
        let mut keyboard = Keyboard::new("");
        keyboard.commit(text, 1);
        keyboard.take_update();
        keyboard.edits.set(0);
        keyboard
    }

    /// A frame; returns whether the input method must restart.
    fn frame(&mut self) -> bool {
        self.host.frame();
        let restart = take_text_input_state(&self.host, &mut self.options).restart;
        self.connections.frame_has_update(&self.host, restart);
        restart
    }

    fn value(&self) -> String {
        self.value.get()
    }

    fn state(&self) -> TextFieldValueOf {
        let value = self.host.input_method_value().expect("a focused field with text");
        TextFieldValueOf {
            text: value.text().to_string(),
            selection: value.selection(),
            composition: value.composition(),
        }
    }

    fn selection(&self) -> TextRange {
        self.state().selection
    }

    fn composition(&self) -> Option<TextRange> {
        self.state().composition
    }

    fn connection(&mut self) -> (&mut UiHost, compose_android::input_method::InputConnection<'_>) {
        let connection = self.connections.get(self.id).expect("an open connection");
        (&mut self.host, connection)
    }

    fn commit(&mut self, text: &str, position: i32) {
        let (ui, mut connection) = self.connection();
        assert!(connection.commit_text(ui, text, position));
        self.frame();
    }

    fn compose(&mut self, text: &str, position: i32) {
        let (ui, mut connection) = self.connection();
        assert!(connection.set_composing_text(ui, text, position));
        self.frame();
    }

    fn region(&mut self, start: i32, end: i32) {
        let (ui, mut connection) = self.connection();
        assert!(connection.set_composing_region(ui, start, end));
        self.frame();
    }

    fn delete(&mut self, before: i32, after: i32) {
        let (ui, mut connection) = self.connection();
        assert!(connection.delete_surrounding_text(ui, before, after));
        self.frame();
    }

    fn select(&mut self, start: i32, end: i32) {
        let (ui, mut connection) = self.connection();
        assert!(connection.set_selection(ui, start, end));
        self.frame();
    }

    fn finish(&mut self) {
        let (ui, mut connection) = self.connection();
        assert!(connection.finish_composing_text(ui));
        self.frame();
    }

    fn take_update(&mut self) -> Option<[i32; 4]> {
        self.connections.take_selection_update(&self.host).map(|update| {
            [
                update.selection_start,
                update.selection_end,
                update.composition_start,
                update.composition_end,
            ]
        })
    }

    fn key(&mut self, key: Key) {
        self.host.send_key(KeyEvent::down(key));
        self.host.send_key(KeyEvent::up(key));
        self.frame();
    }

    /// Text from a key, as the bridge sends it after the key event.
    fn type_key(&mut self, character: char) {
        self.host.send_key(KeyEvent::down(Key::Character(character)));
        self.host.send_text(&character.to_string());
        self.host.send_key(KeyEvent::up(Key::Character(character)));
        self.frame();
    }
}

/// `TextFieldValue`'s parts, to compare and print.
#[derive(PartialEq, Debug)]
struct TextFieldValueOf {
    text: String,
    selection: TextRange,
    composition: Option<TextRange>,
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, getTextBeforeAndAfterCursorTest
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, getTextBeforeAndAfterCursorTest_maxCharTest
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, getSelectedTextTest
#[test]
fn the_keyboard_reads_the_text_around_the_cursor() {
    let mut keyboard = Keyboard::new("Hello, World");
    let read = |keyboard: &Keyboard, max: i32| {
        let value = keyboard.host.input_method_value().unwrap();
        (
            text_before_cursor(&value, max).to_string(),
            text_after_cursor(&value, max).to_string(),
            selected_text(&value).map(str::to_string),
        )
    };
    assert_eq!(read(&keyboard, 100), (String::new(), "Hello, World".to_string(), None));
    assert_eq!(read(&keyboard, 5), (String::new(), "Hello".to_string(), None));
    keyboard.select(1, 1);
    assert_eq!(read(&keyboard, 100), ("H".to_string(), "ello, World".to_string(), None));
    assert_eq!(read(&keyboard, 5), ("H".to_string(), "ello,".to_string(), None));
    keyboard.select(12, 12);
    assert_eq!(read(&keyboard, 5), ("World".to_string(), String::new(), None));
    keyboard.select(0, 5);
    assert_eq!(read(&keyboard, 2), (String::new(), ", ".to_string(), Some("Hello".to_string())));
}

/// Its numbers count UTF-16 units; half a surrogate pair can't be read back in a `String`, so a
/// count ending inside one leaves it out, and the text it composes is there to read at once.
#[test]
fn reading_counts_utf16_units_and_includes_the_composition() {
    let mut keyboard = Keyboard::typed("a\u{1F600}b");
    let value = keyboard.host.input_method_value().unwrap();
    assert_eq!(text_before_cursor(&value, 3), "\u{1F600}b");
    assert_eq!(text_before_cursor(&value, 2), "b");
    assert_eq!(input_state_units(&value), [4, 4, -1, -1]);
    keyboard.compose("ni", 1);
    let value = keyboard.host.input_method_value().unwrap();
    assert_eq!(text_before_cursor(&value, 2), "ni");
    assert_eq!(input_state_units(&value), [6, 6, 4, 6]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/EditorInfoTest.kt, initial_default_selection_info_is_set
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/EditorInfoTest.kt, initial_selection_info_is_set
#[test]
fn a_new_connection_starts_from_what_it_can_read() {
    let mut keyboard = Keyboard::new("");
    assert_eq!(input_state_units(&keyboard.host.input_method_value().unwrap()), [0, 0, -1, -1]);
    keyboard.commit("\u{1F600}x", 1);
    keyboard.select(2, 3);
    assert_eq!(input_state_units(&keyboard.host.input_method_value().unwrap()), [2, 3, -1, -1]);
    // A new connection after the keyboard has heard has nothing to add.
    assert_eq!(keyboard.take_update(), Some([2, 3, -1, -1]));
    keyboard.id = keyboard.connections.open(&keyboard.host);
    assert_eq!(keyboard.take_update(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, commitTextTest
#[test]
fn committed_text_goes_in_with_the_cursor_after_it() {
    let mut keyboard = Keyboard::new("");
    keyboard.commit("Hello, ", 1);
    assert_eq!(keyboard.value(), "Hello, ");
    assert_eq!(keyboard.selection(), TextRange::at(7));
    keyboard.commit("World.", 1);
    assert_eq!(keyboard.value(), "Hello, World.");
    assert_eq!(keyboard.selection(), TextRange::at(13));
    assert_eq!(keyboard.edits.get(), 2);
}

/// `commitText`'s and `setComposingText`'s new cursor position count UTF-16 units from the text: a
/// surrogate pair is 2 of them and 4 bytes, and a cursor inside one goes to its start (D1100).
#[test]
fn the_new_cursor_position_counts_utf16_units() {
    let mut keyboard = Keyboard::typed("a\u{1F600}b");
    // Before the text, then one unit further back: before the "b".
    keyboard.commit("X", -1);
    assert_eq!(keyboard.value(), "a\u{1F600}bX");
    assert_eq!(keyboard.selection(), TextRange::at(5));
    // Nothing committed, two units back: before the emoji, not inside it.
    keyboard.commit("", -2);
    assert_eq!(keyboard.selection(), TextRange::at(1));
    // After the text, then two units further: after the emoji.
    keyboard.commit("Y", 3);
    assert_eq!(keyboard.value(), "aY\u{1F600}bX");
    assert_eq!(keyboard.selection(), TextRange::at(6));
    // One unit back lands inside the emoji: its start.
    keyboard.commit("", -1);
    assert_eq!(keyboard.selection(), TextRange::at(2));
    // Past either end: kept in the text.
    keyboard.commit("", 100);
    assert_eq!(keyboard.selection(), TextRange::at(keyboard.value().len()));
    keyboard.commit("", -100);
    assert_eq!(keyboard.selection(), TextRange::at(0));

    // Composing text's: after it, before it, and past it into the text.
    let mut keyboard = Keyboard::typed("ab");
    keyboard.select(1, 1);
    keyboard.compose("\u{1F600}", 1);
    assert_eq!(keyboard.selection(), TextRange::at(5));
    keyboard.compose("xy", 0);
    assert_eq!(keyboard.state().text, "axyb");
    assert_eq!(keyboard.selection(), TextRange::at(1));
    keyboard.compose("xy", 2);
    assert_eq!(keyboard.selection(), TextRange::at(4));
    assert_eq!(keyboard.composition(), Some(TextRange::new(1, 3)));
}

/// A cursor moved without text is no edit: the application hears nothing.
#[test]
fn a_commit_that_only_moves_the_cursor_is_no_edit() {
    let mut keyboard = Keyboard::typed("abc");
    keyboard.commit("", -1);
    assert_eq!(keyboard.selection(), TextRange::at(2));
    assert_eq!(keyboard.edits.get(), 0);
}

/// The composition is part of the value, as Compose's (D1663): the application hears each step, as
/// a keyboard composing every word would otherwise leave it hearing nothing until a space.
#[test]
fn the_application_hears_what_is_composed() {
    let mut keyboard = Keyboard::new("");
    keyboard.compose("h", 1);
    assert_eq!(keyboard.value(), "h");
    keyboard.compose("he", 1);
    assert_eq!(keyboard.value(), "he");
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 2)));
    assert_eq!(keyboard.edits.get(), 2);
    keyboard.finish();
    assert_eq!(keyboard.value(), "he");
    assert_eq!(keyboard.composition(), None);
    assert_eq!(keyboard.edits.get(), 2, "finishing changes no text");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, setComposingTextTest
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, finishComposingText
#[test]
fn composing_then_finishing_keeps_the_text() {
    let mut keyboard = Keyboard::typed("ab");
    keyboard.compose("cd", 1);
    assert_eq!(keyboard.composition(), Some(TextRange::new(2, 4)));
    keyboard.compose("xyz", 1);
    assert_eq!(keyboard.state().text, "abxyz");
    keyboard.finish();
    assert_eq!(keyboard.value(), "abxyz");
    assert_eq!(keyboard.selection(), TextRange::at(5));
    keyboard.finish();
    assert_eq!(keyboard.value(), "abxyz");
    // Committed text takes a composition's place.
    keyboard.compose("q", 1);
    keyboard.commit("cd", 1);
    assert_eq!(keyboard.value(), "abxyzcd");
    assert_eq!(keyboard.composition(), None);
    // Empty composing text deletes the composition.
    keyboard.compose("q", 1);
    keyboard.compose("", 1);
    assert_eq!(keyboard.value(), "abxyzcd");
    assert_eq!(keyboard.composition(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, deleteSurroundingText
#[test]
fn a_surrounding_deletion_is_one_edit_around_the_cursor() {
    let mut keyboard = Keyboard::typed("abcdef");
    keyboard.commit("", -2);
    keyboard.delete(2, 1);
    assert_eq!(keyboard.value(), "abf");
    assert_eq!(keyboard.selection(), TextRange::at(2));
    assert_eq!(keyboard.edits.get(), 1, "one edit, not one per unit");
    // Past the text: to its ends. Negative lengths delete nothing.
    keyboard.delete(-1, -1);
    assert_eq!(keyboard.value(), "abf");
    keyboard.delete(100, 100);
    assert_eq!(keyboard.value(), "");
}

/// B1: a keyboard told of the selection sends what clears it, but one sending this for Backspace
/// anyway means the selection, not what's around it (D1665; Compose's would keep it). Select all
/// in "hello", or "world" in "hello world".
#[test]
fn a_surrounding_deletion_takes_a_selection() {
    let mut keyboard = Keyboard::typed("hello");
    keyboard.select(0, 5);
    keyboard.delete(1, 0);
    assert_eq!(keyboard.value(), "");
    assert_eq!(keyboard.selection(), TextRange::at(0));

    let mut keyboard = Keyboard::typed("hello world");
    keyboard.select(6, 11);
    keyboard.delete(1, 0);
    assert_eq!(keyboard.value(), "hello ");
    assert_eq!(keyboard.selection(), TextRange::at(6));

    // Nothing asked: nothing goes.
    let mut keyboard = Keyboard::typed("hello");
    keyboard.select(0, 5);
    keyboard.delete(0, 0);
    assert_eq!(keyboard.value(), "hello");
}

/// M1: the lengths are UTF-16 units of whole characters. Half a pair takes its pair (Compose's
/// would leave half), but a count stops inside a cluster where it ends, as Compose's: half a flag,
/// one person of a family, a mark without its letter.
#[test]
fn surrounding_lengths_are_utf16_units_of_whole_characters_as_compose_counts_them() {
    let mut keyboard = Keyboard::typed("a\u{1F600}b");
    keyboard.commit("", -1);
    keyboard.delete(2, 0);
    assert_eq!(keyboard.value(), "ab");
    assert_eq!(keyboard.selection(), TextRange::at(1));

    let mut keyboard = Keyboard::typed("\u{1F600}\u{1F601}\u{1F602}");
    keyboard.commit("", -4);
    assert_eq!(keyboard.selection(), TextRange::at(4));
    keyboard.delete(1, 1);
    assert_eq!(keyboard.value(), "\u{1F602}");

    let mut keyboard = Keyboard::typed("\u{1F1EF}\u{1F1F5}");
    keyboard.delete(2, 0);
    assert_eq!(keyboard.value(), "\u{1F1EF}", "a flag's second letter");

    let mut keyboard = Keyboard::typed("\u{1F468}\u{200D}\u{1F469}");
    keyboard.delete(2, 0);
    assert_eq!(keyboard.value(), "\u{1F468}\u{200D}", "a family's last person");

    let mut keyboard = Keyboard::typed("e\u{301}");
    keyboard.delete(1, 0);
    assert_eq!(keyboard.value(), "e");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, deleteSurroundingTextInCodePoints
#[test]
fn a_surrounding_deletion_in_code_points_counts_characters() {
    let mut keyboard = Keyboard::typed("a\u{1F600}\u{1F601}b");
    keyboard.commit("", -1);
    let (ui, mut connection) = keyboard.connection();
    assert!(connection.delete_surrounding_text_in_code_points(ui, 2, 1));
    keyboard.frame();
    assert_eq!(keyboard.value(), "a");
    assert_eq!(keyboard.edits.get(), 1);
}

/// With a composition the lengths count from the selection, in the text's own units, not from the
/// composition: non-ASCII on every side, so bytes or the wrong end would show.
#[test]
fn surrounding_lengths_count_from_the_selection_around_a_composition() {
    // "日本語" composing "本語", the cursor after it: one unit back shrinks the composition.
    let mut keyboard = Keyboard::typed("\u{65E5}");
    keyboard.compose("\u{672C}\u{8A9E}", 1);
    keyboard.delete(1, 0);
    assert_eq!(keyboard.value(), "\u{65E5}\u{672C}");
    assert_eq!(keyboard.composition(), Some(TextRange::new(3, 6)));

    // The cursor before all of it: one unit forward takes "日", and the composition moves back.
    let mut keyboard = Keyboard::typed("\u{65E5}");
    keyboard.compose("\u{672C}\u{8A9E}", 1);
    keyboard.select(0, 0);
    keyboard.delete(0, 1);
    assert_eq!(keyboard.value(), "\u{672C}\u{8A9E}");
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 6)));

    // The cursor inside it, a pair on either side: two units each way, the pairs whole.
    let mut keyboard = Keyboard::new("");
    keyboard.compose("\u{1F600}\u{1F601}", 1);
    keyboard.select(2, 2);
    keyboard.delete(2, 2);
    assert_eq!(keyboard.value(), "");
    assert_eq!(keyboard.composition(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, setSelection
#[test]
fn the_keyboard_moves_the_selection_in_utf16_units() {
    let mut keyboard = Keyboard::typed("\u{1F600}bc");
    keyboard.select(2, 3);
    assert_eq!(keyboard.selection(), TextRange::new(4, 5));
    assert_eq!(keyboard.edits.get(), 0);
    // Half a pair: a range widens over it, a cursor goes to its start.
    keyboard.select(1, 3);
    assert_eq!(keyboard.selection(), TextRange::new(0, 5));
    keyboard.select(1, 1);
    assert_eq!(keyboard.selection(), TextRange::at(0));
    // Past the text: kept in it.
    keyboard.select(2, 100);
    assert_eq!(keyboard.selection(), TextRange::new(4, 6));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, setComposingRegion
#[test]
fn a_composing_region_marks_the_text_and_changes_nothing_else() {
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 11);
    assert_eq!(keyboard.composition(), Some(TextRange::new(6, 11)));
    assert_eq!(keyboard.value(), "hello world");
    assert_eq!(keyboard.selection(), TextRange::at(11));
    assert_eq!(keyboard.edits.get(), 0);
    // The cursor outside the region stays there.
    keyboard.region(0, 5);
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 5)));
    assert_eq!(keyboard.selection(), TextRange::at(11));
    // In either order; kept in the text; collapsed, none.
    keyboard.region(5, 0);
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 5)));
    keyboard.region(6, 100);
    assert_eq!(keyboard.composition(), Some(TextRange::new(6, 11)));
    keyboard.region(-3, 5);
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 5)));
    keyboard.region(3, 3);
    assert_eq!(keyboard.composition(), None);
    assert_eq!(keyboard.value(), "hello world");
}

/// Its positions are UTF-16 units: a surrogate pair's 2 are 4 bytes, and a range ending inside one
/// widens to it (D1100).
#[test]
fn a_composing_region_converts_utf16_positions() {
    let mut keyboard = Keyboard::typed("\u{1F600}bc");
    keyboard.region(2, 3);
    assert_eq!(keyboard.composition(), Some(TextRange::new(4, 5)));
    keyboard.region(1, 3);
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 5)));
    keyboard.region(0, 1);
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 4)));
    keyboard.region(1, 1);
    assert_eq!(keyboard.composition(), None);
}

/// B2: told of the text and the cursor, a keyboard names the word it finds: "draft ", commit
/// "hello", then the region of "hello" from what it reads, not offsets it can't know.
#[test]
fn a_keyboard_composes_over_the_word_it_reads() {
    let mut keyboard = Keyboard::typed("draft ");
    keyboard.commit("hello", 1);
    // It heard where the cursor went, and reads the word before it.
    assert_eq!(keyboard.take_update(), Some([11, 11, -1, -1]));
    let value = keyboard.host.input_method_value().unwrap();
    assert_eq!(text_before_cursor(&value, 5), "hello");
    keyboard.region(6, 11);
    keyboard.compose("hellos", 1);
    keyboard.finish();
    assert_eq!(keyboard.value(), "draft hellos");
    assert_eq!(keyboard.selection(), TextRange::at(12));
}

/// B2: a held region is the text's: keys do what they do, finishing it first, as Compose's key
/// input. Typed text goes at the cursor, not in the region's place.
#[test]
fn a_held_region_does_not_block_keys_or_get_replaced_by_typing() {
    // Backspace at the end of "hello world", "hello" held.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(0, 5);
    keyboard.key(Key::Backspace);
    assert_eq!(keyboard.value(), "hello worl");
    assert_eq!(keyboard.composition(), None);

    // An arrow moves the cursor; the region stays, as Compose's selection keeps the composition.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(0, 5);
    keyboard.key(Key::DirectionLeft);
    assert_eq!(keyboard.selection(), TextRange::at(10));
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 5)));

    // A typed character goes at the cursor.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(0, 5);
    keyboard.type_key('!');
    assert_eq!(keyboard.value(), "hello world!");
    assert_eq!(keyboard.composition(), None);
    assert_eq!(keyboard.selection(), TextRange::at(12));
}

/// Over a region: a different composition replaces it in the value at once, as Compose's; another
/// region commits what was there.
#[test]
fn composing_over_a_region_replaces_it() {
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 11);
    keyboard.compose("earth", 1);
    assert_eq!(keyboard.value(), "hello earth");
    assert_eq!(keyboard.selection(), TextRange::at(11));
    keyboard.finish();
    assert_eq!(keyboard.composition(), None);

    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(0, 5);
    keyboard.compose("hi", 1);
    assert_eq!(keyboard.selection(), TextRange::at(2));
    keyboard.region(3, 8);
    assert_eq!(keyboard.value(), "hi world");
    assert_eq!(keyboard.composition(), Some(TextRange::new(3, 8)));

    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(5, 11);
    keyboard.compose("", 1);
    assert_eq!(keyboard.value(), "hello");
    assert_eq!(keyboard.composition(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, commitTextTest_batchSession
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, mixedAPICalls_batchSession
#[test]
fn a_batch_applies_its_edits_as_one() {
    let mut keyboard = Keyboard::new("");
    let (ui, mut connection) = keyboard.connection();
    assert!(connection.begin_batch_edit());
    assert!(connection.set_composing_text(ui, "Hello, ", 1));
    assert!(connection.finish_composing_text(ui));
    assert!(connection.commit_text(ui, "World.", 1));
    // Nothing yet, for the application or the keyboard's reads.
    assert_eq!(ui.input_method_value().unwrap().text(), "");
    assert!(connection.set_selection(ui, 0, 12));
    assert!(connection.commit_text(ui, "", 1));
    assert!(connection.begin_batch_edit());
    assert!(connection.end_batch_edit(ui), "still inside the outer batch");
    assert_eq!(ui.input_method_value().unwrap().text(), "");
    assert!(!connection.end_batch_edit(ui));
    keyboard.frame();
    assert_eq!(keyboard.value(), ".");
    assert_eq!(keyboard.selection(), TextRange::at(0));
    assert_eq!(keyboard.edits.get(), 1, "one change");
}

/// Each edit of a batch reads the text the ones before it left: UTF-16 numbers in a text they
/// changed.
#[test]
fn a_batchs_numbers_read_the_text_its_earlier_edits_left() {
    let mut keyboard = Keyboard::new("");
    let (ui, mut connection) = keyboard.connection();
    connection.begin_batch_edit();
    connection.commit_text(ui, "\u{1F600}abc", 1);
    connection.set_selection(ui, 2, 2);
    connection.delete_surrounding_text(ui, 0, 1);
    connection.end_batch_edit(ui);
    keyboard.frame();
    assert_eq!(keyboard.value(), "\u{1F600}bc");
    assert_eq!(keyboard.selection(), TextRange::at(4));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionUpdateTextFieldValueTest.kt, test_update_input_state
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionUpdateTextFieldValueTest.kt, test_update_input_state_extracted_text_monitor
#[test]
fn the_keyboard_hears_each_change_of_the_selection_and_composition() {
    let mut keyboard = Keyboard::typed("hello");
    assert_eq!(keyboard.take_update(), None, "it has heard");
    keyboard.select(1, 2);
    assert_eq!(keyboard.take_update(), Some([1, 2, -1, -1]));
    assert_eq!(keyboard.take_update(), None);
    keyboard.region(0, 3);
    assert_eq!(keyboard.take_update(), Some([1, 2, 0, 3]));
    keyboard.finish();
    assert_eq!(keyboard.take_update(), Some([1, 2, -1, -1]));
    // The application's own, by a key: told as well. In UTF-16 units.
    let mut keyboard = Keyboard::typed("\u{1F600}x");
    keyboard.key(Key::DirectionLeft);
    assert_eq!(keyboard.take_update(), Some([2, 2, -1, -1]));

    // The extracted text's token, once it follows the text.
    let mut keyboard = Keyboard::typed("hi");
    let (ui, mut connection) = keyboard.connection();
    connection.set_extracted_text_monitor(Some(7));
    assert!(connection.commit_text(ui, "!", 1));
    let update = keyboard.connections.take_selection_update(&keyboard.host).expect("an update");
    assert_eq!(update.extracted_text_token, Some(7));
    let (ui, mut connection) = keyboard.connection();
    connection.set_extracted_text_monitor(None);
    assert!(connection.commit_text(ui, "?", 1));
    let update = keyboard.connections.take_selection_update(&keyboard.host).unwrap();
    assert_eq!(update.extracted_text_token, None);
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionUpdateTextFieldValueTest.kt, test_update_input_state_inactive
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionInactiveTest.kt, commitText
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionInactiveTest.kt, deleteSurroundingText
// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionInactiveTest.kt, performEditorAction
#[test]
fn a_closed_connection_takes_nothing() {
    let mut keyboard = Keyboard::typed("abc");
    let id = keyboard.id;
    let (ui, mut connection) = keyboard.connection();
    assert!(connection.begin_batch_edit());
    assert!(connection.commit_text(ui, "dropped", 1));
    connection.close();
    assert!(!connection.is_active());
    assert!(!connection.commit_text(ui, "x", 1));
    assert!(!connection.set_composing_text(ui, "x", 1));
    assert!(!connection.set_composing_region(ui, 0, 1));
    assert!(!connection.delete_surrounding_text(ui, 1, 0));
    assert!(!connection.delete_surrounding_text_in_code_points(ui, 1, 0));
    assert!(!connection.set_selection(ui, 0, 1));
    assert!(!connection.finish_composing_text(ui));
    assert!(!connection.select_all(ui));
    assert!(!connection.begin_batch_edit());
    assert!(!connection.commit_correction(ui));
    assert!(!connection.perform_editor_action(ui, editor_info::IME_ACTION_DONE));
    assert!(!connection.request_cursor_updates(CursorUpdates::from_mode(1, 34)));
    assert!(!connection.end_batch_edit(ui));
    assert_eq!(keyboard.connections.take_selection_update(&keyboard.host), None);
    assert_eq!(keyboard.connections.take_cursor_anchor(&keyboard.host), None);
    assert!(!keyboard.connections.has_update(&keyboard.host));
    assert!(!keyboard.connections.get(id).unwrap().is_active());
    keyboard.frame();
    assert_eq!(keyboard.value(), "abc");
    assert_eq!(keyboard.selection(), TextRange::at(3));
}

/// Its action is the application's to hear only while it's open: Done hides the keyboard.
#[test]
fn a_closed_connection_performs_no_action() {
    let mut keyboard = Keyboard::typed("abc");
    let (ui, mut connection) = keyboard.connection();
    assert!(connection.perform_editor_action(ui, editor_info::IME_ACTION_DONE));
    assert_eq!(ui.take_software_keyboard_request(), Some(false));
    connection.close();
    assert!(!connection.perform_editor_action(ui, editor_info::IME_ACTION_DONE));
    assert_eq!(ui.take_software_keyboard_request(), None);
}

/// B1: Android may ask for a connection (a cold start's `MSG_BIND`, a keyboard switch), throw it
/// away and keep serving the one before: every connection works until its own close. (D1664's one
/// connection at a time closed the one in use: reads answered, and nothing typed.)
#[test]
fn a_connection_works_until_its_own_close_whatever_opens_after_it() {
    let mut keyboard = Keyboard::typed("abc");
    let served = keyboard.id;
    let discarded = keyboard.connections.open(&keyboard.host);
    let also_discarded = keyboard.connections.open(&keyboard.host);
    assert!(keyboard.connections.get(served).unwrap().is_active());
    keyboard.commit("d", 1);
    assert_eq!(keyboard.value(), "abcd");
    assert_eq!(keyboard.take_update(), Some([4, 4, -1, -1]));
    // Closing a newer one says nothing of it.
    keyboard.connections.close(discarded);
    keyboard.commit("e", 1);
    assert_eq!(keyboard.value(), "abcde");
    // Nor closing it of the newer one.
    keyboard.connections.close(served);
    let (ui, mut connection) = keyboard.connection();
    assert!(!connection.commit_text(ui, "x", 1));
    keyboard.id = also_discarded;
    keyboard.commit("f", 1);
    assert_eq!(keyboard.value(), "abcdef");
}

/// What the keyboard was told is the view's: a connection thrown away doesn't take from the one in
/// use an update it hasn't heard, and a batch of one holds them all back until it's applied.
#[test]
fn what_the_keyboard_was_told_is_shared_by_the_connections() {
    let mut keyboard = Keyboard::typed("abc");
    keyboard.commit("d", 1);
    keyboard.connections.open(&keyboard.host);
    assert_eq!(keyboard.take_update(), Some([4, 4, -1, -1]));
    assert_eq!(keyboard.take_update(), None);

    let (ui, mut connection) = keyboard.connection();
    connection.commit_text(ui, "e", 1);
    connection.begin_batch_edit();
    assert!(!keyboard.connections.has_update(&keyboard.host));
    assert_eq!(keyboard.take_update(), None, "a batch is open");
    let (ui, mut connection) = keyboard.connection();
    connection.end_batch_edit(ui);
    assert_eq!(keyboard.take_update(), Some([5, 5, -1, -1]));

    // A request made on one is answered whichever is open.
    keyboard.frame();
    let other = keyboard.connections.open(&keyboard.host);
    let (_, mut connection) = keyboard.connection();
    connection.request_cursor_updates(CursorUpdates::from_mode(1, 34));
    keyboard.connections.close(keyboard.id);
    assert!(keyboard.connections.get(other).unwrap().is_active());
    assert!(keyboard.connections.take_cursor_anchor(&keyboard.host).is_some());
}

/// Connections Android threw away never close: only the last 8 are kept, so an old one's calls
/// answer false, as a connection it can't be serving.
#[test]
fn only_the_last_connections_are_kept() {
    let mut keyboard = Keyboard::typed("abc");
    let first = keyboard.id;
    for _ in 0..7 {
        keyboard.connections.open(&keyboard.host);
    }
    assert!(keyboard.connections.get(first).is_some(), "8 kept");
    let last = keyboard.connections.open(&keyboard.host);
    assert!(keyboard.connections.get(first).is_none());
    assert!(keyboard.connections.get(last).unwrap().is_active());
    // A closed one goes first.
    let mut keyboard = Keyboard::typed("abc");
    let first = keyboard.id;
    let closed = keyboard.connections.open(&keyboard.host);
    keyboard.connections.close(closed);
    for _ in 0..7 {
        keyboard.connections.open(&keyboard.host);
    }
    assert!(keyboard.connections.get(first).is_some());
    assert!(keyboard.connections.get(closed).is_none());
}

/// L1: an edit in flight when the focus has moved is dropped: the connection was the other field's.
#[test]
fn a_connections_edits_after_the_focus_moved_are_dropped() {
    let kind = mutable_state_of(KeyboardType::Text);
    let mut host = host(kind.handle(), Controller::default());
    click(&mut host, 10.0, 10.0);
    host.frame();
    let mut connections = InputConnections::default();
    let first = connections.open(&host);
    click(&mut host, 10.0, 50.0);
    host.frame();
    let mut connection = connections.get(first).unwrap();
    connection.commit_text(&mut host, "late", 1);
    connection.begin_batch_edit();
    connection.commit_text(&mut host, "late", 1);
    connection.end_batch_edit(&mut host);
    host.frame();
    assert_eq!(host.input_method_value().unwrap().text(), "");
    // One opened for the field now focused takes them.
    let second = connections.open(&host);
    connections.get(second).unwrap().commit_text(&mut host, "new", 1);
    host.frame();
    assert_eq!(host.input_method_value().unwrap().text(), "new");
    click(&mut host, 10.0, 10.0);
    host.frame();
    assert_eq!(host.input_method_value().unwrap().text(), "");
}

/// While monitored and unchanged, the keyboard has nothing to hear and the measurer is left alone.
#[test]
fn monitoring_compares_before_asking_the_measurer() {
    let cursors = Rc::new(Cell::new(0));
    let mut host = UiHost::new(
        IntSize::new(400, 200),
        Density::new(1.0),
        Box::new(AndroidTextMeasurer::new(CountingCursors(cursors.clone()))),
    );
    host.set_content(|| {
        let text = remember(|| mutable_state_of(String::from("abc")));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = Modifier.size((300.dp(), 40.dp()))
        );
    });
    host.frame();
    click(&mut host, 1.0, 1.0);
    host.frame();
    let mut connections = InputConnections::default();
    let id = connections.open(&host);
    connections.get(id).unwrap().request_cursor_updates(CursorUpdates::from_mode(2, 34));
    assert!(connections.has_update(&host));
    assert!(connections.take_cursor_anchor(&host).is_some());
    let asked = cursors.get();
    assert!(asked > 0);
    for _ in 0..3 {
        host.frame();
        assert!(!connections.has_update(&host));
        assert_eq!(connections.take_cursor_anchor(&host), None);
    }
    assert_eq!(cursors.get(), asked, "no calls to the measurer");
}

/// A backend counting its cursor calls, the JNI a monitored frame would make.
struct CountingCursors(Rc<Cell<u32>>);

impl TextBackend for CountingCursors {
    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut {
        Fake.layout(handle, text, request)
    }
    fn hit_test(&self, handle: u32, position: Offset) -> usize {
        Fake.hit_test(handle, position)
    }
    fn cursor(&self, handle: u32, offset: usize) -> Rect {
        self.0.set(self.0.get() + 1);
        Fake.cursor(handle, offset)
    }
    fn release(&mut self, _handle: u32) {}
}

/// A backend whose cursor comes 5 px after where its text ends, to clamp.
struct CursorPastTheText;

impl TextBackend for CursorPastTheText {
    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut {
        Fake.layout(handle, text, request)
    }
    fn hit_test(&self, handle: u32, position: Offset) -> usize {
        Fake.hit_test(handle, position)
    }
    fn cursor(&self, _handle: u32, offset: usize) -> Rect {
        let x = offset as f32 * 8.0 + 5.0;
        Rect::new(x, 0.0, x, 20.0)
    }
    fn release(&mut self, _handle: u32) {}
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/CursorAnchorInfoBuilderTest.kt, testInsertionMarkerCursorClamped
#[test]
fn the_insertion_marker_is_kept_in_the_layout_not_the_field() {
    // Past the end of the layout, 16 px wide in a 300 px field: the layout's end.
    let mut host = UiHost::new(
        IntSize::new(400, 200),
        Density::new(1.0),
        Box::new(AndroidTextMeasurer::new(CursorPastTheText)),
    );
    host.set_content(|| {
        let text = remember(|| mutable_state_of(String::from("ab")));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = Modifier.size((300.dp(), 40.dp()))
        );
    });
    host.frame();
    click(&mut host, 1.0, 1.0);
    host.frame();
    let mut connections = InputConnections::default();
    let id = connections.open(&host);
    let mut connection = connections.get(id).unwrap();
    connection.set_selection(&mut host, 2, 2);
    connection.request_cursor_updates(CursorUpdates::from_mode(1, 34));
    host.frame();
    let anchor = connections.take_cursor_anchor(&host).expect("laid out");
    assert_eq!(anchor[7], 16.0);

    // A layout wider than the field: its end, past the field's, scrolled to.
    let mut keyboard = Keyboard::typed(&"x".repeat(100));
    let (_, mut connection) = keyboard.connection();
    connection.request_cursor_updates(CursorUpdates::from_mode(1, 34));
    let anchor = keyboard.connections.take_cursor_anchor(&keyboard.host).expect("laid out");
    assert_eq!(anchor[7], 800.0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, commitCorrection_returns_true_when_autoCorrect_is_on
#[test]
fn a_correction_is_taken_while_the_field_auto_corrects() {
    let mut keyboard = Keyboard::new("");
    let (ui, connection) = keyboard.connection();
    assert!(connection.commit_correction(ui));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidOnStateUpdateTest.kt, onUpdateState_resetInputCalled_whenOnlyTextChanged
#[test]
fn the_application_changing_the_text_restarts_the_input() {
    let mut keyboard = Keyboard::typed("hello");
    keyboard.value.set("bye".to_string());
    assert!(keyboard.frame(), "the input restarts");
    assert_eq!(keyboard.state().text, "bye");
    assert!(!keyboard.frame(), "once");
    // It reads anew on restarting: no update to the connection going away, nor to the next.
    assert_eq!(keyboard.take_update(), None);
    keyboard.id = keyboard.connections.open(&keyboard.host);
    assert_eq!(keyboard.take_update(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidOnStateUpdateTest.kt, onUpdateState_updateSelectionCalled_whenOnlySelectionChanged
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidOnStateUpdateTest.kt, onUpdateState_updateSelectionCalled_whenSelectionIsDifferentFromState
#[test]
fn its_edits_typing_and_cursor_moves_do_not_restart_it() {
    let mut keyboard = Keyboard::typed("hello");
    assert!(!keyboard.frame());
    keyboard.commit(" w", 1);
    assert!(!keyboard.frame());
    keyboard.delete(1, 0);
    keyboard.type_key('x');
    assert!(!keyboard.frame());
    keyboard.key(Key::DirectionLeft);
    assert!(!keyboard.frame());
    assert_eq!(keyboard.take_update(), Some([6, 6, -1, -1]));
}

/// Undo isn't an edit of the keyboard's: the text it knows is gone. (Compose's undo gives the value
/// to its processor from outside, restarting it.)
#[test]
fn an_undo_restarts_the_input_and_ends_a_composition() {
    let mut keyboard = Keyboard::typed("hello");
    keyboard.compose("x", 1);
    let shortcut = KeyModifiers {
        ctrl: !cfg!(target_os = "macos"),
        meta: cfg!(target_os = "macos"),
        ..Default::default()
    };
    keyboard.finish();
    keyboard.host.send_key(KeyEvent { modifiers: shortcut, ..KeyEvent::down(Key::Character('z')) });
    assert!(keyboard.frame());
    assert_eq!(keyboard.composition(), None);
}

/// A rejected edit is undone, and takes its composition with it: nothing of the region's stays,
/// and the input restarts to read what the application kept, as Compose's reset.
#[test]
fn a_rejected_edit_keeps_no_composition() {
    let mut keyboard = Keyboard::with("hello", false, |_| "hello".to_string());
    keyboard.region(0, 5);
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 5)));
    let (ui, mut connection) = keyboard.connection();
    assert!(connection.set_composing_text(ui, "hel", 1));
    assert_eq!(ui.input_method_value().unwrap().composition(), Some(TextRange::new(0, 3)));
    assert!(keyboard.frame(), "the input restarts");
    assert_eq!(keyboard.state().text, "hello");
    assert_eq!(keyboard.composition(), None);

    // Filtered to another text: the same.
    let mut keyboard = Keyboard::with("", false, |new| new.to_uppercase());
    keyboard.compose("ab", 1);
    assert_eq!(keyboard.value(), "AB");
    assert_eq!(keyboard.state().text, "AB");
    assert_eq!(keyboard.composition(), None);
}

/// A field taking the focus starts the input: the next field's text isn't the last one's.
#[test]
fn a_field_taking_the_focus_restarts_the_input() {
    let kind = mutable_state_of(KeyboardType::Email);
    let mut host = host(kind.handle(), Controller::default());
    let mut last = LastTextInput::default();
    click(&mut host, 10.0, 10.0);
    host.frame();
    assert!(take_text_input_state(&host, &mut last).restart);
    host.frame();
    assert!(!take_text_input_state(&host, &mut last).restart);
    // The same options, another field.
    click(&mut host, 10.0, 50.0);
    kind.set(KeyboardType::Text);
    host.frame();
    assert!(take_text_input_state(&host, &mut last).restart);
}

/// A focus change ends a composition and collapses the selection, as Compose's `onBlur`.
#[test]
fn losing_the_focus_ends_the_composition_with_its_text() {
    let kind = mutable_state_of(KeyboardType::Text);
    let mut host = host(kind.handle(), Controller::default());
    click(&mut host, 10.0, 10.0);
    host.frame();
    let mut connections = InputConnections::default();
    let id = connections.open(&host);
    let mut connection = connections.get(id).unwrap();
    connection.set_composing_text(&mut host, "ab", 1);
    host.frame();
    click(&mut host, 10.0, 50.0);
    host.frame();
    assert_eq!(host.input_method_value().unwrap().text(), "");
    click(&mut host, 10.0, 10.0);
    host.frame();
    let value = host.input_method_value().unwrap();
    assert_eq!(value.text(), "ab");
    assert_eq!(value.composition(), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, requestCursorUpdates_preT
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, requestCursorUpdates_T_filterFlags
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, requestCursorUpdates_T_noFilterFlags
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, requestCursorUpdates_atLeastU_filterFlags
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, requestCursorUpdates_atLeastU_noFilterFlags
#[test]
fn cursor_updates_read_their_filters_by_api_level() {
    const IMMEDIATE: i32 = 1;
    const MONITOR: i32 = 2;
    const EDITOR_BOUNDS: i32 = 4;
    const LINE_BOUNDS: i32 = 32;
    let updates = |mode, sdk| CursorUpdates::from_mode(mode, sdk);
    let base = CursorUpdates {
        immediate: false,
        monitor: true,
        insertion_marker: false,
        character_bounds: false,
        editor_bounds: false,
        line_bounds: false,
    };
    // Before T, no filters: the marker and characters always.
    assert_eq!(
        updates(MONITOR | EDITOR_BOUNDS, 32),
        CursorUpdates { insertion_marker: true, character_bounds: true, ..base }
    );
    assert_eq!(updates(MONITOR | EDITOR_BOUNDS, 33), CursorUpdates { editor_bounds: true, ..base });
    assert_eq!(
        updates(IMMEDIATE, 33),
        CursorUpdates {
            immediate: true,
            monitor: false,
            insertion_marker: true,
            character_bounds: true,
            editor_bounds: true,
            ..base
        }
    );
    assert_eq!(updates(MONITOR | LINE_BOUNDS, 34), CursorUpdates { line_bounds: true, ..base });
    assert!(!updates(MONITOR | LINE_BOUNDS, 33).line_bounds);
    assert_eq!(
        updates(MONITOR, 34),
        CursorUpdates {
            insertion_marker: true,
            character_bounds: true,
            editor_bounds: true,
            line_bounds: true,
            ..base
        }
    );
}

const ALL_CURSOR_UPDATES: i32 = 1 | 4 | 8 | 16 | 32;

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidCursorAnchorInfoTest.kt, requestCursorUpdates_immediate
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidCursorAnchorInfoTest.kt, requestCursorUpdates_immediate_beforeUpdateTextLayoutResult
#[test]
fn a_cursor_update_asked_for_once_comes_once_with_the_layout() {
    let mut keyboard = Keyboard::typed("abc");
    keyboard.compose("de", 1);
    keyboard.take_update();
    let (_, mut connection) = keyboard.connection();
    connection.request_cursor_updates(CursorUpdates::from_mode(ALL_CURSOR_UPDATES, 34));
    assert!(keyboard.connections.has_update(&keyboard.host));
    let anchor = keyboard.connections.take_cursor_anchor(&keyboard.host).expect("laid out");
    // The selection, then the composition, in UTF-16 units.
    assert_eq!(&anchor[2..6], &[5.0, 5.0, 3.0, 5.0]);
    // The marker: 8 px a unit, one 20 px line.
    assert_eq!(&anchor[6..10], &[1.0, 40.0, 0.0, 20.0]);
    assert_eq!(anchor[10], 1.0, "visible");
    // Two characters: 3 and 4, 8 px each.
    assert_eq!(anchor[16], 2.0);
    let first = &anchor[CURSOR_ANCHOR_HEADER..CURSOR_ANCHOR_HEADER + 6];
    assert_eq!(first, &[3.0, 24.0, 0.0, 32.0, 20.0, 1.0]);
    let second = &anchor[CURSOR_ANCHOR_HEADER + 6..CURSOR_ANCHOR_HEADER + 12];
    assert_eq!(second[0], 4.0);
    // One line shown.
    assert_eq!(anchor[17], 1.0);
    // Once: nothing further without monitoring.
    assert!(!keyboard.connections.has_update(&keyboard.host));
    assert_eq!(keyboard.connections.take_cursor_anchor(&keyboard.host), None);

    // Asked for before a new layout: it waits for one.
    let mut keyboard = Keyboard::typed("abc");
    let (ui, mut connection) = keyboard.connection();
    assert!(connection.commit_text(ui, "d", 1));
    connection.request_cursor_updates(CursorUpdates::from_mode(1, 34));
    assert_eq!(keyboard.connections.take_cursor_anchor(&keyboard.host), None);
    assert!(keyboard.connections.has_update(&keyboard.host));
    keyboard.frame();
    assert!(keyboard.connections.take_cursor_anchor(&keyboard.host).is_some());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidCursorAnchorInfoTest.kt, requestCursorUpdates_monitor
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/TextInputServiceAndroidCursorAnchorInfoTest.kt, requestCursorUpdates_cancel
#[test]
fn monitored_cursor_updates_come_on_each_change_until_cancelled() {
    let mut keyboard = Keyboard::typed("abc");
    let (_, mut connection) = keyboard.connection();
    connection.request_cursor_updates(CursorUpdates::from_mode(2, 34));
    assert!(keyboard.connections.take_cursor_anchor(&keyboard.host).is_some());
    assert_eq!(keyboard.connections.take_cursor_anchor(&keyboard.host), None, "nothing new");
    keyboard.select(1, 1);
    let anchor = keyboard.connections.take_cursor_anchor(&keyboard.host);
    assert_eq!(anchor.expect("the new place")[7], 8.0);
    let (_, mut connection) = keyboard.connection();
    connection.request_cursor_updates(CursorUpdates::from_mode(0, 34));
    keyboard.select(2, 2);
    assert_eq!(keyboard.connections.take_cursor_anchor(&keyboard.host), None);
}

thread_local! {
    static CUSTOM_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn custom_log(entry: String) {
    CUSTOM_LOG.with(|log| log.borrow_mut().push(entry));
}

#[derive(Clone, PartialEq)]
struct CustomField;

struct CustomFieldNode;

impl ModifierNodeElement for CustomField {
    type Node = CustomFieldNode;
    fn create(&self) -> CustomFieldNode {
        CustomFieldNode
    }
    fn update(&self, _: &mut CustomFieldNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for CustomFieldNode {
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
}

impl FocusTargetModifierNode for CustomFieldNode {}

impl KeyInputModifierNode for CustomFieldNode {
    fn on_key_event(&mut self, event: &KeyEvent, _: &mut EventContext<'_>) -> bool {
        if event.kind == compose_ui::KeyEventType::KeyDown {
            custom_log(format!("key {:?}", event.key));
        }
        true
    }
    fn on_text_input(&mut self, text: &str, _: &mut EventContext<'_>) -> bool {
        custom_log(format!("text {text}"));
        true
    }
    fn on_preedit(
        &mut self,
        text: &str,
        _: Option<(usize, usize)>,
        _: &mut EventContext<'_>,
    ) -> bool {
        custom_log(format!("preedit {text}"));
        true
    }
    fn text_input_area(&self) -> Option<Rect> {
        Some(Rect::new(0.0, 0.0, 1.0, 1.0))
    }
}

/// M2: a custom `KeyInputModifierNode` has no value to convert against: it gets committed text, the
/// composition shown, and deletions as keys, as before the connection knew of values.
#[test]
fn a_node_without_a_value_gets_text_preedits_and_keys() {
    CUSTOM_LOG.with(|log| log.borrow_mut().clear());
    let mut host = UiHost::new(
        IntSize::new(200, 200),
        Density::new(1.0),
        Box::new(AndroidTextMeasurer::new(Fake)),
    );
    host.set_content(|| {
        Box!(modifier = Modifier.size(100.dp()).then_custom(CustomField));
    });
    host.frame();
    let root = host.with_tree(|tree| tree.focus_order().first().copied());
    host.request_focus(root.expect("a focus target"));
    host.frame();
    assert!(host.input_method_value().is_none());
    let mut connections = InputConnections::default();
    let id = connections.open(&host);
    let mut connection = connections.get(id).unwrap();
    assert!(connection.commit_text(&mut host, "a", 1));
    assert!(connection.delete_surrounding_text(&mut host, 2, 1));
    assert!(connection.set_composing_text(&mut host, "ni", 1));
    assert!(connection.finish_composing_text(&mut host));
    assert!(connection.finish_composing_text(&mut host));
    assert!(connection.set_composing_text(&mut host, "x", 1));
    assert!(connection.commit_text(&mut host, "y", 1));
    assert!(connection.set_composing_region(&mut host, 0, 1));
    assert!(connection.set_selection(&mut host, 0, 1));
    assert!(connection.begin_batch_edit());
    assert!(connection.commit_text(&mut host, "z", 1));
    assert!(!connection.end_batch_edit(&mut host));
    assert_eq!(connections.take_selection_update(&host), None);
    CUSTOM_LOG.with(|log| {
        assert_eq!(
            *log.borrow(),
            [
                "text a",
                "key Backspace",
                "key Backspace",
                "key Delete",
                "preedit ni",
                "preedit ",
                "text ni",
                "preedit x",
                "preedit ",
                "text y",
                "text z",
            ]
        );
    });
}

/// A read-only field takes no edits, and has nothing to read.
#[test]
fn a_read_only_field_takes_no_edits_from_the_keyboard() {
    let mut keyboard = Keyboard::with("abc", true, |new| new);
    assert!(keyboard.host.input_method_value().is_none());
    let (ui, mut connection) = keyboard.connection();
    connection.commit_text(ui, "x", 1);
    connection.delete_surrounding_text(ui, 1, 0);
    connection.set_composing_region(ui, 0, 1);
    connection.finish_composing_text(ui);
    keyboard.frame();
    assert_eq!(keyboard.value(), "abc");
    assert_eq!(keyboard.edits.get(), 0);
}

/// Any sequence of the keyboard's edits, over text of 1 to 4 bytes a character and 1 or 2 units,
/// keeps the selection and composition at character boundaries of the value, which the application
/// has, and the keyboard's next update telling it where they are.
#[test]
fn random_edits_from_the_keyboard_stay_at_character_boundaries() {
    let pieces = ["a", "\u{e9}", "\u{1F600}", "e\u{301}", "\n", "", "\u{4F60}\u{597D}"];
    let mut seed = 0x2545_F491u32;
    let mut random = move |below: i32| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % below as u32) as i32
    };
    for _ in 0..40 {
        let mut keyboard = Keyboard::typed("h\u{e9}\u{1F600}llo");
        for _ in 0..60 {
            let piece = pieces[random(pieces.len() as i32) as usize];
            match random(10) {
                0 => keyboard.commit(piece, random(9) - 4),
                1 => keyboard.delete(random(5), random(5)),
                2 => keyboard.region(random(14) - 1, random(14) - 1),
                3 => keyboard.compose(piece, random(9) - 4),
                4 => keyboard.finish(),
                5 => keyboard.select(random(14) - 1, random(14) - 1),
                6 => keyboard.key(Key::Backspace),
                7 => keyboard.key(Key::DirectionLeft),
                8 => keyboard.type_key('x'),
                _ => {
                    let new = format!("{}{piece}", keyboard.value());
                    keyboard.value.set(new);
                    keyboard.frame();
                }
            }
            let state = keyboard.state();
            assert_eq!(state.text, keyboard.value());
            let text = &state.text;
            let range = state.selection;
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            if let Some(range) = state.composition {
                assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            }
            if let Some(update) = keyboard.connections.take_selection_update(&keyboard.host) {
                let [start, end, composition_start, _] =
                    input_state_units(&keyboard.host.input_method_value().unwrap());
                assert_eq!(
                    (update.selection_start, update.selection_end, update.composition_start),
                    (start, end, composition_start)
                );
            }
        }
    }
}

// Survivors of the review's mutations.

/// `getTextAfterCursor`'s count is UTF-16 units, not characters: a pair is 2.
#[test]
fn text_after_the_cursor_counts_utf16_units() {
    let mut keyboard = Keyboard::typed("\u{1F600}ab");
    keyboard.select(0, 0);
    let value = keyboard.host.input_method_value().unwrap();
    assert_eq!(text_after_cursor(&value, 1), "", "half a pair isn't read");
    assert_eq!(text_after_cursor(&value, 2), "\u{1F600}");
    assert_eq!(text_after_cursor(&value, 3), "\u{1F600}a");
}

/// With a composition a selection stays, and the lengths go around it (D1665 is for no composition).
#[test]
fn with_a_composition_a_surrounding_deletion_leaves_the_selection() {
    let mut keyboard = Keyboard::typed("hello");
    keyboard.region(0, 5);
    keyboard.select(1, 2);
    keyboard.delete(1, 0);
    assert_eq!(keyboard.value(), "ello");
    assert_eq!(keyboard.selection(), TextRange::new(0, 1));
    assert_eq!(keyboard.composition(), Some(TextRange::new(0, 4)));
}

/// A line feed from a keyboard isn't kept on a single line, committed or composed.
#[test]
fn a_line_feed_from_the_keyboard_is_dropped_on_a_single_line() {
    let kind = mutable_state_of(KeyboardType::Text);
    let mut host = host(kind.handle(), Controller::default());
    click(&mut host, 10.0, 10.0);
    host.frame();
    let mut connections = InputConnections::default();
    let id = connections.open(&host);
    let mut connection = connections.get(id).unwrap();
    connection.commit_text(&mut host, "a\nb", 1);
    connection.set_composing_text(&mut host, "c\nd", 1);
    host.frame();
    assert_eq!(host.input_method_value().unwrap().text(), "abcd");
    // Multi-line: kept.
    let mut keyboard = Keyboard::new("");
    keyboard.commit("a\nb", 1);
    assert_eq!(keyboard.value(), "a\nb");
}

fn shortcut_key(c: char) -> KeyEvent {
    let modifiers = KeyModifiers {
        ctrl: !cfg!(target_os = "macos"),
        meta: cfg!(target_os = "macos"),
        ..Default::default()
    };
    KeyEvent { modifiers, ..KeyEvent::down(Key::Character(c)) }
}

/// Cut and paste give their value from outside the keyboard's edits, restarting it, as Compose's.
#[test]
fn cut_and_paste_restart_the_input() {
    let mut keyboard = Keyboard::typed("hello");
    keyboard.select(0, 5);
    assert!(!keyboard.frame());
    keyboard.host.send_key(shortcut_key('x'));
    assert!(keyboard.frame(), "cut");
    assert_eq!(keyboard.value(), "");
    assert!(!keyboard.frame());
    keyboard.host.send_key(shortcut_key('v'));
    assert!(keyboard.frame(), "paste");
    assert_eq!(keyboard.value(), "hello");
}

fn undo_key() -> KeyEvent {
    shortcut_key('z')
}

/// A composition begun on the text as it was (a region): undo brings that text back, not what came
/// before it. Shorter, the same length, or longer than the region; composed or committed.
#[test]
fn undo_after_composing_over_existing_text_brings_it_back() {
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 11);
    keyboard.compose("worl", 1);
    keyboard.host.send_key(undo_key());
    keyboard.frame();
    assert_eq!(keyboard.value(), "hello world");

    let mut keyboard = Keyboard::typed("hello there");
    keyboard.region(0, 5);
    keyboard.commit("Hello", 1);
    assert_eq!(keyboard.value(), "Hello there");
    keyboard.host.send_key(undo_key());
    keyboard.frame();
    assert_eq!(keyboard.value(), "hello there");

    // Longer: typing before it doesn't join it.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 11);
    keyboard.compose("worlds", 1);
    keyboard.compose("worldsx", 1);
    keyboard.host.send_key(undo_key());
    keyboard.frame();
    assert_eq!(keyboard.value(), "hello world");

    // Finished as it was: no step, and the next typing joins as before it.
    let mut keyboard = Keyboard::typed("ab");
    keyboard.region(0, 2);
    keyboard.finish();
    keyboard.compose("c", 1);
    keyboard.host.send_key(undo_key());
    keyboard.frame();
    assert_eq!(keyboard.value(), "ab");
}

/// Two nodes without a value, of the same options: a focus move restarts the input all the same,
/// or the keyboard's connection stays the first node's and its typing goes nowhere (D1670).
#[test]
fn a_focus_move_between_nodes_without_a_value_restarts_the_input() {
    let mut host = two_custom_fields();
    let mut last = LastTextInput::default();
    let order = host.with_tree(|tree| tree.focus_order());
    host.request_focus(order[0]);
    host.frame();
    take_text_input_state(&host, &mut last);
    host.frame();
    assert!(!take_text_input_state(&host, &mut last).restart);
    host.request_focus(order[1]);
    host.frame();
    assert!(take_text_input_state(&host, &mut last).restart);
    host.frame();
    assert!(!take_text_input_state(&host, &mut last).restart);
}

fn two_custom_fields() -> UiHost {
    let mut host = UiHost::new(
        IntSize::new(200, 200),
        Density::new(1.0),
        Box::new(AndroidTextMeasurer::new(Fake)),
    );
    host.set_content(|| {
        Column!({
            Box!(modifier = Modifier.size(50.dp()).then_custom(CustomField));
            Box!(modifier = Modifier.size(50.dp()).then_custom(CustomField));
        });
    });
    host.frame();
    host
}

/// What one node without a value composed isn't the next one's: not committed there, nor cancelled
/// there (D1670).
#[test]
fn a_composition_for_a_node_without_a_value_does_not_cross_to_the_next() {
    CUSTOM_LOG.with(|log| log.borrow_mut().clear());
    let mut host = two_custom_fields();
    let order = host.with_tree(|tree| tree.focus_order());
    host.request_focus(order[0]);
    host.frame();
    let mut connections = InputConnections::default();
    let first = connections.open(&host);
    connections.get(first).unwrap().set_composing_text(&mut host, "ni", 1);
    host.request_focus(order[1]);
    host.frame();
    let second = connections.open(&host);
    let mut connection = connections.get(second).unwrap();
    connection.finish_composing_text(&mut host);
    connection.commit_text(&mut host, "a", 1);
    CUSTOM_LOG.with(|log| assert_eq!(*log.borrow(), ["preedit ni", "text a"]));
}

/// The token of a connection following the text ends with it: another connection's update carries
/// none.
#[test]
fn the_extracted_text_token_ends_with_its_connection() {
    let mut keyboard = Keyboard::typed("hi");
    let (_, mut connection) = keyboard.connection();
    connection.set_extracted_text_monitor(Some(7));
    let other = keyboard.connections.open(&keyboard.host);
    // While it's open, an update carries it, the other connection's edits' too.
    keyboard.connections.get(other).unwrap().commit_text(&mut keyboard.host, "!", 1);
    let update = keyboard.connections.take_selection_update(&keyboard.host).unwrap();
    assert_eq!(update.extracted_text_token, Some(7));
    // Closed: none.
    keyboard.connections.close(keyboard.id);
    keyboard.connections.get(other).unwrap().commit_text(&mut keyboard.host, "?", 1);
    let update = keyboard.connections.take_selection_update(&keyboard.host).unwrap();
    assert_eq!(update.extracted_text_token, None);
}

/// A connection whose node lost the focus isn't for what's focused: its keys and actions, which
/// Kotlin gates on this, would act on the next field (D1670).
#[test]
fn a_connections_action_after_the_focus_moved_is_dropped() {
    let kind = mutable_state_of(KeyboardType::Text);
    let mut host = host(kind.handle(), Controller::default());
    click(&mut host, 10.0, 10.0);
    host.frame();
    let mut connections = InputConnections::default();
    let first = connections.open(&host);
    assert!(connections.get(first).unwrap().is_for_focused_node(&host));
    click(&mut host, 10.0, 50.0);
    host.frame();
    let mut connection = connections.get(first).unwrap();
    assert!(!connection.is_for_focused_node(&host));
    assert!(connection.is_active(), "still open: Android's to close");
    connection.perform_editor_action(&mut host, editor_info::IME_ACTION_DONE);
    assert_eq!(host.take_software_keyboard_request(), None);
    let second = connections.open(&host);
    assert!(connections.get(second).unwrap().is_for_focused_node(&host));
    connections.get(second).unwrap().perform_editor_action(&mut host, editor_info::IME_ACTION_DONE);
    assert_eq!(host.take_software_keyboard_request(), Some(false));
}

// ---- A word being composed, and a window opening over its field (D1286) ---------------------

/// A field with `ab`, a keyboard on it, and a dialog on `open`, with a field of its own that takes
/// the focus as it opens if `with_a_field`.
struct OverADialog {
    host: UiHost,
    connections: InputConnections,
    id: i64,
    value: MutableState<String>,
    inside: MutableState<String>,
    open: MutableState<bool>,
    options: LastTextInput,
}

impl OverADialog {
    fn new(with_a_field: bool) -> OverADialog {
        let value = Box::leak(Box::new(mutable_state_of(String::from("ab")))).handle();
        let inside = Box::leak(Box::new(mutable_state_of(String::new()))).handle();
        let open = Box::leak(Box::new(mutable_state_of(false))).handle();
        let mut host = UiHost::new(
            IntSize::new(400, 400),
            Density::new(1.0),
            Box::new(AndroidTextMeasurer::new(Fake)),
        );
        host.set_content(move || {
            Column!({
                BasicTextField!(
                    value.get(),
                    move |new: String| value.set(new),
                    modifier = Modifier.size((300.dp(), 40.dp()))
                );
                if open.get() {
                    Dialog!(on_dismiss_request = move || open.set(false), {
                        if with_a_field {
                            let requester = remember(compose_ui::FocusRequester::new);
                            compose_runtime::LaunchedEffect((), async move {
                                requester.request_focus();
                            });
                            BasicTextField!(
                                inside.get(),
                                move |new: String| inside.set(new),
                                modifier =
                                    Modifier.size((100.dp(), 40.dp())).focus_requester(requester)
                            );
                        } else {
                            Box!(modifier = Modifier.size(20.dp()), {});
                        }
                    });
                }
            });
        });
        host.frame();
        click(&mut host, 1.0, 1.0);
        host.frame();
        host.send_key(KeyEvent::down(Key::MoveEnd));
        let mut connections = InputConnections::default();
        let id = connections.open(&host);
        let mut scene = OverADialog {
            host,
            connections,
            id,
            value,
            inside,
            open,
            options: LastTextInput::default(),
        };
        scene.frames();
        scene
    }

    /// Frames until it's all settled; returns whether the input method was told to restart, and
    /// whether a field is focused for it.
    fn frames(&mut self) -> (bool, bool) {
        let mut restart = false;
        let mut active = false;
        for _ in 0..6 {
            self.host.frame();
            compose_runtime::platform::run_ready();
            let state = take_text_input_state(&self.host, &mut self.options);
            restart |= state.restart;
            active = state.active;
            self.connections.frame_has_update(&self.host, state.restart);
        }
        (restart, active)
    }

    fn compose(&mut self, text: &str) {
        let mut connection = self.connections.get(self.id).expect("open");
        assert!(connection.set_composing_text(&mut self.host, text, 1));
        self.frames();
    }

    /// What a keyboard sends over connection `id`.
    fn commit_over(&mut self, id: i64, text: &str) {
        if let Some(mut connection) = self.connections.get(id) {
            connection.commit_text(&mut self.host, text, 1);
        }
        self.frames();
    }

    /// The new connection Android asks for after a restart.
    fn reconnect(&mut self) -> i64 {
        let id = self.connections.open(&self.host);
        self.id = id;
        id
    }
}

#[test]
fn a_word_being_composed_stays_in_the_text_as_a_dialog_opens_and_typing_goes_on_after_it() {
    let mut scene = OverADialog::new(false);
    scene.compose("hel");
    assert_eq!(scene.value.get(), "abhel", "the composition is in the value");

    scene.open.set(true);
    let (restart, active) = scene.frames();
    assert!(restart, "the keyboard starts over");
    assert!(!active, "no field takes text in the dialog");
    assert_eq!(scene.value.get(), "abhel");

    // A keyboard that hadn't heard yet finishes its word: there's nothing focused to take it.
    let old = scene.id;
    scene.commit_over(old, "hel");
    assert_eq!(scene.value.get(), "abhel", "not typed again behind the dialog");

    scene.open.set(false);
    let (restart, active) = scene.frames();
    assert!(restart);
    assert!(active);
    assert_eq!(scene.host.input_method_value().expect("the field").composition(), None);
    let new = scene.reconnect();
    scene.commit_over(new, "x");
    assert_eq!(scene.value.get(), "abhelx");
}

#[test]
fn a_word_being_composed_stays_out_of_a_dialogs_field_with_the_same_options() {
    let mut scene = OverADialog::new(true);
    scene.compose("hel");
    let old = scene.id;

    scene.open.set(true);
    let (restart, active) = scene.frames();
    assert!(restart, "another node with the same options");
    assert!(active);
    assert_eq!(scene.value.get(), "abhel");

    // What the old connection still sends isn't the dialog's field's.
    scene.commit_over(old, "helx");
    assert_eq!(scene.inside.get(), "");
    assert_eq!(scene.value.get(), "abhel");

    let new = scene.reconnect();
    scene.commit_over(new, "y");
    assert_eq!(scene.inside.get(), "y");
    assert_eq!(scene.value.get(), "abhel");
}

#[test]
fn a_word_being_composed_in_a_dialog_stays_there_as_it_closes() {
    let mut scene = OverADialog::new(true);
    scene.open.set(true);
    scene.frames();
    scene.reconnect();
    scene.compose("hi");
    assert_eq!(scene.inside.get(), "hi");

    let old = scene.id;
    scene.open.set(false);
    let (restart, active) = scene.frames();
    assert!(restart);
    assert!(active, "the field behind has the keys again");
    scene.commit_over(old, "hi");
    assert_eq!(scene.value.get(), "ab", "the dialog's word isn't the field's behind it");

    let new = scene.reconnect();
    scene.commit_over(new, "x");
    assert_eq!(scene.value.get(), "abx");
}
