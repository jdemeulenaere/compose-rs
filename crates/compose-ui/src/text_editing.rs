//! Editing a text field's value, as Compose's `androidx.compose.ui.text.input`: the value
//! ([`TextFieldValue`]: text, selection, composition), the buffer edits are made in
//! ([`EditingBuffer`]), the edits an input method or a key makes ([`EditCommand`] and its
//! commands), and the processor applying them to a value ([`EditProcessor`]).
//!
//! Offsets and lengths are UTF-8 bytes, where Compose's are UTF-16 units (D1100). An offset a
//! command computes or is given that falls inside a character moves to a character boundary: a
//! range widens to the characters it touches, a cursor goes to the start of its character.

use std::fmt;

use compose_runtime::ComposeArg;
use unicode_segmentation::GraphemeCursor;

use crate::annotated::AnnotatedString;
use crate::text_range::TextRange;

/// The largest character boundary of `text` at or before `offset` (at most its length).
pub(crate) fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The smallest character boundary of `text` at or after `offset` (at most its length).
pub(crate) fn ceil_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

/// `range` kept in `text` and at its character boundaries: each end floored, as a cursor.
fn coerce_to_text(range: TextRange, text: &str) -> TextRange {
    TextRange::new(floor_char_boundary(text, range.start), floor_char_boundary(text, range.end))
}

/// The grapheme cluster boundary before `offset`, as Compose's `findPrecedingBreak` (a
/// `BreakIterator`'s character instance); `None` at the start.
fn preceding_break(text: &str, offset: usize) -> Option<usize> {
    if offset == 0 {
        return None;
    }
    GraphemeCursor::new(offset, text.len(), true).prev_boundary(text, 0).ok().flatten()
}

/// The grapheme cluster boundary after `offset`, as Compose's `findFollowingBreak`; `None` at
/// the end.
fn following_break(text: &str, offset: usize) -> Option<usize> {
    if offset >= text.len() {
        return None;
    }
    GraphemeCursor::new(offset, text.len(), true).next_boundary(text, 0).ok().flatten()
}

/// A text field's state, as Compose's `TextFieldValue`: the text, the selection (a cursor when
/// collapsed), and the range an input method is composing, if any. Both ranges are kept in the
/// text when it's made (an end past the text moves to its end, one inside a character to the
/// character's start).
#[derive(Clone, PartialEq)]
pub struct TextFieldValue {
    annotated_string: AnnotatedString,
    selection: TextRange,
    composition: Option<TextRange>,
}

impl TextFieldValue {
    /// As `TextFieldValue(annotatedString, selection, composition)` (and its `text` overload:
    /// a `&str` or `String` converts).
    pub fn new(
        text: impl Into<AnnotatedString>,
        selection: TextRange,
        composition: Option<TextRange>,
    ) -> TextFieldValue {
        let annotated_string = text.into();
        let selection = coerce_to_text(selection, annotated_string.as_str());
        let composition =
            composition.map(|composition| coerce_to_text(composition, annotated_string.as_str()));
        TextFieldValue { annotated_string, selection, composition }
    }

    pub fn text(&self) -> &str {
        self.annotated_string.as_str()
    }

    pub fn annotated_string(&self) -> &AnnotatedString {
        &self.annotated_string
    }

    pub fn selection(&self) -> TextRange {
        self.selection
    }

    pub fn composition(&self) -> Option<TextRange> {
        self.composition
    }

    /// A copy with another text, the ranges kept in it, as `copy(text = …)` (or
    /// `copy(annotatedString = …)`).
    pub fn with_text(&self, text: impl Into<AnnotatedString>) -> TextFieldValue {
        TextFieldValue::new(text, self.selection, self.composition)
    }

    /// A copy with another selection, as `copy(selection = …)`.
    pub fn with_selection(&self, selection: TextRange) -> TextFieldValue {
        TextFieldValue::new(self.annotated_string.clone(), selection, self.composition)
    }

    /// A copy with another composition, as `copy(composition = …)`.
    pub fn with_composition(&self, composition: Option<TextRange>) -> TextFieldValue {
        TextFieldValue::new(self.annotated_string.clone(), self.selection, composition)
    }

