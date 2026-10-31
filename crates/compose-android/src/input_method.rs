//! What the soft keyboard is told of the focused field: its `ImeOptions` as Android's
//! `EditorInfo` (`inputType` and `imeOptions`), as Compose's `EditorInfo.update(imeOptions,
//! textFieldValue)` in `TextInputServiceAndroid`; and the keyboard's action key back as an
//! `ImeAction`, as Compose's `RecordingInputConnection.performEditorAction`. Kotlin only applies
//! the two ints.
//!
//! And the rest of `RecordingInputConnection`, which `ComposeRsInputConnection` passes on as it
//! comes (D1664): [`InputConnections`] holds what the keyboard was last told and each connection's
//! state (open, its batch), turns the keyboard's UTF-16 numbers into Compose's edit commands in
//! bytes, and answers its questions about the text. Kotlin applies what it says.

use compose_ui::platform::UiHost;
use compose_ui::{
    CommitTextCommand, CursorAnchor, CursorAnchorBox, DeleteSurroundingTextCommand,
    DeleteSurroundingTextInCodePointsCommand, EditCommand, EditProcessor,
    FinishComposingTextCommand, ImeAction, ImeOptions, Key, KeyEvent, KeyboardCapitalization,
    KeyboardType, SetComposingRegionCommand, SetComposingTextCommand, SetSelectionCommand,
    TextFieldValue,
};

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
/// One departure, D250: the text flags (multi-line, capitalization, auto-correct) go to the text
/// class only (Compose's `hasFlag(inputType, TYPE_CLASS_TEXT)` also matches the phone class, 3).
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
    EditorInfo { input_type, ime_options: ime_options | IME_FLAG_NO_FULLSCREEN }
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

/// The bytes of `chars`' first `units` UTF-16 units. A count ending inside a surrogate pair takes
/// its character when `widen` (a range covers what it touches), else stops before it (a cursor
/// goes to its character's start), as D1100 says, where Compose's would split the pair; a count
/// past the text stops at its end.
fn bytes_of_units(chars: impl Iterator<Item = char>, units: usize, widen: bool) -> usize {
    let (mut bytes, mut left) = (0, units);
    for c in chars {
        if left == 0 || (c.len_utf16() > left && !widen) {
            break;
        }
        bytes += c.len_utf8();
        left = left.saturating_sub(c.len_utf16());
    }
    bytes
}

/// A UTF-16 index of `text`, as a keyboard is told it.
fn units_to(text: &str, byte: usize) -> i32 {
    text[..byte].encode_utf16().count() as i32
}

/// An edit a keyboard asks for, with its numbers as it gave them: UTF-16 units, read against the
/// text it applies to, which the edits before it in a batch may have changed.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Edit {
    CommitText(String, i32),
    SetComposingText(String, i32),
    SetComposingRegion(i32, i32),
    DeleteSurroundingText(i32, i32),
    DeleteSurroundingTextInCodePoints(i32, i32),
    SetSelection(i32, i32),
    FinishComposingText,
}

/// `new_cursor_position` relative to text taking the place of `value`'s composition or selection,
/// as a command's: in bytes of what's around it (`CommitTextCommand`, `SetComposingTextCommand`).
fn cursor_position_in_bytes(value: &TextFieldValue, new_cursor_position: i32) -> isize {
    let replaced = value.composition().unwrap_or(value.selection());
    if new_cursor_position > 0 {
        let units = new_cursor_position as usize - 1;
        bytes_of_units(value.text()[replaced.max()..].chars(), units, false) as isize + 1
    } else {
        let units = new_cursor_position.unsigned_abs() as usize;
        -(bytes_of_units(value.text()[..replaced.min()].chars().rev(), units, true) as isize)
    }
}

