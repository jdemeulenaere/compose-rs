//! A laid-out paragraph and its queries, as Compose's `Paragraph` (D1105): what a
//! [`TextLayoutResult`](crate::TextLayoutResult) answers about lines, positions, boxes and words.
//! Each measurer gives its own (`TextMeasurer::paragraph`); [`ParagraphLayout`] is the one made of
//! data, which the fixed measurer and Android's fill and whose queries follow Android's `Layout`
//! (and Compose's code over it) for exact ports.
//!
//! Offsets are UTF-8 bytes at character boundaries (D1100); an offset inside a character counts
//! as that character's start.

use std::fmt;

use crate::path::Path;
use crate::text_attributes::{ResolvedTextDirection, TextOverflow};
use crate::text_range::TextRange;
use crate::unit::{Density, Offset, Rect};

/// A laid-out paragraph's queries, as Compose's `Paragraph`. Lines are indexed from 0; offsets
/// are bytes of [`text`](Paragraph::text). Like Compose's, the offset queries panic on an
/// offset past the text, and the line queries on a line past `line_count`.
pub trait Paragraph {
    /// The text laid out.
    fn text(&self) -> &str;
    /// The width it was laid out at.
    fn width(&self) -> f32;
    /// The lines' height.
    fn height(&self) -> f32;
    /// The narrowest width without breaking inside a word: the widest word.
    fn min_intrinsic_width(&self) -> f32;
    /// The width without soft line breaks: the widest hard line.
    fn max_intrinsic_width(&self) -> f32;
    /// The first line's baseline.
    fn first_baseline(&self) -> f32 {
        if self.line_count() == 0 { 0.0 } else { self.get_line_baseline(0) }
    }
    /// The last line's baseline.
    fn last_baseline(&self) -> f32 {
        match self.line_count() {
            0 => 0.0,
            count => self.get_line_baseline(count - 1),
        }
    }
    /// The text had more lines than its maximum (they were cut or ellipsized).
    fn did_exceed_max_lines(&self) -> bool;
    /// The lines laid out.
    fn line_count(&self) -> usize;
    fn get_line_left(&self, line: usize) -> f32;
    fn get_line_right(&self, line: usize) -> f32;
    fn get_line_top(&self, line: usize) -> f32;
    fn get_line_bottom(&self, line: usize) -> f32;
    fn get_line_baseline(&self, line: usize) -> f32;
    fn get_line_height(&self, line: usize) -> f32 {
        self.get_line_bottom(line) - self.get_line_top(line)
    }
    fn get_line_width(&self, line: usize) -> f32;
    /// The offset the line starts at.
    fn get_line_start(&self, line: usize) -> usize;
    /// The offset after the line: its trailing spaces and line feed included, or, with
    /// `visible_end`, after its last visible character.
    fn get_line_end(&self, line: usize, visible_end: bool) -> usize;
    fn is_line_ellipsized(&self, line: usize) -> bool;
    /// The line `offset` is on (the last line for the text's end).
    fn get_line_for_offset(&self, offset: usize) -> usize;
    /// The line at `vertical`, kept in the lines.
    fn get_line_for_vertical_position(&self, vertical: f32) -> usize;
    /// The x of `offset`: where a character of the paragraph's direction would go
    /// (`use_primary_direction`), or one of the other direction.
    fn get_horizontal_position(&self, offset: usize, use_primary_direction: bool) -> f32;
    /// The direction of the paragraph (the text between line feeds) `offset` is in.
    fn get_paragraph_direction(&self, offset: usize) -> ResolvedTextDirection;
    /// The direction of the bidi run the character at `offset` is in.
    fn get_bidi_run_direction(&self, offset: usize) -> ResolvedTextDirection;
    /// The offset nearest to `position`, on the line at its height (the first above the
    /// lines, the last below them).
    fn get_offset_for_position(&self, position: Offset) -> usize;
    /// The box of the character at `offset`, its line's height. Panics unless a character
    /// starts at or covers `offset`.
    fn get_bounding_box(&self, offset: usize) -> Rect;
    /// The boxes of the characters in `start..end`, as Compose's
    /// `TextLayoutResult.getBoundingBoxesForRange`.
    fn get_bounding_boxes_for_range(&self, start: usize, end: usize) -> Vec<Rect> {
        let text = self.text();
        text.char_indices()
            .map(|(offset, _)| offset)
            .filter(|&offset| offset >= start && offset < end)
            .map(|offset| self.get_bounding_box(offset))
            .collect()
    }
    /// The cursor before `offset`: zero wide, its line's height.
    fn get_cursor_rect(&self, offset: usize) -> Rect {
        let length = self.text().len();
        assert!(offset <= length, "offset({offset}) is out of bounds [0,{length}]");
        let horizontal = self.get_horizontal_position(offset, true);
        let line = self.get_line_for_offset(offset);
        Rect::new(horizontal, self.get_line_top(line), horizontal, self.get_line_bottom(line))
    }
    /// The word around `offset`, as Compose's `WordIterator` finds it
    /// ([`word_boundary`](crate::platform::word_boundary): Unicode's word breaks, a run of
    /// punctuation a word, a collapsed range between spaces).
    fn get_word_boundary(&self, offset: usize) -> TextRange {
        crate::word_boundary::word_boundary(self.text(), offset)
    }
    /// The area of `start..end`'s selection, as Android's `Layout.getSelectionPath`.
    fn get_path_for_range(&self, start: usize, end: usize) -> Path;
}

