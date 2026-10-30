//! `FixedTextMeasurer`: text laid out with a fixed advance and fixed font metrics per character,
//! for tests that shouldn't depend on fonts (D1105). Its lines break at spaces as Android's
//! greedy breaker does (spaces hang past the width, a word longer than a line is split), its
//! paragraphs run in the style's direction or their first strong character's, with the
//! Unicode bidi algorithm reordering mixed runs, and its queries are a [`ParagraphLayout`]'s.

use std::ops::Range;
use std::rc::Rc;

use unicode_bidi::{BidiClass, Level, ParagraphBidiInfo, bidi_class};

use crate::annotated::AnnotatedString;
use crate::constraints::Constraints;
use crate::paragraph::{
    GlyphBox, LaidOutLine, LineLayout, Paragraph, ParagraphLayout, ParagraphSize,
};
use crate::text::{
    ParagraphHandle, ParagraphMetrics, TextLayoutOptions, TextMeasurer, TextPosition,
    resolve_font_size_px, resolve_text_unit_px,
};
use crate::text_attributes::{
    LineHeightMode, LineHeightStyle, TextAlign, TextDirection, TextOverflow,
};
use crate::text_style::TextStyle;
use crate::unit::{Density, IntSize, Offset, Rect};

/// The fixed measurer's font, in ems of the font size: every character's advance (a line feed's
/// is zero), and the ascent and descent that make a line's height.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FixedFontMetrics {
    pub advance: f32,
    pub ascent: f32,
    pub descent: f32,
}

impl FixedFontMetrics {
    /// compose-rs's own: half an em wide, lines 1.25 em high with the baseline at 1 em.
    pub const DEFAULT: FixedFontMetrics =
        FixedFontMetrics { advance: 0.5, ascent: 1.0, descent: 0.25 };
    /// Compose's test font (`sample_font`): one em wide, lines one em high with the baseline at
    /// 0.8 em. Compose's paragraph tests expect these.
    pub const SAMPLE_FONT: FixedFontMetrics =
        FixedFontMetrics { advance: 1.0, ascent: 0.8, descent: 0.2 };
}

impl Default for FixedFontMetrics {
    fn default() -> Self {
        FixedFontMetrics::DEFAULT
    }
}

/// A measurer with a fixed advance per character and fixed line metrics
/// ([`FixedFontMetrics::DEFAULT`] unless given others): deterministic layout for tests,
/// independent of fonts.
#[derive(Default)]
pub struct FixedTextMeasurer {
    metrics: FixedFontMetrics,
    paragraphs: Vec<Option<FixedParagraph>>,
    free: Vec<u64>,
    scratch: std::cell::RefCell<FixedScratch>,
}

/// Buffers a layout works in, kept so laying text out allocates only its result.
#[derive(Default)]
struct FixedScratch {
    glyphs: Vec<Glyph>,
    /// The hard paragraphs' glyph ranges.
    paragraphs: Vec<Range<usize>>,
    /// One hard paragraph's lines.
    breaks: Vec<Range<usize>>,
    /// The lines: their hard paragraph and glyph range.
    lines: Vec<(usize, Range<usize>)>,
}

/// A laid-out paragraph: its lines' vertical metrics, measured when it's laid out, and its
/// whole layout (each character's position), laid out again from its input when a query
/// needs it: measuring a text costs no more than its lines.
struct FixedParagraph {
    /// The size and the lines, without their horizontal positions.
    measured: ParagraphLayout,
    input: FixedInput,
    full: std::cell::OnceCell<Rc<ParagraphLayout>>,
}

/// What a paragraph was laid out from.
struct FixedInput {
    text: AnnotatedString,
    style: LayoutStyle,
    options: TextLayoutOptions,
    constraints: Constraints,
    density: Density,
}

/// The attributes of a style the fixed measurer lays text out by: kept with a paragraph to lay
/// it out wholly later, without cloning the whole style.
#[derive(Clone, Copy)]
struct LayoutStyle {
    font_size: crate::unit::TextUnit,
    letter_spacing: crate::unit::TextUnit,
    text_geometric_transform: Option<crate::text_attributes::TextGeometricTransform>,
    line_height: crate::unit::TextUnit,
    line_height_style: Option<LineHeightStyle>,
    text_align: Option<TextAlign>,
    text_direction: Option<TextDirection>,
    text_indent: Option<crate::text_attributes::TextIndent>,
}

