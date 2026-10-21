//! Text: shared strings, styles, and the `TextMeasurer` interface each platform implements with
//! its own text stack (docs/07-platforms.md). `FixedTextMeasurer` lays text out with a fixed
//! advance per character, for layout tests that shouldn't depend on fonts.

use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use compose_runtime::ComposeArg;

use crate::constraints::Constraints;
use crate::graphics::Color;
use crate::unit::{Density, IntSize, Offset, Sp};

/// A string shared by pointer: a static literal or an `Rc<str>`. Cheap to clone; compared by
/// pointer and length first, then by value (UI strings are short).
#[derive(Clone)]
pub enum SharedString {
    Static(&'static str),
    Shared(Rc<str>),
}

impl SharedString {
    pub const EMPTY: SharedString = SharedString::Static("");

    pub const fn from_static(text: &'static str) -> SharedString {
        SharedString::Static(text)
    }

    pub fn as_str(&self) -> &str {
        match self {
            SharedString::Static(text) => text,
            SharedString::Shared(text) => text,
        }
    }
}

impl Deref for SharedString {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for SharedString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SharedString {}

impl std::hash::Hash for SharedString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl ComposeArg for SharedString {
    fn same(&self, other: &Self) -> bool {
        let (a, b) = (self.as_str(), other.as_str());
        (a.as_ptr() == b.as_ptr() && a.len() == b.len()) || a == b
    }
}

impl fmt::Debug for SharedString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), formatter)
    }
}

impl fmt::Display for SharedString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&'static str> for SharedString {
    fn from(text: &'static str) -> SharedString {
        SharedString::Static(text)
    }
}

impl From<String> for SharedString {
    fn from(text: String) -> SharedString {
        SharedString::Shared(Rc::from(text))
    }
}

impl From<Rc<str>> for SharedString {
    fn from(text: Rc<str>) -> SharedString {
        SharedString::Shared(text)
    }
}

impl From<&SharedString> for SharedString {
    fn from(text: &SharedString) -> SharedString {
        text.clone()
    }
}

/// Font weight, 100 to 900, as `FontWeight`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FontWeight(pub u16);

impl ComposeArg for FontWeight {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl FontWeight {
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const MEDIUM: FontWeight = FontWeight(500);
    pub const BOLD: FontWeight = FontWeight(700);
}

/// A subset of Compose's `TextStyle`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextStyle {
    pub color: Color,
    pub font_size: Sp,
    pub font_weight: FontWeight,
    pub line_height: Sp,
}

impl TextStyle {
    /// As `TextStyle.Default`: unspecified color and size, which the theme or measurer resolves.
    pub const DEFAULT: TextStyle = TextStyle {
        color: Color::UNSPECIFIED,
        font_size: Sp::UNSPECIFIED,
        font_weight: FontWeight::NORMAL,
        line_height: Sp::UNSPECIFIED,
    };

    /// `other`'s specified fields over these, as `merge`.
    pub fn merge(&self, other: &TextStyle) -> TextStyle {
        TextStyle {
            color: other.color.or(self.color),
            font_size: if other.font_size.is_specified() {
                other.font_size
            } else {
                self.font_size
            },
            font_weight: other.font_weight,
            line_height: if other.line_height.is_specified() {
                other.line_height
            } else {
                self.line_height
            },
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle::DEFAULT
    }
}

impl ComposeArg for TextStyle {
    fn same(&self, other: &Self) -> bool {
        self.color == other.color
            && self.font_size.same(&other.font_size)
            && self.font_weight == other.font_weight
            && self.line_height.same(&other.line_height)
    }
}

/// A paragraph laid out by a `TextMeasurer`, drawn by handle: the object that measured a text
/// also draws it, so measurement and drawing can't disagree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ParagraphHandle(pub u64);

/// A paragraph's metrics, in pixels: its own size, which the layout coerces into its
/// constraints.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ParagraphMetrics {
    pub size: IntSize,
    pub first_baseline: f32,
    pub last_baseline: f32,
    pub line_count: u32,
}

/// A position in a text, for hit testing (selection, the cursor).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TextPosition {
    pub offset: usize,
}

/// How a paragraph is broken into lines, as `BasicText`'s `softWrap` and `maxLines`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TextLayoutOptions {
    /// Break lines to fit the width; otherwise only at newlines.
    pub soft_wrap: bool,
    /// Lines past this are cut (clipped), as `TextOverflow.Clip`.
    pub max_lines: u32,
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        TextLayoutOptions { soft_wrap: true, max_lines: u32::MAX }
    }
}

