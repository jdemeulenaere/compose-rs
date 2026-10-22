//! What a text field asks of the platform's input method, as Compose's
//! `androidx.compose.ui.text.input`: the kind of keyboard (`KeyboardType`), its action key
//! (`ImeAction`), capitalization and auto-correction, together `ImeOptions`. The focused field
//! gives its options to the host (`UiHost::text_input_options`), which an on-screen keyboard
//! reads; the input method's action key comes back as `UiHost::perform_ime_action`.

/// The action a single-line field's Enter (an input method's action key) performs, as Compose's
/// `ImeAction`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ImeAction {
    /// The platform's default: on desktop, none; on Android, `Done` for a single line, a new
    /// line otherwise.
    #[default]
    Default,
    None,
    Go,
    Search,
    Send,
    /// Moves focus back.
    Previous,
    /// Moves focus on.
    Next,
    Done,
}

/// The kind of text a field takes, as Compose's `KeyboardType`: a hint for on-screen keyboards
/// (desktop keyboards ignore it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum KeyboardType {
    #[default]
    Text,
    Ascii,
    Number,
    Phone,
    Uri,
    Email,
    Password,
    NumberPassword,
    Decimal,
}

/// What an on-screen keyboard capitalizes on its own, as Compose's `KeyboardCapitalization`
/// (desktop keyboards ignore it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum KeyboardCapitalization {
    /// Nothing.
    #[default]
    None,
    /// Every character.
    Characters,
    /// The first character of each word.
    Words,
    /// The first character of each sentence.
    Sentences,
}

/// What the focused text field asks of the input method, as Compose's `ImeOptions`: a platform
/// host reads them (`UiHost::text_input_options`) when the field takes the focus, for its
/// on-screen keyboard (Android's `EditorInfo`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImeOptions {
    /// The field takes one line: the keyboard shows an action key rather than a new line.
    pub single_line: bool,
    pub capitalization: KeyboardCapitalization,
    /// The keyboard may correct words as they're typed.
    pub auto_correct: bool,
    pub keyboard_type: KeyboardType,
    pub ime_action: ImeAction,
}

impl ImeOptions {
    /// As `ImeOptions.Default`.
    pub const DEFAULT: ImeOptions = ImeOptions {
        single_line: false,
        capitalization: KeyboardCapitalization::None,
        auto_correct: true,
        keyboard_type: KeyboardType::Text,
        ime_action: ImeAction::Default,
    };
}

impl Default for ImeOptions {
    fn default() -> Self {
        ImeOptions::DEFAULT
    }
}