impl LayoutStyle {
    fn of(style: &TextStyle) -> LayoutStyle {
        LayoutStyle {
            font_size: style.font_size,
            letter_spacing: style.letter_spacing,
            text_geometric_transform: style.text_geometric_transform,
            line_height: style.line_height,
            line_height_style: style.line_height_style,
            text_align: style.text_align,
            text_direction: style.text_direction,
            text_indent: style.text_indent,
        }
    }

    /// A style with these attributes: what the layout reads of the original.
    fn to_style(self) -> TextStyle {
        TextStyle {
            font_size: self.font_size,
            letter_spacing: self.letter_spacing,
            text_geometric_transform: self.text_geometric_transform,
            line_height: self.line_height,
            line_height_style: self.line_height_style,
            text_align: self.text_align,
            text_direction: self.text_direction,
            text_indent: self.text_indent,
            ..TextStyle::DEFAULT
        }
    }
}

impl FixedTextMeasurer {
    pub fn new() -> FixedTextMeasurer {
        FixedTextMeasurer::default()
    }

    /// A measurer with the font `metrics`: [`FixedFontMetrics::SAMPLE_FONT`] for Compose's
    /// paragraph tests.
    pub fn with_metrics(metrics: FixedFontMetrics) -> FixedTextMeasurer {
        FixedTextMeasurer { metrics, ..FixedTextMeasurer::default() }
    }

    /// The number of live paragraphs: for leak tests.
    pub fn live(&self) -> usize {
        self.paragraphs.iter().filter(|paragraph| paragraph.is_some()).count()
    }

    fn entry(&self, handle: ParagraphHandle) -> &FixedParagraph {
        self.paragraphs
            .get(handle.0 as usize)
            .and_then(Option::as_ref)
            .expect("compose-ui: a released or unknown paragraph")
    }

    /// The paragraph's size and lines' vertical metrics.
    fn measured(&self, handle: ParagraphHandle) -> &ParagraphLayout {
        &self.entry(handle).measured
    }