/// Lays out and measures paragraphs, implemented by each platform's text stack.
pub trait TextMeasurer: std::any::Any {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle;
    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics;
    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition;
    /// The cursor before byte `offset`: its top and bottom in the paragraph, as
    /// `TextLayoutResult.getCursorRect` (zero width).
    fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> crate::unit::Rect;
    /// The rectangles covering bytes `range` of the paragraph, one per line at most: for a
    /// selection's highlight. The default joins the cursors at both ends, right for one line.
    fn selection_rects(
        &self,
        paragraph: ParagraphHandle,
        range: std::ops::Range<usize>,
    ) -> Vec<crate::unit::Rect> {
        if range.is_empty() {
            return Vec::new();
        }
        let (start, end) =
            (self.cursor_rect(paragraph, range.start), self.cursor_rect(paragraph, range.end));
        vec![crate::unit::Rect::new(
            start.left,
            start.top,
            end.left.max(start.left),
            start.bottom.max(end.bottom),
        )]
    }
    /// The paragraph's text, for dumps and semantics.
    fn text(&self, paragraph: ParagraphHandle) -> &str;
    fn release(&mut self, paragraph: ParagraphHandle);
}

/// The default font size when a style leaves it unspecified, as Material's body text.
pub const DEFAULT_FONT_SIZE: Sp = Sp(14.0);

/// A measurer with a fixed advance per character (`font size × 0.5`) and line height
/// (`font size × 1.25`), breaking lines at spaces to fit the width: deterministic layout for
/// tests, independent of fonts.
#[derive(Default)]
pub struct FixedTextMeasurer {
    paragraphs: Vec<Option<FixedParagraph>>,
    free: Vec<u64>,
}

struct FixedParagraph {
    text: String,
    lines: Vec<(usize, usize)>,
    advance: f32,
    line_height: f32,
    size: IntSize,
}

impl FixedTextMeasurer {
    pub fn new() -> FixedTextMeasurer {
        FixedTextMeasurer::default()
    }

    /// The number of live paragraphs: for leak tests.
    pub fn live(&self) -> usize {
        self.paragraphs.iter().filter(|paragraph| paragraph.is_some()).count()
    }

    fn paragraph(&self, handle: ParagraphHandle) -> &FixedParagraph {
        self.paragraphs
            .get(handle.0 as usize)
            .and_then(Option::as_ref)
            .expect("compose-ui: a released or unknown paragraph")
    }
}