/// `edit` as Compose's command in bytes, against `value`.
fn command_of(edit: &Edit, value: &TextFieldValue) -> Box<dyn EditCommand> {
    let text = value.text();
    match edit {
        Edit::CommitText(inserted, position) => {
            let position = cursor_position_in_bytes(value, *position);
            Box::new(CommitTextCommand::new(inserted.as_str(), position))
        }
        Edit::SetComposingText(composing, position) => {
            let position = cursor_position_in_bytes(value, *position);
            Box::new(SetComposingTextCommand::new(composing.as_str(), position))
        }
        Edit::SetComposingRegion(start, end) => {
            let (from, to) = range_in_bytes(text, *start, *end);
            Box::new(SetComposingRegionCommand::new(from, to))
        }
        Edit::SetSelection(start, end) => {
            let (from, to) = range_in_bytes(text, *start, *end);
            Box::new(SetSelectionCommand::new(from, to))
        }
        Edit::DeleteSurroundingText(before, after) => {
            let selection = value.selection();
            let (before, after) = ((*before).max(0) as usize, (*after).max(0) as usize);
            // A selection goes itself: a keyboard sending this for its Backspace means it, where
            // Compose's would leave the selection and delete around it (D1665).
            if !selection.collapsed() && value.composition().is_none() && before + after > 0 {
                return Box::new(CommitTextCommand::new("", 1));
            }
            let before = bytes_of_units(text[..selection.min()].chars().rev(), before, true);
            let after = bytes_of_units(text[selection.max()..].chars(), after, true);
            Box::new(DeleteSurroundingTextCommand::new(before, after))
        }
        Edit::DeleteSurroundingTextInCodePoints(before, after) => {
            let (before, after) = ((*before).max(0) as usize, (*after).max(0) as usize);
            Box::new(DeleteSurroundingTextInCodePointsCommand::new(before, after))
        }
        Edit::FinishComposingText => Box::new(FinishComposingTextCommand),
    }
}

/// UTF-16 `start` and `end` of `text` in bytes, kept in it and in either order as they came:
/// widened to the characters they touch, a collapsed one at its character's start.
fn range_in_bytes(text: &str, start: i32, end: i32) -> (isize, isize) {
    let (start, end) = (start.max(0) as usize, end.max(0) as usize);
    let (min, max) = (start.min(end), start.max(end));
    let from = bytes_of_units(text.chars(), min, false) as isize;
    let to = bytes_of_units(text.chars(), max, min != max) as isize;
    if start <= end { (from, to) } else { (to, from) }
}

/// What a keyboard asks to be told as the layout changes (`requestCursorUpdates`), read as
/// Compose's `RecordingInputConnection`: no filter flag means all of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CursorUpdates {
    pub immediate: bool,
    pub monitor: bool,
    pub insertion_marker: bool,
    pub character_bounds: bool,
    pub editor_bounds: bool,
    pub line_bounds: bool,
}

impl CursorUpdates {
    /// `mode`, `InputConnection.requestCursorUpdates`'s, on API level `sdk`: filter flags from
    /// Android 13, line bounds from 14.
    pub fn from_mode(mode: i32, sdk: i32) -> CursorUpdates {
        const IMMEDIATE: i32 = 1;
        const MONITOR: i32 = 2;
        const EDITOR_BOUNDS: i32 = 4;
        const CHARACTER_BOUNDS: i32 = 8;
        const INSERTION_MARKER: i32 = 16;
        const LINE_BOUNDS: i32 = 32;
        const TIRAMISU: i32 = 33;
        const UPSIDE_DOWN_CAKE: i32 = 34;
        let mut updates = CursorUpdates {
            immediate: mode & IMMEDIATE != 0,
            monitor: mode & MONITOR != 0,
            insertion_marker: true,
            character_bounds: true,
            editor_bounds: false,
            line_bounds: false,
        };
        if sdk >= TIRAMISU {
            updates.insertion_marker = mode & INSERTION_MARKER != 0;
            updates.character_bounds = mode & CHARACTER_BOUNDS != 0;
            updates.editor_bounds = mode & EDITOR_BOUNDS != 0;
            if sdk >= UPSIDE_DOWN_CAKE {
                updates.line_bounds = mode & LINE_BOUNDS != 0;
            }
            if !updates.insertion_marker
                && !updates.character_bounds
                && !updates.editor_bounds
                && !updates.line_bounds
            {
                updates.insertion_marker = true;
                updates.character_bounds = true;
                updates.editor_bounds = true;
                updates.line_bounds = sdk >= UPSIDE_DOWN_CAKE;
            }
        }
        updates
    }
}