impl fmt::Debug for dyn Paragraph {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Paragraph")
            .field("text", &self.text())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("line_count", &self.line_count())
            .finish()
    }
}

/// What a text was laid out from, as Compose's `TextLayoutInput`.
#[derive(Clone, PartialEq, Debug)]
pub struct TextLayoutInput {
    pub text: crate::annotated::AnnotatedString,
    pub style: crate::text_style::TextStyle,
    pub max_lines: u32,
    pub soft_wrap: bool,
    pub overflow: TextOverflow,
    pub density: Density,
    pub layout_direction: crate::alignment::LayoutDirection,
    pub font_family_resolver: crate::font::FontFamilyResolver,
    pub default_locale_list: crate::intl::LocaleList,
    pub constraints: crate::constraints::Constraints,
}

/// Whether a line's visible end leaves `c` out, as Compose's `LayoutHelper.isLineEndSpace`:
/// the space, the line feed, the ogham space mark, the en to hair spaces but the figure space,
/// the medium mathematical space and the ideographic space (a tab stays visible).
pub fn is_line_end_space(c: char) -> bool {
    matches!(c, ' ' | '\n' | '\u{1680}' | '\u{205F}' | '\u{3000}')
        || (('\u{2000}'..='\u{200A}').contains(&c) && c != '\u{2007}')
}

/// One line of a [`ParagraphLayout`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LineLayout {
    /// The offset the line starts at.
    pub start: usize,
    /// The offset after it, its trailing spaces and line feed included (the text's end for an
    /// ellipsized last line, as Compose's).
    pub end: usize,
    /// The offset after its last visible character.
    pub visible_end: usize,
    pub ellipsized: bool,
    pub top: f32,
    pub bottom: f32,
    pub baseline: f32,
    /// Its visible characters' extent.
    pub left: f32,
    pub right: f32,
    /// Its width, as Android's `Layout.getLineWidth`.
    pub width: f32,
    /// Its paragraph runs right to left.
    pub right_to_left: bool,
    /// The horizontal positions of its end, as the line's (Compose's upstream positions): where
    /// a character of the paragraph's direction, or of the other one, would go after its last
    /// visible character.
    pub end_primary: f32,
    pub end_secondary: f32,
}

/// One character boundary of a [`ParagraphLayout`]: its horizontal positions, and the
/// character after it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CharPosition {
    pub offset: usize,
    /// Where a character of the paragraph's direction would go here (Android's
    /// `getPrimaryHorizontal`), on the line this offset starts or is in.
    pub primary: f32,
    /// Where a character of the other direction would go here (`getSecondaryHorizontal`).
    pub secondary: f32,
    /// The character after it is in a right-to-left run (false at the text's end).
    pub right_to_left: bool,
    /// The character's extent on its line, for selections (zero wide at the text's end).
    pub left: f32,
    pub right: f32,
}

