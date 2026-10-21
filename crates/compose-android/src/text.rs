//! `AndroidTextMeasurer`: paragraphs laid out by Android's `StaticLayout` (docs/07-platforms.md,
//! "Text"), on the Kotlin side, which keeps each laid-out paragraph by handle to draw it.
//!
//! A layout is one crossing into Kotlin: the text, and a request encoding its style runs and
//! line options; the metrics come back with it. The text stays here too (byte offsets are
//! converted to UTF-16 ones for Kotlin), so what the layout needs again (the text, the
//! metrics, its direction) costs no crossing.

use compose_ui::{
    AnnotatedString, Color, Constraints, Density, FontFamily, LayoutDirection, Offset,
    ParagraphHandle, ParagraphMetrics, Rect, TextAlign, TextLayoutOptions, TextMeasurer,
    TextOverflow, TextPosition, TextStyle,
};

use crate::serialize::Writer;

/// The Kotlin side of text: `TextMeasurer.kt`, through JNI (or a fake, in tests).
pub trait TextBackend: 'static {
    /// Lays out paragraph `handle` (replacing one of the same handle): `text` with `request`
    /// (see `encode_request`), returning its metrics.
    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> ParagraphMetrics;
    /// The UTF-16 offset nearest to `position`.
    fn hit_test(&self, handle: u32, position: Offset) -> usize;
    /// The cursor before UTF-16 offset `offset`: its top, bottom and x.
    fn cursor(&self, handle: u32, offset: usize) -> Rect;
    fn release(&mut self, handle: u32);
}

struct Entry {
    text: String,
    metrics: ParagraphMetrics,
    right_to_left: bool,
}

/// A `TextMeasurer` over a `TextBackend`.
pub struct AndroidTextMeasurer<B: TextBackend> {
    backend: B,
    paragraphs: Vec<Option<Entry>>,
    free: Vec<u32>,
    request: Writer,
}

impl<B: TextBackend> AndroidTextMeasurer<B> {
    pub fn new(backend: B) -> Self {
        AndroidTextMeasurer {
            backend,
            paragraphs: Vec::new(),
            free: Vec::new(),
            request: Writer::default(),
        }
    }

    fn entry(&self, handle: ParagraphHandle) -> &Entry {
        self.paragraphs[handle.0 as usize].as_ref().expect("compose-android: a live paragraph")
    }

    /// The number of live paragraphs: for leak checks.
    pub fn live(&self) -> usize {
        self.paragraphs.iter().filter(|entry| entry.is_some()).count()
    }

    fn layout_runs(
        &mut self,
        text: &str,
        style: &TextStyle,
        runs: &[(std::ops::Range<usize>, TextStyle, Color)],
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let handle = match self.free.pop() {
            Some(handle) => handle,
            None => {
                self.paragraphs.push(None);
                self.paragraphs.len() as u32 - 1
            }
        };
        let right_to_left =
            first_strong_is_rtl(text).unwrap_or(options.direction == LayoutDirection::Rtl);
        let _layout = compose_ui::trace::section(c"compose-rs: text layout");
        encode_request(
            &mut self.request,
            text,
            style,
            runs,
            options,
            constraints,
            density,
            right_to_left,
        );
        let metrics = self.backend.layout(handle, text, &self.request.bytes);
        self.paragraphs[handle as usize] =
            Some(Entry { text: text.to_string(), metrics, right_to_left });
        ParagraphHandle(handle as u64)
    }
}