/// What `InputMethodManager` should be told of the text since the last time, in UTF-16 units: its
/// `updateSelection`, -1 for no composition, and its `updateExtractedText` token if the keyboard
/// asked to follow the text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SelectionUpdate {
    pub selection_start: i32,
    pub selection_end: i32,
    pub composition_start: i32,
    pub composition_end: i32,
    pub extracted_text_token: Option<i32>,
}

/// The selection's and composition's ends of `value` in UTF-16 units, composition -1 without one.
pub fn input_state_units(value: &TextFieldValue) -> [i32; 4] {
    let (text, selection) = (value.text(), value.selection());
    let composition = value.composition();
    [
        units_to(text, selection.min()),
        units_to(text, selection.max()),
        composition.map_or(-1, |range| units_to(text, range.min())),
        composition.map_or(-1, |range| units_to(text, range.max())),
    ]
}

/// The `max_units` UTF-16 units of `value` before its selection, as `getTextBeforeCursor`: a pair
/// the count ends in is left out, as a `String` can't hold half of one (Compose's returns half).
pub fn text_before_cursor(value: &TextFieldValue, max_units: i32) -> &str {
    let end = value.selection().min();
    let before = &value.text()[..end];
    let bytes = bytes_of_units(before.chars().rev(), max_units.max(0) as usize, false);
    &before[end - bytes..]
}

/// The `max_units` UTF-16 units of `value` after its selection, as `getTextAfterCursor`.
pub fn text_after_cursor(value: &TextFieldValue, max_units: i32) -> &str {
    let after = &value.text()[value.selection().max()..];
    &after[..bytes_of_units(after.chars(), max_units.max(0) as usize, false)]
}

/// The text `value` has selected, as `getSelectedText`: `None` for a cursor.
pub fn selected_text(value: &TextFieldValue) -> Option<&str> {
    let selection = value.selection();
    (!selection.collapsed()).then(|| &value.text()[selection.range()])
}

/// What one connection holds, as Compose's `RecordingInputConnection`: open until closed, its
/// edits held while a batch is open, and the node it was opened for.
struct ConnectionState {
    id: i64,
    active: bool,
    batch_depth: u32,
    edits: Vec<Edit>,
    /// The focused node when it opened: an edit in flight when the focus has moved is dropped,
    /// not applied to the next field (D1667).
    node: Option<compose_ui::platform::NodeId>,
}

/// What all a view's connections share, as there's one `InputMethodManager` per view, which
/// serves one and takes `updateSelection` from any: what the keyboard was told and asked to hear.
#[derive(Default)]
struct KeyboardState {
    /// What the keyboard knows: what it read when connecting, then each update.
    told: Option<TextFieldValue>,
    /// The connection that asked to follow the text (`getExtractedText`'s monitor mode), and its
    /// token: honoured only while that connection is open.
    extracted_text_token: Option<(i64, i32)>,
    cursor_updates: CursorUpdates,
    /// What the last `CursorAnchorInfo` sent depended on (`UiHost::cursor_anchor_key`).
    last_anchor_key: Option<(compose_ui::CursorAnchorKey, compose_ui::Offset)>,
    /// What's composed for a node without a value, which only gets the text when it's committed
    /// (`send_preedit`, `send_text`), and that node: the next one isn't to hear of it.
    composing: Option<(Option<compose_ui::platform::NodeId>, String)>,
}

/// A connection to a keyboard, as Compose's `RecordingInputConnection`: what it's asked, in UTF-16
/// units, which it turns into Compose's commands in bytes.
pub struct InputConnection<'a> {
    state: &'a mut ConnectionState,
    keyboard: &'a mut KeyboardState,
}

