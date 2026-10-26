//! Text: shared strings, styles, and the `TextMeasurer` interface each platform implements with
//! its own text stack (docs/07-platforms.md). `FixedTextMeasurer` lays text out with a fixed
//! advance per character, for layout tests that shouldn't depend on fonts.

use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use compose_runtime::ComposeArg;

use crate::constraints::Constraints;
use crate::text_attributes::TextOverflow;
use crate::text_style::TextStyle;
use crate::unit::{Density, IntSize, Offset, TextUnit};

/// A string shared by pointer: a static literal or an `Rc<str>`. Cheap to clone; compared by
/// pointer and length first, then by value (UI strings are short).
///
/// Converts from `&'static str` (kept as is), `String` and `&String` (copied into an `Rc<str>`),
/// `Rc<str>`, `&Rc<str>` and `&SharedString` (shared). A borrowed `&str` that isn't `'static`
/// converts through `String` (`text.to_string()`): `From<&'a str>` would conflict with the
/// static literals' conversion, which allocates nothing.
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

impl From<&String> for SharedString {
    /// A copy of the text (`Text!(&name)`).
    fn from(text: &String) -> SharedString {
        SharedString::Shared(Rc::from(text.as_str()))
    }
}

impl From<Rc<str>> for SharedString {
    fn from(text: Rc<str>) -> SharedString {
        SharedString::Shared(text)
    }
}

impl From<&Rc<str>> for SharedString {
    fn from(text: &Rc<str>) -> SharedString {
        SharedString::Shared(text.clone())
    }
}