impl<B: TextBackend> TextMeasurer for AndroidTextMeasurer<B> {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.layout_runs(text, style, &[], options, constraints, density)
    }

    fn layout_annotated(
        &mut self,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let runs = if text.span_styles.is_empty() { Vec::new() } else { text.runs(style) };
        self.layout_runs(text.as_str(), style, &runs, options, constraints, density)
    }

    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
        self.entry(paragraph).metrics
    }

    fn is_right_to_left(&self, paragraph: ParagraphHandle) -> bool {
        self.entry(paragraph).right_to_left
    }

    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
        let index = self.backend.hit_test(paragraph.0 as u32, position);
        TextPosition { offset: byte_offset(&self.entry(paragraph).text, index) }
    }

    fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> Rect {
        let index = utf16_index(&self.entry(paragraph).text, offset);
        self.backend.cursor(paragraph.0 as u32, index)
    }

    /// One rectangle per line the range covers: the first line from the start to its end,
    /// the lines between whole, the last from its start to the end (left to right).
    fn selection_rects(
        &self,
        paragraph: ParagraphHandle,
        range: std::ops::Range<usize>,
    ) -> Vec<Rect> {
        if range.is_empty() {
            return Vec::new();
        }
        let (start, end) =
            (self.cursor_rect(paragraph, range.start), self.cursor_rect(paragraph, range.end));
        if start.top == end.top {
            let (left, right) = (start.left.min(end.left), start.left.max(end.left));
            return vec![Rect::new(left, start.top, right, start.bottom.max(end.bottom))];
        }
        let width = self.entry(paragraph).metrics.size.width as f32;
        let mut rects = vec![Rect::new(start.left, start.top, width, start.bottom)];
        if end.top > start.bottom {
            rects.push(Rect::new(0.0, start.bottom, width, end.top));
        }
        rects.push(Rect::new(0.0, end.top, end.left, end.bottom));
        rects
    }

    fn text(&self, paragraph: ParagraphHandle) -> &str {
        &self.entry(paragraph).text
    }

    fn release(&mut self, paragraph: ParagraphHandle) {
        let handle = paragraph.0 as u32;
        if self.paragraphs.get_mut(handle as usize).and_then(Option::take).is_some() {
            self.backend.release(handle);
            self.free.push(handle);
        }
    }
}

/// Whether `text`'s first strong character is right to left (`None` without one), as the
/// desktop measurer decides a paragraph's direction.
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

/// The UTF-16 index of byte `offset` of `text`.
pub fn utf16_index(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    text[..offset].encode_utf16().count()
}

