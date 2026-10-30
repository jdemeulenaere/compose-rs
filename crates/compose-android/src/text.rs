//! `AndroidTextMeasurer`: paragraphs laid out by Android's `StaticLayout` (docs/07-platforms.md,
//! "Text"), on the Kotlin side, which keeps each laid-out paragraph by handle to draw it.
//!
//! A layout is one crossing into Kotlin: the text, and a request encoding its style runs and
//! line options; the metrics come back with it. The text stays here too (byte offsets are
//! converted to UTF-16 ones for Kotlin), so what the layout needs again (the text, the
//! metrics, its direction) costs no crossing. Laying a paragraph's text out again at other
//! constraints (`relayout`) sends only the widths: Kotlin keeps what it measured of the text, as
//! Compose's `ParagraphIntrinsics` (D550).

use compose_ui::{
    AnnotatedString, Color, Constraints, Density, FontFamily, LayoutDirection, Offset, Rect,
    TextAlign, TextOverflow, TextStyle,
    platform::{
        NoFontLoader, ParagraphHandle, ParagraphMetrics, PlatformFontLoader, PlatformFonts,
        ResolvedFonts, TextLayoutOptions, TextMeasurer, TextPosition,
    },
};
use std::collections::HashMap;
use std::rc::Rc;

use crate::serialize::Writer;

/// The Kotlin side of text: `TextMeasurer.kt`, through JNI (or a fake, in tests).
pub trait TextBackend: 'static {
    /// Lays out paragraph `handle` (replacing one of the same handle): `text` with `request`
    /// (see `encode_request`), returning its metrics.
    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut;
    /// Lays out paragraph `previous`'s text again as paragraph `handle`, in the same style and
    /// options, between `min_width` and `max_width` (`i32::MAX` unbounded), and releases
    /// `previous`. `None` (the default: a backend keeping nothing of a text) leaves `previous`
    /// as it was.
    fn relayout(
        &mut self,
        _previous: u32,
        _handle: u32,
        _min_width: i32,
        _max_width: i32,
    ) -> Option<LaidOut> {
        None
    }
    /// Lays out paragraph `previous`'s text again as paragraph `handle`, as `relayout` does,
    /// for its intrinsic sizes: `previous` is kept, and `handle` isn't recorded ahead. `None`
    /// (the default) when the backend kept nothing of `previous`.
    fn probe_layout(
        &mut self,
        _previous: u32,
        _handle: u32,
        _min_width: i32,
        _max_width: i32,
    ) -> Option<LaidOut> {
        None
    }
    /// Gives paragraph `previous`'s layout, as paragraph `handle`, the paint of `color` (ARGB)
    /// and `decoration` (1 underline, 2 line through), and releases `previous`. False (the
    /// default: a backend keeping nothing) leaves `previous` as it was.
    fn restyle(&mut self, _previous: u32, _handle: u32, _color: u32, _decoration: u8) -> bool {
        false
    }
    /// Gives paragraph `previous`'s layout, as paragraph `handle` too, the paint of `color` (ARGB)
    /// and `decoration`, as `restyle`, `previous` staying as it is. False (the default: a backend
    /// keeping nothing) adds nothing.
    fn painted(&mut self, _previous: u32, _handle: u32, _color: u32, _decoration: u8) -> bool {
        false
    }
    /// Paragraph `handle`'s width with every soft line break taken (its widest segment between
    /// line break opportunities), as Compose's `minIntrinsicWidth`. `None` (the default) when the
    /// backend doesn't know it.
    fn min_intrinsic_width(&self, _handle: u32) -> Option<f32> {
        None
    }
    /// The UTF-16 offset nearest to `position`.
    fn hit_test(&self, handle: u32, position: Offset) -> usize;
    /// The cursor before UTF-16 offset `offset`: its top, bottom and x.
    fn cursor(&self, handle: u32, offset: usize) -> Rect;
    /// Paragraph `handle`'s layout for the `Paragraph` queries, in one crossing, as
    /// `TextMeasurer.kt`'s `paragraphLayout` writes it (read by [`paragraph_from_layout`]).
    /// `None` (the default: a backend that can't) reads the queries from the cursor at each
    /// character instead.
    fn paragraph_layout(&self, _handle: u32) -> Option<Vec<f32>> {
        None
    }
    fn release(&mut self, handle: u32);
}

