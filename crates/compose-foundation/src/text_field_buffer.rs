//! What a [`TextFieldState`](crate::TextFieldState) holds and the buffer its edits go through, as
//! Compose's `TextFieldCharSequence`, `TextFieldBuffer` and `ChangeTracker`. Offsets are UTF-8
//! byte offsets at character boundaries, as every text offset in compose-rs (D1100): an offset
//! given inside a character moves to a boundary, a range widening over the characters it touches.

use std::fmt;
use std::rc::Rc;

use compose_ui::TextRange;

/// The start of the character `offset` is in (`offset` itself at a boundary), `offset` kept in
/// the text.
pub(crate) fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The end of the character `offset` is in (`offset` itself at a boundary), `offset` kept in
/// the text.
pub(crate) fn ceil_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

/// `range` at character boundaries of `text`: a collapsed one at its character's start (D1112),
/// another widened over the characters it touches, its direction kept.
fn range_at_boundaries(text: &str, range: TextRange) -> TextRange {
    if range.collapsed() {
        return TextRange::at(floor_boundary(text, range.start));
    }
    let (min, max) = (floor_boundary(text, range.min()), ceil_boundary(text, range.max()));
    if range.reversed() { TextRange::new(max, min) } else { TextRange::new(min, max) }
}

/// A field's value at one time: its text, selection and composition, as Compose's internal
/// `TextFieldCharSequence`. Equal by contents; the text is shared, so a copy copies a pointer.
#[derive(Clone)]
pub(crate) struct TextFieldCharSequence {
    pub(crate) text: Rc<str>,
    pub(crate) selection: TextRange,
    pub(crate) composition: Option<TextRange>,
}

impl TextFieldCharSequence {
    /// The selection and composition kept in the text, at character boundaries, as Compose's
    /// constructor coerces them; a composition coerced to a collapsed range stays one (D2077).
    pub(crate) fn new(
        text: Rc<str>,
        selection: TextRange,
        composition: Option<TextRange>,
    ) -> TextFieldCharSequence {
        let length = text.len();
        let selection = range_at_boundaries(&text, selection.coerce_in(0, length));
        let composition =
            composition.map(|range| range_at_boundaries(&text, range.coerce_in(0, length)));
        TextFieldCharSequence { text, selection, composition }
    }

    /// Whether the text is `other`'s: the same allocation, or the same characters.
    pub(crate) fn content_equals(&self, other: &TextFieldCharSequence) -> bool {
        Rc::ptr_eq(&self.text, &other.text) || self.text == other.text
    }
}

impl PartialEq for TextFieldCharSequence {
    fn eq(&self, other: &TextFieldCharSequence) -> bool {
        self.selection == other.selection
            && self.composition == other.composition
            && self.content_equals(other)
    }
}

impl fmt::Debug for TextFieldCharSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "TextFieldCharSequence(text={:?}, selection={:?}, composition={:?})",
            &*self.text, self.selection, self.composition
        )
    }
}

/// One change of a [`ChangeList`]: where it is now (`pre`, in the text with every change made)
/// and what it replaced (`original`).
#[derive(Clone, Copy, Debug)]
struct Change {
    pre_start: isize,
    pre_end: isize,
    original_start: isize,
    original_end: isize,
    from_hardware: bool,
}

/// The changes made to a [`TextFieldBuffer`] in one edit or transformation, as Compose's
/// `TextFieldBuffer.ChangeList` (its `ChangeTracker`): in the order they are in the text, not the
/// order they were made; changes that touch or overlap are one. Each has its range in the text now
/// and the range of the original text it replaced.
#[derive(Clone, Default)]
pub struct ChangeList {
    changes: Vec<Change>,
    /// The next list, kept to be reused.
    scratch: Vec<Change>,
}

impl ChangeList {
    /// A copy of `other`'s changes, as `ChangeTracker(initialChanges)`.
    pub(crate) fn copy_of(other: &ChangeList) -> ChangeList {
        ChangeList { changes: other.changes.clone(), scratch: Vec::new() }
    }