/// The byte offset of UTF-16 `index` of `text` (at a character boundary).
pub fn byte_offset(text: &str, index: usize) -> usize {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        if units >= index {
            return at;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// The request `TextMeasurer.kt` reads: line options, then the base style, then the runs.
///
/// Options: soft wrap (u8), max lines (u32), ellipsis (u8), alignment (u8: 0 normal, 1
/// opposite, 2 center, 3 justify), right to left (u8), min and max width (i32, max
/// `i32::MAX` unbounded). A style: color (ARGB u32), font size (px f32), weight (u32), italic
/// (u8), family (u8: 0 sans serif, 1 serif, 2 monospace, 3 cursive, 4 named, then its name's
/// length and UTF-8 bytes), letter spacing (px f32), decoration (u8: 1 underline, 2 line
/// through), line height (px f32, 0 unset), background (ARGB u32, 0 none). Runs: their count
/// (u32), then each one's UTF-16 start and end (u32) and style.
#[allow(clippy::too_many_arguments)]
fn encode_request(
    out: &mut Writer,
    text: &str,
    style: &TextStyle,
    runs: &[(std::ops::Range<usize>, TextStyle, Color)],
    options: TextLayoutOptions,
    constraints: Constraints,
    density: &Density,
    right_to_left: bool,
) {
    out.bytes.clear();
    let ellipsis = options.overflow == TextOverflow::Ellipsis;
    // As Compose: an ellipsis without soft wrapping is one line.
    let max_lines = if ellipsis && !options.soft_wrap { 1 } else { options.max_lines };
    out.u8(u8::from(options.soft_wrap));
    // A Java `int`: unlimited is `Integer.MAX_VALUE`, not -1.
    out.u32(max_lines.clamp(1, i32::MAX as u32));
    out.u8(u8::from(ellipsis));
    let normal = |start: bool| if start { 0 } else { 1 };
    out.u8(match style.text_align {
        None | Some(TextAlign::Start) => 0,
        Some(TextAlign::End) => 1,
        // Left and right are absolute: normal or opposite by the paragraph's direction.
        Some(TextAlign::Left) => normal(!right_to_left),
        Some(TextAlign::Right) => normal(right_to_left),
        Some(TextAlign::Center) => 2,
        Some(TextAlign::Justify) => 3,
    });
    out.u8(u8::from(right_to_left));
    out.u32(constraints.min_width as u32);
    out.u32(if constraints.has_bounded_width() {
        constraints.max_width as u32
    } else {
        i32::MAX as u32
    });
    encode_style(out, style, Color::TRANSPARENT, density);
    out.u32(runs.len() as u32);
    for (range, run_style, background) in runs {
        out.u32(utf16_index(text, range.start) as u32);
        out.u32(utf16_index(text, range.end) as u32);
        encode_style(out, run_style, *background, density);
    }
}

fn encode_style(out: &mut Writer, style: &TextStyle, background: Color, density: &Density) {
    let color = if style.color.is_specified() { style.color } else { Color::BLACK };
    let font_size = if style.font_size.is_specified() {
        style.font_size
    } else {
        compose_ui::text::DEFAULT_FONT_SIZE
    };
    let font_px = density.sp_to_px(font_size);
    out.u32(color.argb());
    out.f32(font_px);
    out.u32(u32::from(style.font_weight.map_or(400, |weight| weight.0)));
    out.u8(u8::from(style.font_style == Some(compose_ui::FontStyle::Italic)));
    match style.font_family {
        None | Some(FontFamily::SansSerif) => out.u8(0),
        Some(FontFamily::Serif) => out.u8(1),
        Some(FontFamily::Monospace) => out.u8(2),
        Some(FontFamily::Cursive) => out.u8(3),
        Some(FontFamily::Named(name)) => {
            out.u8(4);
            out.u32(name.len() as u32);
            out.bytes.extend_from_slice(name.as_bytes());
        }
    }
    out.f32(if style.letter_spacing.is_specified() {
        density.sp_to_px(style.letter_spacing)
    } else {
        0.0
    });
    let decoration = style.text_decoration.unwrap_or_default();
    out.u8(u8::from(decoration.underline) | (u8::from(decoration.line_through) << 1));
    out.f32(if style.line_height.is_specified() {
        density.sp_to_px(style.line_height)
    } else {
        0.0
    });
    out.u32(if background.is_specified() { background.argb() } else { 0 });
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::IntSize;

    /// A fake backend: 10 px per UTF-16 unit, 20 px lines, no wrapping.
    #[derive(Default)]
    struct Fake {
        live: std::collections::HashSet<u32>,
        requests: Vec<Vec<u8>>,
    }

    impl TextBackend for Fake {
        fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> ParagraphMetrics {
            self.live.insert(handle);
            self.requests.push(request.to_vec());
            let units = text.encode_utf16().count() as i32;
            ParagraphMetrics {
                size: IntSize::new(units * 10, 20),
                first_baseline: 16.0,
                last_baseline: 16.0,
                line_count: 1,
            }
        }
        fn hit_test(&self, _handle: u32, position: Offset) -> usize {
            (position.x / 10.0).round() as usize
        }
        fn cursor(&self, _handle: u32, offset: usize) -> Rect {
            Rect::new(offset as f32 * 10.0, 0.0, offset as f32 * 10.0, 20.0)
        }
        fn release(&mut self, handle: u32) {
            self.live.remove(&handle);
        }
    }

    #[test]
    fn offsets_convert_between_bytes_and_utf16() {
        let text = "aé😀b";
        assert_eq!(utf16_index(text, 0), 0);
        assert_eq!(utf16_index(text, 1), 1);
        // é is 2 bytes, 1 unit; 😀 is 4 bytes, 2 units.
        assert_eq!(utf16_index(text, 3), 2);
        assert_eq!(utf16_index(text, 7), 4);
        assert_eq!(byte_offset(text, 2), 3);
        assert_eq!(byte_offset(text, 4), 7);
        assert_eq!(byte_offset(text, 9), text.len());
    }

    #[test]
    fn paragraphs_are_laid_out_once_and_released() {
        let mut measurer = AndroidTextMeasurer::new(Fake::default());
        let density = Density::new(2.0);
        let handle = measurer.layout(
            "héllo",
            &TextStyle::DEFAULT,
            TextLayoutOptions::default(),
            Constraints::new(0, 300, 0, 300),
            &density,
        );
        assert_eq!(measurer.metrics(handle).size, IntSize::new(50, 20));
        assert_eq!(measurer.text(handle), "héllo");
        // The cursor after "hé" (3 bytes) is 2 units in.
        assert_eq!(measurer.cursor_rect(handle, 3).left, 20.0);
        assert_eq!(measurer.hit_test(handle, Offset::new(21.0, 5.0)).offset, 3);
        // The default font size, 14 sp, at density 2.
        let request = &measurer.backend.requests[0];
        let font_px = f32::from_le_bytes(request[20..24].try_into().unwrap());
        assert_eq!(font_px, 28.0);
        measurer.release(handle);
        assert_eq!(measurer.live(), 0);
        assert!(measurer.backend.live.is_empty());
    }
}