/// A paragraph's layout as data, with Compose's queries over it: what a measurer that can
/// compute the whole layout at once gives (the fixed measurer, Android's `Layout` read in one
/// crossing).
#[derive(Clone, PartialEq, Debug)]
pub struct ParagraphLayout {
    pub text: String,
    pub width: f32,
    pub height: f32,
    pub min_intrinsic_width: f32,
    pub max_intrinsic_width: f32,
    pub did_exceed_max_lines: bool,
    pub lines: Vec<LineLayout>,
    /// One per character boundary, in order, the text's end included.
    pub positions: Vec<CharPosition>,
}

impl ParagraphLayout {
    /// The position at `offset`, or the character's covering it (all zeros without positions).
    fn position(&self, offset: usize) -> CharPosition {
        let index = match self.positions.binary_search_by_key(&offset, |position| position.offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        self.positions.get(index).or(self.positions.last()).copied().unwrap_or(CharPosition {
            offset: 0,
            primary: 0.0,
            secondary: 0.0,
            right_to_left: false,
            left: 0.0,
            right: 0.0,
        })
    }

    /// The positions of the character boundaries in `range`.
    pub fn positions_in(&self, range: std::ops::Range<usize>) -> &[CharPosition] {
        let first = self.positions.partition_point(|position| position.offset < range.start);
        let last = self.positions.partition_point(|position| position.offset < range.end);
        &self.positions[first..last.max(first)]
    }

    fn line(&self, line: usize) -> &LineLayout {
        &self.lines[line]
    }

    /// The horizontal position of `offset` as on the line before when `offset` starts a line
    /// (Compose's upstream position), else as `get_horizontal_position`.
    fn upstream_horizontal(&self, offset: usize, primary: bool) -> f32 {
        if offset != 0 && offset != self.text.len() {
            let line = self.get_line_for_offset(offset);
            if line > 0 && self.lines[line].start == offset {
                let previous = &self.lines[line - 1];
                return if primary { previous.end_primary } else { previous.end_secondary };
            }
        }
        self.get_horizontal_position(offset, primary)
    }

    /// The offset nearest to `horizontal` on `line`, as Android's `getOffsetForHorizontal`:
    /// a line other than the last ends before its last character.
    pub fn get_offset_for_horizontal(&self, line: usize, horizontal: f32) -> usize {
        use unicode_segmentation::UnicodeSegmentation;
        let layout = self.line(line);
        let text = &self.text[layout.start..layout.end];
        // Grapheme cluster boundaries only (never between a letter and its mark); a line
        // other than the last ends before its last cluster (D1111).
        let max = if line + 1 == self.lines.len() {
            layout.end
        } else {
            text.grapheme_indices(true)
                .next_back()
                .map_or(layout.end, |(index, _)| layout.start + index)
        };
        let mut best = layout.start;
        let mut best_distance = (self.position(layout.start).primary - horizontal).abs();
        for (index, _) in text.grapheme_indices(true).skip(1) {
            let offset = layout.start + index;
            if offset >= max {
                break;
            }
            let distance = (self.position(offset).primary - horizontal).abs();
            if distance < best_distance {
                best = offset;
                best_distance = distance;
            }
        }
        if (self.position(max).primary - horizontal).abs() <= best_distance {
            best = max;
        }
        best
    }

    /// The selection rectangles of `start..end` on `line`, one per bidi run, as Android's
    /// `Layout.addSelection`.
    fn add_selection(&self, line: usize, start: usize, end: usize, path: &Path) {
        let layout = self.line(line);
        let mut line_end = layout.end;
        if line_end > layout.start && self.text.as_bytes()[line_end - 1] == b'\n' {
            line_end -= 1;
        }
        let (top, bottom) = (layout.top, layout.bottom);
        let mut run: Option<(bool, f32, f32)> = None;
        let flush = |run: &mut Option<(bool, f32, f32)>| {
            if let Some((_, left, right)) = run.take() {
                path.add_rect(Rect::new(left, top, right, bottom));
            }
        };
        for position in self.positions_in(layout.start..line_end) {
            let inside = position.offset >= start && position.offset < end;
            match (&mut run, inside) {
                (Some((rtl, left, right)), true) if *rtl == position.right_to_left => {
                    *left = left.min(position.left);
                    *right = right.max(position.right);
                }
                (_, true) => {
                    flush(&mut run);
                    run = Some((position.right_to_left, position.left, position.right));
                }
                (_, false) => flush(&mut run),
            }
        }
        flush(&mut run);
    }
}

impl Paragraph for ParagraphLayout {
    fn text(&self) -> &str {
        &self.text
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn min_intrinsic_width(&self) -> f32 {
        self.min_intrinsic_width
    }

    fn max_intrinsic_width(&self) -> f32 {
        self.max_intrinsic_width
    }

    fn did_exceed_max_lines(&self) -> bool {
        self.did_exceed_max_lines
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn get_line_left(&self, line: usize) -> f32 {
        self.line(line).left
    }

    fn get_line_right(&self, line: usize) -> f32 {
        self.line(line).right
    }

    fn get_line_top(&self, line: usize) -> f32 {
        self.line(line).top
    }

    fn get_line_bottom(&self, line: usize) -> f32 {
        self.line(line).bottom
    }

    fn get_line_baseline(&self, line: usize) -> f32 {
        self.line(line).baseline
    }

    fn get_line_width(&self, line: usize) -> f32 {
        self.line(line).width
    }

    fn get_line_start(&self, line: usize) -> usize {
        self.line(line).start
    }

    fn get_line_end(&self, line: usize, visible_end: bool) -> usize {
        let layout = self.line(line);
        if visible_end { layout.visible_end } else { layout.end }
    }

    fn is_line_ellipsized(&self, line: usize) -> bool {
        self.line(line).ellipsized
    }

    fn get_line_for_offset(&self, offset: usize) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        if offset >= self.text.len() {
            return self.lines.len() - 1;
        }
        // The last line starting at or before it.
        self.lines.partition_point(|line| line.start <= offset).saturating_sub(1)
    }

    fn get_line_for_vertical_position(&self, vertical: f32) -> usize {
        // Android's `getLineForVertical` takes whole pixels.
        let vertical = vertical.trunc();
        self.lines.partition_point(|line| line.top <= vertical).saturating_sub(1)
    }

    fn get_horizontal_position(&self, offset: usize, use_primary_direction: bool) -> f32 {
        let position = self.position(offset);
        if use_primary_direction { position.primary } else { position.secondary }
    }

    fn get_paragraph_direction(&self, offset: usize) -> ResolvedTextDirection {
        if self.lines.is_empty() {
            return ResolvedTextDirection::Ltr;
        }
        if self.line(self.get_line_for_offset(offset)).right_to_left {
            ResolvedTextDirection::Rtl
        } else {
            ResolvedTextDirection::Ltr
        }
    }

    fn get_bidi_run_direction(&self, offset: usize) -> ResolvedTextDirection {
        if self.position(offset).right_to_left {
            ResolvedTextDirection::Rtl
        } else {
            ResolvedTextDirection::Ltr
        }
    }

    fn get_offset_for_position(&self, position: Offset) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        // Above and below the lines: the first and the last (Android's `getLineForVertical`).
        let line = self.get_line_for_vertical_position(position.y);
        self.get_offset_for_horizontal(line, position.x)
    }

