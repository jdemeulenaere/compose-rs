//! What a frame tells `ComposeRsView` of the soft keyboard (`input_method.rs`, D250): the
//! focused field's `EditorInfo`, the input restarting when its options change, its action key,
//! and the content's requests to show or hide the keyboard.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_android::input_method::{
    TextInputState, commit_text, delete_surrounding_text, editor_info, finish_composing_text,
    ime_action, input_type, set_composing_region, take_text_input_state,
};
use compose_android::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{MutableState, mutable_state_of, remember};
use compose_ui::{
    Density, ImeAction, ImeOptions, IntSize, Key, KeyEvent, KeyModifiers, KeyboardType,
    LOCAL_SOFTWARE_KEYBOARD_CONTROLLER, Offset, PointerEvent, Rect, SoftwareKeyboardController,
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
    let mut last = None;
    assert_eq!(take_text_input_state(&host, &mut last), TextInputState::default());

    click(&mut host, 10.0, 10.0);
    host.frame();
    let state = take_text_input_state(&host, &mut last);
    assert!(state.active && !state.restart, "{state:?}");
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
    let mut last = None;
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

// The keyboard's edits: UTF-16 positions and lengths, as `ComposeRsInputConnection` passes them.

/// A focused multi-line field's host, the field's value, and how many edits the application heard.
struct Keyboard {
    host: UiHost,
    value: compose_runtime::OwnedState<String>,
    edits: Rc<Cell<u32>>,
}

impl Keyboard {
    fn new(initial: &str, read_only: bool) -> Keyboard {
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
                    state.set(new);
                },
                modifier = Modifier.size((300.dp(), 40.dp())),
                read_only = read_only
            );
        });
        host.frame();
        click(&mut host, 1.0, 1.0);
        host.frame();
        Keyboard { host, value, edits }
    }

    fn typed(initial: &str) -> Keyboard {
        let mut keyboard = Keyboard::new("", false);
        commit_text(&mut keyboard.host, initial, 1);
        keyboard.host.frame();
        keyboard.edits.set(0);
        keyboard
    }

    fn value(&self) -> String {
        self.value.get()
    }

    fn shown(&self) -> String {
        self.host.input_method_value().expect("a focused field").text().to_string()
    }

    fn selection(&self) -> TextRange {
        self.host.input_method_value().expect("a focused field").selection()
    }

    fn composition(&self) -> Option<TextRange> {
        self.host.input_method_value().expect("a focused field").composition()
    }

    fn commit(&mut self, text: &str, new_cursor_position: i32) {
        assert!(commit_text(&mut self.host, text, new_cursor_position));
        self.host.frame();
    }

    fn delete(&mut self, before: i32, after: i32) {
        assert!(delete_surrounding_text(&mut self.host, before, after));
        self.host.frame();
    }

    fn region(&mut self, start: i32, end: i32) {
        assert!(set_composing_region(&mut self.host, start, end));
        self.host.frame();
    }

    fn compose(&mut self, text: &str) {
        assert!(self.host.send_preedit(text, None));
        self.host.frame();
    }

    fn finish(&mut self) {
        assert!(finish_composing_text(&mut self.host));
        self.host.frame();
    }

    /// Shift+Left, selecting backward from the cursor.
    fn select_back(&mut self) {
        let modifiers = KeyModifiers { shift: true, ..KeyModifiers::default() };
        self.host.send_key(KeyEvent { modifiers, ..KeyEvent::down(Key::DirectionLeft) });
        self.host.frame();
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, commitTextTest
#[test]
fn committed_text_goes_in_with_the_cursor_after_it() {
    let mut keyboard = Keyboard::new("", false);
    keyboard.commit("Hello, ", 1);
    assert_eq!(keyboard.value(), "Hello, ");
    assert_eq!(keyboard.selection(), TextRange::at(7));
    keyboard.commit("World.", 1);
    assert_eq!(keyboard.value(), "Hello, World.");
    assert_eq!(keyboard.selection(), TextRange::at(13));
    assert_eq!(keyboard.edits.get(), 2);
}

/// `commitText`'s new cursor position counts UTF-16 units from the text, as Android's: a surrogate
/// pair is 2 of them and 4 bytes, and a cursor inside one goes to its start (D1100, D1113).
#[test]
fn committed_text_puts_the_cursor_where_the_keyboard_says_in_utf16_units() {
    let mut keyboard = Keyboard::typed("a\u{1F600}b");
    // Before the text, then one unit further back: before the "b".
    keyboard.commit("X", -1);
    assert_eq!(keyboard.value(), "a\u{1F600}bX");
    assert_eq!(keyboard.selection(), TextRange::at(5));
    // Nothing committed, two units back: before the emoji, not inside it.
    keyboard.commit("", -2);
    assert_eq!(keyboard.value(), "a\u{1F600}bX");
    assert_eq!(keyboard.selection(), TextRange::at(1));
    // After the text, then two units further: after the emoji.
    keyboard.commit("Y", 3);
    assert_eq!(keyboard.value(), "aY\u{1F600}bX");
    assert_eq!(keyboard.selection(), TextRange::at(6));
    // One unit back lands inside the emoji: its start.
    keyboard.commit("", -1);
    assert_eq!(keyboard.selection(), TextRange::at(2));
    // After the text, one unit further lands inside the emoji: its start.
    keyboard.commit("Z", 2);
    assert_eq!(keyboard.value(), "aYZ\u{1F600}bX");
    assert_eq!(keyboard.selection(), TextRange::at(3));
    // Past either end: kept in the text.
    keyboard.commit("", 100);
    assert_eq!(keyboard.selection(), TextRange::at(keyboard.value().len()));
    keyboard.commit("", -100);
    assert_eq!(keyboard.selection(), TextRange::at(0));
}

/// A cursor moved without text is no edit: the application hears nothing and undo has no step.
#[test]
fn a_commit_that_only_moves_the_cursor_is_no_edit() {
    let mut keyboard = Keyboard::typed("abc");
    keyboard.commit("", -1);
    assert_eq!(keyboard.selection(), TextRange::at(2));
    assert_eq!(keyboard.edits.get(), 0);
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
}

/// The lengths are UTF-16 units, each surrogate pair two of them, and half a pair deletes it whole
/// (D1100); a mark goes without its letter, where a Backspace key takes the cluster.
#[test]
fn surrounding_lengths_are_utf16_units_of_whole_characters() {
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
    assert_eq!(keyboard.selection(), TextRange::at(0));

    let mut keyboard = Keyboard::typed("e\u{301}");
    keyboard.delete(1, 0);
    assert_eq!(keyboard.value(), "e");

    // Past the text: to its ends.
    let mut keyboard = Keyboard::typed("abc");
    keyboard.commit("", -1);
    keyboard.delete(100, 100);
    assert_eq!(keyboard.value(), "");
    // Negative lengths delete nothing, as Android's `BaseInputConnection`.
    let mut keyboard = Keyboard::typed("abc");
    keyboard.delete(-1, -1);
    assert_eq!(keyboard.value(), "abc");
    assert_eq!(keyboard.edits.get(), 0);
}

/// Around the selection, which stays selected, as Compose's command.
#[test]
fn a_surrounding_deletion_leaves_the_selection() {
    let mut keyboard = Keyboard::typed("abcde");
    keyboard.commit("", -1);
    keyboard.select_back();
    assert_eq!(keyboard.selection(), TextRange::new(4, 3));
    keyboard.delete(1, 1);
    assert_eq!(keyboard.value(), "abd");
    assert_eq!(keyboard.selection(), TextRange::new(3, 2));
}

/// A composition isn't in the text (D117): lengths go around it, as Android's
/// `BaseInputConnection`, and it stays.
#[test]
fn a_surrounding_deletion_goes_around_a_composition() {
    let mut keyboard = Keyboard::typed("abcd");
    keyboard.commit("", -1);
    keyboard.compose("ni");
    assert_eq!(keyboard.shown(), "abcnid");
    keyboard.delete(1, 1);
    assert_eq!(keyboard.value(), "ab");
    assert_eq!(keyboard.shown(), "abni");
    assert_eq!(keyboard.composition(), Some(TextRange::new(2, 4)));
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

/// What the keyboard composes over a region takes its place when it commits; until then the
/// value keeps the region's text (D117), and setting another region commits it first.
#[test]
fn composing_over_a_region_replaces_it_on_commit() {
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 11);
    keyboard.compose("earth");
    assert_eq!(keyboard.shown(), "hello earth");
    assert_eq!(keyboard.value(), "hello world");
    assert_eq!(keyboard.selection(), TextRange::at(11));
    keyboard.finish();
    assert_eq!(keyboard.value(), "hello earth");
    assert_eq!(keyboard.selection(), TextRange::at(11));
    assert_eq!(keyboard.composition(), None);

    // Shorter than the region, the cursor after it; another region commits it first.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(0, 5);
    keyboard.compose("hi");
    assert_eq!(keyboard.selection(), TextRange::at(2));
    keyboard.region(3, 8);
    assert_eq!(keyboard.value(), "hi world");
    assert_eq!(keyboard.composition(), Some(TextRange::new(3, 8)));

    // Committed text takes the region's place, the composition's too.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 11);
    keyboard.commit("there", 1);
    assert_eq!(keyboard.value(), "hello there");
    assert_eq!(keyboard.selection(), TextRange::at(11));
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(0, 5);
    keyboard.compose("x");
    keyboard.commit("bye", 1);
    assert_eq!(keyboard.value(), "bye world");
    assert_eq!(keyboard.selection(), TextRange::at(3));

    // Nothing composed over it, finished: no edit, and the cursor stays.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(0, 5);
    keyboard.finish();
    assert_eq!(keyboard.value(), "hello world");
    assert_eq!(keyboard.selection(), TextRange::at(11));
    assert_eq!(keyboard.edits.get(), 0);

    // Empty composing text deletes the region, as Compose's `SetComposingTextCommand`.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(5, 11);
    keyboard.compose("");
    assert_eq!(keyboard.value(), "hello");
    assert_eq!(keyboard.composition(), None);

    // Lengths go around a region; it moves with the text.
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 8);
    keyboard.compose("WO");
    keyboard.delete(1, 1);
    assert_eq!(keyboard.value(), "hellowold");
    assert_eq!(keyboard.shown(), "helloWOld");
    assert_eq!(keyboard.composition(), Some(TextRange::new(5, 7)));
    keyboard.finish();
    assert_eq!(keyboard.value(), "helloWOld");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/RecordingInputConnectionTest.kt, finishComposingText
#[test]
fn finishing_keeps_the_composition_and_a_second_finish_does_nothing() {
    let mut keyboard = Keyboard::typed("ab");
    keyboard.compose("cd");
    assert_eq!(keyboard.value(), "ab");
    keyboard.finish();
    assert_eq!(keyboard.value(), "abcd");
    assert_eq!(keyboard.selection(), TextRange::at(4));
    assert_eq!(keyboard.edits.get(), 1);
    keyboard.finish();
    assert_eq!(keyboard.value(), "abcd");
    assert_eq!(keyboard.edits.get(), 1);
}

/// Committed text ends a composition without committing it, as Compose's command: it's replaced.
#[test]
fn committed_text_replaces_a_composition() {
    let mut keyboard = Keyboard::typed("ab");
    keyboard.compose("xyz");
    keyboard.commit("cd", 1);
    assert_eq!(keyboard.value(), "abcd");
    assert_eq!(keyboard.selection(), TextRange::at(4));
    assert_eq!(keyboard.composition(), None);
}

/// An application changing the text ends a region composition: its offsets mean nothing.
#[test]
fn an_application_changing_the_text_ends_a_composing_region() {
    let mut keyboard = Keyboard::typed("hello world");
    keyboard.region(6, 11);
    keyboard.value.set("bye".to_string());
    keyboard.host.frame();
    assert_eq!(keyboard.composition(), None);
    assert_eq!(keyboard.shown(), "bye");
}

/// A field that takes no edits gives the keyboard's nothing to convert against, and takes none.
#[test]
fn a_read_only_field_takes_no_edits_from_the_keyboard() {
    let mut keyboard = Keyboard::new("abc", true);
    assert!(keyboard.host.input_method_value().is_none());
    assert!(!commit_text(&mut keyboard.host, "x", 1));
    assert!(!delete_surrounding_text(&mut keyboard.host, 1, 0));
    assert!(!set_composing_region(&mut keyboard.host, 0, 1));
    assert!(!finish_composing_text(&mut keyboard.host));
    keyboard.host.frame();
    assert_eq!(keyboard.value(), "abc");
}

/// Any sequence of the keyboard's edits, over text of 1 to 4 bytes a character and 1 or 2 units,
/// keeps the selection and composition at character boundaries of what's shown, and leaves the
/// value as what's shown once the composition is finished.
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
            match random(7) {
                0 => keyboard.commit(piece, random(9) - 4),
                1 => keyboard.delete(random(5), random(5)),
                2 => keyboard.region(random(14) - 1, random(14) - 1),
                3 => keyboard.compose(piece),
                4 => keyboard.finish(),
                5 => keyboard.select_back(),
                _ => {
                    let new = format!("{}{piece}", keyboard.value());
                    keyboard.value.set(new);
                    keyboard.host.frame();
                }
            }
            let value = keyboard.host.input_method_value().expect("a focused field");
            let text = value.text();
            let range = value.selection();
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            if let Some(range) = value.composition() {
                assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            }
        }
        keyboard.finish();
        assert_eq!(keyboard.shown(), keyboard.value());
    }
}