/// What a backend's layout gives back.
#[derive(Clone, Copy, Debug)]
pub struct LaidOut {
    pub metrics: ParagraphMetrics,
    /// The text had more lines than its max lines (cut or ellipsized).
    pub did_exceed_max_lines: bool,
    /// The text's own width (Compose's `maxIntrinsicWidth`), if measured.
    pub max_intrinsic_width: Option<f32>,
}

/// A typeface of `TextMeasurer.kt`, by its number there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AndroidTypeface(pub u32);

struct Entry {
    /// The fonts its styles resolved to.
    resolved_fonts: ResolvedFonts,
    text: String,
    laid_out: LaidOut,
    right_to_left: bool,
}

/// A `TextMeasurer` over a `TextBackend`.
pub struct AndroidTextMeasurer<B: TextBackend> {
    backend: B,
    /// By handle. A handle isn't reused: a frame's bytes naming the same paragraphs draw the
    /// same text, so the replayer can keep a group's recording by its bytes.
    paragraphs: HashMap<u32, Entry>,
    next: u32,
    request: Writer,
    fonts: Rc<PlatformFonts>,
}

impl<B: TextBackend> AndroidTextMeasurer<B> {
    /// With no fonts of its own: every family is the default's. For tests.
    pub fn new(backend: B) -> Self {
        AndroidTextMeasurer::with_fonts(backend, Rc::new(NoFontLoader))
    }

    /// Loading fonts with `loader`.
    pub fn with_fonts(backend: B, loader: Rc<dyn PlatformFontLoader>) -> Self {
        AndroidTextMeasurer {
            backend,
            paragraphs: HashMap::new(),
            next: 0,
            request: Writer::default(),
            fonts: PlatformFonts::new(loader),
        }
    }

    fn entry(&self, handle: ParagraphHandle) -> &Entry {
        self.paragraphs.get(&(handle.0 as u32)).expect("compose-android: a live paragraph")
    }

    /// The number of live paragraphs: for leak checks.
    pub fn live(&self) -> usize {
        self.paragraphs.len()
    }

    /// A handle no live paragraph has (after 2³² paragraphs, past the live ones).
    fn next_handle(&mut self) -> u32 {
        while self.paragraphs.contains_key(&self.next) {
            self.next = self.next.wrapping_add(1);
        }
        let handle = self.next;
        self.next = self.next.wrapping_add(1);
        handle
    }

    /// Lays out `text` in `style` and `runs`; for intrinsic sizes (`probe`: Kotlin doesn't
    /// record it ahead) or to be drawn.
    #[allow(clippy::too_many_arguments)]
    fn layout_runs(
        &mut self,
        text: &str,
        style: &TextStyle,
        runs: &[(std::ops::Range<usize>, TextStyle, Color)],
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
        probe: bool,
    ) -> ParagraphHandle {
        let handle = self.next_handle();
        let right_to_left =
            first_strong_is_rtl(text).unwrap_or(options.direction == LayoutDirection::Rtl);
        let _layout = compose_ui::platform::trace::section(c"compose-rs: text layout");
        let mut resolved_fonts = ResolvedFonts::default();
        let mut fonts = RequestFonts { fonts: &self.fonts, resolved: &mut resolved_fonts };
        encode_request(
            &mut self.request,
            text,
            style,
            runs,
            options,
            constraints,
            density,
            right_to_left,
            probe,
            &mut fonts,
        );
        let laid_out = self.backend.layout(handle, text, &self.request.bytes);
        self.paragraphs.insert(
            handle,
            Entry { resolved_fonts, text: text.to_string(), laid_out, right_to_left },
        );
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
        self.layout_runs(text, style, &[], options, constraints, density, false)
    }

    fn layout_annotated(
        &mut self,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let runs = paint_free_runs(text, style);
        self.layout_runs(text.as_str(), style, &runs, options, constraints, density, false)
    }

