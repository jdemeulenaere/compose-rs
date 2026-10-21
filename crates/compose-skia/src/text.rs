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
        // As Compose: the widest line, the whole height, rounded up.
        let measured_width = if paragraph.line_number() > 1 && width.is_finite() {
            paragraph.longest_line()
        } else {
            paragraph.max_intrinsic_width()
        };
        let size = IntSize::new(measured_width.ceil() as i32, paragraph.height().ceil() as i32);
        let lines = paragraph.get_line_metrics();
        let metrics = ParagraphMetrics {
            size,
            first_baseline: paragraph.alphabetic_baseline(),
            last_baseline: lines
                .last()
                .map_or(paragraph.alphabetic_baseline(), |line| line.baseline as f32),
            line_count: paragraph.line_number() as u32,
        };
        drop(lines);
        let entry = Entry { paragraph, text: text.to_string(), metrics };
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
        if offset < text.len() && !text[offset..].starts_with('\n') {
            let next =
                utf16_index(text, offset + text[offset..].chars().next().map_or(1, char::len_utf8));
            if let Some(found) = rects(at..next).first() {
                return Rect::new(
                    found.rect.left,
                    found.rect.top,
                    found.rect.left,
                    found.rect.bottom,
                );
            }
        }
        if at > 0 {
            let previous = text[..offset].char_indices().next_back().map_or(0, |(index, _)| index);
            if !text[previous..offset].starts_with('\n')
                && let Some(found) = rects(utf16_index(text, previous)..at).last()
            {
                return Rect::new(
                    found.rect.right,
                    found.rect.top,
                    found.rect.right,
                    found.rect.bottom,
                );
            }
        }
        // An empty paragraph or line: its start, a line high.
        let line = entry.paragraph.get_line_number_at(at).unwrap_or(0);
        let metrics = entry.paragraph.get_line_metrics_at(line);
        let (top, height) = metrics.map_or((0.0, entry.paragraph.height()), |line| {
            ((line.baseline - line.ascent) as f32, (line.ascent + line.descent) as f32)
        });
        Rect::new(0.0, top, 0.0, top + height)
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