    /// The selected text, as Compose's `getSelectedText`.
    pub fn selected_text(&self) -> AnnotatedString {
        self.annotated_string.sub_sequence(self.selection.min(), self.selection.max())
    }

    /// Up to `max_bytes` of the text before the selection, as Compose's `getTextBeforeSelection`
    /// (whole characters: fewer bytes if the limit falls inside one).
    pub fn text_before_selection(&self, max_bytes: usize) -> AnnotatedString {
        let end = self.selection.min();
        let start = ceil_char_boundary(self.text(), end.saturating_sub(max_bytes));
        self.annotated_string.sub_sequence(start, end)
    }

    /// Up to `max_bytes` of the text after the selection, as Compose's `getTextAfterSelection`
    /// (whole characters).
    pub fn text_after_selection(&self, max_bytes: usize) -> AnnotatedString {
        let start = self.selection.max();
        let end = floor_char_boundary(self.text(), start.saturating_add(max_bytes));
        self.annotated_string.sub_sequence(start, end)
    }
}

impl Default for TextFieldValue {
    fn default() -> Self {
        TextFieldValue::new("", TextRange::ZERO, None)
    }
}

impl From<&str> for TextFieldValue {
    fn from(text: &str) -> TextFieldValue {
        TextFieldValue::new(text, TextRange::ZERO, None)
    }
}

impl From<String> for TextFieldValue {
    fn from(text: String) -> TextFieldValue {
        TextFieldValue::new(text, TextRange::ZERO, None)
    }
}

impl fmt::Debug for TextFieldValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "TextFieldValue(text='{}', selection={:?}, composition={:?})",
            self.text(),
            self.selection,
            self.composition
        )
    }
}

impl ComposeArg for TextFieldValue {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The text an [`EditCommand`] edits, with its selection and composition, as Compose's
/// `EditingBuffer`. Its operations are the crate's own, as Compose's are internal: commands
/// edit it, applications go through [`EditProcessor`].
pub struct EditingBuffer {
    text: String,
    selection_start: usize,
    selection_end: usize,
    composition: Option<(usize, usize)>,
}

impl EditingBuffer {
    /// A buffer holding `text` with `selection` (its ends in either order). Panics if an end is
    /// past the text or inside a character.
    pub fn new(text: impl Into<AnnotatedString>, selection: TextRange) -> EditingBuffer {
        let text = text.into().as_str().to_string();
        let (start, end) = (selection.min(), selection.max());
        check_offset(&text, "start", start);
        check_offset(&text, "end", end);
        EditingBuffer { text, selection_start: start, selection_end: end, composition: None }
    }

