//! Forms: `FocusManager`, `SoftwareKeyboardController`, the keyboard options a field gives the
//! host, the input method's action key, and the line bounds of texts and fields.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{mutable_state_of, remember};
use compose_test::*;
use compose_ui::{
    FocusDirection, ImeOptions, Key, LOCAL_FOCUS_MANAGER, LOCAL_SOFTWARE_KEYBOARD_CONTROLLER,
    TextLayoutResult, UnitExt,
};

/// A 3×2 grid of focusable 20 dp boxes tagged "column,row", and the focus manager its content
/// took from the composition.
fn grid() -> (ComposeTestRule, compose_ui::FocusManager) {
    let manager = Rc::new(RefCell::new(None));
    let taken = manager.clone();
    let ui = compose_test(move || {
        *taken.borrow_mut() = Some(LOCAL_FOCUS_MANAGER.current());
        Column!({
            for row in 0..2 {
                Row!({
                    for column in 0..3 {
                        Box!(
                            modifier = Modifier
                                .size(20.dp())
                                .focusable(true)
                                .test_tag(format!("{column},{row}"))
                        );
                    }
                });
            }
        });
    });
    let manager = manager.borrow().clone().expect("composed");
    (ui, manager)
}

#[test]
fn the_focus_manager_moves_the_focus_in_two_dimensions() {
    let (mut ui, manager) = grid();
    let mut step = |direction: FocusDirection, expected: &str| {
        assert!(manager.move_focus(direction));
        ui.wait_for_idle();
        ui.on_node_with_tag(expected).assert_is_focused();
    };
    // Nothing focused: Right starts from the top left.
    step(FocusDirection::Right, "0,0");
    step(FocusDirection::Right, "1,0");
    step(FocusDirection::Down, "1,1");
    step(FocusDirection::Left, "0,1");
    step(FocusDirection::Up, "0,0");
    step(FocusDirection::Next, "1,0");
    step(FocusDirection::Previous, "0,0");
    // Nothing that way: the focus stays.
    step(FocusDirection::Up, "0,0");
    step(FocusDirection::Left, "0,0");
}

#[test]
fn the_focus_manager_clears_the_focus() {
    let (mut ui, manager) = grid();
    ui.on_node_with_tag("1,1").request_focus().assert_is_focused();
    manager.clear_focus(false);
    ui.wait_for_idle();
    assert_eq!(ui.host().with_tree(|tree| tree.focused()), None);
    // A manager of a tree that's gone does nothing.
    drop(ui);
    assert!(!manager.move_focus(FocusDirection::Next));
}

#[test]
fn the_keyboard_controller_asks_the_host_to_show_or_hide_the_keyboard() {
    let controller = Rc::new(RefCell::new(None));
    let taken = controller.clone();
    let mut ui = compose_test(move || {
        *taken.borrow_mut() = LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current();
    });
    let controller = controller.borrow().clone().expect("a tree has a controller");
    assert_eq!(ui.host().take_software_keyboard_request(), None);
    controller.show();
    assert!(ui.host().needs_frame(), "asking wakes the host");
    assert_eq!(ui.host().take_software_keyboard_request(), Some(true));
    assert_eq!(ui.host().take_software_keyboard_request(), None, "taken once");
    controller.show();
    controller.hide();
    assert_eq!(ui.host().take_software_keyboard_request(), Some(false), "the last one");
}

fn field(
    keyboard_options: KeyboardOptions,
    keyboard_actions: KeyboardActions,
    single_line: bool,
) -> ComposeTestRule {
    compose_test(move || {
        let keyboard_actions = keyboard_actions.clone();
        Column!({
            for tag in ["first", "second"] {
                let text = remember(|| mutable_state_of(String::new()));
                BasicTextField!(
                    text.get(),
                    move |new: String| text.set(new),
                    modifier = Modifier.size((200.dp(), 40.dp())).test_tag(tag),
                    keyboard_options = keyboard_options,
                    keyboard_actions = keyboard_actions.clone(),
                    single_line = single_line
                );
            }
        });
    })
}