    /// The number of changes.
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }

    /// Change `change_index`'s range in the text now. Panics if there's no such change, as
    /// Compose's throws.
    pub fn get_range(&self, change_index: usize) -> TextRange {
        let change = self.change(change_index);
        TextRange::new(change.pre_start as usize, change.pre_end as usize)
    }

    /// The range of the original text change `change_index` replaced. Panics if there's no such
    /// change.
    pub fn get_original_range(&self, change_index: usize) -> TextRange {
        let change = self.change(change_index);
        TextRange::new(change.original_start as usize, change.original_end as usize)
    }

    fn change(&self, change_index: usize) -> &Change {
        self.changes.get(change_index).unwrap_or_else(|| {
            panic!("change {change_index} out of bounds: {} changes", self.changes.len())
        })
    }

    /// Calls `block` with each change's range now and its original range, from the start of the
    /// text, as Compose's `forEachChange`. To edit the buffer meanwhile, use
    /// [`TextFieldBuffer::for_each_change`].
    pub fn for_each_change(&self, mut block: impl FnMut(TextRange, TextRange)) {
        for index in 0..self.change_count() {
            block(self.get_range(index), self.get_original_range(index));
        }
    }

    /// As [`for_each_change`](Self::for_each_change), from the end of the text, as Compose's
    /// `forEachChangeReversed`.
    pub fn for_each_change_reversed(&self, mut block: impl FnMut(TextRange, TextRange)) {
        for index in (0..self.change_count()).rev() {
            block(self.get_range(index), self.get_original_range(index));
        }
    }

    /// Whether change `change_index` came from a hardware keyboard.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_from_hardware_source(&self, change_index: usize) -> bool {
        self.change(change_index).from_hardware
    }

    pub(crate) fn clear_changes(&mut self) {
        self.changes.clear();
    }

    /// Records that `pre_start..pre_end` of the text as it is (every change before made) was
    /// replaced by `post_length` bytes, as Compose's `trackChange`: the changes it touches or
    /// overlaps become one with it, and those after it move by its difference in length.
    pub(crate) fn track_change(
        &mut self,
        pre_start: usize,
        pre_end: usize,
        post_length: usize,
        from_hardware: bool,
    ) {
        if pre_start == pre_end && post_length == 0 {
            // Nothing changed.
            return;
        }
        let pre_min = pre_start.min(pre_end) as isize;
        let pre_max = pre_start.max(pre_end) as isize;
        let post_delta = post_length as isize - (pre_max - pre_min);
        let mut recorded = false;
        let mut merged: Option<Change> = None;
        let changes = std::mem::take(&mut self.changes);
        for mut change in changes.iter().copied() {
            let within = |offset: isize, start: isize, end: isize| start <= offset && offset <= end;
            if within(change.pre_start, pre_min, pre_max)
                || within(change.pre_end, pre_min, pre_max)
                || within(pre_min, change.pre_start, change.pre_end)
                || within(pre_max, change.pre_start, change.pre_end)
            {
                match &mut merged {
                    None => merged = Some(change),
                    Some(merged) => {
                        merged.pre_end = change.pre_end;
                        merged.original_end = change.original_end;
                        merged.from_hardware |= change.from_hardware;
                    }
                }
                continue;
            }
            if change.pre_start > pre_max && !recorded {
                self.push_new_change(merged.take(), pre_min, pre_max, post_delta, from_hardware);
                recorded = true;
            }
            if recorded {
                change.pre_start += post_delta;
                change.pre_end += post_delta;
            }
            self.scratch.push(change);
        }
        if !recorded {
            self.push_new_change(merged, pre_min, pre_max, post_delta, from_hardware);
        }
        self.changes = std::mem::replace(&mut self.scratch, changes);
        self.scratch.clear();
    }

    /// Adds the change being tracked to the next list, merged with those it overlapped, as
    /// Compose's `appendNewChange`.
    fn push_new_change(
        &mut self,
        merged: Option<Change>,
        pre_min: isize,
        pre_max: isize,
        post_delta: isize,
        from_hardware: bool,
    ) {
        let mut original_delta =
            self.scratch.last().map_or(0, |change| change.pre_end - change.original_end);
        let change = match merged {
            None => {
                let original_start = pre_min - original_delta;
                Change {
                    pre_start: pre_min,
                    pre_end: pre_max + post_delta,
                    original_start,
                    original_end: original_start + (pre_max - pre_min),
                    from_hardware,
                }
            }
            Some(mut change) => {
                change.from_hardware |= from_hardware;
                if change.pre_start > pre_min {
                    // It starts before the changes it overlaps.
                    change.pre_start = pre_min;
                    change.original_start = pre_min;
                }
                if pre_max > change.pre_end {
                    // It ends after them.
                    original_delta = change.pre_end - change.original_end;
                    change.pre_end = pre_max;
                    change.original_end = pre_max - original_delta;
                }
                change.pre_end += post_delta;
                change
            }
        };
        self.scratch.push(change);
    }
}

/// `ChangeList(changes=[(0,5)->(0,7), …])`, as Compose's: each original range, then its range now.
impl fmt::Display for ChangeList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChangeList(changes=[")?;
        for (index, change) in self.changes.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(
                formatter,
                "({},{})->({},{})",
                change.original_start, change.original_end, change.pre_start, change.pre_end
            )?;
        }
        formatter.write_str("])")
    }
}

impl fmt::Debug for ChangeList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

/// Where `original` is after `replace_start..replace_end` was replaced by `inserted_length`
/// bytes, as Compose's `adjustTextRange`: before the change it stays, after it it moves by the
/// difference, around it its end moves, inside it it collapses to the change's end, and an end
/// inside the change goes to the change's edge.
pub(crate) fn adjust_text_range(
    original: TextRange,
    replace_start: usize,
    replace_end: usize,
    inserted_length: usize,
) -> TextRange {
    let (mut start, mut end) = (original.min() as isize, original.max() as isize);
    let (replace_start, replace_end) = (replace_start as isize, replace_end as isize);
    let inserted = inserted_length as isize;
    if end < replace_start {
        return original;
    }
    let difference = inserted - (replace_end - replace_start);
    if start <= replace_start && replace_end <= end {
        // The change is inside it: its end moves, and a cursor stays one.
        if start == end {
            start += difference;
        }
        end += difference;
    } else if start > replace_start && end < replace_end {
        // It's inside the change.
        start = replace_start + inserted;
        end = replace_start + inserted;
    } else if start >= replace_end {
        // It's after the change.
        start += difference;
        end += difference;
    } else if replace_start < start {
        // The change covers its start.
        start = replace_start + inserted;
        end += difference;
    } else {
        // The change covers its end.
        end = replace_start;
    }
    TextRange::new(start as usize, end as usize)
}

/// The parts of `a` and `b` that differ, past their common prefix and suffix, as
/// `(a_start, a_end, b_start, b_end)`, as Compose's `findCommonPrefixAndSuffix`; `None` if they're
/// equal. Offsets are at character boundaries of each. As Compose's, the prefix and the suffix
/// grow by a character each in turn; the suffix only while something is left between them, where
/// Compose's can count a character in both and find "a" and "aa" equal (D1129).
pub(crate) fn find_common_prefix_and_suffix(
    a: &str,
    b: &str,
) -> Option<(usize, usize, usize, usize)> {
    let (mut a_start, mut a_end, mut b_start, mut b_end) = (0, a.len(), 0, b.len());
    let (mut prefix_found, mut suffix_found) = (false, false);
    while a_start < a_end && b_start < b_end && !(prefix_found && suffix_found) {
        if !prefix_found {
            match (a[a_start..a_end].chars().next(), b[b_start..b_end].chars().next()) {
                (Some(x), Some(y)) if x == y => {
                    a_start += x.len_utf8();
                    b_start += y.len_utf8();
                }
                _ => prefix_found = true,
            }
        }
        if !suffix_found && a_start < a_end && b_start < b_end {
            match (a[a_start..a_end].chars().next_back(), b[b_start..b_end].chars().next_back()) {
                (Some(x), Some(y)) if x == y => {
                    a_end -= x.len_utf8();
                    b_end -= y.len_utf8();
                }
                _ => suffix_found = true,
            }
        }
    }
    if a_start >= a_end && b_start >= b_end {
        return None;
    }
    Some((a_start, a_end, b_start, b_end))
}

