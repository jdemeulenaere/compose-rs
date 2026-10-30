//! The pieces of a text a screen reader moves through (`ACTION_NEXT_AT_MOVEMENT_GRANULARITY`), as
//! Compose's `AccessibilityIterators`, which follow `TextView`'s: characters, words, paragraphs,
//! lines and pages, each asked for what follows or precedes a position. Positions are the UTF-16
//! units Android counts in. Characters and words break by Unicode's rules (`unicode-segmentation`)
//! rather than the locale's `BreakIterator`, as this port's text does elsewhere (D1108, D1650).

use compose_ui::{ResolvedTextDirection, TextLayoutResult};
use unicode_segmentation::UnicodeSegmentation;

use crate::text::{byte_offset, utf16_index};

/// `AccessibilityNodeInfo.MOVEMENT_GRANULARITY_*`.
pub mod movement {
    pub const CHARACTER: i32 = 1;
    pub const WORD: i32 = 1 << 1;
    pub const LINE: i32 = 1 << 2;
    pub const PARAGRAPH: i32 = 1 << 3;
    pub const PAGE: i32 = 1 << 4;
}

/// What's moved through. Lines and pages are the text's layout's; a page is as high as `height`.
#[derive(Clone, Copy)]
pub enum Granularity<'a> {
    Character,
    Word,
    Paragraph,
    Line(&'a TextLayoutResult),
    Page(&'a TextLayoutResult, f32),
}

/// A text to move through.
pub struct TextSegments<'a> {
    text: &'a str,
    units: Vec<u16>,
}

const NEW_LINE: u16 = b'\n' as u16;