impl InputConnection<'_> {
    pub fn is_active(&self) -> bool {
        self.state.active
    }

    /// Whether what it's asked goes where it was opened: it's open and the focus hasn't moved. Keys
    /// and actions from one that isn't would act on the next field (D1667, D1670).
    pub fn is_for_focused_node(&self, ui: &UiHost) -> bool {
        self.state.active && ui.focused_node() == self.state.node
    }

    /// Closes it: its batch is dropped, and nothing it's asked after does anything.
    pub fn close(&mut self) {
        self.state.edits.clear();
        self.state.batch_depth = 0;
        self.state.active = false;
    }

    pub fn begin_batch_edit(&mut self) -> bool {
        if self.state.active {
            self.state.batch_depth += 1;
        }
        self.state.active
    }

    /// Ends a batch, applying its edits when it's the outermost. Returns whether one is still open,
    /// as Android's `endBatchEdit`.
    pub fn end_batch_edit(&mut self, ui: &mut UiHost) -> bool {
        self.state.batch_depth = self.state.batch_depth.saturating_sub(1);
        if self.state.batch_depth == 0 && !self.state.edits.is_empty() {
            let edits = std::mem::take(&mut self.state.edits);
            self.apply(ui, &edits);
        }
        self.state.batch_depth > 0
    }

    fn edit(&mut self, ui: &mut UiHost, edit: Edit) -> bool {
        if self.state.active {
            self.state.edits.push(edit);
            self.state.batch_depth += 1;
            self.end_batch_edit(ui);
        }
        self.state.active
    }

    pub fn commit_text(&mut self, ui: &mut UiHost, text: &str, new_cursor_position: i32) -> bool {
        let text = single_line_text(ui, text);
        self.edit(ui, Edit::CommitText(text, new_cursor_position))
    }

    pub fn set_composing_text(
        &mut self,
        ui: &mut UiHost,
        text: &str,
        new_cursor_position: i32,
    ) -> bool {
        let text = single_line_text(ui, text);
        self.edit(ui, Edit::SetComposingText(text, new_cursor_position))
    }

    pub fn set_composing_region(&mut self, ui: &mut UiHost, start: i32, end: i32) -> bool {
        self.edit(ui, Edit::SetComposingRegion(start, end))
    }

    pub fn delete_surrounding_text(&mut self, ui: &mut UiHost, before: i32, after: i32) -> bool {
        self.edit(ui, Edit::DeleteSurroundingText(before, after))
    }

    pub fn delete_surrounding_text_in_code_points(
        &mut self,
        ui: &mut UiHost,
        before: i32,
        after: i32,
    ) -> bool {
        self.edit(ui, Edit::DeleteSurroundingTextInCodePoints(before, after))
    }

    pub fn set_selection(&mut self, ui: &mut UiHost, start: i32, end: i32) -> bool {
        self.edit(ui, Edit::SetSelection(start, end))
    }

    pub fn finish_composing_text(&mut self, ui: &mut UiHost) -> bool {
        self.edit(ui, Edit::FinishComposingText)
    }

    /// `commitCorrection`: taken while the field auto-corrects, as Compose's (nothing shows it yet).
    pub fn commit_correction(&self, ui: &UiHost) -> bool {
        self.state.active && ui.text_input_options().is_some_and(|options| options.auto_correct)
    }

    /// `performContextMenuAction`'s select all, as Compose's.
    pub fn select_all(&mut self, ui: &mut UiHost) -> bool {
        self.edit(ui, Edit::SetSelection(0, i32::MAX))
    }

    /// `performEditorAction`: the field's action for the keyboard's key.
    pub fn perform_editor_action(&mut self, ui: &mut UiHost, editor_action: i32) -> bool {
        if self.is_for_focused_node(ui) {
            ui.perform_ime_action(ime_action(editor_action));
        }
        self.state.active
    }

    /// Applies `edits`, as one to a field with a value: each read against what those before it
    /// left, so the application hears one change.
    fn apply(&mut self, ui: &mut UiHost, edits: &[Edit]) {
        if !self.is_for_focused_node(ui) {
            return;
        }
        let Some(value) = ui.input_method_value() else {
            for edit in edits {
                self.apply_without_value(ui, edit);
            }
            return;
        };
        let mut processor = EditProcessor::from_value(value.clone());
        let mut current = value;
        let mut commands = Vec::with_capacity(edits.len());
        for edit in edits {
            let command = command_of(edit, &current);
            current = processor.apply(&[command.as_ref()]);
            commands.push(command);
        }
        let commands: Vec<&dyn EditCommand> =
            commands.iter().map(|command| command.as_ref()).collect();
        ui.send_edit_commands(&commands);
    }

    /// `edit` for a node without a value, a custom `KeyInputModifierNode`: text when it's committed
    /// and the composition shown, deletions as keys. A region or a selection has nothing to name.
    fn apply_without_value(&mut self, ui: &mut UiHost, edit: &Edit) {
        match edit {
            Edit::CommitText(text, _) => {
                if !self.take_composing().is_empty() {
                    ui.send_preedit("", None);
                }
                ui.send_text(text);
            }
            Edit::SetComposingText(text, _) => {
                self.keyboard.composing = Some((self.state.node, text.clone()));
                ui.send_preedit(text, None);
            }
            Edit::FinishComposingText => {
                let text = self.take_composing();
                if !text.is_empty() {
                    ui.send_preedit("", None);
                    ui.send_text(&text);
                }
            }
            Edit::DeleteSurroundingText(before, after)
            | Edit::DeleteSurroundingTextInCodePoints(before, after) => {
                for key in std::iter::repeat_n(Key::Backspace, (*before).max(0) as usize)
                    .chain(std::iter::repeat_n(Key::Delete, (*after).max(0) as usize))
                {
                    ui.send_key(KeyEvent::down(key));
                    ui.send_key(KeyEvent::up(key));
                }
            }
            Edit::SetComposingRegion(..) | Edit::SetSelection(..) => {}
        }
    }

    /// What this connection's node has composed, taken; another node's goes.
    fn take_composing(&mut self) -> String {
        match self.keyboard.composing.take() {
            Some((node, text)) if node == self.state.node => text,
            _ => String::new(),
        }
    }

    /// `getExtractedText`'s flags: follow the text with `updateExtractedText` from now on, with
    /// `token`, or stop.
    pub fn set_extracted_text_monitor(&mut self, token: Option<i32>) {
        if self.state.active {
            self.keyboard.extracted_text_token = token.map(|token| (self.state.id, token));
        }
    }

    /// `requestCursorUpdates`. Returns whether it's open.
    pub fn request_cursor_updates(&mut self, updates: CursorUpdates) -> bool {
        if self.state.active {
            self.keyboard.cursor_updates = updates;
            self.keyboard.last_anchor_key = None;
        }
        self.state.active
    }
}