/// A text being edited, with its selection, as Compose's `TextFieldBuffer`: what
/// [`TextFieldState::edit`](crate::TextFieldState::edit) gives its block. Every change of the text
/// is in [`changes`](Self::changes); the selection moves with the edits, as Compose's.
///
/// Offsets are UTF-8 byte offsets (D1100). Compose's negative indices have no counterpart.
pub struct TextFieldBuffer {
    text: String,
    selection: TextRange,
    /// What an input method is composing: `None` for a collapsed range.
    composition: Option<TextRange>,
    original: TextFieldCharSequence,
    changes: ChangeList,
    /// The mapping an output transformation's edits make, while one runs (Compose's
    /// `offsetMappingCalculator`).
    offset_mapping: Option<crate::output_transformation::OffsetMappingCalculator>,
}

impl TextFieldBuffer {
    /// A buffer of `initial`, which is also its original.
    pub(crate) fn new(initial: TextFieldCharSequence) -> TextFieldBuffer {
        TextFieldBuffer::with_original(initial.clone(), initial, None)
    }

    /// A buffer of `initial` made from `original` by `changes`, as Compose's constructor with
    /// `originalValue` and `initialChanges`.
    pub(crate) fn with_original(
        initial: TextFieldCharSequence,
        original: TextFieldCharSequence,
        changes: Option<&ChangeList>,
    ) -> TextFieldBuffer {
        TextFieldBuffer {
            text: initial.text.to_string(),
            selection: initial.selection,
            composition: initial.composition,
            original,
            changes: changes.map(ChangeList::copy_of).unwrap_or_default(),
            offset_mapping: None,
        }
    }

    /// Keeps the mapping the edits from now on make, for an output transformation.
    pub(crate) fn track_offset_mapping(&mut self) {
        self.offset_mapping.get_or_insert_with(Default::default);
    }

    pub(crate) fn take_offset_mapping(
        &mut self,
    ) -> Option<crate::output_transformation::OffsetMappingCalculator> {
        self.offset_mapping.take()
    }

    /// The text's length, in bytes.
    pub fn length(&self) -> usize {
        self.text.len()
    }

    /// The text, as it is now (Compose's `asCharSequence`).
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The text before any change: what [`revert_all_changes`](Self::revert_all_changes) goes
    /// back to.
    pub fn original_text(&self) -> &str {
        &self.original.text
    }

    /// The selection before any change.
    pub fn original_selection(&self) -> TextRange {
        self.original.selection
    }

    /// The changes made so far, as Compose's `changes`.
    pub fn changes(&self) -> &ChangeList {
        &self.changes
    }

    /// The selection; a collapsed one is the cursor.
    pub fn selection(&self) -> TextRange {
        self.selection
    }

    /// Whether the selection isn't collapsed, as `hasSelection`.
    pub fn has_selection(&self) -> bool {
        !self.selection.collapsed()
    }

    /// Selects `selection`, as Compose's `selection` setter: an end inside a character widens the
    /// selection over it (a cursor goes to its character's start). Panics if it isn't within the
    /// text, as Compose's throws.
    pub fn set_selection(&mut self, selection: TextRange) {
        let valid = TextRange::new(0, self.length());
        assert!(valid.contains_range(selection), "Expected {selection} to be in {valid}");
        self.selection = range_at_boundaries(&self.text, selection);
    }

    /// Selects `start..end` kept within the text, as Compose's `setSelectionCoerced`, for what an
    /// input method or the field gives.
    pub(crate) fn set_selection_coerced(&mut self, selection: TextRange) {
        self.set_selection(selection.coerce_in(0, self.length()));
    }

    /// What's being composed.
    pub(crate) fn composition(&self) -> Option<TextRange> {
        self.composition
    }

    /// Nothing's composed any more, as `commitComposition`.
    pub(crate) fn commit_composition(&mut self) {
        self.composition = None;
    }

    /// Composes `start..end`, as Compose's `setComposition`. Panics if it's outside the text,
    /// reversed or empty, as Compose's throws.
    pub(crate) fn set_composition(&mut self, start: usize, end: usize) {
        let length = self.length();
        assert!(start <= length, "start ({start}) offset is outside of text region {length}");
        assert!(end <= length, "end ({end}) offset is outside of text region {length}");
        assert!(start < end, "Do not set reversed or empty range: {start} > {end}");
        self.composition = Some(range_at_boundaries(&self.text, TextRange::new(start, end)));
    }

    /// Replaces `start..end` with `text` and records the change, as Compose's `replace`: the
    /// range is kept within the text, the selection moves with the edit, and what was composed
    /// is composed no more. Panics if `start > end`.
    pub fn replace(&mut self, start: usize, end: usize, text: &str) {
        self.replace_part(start, end, text, 0, text.len(), false);
    }

    /// Replaces `start..end` with `text[text_start..text_end]`, as Compose's internal `replace`:
    /// both ranges kept within their texts. Panics if `start > end` or `text_start > text_end`.
    pub(crate) fn replace_part(
        &mut self,
        start: usize,
        end: usize,
        text: &str,
        text_start: usize,
        text_end: usize,
        from_hardware: bool,
    ) {
        assert!(start <= end, "Expected start={start} <= end={end}");
        assert!(text_start <= text_end, "Expected textStart={text_start} <= textEnd={text_end}");
        // A collapsed range inside a character is before it (D1112); another widens over it.
        let collapsed = start == end;
        let start = floor_boundary(&self.text, start);
        let end = if collapsed { start } else { ceil_boundary(&self.text, end) };
        let text = &text[floor_boundary(text, text_start)..ceil_boundary(text, text_end)];
        let one_character = text.chars().nth(1).is_none();
        self.text_will_change(start, end, text.len(), one_character, from_hardware);
        self.text.replace_range(start..end, text);
        self.commit_composition();
    }

