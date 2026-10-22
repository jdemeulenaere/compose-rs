//! What a text field's keyboard does and how its text shows, as Compose's `KeyboardOptions`,
//! `KeyboardActions` and `VisualTransformation`.

use std::rc::Rc;

use compose_runtime::{Callback, ComposeArg};

pub use compose_ui::text_input::{ImeAction, ImeOptions, KeyboardCapitalization, KeyboardType};

/// A field's keyboard, as Compose's `KeyboardOptions`:
/// `KeyboardOptions { ime_action: ImeAction::Done, ..KeyboardOptions::DEFAULT }`. On-screen
/// keyboards read them (`UiHost::text_input_options`); desktop keyboards ignore all but the
/// action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyboardOptions {
    /// What the keyboard capitalizes on its own.
    pub capitalization: KeyboardCapitalization,
    /// Whether the keyboard may correct words as they're typed (Compose's `autoCorrectEnabled`,
    /// true when unspecified).
    pub auto_correct_enabled: bool,
    pub keyboard_type: KeyboardType,
    pub ime_action: ImeAction,
}

impl KeyboardOptions {
    pub const DEFAULT: KeyboardOptions = KeyboardOptions {
        capitalization: KeyboardCapitalization::None,
        auto_correct_enabled: true,
        keyboard_type: KeyboardType::Text,
        ime_action: ImeAction::Default,
    };

    /// What the field asks of the input method, as Compose's `toImeOptions(singleLine)`.
    pub fn to_ime_options(&self, single_line: bool) -> ImeOptions {
        ImeOptions {
            single_line,
            capitalization: self.capitalization,
            auto_correct: self.auto_correct_enabled,
            keyboard_type: self.keyboard_type,
            ime_action: self.ime_action,
        }
    }
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        KeyboardOptions::DEFAULT
    }
}

impl ComposeArg for KeyboardOptions {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// What a field's actions do, as Compose's `KeyboardActions`: the one for its `ime_action` runs
/// on Enter in a single-line field, or when the input method's action key is pressed. Without
/// one, `Next` and `Previous` move focus and `Done` hides the on-screen keyboard, as Compose's
/// default actions; the others do nothing.
#[derive(Clone, Default)]
pub struct KeyboardActions {
    pub on_done: Option<Callback<dyn Fn()>>,
    pub on_go: Option<Callback<dyn Fn()>>,
    pub on_next: Option<Callback<dyn Fn()>>,
    pub on_previous: Option<Callback<dyn Fn()>>,
    pub on_search: Option<Callback<dyn Fn()>>,
    pub on_send: Option<Callback<dyn Fn()>>,
}

impl KeyboardActions {
    /// No actions, as `KeyboardActions.Default`.
    pub fn new() -> KeyboardActions {
        KeyboardActions::default()
    }

    pub fn on_done(mut self, action: impl Fn() + 'static) -> Self {
        self.on_done = Some(Callback::new(action));
        self
    }
    pub fn on_go(mut self, action: impl Fn() + 'static) -> Self {
        self.on_go = Some(Callback::new(action));
        self
    }
    pub fn on_next(mut self, action: impl Fn() + 'static) -> Self {
        self.on_next = Some(Callback::new(action));
        self
    }
    pub fn on_previous(mut self, action: impl Fn() + 'static) -> Self {
        self.on_previous = Some(Callback::new(action));
        self
    }
    pub fn on_search(mut self, action: impl Fn() + 'static) -> Self {
        self.on_search = Some(Callback::new(action));
        self
    }
    pub fn on_send(mut self, action: impl Fn() + 'static) -> Self {
        self.on_send = Some(Callback::new(action));
        self
    }

