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