impl From<&SharedString> for SharedString {
    fn from(text: &SharedString) -> SharedString {
        text.clone()
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

/// What a text's layout came to, as Compose's `TextLayoutResult` (a snapshot of its metrics,
/// without its queries): what `on_text_layout` hears after each new layout.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextLayoutResult {
    /// The text's size: the paragraph's, coerced into the constraints.
    pub size: IntSize,
    /// The lines laid out (at most `max_lines`).
    pub line_count: u32,
    pub first_baseline: f32,
    pub last_baseline: f32,
    /// The paragraph is wider than `size`.
    pub did_overflow_width: bool,
    /// The paragraph is taller than `size`, or had more lines than `max_lines`.
    pub did_overflow_height: bool,
}

impl TextLayoutResult {
    /// The text doesn't fit: it's clipped, ellipsized or drawn past its bounds, as Compose's
    /// `hasVisualOverflow`.
    pub fn has_visual_overflow(&self) -> bool {
        self.did_overflow_width || self.did_overflow_height
    }
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
    /// The paragraph's direction when its text has no strong character (the layout's), as
    /// `TextDirection.Content`'s fallback.
    pub direction: crate::alignment::LayoutDirection,
    /// What shows when the text doesn't fit its lines or width.
    pub overflow: TextOverflow,
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        TextLayoutOptions {
            soft_wrap: true,
            max_lines: u32::MAX,
            direction: crate::alignment::LayoutDirection::Ltr,
            overflow: TextOverflow::Clip,
        }
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
    /// Lays out styled text, as `layout` does plain text: each span in its style. The default
    /// ignores the spans (a measurer that can't style runs).
    fn layout_annotated(
        &mut self,
        text: &crate::annotated::AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.layout(text.as_str(), style, options, constraints, density)
    }
    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics;
    /// Whether the text had more lines than its `max_lines` (cut or ellipsized), as
    /// `Paragraph.didExceedMaxLines`. The default says no (a measurer that can't tell).
    fn did_exceed_max_lines(&self, _paragraph: ParagraphHandle) -> bool {
        false
    }
    /// The text's own width, as Compose's `ParagraphIntrinsics.maxIntrinsicWidth`: the width it
    /// takes without soft line breaks, from which the paragraph's width was chosen (Compose's
    /// `finalMaxWidth`). `None` when the measurer doesn't know it (the default).
    fn max_intrinsic_width(&self, _paragraph: ParagraphHandle) -> Option<f32> {
        None
    }
    /// The text's width with every soft line break taken (its widest word), as Compose's
    /// `ParagraphIntrinsics.minIntrinsicWidth`: what `IntrinsicSize::Min` sizes a text to.
    /// `None` when the measurer doesn't know it (the default): the text then answers its
    /// maximum intrinsic width for it.
    fn min_intrinsic_width(&self, _paragraph: ParagraphHandle) -> Option<f32> {
        None
    }
    /// Lays out `paragraph`'s text again, in the same style and options, at `constraints`, as
    /// Compose lays out a new `Paragraph` from the `ParagraphIntrinsics` it keeps (the text's
    /// spans, paint and own width, measured once): returns the new paragraph and releases
    /// `paragraph`. `None` (the default) when the measurer keeps nothing to reuse: `paragraph`
    /// is left as it is, and the caller lays the text out anew.
    fn relayout(
        &mut self,
        _paragraph: ParagraphHandle,
        _constraints: Constraints,
    ) -> Option<ParagraphHandle> {
        None
    }
    /// Lays `text` out for its intrinsic sizes: measured, never drawn, so a platform prepares
    /// nothing to draw it (Android doesn't record it ahead, D322). `measured` is the text's
    /// measured paragraph in the same style, options, density and direction, if there's one: what
    /// the measurer kept of it may be reused (as Compose's intrinsics come from its
    /// `ParagraphIntrinsics`), and it stays as it is. The default lays the text out anew.
    #[allow(clippy::too_many_arguments)]
    fn layout_probe(
        &mut self,
        measured: Option<ParagraphHandle>,
        text: &crate::annotated::AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let _ = measured;
        self.layout_annotated(text, style, options, constraints, density)
    }
    /// Draws `paragraph`'s layout in `style`'s paint (its color and decoration, what
    /// [`TextStyle::has_same_layout_affecting_attributes`] leaves out), as Compose draws a kept
    /// paragraph with `Paragraph.paint(color, textDecoration)` after a style change that leaves
    /// its layout alone: returns a new paragraph with the same layout, laid out again in that
    /// paint later, and releases `paragraph`. `None` (the default) when the measurer can't:
    /// `paragraph` is left as it is, and the caller lays the text out anew.
    fn restyle(
        &mut self,
        _paragraph: ParagraphHandle,
        _style: &TextStyle,
    ) -> Option<ParagraphHandle> {
        None
    }
    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition;
    /// Whether the paragraph runs right to left (its first strong character's direction), as
    /// `TextLayoutResult.getParagraphDirection`: Left and Right move the other way.
    fn is_right_to_left(&self, _paragraph: ParagraphHandle) -> bool {
        false
    }
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

/// The default font size when a style leaves it unspecified, as Compose's (`DefaultFontSize`).
pub const DEFAULT_FONT_SIZE: TextUnit = TextUnit::sp(14.0);

/// A style's font size in pixels: 14 sp when unset, and an `em` size a multiple of 14 sp (a
/// text's own style has no enclosing size; a span's `em` size is resolved over its text's by
/// [`crate::AnnotatedString::runs`]).
pub fn resolve_font_size_px(style: &TextStyle, density: &Density) -> f32 {
    let size = style.font_size;
    if size.is_sp() {
        density.sp_to_px(size)
    } else if size.is_em() {
        density.sp_to_px(DEFAULT_FONT_SIZE) * size.value()
    } else {
        density.sp_to_px(DEFAULT_FONT_SIZE)
    }
}

/// The color a style's text is painted in by a measurer without text shaders: its color, a
/// brush's color or gradient average at its alpha, black when unset (Compose's default).
pub fn resolve_text_color(style: &TextStyle) -> crate::graphics::Color {
    match style.foreground() {
        crate::text_attributes::TextForegroundStyle::Unspecified => crate::graphics::Color::BLACK,
        crate::text_attributes::TextForegroundStyle::Color(color) => color,
        crate::text_attributes::TextForegroundStyle::Brush { brush, alpha } => {
            brush.color(if alpha.is_nan() { 1.0 } else { alpha })
        }
    }
}

/// The pixels of `unit` in text `font_px` pixels high: `sp` through the density, `em` a
/// multiple of the font size; `None` when unspecified. For letter spacing and line heights.
pub fn resolve_text_unit_px(unit: TextUnit, font_px: f32, density: &Density) -> Option<f32> {
    if unit.is_sp() {
        Some(density.sp_to_px(unit))
    } else if unit.is_em() {
        Some(unit.value() * font_px)
    } else {
        None
    }
}

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
    exceeded_max_lines: bool,
    /// The widest hard line's width.
    intrinsic: f32,
    /// The widest segment's width ([`widest_segment`]).
    min_intrinsic: f32,
}

/// The characters in `text`'s longest segment between line break opportunities: a word with
/// the spaces after it, as a line break iterator segments text (what Compose's
/// `minIntrinsicWidth` measures).
fn widest_segment(text: &str) -> usize {
    let mut widest = 0;
    for line in text.split('\n') {
        let (mut length, mut after_space) = (0, false);
        for character in line.chars() {
            if character != ' ' && after_space {
                widest = widest.max(length);
                length = 0;
            }
            after_space = character == ' ';
            length += 1;
        }
        widest = widest.max(length);
    }
    widest
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
        let font_px = resolve_font_size_px(style, density);
        // Letter spacing widens each character's advance.
        let spacing = resolve_text_unit_px(style.letter_spacing, font_px, density).unwrap_or(0.0);
        let advance = font_px * 0.5 + spacing;
        let line_height =
            resolve_text_unit_px(style.line_height, font_px, density).unwrap_or(font_px * 1.25);
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
        // As Compose: an ellipsis without soft wrapping is one line.
        let max_lines = if options.overflow == TextOverflow::Ellipsis && !options.soft_wrap {
            1
        } else {
            options.max_lines
        };
        let exceeded_max_lines = lines.len() > max_lines.max(1) as usize;
        lines.truncate(max_lines.max(1) as usize);
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
        let paragraph = FixedParagraph {
            text: text.to_string(),
            lines,
            advance,
            line_height,
            size,
            exceeded_max_lines,
            intrinsic,
            min_intrinsic: widest_segment(text) as f32 * advance,
        };
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

    fn did_exceed_max_lines(&self, handle: ParagraphHandle) -> bool {
        self.paragraph(handle).exceeded_max_lines
    }

    fn max_intrinsic_width(&self, handle: ParagraphHandle) -> Option<f32> {
        Some(self.paragraph(handle).intrinsic)
    }

    fn min_intrinsic_width(&self, handle: ParagraphHandle) -> Option<f32> {
        Some(self.paragraph(handle).min_intrinsic)
    }

    /// The same lines under a new handle: its paragraphs have no paint.
    fn restyle(&mut self, handle: ParagraphHandle, _style: &TextStyle) -> Option<ParagraphHandle> {
        let paragraph = self.paragraphs.get_mut(handle.0 as usize)?.take()?;
        let restyled = match self.free.pop() {
            Some(index) => {
                self.paragraphs[index as usize] = Some(paragraph);
                ParagraphHandle(index)
            }
            None => {
                self.paragraphs.push(Some(paragraph));
                ParagraphHandle(self.paragraphs.len() as u64 - 1)
            }
        };
        self.free.push(handle.0);
        Some(restyled)
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
        let style = TextStyle { font_size: TextUnit::sp(10.0), ..TextStyle::DEFAULT };
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
        let clipped = TextLayoutOptions { soft_wrap: true, max_lines: 1, ..Default::default() };
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
