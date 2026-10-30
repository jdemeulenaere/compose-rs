//! `BasicTextField`'s keyboard options and actions, and visual transformations (passwords).

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{mutable_state_of, remember};
use compose_test::*;
use compose_ui::{Key, KeyEvent, KeyEventType, KeyModifiers, UnitExt};

fn shortcut(ui: &mut ComposeTestRule, c: char) {
    let modifiers = if cfg!(target_os = "macos") {
        KeyModifiers { meta: true, ..Default::default() }
    } else {
        KeyModifiers { ctrl: true, ..Default::default() }
    };
    let key = Key::Character(c);
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyDown, modifiers });
    ui.host().send_key(KeyEvent { key, kind: KeyEventType::KeyUp, modifiers });
    ui.wait_for_idle();
}

fn password_field(value: Rc<RefCell<String>>) -> ComposeTestRule {
    compose_test(move || {
        let value = value.clone();
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            text.get(),
            move |new: String| {
                *value.borrow_mut() = new.clone();
                text.set(new);
            },
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            single_line = true,
            visual_transformation = PasswordVisualTransformation::new()
        );
    })
}

#[test]
fn a_password_shows_masked_and_says_so() {
    let value = Rc::new(RefCell::new(String::new()));
    let mut ui = password_field(value.clone());
    ui.on_node_with_tag("field").perform_text_input("héllo");
    assert_eq!(*value.borrow(), "héllo");
    let node = ui.on_node_with_tag("field").fetch_semantics_node();
    assert_eq!(
        node.config.get(&compose_ui::SemanticsProperties::EDITABLE_TEXT).cloned().as_deref(),
        Some("•••••")
    );
    assert!(node.config.contains(&compose_ui::SemanticsProperties::PASSWORD));
    // The cursor follows the text: typing on appends.
    ui.type_text("!");
    assert_eq!(*value.borrow(), "héllo!");
    ui.press_key(Key::Backspace);
    ui.press_key(Key::DirectionLeft);
    ui.type_text("x");
    assert_eq!(*value.borrow(), "héllxo");
}

#[test]
fn a_password_cannot_be_copied_nor_cut() {
    let value = Rc::new(RefCell::new(String::new()));
    let mut ui = password_field(value.clone());
    ui.on_node_with_tag("field").perform_text_input("secret");
    ui.host().with_tree(|tree| tree.clipboard().set_text("before"));
    shortcut(&mut ui, 'a');
    shortcut(&mut ui, 'c');
    shortcut(&mut ui, 'x');
    assert_eq!(ui.host().with_tree(|tree| tree.clipboard().get_text()).as_deref(), Some("before"));
    assert_eq!(*value.borrow(), "secret");
}

#[test]
fn enter_in_a_single_line_field_performs_its_action() {
    let done = Rc::new(RefCell::new(Vec::<String>::new()));
    let log = done.clone();
    let mut ui = compose_test(move || {
        let log = log.clone();
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            keyboard_options =
                KeyboardOptions { ime_action: ImeAction::Search, ..KeyboardOptions::DEFAULT },
            keyboard_actions = KeyboardActions::new().on_search(move || {
                log.borrow_mut().push(text.get());
            }),
            single_line = true
        );
    });
    ui.on_node_with_tag("field").perform_text_input("query");
    ui.press_key(Key::Enter);
    assert_eq!(*done.borrow(), vec!["query"]);
}

#[test]
fn next_without_an_action_moves_focus_on() {
    let mut ui = compose_test(|| {
        Column!({
            for tag in ["first", "second"] {
                let text = remember(|| mutable_state_of(String::new()));
                BasicTextField!(
                    text.get(),
                    move |new: String| text.set(new),
                    modifier = Modifier.size((200.dp(), 40.dp())).test_tag(tag),
                    keyboard_options =
                        KeyboardOptions { ime_action: ImeAction::Next, ..KeyboardOptions::DEFAULT },
                    single_line = true
                );
            }
        });
    });
    ui.on_node_with_tag("first").perform_text_input("a");
    ui.press_key(Key::Enter);
    ui.on_node_with_tag("second").assert_is_focused();
}

#[test]
fn enter_in_a_multi_line_field_types_a_new_line() {
    let value = Rc::new(RefCell::new(String::new()));
    let seen = value.clone();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            text.get(),
            move |new: String| {
                *seen.borrow_mut() = new.clone();
                text.set(new);
            },
            modifier = Modifier.size((200.dp(), 80.dp())).test_tag("field"),
            keyboard_options =
                KeyboardOptions { ime_action: ImeAction::Done, ..KeyboardOptions::DEFAULT },
            keyboard_actions =
                KeyboardActions::new().on_done(|| panic!("not on a multi-line field"))
        );
    });
    ui.on_node_with_tag("field").perform_text_input("a");
    ui.press_key(Key::Enter);
    assert_eq!(*value.borrow(), "a\n");
}