    /// From what Kotlin kept of `measured` where it can (`probe_layout`), else anew; either way
    /// not recorded ahead (D556).
    fn layout_probe(
        &mut self,
        measured: Option<ParagraphHandle>,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let measured = measured.map(|paragraph| paragraph.0 as u32);
        if let Some(previous) = measured.filter(|previous| self.paragraphs.contains_key(previous)) {
            let handle = self.next_handle();
            let _layout = compose_ui::platform::trace::section(c"compose-rs: text layout");
            let (min_width, max_width) = width_bounds(constraints);
            if let Some(laid_out) =
                self.backend.probe_layout(previous, handle, min_width, max_width)
            {
                let entry = &self.paragraphs[&previous];
                let entry = Entry {
                    resolved_fonts: entry.resolved_fonts.clone(),
                    text: entry.text.clone(),
                    laid_out,
                    right_to_left: entry.right_to_left,
                };
                self.paragraphs.insert(handle, entry);
                return ParagraphHandle(handle as u64);
            }
        }
        let runs = paint_free_runs(text, style);
        self.layout_runs(text.as_str(), style, &runs, options, constraints, density, true)
    }

    fn fonts(&self) -> Option<Rc<PlatformFonts>> {
        Some(self.fonts.clone())
    }

    fn has_stale_resolved_fonts(&self, paragraph: ParagraphHandle) -> bool {
        self.entry(paragraph).resolved_fonts.is_stale()
    }

    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
        self.entry(paragraph).laid_out.metrics
    }

    fn did_exceed_max_lines(&self, paragraph: ParagraphHandle) -> bool {
        self.entry(paragraph).laid_out.did_exceed_max_lines
    }

    fn is_right_to_left(&self, paragraph: ParagraphHandle) -> bool {
        self.entry(paragraph).right_to_left
    }

    fn max_intrinsic_width(&self, paragraph: ParagraphHandle) -> Option<f32> {
        self.entry(paragraph).laid_out.max_intrinsic_width
    }

    fn min_intrinsic_width(&self, paragraph: ParagraphHandle) -> Option<f32> {
        self.backend.min_intrinsic_width(paragraph.0 as u32)
    }

    /// A new handle for the same layout in `style`'s paint: Kotlin draws the layout it has in
    /// the new color and decoration, and lays the text out in them from then on.
    fn restyle(
        &mut self,
        paragraph: ParagraphHandle,
        style: &TextStyle,
    ) -> Option<ParagraphHandle> {
        let previous = paragraph.0 as u32;
        if !self.paragraphs.contains_key(&previous) {
            return None;
        }
        let handle = self.next_handle();
        let (color, decoration) = paint_of(style);
        if !self.backend.restyle(previous, handle, color, decoration) {
            return None;
        }
        let entry = self.paragraphs.remove(&previous).expect("checked above");
        self.paragraphs.insert(handle, entry);
        Some(ParagraphHandle(handle as u64))
    }

    /// A new handle for the same layout in `style`'s paint, `paragraph` staying: Kotlin draws the
    /// one `Layout` in each paint, as Compose's `Paragraph.paint(color, …)`.
    fn painted(
        &mut self,
        paragraph: ParagraphHandle,
        style: &TextStyle,
    ) -> Option<ParagraphHandle> {
        let previous = paragraph.0 as u32;
        let entry = self.paragraphs.get(&previous)?;
        let entry = Entry {
            resolved_fonts: entry.resolved_fonts.clone(),
            text: entry.text.clone(),
            laid_out: entry.laid_out,
            right_to_left: entry.right_to_left,
        };
        let handle = self.next_handle();
        let (color, decoration) = paint_of(style);
        if !self.backend.painted(previous, handle, color, decoration) {
            return None;
        }
        self.paragraphs.insert(handle, entry);
        Some(ParagraphHandle(handle as u64))
    }

    /// A new handle (a frame's bytes naming a paragraph always draw the same layout), the text
    /// laid out again by Kotlin from what it kept of it.
    fn relayout(
        &mut self,
        paragraph: ParagraphHandle,
        constraints: Constraints,
    ) -> Option<ParagraphHandle> {
        let previous = paragraph.0 as u32;
        if !self.paragraphs.contains_key(&previous) {
            return None;
        }
        let handle = self.next_handle();
        let _layout = compose_ui::platform::trace::section(c"compose-rs: text layout");
        let (min_width, max_width) = width_bounds(constraints);
        let laid_out = self.backend.relayout(previous, handle, min_width, max_width)?;
        let entry = self.paragraphs.remove(&previous).expect("checked above");
        self.paragraphs.insert(handle, Entry { laid_out, ..entry });
        Some(ParagraphHandle(handle as u64))
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
        let width = self.entry(paragraph).laid_out.metrics.size.width as f32;
        let mut rects = vec![Rect::new(start.left, start.top, width, start.bottom)];
        if end.top > start.bottom {
            rects.push(Rect::new(0.0, start.bottom, width, end.top));
        }
        rects.push(Rect::new(0.0, end.top, end.left, end.bottom));
        rects
    }

    fn paragraph(&self, paragraph: ParagraphHandle) -> std::rc::Rc<dyn compose_ui::Paragraph> {
        let entry = self.entry(paragraph);
        let layout = self
            .backend
            .paragraph_layout(paragraph.0 as u32)
            .and_then(|data| paragraph_from_layout(&entry.text, &data))
            .unwrap_or_else(|| compose_ui::platform::sampled_paragraph(self, paragraph));
        std::rc::Rc::new(layout)
    }

    fn text(&self, paragraph: ParagraphHandle) -> &str {
        &self.entry(paragraph).text
    }

    fn release(&mut self, paragraph: ParagraphHandle) {
        let handle = paragraph.0 as u32;
        if self.paragraphs.remove(&handle).is_some() {
            self.backend.release(handle);
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

/// Floats of a paragraph's layout as `TextMeasurer.kt`'s `paragraphLayout` writes them: its line
/// count, UTF-16 length, width, height, min and max intrinsic widths and whether it exceeded its
/// max lines or ellipsized; per line its start, end, visible end, top, bottom, baseline, left,
/// right, width and whether its paragraph runs right to left; per UTF-16 offset, the text's end
/// included, its primary and secondary horizontal positions and whether the character there is
/// right to left.
pub const PARAGRAPH_HEADER: usize = 8;
pub const PARAGRAPH_LINE: usize = 10;
pub const PARAGRAPH_OFFSET: usize = 3;

/// The `ParagraphLayout` of `text` from `data` (see [`PARAGRAPH_HEADER`]), its UTF-16 offsets
/// as bytes; `None` when the data doesn't fit the text. Each character's box is Compose's from
/// the positions (`TextLayout.getBoundingBox`), and a line's end positions as Compose's
/// one-run lines' (its edges).
pub fn paragraph_from_layout(
    text: &str,
    data: &[f32],
) -> Option<compose_ui::platform::ParagraphLayout> {
    use compose_ui::Paragraph;
    use compose_ui::platform::{CharPosition, LineLayout, ParagraphLayout};
    let header = data.get(..PARAGRAPH_HEADER)?;
    let (lines, length) = (header[0] as usize, header[1] as usize);
    let units: usize = text.chars().map(char::len_utf16).sum();
    let offsets_at = PARAGRAPH_HEADER + lines * PARAGRAPH_LINE;
    if length != units || data.len() != offsets_at + (length + 1) * PARAGRAPH_OFFSET {
        return None;
    }
    let offset_data = |index: usize| &data[offsets_at + index * PARAGRAPH_OFFSET..][..3];
    // Each character boundary's UTF-16 index.
    let mut boundaries: Vec<(usize, usize)> = Vec::with_capacity(text.len() + 1);
    let mut unit = 0;
    for (offset, c) in text.char_indices() {
        boundaries.push((offset, unit));
        unit += c.len_utf16();
    }
    boundaries.push((text.len(), unit));
    let mut positions: Vec<CharPosition> = boundaries
        .iter()
        .map(|&(offset, unit)| {
            let values = offset_data(unit);
            CharPosition {
                offset,
                primary: values[0],
                secondary: values[1],
                right_to_left: values[2] != 0.0 && offset < text.len(),
                left: values[0],
                right: values[0],
            }
        })
        .collect();
    let line_layouts = (0..lines)
        .map(|line| {
            let values = &data[PARAGRAPH_HEADER + line * PARAGRAPH_LINE..][..PARAGRAPH_LINE];
            let start = byte_offset(text, values[0] as usize);
            let rtl = values[9] != 0.0;
            let (left, right) = (values[6], values[7]);
            // Compose's upstream end of a one-run line: its end edge for the paragraph's
            // direction, and for the other one when the line's run is the other way.
            let end_primary = if rtl { left } else { right };
            let run_rtl = offset_data(values[0] as usize)[2] != 0.0;
            let end_secondary = if run_rtl == rtl {
                end_primary
            } else if rtl {
                right
            } else {
                left
            };
            LineLayout {
                start,
                end: byte_offset(text, values[1] as usize),
                visible_end: byte_offset(text, values[2] as usize),
                ellipsized: false,
                top: values[3],
                bottom: values[4],
                baseline: values[5],
                left,
                right,
                width: values[8],
                right_to_left: rtl,
                end_primary,
                end_secondary,
            }
        })
        .collect::<Vec<_>>();
    let exceeded = header[6] != 0.0;
    let ellipsized = header[7] != 0.0;
    let mut layout = ParagraphLayout {
        text: text.to_string(),
        width: header[2],
        height: header[3],
        min_intrinsic_width: header[4],
        max_intrinsic_width: header[5],
        did_exceed_max_lines: exceeded,
        lines: line_layouts,
        positions: Vec::new(),
    };
    if ellipsized && let Some(last) = layout.lines.last_mut() {
        last.ellipsized = true;
    }
    layout.positions = positions.clone();
    // The characters' boxes, for selections.
    for (index, position) in positions.iter_mut().enumerate() {
        if index + 1 < boundaries.len() && !layout.lines.is_empty() {
            let bounds = layout.get_bounding_box(position.offset);
            position.left = bounds.left.min(bounds.right);
            position.right = bounds.left.max(bounds.right);
        }
    }
    layout.positions = positions;
    Some(layout)
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

/// `text`'s runs, each with its own color and decoration only (none if its spans set none), the
/// paragraph's paint giving the rest, so a new paint (`restyle`) reaches the runs that don't set
/// them. None without spans.
fn paint_free_runs(
    text: &AnnotatedString,
    style: &TextStyle,
) -> Vec<(std::ops::Range<usize>, TextStyle, Color)> {
    if !text.has_span_styles() {
        return Vec::new();
    }
    text.runs(&TextStyle { color: Color::UNSPECIFIED, text_decoration: None, ..style.clone() })
}

/// The minimum and maximum width Kotlin lays a paragraph out between (`i32::MAX`: unbounded).
fn width_bounds(constraints: Constraints) -> (i32, i32) {
    let max_width = if constraints.has_bounded_width() { constraints.max_width } else { i32::MAX };
    (constraints.min_width, max_width)
}

/// The request `TextMeasurer.kt` reads: line options, then the base style, then the runs.
///
/// Options: flags (u8: 1 soft wrap, 2 for intrinsic sizes only), max lines (u32), ellipsis (u8), alignment (u8: 0 normal, 1
/// opposite, 2 center, 3 justify), right to left (u8), min and max width (i32, max
/// `i32::MAX` unbounded). A style: color (ARGB u32), font size (px f32), weight (u32), italic
/// (u8), family (u8: 0 sans serif, 1 serif, 2 monospace, 3 cursive, 4 named, then its name's
/// length and UTF-8 bytes, 5 a typeface, then its number), letter spacing (px f32), decoration (u8: 1 underline, 2 line
/// through), line height (px f32, 0 unset), background (ARGB u32, 0 none). Runs: their count
/// (u32), then each one's UTF-16 start and end (u32), what it takes from the paragraph's paint
/// (u8: 1 its color, 2 its decoration: the ones its spans don't set) and style.
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
    probe: bool,
    fonts: &mut RequestFonts<'_>,
) {
    out.bytes.clear();
    let ellipsis = options.overflow == TextOverflow::Ellipsis;
    // As Compose: an ellipsis without soft wrapping is one line.
    let max_lines = if ellipsis && !options.soft_wrap { 1 } else { options.max_lines };
    out.u8(u8::from(options.soft_wrap) | (u8::from(probe) << 1));
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
        Some(_) => 0,
    });
    out.u8(u8::from(right_to_left));
    let (min_width, max_width) = width_bounds(constraints);
    out.u32(min_width as u32);
    out.u32(max_width as u32);
    encode_style(out, style, Color::TRANSPARENT, density, fonts);
    out.u32(runs.len() as u32);
    for (range, run_style, background) in runs {
        out.u32(utf16_index(text, range.start) as u32);
        out.u32(utf16_index(text, range.end) as u32);
        let inherits = u8::from(!run_style.color.is_specified())
            | (u8::from(run_style.text_decoration.is_none()) << 1);
        out.u8(inherits);
        encode_style(out, run_style, *background, density, fonts);
    }
}