    /// `replace(0, length, new_text)`, recording a change only for what differs, as Compose's
    /// `setTextIfChanged`.
    pub(crate) fn set_text_if_changed(&mut self, new_text: &str) {
        if let Some((start, end, new_start, new_end)) =
            find_common_prefix_and_suffix(&self.text, new_text)
        {
            self.replace_part(start, end, new_text, new_start, new_end, false);
        }
    }

    /// Adds `text` at the end, as Compose's `append` (its `Appendable`): what's composed stays.
    /// Compose's range form is a slice: `append(&text[start..end])`.
    pub fn append(&mut self, text: &str) {
        let length = self.length();
        self.text_will_change(length, length, text.len(), false, false);
        self.text.push_str(text);
    }

    /// Adds `c` at the end, as Compose's `append(Char)`.
    pub fn append_char(&mut self, c: char) {
        let length = self.length();
        self.text_will_change(length, length, c.len_utf8(), false, false);
        self.text.push(c);
    }

    /// Records a change and moves the selection, before the text changes, as Compose's
    /// `onTextWillChange`: edits from an application or a desktop place the selection somewhere
    /// sensible too.
    fn text_will_change(
        &mut self,
        start: usize,
        end: usize,
        new_length: usize,
        new_is_one_character: bool,
        hardware: bool,
    ) {
        self.changes.track_change(start, end, new_length, hardware);
        // One character for one character as long (a byte offset can't fall inside either):
        // nothing to map,
        // as Compose's skips one unit for one, so a transformation going character by character
        // keeps the mapping short.
        if let Some(mapping) = &mut self.offset_mapping {
            let (min, max) = (start.min(end), start.max(end));
            let one_for_one = max - min == new_length
                && new_is_one_character
                && self
                    .text
                    .get(min..max)
                    .is_some_and(|replaced| replaced.chars().nth(1).is_none());
            if !one_for_one {
                mapping.record_edit_operation(start, end, new_length);
            }
        }
        self.selection = adjust_text_range(self.selection, start, end, new_length);
    }

    /// The character at `index` (the one it's in, inside one), as `charAt`. Panics if `index` is
    /// past the text.
    pub fn char_at(&self, index: usize) -> char {
        assert!(index < self.length(), "index {index} out of bounds for length {}", self.length());
        self.text[floor_boundary(&self.text, index)..].chars().next().expect("a character")
    }

    /// Goes back to the original text and selection, with no change recorded, as
    /// `revertAllChanges`.
    pub fn revert_all_changes(&mut self) {
        let original = self.original.text.clone();
        self.replace(0, self.length(), &original);
        self.selection = self.original.selection;
        self.changes.clear_changes();
    }

    /// Puts the cursor before the character at `index` (`length()`: at the end), as
    /// `placeCursorBeforeCharAt`; inside a character, before it. Panics past the end.
    pub fn place_cursor_before_char_at(&mut self, index: usize) {
        let end = self.length() + 1;
        assert!(index < end, "Expected {index} to be in [0, {end})");
        self.selection = TextRange::at(floor_boundary(&self.text, index));
    }

    /// Puts the cursor after the character at `index`, as `placeCursorAfterCharAt`. Panics if
    /// there's no character there.
    pub fn place_cursor_after_char_at(&mut self, index: usize) {
        let end = self.length();
        assert!(index < end, "Expected {index} to be in [-1, {end})");
        let after = floor_boundary(&self.text, index) + self.char_at(index).len_utf8();
        self.selection = TextRange::at(after.min(end));
    }

    /// The buffer's value, with its selection and composition.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn to_text_field_char_sequence(&self) -> TextFieldCharSequence {
        TextFieldCharSequence::new(Rc::from(self.text.as_str()), self.selection, self.composition)
    }

    /// Calls `block` with the buffer and each change's range now and its original range, from the
    /// start of the text, as Compose's `forEachChange`: `block` may edit the buffer, the count read
    /// again at each turn (an edit before the change being visited visits it again).
    pub fn for_each_change(
        &mut self,
        mut block: impl FnMut(&mut TextFieldBuffer, TextRange, TextRange),
    ) {
        let mut index = 0;
        while index < self.changes.change_count() {
            let (range, original) =
                (self.changes.get_range(index), self.changes.get_original_range(index));
            block(self, range, original);
            index += 1;
        }
    }

    /// As [`for_each_change`](Self::for_each_change), from the end of the text, as Compose's
    /// `forEachChangeReversed`: edits after the change being visited aren't visited.
    pub fn for_each_change_reversed(
        &mut self,
        mut block: impl FnMut(&mut TextFieldBuffer, TextRange, TextRange),
    ) {
        let mut index = self.changes.change_count();
        while index > 0 {
            index -= 1;
            if index >= self.changes.change_count() {
                continue;
            }
            let (range, original) =
                (self.changes.get_range(index), self.changes.get_original_range(index));
            block(self, range, original);
        }
    }

    /// Inserts `text` at `index` (`length()`: at the end), as Compose's `insert`.
    pub fn insert(&mut self, index: usize, text: &str) {
        self.replace(index, index, text);
    }

    /// Deletes `start..end`, as Compose's `delete`.
    pub fn delete(&mut self, start: usize, end: usize) {
        self.replace(start, end, "");
    }

    /// Puts the cursor at the end, as `placeCursorAtEnd`.
    pub fn place_cursor_at_end(&mut self) {
        self.place_cursor_before_char_at(self.length());
    }

    /// Selects the whole text, as `selectAll`.
    pub fn select_all(&mut self) {
        self.set_selection(TextRange::new(0, self.length()));
    }
}

/// The text, as Compose's `toString`.
impl fmt::Display for TextFieldBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

impl fmt::Debug for TextFieldBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "TextFieldBuffer(text={:?}, selection={:?}, changes={})",
            self.text, self.selection, self.changes
        )
    }
}