#[test]
fn a_focused_field_gives_the_host_its_keyboard_options() {
    let options = KeyboardOptions {
        capitalization: KeyboardCapitalization::Words,
        auto_correct_enabled: false,
        keyboard_type: KeyboardType::Email,
        ime_action: ImeAction::Next,
    };
    let mut ui = field(options, KeyboardActions::new(), true);
    assert_eq!(ui.host().text_input_options(), None, "nothing focused takes text");
    ui.on_node_with_tag("first").request_focus();
    assert_eq!(
        ui.host().text_input_options(),
        Some(ImeOptions {
            single_line: true,
            capitalization: KeyboardCapitalization::Words,
            auto_correct: false,
            keyboard_type: KeyboardType::Email,
            ime_action: ImeAction::Next,
        })
    );
    assert_eq!(KeyboardOptions::DEFAULT.to_ime_options(false), ImeOptions::DEFAULT);
}

#[test]
fn the_input_methods_action_key_runs_the_fields_action() {
    let log = Rc::new(RefCell::new(Vec::<&str>::new()));
    let (done, send) = (log.clone(), log.clone());
    let actions = KeyboardActions::new()
        .on_done(move || done.borrow_mut().push("done"))
        .on_send(move || send.borrow_mut().push("send"));
    let options = KeyboardOptions { ime_action: ImeAction::Send, ..KeyboardOptions::DEFAULT };
    let mut ui = field(options, actions, false);
    ui.on_node_with_tag("first").request_focus();
    // Whichever key the keyboard showed, as Compose's: a multi-line field too.
    assert!(ui.host().perform_ime_action(ImeAction::Send));
    assert!(ui.host().perform_ime_action(ImeAction::Done));
    assert!(!ui.host().perform_ime_action(ImeAction::Go), "no action, no default");
    assert_eq!(*log.borrow(), vec!["send", "done"]);
}

#[test]
fn the_default_actions_move_the_focus_and_hide_the_keyboard() {
    let mut ui = field(KeyboardOptions::DEFAULT, KeyboardActions::new(), true);
    ui.on_node_with_tag("first").request_focus();
    assert!(ui.host().perform_ime_action(ImeAction::Next));
    ui.wait_for_idle();
    ui.on_node_with_tag("second").assert_is_focused();
    assert!(ui.host().perform_ime_action(ImeAction::Previous));
    ui.wait_for_idle();
    ui.on_node_with_tag("first").assert_is_focused();
    assert_eq!(ui.host().take_software_keyboard_request(), None);
    // A single line's default key is Done: it hides the keyboard.
    assert!(ui.host().perform_ime_action(ImeAction::Default));
    assert_eq!(ui.host().take_software_keyboard_request(), Some(false));
    // As Enter does with `Done`.
    let options = KeyboardOptions { ime_action: ImeAction::Done, ..KeyboardOptions::DEFAULT };
    let mut ui = field(options, KeyboardActions::new(), true);
    ui.on_node_with_tag("first").request_focus();
    ui.press_key(Key::Enter);
    assert_eq!(ui.host().take_software_keyboard_request(), Some(false));
}

/// As Compose's `tapToFocus`: a tap on the focused field asks for the keyboard again (after
/// `Done` hid it), on its text or on its decoration; a desktop host just ignores the request.
#[test]
fn a_tap_on_the_focused_field_asks_for_the_keyboard() {
    let mut ui = field(KeyboardOptions::DEFAULT, KeyboardActions::new(), true);
    ui.on_node_with_tag("first").perform_click().assert_is_focused();
    // Focusing shows it anyway (the platform shows it for a newly focused field).
    let _ = ui.host().take_software_keyboard_request();
    assert!(ui.host().perform_ime_action(ImeAction::Done));
    assert_eq!(ui.host().take_software_keyboard_request(), Some(false));
    ui.on_node_with_tag("first").perform_click().assert_is_focused();
    assert_eq!(ui.host().take_software_keyboard_request(), Some(true));

    // A tap on the padding of a decoration around the text.
    let mut ui = compose_test(|| {
        let text = remember(|| mutable_state_of(String::new()));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = Modifier.size((200.dp(), 80.dp())).test_tag("field"),
            single_line = true,
            decoration_box = |inner_text_field: compose_runtime::Callback| {
                Box!(modifier = Modifier.padding(30.dp()), { inner_text_field() });
            }
        );
    });
    ui.on_node_with_tag("field").request_focus();
    assert!(ui.host().perform_ime_action(ImeAction::Done));
    assert_eq!(ui.host().take_software_keyboard_request(), Some(false));
    let padding = compose_ui::Offset::new(5.0, 5.0);
    ui.host().send_pointer(compose_ui::PointerEvent::press(padding));
    ui.host().send_pointer(compose_ui::PointerEvent::release(padding));
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_focused();
    assert_eq!(ui.host().take_software_keyboard_request(), Some(true));

    // A read-only field doesn't.
    let mut ui = compose_test(|| {
        BasicTextField!(
            String::from("fixed"),
            |_: String| {},
            modifier = Modifier.size((200.dp(), 40.dp())).test_tag("field"),
            read_only = true
        );
    });
    ui.on_node_with_tag("field").perform_click().perform_click();
    assert_eq!(ui.host().take_software_keyboard_request(), None);
}