/// `text` without its line feeds in a focused single-line field (D1664).
fn single_line_text(ui: &UiHost, text: &str) -> String {
    if ui.text_input_options().is_some_and(|options| options.single_line) {
        text.chars().filter(|c| *c != '\n').collect()
    } else {
        text.to_string()
    }
}

/// `CursorAnchorInfo`'s flags.
fn cursor_anchor_flags(anchor_box: &CursorAnchorBox) -> f32 {
    const HAS_VISIBLE_REGION: i32 = 1;
    const HAS_INVISIBLE_REGION: i32 = 2;
    const IS_RTL: i32 = 4;
    let mut flags = 0;
    if anchor_box.visible {
        flags |= HAS_VISIBLE_REGION;
    }
    if anchor_box.invisible {
        flags |= HAS_INVISIBLE_REGION;
    }
    if anchor_box.right_to_left {
        flags |= IS_RTL;
    }
    flags as f32
}

/// How many floats come before a [`encode_cursor_anchor`]'s characters and lines.
pub const CURSOR_ANCHOR_HEADER: usize = 19;

/// `anchor` and `value` as `ComposeRsView` reads them to build a `CursorAnchorInfo`: the layout's
/// origin in the root (2 floats); the selection's and composition's ends in UTF-16 units (4); the
/// insertion marker's presence, x, top, bottom and flags (5); the editor bounds' presence, left,
/// top, right and bottom (5); how many characters, how many lines (2) and whether it's for line
/// bounds (1); per character its UTF-16 index, left, top, right, bottom and flags, and per line
/// its left, top, right and bottom.
pub fn encode_cursor_anchor(
    anchor: &CursorAnchor,
    value: &TextFieldValue,
    updates: CursorUpdates,
) -> Vec<f32> {
    let [start, end, composition_start, composition_end] = input_state_units(value);
    let marker = &anchor.insertion_marker;
    let bounds = anchor.editor_bounds;
    let characters: &[(usize, CursorAnchorBox)] =
        if updates.character_bounds { &anchor.characters } else { &[] };
    let mut out = vec![
        anchor.origin.x,
        anchor.origin.y,
        start as f32,
        end as f32,
        composition_start as f32,
        composition_end as f32,
        f32::from(updates.insertion_marker),
        marker.rect.left,
        marker.rect.top,
        marker.rect.bottom,
        cursor_anchor_flags(marker),
        f32::from(updates.editor_bounds),
        bounds.left,
        bounds.top,
        bounds.right,
        bounds.bottom,
        characters.len() as f32,
        anchor.visible_lines.len() as f32,
        f32::from(updates.line_bounds),
    ];
    for (offset, character) in characters {
        out.push(units_to(value.text(), *offset) as f32);
        out.extend([
            character.rect.left,
            character.rect.top,
            character.rect.right,
            character.rect.bottom,
            cursor_anchor_flags(character),
        ]);
    }
    for line in &anchor.visible_lines {
        out.extend([line.left, line.top, line.right, line.bottom]);
    }
    out
}