    pub(crate) fn len(&self) -> usize {
        self.text.len()
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(crate) fn selection_start(&self) -> usize {
        self.selection_start
    }

    pub(crate) fn selection_end(&self) -> usize {
        self.selection_end
    }

    pub(crate) fn selection(&self) -> TextRange {
        TextRange::new(self.selection_start, self.selection_end)
    }

    pub(crate) fn has_composition(&self) -> bool {
        self.composition.is_some()
    }

    pub(crate) fn composition(&self) -> Option<TextRange> {
        self.composition.map(|(start, end)| TextRange::new(start, end))
    }

    /// The cursor when the selection is collapsed (Compose's -1 otherwise).
    pub(crate) fn cursor(&self) -> Option<usize> {
        (self.selection_start == self.selection_end).then_some(self.selection_end)
    }

    pub(crate) fn set_cursor(&mut self, cursor: usize) {
        self.set_selection(cursor, cursor);
    }

    /// Replaces `start..end` with `text`, puts the cursor after it, and ends any composition,
    /// as Compose's `replace`.
    pub(crate) fn replace(&mut self, start: usize, end: usize, text: &str) {
        self.check_range(start, end);
        self.text.replace_range(start..end, text);
        // As Compose: the cursor goes after the new text (a platform setting the selection
        // after a replacement sets it again), and a change ends the composition.
        self.selection_start = start + text.len();
        self.selection_end = start + text.len();
        self.composition = None;
    }

    /// Deletes `start..end` (its ends in either order), moving the selection and composition
    /// with the text, as Compose's `delete`: a composition deleted entirely ends.
    pub(crate) fn delete(&mut self, start: usize, end: usize) {
        let deleted = TextRange::new(start, end);
        check_offset(&self.text, "start", deleted.min());
        check_offset(&self.text, "end", deleted.max());
        self.text.replace_range(deleted.range(), "");
        let selection = update_range_after_delete(self.selection(), deleted);
        self.selection_start = selection.min();
        self.selection_end = selection.max();
        if let Some(composition) = self.composition() {
            let composition = update_range_after_delete(composition, deleted);
            self.composition = if composition.collapsed() {
                None
            } else {
                Some((composition.min(), composition.max()))
            };
        }
    }

    /// Panics if `start > end` or an end is past the text or inside a character.
    pub(crate) fn set_selection(&mut self, start: usize, end: usize) {
        self.check_range(start, end);
        self.selection_start = start;
        self.selection_end = end;
    }

    /// Marks `start..end` as the composition. Panics if it's empty or reversed, or an end is
    /// past the text or inside a character.
    pub(crate) fn set_composition(&mut self, start: usize, end: usize) {
        check_offset(&self.text, "start", start);
        check_offset(&self.text, "end", end);
        assert!(start < end, "Do not set reversed or empty range: {start} > {end}");
        self.composition = Some((start, end));
    }

    /// Removes the composing text and ends the composition, as Compose's `cancelComposition`.
    #[cfg_attr(not(test), expect(dead_code, reason = "Compose's internal API, used by its tests"))]
    pub(crate) fn cancel_composition(&mut self) {
        if let Some((start, end)) = self.composition {
            self.replace(start, end, "");
        }
        self.composition = None;
    }

    /// Ends the composition, keeping its text, as Compose's `commitComposition`.
    pub(crate) fn commit_composition(&mut self) {
        self.composition = None;
    }

    pub(crate) fn to_annotated_string(&self) -> AnnotatedString {
        AnnotatedString::new(self.text.clone())
    }

    fn check_range(&self, start: usize, end: usize) {
        check_offset(&self.text, "start", start);
        check_offset(&self.text, "end", end);
        assert!(start <= end, "Do not set reversed range: {start} > {end}");
    }
}

fn check_offset(text: &str, name: &str, offset: usize) {
    assert!(
        offset <= text.len(),
        "{name} ({offset}) offset is outside of text region {}",
        text.len()
    );
    assert!(text.is_char_boundary(offset), "{name} ({offset}) offset is inside a character");
}

impl fmt::Display for EditingBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

impl fmt::Debug for EditingBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "EditingBuffer(text={:?}, selection={:?}, composition={:?})",
            self.text,
            self.selection(),
            self.composition()
        )
    }
}

/// `target` after `deleted` is removed from the text, as Compose's `updateRangeAfterDelete`.
pub(crate) fn update_range_after_delete(target: TextRange, deleted: TextRange) -> TextRange {
    let (mut min, mut max) = (target.min(), target.max());
    if deleted.intersects(target) {
        if deleted.contains_range(target) {
            // The target is deleted: collapsed where the deletion was.
            min = deleted.min();
            max = min;
        } else if target.contains_range(deleted) {
            // Deleted from within the target: it shrinks.
            max -= deleted.length();
        } else if deleted.contains(min) {
            // The target's start is deleted.
            min = deleted.min();
            max -= deleted.length();
        } else {
            // The target's end is deleted.
            max = deleted.min();
        }
    } else if max > deleted.min() {
        // Deleted before the target: it moves back.
        min -= deleted.length();
        max -= deleted.length();
    }
    TextRange::new(min, max)
}

/// An edit made to an [`EditingBuffer`]: what an input method or a key asks of a text field,
/// as Compose's `EditCommand`.
pub trait EditCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer);

    /// How an [`EditProcessor`]'s error message names the command, as Compose's
    /// `toStringForLog`: the commands here say what they are without the text they carry; any
    /// other is "Unknown EditCommand" and its type's name.
    fn to_string_for_log(&self) -> String {
        let name = std::any::type_name::<Self>();
        let name = name.rsplit("::").next().unwrap_or(name);
        format!("Unknown EditCommand: {name}")
    }
}