    /// The action for `ime_action`, if given.
    pub(crate) fn action(&self, ime_action: ImeAction) -> Option<&Callback<dyn Fn()>> {
        match ime_action {
            ImeAction::Done => self.on_done.as_ref(),
            ImeAction::Go => self.on_go.as_ref(),
            ImeAction::Next => self.on_next.as_ref(),
            ImeAction::Previous => self.on_previous.as_ref(),
            ImeAction::Search => self.on_search.as_ref(),
            ImeAction::Send => self.on_send.as_ref(),
            ImeAction::Default | ImeAction::None => None,
        }
    }
}

impl ComposeArg for KeyboardActions {
    fn same(&self, other: &Self) -> bool {
        let same = |a: &Option<Callback<dyn Fn()>>, b: &Option<Callback<dyn Fn()>>| match (a, b) {
            (Some(a), Some(b)) => a.ptr_eq(b),
            (None, None) => true,
            _ => false,
        };
        same(&self.on_done, &other.on_done)
            && same(&self.on_go, &other.on_go)
            && same(&self.on_next, &other.on_next)
            && same(&self.on_previous, &other.on_previous)
            && same(&self.on_search, &other.on_search)
            && same(&self.on_send, &other.on_send)
    }
}

/// Maps offsets between a field's text and the text it shows, as Compose's `OffsetMapping`:
/// byte offsets at character boundaries.
pub trait OffsetMapping {
    fn original_to_transformed(&self, offset: usize) -> usize;
    fn transformed_to_original(&self, offset: usize) -> usize;
}

/// The text a field shows for its text, and how their offsets map, as Compose's
/// `TransformedText`.
#[derive(Clone)]
pub struct TransformedText {
    pub text: String,
    pub offset_mapping: Rc<dyn OffsetMapping>,
}

/// Changes how a field's text shows without changing it, as Compose's `VisualTransformation`.
pub trait VisualTransform {
    fn filter(&self, text: &str) -> TransformedText;
}

/// A field's visual transformation: none (`VisualTransformation::NONE`), a password's mask
/// (`PasswordVisualTransformation`), or one of the application's (`VisualTransformation::new`).
#[derive(Clone)]
pub struct VisualTransformation(Kind);

#[derive(Clone)]
enum Kind {
    None,
    Password(char),
    Custom(Rc<dyn VisualTransform>),
}

impl VisualTransformation {
    /// The text as it is, as `VisualTransformation.None`.
    pub const NONE: VisualTransformation = VisualTransformation(Kind::None);

    pub fn new(transform: impl VisualTransform + 'static) -> VisualTransformation {
        VisualTransformation(Kind::Custom(Rc::new(transform)))
    }

    /// Whether it masks a password: the field's text can't be copied or cut, and its semantics
    /// say so.
    pub fn is_password(&self) -> bool {
        matches!(self.0, Kind::Password(_))
    }

    /// The text shown for `text`, or `None` when it's shown as it is.
    pub(crate) fn filter(&self, text: &str) -> Option<TransformedText> {
        match &self.0 {
            Kind::None => None,
            Kind::Password(mask) => Some(PasswordVisualTransformation { mask: *mask }.filter(text)),
            Kind::Custom(transform) => Some(transform.filter(text)),
        }
    }
}

impl Default for VisualTransformation {
    fn default() -> Self {
        VisualTransformation::NONE
    }
}

impl ComposeArg for VisualTransformation {
    fn same(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Kind::None, Kind::None) => true,
            (Kind::Password(a), Kind::Password(b)) => a == b,
            (Kind::Custom(a), Kind::Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Shows each character as `mask` (•), as Compose's `PasswordVisualTransformation`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PasswordVisualTransformation {
    pub mask: char,
}

impl PasswordVisualTransformation {
    pub fn new() -> PasswordVisualTransformation {
        PasswordVisualTransformation { mask: '\u{2022}' }
    }
}

impl Default for PasswordVisualTransformation {
    fn default() -> Self {
        PasswordVisualTransformation::new()
    }
}

impl From<PasswordVisualTransformation> for VisualTransformation {
    fn from(password: PasswordVisualTransformation) -> VisualTransformation {
        VisualTransformation(Kind::Password(password.mask))
    }
}

impl VisualTransform for PasswordVisualTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        let mask_length = self.mask.len_utf8();
        TransformedText {
            text: std::iter::repeat_n(self.mask, text.chars().count()).collect(),
            offset_mapping: Rc::new(MaskMapping { original: text.to_string(), mask_length }),
        }
    }
}

/// Each character of `original` shown as one mask of `mask_length` bytes.
struct MaskMapping {
    original: String,
    mask_length: usize,
}

impl OffsetMapping for MaskMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        let offset = offset.min(self.original.len());
        self.original[..offset].chars().count() * self.mask_length
    }
    fn transformed_to_original(&self, offset: usize) -> usize {
        let index = offset / self.mask_length;
        self.original.char_indices().nth(index).map_or(self.original.len(), |(at, _)| at)
    }
}
