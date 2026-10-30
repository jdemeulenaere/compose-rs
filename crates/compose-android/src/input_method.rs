//! What the soft keyboard is told of the focused field: its `ImeOptions` as Android's
//! `EditorInfo` (`inputType` and `imeOptions`), as Compose's `EditorInfo.update(imeOptions,
//! textFieldValue)` in `TextInputServiceAndroid`; and the keyboard's action key back as an
//! `ImeAction`, as Compose's `RecordingInputConnection.performEditorAction`. Kotlin only applies
//! the two ints.

use compose_ui::{ImeAction, ImeOptions, KeyboardCapitalization, KeyboardType};

/// `android.text.InputType`'s constants.
pub mod input_type {
    pub const TYPE_MASK_CLASS: i32 = 0xf;
    pub const TYPE_CLASS_TEXT: i32 = 1;
    pub const TYPE_CLASS_NUMBER: i32 = 2;
    pub const TYPE_CLASS_PHONE: i32 = 3;
    pub const TYPE_TEXT_VARIATION_URI: i32 = 0x10;
    pub const TYPE_TEXT_VARIATION_EMAIL_ADDRESS: i32 = 0x20;
    pub const TYPE_TEXT_VARIATION_PASSWORD: i32 = 0x80;
    pub const TYPE_NUMBER_VARIATION_PASSWORD: i32 = 0x10;
    pub const TYPE_NUMBER_FLAG_DECIMAL: i32 = 0x2000;
    pub const TYPE_TEXT_FLAG_CAP_CHARACTERS: i32 = 0x1000;
    pub const TYPE_TEXT_FLAG_CAP_WORDS: i32 = 0x2000;
    pub const TYPE_TEXT_FLAG_CAP_SENTENCES: i32 = 0x4000;
    pub const TYPE_TEXT_FLAG_AUTO_CORRECT: i32 = 0x8000;
    pub const TYPE_TEXT_FLAG_MULTI_LINE: i32 = 0x20000;
}

/// `android.view.inputmethod.EditorInfo`'s constants.
pub mod editor_info {
    pub const IME_MASK_ACTION: i32 = 0xff;
    pub const IME_ACTION_UNSPECIFIED: i32 = 0;
    pub const IME_ACTION_NONE: i32 = 1;
    pub const IME_ACTION_GO: i32 = 2;
    pub const IME_ACTION_SEARCH: i32 = 3;
    pub const IME_ACTION_SEND: i32 = 4;
    pub const IME_ACTION_NEXT: i32 = 5;
    pub const IME_ACTION_DONE: i32 = 6;
    pub const IME_ACTION_PREVIOUS: i32 = 7;
    pub const IME_FLAG_NO_FULLSCREEN: i32 = 0x0200_0000;
    pub const IME_FLAG_NO_EXTRACT_UI: i32 = 0x1000_0000;
    pub const IME_FLAG_NO_ENTER_ACTION: i32 = 0x4000_0000;
    pub const IME_FLAG_FORCE_ASCII: i32 = 0x8000_0000_u32 as i32;
}

use editor_info::*;
use input_type::*;

/// An `EditorInfo`'s `inputType` and `imeOptions`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EditorInfo {
    pub input_type: i32,
    pub ime_options: i32,
}

impl EditorInfo {
    /// Both in one `long` for one JNI return: `imeOptions` in the high 32 bits.
    pub fn pack(self) -> i64 {
        (i64::from(self.ime_options) << 32) | i64::from(self.input_type as u32)
    }
}