    fn get_bounding_box(&self, offset: usize) -> Rect {
        let length = self.text.len();
        assert!(offset < length, "offset({offset}) is out of bounds [0,{length})");
        let offset = crate::text_editing::floor_char_boundary(&self.text, offset);
        let next = crate::text_editing::ceil_char_boundary(&self.text, offset + 1);
        let line = self.line(self.get_line_for_offset(offset));
        let rtl_char = self.position(offset).right_to_left;
        // Compose's `TextLayout.getBoundingBox`, from the horizontal positions.
        let (left, right) = match (line.right_to_left, rtl_char) {
            (false, false) => {
                (self.get_horizontal_position(offset, true), self.upstream_horizontal(next, true))
            }
            (false, true) => {
                (self.upstream_horizontal(next, false), self.get_horizontal_position(offset, false))
            }
            (true, true) => {
                (self.upstream_horizontal(next, true), self.get_horizontal_position(offset, true))
            }
            (true, false) => {
                (self.get_horizontal_position(offset, false), self.upstream_horizontal(next, false))
            }
        };
        Rect::new(left, line.top, right, line.bottom)
    }

    fn get_path_for_range(&self, start: usize, end: usize) -> Path {
        let length = self.text.len();
        assert!(
            start <= end && end <= length,
            "start({start}) or end({end}) is out of range [0..{length}], or start > end!"
        );
        let path = Path::new();
        // A range's ends inside characters shrink it to the characters it covers whole, as
        // Android's with surrogate pairs.
        let start = crate::text_editing::ceil_char_boundary(&self.text, start);
        let end = crate::text_editing::floor_char_boundary(&self.text, end);
        if start >= end || self.lines.is_empty() {
            return path;
        }
        let first = self.get_line_for_offset(start);
        let last = self.get_line_for_offset(end);
        if first == last {
            self.add_selection(first, start, end, &path);
            return path;
        }
        // Android's `getSelectionPath`: the first line to the paragraph's end, the lines
        // between whole, the last line from the paragraph's start.
        let width = self.width;
        let line = self.line(first);
        self.add_selection(first, start, line.end, &path);
        if line.right_to_left {
            path.add_rect(Rect::new(0.0, line.top, line.left, line.bottom));
        } else {
            path.add_rect(Rect::new(line.right, line.top, width, line.bottom));
        }
        for index in first + 1..last {
            let line = self.line(index);
            path.add_rect(Rect::new(0.0, line.top, width, line.bottom));
        }
        let line = self.line(last);
        self.add_selection(last, line.start, end, &path);
        if line.right_to_left {
            path.add_rect(Rect::new(line.right, line.top, width, line.bottom));
        } else {
            path.add_rect(Rect::new(0.0, line.top, line.left, line.bottom));
        }
        path
    }
}

/// A [`ParagraphLayout`] read from a measurer's general queries: its metrics, and its cursor
/// at each character boundary (lines are the runs of boundaries with the same top; a line's
/// baseline is as far below its top as the first line's). `TextMeasurer::paragraph`'s default,
/// for measurers that don't give their lines; exact for left-to-right text, without bidi runs.
pub fn sampled_paragraph<M: crate::text::TextMeasurer + ?Sized>(
    measurer: &M,
    handle: crate::text::ParagraphHandle,
) -> ParagraphLayout {
    let text = measurer.text(handle).to_string();
    let metrics = measurer.metrics(handle);
    let rtl = measurer.is_right_to_left(handle);
    let mut boundaries: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
    boundaries.push(text.len());
    let cursors: Vec<Rect> =
        boundaries.iter().map(|&offset| measurer.cursor_rect(handle, offset)).collect();
    let mut lines: Vec<LineLayout> = Vec::new();
    let baseline_offset = metrics.first_baseline - cursors.first().map_or(0.0, |rect| rect.top);
    for (&offset, cursor) in boundaries.iter().zip(&cursors) {
        if lines.last().is_none_or(|line| line.top != cursor.top) {
            if let Some(line) = lines.last_mut() {
                line.end = offset;
            }
            lines.push(LineLayout {
                start: offset,
                end: text.len(),
                visible_end: offset,
                ellipsized: false,
                top: cursor.top,
                bottom: cursor.bottom,
                baseline: cursor.top + baseline_offset,
                left: cursor.left,
                right: cursor.left,
                width: 0.0,
                right_to_left: rtl,
                end_primary: cursor.left,
                end_secondary: cursor.left,
            });
        }
        let line = lines.last_mut().expect("pushed above");
        line.left = line.left.min(cursor.left);
        line.right = line.right.max(cursor.left);
        if let Some(c) = text[offset..].chars().next()
            && !is_line_end_space(c)
        {
            line.visible_end = offset + c.len_utf8();
        }
    }
    let positions = boundaries
        .iter()
        .zip(&cursors)
        .enumerate()
        .map(|(index, (&offset, cursor))| {
            let next = cursors.get(index + 1).filter(|next| next.top == cursor.top);
            let (left, right) = match next {
                Some(next) => (cursor.left.min(next.left), cursor.left.max(next.left)),
                None => (cursor.left, cursor.left),
            };
            CharPosition {
                offset,
                primary: cursor.left,
                secondary: cursor.left,
                right_to_left: rtl && offset < text.len(),
                left,
                right,
            }
        })
        .collect::<Vec<_>>();
    for line in &mut lines {
        let edge = if rtl { line.left } else { line.right };
        line.width = line.right - line.left;
        line.end_primary = edge;
        line.end_secondary = edge;
    }
    let width = metrics.size.width as f32;
    ParagraphLayout {
        min_intrinsic_width: measurer
            .min_intrinsic_width(handle)
            .unwrap_or_else(|| widest_word(&text, &positions)),
        max_intrinsic_width: measurer.max_intrinsic_width(handle).unwrap_or(width),
        did_exceed_max_lines: measurer.did_exceed_max_lines(handle),
        width,
        height: metrics.size.height as f32,
        text,
        lines,
        positions,
    }
}

/// The widest word of `text` with its trailing spaces, from its characters' extents.
fn widest_word(text: &str, positions: &[CharPosition]) -> f32 {
    let mut widest: f32 = 0.0;
    let mut word = 0.0;
    let mut characters = text.chars().peekable();
    for position in positions {
        let Some(c) = characters.next() else { break };
        word += position.right - position.left;
        if c == '\n' || (c == ' ' && characters.peek() != Some(&' ')) {
            widest = widest.max(word);
            word = 0.0;
        }
    }
    widest.max(word)
}

/// A character's extent on its line and its bidi run's direction, as a measurer laid it out:
/// for [`ParagraphLayout::from_lines`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlyphBox {
    pub offset: usize,
    pub left: f32,
    pub right: f32,
    pub right_to_left: bool,
}

