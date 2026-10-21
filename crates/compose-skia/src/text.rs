//! `SkiaTextMeasurer`: paragraphs laid out by Skia's paragraph module (SkParagraph: HarfBuzz
//! shaping, ICU line breaking and bidi), as Compose Desktop's (docs/07-platforms.md, "Text").

use compose_ui::{
    Constraints, Density, IntSize, Offset, ParagraphHandle, ParagraphMetrics, Rect,
    TextLayoutOptions, TextMeasurer, TextPosition, TextStyle,
};
use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, RectHeightStyle, RectWidthStyle,
    TextStyle as SkiaTextStyle,
};
use skia_safe::{FontMgr, FontStyle, font_style};

/// The families tried first, as Compose Desktop's default `FontFamily.SansSerif`, before the
/// font manager's fallback.
const DEFAULT_FAMILIES: &[&str] = &[
    "Roboto",
    "Inter",
    "Helvetica Neue",
    "Segoe UI",
    "Noto Sans",
    "DejaVu Sans",
    "Arial",
    "sans-serif",
];

struct Entry {
    paragraph: Paragraph,
    text: String,
    metrics: ParagraphMetrics,
    right_to_left: bool,
}

/// Lays out paragraphs with SkParagraph and the system's fonts, keeping each until released:
/// the renderer draws a paragraph by handle with the same object.
pub struct SkiaTextMeasurer {
    collection: FontCollection,
    paragraphs: Vec<Option<Entry>>,
    free: Vec<u64>,
}

impl Default for SkiaTextMeasurer {
    fn default() -> Self {
        SkiaTextMeasurer::new()
    }
}

impl SkiaTextMeasurer {
    /// A measurer over the system's fonts.
    pub fn new() -> SkiaTextMeasurer {
        SkiaTextMeasurer::with_font_manager(FontMgr::new())
    }

    pub fn with_font_manager(font_manager: FontMgr) -> SkiaTextMeasurer {
        let mut collection = FontCollection::new();
        collection.set_default_font_manager(font_manager, None);
        SkiaTextMeasurer { collection, paragraphs: Vec::new(), free: Vec::new() }
    }

    /// The paragraph behind `handle`, for drawing.
    pub fn paragraph(&self, handle: ParagraphHandle) -> Option<&Paragraph> {
        self.paragraphs.get(handle.0 as usize)?.as_ref().map(|entry| &entry.paragraph)
    }

    /// The number of live paragraphs: for leak tests.
    pub fn live(&self) -> usize {
        self.paragraphs.iter().filter(|entry| entry.is_some()).count()
    }

    fn entry(&self, handle: ParagraphHandle) -> &Entry {
        self.paragraphs
            .get(handle.0 as usize)
            .and_then(Option::as_ref)
            .expect("compose-skia: a released or unknown paragraph")
    }
}

/// Whether the first strong character of `text` is right-to-left (a simplified Unicode bidi
/// rule P2: Hebrew, Arabic, Syriac, Thaana, N'Ko and their presentation forms).
fn first_strong_is_rtl(text: &str) -> Option<bool> {
    for c in text.chars() {
        let code = c as u32;
        if matches!(code, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF) {
            return Some(true);
        }
        if c.is_alphabetic() {
            return Some(false);
        }
    }
    None
}

/// Whether a paragraph of `text` runs right to left: from its first strong character, else
/// the layout's direction, as Compose's `TextDirection.Content`.
fn is_rtl_paragraph(text: &str, options: TextLayoutOptions) -> bool {
    first_strong_is_rtl(text).unwrap_or(options.direction == compose_ui::LayoutDirection::Rtl)
}

/// The UTF-16 index of byte `offset` of `text` (SkParagraph indexes in UTF-16).
fn utf16_index(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].encode_utf16().count()
}

/// The byte offset of UTF-16 index `index` of `text`.
fn byte_offset(text: &str, index: usize) -> usize {
    let mut units = 0;
    for (offset, c) in text.char_indices() {
        if units >= index {
            return offset;
        }
        units += c.len_utf16();
    }
    text.len()
}