/// The new cursor after inserting `inserted` with the cursor at `cursor`, as Android's
/// `newCursorPosition`: after the text plus `position - 1` when positive, before it plus
/// `position` otherwise, kept in the text.
fn cursor_after_insert(buffer: &EditingBuffer, position: isize, inserted: usize) -> usize {
    let cursor = buffer.cursor().unwrap_or(buffer.selection_start()) as isize;
    let target = if position > 0 {
        cursor.saturating_add(position - 1)
    } else {
        cursor.saturating_add(position).saturating_sub(inserted as isize)
    };
    floor_char_boundary(buffer.as_str(), target.clamp(0, buffer.len() as isize) as usize)
}

/// Commits text: replaces the composition (or the selection) with it, ending the composition,
/// and puts the cursor at `new_cursor_position`, as Compose's `CommitTextCommand` and Android's
/// `InputConnection.commitText`: when positive, after the text plus `new_cursor_position - 1`
/// bytes; otherwise before it plus `new_cursor_position`.
#[derive(Clone, Debug)]
pub struct CommitTextCommand {
    pub annotated_string: AnnotatedString,
    pub new_cursor_position: isize,
}

impl CommitTextCommand {
    pub fn new(text: impl Into<AnnotatedString>, new_cursor_position: isize) -> CommitTextCommand {
        CommitTextCommand { annotated_string: text.into(), new_cursor_position }
    }

    pub fn text(&self) -> &str {
        self.annotated_string.as_str()
    }
}

/// Equal when the texts and positions are, as Compose's (the text's styles aside).
impl PartialEq for CommitTextCommand {
    fn eq(&self, other: &Self) -> bool {
        self.text() == other.text() && self.new_cursor_position == other.new_cursor_position
    }
}

impl Eq for CommitTextCommand {}

impl std::hash::Hash for CommitTextCommand {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text().hash(state);
        self.new_cursor_position.hash(state);
    }
}

impl EditCommand for CommitTextCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        let text = self.text();
        match buffer.composition {
            Some((start, end)) => buffer.replace(start, end, text),
            None => buffer.replace(buffer.selection_start(), buffer.selection_end(), text),
        }
        let cursor = cursor_after_insert(buffer, self.new_cursor_position, text.len());
        buffer.set_cursor(cursor);
    }

    fn to_string_for_log(&self) -> String {
        format!(
            "CommitTextCommand(text.length={}, newCursorPosition={})",
            self.text().len(),
            self.new_cursor_position
        )
    }
}

impl fmt::Display for CommitTextCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "CommitTextCommand(text='{}', newCursorPosition={})",
            self.text(),
            self.new_cursor_position
        )
    }
}

/// Marks `start..end` (its ends in either order, kept in the text) as the composition,
/// committing any composition first, as Compose's `SetComposingRegionCommand`; an empty range
/// leaves none (one inside a character too). A range inside characters widens to them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SetComposingRegionCommand {
    pub start: isize,
    pub end: isize,
}

impl SetComposingRegionCommand {
    pub fn new(start: isize, end: isize) -> SetComposingRegionCommand {
        SetComposingRegionCommand { start, end }
    }
}

/// `start..end` (in either order) kept in the buffer's text: a range covering something widened
/// to whole characters, a collapsed one (a cursor) at its character's start (D1112).
fn clamped_range(buffer: &EditingBuffer, start: isize, end: isize) -> (usize, usize) {
    let clamp = |offset: isize| offset.clamp(0, buffer.len() as isize) as usize;
    let (start, end) = (clamp(start), clamp(end));
    let (min, max) = (start.min(end), start.max(end));
    let text = buffer.as_str();
    if min == max {
        let at = floor_char_boundary(text, min);
        return (at, at);
    }
    (floor_char_boundary(text, min), ceil_char_boundary(text, max))
}

impl EditCommand for SetComposingRegionCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        if buffer.has_composition() {
            buffer.commit_composition();
        }
        let (start, end) = clamped_range(buffer, self.start, self.end);
        // An empty range is a cancelled composition (Android's `setComposingRegion` with
        // equal ends): none is set.
        if start < end {
            buffer.set_composition(start, end);
        }
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for SetComposingRegionCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SetComposingRegionCommand(start={}, end={})", self.start, self.end)
    }
}