/// A line as a measurer laid it out, for [`ParagraphLayout::from_lines`]: its metrics (as
/// [`LineLayout`]'s), and the boxes of its characters but a line feed ending it, in the text's
/// order.
#[derive(Clone, PartialEq, Debug)]
pub struct LaidOutLine {
    pub start: usize,
    pub end: usize,
    pub visible_end: usize,
    pub top: f32,
    pub bottom: f32,
    pub baseline: f32,
    pub left: f32,
    pub right: f32,
    pub width: f32,
    pub right_to_left: bool,
    pub glyphs: Vec<GlyphBox>,
}

/// What `from_lines` measures a text by.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ParagraphSize {
    pub width: f32,
    pub height: f32,
    pub min_intrinsic_width: f32,
    pub max_intrinsic_width: f32,
    pub did_exceed_max_lines: bool,
    /// The last line ends with an ellipsis: it holds the rest of the text, as Android's.
    pub ellipsized: bool,
}

impl ParagraphLayout {
    /// The layout of `text` from its laid-out `lines`: each character boundary's horizontal
    /// positions by the rule Android's `getPrimaryHorizontal` and `getSecondaryHorizontal`
    /// follow (where a character of the paragraph's direction, or of the other one, would go:
    /// after the character before it when that one runs that way, else before the character
    /// after it when that one does, else between them, and at the line's start or end edge
    /// alone). Characters after the last line (cut by the maximum lines) are hidden at its end.
    pub fn from_lines(
        text: String,
        size: ParagraphSize,
        lines: Vec<LaidOutLine>,
    ) -> ParagraphLayout {
        let mut positions: Vec<CharPosition> = Vec::with_capacity(text.len() + 1);
        let mut layout_lines: Vec<LineLayout> = Vec::with_capacity(lines.len());
        let count = lines.len();
        for (number, line) in lines.into_iter().enumerate() {
            let rtl = line.right_to_left;
            let boxes = &line.glyphs;
            let start_edge = if rtl { line.right } else { line.left };
            let end_edge = if rtl { line.left } else { line.right };
            let leading = |b: &GlyphBox| if b.right_to_left { b.right } else { b.left };
            let trailing = |b: &GlyphBox| if b.right_to_left { b.left } else { b.right };
            // Where a character of direction `preferred_rtl` goes before box `index`, the
            // boxes from `limit` on not counted.
            let horizontal = |index: usize, limit: usize, preferred_rtl: bool| -> f32 {
                let before = (index > 0).then(|| &boxes[index - 1]);
                let after = (index < limit).then(|| &boxes[index]);
                match (before, after) {
                    (Some(b), _) if b.right_to_left == preferred_rtl => trailing(b),
                    (_, Some(a)) if a.right_to_left == preferred_rtl => leading(a),
                    (Some(b), Some(_)) => trailing(b),
                    (None, _) => start_edge,
                    (Some(_), None) => end_edge,
                }
            };
            let limit = boxes.len();
            for (index, glyph) in boxes.iter().enumerate() {
                positions.push(CharPosition {
                    offset: glyph.offset,
                    primary: horizontal(index, limit, rtl),
                    secondary: horizontal(index, limit, !rtl),
                    right_to_left: glyph.right_to_left,
                    left: glyph.left.min(glyph.right),
                    right: glyph.left.max(glyph.right),
                });
            }
            // A line feed ending it: after its content.
            let content_end = boxes.last().map_or(line.start, |b| {
                b.offset + text[b.offset..].chars().next().map_or(0, char::len_utf8)
            });
            let after = (horizontal(limit, limit, rtl), horizontal(limit, limit, !rtl));
            for (offset, _) in text[content_end.min(line.end)..line.end].char_indices() {
                positions.push(CharPosition {
                    offset: content_end + offset,
                    primary: after.0,
                    secondary: after.1,
                    right_to_left: rtl,
                    left: after.0,
                    right: after.0,
                });
            }
            let visible = boxes.partition_point(|b| b.offset < line.visible_end);
            let end_primary = horizontal(visible, visible, rtl);
            let end_secondary = horizontal(visible, visible, !rtl);
            if line.end == text.len() && number + 1 == count {
                positions.push(CharPosition {
                    offset: text.len(),
                    primary: after.0,
                    secondary: after.1,
                    right_to_left: false,
                    left: after.0,
                    right: after.0,
                });
            }
            layout_lines.push(LineLayout {
                start: line.start,
                end: line.end,
                visible_end: line.visible_end,
                ellipsized: size.ellipsized && number + 1 == count,
                top: line.top,
                bottom: line.bottom,
                baseline: line.baseline,
                left: line.left,
                right: line.right,
                width: line.width,
                right_to_left: rtl,
                end_primary,
                end_secondary,
            });
        }
        // Characters cut by the maximum lines: hidden at the last line's end, which an
        // ellipsis makes the text's end.
        if let Some(last) = layout_lines.last_mut()
            && positions.last().is_none_or(|position| position.offset < text.len())
        {
            let cut = last.end;
            if size.ellipsized {
                last.end = text.len();
            }
            let last = *last;
            let hidden = |offset| CharPosition {
                offset,
                primary: last.end_primary,
                secondary: last.end_secondary,
                right_to_left: last.right_to_left,
                left: last.end_primary,
                right: last.end_primary,
            };
            positions.extend(text[cut..].char_indices().map(|(offset, _)| hidden(cut + offset)));
            positions.push(CharPosition { right_to_left: false, ..hidden(text.len()) });
        }
        if positions.is_empty() {
            positions.push(CharPosition {
                offset: 0,
                primary: 0.0,
                secondary: 0.0,
                right_to_left: false,
                left: 0.0,
                right: 0.0,
            });
        }
        ParagraphLayout {
            text,
            width: size.width,
            height: size.height,
            min_intrinsic_width: size.min_intrinsic_width,
            max_intrinsic_width: size.max_intrinsic_width,
            did_exceed_max_lines: size.did_exceed_max_lines,
            lines: layout_lines,
            positions,
        }
    }
}