/// The `EditorInfo` for a field's `options`, as Compose's `EditorInfo.update`. `masks_password`
/// (its semantics say a password: a `PasswordVisualTransformation`) makes a `Text` keyboard a
/// password's, so the keyboard doesn't learn it (compose-rs's addition; Compose's field asks
/// for `KeyboardType.Password` itself).
///
/// Two departures, D250: the text flags (multi-line, capitalization, auto-correct) go to the
/// text class only (Compose's `hasFlag(inputType, TYPE_CLASS_TEXT)` also matches the phone
/// class, 3); and `IME_FLAG_NO_EXTRACT_UI` joins Compose's `IME_FLAG_NO_FULLSCREEN`, as the
/// connection shows the keyboard no text to extract.
pub fn editor_info(options: ImeOptions, masks_password: bool) -> EditorInfo {
    let mut ime_options = match options.ime_action {
        // The last resort to make a single line: the keyboard shows Done, not a return key.
        ImeAction::Default if options.single_line => IME_ACTION_DONE,
        ImeAction::Default => IME_ACTION_UNSPECIFIED,
        ImeAction::None => IME_ACTION_NONE,
        ImeAction::Go => IME_ACTION_GO,
        ImeAction::Next => IME_ACTION_NEXT,
        ImeAction::Previous => IME_ACTION_PREVIOUS,
        ImeAction::Search => IME_ACTION_SEARCH,
        ImeAction::Send => IME_ACTION_SEND,
        ImeAction::Done => IME_ACTION_DONE,
        // An action added later: the keyboard's default.
        _ => IME_ACTION_UNSPECIFIED,
    };
    let keyboard_type = match options.keyboard_type {
        KeyboardType::Text if masks_password => KeyboardType::Password,
        keyboard_type => keyboard_type,
    };
    let mut input_type = match keyboard_type {
        KeyboardType::Text => TYPE_CLASS_TEXT,
        KeyboardType::Ascii => {
            ime_options |= IME_FLAG_FORCE_ASCII;
            TYPE_CLASS_TEXT
        }
        KeyboardType::Number => TYPE_CLASS_NUMBER,
        KeyboardType::Phone => TYPE_CLASS_PHONE,
        KeyboardType::Uri => TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_URI,
        KeyboardType::Email => TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_EMAIL_ADDRESS,
        KeyboardType::Password => TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_PASSWORD,
        KeyboardType::NumberPassword => TYPE_CLASS_NUMBER | TYPE_NUMBER_VARIATION_PASSWORD,
        KeyboardType::Decimal => TYPE_CLASS_NUMBER | TYPE_NUMBER_FLAG_DECIMAL,
        _ => TYPE_CLASS_TEXT,
    };
    if input_type & TYPE_MASK_CLASS == TYPE_CLASS_TEXT {
        if !options.single_line {
            // As `TextView`'s `setInputTypeSingleLine`.
            input_type |= TYPE_TEXT_FLAG_MULTI_LINE;
            // Enter types a new line, not an action.
            if options.ime_action == ImeAction::Default {
                ime_options |= IME_FLAG_NO_ENTER_ACTION;
            }
        }
        input_type |= match options.capitalization {
            KeyboardCapitalization::None => 0,
            KeyboardCapitalization::Characters => TYPE_TEXT_FLAG_CAP_CHARACTERS,
            KeyboardCapitalization::Words => TYPE_TEXT_FLAG_CAP_WORDS,
            KeyboardCapitalization::Sentences => TYPE_TEXT_FLAG_CAP_SENTENCES,
            _ => 0,
        };
        if options.auto_correct {
            input_type |= TYPE_TEXT_FLAG_AUTO_CORRECT;
        }
    }
    EditorInfo {
        input_type,
        ime_options: ime_options | IME_FLAG_NO_FULLSCREEN | IME_FLAG_NO_EXTRACT_UI,
    }
}

/// The keyboard's action key (`performEditorAction`'s code) as an `ImeAction`, as Compose's
/// `RecordingInputConnection`: an action it doesn't know (`IME_ACTION_NONE` among them) is
/// `Default`.
pub fn ime_action(editor_action: i32) -> ImeAction {
    match editor_action & IME_MASK_ACTION {
        IME_ACTION_DONE => ImeAction::Done,
        IME_ACTION_SEND => ImeAction::Send,
        IME_ACTION_SEARCH => ImeAction::Search,
        IME_ACTION_PREVIOUS => ImeAction::Previous,
        IME_ACTION_NEXT => ImeAction::Next,
        IME_ACTION_GO => ImeAction::Go,
        _ => ImeAction::Default,
    }
}

/// What a frame tells the view of text input.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TextInputState {
    /// A focused node takes text: the view is a text editor and its keyboard shows.
    pub active: bool,
    /// The input method starts again, reading the field's options anew: the field committed or
    /// dropped its composition itself, or the focused field's options changed (another field,
    /// or the same one's), as Compose's `TextInputServiceAndroid` restarting the input.
    pub restart: bool,
    /// What the content last asked of the soft keyboard: show (`true`) or hide (`false`).
    pub keyboard: Option<bool>,
}

