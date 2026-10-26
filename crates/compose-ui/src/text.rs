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

/// Upright or italic, as `FontStyle`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// A font family, as `FontFamily`: the generic ones, or a family installed on the system by
/// name (falling back to the default).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FontFamily {
    SansSerif,
    Serif,
    Monospace,
    Cursive,
    Named(&'static str),
}

/// Lines drawn through or under text, as `TextDecoration` (combinable with `|`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct TextDecoration {
    pub underline: bool,
    pub line_through: bool,
}

impl TextDecoration {
    pub const NONE: TextDecoration = TextDecoration { underline: false, line_through: false };
    pub const UNDERLINE: TextDecoration = TextDecoration { underline: true, line_through: false };
    pub const LINE_THROUGH: TextDecoration =
        TextDecoration { underline: false, line_through: true };
}

impl std::ops::BitOr for TextDecoration {
    type Output = TextDecoration;
    fn bitor(self, other: TextDecoration) -> TextDecoration {
        TextDecoration {
            underline: self.underline || other.underline,
            line_through: self.line_through || other.line_through,
        }
    }
}

/// How lines sit in a paragraph's width, as `TextAlign`: `Start` and `End` follow the text's
/// direction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,
}

/// What shows when text doesn't fit, as `TextOverflow`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[non_exhaustive]
pub enum TextOverflow {
    /// Cut at the bounds.
    #[default]
    Clip,
    /// The last line shown ends with "…".
    Ellipsis,
    /// Drawn past the bounds.
    Visible,
}

macro_rules! compose_arg_by_eq {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

/// Where a line's extra height goes, as `LineHeightStyle.Alignment`: the share of it above the
/// text (`top_ratio`, 0 to 1), or `PROPORTIONAL` (-1: as the font's ascent and descent share
/// its height).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LineHeightAlignment {
    pub top_ratio: f32,
}

impl LineHeightAlignment {
    pub const TOP: LineHeightAlignment = LineHeightAlignment { top_ratio: 0.0 };
    pub const CENTER: LineHeightAlignment = LineHeightAlignment { top_ratio: 0.5 };
    pub const PROPORTIONAL: LineHeightAlignment = LineHeightAlignment { top_ratio: -1.0 };
    pub const BOTTOM: LineHeightAlignment = LineHeightAlignment { top_ratio: 1.0 };
}

/// Whether the first line's space above the text and the last line's below it are left out,
/// as `LineHeightStyle.Trim`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LineHeightTrim {
    FirstLineTop,
    LastLineBottom,
    Both,
    None,
}

impl LineHeightTrim {
    pub fn trims_first_line_top(self) -> bool {
        matches!(self, LineHeightTrim::FirstLineTop | LineHeightTrim::Both)
    }

    pub fn trims_last_line_bottom(self) -> bool {
        matches!(self, LineHeightTrim::LastLineBottom | LineHeightTrim::Both)
    }
}

/// How a line height is spread around each line's text, as Compose's `LineHeightStyle` (its
/// `Mode.Fixed`, the default: every line is the line height). A style without one
/// (`TextStyle::line_height_style` `None`) takes `LineHeightStyle::DEFAULT`, proportional with
/// both ends trimmed, as Compose's `createCharSequence` does without `includeFontPadding`
/// (which compose-rs doesn't have). Material 3's text styles center it and trim nothing
/// (`DefaultLineHeightStyle`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LineHeightStyle {
    pub alignment: LineHeightAlignment,
    pub trim: LineHeightTrim,
}

impl LineHeightStyle {
    /// As `LineHeightStyle.Default`: proportional, both ends trimmed.
    pub const DEFAULT: LineHeightStyle = LineHeightStyle {
        alignment: LineHeightAlignment::PROPORTIONAL,
        trim: LineHeightTrim::Both,
    };
}

compose_arg_by_eq!(FontStyle, FontFamily, TextDecoration, TextAlign, TextOverflow, LineHeightStyle);

/// How text looks, as Compose's `TextStyle`: what isn't set (`None`, unspecified) comes from
/// the style it's merged over, or the measurer's defaults.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextStyle {
    pub color: Color,
    pub font_size: Sp,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub font_family: Option<FontFamily>,
    pub letter_spacing: Sp,
    pub text_decoration: Option<TextDecoration>,
    pub text_align: Option<TextAlign>,
    pub line_height: Sp,
    /// How `line_height` is spread around each line: `None` as `LineHeightStyle::DEFAULT`
    /// (proportionally, both ends trimmed), as Compose's.
    pub line_height_style: Option<LineHeightStyle>,
}

impl TextStyle {
    /// As `TextStyle.Default`: nothing set, which the theme or measurer resolves.
    pub const DEFAULT: TextStyle = TextStyle {
        color: Color::UNSPECIFIED,
        font_size: Sp::UNSPECIFIED,
        font_weight: None,
        font_style: None,
        font_family: None,
        letter_spacing: Sp::UNSPECIFIED,
        text_decoration: None,
        text_align: None,
        line_height: Sp::UNSPECIFIED,
        line_height_style: None,
    };

    /// Whether a text in `other` is laid out as in this style, as Compose's
    /// `hasSameLayoutAffectingAttributes`: everything but what only paints (the color and the
    /// decoration, Compose's `hasSameDrawAffectingAttributes`) is the same.
    pub fn has_same_layout_affecting_attributes(&self, other: &TextStyle) -> bool {
        TextStyle { color: other.color, text_decoration: other.text_decoration, ..*self } == *other
    }

    /// `other`'s set fields over these, as `merge`.
    pub fn merge(&self, other: &TextStyle) -> TextStyle {
        let sp = |other: Sp, this: Sp| if other.is_specified() { other } else { this };
        TextStyle {
            color: other.color.or(self.color),
            font_size: sp(other.font_size, self.font_size),
            font_weight: other.font_weight.or(self.font_weight),
            font_style: other.font_style.or(self.font_style),
            font_family: other.font_family.or(self.font_family),
            letter_spacing: sp(other.letter_spacing, self.letter_spacing),
            text_decoration: other.text_decoration.or(self.text_decoration),
            text_align: other.text_align.or(self.text_align),
            line_height: sp(other.line_height, self.line_height),
            line_height_style: other.line_height_style.or(self.line_height_style),
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
            && self.font_style == other.font_style
            && self.font_family == other.font_family
            && self.letter_spacing.same(&other.letter_spacing)
            && self.text_decoration == other.text_decoration
            && self.text_align == other.text_align
            && self.line_height.same(&other.line_height)
            && self.line_height_style == other.line_height_style
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
    exceeded_max_lines: bool,
    /// The widest hard line's width.
    intrinsic: f32,
    /// The widest word's width.
    min_intrinsic: f32,
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
        // Letter spacing widens each character's advance.
        let spacing = if style.letter_spacing.is_specified() {
            density.sp_to_px(style.letter_spacing)
        } else {
            0.0
        };
        let advance = font_px * 0.5 + spacing;
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
            min_intrinsic: text
                .split([' ', '\n'])
                .map(|word| word.chars().count())
                .max()
                .unwrap_or(0) as f32
                * advance,
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