/// Replaces the composition (or the selection) with text that becomes the composition, and
/// puts the cursor as [`CommitTextCommand`] does, as Compose's `SetComposingTextCommand`.
#[derive(Clone, Debug)]
pub struct SetComposingTextCommand {
    pub annotated_string: AnnotatedString,
    pub new_cursor_position: isize,
}

impl SetComposingTextCommand {
    pub fn new(
        text: impl Into<AnnotatedString>,
        new_cursor_position: isize,
    ) -> SetComposingTextCommand {
        SetComposingTextCommand { annotated_string: text.into(), new_cursor_position }
    }

    pub fn text(&self) -> &str {
        self.annotated_string.as_str()
    }
}

/// Equal when the texts and positions are, as Compose's (the text's styles aside).
impl PartialEq for SetComposingTextCommand {
    fn eq(&self, other: &Self) -> bool {
        self.text() == other.text() && self.new_cursor_position == other.new_cursor_position
    }
}

impl Eq for SetComposingTextCommand {}

impl std::hash::Hash for SetComposingTextCommand {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text().hash(state);
        self.new_cursor_position.hash(state);
    }
}

impl EditCommand for SetComposingTextCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        let text = self.text();
        let (start, end) =
            buffer.composition.unwrap_or((buffer.selection_start(), buffer.selection_end()));
        buffer.replace(start, end, text);
        if !text.is_empty() {
            buffer.set_composition(start, start + text.len());
        }
        let cursor = cursor_after_insert(buffer, self.new_cursor_position, text.len());
        buffer.set_cursor(cursor);
    }

    fn to_string_for_log(&self) -> String {
        format!(
            "SetComposingTextCommand(text.length={}, newCursorPosition={})",
            self.text().len(),
            self.new_cursor_position
        )
    }
}

impl fmt::Display for SetComposingTextCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "SetComposingTextCommand(text='{}', newCursorPosition={})",
            self.text(),
            self.new_cursor_position
        )
    }
}

/// Deletes `length_before_cursor` bytes before the selection and `length_after_cursor` after
/// it, leaving the selection, as Compose's `DeleteSurroundingTextCommand` (UTF-16 units there:
/// D1100); a length reaching into a character deletes the whole character, and one past the
/// text stops at its end.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DeleteSurroundingTextCommand {
    pub length_before_cursor: usize,
    pub length_after_cursor: usize,
}

impl DeleteSurroundingTextCommand {
    pub fn new(length_before_cursor: usize, length_after_cursor: usize) -> Self {
        DeleteSurroundingTextCommand { length_before_cursor, length_after_cursor }
    }
}

impl EditCommand for DeleteSurroundingTextCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        let end = buffer.selection_end().saturating_add(self.length_after_cursor).min(buffer.len());
        let end = ceil_char_boundary(buffer.as_str(), end);
        buffer.delete(buffer.selection_end(), end);
        let start = buffer.selection_start().saturating_sub(self.length_before_cursor);
        let start = floor_char_boundary(buffer.as_str(), start);
        buffer.delete(start, buffer.selection_start());
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for DeleteSurroundingTextCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "DeleteSurroundingTextCommand(lengthBeforeCursor={}, lengthAfterCursor={})",
            self.length_before_cursor, self.length_after_cursor
        )
    }
}

/// Deletes `length_before_cursor` code points (`char`s) before the selection and
/// `length_after_cursor` after it, leaving the selection, as Compose's
/// `DeleteSurroundingTextInCodePointsCommand`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DeleteSurroundingTextInCodePointsCommand {
    pub length_before_cursor: usize,
    pub length_after_cursor: usize,
}

impl DeleteSurroundingTextInCodePointsCommand {
    pub fn new(length_before_cursor: usize, length_after_cursor: usize) -> Self {
        DeleteSurroundingTextInCodePointsCommand { length_before_cursor, length_after_cursor }
    }
}