impl TextMeasurer for FixedTextMeasurer {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let font_size =
            if style.font_size.is_specified() { style.font_size } else { DEFAULT_FONT_SIZE };
        let font_px = density.sp_to_px(font_size);
        let advance = font_px * 0.5;
        let line_height = if style.line_height.is_specified() {
            density.sp_to_px(style.line_height)
        } else {
            font_px * 1.25
        };
        let max_chars = if options.soft_wrap && constraints.has_bounded_width() && advance > 0.0 {
            ((constraints.max_width as f32 / advance).floor() as usize).max(1)
        } else {
            usize::MAX
        };
        // Greedy line breaking at spaces; a word longer than a line is split.
        let mut lines = Vec::new();
        for paragraph_line in text.split('\n') {
            let base = paragraph_line.as_ptr() as usize - text.as_ptr() as usize;
            let chars: Vec<(usize, char)> = paragraph_line.char_indices().collect();
            if chars.is_empty() {
                lines.push((base, base));
                continue;
            }
            let mut start = 0;
            while start < chars.len() {
                let mut end = (start + max_chars).min(chars.len());
                if end < chars.len()
                    && let Some(space) = (start..end).rev().find(|&index| chars[index].1 == ' ')
                    && space > start
                {
                    end = space;
                }
                let from = base + chars[start].0;
                let to = if end < chars.len() {
                    base + chars[end].0
                } else {
                    base + paragraph_line.len()
                };
                lines.push((from, to));
                start = end;
                while start < chars.len() && chars[start].1 == ' ' {
                    start += 1;
                }
            }
        }
        lines.truncate(options.max_lines.max(1) as usize);
        // As Compose: the width the text was laid out at when wrapping (its own width, the
        // widest hard line, when narrower), the lines' height; the layout coerces it into its
        // constraints.
        let intrinsic =
            text.split('\n').map(|line| line.chars().count()).max().unwrap_or(0) as f32 * advance;
        let width = if max_chars == usize::MAX {
            intrinsic
        } else {
            intrinsic.min(constraints.max_width as f32)
        };
        let size =
            IntSize::new(width.ceil() as i32, (lines.len() as f32 * line_height).ceil() as i32);
        let paragraph =
            FixedParagraph { text: text.to_string(), lines, advance, line_height, size };
        match self.free.pop() {
            Some(index) => {
                self.paragraphs[index as usize] = Some(paragraph);
                ParagraphHandle(index)
            }
            None => {
                self.paragraphs.push(Some(paragraph));
                ParagraphHandle(self.paragraphs.len() as u64 - 1)
            }
        }
    }

    fn metrics(&self, handle: ParagraphHandle) -> ParagraphMetrics {
        let paragraph = self.paragraph(handle);
        let baseline = paragraph.line_height * 0.8;
        ParagraphMetrics {
            size: paragraph.size,
            first_baseline: baseline,
            last_baseline: baseline
                + paragraph.line_height * (paragraph.lines.len().max(1) - 1) as f32,
            line_count: paragraph.lines.len() as u32,
        }
    }

    fn hit_test(&self, handle: ParagraphHandle, position: Offset) -> TextPosition {
        let paragraph = self.paragraph(handle);
        let line = ((position.y / paragraph.line_height).floor().max(0.0) as usize)
            .min(paragraph.lines.len().saturating_sub(1));
        let Some(&(from, to)) = paragraph.lines.get(line) else {
            return TextPosition { offset: 0 };
        };
        let column = (position.x / paragraph.advance).round().max(0.0) as usize;
        let offset = paragraph.text[from..to]
            .char_indices()
            .nth(column)
            .map_or(to, |(index, _)| from + index);
        TextPosition { offset }
    }

    fn selection_rects(
        &self,
        handle: ParagraphHandle,
        range: std::ops::Range<usize>,
    ) -> Vec<crate::unit::Rect> {
        let paragraph = self.paragraph(handle);
        let mut rects = Vec::new();
        for (index, &(from, to)) in paragraph.lines.iter().enumerate() {
            let (start, end) = (range.start.max(from), range.end.min(to));
            if start >= end {
                continue;
            }
            let column = |offset: usize| {
                paragraph.text[from..offset].chars().count() as f32 * paragraph.advance
            };
            let top = index as f32 * paragraph.line_height;
            rects.push(crate::unit::Rect::new(
                column(start),
                top,
                column(end),
                top + paragraph.line_height,
            ));
        }
        rects
    }

    fn cursor_rect(&self, handle: ParagraphHandle, offset: usize) -> crate::unit::Rect {
        let paragraph = self.paragraph(handle);
        let line = paragraph
            .lines
            .iter()
            .position(|&(from, to)| offset >= from && offset <= to)
            .unwrap_or(paragraph.lines.len().saturating_sub(1));
        let from = paragraph.lines.get(line).map_or(0, |line| line.0);
        let column =
            paragraph.text.get(from..offset.max(from)).map_or(0, |text| text.chars().count());
        let x = column as f32 * paragraph.advance;
        let top = line as f32 * paragraph.line_height;
        crate::unit::Rect::new(x, top, x, top + paragraph.line_height)
    }

    fn text(&self, handle: ParagraphHandle) -> &str {
        &self.paragraph(handle).text
    }

    fn release(&mut self, handle: ParagraphHandle) {
        if let Some(slot) = self.paragraphs.get_mut(handle.0 as usize)
            && slot.take().is_some()
        {
            self.free.push(handle.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_measurer_breaks_lines_at_spaces() {
        let mut measurer = FixedTextMeasurer::new();
        let style = TextStyle { font_size: Sp(10.0), ..TextStyle::DEFAULT };
        // 5 px per character, 12.5 px per line.
        let options = TextLayoutOptions::default();
        let handle = measurer.layout(
            "hello brave world",
            &style,
            options,
            Constraints::at_most(60, 100),
            &Density::default(),
        );
        let metrics = measurer.metrics(handle);
        assert_eq!(metrics.line_count, 2);
        // The width it was laid out at, as Compose's (not the longest line's).
        assert_eq!(metrics.size, IntSize::new(60, 25));
        let single =
            measurer.layout("hi", &style, options, Constraints::unbounded(), &Density::new(2.0));
        assert_eq!(measurer.cursor_rect(handle, 13), crate::unit::Rect::new(5.0, 12.5, 5.0, 25.0));
        let clipped = TextLayoutOptions { soft_wrap: true, max_lines: 1 };
        let one_line = measurer.layout(
            "hello brave world",
            &style,
            clipped,
            Constraints::at_most(60, 100),
            &Density::default(),
        );
        assert_eq!(measurer.metrics(one_line).line_count, 1);
        measurer.release(one_line);
        assert_eq!(measurer.metrics(single).size, IntSize::new(20, 25));
        assert_eq!(measurer.hit_test(handle, Offset::new(11.0, 1.0)).offset, 2);
        measurer.release(handle);
        assert_eq!(measurer.live(), 1);
    }

    #[test]
    fn shared_strings_compare_by_value() {
        let a: SharedString = String::from("abc").into();
        let b: SharedString = "abc".into();
        assert!(a.same(&b) && a == b);
        assert!(a.same(&a.clone()));
    }
}