const FIVE_LINES: &str = "A\nB\nC\nD\nE";

/// A field of `FIVE_LINES` `lines` lines high, its cursor `downs` lines down.
fn paged_field(lines: f32, downs: usize, value: Rc<RefCell<String>>) -> ComposeTestRule {
    let line = {
        let tall: Rc<std::cell::Cell<i32>> = Rc::default();
        let noted = tall.clone();
        let mut ui = compose_test(move || {
            let noted = noted.clone();
            BasicTextField!(
                FIVE_LINES.to_string(),
                |_: String| {},
                modifier = Modifier.on_size_changed(move |size: compose_ui::IntSize| {
                    noted.set(size.height);
                })
            );
        });
        ui.wait_for_idle();
        tall.get() as f32 / 5.0
    };
    let mut ui = compose_test(move || {
        let value = value.clone();
        let text = remember(|| mutable_state_of(FIVE_LINES.to_string()));
        BasicTextField!(
            text.get(),
            move |new: String| {
                *value.borrow_mut() = new.clone();
                text.set(new);
            },
            modifier =
                Modifier.required_size((200.dp(), compose_ui::Dp(line * lines))).test_tag("field")
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    for _ in 0..downs {
        ui.press_key(Key::DirectionDown);
    }
    ui
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/input/TextFieldKeyEventTest.kt, textField_pageNavigationDown, _exactFit, textField_pageNavigationUp, _exactFit
#[test]
fn page_keys_move_the_cursor_by_the_fields_height() {
    for (lines, downs, key, expected) in [
        // Less than three lines: the line most of the cursor would be on.
        (2.4, 0, Key::PageDown, "A\nB\nxC\nD\nE"),
        (3.0, 0, Key::PageDown, "A\nB\nC\nxD\nE"),
        (2.4, 4, Key::PageUp, "A\nB\nxC\nD\nE"),
        (3.0, 4, Key::PageUp, "A\nxB\nC\nD\nE"),
        // Past the text: its last or first line.
        (3.0, 3, Key::PageDown, "A\nB\nC\nD\nxE"),
        (3.0, 1, Key::PageUp, "xA\nB\nC\nD\nE"),
    ] {
        let value = Rc::new(RefCell::new(String::new()));
        let mut ui = paged_field(lines, downs, value.clone());
        ui.press_key(key);
        ui.type_text("x");
        assert_eq!(*value.borrow(), expected, "{lines} lines, {downs} down, {key:?}");
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/text/KeyMapping.kt: Shift+Page Down, SELECT_PAGE_DOWN
#[test]
fn shift_page_down_extends_the_selection() {
    let value = Rc::new(RefCell::new(String::new()));
    let mut ui = paged_field(3.0, 0, value.clone());
    let modifiers = KeyModifiers { shift: true, ..Default::default() };
    ui.host().send_key(KeyEvent { key: Key::PageDown, kind: KeyEventType::KeyDown, modifiers });
    ui.host().send_key(KeyEvent { key: Key::PageDown, kind: KeyEventType::KeyUp, modifiers });
    ui.wait_for_idle();
    ui.type_text("x");
    assert_eq!(*value.borrow(), "xD\nE");
}

/// A key a preview took isn't typed: the host doesn't send its text. A key the field took itself,
/// as it takes what's typed, is.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/key/ProcessKeyInputTest.kt, onKeyEventNotTriggered_ifOnPreviewKeyEventConsumesEvent
#[test]
fn a_key_a_preview_consumed_types_nothing() {
    let value = Rc::new(RefCell::new(String::new()));
    let typed = value.clone();
    let mut ui = compose_test(move || {
        let typed = typed.clone();
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            text.get(),
            move |new: String| {
                *typed.borrow_mut() = new.clone();
                text.set(new);
            },
            modifier = compose_ui::Modifier
                .on_preview_key_event(|event: KeyEvent| event.key == Key::Character('x'))
                .test_tag("field")
                .size(100.dp())
        );
    });
    ui.on_node_with_tag("field").request_focus();
    let down = |c| KeyEvent {
        key: Key::Character(c),
        kind: KeyEventType::KeyDown,
        modifiers: KeyModifiers::default(),
    };
    assert!(ui.host().send_key_and_text(down('y'), Some("y")));
    ui.wait_for_idle();
    assert_eq!(*value.borrow(), "y");
    assert!(ui.host().send_key_and_text(down('x'), Some("x")));
    ui.wait_for_idle();
    assert_eq!(*value.borrow(), "y");
    assert!(ui.host().send_key_and_text(down('y'), Some("y")));
    ui.wait_for_idle();
    assert_eq!(*value.borrow(), "yy");
}

/// A text field of 400 by 100 in a column that scrolls, tagged, and what's scrolled.
fn field_in_a_scrolling_page() -> (ComposeTestRule, ScrollState) {
    let out = Rc::new(std::cell::Cell::new(None));
    let scroll = out.clone();
    let mut ui = compose_test(move || {
        let state = remember_scroll_state(0);
        scroll.set(Some(state));
        Column!(modifier = compose_ui::Modifier.fill_max_size().vertical_scroll(state), {
            let text = remember(|| mutable_state_of("hello world".to_string()));
            BasicTextField!(
                text.get(),
                move |new: String| text.set(new),
                modifier = compose_ui::Modifier.size((400.dp(), 100.dp())).test_tag("field")
            );
            Spacer!(compose_ui::Modifier.height(3000.dp()));
        });
    });
    ui.wait_for_idle();
    let state = out.get().unwrap();
    (ui, state)
}

fn text_after_typing(ui: &mut ComposeTestRule) -> String {
    ui.type_text("X");
    let node = ui.on_node_with_tag("field").fetch_semantics_node();
    node.config.get(&compose_ui::SemanticsProperties::EDITABLE_TEXT).unwrap().to_string()
}

fn finger_drag(ui: &mut ComposeTestRule, from: compose_ui::Offset, by: compose_ui::Offset) {
    ui.on_node_with_tag("field").perform_touch_input(|touch| {
        touch.down(from);
        for _ in 0..10 {
            touch.move_by(compose_ui::Offset::new(by.x / 10.0, by.y / 10.0));
        }
        touch.up();
    });
    ui.wait_for_idle();
}

/// A finger dragged on a field scrolls the page around it, and it's no tap.
#[test]
fn a_finger_dragged_on_a_text_field_scrolls_the_page() {
    let (mut ui, state) = field_in_a_scrolling_page();
    let from = compose_ui::Offset::new(100.0, 90.0);
    finger_drag(&mut ui, from, compose_ui::Offset::new(0.0, -80.0));
    assert!(state.value() > 40, "scrolled {}", state.value());
    ui.on_node_with_tag("field").assert_is_not_focused();
}

/// With nothing taking it, a finger's wobble past the slop is still a tap, as `detectTapAndPress`,
/// and doesn't select: a first press selects after a long press in Compose.
#[test]
fn a_fingers_wobble_past_the_slop_still_taps_and_does_not_select() {
    let mut ui = compose_test(|| {
        let text = remember(|| mutable_state_of("hello world".to_string()));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = compose_ui::Modifier.size((400.dp(), 100.dp())).test_tag("field")
        );
    });
    ui.wait_for_idle();
    // Over the text, so that a drag would select.
    let from = compose_ui::Offset::new(1.0, 8.0);
    finger_drag(&mut ui, from, compose_ui::Offset::new(30.0, 0.0));
    ui.on_node_with_tag("field").assert_is_focused();
    // Nothing selected: what's typed replaces nothing.
    assert_eq!(text_after_typing(&mut ui).chars().count(), 12);
}

/// A finger leaving the field is no tap, as `waitForUpOrCancellation`.
#[test]
fn a_finger_leaving_the_field_is_no_tap() {
    let mut ui = compose_test(|| {
        let text = remember(|| mutable_state_of("hello world".to_string()));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = compose_ui::Modifier.size((400.dp(), 100.dp())).test_tag("field")
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("field").perform_touch_input(|touch| {
        touch.down(compose_ui::Offset::new(100.0, 50.0));
        touch.move_by(compose_ui::Offset::new(0.0, 80.0));
        touch.move_by(compose_ui::Offset::new(0.0, -80.0));
        touch.up();
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_not_focused();
}

/// A double tap's drag selects by words, whatever scrolls around, as
/// `touchSelectionSubsequentPress`.
#[test]
fn a_fingers_second_press_drags_by_words_and_the_page_stays() {
    let (mut ui, state) = field_in_a_scrolling_page();
    ui.on_node_with_tag("field").perform_touch_input(|touch| {
        touch.click_at(compose_ui::Offset::new(10.0, 10.0));
    });
    ui.on_node_with_tag("field").perform_touch_input(|touch| {
        touch.down(compose_ui::Offset::new(10.0, 10.0));
        touch.move_by(compose_ui::Offset::new(60.0, 0.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(state.value(), 0);
    // A word selected: what's typed replaces it.
    assert!(text_after_typing(&mut ui).chars().count() < 12);
}