/// Takes what `ui` asks of the input method after a frame (the events before it having woken
/// it), `last_options` being the focused field's options the last frame saw.
pub fn take_text_input_state(
    ui: &compose_ui::platform::UiHost,
    last_options: &mut Option<ImeOptions>,
) -> TextInputState {
    let options = if ui.text_input_area().is_some() { ui.text_input_options() } else { None };
    let changed = options.is_some() && last_options.is_some() && options != *last_options;
    *last_options = options;
    TextInputState {
        active: options.is_some(),
        // Both taken: neither is left for a later frame.
        restart: ui.take_input_method_reset() | changed,
        keyboard: ui.take_software_keyboard_request(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALWAYS: i32 = IME_FLAG_NO_FULLSCREEN | IME_FLAG_NO_EXTRACT_UI;

    fn single_line(keyboard_type: KeyboardType, ime_action: ImeAction) -> ImeOptions {
        ImeOptions {
            single_line: true,
            auto_correct: false,
            keyboard_type,
            ime_action,
            ..ImeOptions::DEFAULT
        }
    }

    #[test]
    fn the_defaults_are_a_multi_line_auto_corrected_text_whose_enter_types_a_new_line() {
        assert_eq!(
            editor_info(ImeOptions::DEFAULT, false),
            EditorInfo {
                input_type: TYPE_CLASS_TEXT
                    | TYPE_TEXT_FLAG_MULTI_LINE
                    | TYPE_TEXT_FLAG_AUTO_CORRECT,
                ime_options: IME_ACTION_UNSPECIFIED | IME_FLAG_NO_ENTER_ACTION | ALWAYS,
            }
        );
    }

    #[test]
    fn keyboard_types_map_as_composes() {
        let table = [
            (KeyboardType::Text, TYPE_CLASS_TEXT, 0),
            (KeyboardType::Ascii, TYPE_CLASS_TEXT, IME_FLAG_FORCE_ASCII),
            (KeyboardType::Number, TYPE_CLASS_NUMBER, 0),
            (KeyboardType::Phone, TYPE_CLASS_PHONE, 0),
            (KeyboardType::Uri, TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_URI, 0),
            (KeyboardType::Email, TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_EMAIL_ADDRESS, 0),
            (KeyboardType::Password, TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_PASSWORD, 0),
            (KeyboardType::NumberPassword, TYPE_CLASS_NUMBER | TYPE_NUMBER_VARIATION_PASSWORD, 0),
            (KeyboardType::Decimal, TYPE_CLASS_NUMBER | TYPE_NUMBER_FLAG_DECIMAL, 0),
        ];
        for (keyboard_type, input_type, flags) in table {
            assert_eq!(
                editor_info(single_line(keyboard_type, ImeAction::Done), false),
                EditorInfo { input_type, ime_options: IME_ACTION_DONE | flags | ALWAYS },
                "{keyboard_type:?}"
            );
        }
    }

    #[test]
    fn ime_actions_map_as_composes() {
        let table = [
            (ImeAction::Default, IME_ACTION_DONE),
            (ImeAction::None, IME_ACTION_NONE),
            (ImeAction::Go, IME_ACTION_GO),
            (ImeAction::Search, IME_ACTION_SEARCH),
            (ImeAction::Send, IME_ACTION_SEND),
            (ImeAction::Previous, IME_ACTION_PREVIOUS),
            (ImeAction::Next, IME_ACTION_NEXT),
            (ImeAction::Done, IME_ACTION_DONE),
        ];
        for (action, code) in table {
            let info = editor_info(single_line(KeyboardType::Text, action), false);
            assert_eq!(info.ime_options, code | ALWAYS, "{action:?}");
            assert_eq!(info.input_type, TYPE_CLASS_TEXT, "{action:?}");
        }
    }

    #[test]
    fn a_multi_line_field_keeps_its_action_key_unless_default() {
        let options = ImeOptions { ime_action: ImeAction::Send, ..ImeOptions::DEFAULT };
        let info = editor_info(options, false);
        assert_eq!(info.ime_options, IME_ACTION_SEND | ALWAYS);
        assert_ne!(info.input_type & TYPE_TEXT_FLAG_MULTI_LINE, 0);
    }

    #[test]
    fn capitalization_and_auto_correct_are_text_flags() {
        let table = [
            (KeyboardCapitalization::None, 0),
            (KeyboardCapitalization::Characters, TYPE_TEXT_FLAG_CAP_CHARACTERS),
            (KeyboardCapitalization::Words, TYPE_TEXT_FLAG_CAP_WORDS),
            (KeyboardCapitalization::Sentences, TYPE_TEXT_FLAG_CAP_SENTENCES),
        ];
        for (capitalization, flag) in table {
            for auto_correct in [false, true] {
                let options = ImeOptions {
                    capitalization,
                    auto_correct,
                    ..single_line(KeyboardType::Email, ImeAction::Next)
                };
                let correct = if auto_correct { TYPE_TEXT_FLAG_AUTO_CORRECT } else { 0 };
                assert_eq!(
                    editor_info(options, false).input_type,
                    TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_EMAIL_ADDRESS | flag | correct,
                    "{capitalization:?} {auto_correct}"
                );
            }
        }
    }

    #[test]
    fn numbers_and_phones_take_no_text_flags() {
        for keyboard_type in [KeyboardType::Number, KeyboardType::Phone, KeyboardType::Decimal] {
            let options = ImeOptions {
                capitalization: KeyboardCapitalization::Words,
                keyboard_type,
                ..ImeOptions::DEFAULT
            };
            let info = editor_info(options, false);
            assert_eq!(info.input_type & !0xffff_i32, 0, "{keyboard_type:?}: no multi-line");
            assert_eq!(
                info.input_type & (TYPE_TEXT_FLAG_CAP_WORDS | TYPE_TEXT_FLAG_AUTO_CORRECT),
                if keyboard_type == KeyboardType::Decimal { TYPE_NUMBER_FLAG_DECIMAL } else { 0 },
            );
            // No new line to type: a multi-line number keeps the unspecified action key.
            assert_eq!(info.ime_options, IME_ACTION_UNSPECIFIED | ALWAYS);
        }
    }

    #[test]
    fn a_masked_password_takes_a_password_keyboard() {
        let options = single_line(KeyboardType::Text, ImeAction::Done);
        assert_eq!(
            editor_info(options, true).input_type,
            TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_PASSWORD
        );
        // A keyboard type the field asked for stays.
        let options = single_line(KeyboardType::NumberPassword, ImeAction::Done);
        assert_eq!(
            editor_info(options, true).input_type,
            TYPE_CLASS_NUMBER | TYPE_NUMBER_VARIATION_PASSWORD
        );
    }

    #[test]
    fn editor_actions_come_back_as_composes() {
        assert_eq!(ime_action(IME_ACTION_DONE), ImeAction::Done);
        assert_eq!(ime_action(IME_ACTION_SEND), ImeAction::Send);
        assert_eq!(ime_action(IME_ACTION_SEARCH), ImeAction::Search);
        assert_eq!(ime_action(IME_ACTION_PREVIOUS), ImeAction::Previous);
        assert_eq!(ime_action(IME_ACTION_NEXT), ImeAction::Next);
        assert_eq!(ime_action(IME_ACTION_GO), ImeAction::Go);
        assert_eq!(ime_action(IME_ACTION_UNSPECIFIED), ImeAction::Default);
        assert_eq!(ime_action(IME_ACTION_NONE), ImeAction::Default);
        assert_eq!(ime_action(42), ImeAction::Default);
        // Every action survives the round trip but Default and None (as Compose's).
        for action in [ImeAction::Go, ImeAction::Search, ImeAction::Send, ImeAction::Next] {
            let info = editor_info(single_line(KeyboardType::Text, action), false);
            assert_eq!(ime_action(info.ime_options), action);
        }
    }

    #[test]
    fn packs_into_one_long() {
        let info = EditorInfo { input_type: -2, ime_options: IME_FLAG_FORCE_ASCII | 6 };
        let packed = info.pack();
        assert_eq!(packed as i32, -2);
        assert_eq!((packed >> 32) as i32, IME_FLAG_FORCE_ASCII | 6);
    }
}
