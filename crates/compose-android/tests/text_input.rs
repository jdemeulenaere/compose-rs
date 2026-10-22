//! What a frame tells `ComposeRsView` of the soft keyboard (`input_method.rs`, D250): the
//! focused field's `EditorInfo`, the input restarting when its options change, its action key,
//! and the content's requests to show or hide the keyboard.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::RefCell;
use std::rc::Rc;

use compose_android::input_method::{
    TextInputState, editor_info, ime_action, input_type, take_text_input_state,
};
use compose_android::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{MutableState, mutable_state_of, remember};
use compose_ui::{
    Density, ImeAction, ImeOptions, IntSize, KeyboardType, LOCAL_SOFTWARE_KEYBOARD_CONTROLLER,
    Offset, ParagraphMetrics, PointerEvent, Rect, SoftwareKeyboardController, UiHost, UnitExt,
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
        LaidOut { metrics, did_exceed_max_lines: false }
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
                modifier = Modifier.size_wh(160.dp(), 40.dp()),
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
                modifier = Modifier.size_wh(160.dp(), 40.dp()),
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