impl TextMeasurer for SkiaTextMeasurer {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let font_size = if style.font_size.is_specified() {
            style.font_size
        } else {
            compose_ui::text::DEFAULT_FONT_SIZE
        };
        let font_px = density.sp_to_px(font_size);
        let mut text_style = SkiaTextStyle::new();
        let color = if style.color.is_specified() { style.color } else { compose_ui::Color::BLACK };
        text_style.set_color(skia_safe::Color::new(color.argb()));
        text_style.set_font_size(font_px);
        text_style.set_font_families(DEFAULT_FAMILIES);
        let weight = font_style::Weight::from(style.font_weight.0 as i32);
        text_style.set_font_style(FontStyle::new(
            weight,
            font_style::Width::NORMAL,
            font_style::Slant::Upright,
        ));
        if style.line_height.is_specified() && font_px > 0.0 {
            text_style.set_height(density.sp_to_px(style.line_height) / font_px);
            text_style.set_height_override(true);
        }
        let mut paragraph_style = ParagraphStyle::new();
        paragraph_style.set_text_style(&text_style);
        // The direction from the content, as Compose resolves it: right-to-left when the first
        // strong character is (Hebrew, Arabic and the other right-to-left scripts), else the
        // layout's.
        let right_to_left = is_rtl_paragraph(text, options);
        if right_to_left {
            paragraph_style.set_text_direction(skia_safe::textlayout::TextDirection::RTL);
        }
        if options.max_lines != u32::MAX {
            paragraph_style.set_max_lines(options.max_lines.max(1) as usize);
        }
        let mut builder = ParagraphBuilder::new(&paragraph_style, self.collection.clone());
        builder.push_style(&text_style);
        builder.add_text(text);
        let mut paragraph = builder.build();
        let width = if options.soft_wrap && constraints.has_bounded_width() {
            constraints.max_width as f32
        } else {
            f32::INFINITY
        };
        paragraph.layout(width);
        // As Compose's `finalMaxWidth`: laid out again at the text's own width, rounded up and
        // kept in the constraints, so alignment and right-to-left text start where the text
        // is (not at an unbounded width's far edge).
        let min_width = constraints.min_width as f32;
        let measured_width = if min_width == width {
            width
        } else {
            paragraph.max_intrinsic_width().ceil().clamp(min_width, width)
        };
        if measured_width != width {
            paragraph.layout(measured_width);
        }
        let size = IntSize::new(measured_width.ceil() as i32, paragraph.height().ceil() as i32);
        let lines = paragraph.get_line_metrics();
        let metrics = ParagraphMetrics {
            size,
            first_baseline: paragraph.alphabetic_baseline(),
            last_baseline: lines
                .last()
                .map_or(paragraph.alphabetic_baseline(), |line| line.baseline as f32),
            line_count: paragraph.line_number().max(1) as u32,
        };
        drop(lines);
        let entry = Entry { paragraph, text: text.to_string(), metrics, right_to_left };
        match self.free.pop() {
            Some(index) => {
                self.paragraphs[index as usize] = Some(entry);
                ParagraphHandle(index)
            }
            None => {
                self.paragraphs.push(Some(entry));
                ParagraphHandle(self.paragraphs.len() as u64 - 1)
            }
        }
    }

    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
        self.entry(paragraph).metrics
    }

    fn is_right_to_left(&self, paragraph: ParagraphHandle) -> bool {
        self.entry(paragraph).right_to_left
    }

    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
        let entry = self.entry(paragraph);
        let found = entry.paragraph.get_glyph_position_at_coordinate((position.x, position.y));
        TextPosition { offset: byte_offset(&entry.text, found.position.max(0) as usize) }
    }

    fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> Rect {
        let entry = self.entry(paragraph);
        let text = &entry.text;
        let at = utf16_index(text, offset);
        let rects = |range: std::ops::Range<usize>| {
            entry.paragraph.get_rects_for_range(range, RectHeightStyle::Max, RectWidthStyle::Tight)
        };
        let rtl = |found: &skia_safe::textlayout::TextBox| {
            found.direct == skia_safe::textlayout::TextDirection::RTL
        };
        // Before the character at the cursor: its leading edge (its right edge in right-to-left).
        if offset < text.len() && !text[offset..].starts_with('\n') {
            let next =
                utf16_index(text, offset + text[offset..].chars().next().map_or(1, char::len_utf8));
            if let Some(found) = rects(at..next).first() {
                let x = if rtl(found) { found.rect.right } else { found.rect.left };
                return Rect::new(x, found.rect.top, x, found.rect.bottom);
            }
        }
        // After the character before it: its trailing edge.
        if at > 0 {
            let previous = text[..offset].char_indices().next_back().map_or(0, |(index, _)| index);
            if !text[previous..offset].starts_with('\n')
                && let Some(found) = rects(utf16_index(text, previous)..at).last()
            {
                let x = if rtl(found) { found.rect.left } else { found.rect.right };
                return Rect::new(x, found.rect.top, x, found.rect.bottom);
            }
        }
        // An empty line: its start, a line high. The empty line after a final newline has no
        // metrics of its own: it's below the last one.
        let lines = entry.paragraph.get_line_metrics();
        let line = entry.paragraph.get_line_number_at(at);
        let (top, height) = match line.and_then(|line| lines.get(line)) {
            Some(line) => {
                ((line.baseline - line.ascent) as f32, (line.ascent + line.descent) as f32)
            }
            None => match lines.last() {
                Some(last) if text.ends_with('\n') && offset >= text.len() => {
                    let height = (last.ascent + last.descent) as f32;
                    ((last.baseline + last.descent) as f32, height)
                }
                Some(first) if line.is_none() && offset == 0 => {
                    ((first.baseline - first.ascent) as f32, (first.ascent + first.descent) as f32)
                }
                _ => (0.0, entry.paragraph.height()),
            },
        };
        let x = if entry.right_to_left {
            entry.paragraph.max_width().min(entry.metrics.size.width as f32)
        } else {
            0.0
        };
        Rect::new(x, top, x, top + height)
    }

    fn selection_rects(
        &self,
        paragraph: ParagraphHandle,
        range: std::ops::Range<usize>,
    ) -> Vec<Rect> {
        let entry = self.entry(paragraph);
        let (start, end) =
            (utf16_index(&entry.text, range.start), utf16_index(&entry.text, range.end));
        entry
            .paragraph
            .get_rects_for_range(start..end, RectHeightStyle::Max, RectWidthStyle::Tight)
            .iter()
            .map(|found| {
                Rect::new(found.rect.left, found.rect.top, found.rect.right, found.rect.bottom)
            })
            .collect()
    }

    fn text(&self, paragraph: ParagraphHandle) -> &str {
        &self.entry(paragraph).text
    }

    fn release(&mut self, paragraph: ParagraphHandle) {
        if let Some(slot) = self.paragraphs.get_mut(paragraph.0 as usize)
            && slot.take().is_some()
        {
            self.free.push(paragraph.0);
        }
    }
}