/// How many open connections a view keeps: Android asks for ones it then throws away and never
/// closes, so some must go; the oldest first.
const MOST_OPEN_CONNECTIONS: usize = 8;

/// A view's keyboard connections, as Compose's `ics`: each usable until its own `closeConnection`.
/// On `MSG_BIND` (a cold start, or after a keyboard switch, where the one it keeps is the one
/// `MSG_UNBIND` just created) Android asks for a new one, throws it away and keeps serving the one
/// before, so a newer connection says nothing of an older one's use (D1667, D1670);
/// `RemoteInputConnectionImpl` refuses what a keyboard asks of one it has deactivated.
#[derive(Default)]
pub struct InputConnections {
    connections: Vec<ConnectionState>,
    keyboard: KeyboardState,
}

/// Ids are the process's, as a connection can outlive its view's host and meet the next one's.
static NEXT_CONNECTION_ID: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);

impl InputConnections {
    /// A new connection to the focused field. Returns its id.
    pub fn open(&mut self, ui: &UiHost) -> i64 {
        let id = NEXT_CONNECTION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.connections.retain(|state| state.active);
        if self.connections.len() == MOST_OPEN_CONNECTIONS {
            let dropped = self.connections.remove(0);
            // Kotlin closes the ones collected, so this takes a keyboard binding many times over
            // between collections. If it's the one in use nothing types: say so (D1670).
            crate::bridge::log(&format!(
                "connection {} dropped: {MOST_OPEN_CONNECTIONS} open",
                dropped.id
            ));
        }
        self.connections.push(ConnectionState {
            id,
            active: true,
            batch_depth: 0,
            edits: Vec::new(),
            node: ui.focused_node(),
        });
        // What it starts from is what `EditorInfo` said, but the connection in use may be another,
        // so a change the keyboard hasn't heard of stays to tell. A restart, which reads anew, set
        // this already.
        if self.keyboard.told.is_none() {
            self.keyboard.told = ui.input_method_value();
        }
        id
    }