impl EditCommand for DeleteSurroundingTextInCodePointsCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        let text = buffer.as_str();
        let before: usize = text[..buffer.selection_start()]
            .chars()
            .rev()
            .take(self.length_before_cursor)
            .map(char::len_utf8)
            .sum();
        let after: usize = text[buffer.selection_end()..]
            .chars()
            .take(self.length_after_cursor)
            .map(char::len_utf8)
            .sum();
        buffer.delete(buffer.selection_end(), buffer.selection_end() + after);
        buffer.delete(buffer.selection_start() - before, buffer.selection_start());
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for DeleteSurroundingTextInCodePointsCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "DeleteSurroundingTextInCodePointsCommand(lengthBeforeCursor={}, lengthAfterCursor={})",
            self.length_before_cursor, self.length_after_cursor
        )
    }
}

/// Selects `start..end` (its ends in either order, kept in the text, widened to whole
/// characters), as Compose's `SetSelectionCommand`; the composition stays.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SetSelectionCommand {
    pub start: isize,
    pub end: isize,
}

impl SetSelectionCommand {
    pub fn new(start: isize, end: isize) -> SetSelectionCommand {
        SetSelectionCommand { start, end }
    }
}

impl EditCommand for SetSelectionCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        let (start, end) = clamped_range(buffer, self.start, self.end);
        buffer.set_selection(start, end);
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for SetSelectionCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SetSelectionCommand(start={}, end={})", self.start, self.end)
    }
}

/// Ends the composition, keeping its text, as Compose's `FinishComposingTextCommand`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FinishComposingTextCommand;

impl EditCommand for FinishComposingTextCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        buffer.commit_composition();
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for FinishComposingTextCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FinishComposingTextCommand()")
    }
}

/// What an input method asks of the focused field: the edits Compose's
/// `RecordingInputConnection` sends as commands, in bytes (D1100, D1113): the platform's bridge
/// converts an input method's UTF-16 positions and lengths with the field's text
/// (`UiHost::text_input_value`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum InputMethodCommand {
    CommitText(CommitTextCommand),
    DeleteSurroundingText(DeleteSurroundingTextCommand),
    SetComposingRegion(SetComposingRegionCommand),
    FinishComposingText(FinishComposingTextCommand),
}

/// Deletes the composition, else the selection, else the grapheme cluster before the cursor,
/// as Compose's `BackspaceCommand`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct BackspaceCommand;

impl EditCommand for BackspaceCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        if let Some((start, end)) = buffer.composition {
            buffer.delete(start, end);
            return;
        }
        let Some(cursor) = buffer.cursor() else {
            let (start, end) = (buffer.selection_start(), buffer.selection_end());
            buffer.set_cursor(start);
            buffer.delete(start, end);
            return;
        };
        if let Some(previous) = preceding_break(buffer.as_str(), cursor) {
            buffer.delete(previous, cursor);
        }
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for BackspaceCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BackspaceCommand()")
    }
}

/// Moves the cursor `amount` grapheme clusters (left when negative) from the selection's
/// start, collapsing the selection, as Compose's `MoveCursorCommand`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MoveCursorCommand {
    pub amount: isize,
}

impl MoveCursorCommand {
    pub fn new(amount: isize) -> MoveCursorCommand {
        MoveCursorCommand { amount }
    }
}

impl EditCommand for MoveCursorCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        if buffer.cursor().is_none() {
            buffer.set_cursor(buffer.selection_start());
        }
        let mut cursor = buffer.selection_start();
        let text = buffer.as_str();
        for _ in 0..self.amount.unsigned_abs() {
            let next = if self.amount > 0 {
                following_break(text, cursor)
            } else {
                preceding_break(text, cursor)
            };
            match next {
                Some(next) => cursor = next,
                None => break,
            }
        }
        buffer.set_cursor(cursor);
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for MoveCursorCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "MoveCursorCommand(amount={})", self.amount)
    }
}

/// Deletes the whole text, as Compose's `DeleteAllCommand`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct DeleteAllCommand;

impl EditCommand for DeleteAllCommand {
    fn apply_to(&self, buffer: &mut EditingBuffer) {
        buffer.replace(0, buffer.len(), "");
    }