#[test]
fn a_password_field_takes_its_transformation_without_into() {
    let mut ui = compose_test(|| {
        let text = remember(|| mutable_state_of(String::from("secret")));
        BasicTextField!(
            text.get(),
            move |new: String| text.set(new),
            modifier = Modifier.test_tag("field"),
            visual_transformation = PasswordVisualTransformation::new()
        );
    });
    let node = ui.on_node_with_tag("field").fetch_semantics_node();
    assert_eq!(node.config.editable_text.as_deref(), Some("••••••"));
}

// ---- Line bounds ---------------------------------------------------------------------------

/// The fixed measurer's line height at the default 14 sp: 17.5 px.
fn lines_high(lines: u32) -> i32 {
    (14.0f32 * 1.25 * lines as f32).ceil() as i32
}

fn height_of(ui: &mut ComposeTestRule, tag: &str) -> i32 {
    let bounds = ui.on_node_with_tag(tag).fetch_semantics_node().bounds;
    (bounds.bottom - bounds.top).round() as i32
}

#[test]
fn a_text_is_at_least_min_lines_high_and_reports_its_layout() {
    let layouts = Rc::new(RefCell::new(Vec::<TextLayoutResult>::new()));
    let seen = layouts.clone();
    let text = mutable_state_of(String::from("one"));
    let handle = text.handle();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        Column!({
            BasicText!("one", modifier = Modifier.test_tag("plain"));
            BasicText!("one", modifier = Modifier.test_tag("three"), min_lines = 3);
            let seen = seen.clone();
            BasicText!(
                handle.get(),
                modifier = Modifier.width(40.dp()).test_tag("reported"),
                max_lines = 2,
                on_text_layout = move |result: TextLayoutResult| seen.borrow_mut().push(result)
            );
        });
    });
    assert_eq!(height_of(&mut ui, "plain"), lines_high(1));
    // "H" and "H\nH" give the first line's height and each next one's.
    let three = height_of(&mut ui, "three");
    assert!((three - lines_high(3)).abs() <= 1, "{three}");
    let first = *layouts.borrow().last().expect("reported");
    assert_eq!(first.line_count, 1);
    assert!(!first.has_visual_overflow());
    assert_eq!(layouts.borrow().len(), 1, "once per new layout");
    // 40 dp at 7 px a character: 5 per line, so three lines cut to two.
    text.set(String::from("aaaa bbbb cccc"));
    ui.wait_for_idle();
    let cut = *layouts.borrow().last().expect("reported");
    assert_eq!(cut.line_count, 2);
    assert!(cut.did_overflow_height && cut.has_visual_overflow());
    assert_eq!(layouts.borrow().len(), 2);
}

#[test]
fn a_multi_line_field_is_between_min_lines_and_max_lines_high() {
    let value = mutable_state_of(String::new());
    let handle = value.handle();
    let mut ui = compose_test(move || {
        Column!({
            BasicTextField!(
                handle.get(),
                |_: String| {},
                modifier = Modifier.width(200.dp()).test_tag("bounded"),
                max_lines = 3,
                min_lines = 2
            );
            BasicTextField!(
                handle.get(),
                |_: String| {},
                modifier = Modifier.width(200.dp()).test_tag("free")
            );
            BasicTextField!(
                handle.get(),
                |_: String| {},
                modifier = Modifier.width(200.dp()).test_tag("single"),
                single_line = true,
                min_lines = 3
            );
        });
    });
    let near = |height: i32, lines: u32| (height - lines_high(lines)).abs() <= 1;
    assert!(near(height_of(&mut ui, "bounded"), 2), "min_lines");
    assert!(near(height_of(&mut ui, "free"), 1));
    assert!(near(height_of(&mut ui, "single"), 1), "a single line ignores min_lines");
    value.set(String::from("1\n2\n3\n4\n5"));
    ui.wait_for_idle();
    assert!(near(height_of(&mut ui, "bounded"), 3), "max_lines: the rest scrolls");
    assert!(near(height_of(&mut ui, "free"), 5));
}