    /// Connection `id`, while it's kept: one closed answers false to everything.
    pub fn get(&mut self, id: i64) -> Option<InputConnection<'_>> {
        let state = self.connections.iter_mut().find(|state| state.id == id)?;
        Some(InputConnection { state, keyboard: &mut self.keyboard })
    }

    /// Closes connection `id`: only it.
    pub fn close(&mut self, id: i64) {
        if let Some(mut connection) = self.get(id) {
            connection.close();
        }
    }

    fn any_open(&self) -> bool {
        self.connections.iter().any(|state| state.active)
    }

    /// Whether some connection is open, none with a batch in progress: when the keyboard hears of
    /// edits, once they've applied.
    fn may_tell(&self) -> bool {
        self.any_open() && self.connections.iter().all(|state| state.batch_depth == 0)
    }

    /// Whether the keyboard has something to hear: the text changed from what it was told, or the
    /// cursor from where it's been told, when it asked. Copies nothing, and asks the measurer
    /// nothing: it's asked each frame.
    pub fn has_update(&self, ui: &UiHost) -> bool {
        if !self.any_open() {
            return false;
        }
        let updates = self.keyboard.cursor_updates;
        if updates.immediate
            || (updates.monitor
                && ui
                    .cursor_anchor_key()
                    .is_some_and(|key| Some(key) != self.keyboard.last_anchor_key))
        {
            return true;
        }
        self.may_tell()
            && match &self.keyboard.told {
                Some(told) => ui.input_method_value_is(told) == Some(false),
                None => ui.input_method_value().is_some(),
            }
    }

    /// After a frame: whether the keyboard has something to hear of the text or the cursor. Not of
    /// a text it's restarting to read anew (`restart`), as Compose's `updateState` tells it either.
    pub fn frame_has_update(&mut self, ui: &UiHost, restart: bool) -> bool {
        if restart {
            self.keyboard.told = ui.input_method_value();
        }
        self.has_update(ui)
    }

    /// What the keyboard hasn't heard of the text, as Compose's `updateInputState`, on each change
    /// of the selection, composition or text; `None` when it's heard it all.
    pub fn take_selection_update(&mut self, ui: &UiHost) -> Option<SelectionUpdate> {
        if !self.may_tell() {
            return None;
        }
        if let Some(told) = &self.keyboard.told
            && ui.input_method_value_is(told) != Some(false)
        {
            return None;
        }
        let value = ui.input_method_value()?;
        let [selection_start, selection_end, composition_start, composition_end] =
            input_state_units(&value);
        self.keyboard.told = Some(value);
        Some(SelectionUpdate {
            selection_start,
            selection_end,
            composition_start,
            composition_end,
            extracted_text_token: self.keyboard.extracted_text_token.and_then(|(id, token)| {
                self.connections.iter().any(|state| state.id == id && state.active).then_some(token)
            }),
        })
    }

    /// The `CursorAnchorInfo` the keyboard asked for, encoded (`encode_cursor_anchor`): at once if
    /// asked for once, and on each change while monitoring, compared before asking the measurer.
    pub fn take_cursor_anchor(&mut self, ui: &UiHost) -> Option<Vec<f32>> {
        let updates = self.keyboard.cursor_updates;
        if !self.any_open() || !(updates.immediate || updates.monitor) {
            return None;
        }
        let key = ui.cursor_anchor_key()?;
        if !updates.immediate && Some(key) == self.keyboard.last_anchor_key {
            return None;
        }
        let anchor = ui.cursor_anchor(updates.line_bounds)?;
        let value = ui.input_method_value()?;
        self.keyboard.cursor_updates.immediate = false;
        self.keyboard.last_anchor_key = Some(key);
        Some(encode_cursor_anchor(&anchor, &value, updates))
    }
}

/// What a frame tells the view of text input.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TextInputState {
    /// A focused node takes text: the view is a text editor and its keyboard shows.
    pub active: bool,
    /// The input method starts again, reading the field anew: a preedit was interrupted, the text
    /// changed other than by its edits or typing, a field took the focus, or the focused field's
    /// options changed, as Compose's `TextInputServiceAndroid` restarting the input.
    pub restart: bool,
    /// What the content last asked of the soft keyboard: show (`true`) or hide (`false`).
    pub keyboard: Option<bool>,
}

/// What the last frame saw of what takes text.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LastTextInput {
    options: Option<ImeOptions>,
    node: Option<compose_ui::platform::NodeId>,
    masks_password: bool,
}

/// Takes what `ui` asks of the input method after a frame (the events before it having woken
/// it), `last` being what the last frame saw: the options, the node, and whether it masked its text.
/// Any of them changes what `editor_info` says or whose connection it is, so restarts the keyboard.
pub fn take_text_input_state(
    ui: &compose_ui::platform::UiHost,
    last: &mut LastTextInput,
) -> TextInputState {
    let options = if ui.text_input_area().is_some() { ui.text_input_options() } else { None };
    let masks_password = options.is_some() && ui.text_input_masks_password();
    let node = ui.focused_node();
    // Another node with the same options is another field too, whatever it keeps: a connection
    // stays its own node's (D1670).
    let changed = options.is_some()
        && last.options.is_some()
        && (options != last.options || node != last.node || masks_password != last.masks_password);
    *last = LastTextInput { options, node, masks_password };
    TextInputState {
        active: options.is_some(),
        // Both taken: neither is left for a later frame.
        restart: ui.take_input_method_reset() | ui.take_input_method_restart() | changed,
        keyboard: ui.take_software_keyboard_request(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALWAYS: i32 = IME_FLAG_NO_FULLSCREEN;

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