impl<'a> TextSegments<'a> {
    pub fn new(text: &'a str) -> TextSegments<'a> {
        TextSegments { text, units: text.encode_utf16().collect() }
    }

    /// The text's length in UTF-16 units.
    pub fn len(&self) -> i32 {
        self.units.len() as i32
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// The next piece at or after `current`: its start and end. `None` at the end.
    pub fn following(&self, granularity: Granularity<'_>, current: i32) -> Option<(i32, i32)> {
        if self.is_empty() || current >= self.len() {
            return None;
        }
        let start = current.max(0);
        let range = match granularity {
            Granularity::Character => self.character_following(start)?,
            Granularity::Word => self.word_following(start)?,
            Granularity::Paragraph => self.paragraph_following(start)?,
            Granularity::Line(layout) => self.line_following(layout, current)?,
            Granularity::Page(layout, height) => self.page_following(layout, height, start),
        };
        non_empty(range)
    }

    /// The piece before `current`: its start and end. `None` at the start.
    pub fn preceding(&self, granularity: Granularity<'_>, current: i32) -> Option<(i32, i32)> {
        if self.is_empty() || current <= 0 {
            return None;
        }
        let end = current.min(self.len());
        let range = match granularity {
            Granularity::Character => self.character_preceding(end)?,
            Granularity::Word => self.word_preceding(end)?,
            Granularity::Paragraph => self.paragraph_preceding(end)?,
            Granularity::Line(layout) => self.line_preceding(layout, current)?,
            Granularity::Page(layout, height) => self.page_preceding(layout, height, end),
        };
        non_empty(range)
    }

    fn unit_of(&self, byte: usize) -> i32 {
        utf16_index(self.text, byte) as i32
    }

    fn byte_of(&self, unit: i32) -> usize {
        byte_offset(self.text, unit.max(0) as usize)
    }

    /// Where `pieces`, all of the text in order, start in UTF-16 units, and its end: one walk, for
    /// a long text's move not to be quadratic.
    fn boundaries_of<'b>(pieces: impl Iterator<Item = &'b str>) -> Vec<i32> {
        let mut units = 0;
        let mut all = vec![0];
        for piece in pieces {
            units += piece.encode_utf16().count() as i32;
            all.push(units);
        }
        all
    }

    fn character_boundaries(&self) -> Vec<i32> {
        Self::boundaries_of(self.text.graphemes(true))
    }

    fn word_boundaries(&self) -> Vec<i32> {
        Self::boundaries_of(self.text.split_word_bounds())
    }

    fn character_following(&self, current: i32) -> Option<(i32, i32)> {
        let all = self.character_boundaries();
        let start = *all.iter().find(|&&at| at >= current)?;
        Some((start, next_boundary(&all, start)?))
    }

    fn character_preceding(&self, current: i32) -> Option<(i32, i32)> {
        let all = self.character_boundaries();
        let end = *all.iter().rev().find(|&&at| at <= current)?;
        Some((previous_boundary(&all, end)?, end))
    }

    /// Whether what's at `unit` is a letter or digit.
    fn is_letter_or_digit(&self, unit: i32) -> bool {
        if unit < 0 || unit >= self.len() {
            return false;
        }
        char::decode_utf16(self.units[unit as usize..].iter().copied())
            .next()
            .is_some_and(|c| c.is_ok_and(char::is_alphanumeric))
    }

    fn starts_word(&self, unit: i32) -> bool {
        self.is_letter_or_digit(unit) && (unit == 0 || !self.is_letter_or_digit(unit - 1))
    }

    fn ends_word(&self, unit: i32) -> bool {
        unit > 0
            && self.is_letter_or_digit(unit - 1)
            && (unit == self.len() || !self.is_letter_or_digit(unit))
    }

    fn word_following(&self, current: i32) -> Option<(i32, i32)> {
        let all = self.word_boundaries();
        let mut start = current;
        while !self.is_letter_or_digit(start) {
            start = next_boundary(&all, start)?;
        }
        // A run of letters or digits is a word, where Unicode breaks inside it (each ideograph)
        // and a locale's dictionary would have found the words in it (D1650).
        let mut end = next_boundary(&all, start)?;
        while !self.ends_word(end) {
            end = next_boundary(&all, end)?;
        }
        Some((start, end))
    }

    fn word_preceding(&self, current: i32) -> Option<(i32, i32)> {
        let all = self.word_boundaries();
        let mut end = current;
        while end > 0 && !self.is_letter_or_digit(end - 1) && !self.ends_word(end) {
            end = previous_boundary(&all, end)?;
        }
        let mut start = previous_boundary(&all, end)?;
        while !self.starts_word(start) {
            start = previous_boundary(&all, start)?;
        }
        Some((start, end))
    }

    fn starts_paragraph(&self, unit: i32) -> bool {
        self.units[unit as usize] != NEW_LINE
            && (unit == 0 || self.units[unit as usize - 1] == NEW_LINE)
    }

    fn ends_paragraph(&self, unit: i32) -> bool {
        unit > 0
            && self.units[unit as usize - 1] != NEW_LINE
            && (unit == self.len() || self.units[unit as usize] == NEW_LINE)
    }

    fn paragraph_following(&self, current: i32) -> Option<(i32, i32)> {
        let mut start = current;
        while start < self.len()
            && self.units[start as usize] == NEW_LINE
            && !self.starts_paragraph(start)
        {
            start += 1;
        }
        if start >= self.len() {
            return None;
        }
        let mut end = start + 1;
        while end < self.len() && !self.ends_paragraph(end) {
            end += 1;
        }
        Some((start, end))
    }

    fn paragraph_preceding(&self, current: i32) -> Option<(i32, i32)> {
        let mut end = current;
        while end > 0 && self.units[end as usize - 1] == NEW_LINE && !self.ends_paragraph(end) {
            end -= 1;
        }
        if end <= 0 {
            return None;
        }
        let mut start = end - 1;
        while start > 0 && !self.starts_paragraph(start) {
            start -= 1;
        }
        Some((start, end))
    }

    /// The first unit of `line`, in the order `TextView`'s movement sees it: from the other end in a
    /// right-to-left paragraph.
    fn line_start_edge(&self, layout: &TextLayoutResult, line: usize) -> i32 {
        let first = layout.get_line_start(line);
        if layout.get_paragraph_direction(first) == ResolvedTextDirection::Rtl {
            self.unit_of(layout.get_line_end(line, false)) - 1
        } else {
            self.unit_of(first)
        }
    }

    /// One past the last unit of `line`, as `line_start_edge`.
    fn line_end_edge(&self, layout: &TextLayoutResult, line: usize) -> i32 {
        let first = layout.get_line_start(line);
        if layout.get_paragraph_direction(first) == ResolvedTextDirection::Rtl {
            self.unit_of(first) + 1
        } else {
            self.unit_of(layout.get_line_end(line, false))
        }
    }

    fn line_following(&self, layout: &TextLayoutResult, current: i32) -> Option<(i32, i32)> {
        let line = if current < 0 {
            layout.get_line_for_offset(0)
        } else {
            let at = layout.get_line_for_offset(self.byte_of(current));
            // From the start of a line, that line; from inside one, the next.
            if self.line_start_edge(layout, at) == current { at } else { at + 1 }
        };
        if line >= layout.line_count as usize {
            return None;
        }
        Some((self.line_start_edge(layout, line), self.line_end_edge(layout, line)))
    }

    fn line_preceding(&self, layout: &TextLayoutResult, current: i32) -> Option<(i32, i32)> {
        let line = if current > self.len() {
            layout.get_line_for_offset(self.text.len())
        } else {
            let at = layout.get_line_for_offset(self.byte_of(current));
            // From the end of a line, that line; from inside one, the one before.
            if self.line_end_edge(layout, at) == current { at } else { at.checked_sub(1)? }
        };
        Some((self.line_start_edge(layout, line), self.line_end_edge(layout, line)))
    }

    fn page_following(&self, layout: &TextLayoutResult, height: f32, start: i32) -> (i32, i32) {
        let line = layout.get_line_for_offset(self.byte_of(start));
        let next_page = layout.get_line_top(line) + height.round();
        let last_line = (layout.line_count as usize).saturating_sub(1);
        let end_line = if next_page < layout.get_line_top(last_line) {
            layout.get_line_for_vertical_position(next_page).saturating_sub(1)
        } else {
            last_line
        };
        (start, self.line_end_edge(layout, end_line))
    }

    fn page_preceding(&self, layout: &TextLayoutResult, height: f32, end: i32) -> (i32, i32) {
        let line = layout.get_line_for_offset(self.byte_of(end));
        let previous_page = layout.get_line_top(line) - height.round();
        let mut start_line = if previous_page > 0.0 {
            layout.get_line_for_vertical_position(previous_page)
        } else {
            0
        };
        // At the end of the text, it's the end of the current line rather than the start of a next.
        if end == self.len() && start_line < line {
            start_line += 1;
        }
        (self.line_start_edge(layout, start_line), end)
    }
}

fn next_boundary(all: &[i32], after: i32) -> Option<i32> {
    all.iter().copied().find(|&at| at > after)
}

fn previous_boundary(all: &[i32], before: i32) -> Option<i32> {
    all.iter().rev().copied().find(|&at| at < before)
}

fn non_empty(range: (i32, i32)) -> Option<(i32, i32)> {
    (range.0 >= 0 && range.1 >= 0 && range.0 != range.1).then_some(range)
}