/// What of `style` only paints: its color (ARGB, black if unset) and decoration (1 underline,
/// 2 line through).
fn paint_of(style: &TextStyle) -> (u32, u8) {
    let color = compose_ui::platform::resolve_text_color(style);
    let decoration = style.text_decoration.unwrap_or_default();
    (color.argb(), u8::from(decoration.underline) | (u8::from(decoration.line_through) << 1))
}

/// What a request's styles resolve their fonts with.
struct RequestFonts<'a> {
    fonts: &'a Rc<PlatformFonts>,
    resolved: &'a mut ResolvedFonts,
}

fn encode_style(
    out: &mut Writer,
    style: &TextStyle,
    background: Color,
    density: &Density,
    fonts: &mut RequestFonts<'_>,
) {
    let (color, decoration) = paint_of(style);
    let font_px = compose_ui::platform::resolve_font_size_px(style, density);
    out.u32(color);
    out.f32(font_px);
    out.u32(u32::from(style.font_weight.map_or(400, |weight| weight.weight())));
    out.u8(u8::from(style.font_style == Some(compose_ui::FontStyle::Italic)));
    match &style.font_family {
        None | Some(FontFamily::SansSerif | FontFamily::Default) => out.u8(0),
        Some(FontFamily::FontList(_) | FontFamily::Loaded(_)) => {
            let resolved = fonts.resolved.resolve(fonts.fonts, style);
            match resolved.as_ref().and_then(|typeface| typeface.downcast_ref::<AndroidTypeface>())
            {
                Some(typeface) => {
                    out.u8(5);
                    out.u32(typeface.0);
                }
                // Another stack's typeface, or none: the default.
                None => out.u8(0),
            }
        }
        Some(FontFamily::Serif) => out.u8(1),
        Some(FontFamily::Monospace) => out.u8(2),
        Some(FontFamily::Cursive) => out.u8(3),
        Some(FontFamily::Named(name)) => {
            out.u8(4);
            out.u32(name.len() as u32);
            out.bytes.extend_from_slice(name.as_bytes());
        }
    }
    let px = |unit| compose_ui::platform::resolve_text_unit_px(unit, font_px, density);
    out.f32(px(style.letter_spacing).unwrap_or(0.0));
    out.u8(decoration);
    out.f32(px(style.line_height).unwrap_or(0.0));
    // How the line height is spread, as Compose's `LineHeightStyleSpan`: the alignment's top
    // ratio (-1 proportional) and the trim (1 the first line's top, 2 the last line's bottom).
    // A style without one takes `LineHeightStyle.Default`, as Compose's `createCharSequence`
    // (its plain `LineHeightSpan` is for `includeFontPadding`, which compose-rs doesn't have).
    let line_height_style = style.line_height_style.unwrap_or(compose_ui::LineHeightStyle::DEFAULT);
    out.f32(line_height_style.alignment.top_ratio);
    let trim = line_height_style.trim;
    out.u8(u8::from(trim.trims_first_line_top()) | (u8::from(trim.trims_last_line_bottom()) << 1));
    out.u32(if background.is_specified() { background.argb() } else { 0 });
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::IntSize;

    /// A fake backend: 10 px per UTF-16 unit, 20 px lines, no wrapping; with `layouts`, it
    /// gives its paragraphs' layouts as `TextMeasurer.kt` does (one line).
    #[derive(Default)]
    struct Fake {
        live: std::collections::HashSet<u32>,
        requests: Vec<Vec<u8>>,
        layouts: bool,
        texts: std::collections::HashMap<u32, String>,
    }

    impl TextBackend for Fake {
        fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut {
            self.live.insert(handle);
            self.requests.push(request.to_vec());
            self.texts.insert(handle, text.to_string());
            let units = text.encode_utf16().count() as i32;
            LaidOut {
                metrics: ParagraphMetrics {
                    size: IntSize::new(units * 10, 20),
                    first_baseline: 16.0,
                    last_baseline: 16.0,
                    line_count: 1,
                },
                // A new line is one too many.
                did_exceed_max_lines: text.contains('\n'),
                max_intrinsic_width: Some(units as f32 * 10.0),
            }
        }
        fn hit_test(&self, _handle: u32, position: Offset) -> usize {
            (position.x / 10.0).round() as usize
        }
        fn cursor(&self, _handle: u32, offset: usize) -> Rect {
            Rect::new(offset as f32 * 10.0, 0.0, offset as f32 * 10.0, 20.0)
        }
        fn paragraph_layout(&self, handle: u32) -> Option<Vec<f32>> {
            if !self.layouts {
                return None;
            }
            let units = self.texts[&handle].encode_utf16().count();
            let width = units as f32 * 10.0;
            let mut data = vec![1.0, units as f32, width, 20.0, width, width, 0.0, 0.0];
            data.extend([0.0, units as f32, units as f32, 0.0, 20.0, 16.0, 0.0, width, width, 0.0]);
            for unit in 0..=units {
                data.extend([unit as f32 * 10.0, unit as f32 * 10.0, 0.0]);
            }
            Some(data)
        }
        fn release(&mut self, handle: u32) {
            self.live.remove(&handle);
        }
    }

    #[test]
    fn whether_lines_were_cut_is_the_backends() {
        let mut measurer = AndroidTextMeasurer::new(Fake::default());
        let density = Density::new(1.0);
        let options = TextLayoutOptions { max_lines: 1, ..TextLayoutOptions::default() };
        let constraints = Constraints::new(0, 300, 0, 300);
        let one = measurer.layout("one", &TextStyle::DEFAULT, options, constraints, &density);
        let two = measurer.layout("one\ntwo", &TextStyle::DEFAULT, options, constraints, &density);
        assert!(!measurer.did_exceed_max_lines(one));
        assert!(measurer.did_exceed_max_lines(two));
    }

    #[test]
    fn paragraph_queries_come_from_the_backends_layout_in_bytes() {
        let text = "a\u{e9}\u{1F600}b";
        for layouts in [true, false] {
            let mut measurer = AndroidTextMeasurer::new(Fake { layouts, ..Fake::default() });
            let handle = measurer.layout(
                text,
                &TextStyle::DEFAULT,
                TextLayoutOptions::default(),
                Constraints::unbounded(),
                &Density::new(1.0),
            );
            // Without the backend's layout, the queries come from its cursors.
            let paragraph = measurer.paragraph(handle);
            measurer.release(handle);
            assert_eq!(paragraph.line_count(), 1, "{layouts}");
            assert_eq!(paragraph.get_line_end(0, false), text.len(), "{layouts}");
            assert_eq!(paragraph.get_line_end(0, true), text.len(), "{layouts}");
            // The emoji: bytes 3 to 7, UTF-16 units 2 to 4.
            assert_eq!(paragraph.get_horizontal_position(3, true), 20.0, "{layouts}");
            assert_eq!(paragraph.get_horizontal_position(7, true), 40.0, "{layouts}");
            assert_eq!(paragraph.get_bounding_box(3), Rect::new(20.0, 0.0, 40.0, 20.0));
            assert_eq!(paragraph.get_cursor_rect(text.len()), Rect::new(50.0, 0.0, 50.0, 20.0));
            assert_eq!(paragraph.get_offset_for_position(Offset::new(29.0, 10.0)), 3);
            assert_eq!(paragraph.width(), 50.0, "{layouts}");
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

    #[test]
    fn runs_take_the_color_and_decoration_their_spans_dont_set_from_the_paint() {
        use compose_ui::{AnnotatedString, SpanStyle};
        let mut measurer = AndroidTextMeasurer::new(Fake::default());
        let text = AnnotatedString::build(|builder| {
            builder.with_style(
                SpanStyle { font_size: compose_ui::TextUnit::sp(20.0), ..SpanStyle::DEFAULT },
                |builder| {
                    builder.append("big");
                },
            );
            builder.with_style(SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT }, |builder| {
                builder.append("red");
            });
        });
        let style = TextStyle {
            color: Color::BLUE,
            text_decoration: Some(compose_ui::TextDecoration::UNDERLINE),
            ..TextStyle::DEFAULT
        };
        let options = TextLayoutOptions::default();
        let constraints = Constraints::new(0, 300, 0, 300);
        measurer.layout_annotated(&text, &style, options, constraints, &Density::new(1.0));
        let request = &measurer.backend.requests[0];
        // 16 bytes of options, then the 32-byte base style (its color first: the paint's).
        assert_eq!(u32::from_le_bytes(request[16..20].try_into().unwrap()), Color::BLUE.argb());
        let u32_at = |at: usize| u32::from_le_bytes(request[at..at + 4].try_into().unwrap());
        assert_eq!(u32_at(48), 2, "two runs");
        // Each run: its range, what it takes from the paint, its style (32 bytes).
        assert_eq!(
            (u32_at(52), u32_at(56), request[60]),
            (0, 3, 0b11),
            "the paint's color and decoration"
        );
        assert_eq!((u32_at(93), u32_at(97), request[101]), (3, 6, 0b10), "its own color");
        assert_eq!(u32_at(102), Color::RED.argb());
    }

    /// A style's line height style follows its line height (D571): the alignment's top ratio and
    /// the trim's two flags; a style without one sends `LineHeightStyle::DEFAULT` (proportional,
    /// both ends trimmed), as Compose's `createCharSequence` applies `LineHeightStyle.Default`
    /// (its plain `LineHeightSpan` is for `includeFontPadding` only).
    #[test]
    fn a_line_height_style_is_sent_after_the_line_height() {
        use compose_ui::{LineHeightAlignment, LineHeightStyle, LineHeightTrim};
        let mut measurer = AndroidTextMeasurer::new(Fake::default());
        let text = AnnotatedString::new(compose_ui::SharedString::from("text"));
        let options = TextLayoutOptions::default();
        let constraints = Constraints::new(0, 300, 0, 300);
        let density = Density::new(2.0);
        let plain = TextStyle { line_height: compose_ui::TextUnit::sp(20.0), ..TextStyle::DEFAULT };
        let styles = [
            (LineHeightAlignment::CENTER, LineHeightTrim::None, 0b00),
            (LineHeightAlignment::PROPORTIONAL, LineHeightTrim::Both, 0b11),
            (LineHeightAlignment::TOP, LineHeightTrim::FirstLineTop, 0b01),
            (LineHeightAlignment::BOTTOM, LineHeightTrim::LastLineBottom, 0b10),
        ];
        measurer.layout_annotated(&text, &plain, options, constraints, &density);
        for (alignment, trim, _) in styles {
            let style = TextStyle {
                line_height_style: Some(LineHeightStyle::new(alignment, trim)),
                ..plain.clone()
            };
            measurer.layout_annotated(&text, &style, options, constraints, &density);
        }
        let requests = &measurer.backend.requests;
        let f32_at =
            |request: &[u8], at: usize| f32::from_le_bytes(request[at..at + 4].try_into().unwrap());
        // The base style's line height is its bytes 19 to 22 (16 bytes of options before).
        assert_eq!(f32_at(&requests[0], 16 + 19), 40.0, "the line height in pixels");
        assert_eq!(
            (f32_at(&requests[0], 16 + 23), requests[0][16 + 27]),
            (-1.0, 0b11),
            "no line height style: the default, proportional and trimmed at both ends"
        );
        for (request, (alignment, _, trim)) in requests[1..].iter().zip(styles) {
            assert_eq!(f32_at(request, 16 + 23), alignment.top_ratio);
            assert_eq!(request[16 + 27], trim);
        }
    }

    #[test]
    fn a_probe_without_kept_intrinsics_is_marked_for_measuring_only() {
        let mut measurer = AndroidTextMeasurer::new(Fake::default());
        let text = AnnotatedString::new(compose_ui::SharedString::from("probe"));
        let options = TextLayoutOptions::default();
        let constraints = Constraints::new(0, 300, 0, 300);
        let density = Density::new(1.0);
        let measured =
            measurer.layout_annotated(&text, &TextStyle::DEFAULT, options, constraints, &density);
        // The fake keeps nothing to lay out from: a request, flagged.
        measurer.layout_probe(
            Some(measured),
            &text,
            &TextStyle::DEFAULT,
            options,
            constraints,
            &density,
        );
        let requests = &measurer.backend.requests;
        assert_eq!(requests[0][0], 1, "soft wrap, to be drawn");
        assert_eq!(requests[1][0], 0b11, "soft wrap, for a probe only");
    }
}