    fn to_string_for_log(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for DeleteAllCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeleteAllCommand()")
    }
}

/// Applies [`EditCommand`]s to a text field's value, as Compose's `EditProcessor`: `reset`
/// gives it the value the application holds, `apply` the edits, which it returns as the new
/// value for the application to take (or not).
pub struct EditProcessor {
    buffer_state: TextFieldValue,
    buffer: EditingBuffer,
    /// How many buffers it made: tests check when a reset makes a new one.
    #[cfg(test)]
    buffers_made: usize,
}

impl Default for EditProcessor {
    fn default() -> Self {
        EditProcessor::new()
    }
}

impl EditProcessor {
    pub fn new() -> EditProcessor {
        let buffer_state = TextFieldValue::default();
        let buffer = EditingBuffer::new(buffer_state.annotated_string.clone(), TextRange::ZERO);
        EditProcessor {
            buffer_state,
            buffer,
            #[cfg(test)]
            buffers_made: 1,
        }
    }

    /// Takes `value` as the text field's state, as Compose's `reset` (its input session
    /// argument, deprecated, has no counterpart): a new text makes a new buffer and ends any
    /// composition; a new selection alone keeps the buffer and its composition; a new
    /// composition the input method didn't make ends it.
    pub fn reset(&mut self, value: TextFieldValue) {
        let mut text_changed = false;
        let mut selection_changed = false;
        let composition_changed = value.composition != self.buffer.composition();
        if self.buffer_state.text() != value.text() {
            self.buffer = EditingBuffer::new(value.annotated_string.clone(), value.selection);
            #[cfg(test)]
            {
                self.buffers_made += 1;
            }
            text_changed = true;
        } else if self.buffer_state.selection != value.selection {
            self.buffer.set_selection(value.selection.min(), value.selection.max());
            selection_changed = true;
        }
        match value.composition {
            None => self.buffer.commit_composition(),
            Some(composition) if !composition.collapsed() => {
                self.buffer.set_composition(composition.min(), composition.max())
            }
            Some(_) => {}
        }
        self.buffer_state = if text_changed || (!selection_changed && composition_changed) {
            self.buffer.commit_composition();
            value.with_composition(None)
        } else {
            value
        };
    }

    /// Applies `commands` in order and returns the new value, as Compose's `apply`: a reversed
    /// selection stays reversed. A command that panics panics again with the batch in the
    /// message, the failing one marked.
    pub fn apply(&mut self, commands: &[&dyn EditCommand]) -> TextFieldValue {
        for (index, command) in commands.iter().enumerate() {
            let buffer = &mut self.buffer;
            let applied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                command.apply_to(buffer);
            }));
            if let Err(cause) = applied {
                let cause = cause
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| cause.downcast_ref::<&str>().copied())
                    .unwrap_or("a panic");
                panic!("{}\nCaused by: {cause}", self.batch_error_message(commands, index));
            }
        }
        let selection = self.buffer.selection();
        let selection = if self.buffer_state.selection.reversed() {
            TextRange::new(selection.max(), selection.min())
        } else {
            selection
        };
        let value = TextFieldValue::new(
            self.buffer.to_annotated_string(),
            selection,
            self.buffer.composition(),
        );
        self.buffer_state = value.clone();
        value
    }

    /// The buffer's state, as Compose's `toTextFieldValue`.
    pub fn to_text_field_value(&self) -> TextFieldValue {
        self.buffer_state.clone()
    }

    fn batch_error_message(&self, commands: &[&dyn EditCommand], failed: usize) -> String {
        let mut message = format!(
            "Error while applying EditCommand batch to buffer (length={}, composition={}, \
             selection={:?}):",
            self.buffer.len(),
            self.buffer.composition().map_or("null".to_string(), |range| format!("{range:?}")),
            self.buffer.selection(),
        );
        for (index, command) in commands.iter().enumerate() {
            let prefix = if index == failed { " > " } else { "   " };
            message.push('\n');
            message.push_str(prefix);
            message.push_str(&command.to_string_for_log());
        }
        message
    }
}

#[cfg(test)]
#[path = "text_editing_tests.rs"]
mod tests;