/// `write!` appends, as Compose's `Appendable`.
impl fmt::Write for TextFieldBuffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.append(text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The differing middles of two texts, if any.
    type Middles = Option<(usize, usize, usize, usize)>;

    /// A text and its changes, as Compose's tests' `SimpleBuffer`.
    struct TrackedText {
        text: String,
        changes: ChangeList,
    }

    impl TrackedText {
        fn new(text: &str) -> TrackedText {
            TrackedText { text: text.to_string(), changes: ChangeList::default() }
        }

        fn append(&mut self, text: &str) {
            let length = self.text.len();
            self.changes.track_change(length, length, text.len(), false);
            self.text.push_str(text);
        }

        /// Replaces the first `found` with `text`.
        fn replace_found(&mut self, found: &str, text: &str) {
            if let Some(start) = self.text.find(found) {
                let end = start + found.len();
                self.changes.track_change(start, end, text.len(), false);
                self.text.replace_range(start..end, text);
            }
        }

        fn replace(&mut self, start: usize, end: usize, text: &str) {
            self.changes.track_change(start, end, text.len(), false);
            self.text.replace_range(start.min(end)..start.max(end), text);
        }

        /// Its only change, as (range now, original range).
        fn single_change(&self) -> (TextRange, TextRange) {
            assert_eq!(self.changes.change_count(), 1, "{}", self.changes);
            (self.changes.get_range(0), self.changes.get_original_range(0))
        }
    }

    fn range(start: usize, end: usize) -> TextRange {
        TextRange::new(start, end)
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, initialInsert
    #[test]
    fn a_first_insertion_is_one_change_from_nothing() {
        let mut text = TrackedText::new("");
        text.append("hello");
        assert_eq!(text.text, "hello");
        assert_eq!(text.single_change(), (range(0, 5), TextRange::at(0)));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, deleteAll
    #[test]
    fn deleting_everything_is_one_change_to_nothing() {
        let mut text = TrackedText::new("hello");
        text.replace_found("hello", "");
        assert_eq!(text.single_change(), (TextRange::at(0), range(0, 5)));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleDiscontinuousChanges
    #[test]
    fn changes_apart_stay_apart_in_text_order() {
        let mut text = TrackedText::new("hello world");
        text.replace_found("world", "Compose");
        text.replace_found("hello", "goodbye");
        assert_eq!(text.text, "goodbye Compose");
        assert_eq!(text.changes.change_count(), 2);
        assert_eq!(text.changes.get_range(0), range(0, 7));
        assert_eq!(text.changes.get_original_range(0), range(0, 5));
        assert_eq!(text.changes.get_range(1), range(8, 15));
        assert_eq!(text.changes.get_original_range(1), range(6, 11));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, twoAppends
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, threeAppends
    #[test]
    fn appends_one_after_another_are_one_change() {
        let mut text = TrackedText::new("");
        text.append("foo");
        text.append("bar");
        assert_eq!(text.single_change(), (range(0, 6), TextRange::at(0)));
        text.append("baz");
        assert_eq!(text.text, "foobarbaz");
        assert_eq!(text.single_change(), (range(0, 9), TextRange::at(0)));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, replaceWithReversedIndices
    #[test]
    fn a_change_given_backwards_is_tracked_forwards() {
        let mut text = TrackedText::new("abcd");
        text.replace(2, 0, "e");
        assert_eq!(text.text, "ecd");
        assert_eq!(text.single_change(), (range(0, 1), range(0, 2)));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleAdjacentReplaces_whenPerformedInOrder_replacementsShorter
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleAdjacentReplaces_whenPerformedInOrder_replacementsLonger
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleAdjacentReplaces_whenPerformedInReverseOrder_replacementsShorter
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleAdjacentReplaces_whenPerformedInReverseOrder_replacementsLonger
    #[test]
    fn touching_replacements_in_either_order_are_one_change() {
        let cases = [
            (("ab", "e"), ("cd", "f"), "ef", range(0, 2)),
            (("ab", "efg"), ("cd", "hij"), "efghij", range(0, 6)),
            (("cd", "f"), ("ab", "e"), "ef", range(0, 2)),
            (("cd", "efg"), ("ab", "hij"), "hijefg", range(0, 6)),
        ];
        for ((found, with), (then_found, then_with), expected, now) in cases {
            let mut text = TrackedText::new("abcd");
            text.replace_found(found, with);
            text.replace_found(then_found, then_with);
            assert_eq!(text.text, expected);
            assert_eq!(text.single_change(), (now, range(0, 4)), "{expected}");
        }
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multiplePartiallyOverlappingChanges_atStart
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multiplePartiallyOverlappingChanges_atEnd
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleFullyOverlappingChanges
    #[test]
    fn overlapping_changes_merge() {
        let cases = [
            ("ae", "gh", "ghfd", range(0, 3), range(0, 3)),
            ("fd", "gh", "aegh", range(1, 4), range(1, 4)),
            ("ef", "gh", "aghd", range(1, 3), range(1, 3)),
        ];
        for (found, with, expected, now, original) in cases {
            let mut text = TrackedText::new("abcd");
            text.replace_found("bc", "ef");
            text.replace_found(found, with);
            assert_eq!(text.text, expected);
            assert_eq!(text.single_change(), (now, original), "{expected}");
        }
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleFullyNestedChanges
    #[test]
    fn changes_inside_an_insertion_stay_that_insertion() {
        let mut text = TrackedText::new("");
        text.replace(0, 0, "abcdefg");
        assert_eq!(text.single_change(), (range(0, 7), TextRange::at(0)));
        text.replace(0, 4, "hijk");
        text.replace(3, 6, "abc");
        assert_eq!(text.text, "hijabcg");
        assert_eq!(text.single_change(), (range(0, 7), TextRange::at(0)));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleFullyNestedDeletes
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, multipleFullyNestedDeletes_inReverse
    #[test]
    fn deletions_inside_an_insertion_shrink_it() {
        for (deletions, expected) in
            [([(0, 1), (1, 2), (2, 3)], "bdfg"), ([(6, 7), (4, 5), (2, 3)], "abdf")]
        {
            let mut text = TrackedText::new("");
            text.replace(0, 0, "abcdefg");
            for (start, end) in deletions {
                text.replace(start, end, "");
            }
            assert_eq!(text.text, expected);
            assert_eq!(text.single_change(), (range(0, 4), TextRange::at(0)), "{expected}");
        }
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, trackChange_hardwareKeyboard_preserved
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, copyConstructor_retainsFlags
    #[test]
    fn a_hardware_change_says_so_in_a_copy_too() {
        let mut changes = ChangeList::default();
        changes.track_change(0, 0, 1, true);
        assert!(changes.is_from_hardware_source(0));
        let copy = ChangeList::copy_of(&changes);
        assert_eq!(copy.change_count(), 1);
        assert!(copy.is_from_hardware_source(0));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, mergeChanges_softThenHardware_prefersHardware
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, mergeChanges_hardwareThenSoft_prefersHardware
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, mergeChanges_softThenSoft_staysSoft
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, mergeChanges_hardwareThenHardware_staysHardware
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, mergeChanges_adjacent_prefersHardware
    #[test]
    fn merged_changes_are_hardware_if_any_was() {
        for (first, second, start, expected) in [
            (false, true, 0, true),
            (true, false, 0, true),
            (false, false, 0, false),
            (true, true, 0, true),
            (false, true, 1, true),
        ] {
            let mut changes = ChangeList::default();
            changes.track_change(0, 0, 1, first);
            changes.track_change(start, 1, 1, second);
            assert_eq!(changes.change_count(), 1);
            assert_eq!(changes.is_from_hardware_source(0), expected, "{first} then {second}");
        }
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, mergeChanges_multipleOverlapping_prefersHardware
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, trackChange_nonOverlapping_retainsIndividualFlags
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/internal/ChangeTrackerTest.kt, mergeChanges_bridgingGap_prefersHardware
    #[test]
    fn changes_apart_keep_their_sources_until_joined() {
        let mut changes = ChangeList::default();
        changes.track_change(0, 0, 1, false);
        changes.track_change(2, 2, 1, true);
        assert_eq!(changes.change_count(), 2);
        assert!(!changes.is_from_hardware_source(0));
        assert!(changes.is_from_hardware_source(1));
        let mut covering = changes.clone();
        covering.track_change(0, 3, 1, false);
        assert_eq!(covering.change_count(), 1);
        assert!(covering.is_from_hardware_source(0));
        changes.track_change(1, 2, 1, false);
        assert_eq!(changes.change_count(), 1);
        assert!(changes.is_from_hardware_source(0));
    }

    fn sequence(text: &str, selection: TextRange) -> TextFieldCharSequence {
        TextFieldCharSequence::new(Rc::from(text), selection, None)
    }

    fn buffer(text: &str) -> TextFieldBuffer {
        TextFieldBuffer::new(sequence(text, TextRange::ZERO))
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, resetTo_copiesTextAndSelection
    #[test]
    fn reverting_goes_back_to_the_original_and_forgets_the_changes() {
        let original = sequence("world", TextRange::at(5));
        let mut buffer = TextFieldBuffer::with_original(
            sequence("hello", TextRange::at(2)),
            original.clone(),
            None,
        );
        buffer.revert_all_changes();
        assert_eq!(buffer.to_text_field_char_sequence(), original);
        assert_eq!(buffer.changes().change_count(), 0);
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenChanged
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenPrefixChanged
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenPrefixAdded
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenPrefixRemoved
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenSuffixChanged
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenSuffixAdded
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenSuffixRemoved
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenMiddleChanged_once
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenMiddleAdded_once
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenMiddleRemoved_once
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenMiddleChanged_multiple
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenMiddleAdded_multiple
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_updatesText_whenMiddleRemoved_multiple
    #[test]
    fn setting_a_text_records_only_what_differs() {
        let cases = [
            ("hello", "world", range(0, 5), range(0, 5)),
            ("hello", "1ello", range(0, 1), range(0, 1)),
            ("hello", "1hello", TextRange::at(0), range(0, 1)),
            ("hello", "ello", range(0, 1), TextRange::at(0)),
            ("hello", "hell1", range(4, 5), range(4, 5)),
            ("hello", "hello1", TextRange::at(5), range(5, 6)),
            ("hello", "hell", range(4, 5), TextRange::at(4)),
            ("hello", "h1llo", range(1, 2), range(1, 2)),
            ("hello", "he1llo", TextRange::at(2), range(2, 3)),
            ("hello", "helo", range(2, 3), TextRange::at(2)),
            ("hello", "h1l2o", range(1, 4), range(1, 4)),
            ("hello", "he1ll2o", range(2, 4), range(2, 6)),
            ("abcde", "ace", range(1, 4), range(1, 2)),
        ];
        for (text, new, original, now) in cases {
            let mut buffer = buffer(text);
            buffer.set_text_if_changed(new);
            assert_eq!(buffer.as_str(), new);
            assert_eq!(buffer.changes().change_count(), 1, "{text} to {new}");
            assert_eq!(buffer.changes().get_original_range(0), original, "{text} to {new}");
            assert_eq!(buffer.changes().get_range(0), now, "{text} to {new}");
        }
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_doesNotUpdateTextIfEqual
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, setTextIfChanged_doesNotUpdateTextIfEqual_afterChange
    #[test]
    fn setting_the_same_text_records_nothing_more() {
        let mut buffer = buffer("hello");
        buffer.set_text_if_changed("hello");
        assert_eq!(buffer.changes().change_count(), 0);
        buffer.append(" world");
        buffer.set_text_if_changed("hello world");
        assert_eq!(buffer.changes().change_count(), 1);
    }

    /// Compose's walk finds "a" and "aa" equal (D1129); this one doesn't.
    #[test]
    fn setting_a_text_of_repeated_characters_records_it() {
        let mut buffer = buffer("a");
        buffer.set_text_if_changed("aa");
        assert_eq!(buffer.as_str(), "aa");
        assert_eq!(buffer.changes().change_count(), 1);
        assert_eq!(find_common_prefix_and_suffix("aa", "aaa"), Some((1, 1, 1, 2)));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, findCommonPrefixAndSuffix_works
    #[test]
    fn the_differing_middles_of_two_texts() {
        let cases: [(&str, &str, Middles); 16] = [
            ("", "", None),
            ("a", "a", None),
            ("abc", "abc", None),
            ("", "b", Some((0, 0, 0, 1))),
            ("a", "", Some((0, 1, 0, 0))),
            ("ab", "ac", Some((1, 2, 1, 2))),
            ("abb", "ac", Some((1, 3, 1, 2))),
            ("ab", "acc", Some((1, 2, 1, 3))),
            ("az", "bz", Some((0, 1, 0, 1))),
            ("cba", "za", Some((0, 2, 0, 1))),
            ("za", "cba", Some((0, 1, 0, 2))),
            ("aoz", "apz", Some((1, 2, 1, 2))),
            ("amnoz", "az", Some((1, 4, 1, 1))),
            ("az", "amnoz", Some((1, 1, 1, 4))),
            ("amnoz", "axz", Some((1, 4, 1, 2))),
            ("axz", "amnoz", Some((1, 2, 1, 4))),
        ];
        for (a, b, expected) in cases {
            assert_eq!(find_common_prefix_and_suffix(a, b), expected, "{a:?} and {b:?}");
        }
        // Characters, not bytes: é and è share their first byte.
        assert_eq!(find_common_prefix_and_suffix("é", "è"), Some((0, 2, 0, 2)));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_crossedOffsets
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_throws_whenTextStartGreaterThanTextEnd
    #[test]
    #[should_panic(expected = "Expected textStart=2 <= textEnd=0")]
    fn a_part_given_backwards_panics() {
        buffer("").replace_part(0, 0, "hi", 2, 0, false);
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_endTooBig
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_empty
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_singleCharFromStart
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_singleCharFromEnd
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_middle
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_full
    #[test]
    fn a_part_of_a_text_replaces() {
        for (text, start, end, expected) in [
            ("hi", 2, 3, ""),
            ("hi", 0, 0, ""),
            ("hi", 0, 1, "h"),
            ("hi", 1, 2, "i"),
            ("abcd", 1, 3, "bc"),
            ("abcd", 0, 4, "abcd"),
        ] {
            let mut buffer = buffer("");
            buffer.replace_part(0, 0, text, start, end, false);
            assert_eq!(buffer.as_str(), expected);
        }
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, replace_withSubSequence_indicesCoerced
    #[test]
    fn a_part_past_the_ends_is_kept_within_them() {
        let mut first = buffer("ABCD");
        first.replace_part(0, 10, "XYZ", 0, 10, false);
        assert_eq!(first.as_str(), "XYZ");
        assert_eq!(first.changes().get_original_range(0), range(0, 4));
        assert_eq!(first.changes().get_range(0), range(0, 3));
        let mut second = buffer("ABCD");
        second.replace_part(1, 3, "XYZ", 0, 2, false);
        assert_eq!(second.as_str(), "AXYD");
        assert_eq!(second.changes().get_original_range(0), range(1, 3));
        assert_eq!(second.changes().get_range(0), range(1, 3));
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, textFieldBuffer_replace_defaultsToSoftwareSource
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, textFieldBuffer_replace_canSetHardwareSource
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, textFieldBuffer_append_defaultsToSoftwareSource
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, textFieldBuffer_append_subSequence_defaultsToSoftwareSource
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, textFieldBuffer_append_char_defaultsToSoftwareSource
    #[test]
    fn an_edit_is_from_software_unless_said() {
        let edits: [fn(&mut TextFieldBuffer); 4] = [
            |buffer| buffer.replace(0, 5, "world"),
            |buffer| buffer.append("world"),
            |buffer| buffer.append(&"world"[0..3]),
            |buffer| buffer.append_char('!'),
        ];
        for edit in edits {
            let mut buffer = buffer("hello");
            edit(&mut buffer);
            assert_eq!(buffer.changes().change_count(), 1);
            assert!(!buffer.changes().is_from_hardware_source(0));
        }
        let mut hardware = buffer("hello");
        hardware.replace_part(0, 5, "world", 0, 5, true);
        assert!(hardware.changes().is_from_hardware_source(0));
    }

    /// Offsets inside a character move to its edges: a range widens, a cursor goes before it.
    #[test]
    fn offsets_inside_a_character_move_to_its_edges() {
        let mut buffer = buffer("héllo");
        buffer.set_selection(range(2, 2));
        assert_eq!(buffer.selection(), TextRange::at(1));
        buffer.set_selection(range(2, 4));
        assert_eq!(buffer.selection(), range(1, 4));
        assert_eq!(buffer.char_at(2), 'é');
        buffer.place_cursor_after_char_at(2);
        assert_eq!(buffer.selection(), TextRange::at(3));
        buffer.replace(2, 2, "x");
        assert_eq!(buffer.as_str(), "hxéllo");
        buffer.replace(3, 4, "e");
        assert_eq!(buffer.as_str(), "hxello");
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldBufferTest.kt, initialSelection
    #[test]
    fn an_empty_buffer_has_a_cursor_at_zero() {
        let buffer = buffer("");
        assert_eq!(buffer.selection(), TextRange::at(0));
        assert!(!buffer.has_selection());
    }

    // Compose's `TextFieldCharSequenceTest`. n/a: `throws_exception_for_negative_selection` and
    // `throws_exception_for_negative_composition` (offsets are `usize`, D1128); its highlight
    // (`TextHighlightType`, the handwriting previews) isn't in compose-rs, so the equality tests
    // compare the text, selection and composition.

    /// The test's `TextEditState`: `text` composing `composition`, the cursor at 0.
    fn text_edit_state(text: &str, composition: TextRange) -> TextFieldCharSequence {
        TextFieldCharSequence::new(Rc::from(text), TextRange::ZERO, Some(composition))
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, aligns_selection_to_the_text_length
    #[test]
    fn aligns_selection_to_the_text_length() {
        let text = "a";
        let value = sequence(text, TextRange::at(text.len() + 1));
        assert!(value.selection.collapsed());
        assert_eq!(value.selection.max(), value.text.len());
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, keep_selection_that_is_less_than_text_length
    #[test]
    fn keep_selection_that_is_less_than_text_length() {
        let text = "a bc";
        let selection = TextRange::new(0, "a".len());
        let value = sequence(text, selection);
        assert_eq!(&*value.text, text);
        assert_eq!(value.selection, selection);
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, aligns_composition_to_text_length
    #[test]
    fn aligns_composition_to_text_length() {
        let text = "a";
        let value = text_edit_state(text, TextRange::at(text.len() + 1));
        let composition = value.composition.expect("a composition, collapsed");
        assert!(composition.collapsed());
        assert_eq!(composition.max(), value.text.len());
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, keep_composition_that_is_less_than_text_length
    #[test]
    fn keep_composition_that_is_less_than_text_length() {
        let text = "a bc";
        let composition = TextRange::new(0, "a".len());
        let value = text_edit_state(text, composition);
        assert_eq!(&*value.text, text);
        assert_eq!(value.composition, Some(composition));
    }

    fn with_composition() -> TextFieldCharSequence {
        TextFieldCharSequence::new(Rc::from("a"), TextRange::at(1), Some(TextRange::at(2)))
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, equals_returns_true_for_same_instance
    #[test]
    fn equals_returns_true_for_same_instance() {
        let value = with_composition();
        assert_eq!(value, value);
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, equals_returns_true_for_equivalent_object
    #[test]
    fn equals_returns_true_for_equivalent_object() {
        let value = with_composition();
        let copy = TextFieldCharSequence::new(
            Rc::from(value.text.to_string()),
            value.selection,
            value.composition,
        );
        assert_eq!(copy, value);
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, text_and_selection_parameter_constructor_has_null_composition_and_highlight
    #[test]
    fn text_and_selection_parameter_constructor_has_null_composition_and_highlight() {
        assert_eq!(sequence("a", TextRange::at(1)).composition, None);
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, contentEquals_textFieldCharSequence_sameTextDifferentSelection_isTrue
    #[test]
    fn content_equals_text_field_char_sequence_same_text_different_selection_is_true() {
        let a = sequence("hello", TextRange::at(0));
        let b = sequence("hello", TextRange::new(2, 4));
        assert!(a.content_equals(&b));
        assert!(b.content_equals(&a));
        assert_ne!(a, b);
    }

    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, contentEquals_textFieldCharSequence_differentText_isFalse
    #[test]
    fn content_equals_text_field_char_sequence_different_text_is_false() {
        let a = sequence("hello", TextRange::ZERO);
        for other in ["hellO", "hell", "hello!"] {
            assert!(!a.content_equals(&sequence(other, TextRange::ZERO)), "{other}");
        }
    }

    /// Compose's `StringBuilder`-backed sequence: here a text in an allocation of its own.
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, contentEquals_textFieldCharSequence_nonStringBackingText
    #[test]
    fn content_equals_text_field_char_sequence_non_string_backing_text() {
        let string_backed = sequence("hello", TextRange::ZERO);
        let built = |text: &str| {
            let mut builder = String::new();
            builder.push_str(text);
            TextFieldCharSequence::new(Rc::from(builder), TextRange::ZERO, None)
        };
        let builder_backed = built("hello");
        assert!(!Rc::ptr_eq(&string_backed.text, &builder_backed.text));
        assert!(string_backed.content_equals(&builder_backed));
        assert!(builder_backed.content_equals(&string_backed));
        assert!(!string_backed.content_equals(&built("world")));
    }

    /// Compose compares with any `CharSequence`: here, with the text's `str`.
    // Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/text/input/TextFieldCharSequenceTest.kt, contentEquals_plainCharSequence
    #[test]
    fn content_equals_plain_char_sequence() {
        let a = sequence("hello", TextRange::ZERO);
        assert_eq!(&*a.text, "hello");
        assert_eq!(&*a.text, String::from("hello").as_str());
        assert_ne!(&*a.text, "world");
        assert_ne!(&*a.text, "");
    }

    #[test]
    fn coerced_composition_keeps_collapsed_ranges_and_utf8_boundaries() {
        // 'a' (0..1), 'é' (1..3), '🙂' (3..7), 'b' (7..8): 8 UTF-8 bytes.
        let text: Rc<str> = Rc::from("aé🙂b");
        assert_eq!(
            TextFieldCharSequence::new(text.clone(), TextRange::ZERO, Some(TextRange::ZERO))
                .composition,
            Some(TextRange::ZERO)
        );
        assert_eq!(
            TextFieldCharSequence::new(text.clone(), TextRange::ZERO, Some(TextRange::new(12, 20)))
                .composition,
            Some(TextRange::at(8))
        );
        // Collapsed inside 'é' (byte 2) or '🙂' (byte 5) floors to the character's start.
        assert_eq!(
            TextFieldCharSequence::new(text.clone(), TextRange::ZERO, Some(TextRange::at(2)))
                .composition,
            Some(TextRange::at(1))
        );
        assert_eq!(
            TextFieldCharSequence::new(text.clone(), TextRange::ZERO, Some(TextRange::at(5)))
                .composition,
            Some(TextRange::at(3))
        );
        // Non-collapsed inside multi-byte characters widens to character boundaries, direction kept.
        assert_eq!(
            TextFieldCharSequence::new(text.clone(), TextRange::ZERO, Some(TextRange::new(2, 5)))
                .composition,
            Some(TextRange::new(1, 7))
        );
        assert_eq!(
            TextFieldCharSequence::new(text.clone(), TextRange::ZERO, Some(TextRange::new(5, 2)))
                .composition,
            Some(TextRange::new(7, 1))
        );
        for start in 0..=10 {
            for end in 0..=10 {
                let value = TextFieldCharSequence::new(
                    text.clone(),
                    TextRange::new(start, end),
                    Some(TextRange::new(end, start)),
                );
                let comp = value.composition.expect("composition stays");
                assert!(value.text.is_char_boundary(value.selection.start));
                assert!(value.text.is_char_boundary(value.selection.end));
                assert!(value.text.is_char_boundary(comp.start));
                assert!(value.text.is_char_boundary(comp.end));
                let _ = comp.substring(&value.text);
            }
        }
    }
}