    /// The paragraph's whole layout, laid out on first use.
    fn layout_of(&self, handle: ParagraphHandle) -> &Rc<ParagraphLayout> {
        let entry = self.entry(handle);
        entry.full.get_or_init(|| {
            let input = &entry.input;
            Rc::new(self.lay_out(
                input.text.as_str(),
                Some(&input.text),
                &input.style.to_style(),
                input.options,
                input.constraints,
                &input.density,
                true,
            ))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn measure_and_store(
        &mut self,
        text: AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        let measured =
            self.lay_out(text.as_str(), Some(&text), style, options, constraints, density, false);
        let input = FixedInput {
            text,
            style: LayoutStyle::of(style),
            options,
            constraints,
            density: *density,
        };
        self.store(FixedParagraph { measured, input, full: std::cell::OnceCell::new() })
    }

    fn store(&mut self, layout: FixedParagraph) -> ParagraphHandle {
        match self.free.pop() {
            Some(index) => {
                self.paragraphs[index as usize] = Some(layout);
                ParagraphHandle(index)
            }
            None => {
                self.paragraphs.push(Some(layout));
                ParagraphHandle(self.paragraphs.len() as u64 - 1)
            }
        }
    }
}

/// A character as laid out: its advance and its font's ascent and descent, in pixels.
#[derive(Clone, Copy)]
struct Glyph {
    offset: usize,
    character: char,
    advance: f32,
    ascent: f32,
    descent: f32,
}

/// Whether a paragraph runs right to left: its direction, else its first strong character's,
/// else the fallback, as Android's text direction heuristics.
fn paragraph_is_rtl(text: &str, direction: TextDirection) -> bool {
    let first_strong = || {
        text.chars().find_map(|c| match bidi_class(c) {
            BidiClass::L => Some(false),
            BidiClass::R | BidiClass::AL => Some(true),
            _ => None,
        })
    };
    match direction {
        TextDirection::Ltr => false,
        TextDirection::Rtl => true,
        TextDirection::ContentOrRtl => first_strong().unwrap_or(true),
        TextDirection::Content | TextDirection::ContentOrLtr => first_strong().unwrap_or(false),
    }
}

/// Whether `c` takes no space: a joiner, a combining mark, a bidi control.
fn is_zero_width(c: char) -> bool {
    !c.is_ascii()
        && matches!(
            bidi_class(c),
            BidiClass::BN
                | BidiClass::NSM
                | BidiClass::LRE
                | BidiClass::RLE
                | BidiClass::LRO
                | BidiClass::RLO
                | BidiClass::PDF
                | BidiClass::LRI
                | BidiClass::RLI
                | BidiClass::FSI
                | BidiClass::PDI
        )
}

/// Whether `text` has characters that can make a right-to-left run.
fn needs_bidi(text: &str) -> bool {
    text.chars().any(|c| {
        !c.is_ascii()
            && matches!(
                bidi_class(c),
                BidiClass::R
                    | BidiClass::AL
                    | BidiClass::AN
                    | BidiClass::RLE
                    | BidiClass::RLO
                    | BidiClass::RLI
                    | BidiClass::FSI
            )
    })
}

/// Lines of one hard paragraph (`glyphs`, without its line feed) broken greedily, its first line
/// at `first_width` and the others at `rest_width`: each line's glyph index range, its trailing
/// spaces included.
fn break_lines(glyphs: &[Glyph], first_width: f32, rest_width: f32, lines: &mut Vec<Range<usize>>) {
    let mut start = 0;
    while start < glyphs.len() {
        let width = if lines.is_empty() { first_width } else { rest_width };
        let mut used = 0.0;
        let mut end = start;
        // After the last space run seen: where the line can break.
        let mut opportunity = None;
        while end < glyphs.len() {
            let glyph = glyphs[end];
            if glyph.character == ' ' {
                // Spaces hang past the width.
                used += glyph.advance;
                end += 1;
                if end == glyphs.len() || glyphs[end].character != ' ' {
                    opportunity = Some(end);
                }
                continue;
            }
            // The width without trailing spaces, with this character.
            let visible = used + glyph.advance;
            if visible > width + 0.001 && end > start {
                break;
            }
            used = visible;
            end += 1;
        }
        if end < glyphs.len()
            && let Some(opportunity) = opportunity
        {
            end = opportunity;
        }
        lines.push(start..end);
        start = end;
    }
}

impl FixedTextMeasurer {
    /// Lays out `text` in `style` with each run's own size and letter spacing: wholly
    /// (`full`), or only its size, intrinsic widths and lines' vertical metrics (no positions,
    /// the lines left to right at 0).
    #[allow(clippy::too_many_arguments)]
    fn lay_out(
        &self,
        string: &str,
        spans: Option<&AnnotatedString>,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
        full: bool,
    ) -> ParagraphLayout {
        let metrics = self.metrics;
        let base_px = resolve_font_size_px(style, density);
        // The measurer's buffers, kept from layout to layout.
        let mut scratch = self.scratch.borrow_mut();
        let FixedScratch { glyphs, paragraphs, breaks, lines } = &mut *scratch;
        glyphs.clear();
        paragraphs.clear();
        lines.clear();
        // Only a span's size, letter spacing or scale changes its characters' metrics: other
        // spans (colors, weights) leave the text one run.
        let sized = |span: &crate::span_style::SpanStyle| {
            span.font_size.is_specified()
                || span.letter_spacing.is_specified()
                || span.text_geometric_transform.is_some()
        };
        let runs = match spans {
            Some(text) if text.span_style_ranges().any(|(span, _, _)| sized(span)) => {
                text.runs(style).into_iter().map(|(range, style, _)| (range, Some(style))).collect()
            }
            _ => vec![(0..string.len(), None)],
        };
        for (range, run_style) in &runs {
            let run_style = run_style.as_ref().unwrap_or(style);
            let font_px = resolve_font_size_px(run_style, density);
            // A horizontal scale widens the glyphs and an `em` letter spacing with them.
            let scale_x = run_style.text_geometric_transform.map_or(1.0, |t| t.scale_x);
            let spacing =
                resolve_text_unit_px(run_style.letter_spacing, font_px * scale_x, density)
                    .unwrap_or(0.0);
            for (index, character) in string[range.clone()].char_indices() {
                glyphs.push(Glyph {
                    offset: range.start + index,
                    character,
                    advance: if character == '\n' || is_zero_width(character) {
                        0.0
                    } else {
                        font_px * metrics.advance * scale_x + spacing
                    },
                    ascent: font_px * metrics.ascent,
                    descent: font_px * metrics.descent,
                });
            }
        }
        // The hard paragraphs: glyph ranges without their line feeds.
        let mut start = 0;
        for (index, glyph) in glyphs.iter().enumerate() {
            if glyph.character == '\n' {
                paragraphs.push(start..index);
                start = index + 1;
            }
        }
        paragraphs.push(start..glyphs.len());
        let width_of = |range: Range<usize>| glyphs[range].iter().map(|g| g.advance).sum::<f32>();
        let max_intrinsic =
            paragraphs.iter().map(|range| width_of(range.clone())).fold(0.0, f32::max);
        // The widest word with its trailing spaces.
        let mut min_intrinsic: f32 = 0.0;
        for paragraph in paragraphs.iter() {
            let mut word = 0.0;
            for (index, glyph) in glyphs[paragraph.clone()].iter().enumerate() {
                word += glyph.advance;
                let next = paragraph.start + index + 1;
                let breaks = glyph.character == ' '
                    && (next == paragraph.end || glyphs[next].character != ' ');
                if breaks {
                    min_intrinsic = min_intrinsic.max(word);
                    word = 0.0;
                }
            }
            min_intrinsic = min_intrinsic.max(word);
        }
        // Compose's `finalMaxWidth`: the constraints' width when fixed, else the text's own
        // width rounded up and kept in them; unbounded without wrapping.
        let width_matters = options.soft_wrap || options.overflow == TextOverflow::Ellipsis;
        let max_width = if width_matters && constraints.has_bounded_width() {
            constraints.max_width as f32
        } else {
            f32::INFINITY
        };
        let width = if constraints.min_width as f32 == max_width {
            max_width
        } else {
            max_intrinsic.ceil().max(constraints.min_width as f32).min(max_width)
        };
        // As Compose's `resolveTextDirection`: unspecified is the layout's, for all text.
        let direction = TextDirection::resolve(style.text_direction, options.direction);
        // The leading margins of a paragraph's first line and of its others.
        let indent = style.text_indent.map_or((0.0, 0.0), |indent| {
            let px = |unit| resolve_text_unit_px(unit, base_px, density).unwrap_or(0.0);
            (px(indent.first_line), px(indent.rest_line))
        });
        // Lines: (hard paragraph index, glyph range), a paragraph's line feed in its last line.
        for (index, paragraph) in paragraphs.iter().enumerate() {
            breaks.clear();
            break_lines(&glyphs[paragraph.clone()], width - indent.0, width - indent.1, breaks);
            let feed = usize::from(paragraph.end < glyphs.len());
            if breaks.is_empty() {
                lines.push((index, paragraph.start..paragraph.end + feed));
            }
            let count = breaks.len();
            for (line, range) in breaks.drain(..).enumerate() {
                let end = paragraph.start + range.end + if line + 1 == count { feed } else { 0 };
                lines.push((index, paragraph.start + range.start..end));
            }
        }
        // As Compose: an ellipsis without soft wrapping is one line.
        let max_lines = if options.overflow == TextOverflow::Ellipsis && !options.soft_wrap {
            1
        } else {
            options.max_lines.max(1)
        } as usize;
        let exceeded = lines.len() > max_lines;
        let ellipsized = exceeded && options.overflow == TextOverflow::Ellipsis;
        lines.truncate(max_lines);
        let line_height = resolve_text_unit_px(style.line_height, base_px, density);
        let base_ascent = base_px * metrics.ascent;
        let base_descent = base_px * metrics.descent;
        let align = style.text_align.unwrap_or(TextAlign::Start);
        let line_height_style = style.line_height_style.unwrap_or(LineHeightStyle::DEFAULT);
        let offset_of = |index: usize| glyphs.get(index).map_or(string.len(), |g| g.offset);
        let mut laid_out = Vec::with_capacity(lines.len());
        let mut top = 0.0;
        // The current hard paragraph's index, bidi levels (none when all left to right) and
        // direction.
        let mut bidi: Option<(usize, Option<ParagraphBidiInfo<'_>>, bool)> = None;
        let mut measured_lines = Vec::new();
        // An ellipsized last line's visible end, before its "…".
        let mut shown_visible_end = None;
        for (number, (paragraph_index, range)) in lines.iter().enumerate() {
            let paragraph = paragraphs[*paragraph_index].clone();
            // The line's glyphs without its line feed, and its visible ones.
            let content_end = range.end.min(paragraph.end).max(range.start);
            let visible_end = (range.start..content_end)
                .rev()
                .find(|&index| !crate::paragraph::is_line_end_space(glyphs[index].character))
                .map_or(range.start, |index| index + 1);
            // An ellipsized last line shows what fits before its "…" (one advance of the
            // style's font), as Android's ellipsis start (D1110).
            let shown_end = if ellipsized && number + 1 == lines.len() {
                let margin = if range.start == paragraph.start { indent.0 } else { indent.1 };
                let room = width - margin - base_px * metrics.advance;
                let (mut used, mut index) = (0.0, range.start);
                while index < visible_end && used + glyphs[index].advance <= room + 0.001 {
                    used += glyphs[index].advance;
                    index += 1;
                }
                shown_visible_end = Some(offset_of(index));
                index
            } else {
                visible_end
            };
            let line_glyphs = &glyphs[range.start..content_end.max(range.start)];
            let (ascent, descent) = if line_glyphs.is_empty() {
                (base_ascent, base_descent)
            } else {
                line_glyphs
                    .iter()
                    .fold((0.0f32, 0.0f32), |(a, d), g| (a.max(g.ascent), d.max(g.descent)))
            };
            let (height, baseline) = match line_height {
                Some(height) => {
                    // Compose's `LineHeightStyleSpan`: the extra height above and below the
                    // text by the alignment, the first line's top and the text's last line's
                    // bottom trimmed to the font's as the style says.
                    let font_height = ascent + descent;
                    let height = if line_height_style.mode == LineHeightMode::Minimum {
                        height.max(font_height)
                    } else {
                        height
                    };
                    let extra = height - font_height;
                    let ratio = line_height_style.alignment.top_ratio;
                    let above =
                        if ratio < 0.0 { extra * ascent / font_height } else { extra * ratio };
                    let mut top_part = ascent + above;
                    let mut bottom_part = descent + extra - above;
                    if number == 0 && line_height_style.trim.trims_first_line_top() {
                        top_part = ascent;
                    }
                    let last =
                        number + 1 == lines.len() && (ellipsized || range.end == glyphs.len());
                    if last && line_height_style.trim.trims_last_line_bottom() {
                        bottom_part = descent;
                    }
                    (top_part + bottom_part, top_part)
                }
                None => (ascent + descent, ascent),
            };
            if !full {
                measured_lines.push(LineLayout {
                    start: offset_of(range.start),
                    end: offset_of(range.end),
                    visible_end: offset_of(shown_end),
                    ellipsized: ellipsized && number + 1 == lines.len(),
                    top,
                    bottom: top + height,
                    baseline: top + baseline,
                    left: 0.0,
                    right: 0.0,
                    width: 0.0,
                    right_to_left: false,
                    end_primary: 0.0,
                    end_secondary: 0.0,
                });
                top += height;
                continue;
            }
            let paragraph_text = &string[offset_of(paragraph.start)..offset_of(paragraph.end)];
            if bidi.as_ref().is_none_or(|(index, _, _)| index != paragraph_index) {
                let rtl = paragraph_is_rtl(paragraph_text, direction);
                let level = if rtl { Level::rtl() } else { Level::ltr() };
                let info = (rtl || needs_bidi(paragraph_text))
                    .then(|| ParagraphBidiInfo::new(paragraph_text, Some(level)));
                bidi = Some((*paragraph_index, info, rtl));
            }
            let (_, info, rtl) = bidi.as_ref().expect("set above");
            let rtl = *rtl;
            // A justified line other than its paragraph's last spreads its interior spaces to
            // the width.
            let justified =
                align == TextAlign::Justify && range.end < paragraph.end && width.is_finite();
            let interior_spaces = if justified {
                glyphs[range.start..visible_end].iter().filter(|g| g.character == ' ').count()
            } else {
                0
            };
            // The line's region: the width without its leading margin (on the paragraph's
            // start side).
            let margin = if range.start == paragraph.start { indent.0 } else { indent.1 };
            let region = if rtl { (0.0, width - margin) } else { (margin, width) };
            let mut visible_width = width_of(range.start..visible_end);
            let mut total_width = width_of(range.start..content_end);
            let stretch = if interior_spaces > 0 {
                let target = region.1 - region.0;
                let stretch = (target - visible_width) / interior_spaces as f32;
                total_width += target - visible_width;
                visible_width = target;
                stretch
            } else {
                0.0
            };
            let advance_of = |index: usize| {
                let glyph = &glyphs[index];
                if stretch != 0.0 && glyph.character == ' ' && index < visible_end {
                    glyph.advance + stretch
                } else {
                    glyph.advance
                }
            };
            let resolved = match align {
                TextAlign::Left => TextAlign::Left,
                TextAlign::Right => TextAlign::Right,
                TextAlign::Center => TextAlign::Center,
                TextAlign::Start | TextAlign::Justify => {
                    if rtl {
                        TextAlign::Right
                    } else {
                        TextAlign::Left
                    }
                }
                TextAlign::End => {
                    if rtl {
                        TextAlign::Left
                    } else {
                        TextAlign::Right
                    }
                }
            };
            let (left, right) = match resolved {
                TextAlign::Right => (region.1 - visible_width, region.1),
                TextAlign::Center => {
                    let space = region.1 - region.0 - visible_width;
                    ((region.0 + space / 2.0).floor(), (region.1 - space / 2.0).ceil())
                }
                _ => (region.0, region.0 + visible_width),
            };
            // Each glyph's extent, by the bidi runs' visual order.
            let mut boxes = vec![(0.0f32, 0.0f32, rtl); line_glyphs.len()];
            if info.is_none() {
                // One left-to-right run.
                let mut x = left;
                for (index, glyph_box) in boxes.iter_mut().enumerate() {
                    let advance = advance_of(range.start + index);
                    *glyph_box = (x, x + advance, false);
                    x += advance;
                }
            } else if let Some(info) = info
                && !line_glyphs.is_empty()
            {
                let base = offset_of(paragraph.start);
                let line_bytes = offset_of(range.start) - base..offset_of(content_end) - base;
                let (levels, runs) = info.visual_runs(line_bytes);
                let mut x = if rtl { right - total_width } else { left };
                for run in runs {
                    let run_rtl = levels[run.start].is_rtl();
                    let first = line_glyphs.partition_point(|g| g.offset - base < run.start);
                    let last = line_glyphs.partition_point(|g| g.offset - base < run.end);
                    let mut place = |index: usize| {
                        let advance = advance_of(range.start + index);
                        boxes[index] = (x, x + advance, run_rtl);
                        x += advance;
                    };
                    if run_rtl {
                        (first..last).rev().for_each(&mut place);
                    } else {
                        (first..last).for_each(&mut place);
                    }
                }
            }
            let mut glyph_boxes = Vec::with_capacity(line_glyphs.len());
            for (glyph, (left, right, rtl)) in line_glyphs.iter().zip(boxes) {
                glyph_boxes.push(GlyphBox {
                    offset: glyph.offset,
                    left,
                    right,
                    right_to_left: rtl,
                });
            }
            laid_out.push(LaidOutLine {
                start: offset_of(range.start),
                end: offset_of(range.end),
                visible_end: offset_of(visible_end),
                top,
                bottom: top + height,
                baseline: top + baseline,
                left,
                right,
                width: total_width,
                right_to_left: rtl,
                glyphs: glyph_boxes,
            });
            top += height;
        }
        let size = ParagraphSize {
            width,
            height: top,
            min_intrinsic_width: min_intrinsic,
            max_intrinsic_width: max_intrinsic,
            did_exceed_max_lines: exceeded,
            ellipsized,
        };
        if !full {
            return ParagraphLayout {
                text: String::new(),
                width,
                height: top,
                min_intrinsic_width: min_intrinsic,
                max_intrinsic_width: max_intrinsic,
                did_exceed_max_lines: exceeded,
                lines: measured_lines,
                positions: Vec::new(),
            };
        }
        let mut layout = ParagraphLayout::from_lines(string.to_string(), size, laid_out);
        if let (Some(shown), Some(last)) = (shown_visible_end, layout.lines.last_mut()) {
            last.visible_end = shown;
        }
        layout
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
        self.measure_and_store(AnnotatedString::from(text), style, options, constraints, density)
    }

    fn layout_annotated(
        &mut self,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.measure_and_store(text.clone(), style, options, constraints, density)
    }

    fn metrics(&self, handle: ParagraphHandle) -> ParagraphMetrics {
        let layout = self.measured(handle);
        let width =
            if layout.width.is_finite() { layout.width } else { layout.max_intrinsic_width };
        ParagraphMetrics {
            size: IntSize::new(width.ceil() as i32, layout.height.ceil() as i32),
            first_baseline: layout.first_baseline(),
            last_baseline: layout.last_baseline(),
            line_count: layout.lines.len() as u32,
        }
    }

    fn did_exceed_max_lines(&self, handle: ParagraphHandle) -> bool {
        self.measured(handle).did_exceed_max_lines
    }

    fn min_intrinsic_width(&self, handle: ParagraphHandle) -> Option<f32> {
        Some(self.measured(handle).min_intrinsic_width)
    }

    fn max_intrinsic_width(&self, handle: ParagraphHandle) -> Option<f32> {
        Some(self.measured(handle).max_intrinsic_width)
    }

    /// The same layout under a new handle: its paragraphs have no paint.
    fn restyle(&mut self, handle: ParagraphHandle, _style: &TextStyle) -> Option<ParagraphHandle> {
        let layout = self.paragraphs.get_mut(handle.0 as usize)?.take()?;
        let restyled = self.store(layout);
        self.free.push(handle.0);
        Some(restyled)
    }

    /// A copy of the layout: its paragraphs have no paint.
    fn painted(&mut self, handle: ParagraphHandle, _style: &TextStyle) -> Option<ParagraphHandle> {
        let kept = self.paragraphs.get(handle.0 as usize)?.as_ref()?;
        let copy = FixedParagraph {
            measured: kept.measured.clone(),
            input: FixedInput {
                text: kept.input.text.clone(),
                style: kept.input.style,
                options: kept.input.options,
                constraints: kept.input.constraints,
                density: kept.input.density,
            },
            full: kept.full.clone(),
        };
        Some(self.store(copy))
    }

    fn hit_test(&self, handle: ParagraphHandle, position: Offset) -> TextPosition {
        let layout = self.layout_of(handle);
        if layout.lines.is_empty() {
            return TextPosition { offset: 0 };
        }
        // Above and below the lines: the first and last line.
        let line = layout.get_line_for_vertical_position(position.y.max(0.0));
        TextPosition { offset: layout.get_offset_for_horizontal(line, position.x) }
    }

    fn is_right_to_left(&self, handle: ParagraphHandle) -> bool {
        self.layout_of(handle).lines.first().is_some_and(|line| line.right_to_left)
    }

    fn selection_rects(&self, handle: ParagraphHandle, range: Range<usize>) -> Vec<Rect> {
        let layout = self.layout_of(handle);
        let mut rects = Vec::new();
        for line in &layout.lines {
            let (start, end) = (range.start.max(line.start), range.end.min(line.visible_end));
            if start >= end {
                continue;
            }
            let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
            for position in layout.positions_in(start..end) {
                left = left.min(position.left);
                right = right.max(position.right);
            }
            if left <= right {
                rects.push(Rect::new(left, line.top, right, line.bottom));
            }
        }
        rects
    }

    fn cursor_rect(&self, handle: ParagraphHandle, offset: usize) -> Rect {
        let layout = self.layout_of(handle);
        if layout.lines.is_empty() {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        layout.get_cursor_rect(offset.min(layout.text.len()))
    }

    fn paragraph(&self, handle: ParagraphHandle) -> Rc<dyn Paragraph> {
        self.layout_of(handle).clone()
    }

    fn text(&self, handle: ParagraphHandle) -> &str {
        self.entry(handle).input.text.as_str()
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
    use crate::unit::TextUnit;

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
        assert_eq!(measurer.cursor_rect(handle, 13), Rect::new(5.0, 12.5, 5.0, 25.0));
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
}
