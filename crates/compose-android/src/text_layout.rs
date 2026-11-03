//! Native Rust `BoringLayout` and `StaticLayout` text layout engine.
//!
//! Mirrors Android's `BoringLayout` (API 33+, fallback line spacing), `StaticLayout`,
//! `MeasuredParagraph`, `Layout`, `TextLine`, and Compose's `LineHeightStyleSpan` +
//! `lastLineMetrics` (`ui-text` 1.12.1) on top of [`TextShaper`]. All shaping, line-breaking,
//! BiDi reordering, and font metrics delegate through [`TextShaper`] directly to Android's native
//! `libhwui.so` (`Minikin` / `HarfBuzz`) and `libicu.so` (`UBiDi`) on device, or a deterministic
//! mock shaper in host integration tests.

#![forbid(unsafe_code)]

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_ui::{IntSize, Offset, Rect, platform::ParagraphMetrics};

use crate::text::LaidOut;

/// `Layout.DIR_LEFT_TO_RIGHT`.
pub const DIR_LEFT_TO_RIGHT: i32 = 1;
/// `Layout.DIR_RIGHT_TO_LEFT`.
pub const DIR_RIGHT_TO_LEFT: i32 = -1;

/// `Layout.RUN_LENGTH_MASK` (`0x03ff_ffff`).
pub const RUN_LENGTH_MASK: i32 = 0x03ff_ffff;
/// `Layout.RUN_LEVEL_SHIFT` (`26`).
pub const RUN_LEVEL_SHIFT: u32 = 26;
/// `Layout.RUN_LEVEL_MASK` (`0x3f`).
pub const RUN_LEVEL_MASK: i32 = 0x3f;
/// `Layout.RUN_RTL_FLAG` (`1 << 26`).
pub const RUN_RTL_FLAG: i32 = 1 << RUN_LEVEL_SHIFT;

/// `Paint.TEXT_RUN_FLAG_LEFT_EDGE`.
pub const TEXT_RUN_FLAG_LEFT_EDGE: i32 = 0x2000;
/// `Paint.TEXT_RUN_FLAG_RIGHT_EDGE`.
pub const TEXT_RUN_FLAG_RIGHT_EDGE: i32 = 0x4000;

/// `LineBreaker.TAB_MASK` (`0x2000_0000`).
const TAB_MASK: i32 = 0x2000_0000;
const TAB_INCREMENT: f32 = 20.0;
const CHAR_ELLIPSIS: u16 = 0x2026;
const CHAR_ELLIPSIS_FILLER: u16 = 0xFEFF;
const CHAR_ZWSP: u16 = 0x200B;

/// Integer font vertical metrics matching `android.graphics.Paint.FontMetricsInt`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontMetricsInt {
    /// Maximum distance above the baseline (negative).
    pub top: i32,
    /// Recommended distance above the baseline (negative).
    pub ascent: i32,
    /// Recommended distance below the baseline (positive).
    pub descent: i32,
    /// Maximum distance below the baseline (positive).
    pub bottom: i32,
    /// Recommended extra leading between lines.
    pub leading: i32,
}

/// Underline and strike-through stroke metrics read from a native `Paint`.
#[derive(Clone, Copy, Debug, Default)]
pub struct StrokeMetrics {
    /// Offset from baseline to top of underline stroke.
    pub underline_position: f32,
    /// Thickness of underline stroke in pixels.
    pub underline_thickness: f32,
    /// Offset from baseline to top of strike-through stroke.
    pub strikethru_position: f32,
    /// Thickness of strike-through stroke in pixels.
    pub strikethru_thickness: f32,
}

/// Key identifying a native `Paint` configuration (without underline/strikethrough flags, which
/// `TextLine` renders as explicit `drawRect` strokes).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaintStyleKey {
    /// ARGB8888 fill color.
    pub color: u32,
    /// Raw IEEE-754 bits of font size in pixels.
    pub font_px_bits: u32,
    /// Font weight (`1..=1000`, `400` normal).
    pub weight: i32,
    /// Whether italic posture is requested.
    pub italic: bool,
    /// Generic/named family selector (`0` sans, `1` serif, `2` mono, `3` cursive, `4` named).
    pub family: u8,
    /// Family name when `family == 4`, `None` otherwise.
    pub family_name: Option<Rc<str>>,
    /// Raw IEEE-754 bits of letter spacing in pixels.
    pub letter_spacing_px_bits: u32,
}

impl PaintStyleKey {
    /// Font size in pixels.
    #[inline]
    pub fn font_px(&self) -> f32 {
        f32::from_bits(self.font_px_bits)
    }

    /// Letter spacing in pixels.
    #[inline]
    pub fn letter_spacing_px(&self) -> f32 {
        f32::from_bits(self.letter_spacing_px_bits)
    }
}

/// Compose `LineHeightStyle` parameters encoded in layout requests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineHeightStyleSpec {
    /// Share of extra line height placed above text (`-1.0` for proportional).
    pub top_ratio: f32,
    /// Trim extra top space from the first line.
    pub trim_first_line_top: bool,
    /// Trim extra bottom space from the last line.
    pub trim_last_line_bottom: bool,
}

/// Decoded paragraph or span style from `text::encode_request`.
#[derive(Clone, Debug)]
pub struct DecodedStyle {
    /// Glyph paint attributes.
    pub paint: PaintStyleKey,
    /// Decoration bitmask (`1` underline, `2` line-through).
    pub decoration: u8,
    /// Target line height in pixels (`0.0` when unset).
    pub line_height_px: f32,
    /// Line height distribution and trim behavior.
    pub line_height_style: LineHeightStyleSpec,
    /// Background ARGB8888 (`0` when transparent/unset).
    pub background: u32,
}

/// Decoded style span over UTF-16 range `[start, end)`.
#[derive(Clone, Debug)]
pub struct DecodedRun {
    /// Inclusive UTF-16 start index.
    pub start: usize,
    /// Exclusive UTF-16 end index.
    pub end: usize,
    /// True when the span inherits the paragraph's paint color (`restyle` updates it).
    pub inherits_color: bool,
    /// True when the span inherits the paragraph's text decoration (`restyle` updates it).
    pub inherits_decoration: bool,
    /// Span style attributes.
    pub style: DecodedStyle,
}

/// Decoded binary layout request matching `text::encode_request`.
#[derive(Clone, Debug)]
pub struct DecodedRequest {
    /// Whether soft line wrapping is enabled.
    pub soft_wrap: bool,
    /// Whether this layout is an intrinsic probe that will never be drawn.
    pub probe: bool,
    /// Maximum visible lines (`>= 1`).
    pub max_lines: i32,
    /// Whether overflowing text should be ellipsized with `TruncateAt.END`.
    pub ellipsis: bool,
    /// Alignment code (`0` normal, `1` opposite, `2` center, `3` justify).
    pub align: u8,
    /// Base paragraph direction (`true` for RTL, `false` for LTR).
    pub rtl: bool,
    /// Minimum layout width in pixels.
    pub min_width: i32,
    /// Maximum layout width in pixels (`i32::MAX` when unbounded).
    pub max_width: i32,
    /// Maximum layout height in pixels (`i32::MAX` when unbounded).
    pub max_height: i32,
    /// Base paragraph style.
    pub base: DecodedStyle,
    /// Annotated style runs.
    pub runs: Vec<DecodedRun>,
}

/// BiDi analysis for a single `\n`-delimited paragraph.
#[derive(Clone, Debug)]
pub struct ParagraphBidi {
    /// `DIR_LEFT_TO_RIGHT` (`1`) or `DIR_RIGHT_TO_LEFT` (`-1`).
    pub paragraph_dir: i32,
    /// Per-UTF-16-unit embedding levels (`empty` when pure LTR).
    pub levels: Vec<u8>,
    /// Base request direction (`true` when `TextDirectionHeuristics.RTL`).
    pub rtl_request: bool,
}

impl ParagraphBidi {
    /// Pure left-to-right BiDi metadata requiring zero ICU calls.
    pub fn all_ltr() -> Self {
        Self { paragraph_dir: DIR_LEFT_TO_RIGHT, levels: Vec::new(), rtl_request: false }
    }
}

/// Unidirectional style run passed to `MeasuredText$Builder::nAddStyleRun`.
#[derive(Clone, Copy, Debug)]
pub struct StyleRunSpec {
    /// Native `Paint*` (or mock paint ID) for this run.
    pub paint_ptr: i64,
    /// Inclusive UTF-16 start offset within the paragraph buffer.
    pub start: usize,
    /// Exclusive UTF-16 end offset within the paragraph buffer.
    pub end: usize,
    /// Whether this run has odd BiDi level (RTL).
    pub is_rtl: bool,
}

/// Handle to a native `minikin::MeasuredText` (with optional RAII owner).
#[derive(Clone)]
pub struct MeasuredTextHandle {
    /// Native `minikin::MeasuredText*` (`0` in host mock tests).
    pub ptr: i64,
    /// Reference-counted owner freeing `ptr` when the last layout/draw plan drops.
    pub owner: Option<Rc<dyn Any>>,
}

/// Single line break produced by Minikin `LineBreaker`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LineBreakItem {
    /// Exclusive end offset in UTF-16 code units relative to the paragraph buffer.
    pub break_offset: usize,
    /// Measured line width in pixels.
    pub width: f32,
    /// Effective line ascent (negative, includes fallback font spacing).
    pub ascent: f32,
    /// Effective line descent (positive, includes fallback font spacing).
    pub descent: f32,
    /// Line flags (`TAB_MASK`, hyphen edits).
    pub flags: i32,
}

/// Filled rectangle emitted during complex paragraph drawing (background or decoration stroke).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawRectCmd {
    /// Left X coordinate in paragraph space.
    pub left: f32,
    /// Top Y coordinate in paragraph space.
    pub top: f32,
    /// Right X coordinate in paragraph space.
    pub right: f32,
    /// Bottom Y coordinate in paragraph space.
    pub bottom: f32,
    /// ARGB8888 fill color.
    pub color: u32,
}

/// Shaped directional style run emitted during complex paragraph drawing.
#[derive(Clone, Debug)]
pub struct DrawRunCmd {
    /// Optional span background rectangle drawn immediately before this run's glyphs.
    pub background: Option<DrawRectCmd>,
    /// Index into [`ComplexParagraphDraw::buffers`].
    pub buffer_index: u32,
    /// Native `minikin::MeasuredText*` (`0` when drawing an ellipsized line buffer).
    pub mt_ptr: i64,
    /// Start index in `buffers[buffer_index]`.
    pub start: i32,
    /// Character count (`end - start`).
    pub count: i32,
    /// Shaping context start index in `buffers[buffer_index]`.
    pub context_start: i32,
    /// Shaping context count (`context_end - context_start`).
    pub context_count: i32,
    /// Visual left X coordinate in paragraph space.
    pub x: f32,
    /// Baseline Y coordinate in paragraph space.
    pub baseline_y: f32,
    /// Whether the run is right-to-left (`minikin::Bidi::FORCE_RTL`).
    pub is_rtl: bool,
    /// Native `Paint*` configured for glyph drawing (with underline/strikethrough flags cleared).
    pub paint_ptr: i64,
    /// `TEXT_RUN_FLAG_LEFT_EDGE` / `TEXT_RUN_FLAG_RIGHT_EDGE` bits to apply while drawing.
    pub run_flags: i32,
    /// Extra word spacing in pixels (`0.0` unless justified).
    pub word_spacing_px: f32,
    /// Slice of [`ComplexParagraphDraw::decorations`] drawn immediately after this run's glyphs.
    pub decorations_range: std::ops::Range<u32>,
}

/// Packed `Layout.Directions` pairs (`[start, len | (level << 26), ...]`) in visual order.
#[derive(Clone, Debug)]
pub enum LineDirections {
    /// Single unidirectional run (`[0, packed]`) with zero heap allocation.
    Single([i32; 2]),
    /// Multi-run mixed BiDi visual order.
    Multi(Box<[i32]>),
}

impl LineDirections {
    /// Returns the packed `[start, len_and_level, ...]` slice.
    #[inline]
    pub fn as_slice(&self) -> &[i32] {
        match self {
            Self::Single(pair) => pair,
            Self::Multi(runs) => runs,
        }
    }
}

impl std::ops::Deref for LineDirections {
    type Target = [i32];

    #[inline]
    fn deref(&self) -> &[i32] {
        self.as_slice()
    }
}

/// Precomputed native draw command plan for non-trivial paragraphs.
#[derive(Clone, Default)]
pub struct ComplexParagraphDraw {
    /// Shared UTF-16 buffers referenced by `runs`.
    pub buffers: Vec<Rc<[u16]>>,
    /// RAII owners keeping `MeasuredText*` pointers alive while this draw plan is retained.
    pub owners: Vec<Rc<dyn Any>>,
    /// Ordered glyph run draw commands.
    pub runs: Vec<DrawRunCmd>,
    /// Underline and strike-through stroke rectangles referenced by `runs`.
    pub decorations: Vec<DrawRectCmd>,
}

/// Platform text shaping, BiDi, and line-breaking abstraction backed by Minikin + ICU on Android.
pub trait TextShaper {
    /// Resolves (or caches) a native `Paint*` for `style` (ignoring underline/strikethrough flags).
    fn resolve_paint(&mut self, style: &PaintStyleKey) -> i64;

    /// Returns an optional RAII owner keeping `paint_ptr` alive while retained by a draw plan/slot.
    fn paint_owner(&self, _paint_ptr: i64) -> Option<Rc<dyn Any>> {
        None
    }

    /// Returns vertical font metrics for `paint_ptr` (`Paint.getFontMetricsInt`).
    fn font_metrics_int(&mut self, paint_ptr: i64) -> FontMetricsInt;

    /// Returns underline and strike-through stroke metrics for `paint_ptr`.
    fn stroke_metrics(&mut self, paint_ptr: i64) -> StrokeMetrics;

    /// Expands `fmi` with fallback-font vertical extents for `chars[start..end)` in context
    /// `chars[ctx_start..ctx_end)` (`Paint::nGetFontMetricsIntForText`).
    #[allow(clippy::too_many_arguments)]
    fn font_metrics_int_for_text(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        ctx_start: usize,
        ctx_end: usize,
        is_rtl: bool,
    ) -> FontMetricsInt;

    /// Analyzes BiDi paragraph direction and per-code-unit levels (`MeasuredParagraph.buildForBidi`).
    fn analyze_bidi(&mut self, chars: &[u16], rtl: bool) -> ParagraphBidi;

    /// Computes packed `Layout.Directions` pairs (`[start, len | (level << 26), ...]`) in visual
    /// order for line `[start..end)` within the paragraph analyzed by `bidi`.
    fn line_directions(
        &mut self,
        bidi: &ParagraphBidi,
        chars: &[u16],
        start: usize,
        end: usize,
    ) -> LineDirections;

    /// Measures a single unidirectional LTR boring run `[start..end)` (`BoringLayout.isBoring` /
    /// `TextLine.metrics`), returning `(advance_width, expanded_font_metrics)`.
    fn measure_boring_run(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        run_flags: i32,
    ) -> (f32, FontMetricsInt) {
        let base_fmi = self.font_metrics_int(paint_ptr);
        if start >= end {
            return (0.0, base_fmi);
        }
        let ext = self.font_metrics_int_for_text(paint_ptr, chars, start, end, start, end, false);
        let asc = base_fmi.ascent.min(ext.ascent);
        let desc = base_fmi.descent.max(ext.descent);
        let fmi = FontMetricsInt {
            top: base_fmi.top.min(asc),
            ascent: asc,
            descent: desc,
            bottom: base_fmi.bottom.max(desc),
            leading: base_fmi.leading,
        };
        let w = self.get_run_advance(
            paint_ptr, chars, start, end, start, end, false, end, run_flags, 0.0, None,
        );
        (w, fmi)
    }

    /// Builds a native `MeasuredText` for `chars` across unidirectional `runs`.
    fn build_measured_text(&mut self, chars: &[u16], runs: &[StyleRunSpec]) -> MeasuredTextHandle;

    /// Total advance width of `[start..end)` inside `mt` (`MeasuredText::nGetWidth`).
    fn mt_width(&self, mt: &MeasuredTextHandle, start: usize, end: usize) -> f32;

    /// Single code-unit advance at `offset` inside `mt` (`MeasuredText::nGetCharWidthAt`).
    fn mt_char_width_at(&self, mt: &MeasuredTextHandle, offset: usize) -> f32;

    /// Computes line breaks for `chars` + `mt` at `width` (`LineBreaker::nComputeLineBreaks`).
    fn compute_line_breaks(
        &mut self,
        mt: &MeasuredTextHandle,
        chars: &[u16],
        width: f32,
        justified: bool,
        out: &mut Vec<LineBreakItem>,
    );

    /// Measures run advance up to `offset` (and optionally per-char advances into
    /// `out_advances[0..end - start]`) with optional `run_flags` and `word_spacing_px`.
    #[allow(clippy::too_many_arguments)]
    fn get_run_advance(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        ctx_start: usize,
        ctx_end: usize,
        is_rtl: bool,
        offset: usize,
        run_flags: i32,
        word_spacing_px: f32,
        out_advances: Option<&mut [f32]>,
    ) -> f32;

    /// Walks one grapheme cluster inside `chars[ctx_start..ctx_end)` from `offset`
    /// (`Paint::nGetTextRunCursor`; `after = true` -> `CURSOR_AFTER`, `false` -> `CURSOR_BEFORE`).
    #[allow(clippy::too_many_arguments)]
    fn get_text_run_cursor(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        ctx_start: usize,
        ctx_end: usize,
        is_rtl: bool,
        offset: usize,
        after: bool,
        word_spacing_px: f32,
    ) -> usize;
}

/// Decodes a binary layout request produced by `text::encode_request`.
pub fn decode_request(bytes: &[u8]) -> Option<DecodedRequest> {
    let mut r = SliceReader { bytes, pos: 0 };
    let flags = r.u8()?;
    // Placeholders (`flags & 4 != 0`) delegate to `JniText`.
    if flags & 4 != 0 {
        return None;
    }
    let soft_wrap = flags & 1 != 0;
    let probe = flags & 2 != 0;
    let max_lines = r.i32()?.max(1);
    let ellipsis_byte = r.u8()?;
    // `Start` (`2`) and `Middle` (`3`) ellipsis delegate to `JniText`.
    if ellipsis_byte > 1 {
        return None;
    }
    let ellipsis = ellipsis_byte == 1;
    let align = r.u8()?;
    let rtl = r.u8()? != 0;
    let min_width = r.i32()?;
    let max_width = r.i32()?;
    let base = read_style(&mut r)?;
    let run_count = r.u32()? as usize;
    let mut runs = Vec::with_capacity(run_count);
    for _ in 0..run_count {
        let start = r.u32()? as usize;
        let end = r.u32()? as usize;
        let inherits = r.u8()?;
        let style = read_style(&mut r)?;
        runs.push(DecodedRun {
            start,
            end,
            inherits_color: inherits & 1 != 0,
            inherits_decoration: inherits & 2 != 0,
            style,
        });
    }
    let max_height = if r.pos + 4 <= r.bytes.len() { r.i32()? } else { i32::MAX };
    Some(DecodedRequest {
        soft_wrap,
        probe,
        max_lines,
        ellipsis,
        align,
        rtl,
        min_width,
        max_width,
        max_height,
        base,
        runs,
    })
}

struct SliceReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> SliceReader<'a> {
    #[inline]
    fn u8(&mut self) -> Option<u8> {
        let &b = self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    #[inline]
    fn u32(&mut self) -> Option<u32> {
        let slice = self.bytes.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    #[inline]
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }

    #[inline]
    fn str_rc(&mut self, len: usize) -> Option<Rc<str>> {
        let slice = self.bytes.get(self.pos..self.pos + len)?;
        self.pos += len;
        std::str::from_utf8(slice).ok().map(Rc::from)
    }
}

fn read_style(r: &mut SliceReader<'_>) -> Option<DecodedStyle> {
    let color = r.u32()?;
    let font_px_bits = r.u32()?;
    let weight = r.i32()?;
    let italic = r.u8()? != 0;
    let family = r.u8()?;
    // Loaded custom typefaces (`family == 5`) delegate to `JniText`.
    if family > 4 {
        return None;
    }
    let family_name: Option<Rc<str>> = if family == 4 {
        let len = r.u32()? as usize;
        Some(r.str_rc(len)?)
    } else {
        None
    };
    let letter_spacing_px_bits = r.u32()?;
    let decoration = r.u8()?;
    // Brush / shadow / stroke paint extras (`decoration & !3 != 0`) delegate to `JniText`.
    if (decoration & !3) != 0 {
        return None;
    }
    let line_height_px = f32::from_bits(r.u32()?);
    let top_ratio = f32::from_bits(r.u32()?);
    let trim = r.u8()?;
    let background = r.u32()?;
    Some(DecodedStyle {
        paint: PaintStyleKey {
            color,
            font_px_bits,
            weight,
            italic,
            family,
            family_name,
            letter_spacing_px_bits,
        },
        decoration,
        line_height_px: if line_height_px > 0.0 { line_height_px } else { 0.0 },
        line_height_style: LineHeightStyleSpec {
            top_ratio,
            trim_first_line_top: trim & 1 != 0,
            trim_last_line_bottom: trim & 2 != 0,
        },
        background,
    })
}

/// Whether `c` could trigger RTL or surrogate shaping, matching `TextUtils.couldAffectRtl`.
#[inline]
pub fn could_affect_rtl(c: u16) -> bool {
    (0x0590..=0x08FF).contains(&c)
        || c == 0x200E
        || c == 0x200F
        || (0x202A..=0x202E).contains(&c)
        || (0x2066..=0x2069).contains(&c)
        || (0xD800..=0xDFFF).contains(&c)
        || (0xFB1D..=0xFDFF).contains(&c)
        || (0xFE70..=0xFEFE).contains(&c)
}

/// Whether `chars` needs ICU BiDi analysis (`MeasuredParagraph.doesNotNeedBidi` inverse).
#[inline]
pub fn needs_bidi(chars: &[u16], rtl: bool) -> bool {
    rtl || chars.iter().any(|&c| c >= 0x0590)
}

/// Compose's `maxIntrinsicWidth` adjustment: `+0.5` px for non-empty letter-spaced text.
#[inline]
fn intrinsic_width(width: f32, letter_spaced: bool) -> f32 {
    if letter_spaced && width != 0.0 { width + 0.5 } else { width }
}

/// Whitespace stripped from visual line ends (`TextLine.isLineEndSpace`).
#[inline]
pub fn is_line_end_space(ch: u16) -> bool {
    ch == 0x0020
        || ch == 0x0009
        || ch == 0x1680
        || ((0x2000..=0x200A).contains(&ch) && ch != 0x2007)
        || ch == 0x205F
        || ch == 0x3000
}

/// Compose's `LineHeightStyleSpan` (`Fixed` mode, `ui-text` 1.12.1).
struct StyledLineHeightSpan {
    line_height: f32,
    end_index: usize,
    style: LineHeightStyleSpec,
    first_ascent: i32,
    ascent: i32,
    descent: i32,
    last_descent: i32,
}

impl StyledLineHeightSpan {
    fn new(line_height: f32, end_index: usize, style: LineHeightStyleSpec) -> Self {
        Self {
            line_height,
            end_index,
            style,
            first_ascent: i32::MIN,
            ascent: 0,
            descent: 0,
            last_descent: 0,
        }
    }

    fn choose_height(&mut self, start: usize, end: usize, metrics: &mut FontMetricsInt) {
        if metrics.descent - metrics.ascent <= 0 {
            return;
        }
        let first = start == 0;
        let last = end == self.end_index;
        if first && last && self.style.trim_first_line_top && self.style.trim_last_line_bottom {
            return;
        }
        if self.first_ascent == i32::MIN {
            self.compute_target(metrics);
        }
        metrics.ascent = if first { self.first_ascent } else { self.ascent };
        metrics.descent = if last { self.last_descent } else { self.descent };
    }

    fn compute_target(&mut self, metrics: &FontMetricsInt) {
        let height = metrics.descent - metrics.ascent;
        let target = self.line_height.ceil() as i32;
        let difference = target - height;
        let top_ratio = if self.style.top_ratio == -1.0 {
            (metrics.ascent as f32).abs() / (height as f32)
        } else {
            self.style.top_ratio
        };
        let descent_diff = if difference <= 0 {
            (f64::from((difference as f32) * top_ratio)).ceil() as i32
        } else {
            (f64::from((difference as f32) * (1.0 - top_ratio))).ceil() as i32
        };
        self.descent = metrics.descent + descent_diff;
        self.ascent = self.descent - target;
        self.first_ascent =
            if self.style.trim_first_line_top { metrics.ascent } else { self.ascent };
        self.last_descent =
            if self.style.trim_last_line_bottom { metrics.descent } else { self.descent };
    }
}

const CHAR_INLINE_CAP: usize = 32;
const SPARE_POOL_CAP: usize = 64;

#[derive(Clone)]
enum CharBuffer {
    Inline { buf: [u16; CHAR_INLINE_CAP], len: u8 },
    Shared(Rc<[u16]>),
}

impl CharBuffer {
    fn from_str_utf16(text: &str) -> Self {
        if text.is_ascii() && text.len() <= CHAR_INLINE_CAP {
            let mut buf = [0u16; CHAR_INLINE_CAP];
            for (dst, b) in buf.iter_mut().zip(text.bytes()) {
                *dst = u16::from(b);
            }
            return Self::Inline { buf, len: text.len() as u8 };
        }
        let mut buf = [0u16; CHAR_INLINE_CAP];
        let mut len = 0usize;
        let mut iter = text.encode_utf16();
        while let Some(u) = iter.next() {
            if len < CHAR_INLINE_CAP {
                buf[len] = u;
                len += 1;
            } else {
                let mut vec = Vec::with_capacity(64);
                vec.extend_from_slice(&buf[..CHAR_INLINE_CAP]);
                vec.push(u);
                vec.extend(iter);
                return Self::Shared(vec.into());
            }
        }
        Self::Inline { buf, len: len as u8 }
    }

    fn to_rc_slice(&self) -> Rc<[u16]> {
        match self {
            Self::Inline { buf, len } => Rc::from(&buf[..*len as usize]),
            Self::Shared(rc) => rc.clone(),
        }
    }
}

impl std::ops::Deref for CharBuffer {
    type Target = [u16];

    #[inline]
    fn deref(&self) -> &[u16] {
        match self {
            Self::Inline { buf, len } => &buf[..*len as usize],
            Self::Shared(rc) => rc,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BoringMetrics {
    fmi: FontMetricsInt,
    width: i32,
    base_paint: i64,
}

struct PrecomputedPara {
    start: usize,
    end: usize,
    chars: Rc<[u16]>,
    bidi: ParagraphBidi,
    mt: MeasuredTextHandle,
    has_tabs: bool,
}

struct Intrinsics {
    chars: CharBuffer,
    base: DecodedStyle,
    runs: Vec<DecodedRun>,
    soft_wrap: bool,
    max_lines: i32,
    ellipsis: bool,
    align: u8,
    rtl: bool,
    boring: Option<BoringMetrics>,
    boring_width: f32,
    max_intrinsic_width: f32,
    paras: Vec<PrecomputedPara>,
    slot0: Option<Rc<ComputedLayout>>,
    slot1: Option<Rc<ComputedLayout>>,
}

#[derive(Clone, Debug)]
struct LayoutLine {
    start: usize,
    end: usize,
    top: i32,
    descent: i32,
    bottom: i32,
    dir: i32,
    directions: LineDirections,
    has_tab: bool,
    ellipsis_start: usize,
    ellipsis_count: usize,
    para_index: usize,
}

#[derive(Clone, Debug)]
enum LayoutLines {
    Single([LayoutLine; 1]),
    Multi(Vec<LayoutLine>),
}

impl std::ops::Deref for LayoutLines {
    type Target = [LayoutLine];

    #[inline]
    fn deref(&self) -> &[LayoutLine] {
        match self {
            Self::Single(line) => line,
            Self::Multi(lines) => lines,
        }
    }
}

impl std::ops::DerefMut for LayoutLines {
    #[inline]
    fn deref_mut(&mut self) -> &mut [LayoutLine] {
        match self {
            Self::Single(line) => line,
            Self::Multi(lines) => lines,
        }
    }
}

struct ComputedLayout {
    min_same_width: i32,
    max_same_width: i32,
    layout_max_height: i32,
    effective_max_lines: i32,
    laid_out: LaidOut,
    is_boring: bool,
    lines: LayoutLines,
    last_line_override: Option<FontMetricsInt>,
    /// Substituted display chars when any line has `ellipsis_count > 0`, else `None` (uses `chars`).
    ellipsized_chars: Option<Box<[u16]>>,
    /// Non-zero when single-line LTR boring plain text can draw via `canvas_draw_text_chars`.
    simple_paint_ptr: i64,
    simple_paint_owner: Option<Rc<dyn Any>>,
    simple_baseline: f32,
    /// Precomputed native draw plan when `simple_paint_ptr == 0` and `!chars.is_empty()`.
    draw_plan: Option<Rc<ComplexParagraphDraw>>,
}

struct LiveHandle {
    intrinsics: Rc<RefCell<Intrinsics>>,
    layout: Rc<ComputedLayout>,
    width: i32,
}

/// Layout result paired with native draw data for `SharedParagraphTable`.
pub struct NativeLayoutOutput {
    /// Metrics, `did_exceed_max_lines`, and `max_intrinsic_width`.
    pub laid_out: LaidOut,
    /// Minimum constraint width yielding the exact same line breaks/positions.
    pub min_same_width: i32,
    /// Maximum constraint width yielding the exact same line breaks/positions.
    pub max_same_width: i32,
    /// Non-zero native `Paint*` when eligible for single-call `canvas_draw_text_chars`.
    pub simple_paint_ptr: i64,
    /// Optional RAII owner keeping `simple_paint_ptr` alive while retained by a slot.
    pub simple_paint_owner: Option<Rc<dyn Any>>,
    /// Baseline Y for `simple_paint_ptr`.
    pub simple_baseline: f32,
    /// Precomputed draw commands when `simple_paint_ptr == 0` and the paragraph draws content.
    pub complex_draw: Option<Rc<ComplexParagraphDraw>>,
}

/// Pure-Rust `BoringLayout` + `StaticLayout` manager parameterized over [`TextShaper`].
pub struct NativeLayoutEngine<S: TextShaper> {
    shaper: RefCell<S>,
    handles: HashMap<u32, LiveHandle>,
    spare_intrinsics: Vec<Rc<RefCell<Intrinsics>>>,
    spare_layouts: Vec<Rc<ComputedLayout>>,
    breaks_scratch: Vec<LineBreakItem>,
    style_runs_scratch: Vec<StyleRunSpec>,
}

impl<S: TextShaper> NativeLayoutEngine<S> {
    /// Creates a new layout engine wrapping `shaper`.
    pub fn new(shaper: S) -> Self {
        Self {
            shaper: RefCell::new(shaper),
            handles: HashMap::with_capacity(256),
            spare_intrinsics: Vec::with_capacity(SPARE_POOL_CAP),
            spare_layouts: Vec::with_capacity(SPARE_POOL_CAP),
            breaks_scratch: Vec::with_capacity(16),
            style_runs_scratch: Vec::with_capacity(8),
        }
    }

    #[inline]
    fn alloc_intrinsics_rc(&mut self, intr: Intrinsics) -> Rc<RefCell<Intrinsics>> {
        while let Some(mut rc) = self.spare_intrinsics.pop() {
            if let Some(cell) = Rc::get_mut(&mut rc) {
                *cell.get_mut() = intr;
                return rc;
            }
        }
        Rc::new(RefCell::new(intr))
    }

    #[inline]
    fn alloc_layout_rc(&mut self, layout: ComputedLayout) -> Rc<ComputedLayout> {
        while let Some(mut rc) = self.spare_layouts.pop() {
            if let Some(slot) = Rc::get_mut(&mut rc) {
                *slot = layout;
                return rc;
            }
        }
        Rc::new(layout)
    }

    /// Borrows the underlying shaper mutably.
    pub fn with_shaper_mut<R>(&self, f: impl FnOnce(&mut S) -> R) -> R {
        f(&mut self.shaper.borrow_mut())
    }

    /// Decodes `request` and lays out `text` under `handle`.
    pub fn layout(
        &mut self,
        handle: u32,
        text: &str,
        request: &[u8],
    ) -> Option<NativeLayoutOutput> {
        let req = decode_request(request)?;
        let chars = CharBuffer::from_str_utf16(text);
        let intrinsics = self.build_intrinsics(chars, &req);
        let intr_rc = self.alloc_intrinsics_rc(intrinsics);
        Some(self.layout_from_intrinsics(
            handle,
            intr_rc,
            req.min_width,
            req.max_width,
            req.max_height,
            !req.probe,
        ))
    }

    /// Re-lays out `previous` at `[min_width, max_width]` and `max_height` as `handle`, releasing `previous`.
    pub fn relayout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<NativeLayoutOutput> {
        let prev = self.handles.remove(&previous)?;
        Some(self.layout_from_intrinsics(
            handle,
            prev.intrinsics,
            min_width,
            max_width,
            max_height,
            true,
        ))
    }

    /// Updates `handle`'s width in place when `[min_width, max_width]` falls within `[min_same, max_same]`.
    pub fn relayout_in_place(
        &mut self,
        handle: u32,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<LaidOut> {
        let live = self.handles.get_mut(&handle)?;
        let (width, height_matches) = {
            let intr = live.intrinsics.borrow();
            let w = target_layout_width(
                intr.soft_wrap,
                intr.ellipsis,
                intr.max_intrinsic_width,
                min_width,
                max_width,
            );
            let hm = !intr.ellipsis
                || intr.max_lines <= 1
                || live.layout.layout_max_height == max_height;
            (w, hm)
        };
        if !height_matches
            || width < live.layout.min_same_width
            || width > live.layout.max_same_width
        {
            return None;
        }
        live.width = width;
        let mut out = live.layout.laid_out;
        out.metrics.size.width = width;
        Some(out)
    }

    /// Probes layout of `previous` at `[min_width, max_width]` and `max_height` as `handle` without releasing `previous`
    /// and without building a `ComplexParagraphDraw`.
    pub fn probe_layout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<NativeLayoutOutput> {
        let intr_rc = self.handles.get(&previous)?.intrinsics.clone();
        Some(self.layout_from_intrinsics(handle, intr_rc, min_width, max_width, max_height, false))
    }

    /// Restyles `previous` as `handle` with a new base `(color, decoration)`, releasing `previous`.
    pub fn restyle(
        &mut self,
        previous: u32,
        handle: u32,
        color: u32,
        decoration: u8,
    ) -> Option<NativeLayoutOutput> {
        let prev = self.handles.remove(&previous)?;
        let width = prev.width;
        let mut new_base = prev.intrinsics.borrow().base.clone();
        new_base.paint.color = color;
        new_base.decoration = decoration;

        let new_boring = prev.intrinsics.borrow().boring.map(|mut bm| {
            if prev.intrinsics.borrow().runs.is_empty() {
                bm.base_paint = self.shaper.borrow_mut().resolve_paint(&new_base.paint);
            }
            bm
        });

        let new_intr = {
            let old = prev.intrinsics.borrow();
            Intrinsics {
                chars: old.chars.clone(),
                base: new_base,
                runs: old.runs.clone(),
                soft_wrap: old.soft_wrap,
                max_lines: old.max_lines,
                ellipsis: old.ellipsis,
                align: old.align,
                rtl: old.rtl,
                boring: new_boring,
                boring_width: old.boring_width,
                max_intrinsic_width: old.max_intrinsic_width,
                paras: old
                    .paras
                    .iter()
                    .map(|p| PrecomputedPara {
                        start: p.start,
                        end: p.end,
                        chars: p.chars.clone(),
                        bidi: p.bidi.clone(),
                        mt: p.mt.clone(),
                        has_tabs: p.has_tabs,
                    })
                    .collect(),
                slot0: None,
                slot1: None,
            }
        };

        let mut restyled = ComputedLayout {
            min_same_width: prev.layout.min_same_width,
            max_same_width: prev.layout.max_same_width,
            layout_max_height: prev.layout.layout_max_height,
            effective_max_lines: prev.layout.effective_max_lines,
            laid_out: prev.layout.laid_out,
            is_boring: prev.layout.is_boring,
            lines: prev.layout.lines.clone(),
            last_line_override: prev.layout.last_line_override,
            ellipsized_chars: prev.layout.ellipsized_chars.clone(),
            simple_paint_ptr: 0,
            simple_paint_owner: None,
            simple_baseline: 0.0,
            draw_plan: None,
        };
        restyled.laid_out.metrics.size.width = width;
        self.populate_draw_for_layout(&new_intr, &mut restyled, width);

        let rc_layout = self.alloc_layout_rc(restyled);
        let mut intr_with_slot = new_intr;
        intr_with_slot.slot0 = Some(rc_layout.clone());
        let intr_rc = self.alloc_intrinsics_rc(intr_with_slot);
        let out = output_from_layout(&rc_layout, width);
        self.handles.insert(handle, LiveHandle { intrinsics: intr_rc, layout: rc_layout, width });
        Some(out)
    }

    /// Releases `handle`.
    pub fn release(&mut self, handle: u32) {
        if let Some(mut live) = self.handles.remove(&handle) {
            if let Some(cell) = Rc::get_mut(&mut live.intrinsics) {
                let intr = cell.get_mut();
                intr.slot0 = None;
                intr.slot1 = None;
                intr.paras.clear();
                intr.runs.clear();
                if self.spare_intrinsics.len() < SPARE_POOL_CAP {
                    self.spare_intrinsics.push(live.intrinsics);
                }
            }
            if self.spare_layouts.len() < SPARE_POOL_CAP && Rc::strong_count(&live.layout) == 1 {
                self.spare_layouts.push(live.layout);
            }
        }
    }

    /// Returns the UTF-16 offset nearest to `position` (`Layout.getOffsetForHorizontal`).
    pub fn hit_test(&self, handle: u32, position: Offset) -> usize {
        let Some(live) = self.handles.get(&handle) else { return 0 };
        let intr = live.intrinsics.borrow();
        let layout = &live.layout;
        let line = line_for_vertical(&layout.lines, position.y as i32);
        let mut shaper = self.shaper.borrow_mut();
        offset_for_horizontal(&mut *shaper, &intr, layout, live.width, line, position.x)
    }

    /// Returns the cursor rectangle at UTF-16 `offset` (`TextMeasurer.cursor`).
    pub fn cursor(&self, handle: u32, offset: usize) -> Rect {
        let Some(live) = self.handles.get(&handle) else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        let intr = live.intrinsics.borrow();
        let layout = &live.layout;
        let at = offset.min(intr.chars.len());
        let line = line_for_offset(&layout.lines, at);
        let trailing = primary_is_trailing_previous(&layout.lines[line], at);
        let mut shaper = self.shaper.borrow_mut();
        let x = get_horizontal(&mut *shaper, &intr, layout, live.width, at, trailing, line);
        let visible_lines =
            (layout.lines.len() as i32).clamp(1, layout.effective_max_lines.max(1)) as usize;
        let clamped_line = line.min(visible_lines - 1);
        let top = layout.lines[clamped_line].top as f32;
        let bottom =
            line_bottom(&layout.lines, clamped_line, visible_lines, layout.last_line_override);
        Rect::new(x, top, x, bottom)
    }

    fn build_intrinsics(&mut self, chars: CharBuffer, req: &DecodedRequest) -> Intrinsics {
        let letter_spaced =
            (req.base.paint.font_px() > 0.0 && req.base.paint.letter_spacing_px() != 0.0)
                || req.runs.iter().any(|r| {
                    r.style.paint.font_px() > 0.0 && r.style.paint.letter_spacing_px() != 0.0
                });

        let can_be_boring = !req.rtl
            && !(chars.is_empty() && req.base.line_height_px > 0.0)
            && chars.iter().all(|&c| c != 0x0009 && c != 0x000A && !could_affect_rtl(c));

        if can_be_boring {
            let bm = self.measure_boring(&chars, &req.base, &req.runs);
            let bw = intrinsic_width(bm.width as f32, letter_spaced);
            return Intrinsics {
                chars,
                base: req.base.clone(),
                runs: req.runs.clone(),
                soft_wrap: req.soft_wrap,
                max_lines: req.max_lines,
                ellipsis: req.ellipsis,
                align: req.align,
                rtl: req.rtl,
                boring: Some(bm),
                boring_width: bw,
                max_intrinsic_width: bw,
                paras: Vec::new(),
                slot0: None,
                slot1: None,
            };
        }

        let paras = self.build_paragraphs(&chars, req);
        let desired = self.compute_desired_width(&paras, &req.base, &req.runs);
        let max_intr = intrinsic_width(desired.ceil(), letter_spaced);
        Intrinsics {
            chars,
            base: req.base.clone(),
            runs: req.runs.clone(),
            soft_wrap: req.soft_wrap,
            max_lines: req.max_lines,
            ellipsis: req.ellipsis,
            align: req.align,
            rtl: req.rtl,
            boring: None,
            boring_width: 0.0,
            max_intrinsic_width: max_intr,
            paras,
            slot0: None,
            slot1: None,
        }
    }

    fn measure_boring(
        &mut self,
        chars: &[u16],
        base: &DecodedStyle,
        runs: &[DecodedRun],
    ) -> BoringMetrics {
        let mut shaper = self.shaper.borrow_mut();
        let base_paint = shaper.resolve_paint(&base.paint);
        let (width, mut fmi) = if chars.is_empty() {
            (0, shaper.font_metrics_int(base_paint))
        } else if runs.is_empty() {
            let (raw_w, run_fmi) = shaper.measure_boring_run(
                base_paint,
                chars,
                0,
                chars.len(),
                TEXT_RUN_FLAG_LEFT_EDGE | TEXT_RUN_FLAG_RIGHT_EDGE,
            );
            (raw_w.ceil() as i32, run_fmi)
        } else {
            self.style_runs_scratch.clear();
            build_style_runs_for_range(
                &mut *shaper,
                base,
                runs,
                0,
                chars.len(),
                0,
                &[],
                false,
                &mut self.style_runs_scratch,
            );
            let full_flag = TEXT_RUN_FLAG_LEFT_EDGE | TEXT_RUN_FLAG_RIGHT_EDGE;
            let len = chars.len();
            let mut raw_w = 0.0f32;
            let mut acc_fmi = FontMetricsInt::default();
            let mut initialized_fmi = false;
            for sr in &self.style_runs_scratch {
                let sub_flag =
                    resolve_run_flag_for_subsequence(full_flag, false, 0, len, sr.start, sr.end);
                let (seg_w, seg_fmi) =
                    shaper.measure_boring_run(sr.paint_ptr, chars, sr.start, sr.end, sub_flag);
                raw_w += seg_w;
                if !initialized_fmi {
                    acc_fmi = seg_fmi;
                    initialized_fmi = true;
                } else {
                    acc_fmi.top = acc_fmi.top.min(seg_fmi.top);
                    acc_fmi.ascent = acc_fmi.ascent.min(seg_fmi.ascent);
                    acc_fmi.descent = acc_fmi.descent.max(seg_fmi.descent);
                    acc_fmi.bottom = acc_fmi.bottom.max(seg_fmi.bottom);
                    acc_fmi.leading = acc_fmi.leading.max(seg_fmi.leading);
                }
            }
            (raw_w.ceil() as i32, acc_fmi)
        };

        if base.line_height_px > 0.0 {
            let mut span =
                StyledLineHeightSpan::new(base.line_height_px, chars.len(), base.line_height_style);
            span.choose_height(0, chars.len(), &mut fmi);
            fmi.top = fmi.top.min(fmi.ascent);
            fmi.bottom = fmi.bottom.max(fmi.descent);
        }

        BoringMetrics { fmi, width, base_paint }
    }

    fn build_paragraphs(
        &mut self,
        chars: &CharBuffer,
        req: &DecodedRequest,
    ) -> Vec<PrecomputedPara> {
        if chars.is_empty() {
            return Vec::new();
        }
        let mut shaper = self.shaper.borrow_mut();
        let mut out = Vec::new();
        let len = chars.len();
        let mut para_start = 0usize;
        while para_start < len {
            let rel_nl = chars[para_start..].iter().position(|&c| c == 0x000A);
            let para_end = match rel_nl {
                Some(off) => para_start + off + 1,
                None => len,
            };
            let slice = &chars[para_start..para_end];
            let bidi = if needs_bidi(slice, req.rtl) {
                shaper.analyze_bidi(slice, req.rtl)
            } else {
                ParagraphBidi::all_ltr()
            };
            self.style_runs_scratch.clear();
            build_style_runs_for_range(
                &mut *shaper,
                &req.base,
                &req.runs,
                para_start,
                para_end,
                para_start,
                &bidi.levels,
                bidi.paragraph_dir == DIR_RIGHT_TO_LEFT,
                &mut self.style_runs_scratch,
            );
            let mt = shaper.build_measured_text(slice, &self.style_runs_scratch);
            let has_tabs = slice.contains(&0x0009);
            let para_chars = if para_start == 0 && para_end == len {
                chars.to_rc_slice()
            } else {
                Rc::from(slice)
            };
            out.push(PrecomputedPara {
                start: para_start,
                end: para_end,
                chars: para_chars,
                bidi,
                mt,
                has_tabs,
            });
            para_start = para_end;
        }
        out
    }

    fn compute_desired_width(
        &mut self,
        paras: &[PrecomputedPara],
        base: &DecodedStyle,
        runs: &[DecodedRun],
    ) -> f32 {
        let mut max_w = 0.0f32;
        let mut shaper = self.shaper.borrow_mut();
        for p in paras {
            let visible_len =
                if p.chars.last() == Some(&0x000A) { p.chars.len() - 1 } else { p.chars.len() };
            if visible_len == 0 {
                continue;
            }
            let w = if !p.has_tabs {
                shaper.mt_width(&p.mt, 0, visible_len)
            } else {
                let dirs = shaper.line_directions(&p.bidi, &p.chars, 0, visible_len);
                measure_line_signed_width(
                    &mut *shaper,
                    &p.chars[..visible_len],
                    p.start,
                    base,
                    runs,
                    p.bidi.paragraph_dir,
                    &dirs,
                    true,
                    0.0,
                )
                .abs()
            };
            if w > max_w {
                max_w = w;
            }
        }
        max_w
    }

    fn ensure_paragraphs_built(&mut self, intr: &mut Intrinsics) {
        if !intr.paras.is_empty() || intr.chars.is_empty() {
            return;
        }
        let req = DecodedRequest {
            soft_wrap: intr.soft_wrap,
            probe: false,
            max_lines: intr.max_lines,
            ellipsis: intr.ellipsis,
            align: intr.align,
            rtl: intr.rtl,
            min_width: 0,
            max_width: i32::MAX,
            max_height: i32::MAX,
            base: intr.base.clone(),
            runs: intr.runs.clone(),
        };
        intr.paras = self.build_paragraphs(&intr.chars, &req);
    }

    fn layout_from_intrinsics(
        &mut self,
        handle: u32,
        intr_rc: Rc<RefCell<Intrinsics>>,
        min_width: i32,
        max_width: i32,
        max_height: i32,
        need_draw: bool,
    ) -> NativeLayoutOutput {
        let width = {
            let intr = intr_rc.borrow();
            target_layout_width(
                intr.soft_wrap,
                intr.ellipsis,
                intr.max_intrinsic_width,
                min_width,
                max_width,
            )
        };

        let cached_layout = {
            let mut intr = intr_rc.borrow_mut();
            let height_sensitive = intr.ellipsis && intr.max_lines > 1;
            if let Some(ref s0) = intr.slot0
                && width >= s0.min_same_width
                && width <= s0.max_same_width
                && (!height_sensitive || s0.layout_max_height == max_height)
                && (!need_draw
                    || s0.simple_paint_ptr != 0
                    || s0.draw_plan.is_some()
                    || intr.chars.is_empty())
            {
                Some(s0.clone())
            } else if let Some(ref s1) = intr.slot1
                && width >= s1.min_same_width
                && width <= s1.max_same_width
                && (!height_sensitive || s1.layout_max_height == max_height)
                && (!need_draw
                    || s1.simple_paint_ptr != 0
                    || s1.draw_plan.is_some()
                    || intr.chars.is_empty())
            {
                let hit = s1.clone();
                intr.slot1 = intr.slot0.take();
                intr.slot0 = Some(hit.clone());
                Some(hit)
            } else {
                None
            }
        };

        let rc_layout = if let Some(hit) = cached_layout {
            hit
        } else {
            let built = {
                let mut intr = intr_rc.borrow_mut();
                if intr.boring.is_none() || intr.boring_width > width as f32 {
                    self.ensure_paragraphs_built(&mut intr);
                }
                self.compute_layout_at(&intr, width, max_height, need_draw)
            };
            let rc = self.alloc_layout_rc(built);
            let mut intr = intr_rc.borrow_mut();
            intr.slot1 = intr.slot0.take();
            intr.slot0 = Some(rc.clone());
            rc
        };

        let out = output_from_layout(&rc_layout, width);
        self.handles.insert(handle, LiveHandle { intrinsics: intr_rc, layout: rc_layout, width });
        out
    }

    fn compute_layout_at(
        &mut self,
        intr: &Intrinsics,
        width: i32,
        max_height: i32,
        need_draw: bool,
    ) -> ComputedLayout {
        if let Some(bm) = intr.boring
            && intr.boring_width <= width as f32
        {
            let h = bm.fmi.descent - bm.fmi.ascent;
            let baseline = -bm.fmi.ascent;
            let exceeded = 1 > intr.max_lines;
            let min_same =
                if intr.soft_wrap || intr.ellipsis { intr.boring_width.ceil() as i32 } else { 0 };
            let max_same = if intr.align == 0 && !intr.rtl && !exceeded { i32::MAX } else { width };
            let line = LayoutLine {
                start: 0,
                end: intr.chars.len(),
                top: 0,
                descent: bm.fmi.descent,
                bottom: h,
                dir: DIR_LEFT_TO_RIGHT,
                directions: LineDirections::Single([0, RUN_LENGTH_MASK]),
                has_tab: false,
                ellipsis_start: 0,
                ellipsis_count: 0,
                para_index: 0,
            };
            let laid_out = LaidOut {
                metrics: ParagraphMetrics {
                    size: IntSize::new(width, h),
                    first_baseline: baseline as f32,
                    last_baseline: baseline as f32,
                    line_count: 1,
                },
                did_exceed_max_lines: exceeded,
                max_intrinsic_width: Some(intr.max_intrinsic_width),
            };
            let mut computed = ComputedLayout {
                min_same_width: min_same,
                max_same_width: max_same,
                layout_max_height: max_height,
                effective_max_lines: intr.max_lines,
                laid_out,
                is_boring: true,
                lines: LayoutLines::Single([line]),
                last_line_override: None,
                ellipsized_chars: None,
                simple_paint_ptr: 0,
                simple_paint_owner: None,
                simple_baseline: 0.0,
                draw_plan: None,
            };
            if need_draw {
                self.populate_draw_for_layout(intr, &mut computed, width);
            }
            return computed;
        }

        let mut computed =
            self.compute_static_layout(intr, width, max_height, false, intr.max_lines);
        if intr.ellipsis && intr.max_lines > 1 && computed.laid_out.metrics.size.height > max_height
        {
            let mut fitting = 0usize;
            while fitting < computed.lines.len() && computed.lines[fitting].bottom <= max_height {
                fitting += 1;
            }
            let clamped = (fitting as i32).max(1);
            if clamped < intr.max_lines {
                computed = self.compute_static_layout(intr, width, max_height, false, clamped);
            }
        }
        if need_draw {
            self.populate_draw_for_layout(intr, &mut computed, width);
        }
        computed
    }

    fn compute_static_layout(
        &mut self,
        intr: &Intrinsics,
        width: i32,
        max_height: i32,
        need_draw: bool,
        max_lines_override: i32,
    ) -> ComputedLayout {
        let outer_width = width.max(0);
        let max_visible_lines = max_lines_override.max(1) as usize;
        let buf_end = intr.chars.len();
        let justified_mode = intr.align == 3;

        let lh_end = if intr.chars.is_empty() || intr.chars.last() == Some(&0x000A) {
            buf_end + 1
        } else {
            buf_end
        };
        let mut lh_span = (intr.base.line_height_px > 0.0).then(|| {
            StyledLineHeightSpan::new(intr.base.line_height_px, lh_end, intr.base.line_height_style)
        });

        let mut lines: Vec<LayoutLine> = Vec::with_capacity(4);
        let mut v = 0i32;
        let mut ellipsized = false;

        let base_paint_ptr = self.shaper.borrow_mut().resolve_paint(&intr.base.paint);
        let ellipsis_glyph_width = if intr.ellipsis {
            let mut shaper = self.shaper.borrow_mut();
            let ell = [CHAR_ELLIPSIS];
            shaper.get_run_advance(
                base_paint_ptr,
                &ell,
                0,
                1,
                0,
                1,
                false,
                1,
                TEXT_RUN_FLAG_LEFT_EDGE | TEXT_RUN_FLAG_RIGHT_EDGE,
                0.0,
                None,
            )
        } else {
            0.0
        };

        'para_loop: for (para_idx, para) in intr.paras.iter().enumerate() {
            {
                let mut shaper = self.shaper.borrow_mut();
                self.breaks_scratch.clear();
                shaper.compute_line_breaks(
                    &para.mt,
                    &para.chars,
                    outer_width as f32,
                    justified_mode,
                    &mut self.breaks_scratch,
                );
            }

            let mut break_count = self.breaks_scratch.len();
            let remaining = max_visible_lines.saturating_sub(lines.len());
            if intr.ellipsis && remaining > 0 && remaining < break_count {
                // `StaticLayout.java:974–996`: collapse the last visible line and all overflowed
                // lines in this paragraph into a single line so `TruncateAt.END` fills the full
                // `outer_width` rather than stopping at an earlier soft word wrap.
                let shaper = self.shaper.borrow();
                let mut combined_width = 0.0f32;
                let mut combined_flags = 0i32;
                for i in (remaining - 1)..break_count {
                    if i + 1 == break_count {
                        combined_width += self.breaks_scratch[i].width;
                    } else {
                        let seg_s =
                            if i == 0 { 0 } else { self.breaks_scratch[i - 1].break_offset };
                        let seg_e = self.breaks_scratch[i].break_offset;
                        combined_width += shaper.mt_width(&para.mt, seg_s, seg_e);
                    }
                    combined_flags |= self.breaks_scratch[i].flags & TAB_MASK;
                }
                let final_offset = self.breaks_scratch[break_count - 1].break_offset;
                let target = &mut self.breaks_scratch[remaining - 1];
                target.break_offset = final_offset;
                target.width = combined_width;
                target.flags = (target.flags & !TAB_MASK) | combined_flags;
                self.breaks_scratch.truncate(remaining);
                break_count = remaining;
            }

            let mut here = para.start;
            let mut span_end = para.start;
            let mut fm_top = 0i32;
            let mut fm_bottom = 0i32;
            let mut fm_ascent = 0i32;
            let mut fm_descent = 0i32;

            let mut break_idx = 0usize;
            while span_end < para.end {
                let next_span = next_metric_transition(&intr.runs, span_end, para.end);
                let seg_paint = resolve_span_paint(
                    &mut *self.shaper.borrow_mut(),
                    &intr.base,
                    &intr.runs,
                    span_end,
                    next_span,
                );
                let fm = self.shaper.borrow_mut().font_metrics_int(seg_paint);
                span_end = next_span;

                if fm.top < fm_top {
                    fm_top = fm.top;
                }
                if fm.bottom > fm_bottom {
                    fm_bottom = fm.bottom;
                }
                if fm.ascent < fm_ascent {
                    fm_ascent = fm.ascent;
                }
                if fm.descent > fm_descent {
                    fm_descent = fm.descent;
                }

                while break_idx < break_count
                    && para.start + self.breaks_scratch[break_idx].break_offset <= span_end
                {
                    let brk = self.breaks_scratch[break_idx];
                    let end_pos = para.start + brk.break_offset;
                    let more_chars = end_pos < buf_end;

                    let ascent_fb = fm_ascent.min(java_round(brk.ascent));
                    let descent_fb = fm_descent.max(java_round(brk.descent));
                    let top_fb = fm_top.min(ascent_fb);
                    let bottom_fb = fm_bottom.max(descent_fb);

                    let mut line_fmi = FontMetricsInt {
                        top: top_fb,
                        ascent: ascent_fb,
                        descent: descent_fb,
                        bottom: bottom_fb,
                        leading: 0,
                    };
                    if let Some(ref mut span) = lh_span {
                        span.choose_height(here, end_pos, &mut line_fmi);
                    }

                    let line_idx = lines.len();
                    let first_line = line_idx == 0;
                    let cur_is_last_visible = line_idx + 1 == max_visible_lines;

                    let mut ell_start = 0usize;
                    let mut ell_count = 0usize;
                    if intr.ellipsis {
                        let force_ellipsis = more_chars && cur_is_last_visible;
                        let do_ellipsis = (max_visible_lines == 1 && more_chars)
                            || (first_line && !more_chars)
                            || (!first_line && (cur_is_last_visible || !more_chars));
                        if do_ellipsis && (brk.width > outer_width as f32 || force_ellipsis) {
                            let shaper = self.shaper.borrow();
                            let line_len = end_pos - here;
                            let in_para_start = here - para.start;
                            let avail = outer_width as f32;
                            let mut sum = 0.0f32;
                            let mut i = 0usize;
                            while i < line_len {
                                let w = shaper.mt_char_width_at(&para.mt, in_para_start + i);
                                if w + sum + ellipsis_glyph_width > avail {
                                    break;
                                }
                                sum += w;
                                i += 1;
                            }
                            ell_start = i;
                            ell_count = line_len - i;
                            if force_ellipsis && ell_count == 0 && line_len > 0 {
                                ell_start = line_len - 1;
                                ell_count = 1;
                            }
                            if ell_count > 0 {
                                ellipsized = true;
                            }
                        }
                    }

                    let line_top = v;
                    let line_height = line_fmi.descent - line_fmi.ascent;
                    v += line_height;
                    let dirs = self.shaper.borrow_mut().line_directions(
                        &para.bidi,
                        &para.chars,
                        here - para.start,
                        end_pos - para.start,
                    );

                    lines.push(LayoutLine {
                        start: here,
                        end: end_pos,
                        top: line_top,
                        descent: line_fmi.descent,
                        bottom: v,
                        dir: para.bidi.paragraph_dir,
                        directions: dirs,
                        has_tab: (brk.flags & TAB_MASK) != 0,
                        ellipsis_start: ell_start,
                        ellipsis_count: ell_count,
                        para_index: para_idx,
                    });

                    if end_pos < span_end {
                        fm_top = fm.top;
                        fm_bottom = fm.bottom;
                        fm_ascent = fm.ascent;
                        fm_descent = fm.descent;
                    } else {
                        fm_top = 0;
                        fm_bottom = 0;
                        fm_ascent = 0;
                        fm_descent = 0;
                    }

                    here = end_pos;
                    break_idx += 1;

                    if lines.len() >= max_visible_lines && ellipsized {
                        break 'para_loop;
                    }
                }
            }
        }

        if (buf_end == 0 || intr.chars.last() == Some(&0x000A))
            && lines.len() < max_visible_lines
            && !ellipsized
        {
            let empty_bidi = if intr.rtl {
                ParagraphBidi {
                    paragraph_dir: DIR_RIGHT_TO_LEFT,
                    levels: Vec::new(),
                    rtl_request: true,
                }
            } else {
                ParagraphBidi::all_ltr()
            };
            let fm = self.shaper.borrow_mut().font_metrics_int(base_paint_ptr);
            let dirs = if intr.rtl {
                LineDirections::Single([0, RUN_LENGTH_MASK | RUN_RTL_FLAG])
            } else {
                LineDirections::Single([0, RUN_LENGTH_MASK])
            };
            let line_top = v;
            v += fm.descent - fm.ascent;
            lines.push(LayoutLine {
                start: buf_end,
                end: buf_end,
                top: line_top,
                descent: fm.descent,
                bottom: v,
                dir: empty_bidi.paragraph_dir,
                directions: dirs,
                has_tab: false,
                ellipsis_start: 0,
                ellipsis_count: 0,
                para_index: intr.paras.len(),
            });
        }

        let visible_lines = (lines.len() as i32).clamp(1, max_lines_override.max(1)) as usize;
        let last_line_override = self.compute_last_line_metrics(intr, &lines, visible_lines - 1);

        let height = line_bottom(&lines, visible_lines - 1, visible_lines, last_line_override);
        let first_baseline = line_baseline(&lines, 0, visible_lines, last_line_override);
        let last_baseline =
            line_baseline(&lines, visible_lines - 1, visible_lines, last_line_override);
        let last_ellipsized = lines.last().is_some_and(|l| l.ellipsis_count > 0);
        let exceeded = (lines.len() as i32) > max_lines_override || last_ellipsized;

        let (min_same, max_same) =
            if intr.align == 0 && !intr.rtl && !intr.ellipsis && !exceeded && lines.len() == 1 {
                let min_s = if intr.soft_wrap { intr.max_intrinsic_width.ceil() as i32 } else { 0 };
                (min_s, i32::MAX)
            } else {
                (width, width)
            };

        let ellipsized_chars = if lines.iter().any(|l| l.ellipsis_count > 0) {
            let mut copy = intr.chars.to_vec();
            for l in &lines {
                if l.ellipsis_count > 0 {
                    let at = l.start + l.ellipsis_start;
                    for i in 0..l.ellipsis_count {
                        if at + i < copy.len() {
                            copy[at + i] =
                                if i == 0 { CHAR_ELLIPSIS } else { CHAR_ELLIPSIS_FILLER };
                        }
                    }
                }
            }
            Some(copy.into_boxed_slice())
        } else {
            None
        };

        let laid_out = LaidOut {
            metrics: ParagraphMetrics {
                size: IntSize::new(width, height.round() as i32),
                first_baseline,
                last_baseline,
                line_count: visible_lines as u32,
            },
            did_exceed_max_lines: exceeded,
            max_intrinsic_width: Some(intr.max_intrinsic_width),
        };

        let mut computed = ComputedLayout {
            min_same_width: min_same,
            max_same_width: max_same,
            layout_max_height: max_height,
            effective_max_lines: max_lines_override,
            laid_out,
            is_boring: false,
            lines: LayoutLines::Multi(lines),
            last_line_override,
            ellipsized_chars,
            simple_paint_ptr: 0,
            simple_paint_owner: None,
            simple_baseline: 0.0,
            draw_plan: None,
        };
        if need_draw {
            self.populate_draw_for_layout(intr, &mut computed, width);
        }
        computed
    }

    /// Compose's `TextLayout.getLastLineMetrics` (`TextMeasurer.kt:888–908`): lays out `"\u200B"`
    /// with a modified `StyledLineHeightSpan` when the last visible line is empty and `lineHeight > 0`.
    fn compute_last_line_metrics(
        &mut self,
        intr: &Intrinsics,
        lines: &[LayoutLine],
        line: usize,
    ) -> Option<FontMetricsInt> {
        if intr.base.line_height_px <= 0.0 {
            return None;
        }
        let l = lines.get(line)?;
        if l.start != l.end {
            return None;
        }
        let style = intr.base.line_height_style;
        let trim_first = if line != 0 && style.trim_last_line_bottom {
            false
        } else {
            style.trim_last_line_bottom
        };
        let modified_style = LineHeightStyleSpec {
            top_ratio: style.top_ratio,
            trim_first_line_top: trim_first,
            trim_last_line_bottom: style.trim_last_line_bottom,
        };
        let zwsp = [CHAR_ZWSP];
        let mut shaper = self.shaper.borrow_mut();
        let base_paint = shaper.resolve_paint(&intr.base.paint);
        let mut fmi = shaper.font_metrics_int_for_text(base_paint, &zwsp, 0, 1, 0, 1, intr.rtl);
        let mut span = StyledLineHeightSpan::new(intr.base.line_height_px, 1, modified_style);
        span.choose_height(0, 1, &mut fmi);
        fmi.top = fmi.top.min(fmi.ascent);
        fmi.bottom = fmi.descent - fmi.ascent;
        Some(fmi)
    }

    fn populate_draw_for_layout(
        &mut self,
        intr: &Intrinsics,
        layout: &mut ComputedLayout,
        width: i32,
    ) {
        layout.simple_paint_ptr = 0;
        layout.simple_paint_owner = None;
        layout.simple_baseline = 0.0;
        layout.draw_plan = None;

        if intr.chars.is_empty() {
            return;
        }

        // Fast path: single-line LTR boring plain text with no background/decorations and start alignment.
        let is_start_ltr = !intr.rtl && (intr.align == 0 || intr.align == 3);
        if layout.is_boring
            && is_start_ltr
            && intr.runs.is_empty()
            && intr.base.background == 0
            && intr.base.decoration == 0
        {
            let mut shaper = self.shaper.borrow_mut();
            let paint_ptr = match intr.boring {
                Some(bm) if bm.base_paint != 0 => bm.base_paint,
                _ => shaper.resolve_paint(&intr.base.paint),
            };
            layout.simple_paint_owner = shaper.paint_owner(paint_ptr);
            layout.simple_paint_ptr = paint_ptr;
            layout.simple_baseline = layout.laid_out.metrics.first_baseline;
            return;
        }

        let plan = self.build_complex_draw_plan(intr, layout, width);
        if !plan.runs.is_empty() {
            layout.draw_plan = Some(Rc::new(plan));
        }
    }

    fn build_complex_draw_plan(
        &mut self,
        intr: &Intrinsics,
        layout: &ComputedLayout,
        outer_width: i32,
    ) -> ComplexParagraphDraw {
        let mut plan = ComplexParagraphDraw::default();
        let visible_lines =
            (layout.lines.len() as i32).clamp(1, layout.effective_max_lines.max(1)) as usize;
        let display_chars: &[u16] = layout.ellipsized_chars.as_deref().unwrap_or(&intr.chars);
        let mut shaper = self.shaper.borrow_mut();

        if layout.is_boring {
            plan.buffers.push(intr.chars.to_rc_slice());
            let line = &layout.lines[0];
            let ltop = line.top as f32;
            let lbottom = line.bottom as f32;
            let lbaseline = (line.bottom - line.descent) as f32;
            let x_start = compute_line_start_x(
                &mut *shaper,
                intr,
                layout,
                outer_width,
                0,
                display_chars,
                line.end,
            ) as f32;
            emit_line_runs(
                &mut *shaper,
                intr,
                display_chars,
                0,
                line.end,
                0,
                0,
                0,
                line.dir,
                &line.directions,
                false,
                0.0,
                x_start,
                ltop,
                lbaseline,
                lbottom,
                &mut plan,
            );
            return plan;
        }

        // Pre-populate paragraph buffers `0..intr.paras.len()` so multiple wrapped lines in the
        // same paragraph share a single UTF-16 buffer and `MeasuredText*`.
        for para in &intr.paras {
            plan.buffers.push(para.chars.clone());
            if let Some(ref owner) = para.mt.owner {
                plan.owners.push(owner.clone());
            }
        }

        for line_idx in 0..visible_lines {
            let line = &layout.lines[line_idx];
            let visible_end = line_visible_end(
                display_chars,
                line_idx,
                layout.lines.len(),
                line.start,
                line.end,
                true,
            );
            if visible_end <= line.start {
                continue;
            }
            let justify = is_justification_required(intr, display_chars, line.end);
            let word_spacing_px = if justify {
                compute_justify_word_spacing(
                    &mut *shaper,
                    intr,
                    display_chars,
                    line,
                    outer_width as f32,
                )
            } else {
                0.0
            };
            let x_start = compute_line_start_x(
                &mut *shaper,
                intr,
                layout,
                outer_width,
                line_idx,
                display_chars,
                visible_end,
            ) as f32;
            let ltop = line.top as f32;
            let lbottom = line.bottom as f32;
            let lbaseline = (line.bottom - line.descent) as f32;

            let (buf_idx, buf_base_offset, mt_ptr) = if line.ellipsis_count > 0 {
                let idx = plan.buffers.len() as u32;
                plan.buffers.push(Rc::from(&display_chars[line.start..visible_end]));
                (idx, line.start, 0)
            } else if let Some(para) = intr.paras.get(line.para_index) {
                (line.para_index as u32, para.start, para.mt.ptr)
            } else {
                let idx = plan.buffers.len() as u32;
                plan.buffers.push(Rc::from(&display_chars[line.start..visible_end]));
                (idx, line.start, 0)
            };

            emit_line_runs(
                &mut *shaper,
                intr,
                display_chars,
                line.start,
                visible_end,
                buf_idx,
                buf_base_offset,
                mt_ptr,
                line.dir,
                &line.directions,
                line.has_tab,
                word_spacing_px,
                x_start,
                ltop,
                lbaseline,
                lbottom,
                &mut plan,
            );
        }

        plan
    }
}

#[inline]
fn java_round(v: f32) -> i32 {
    (f64::from(v) + 0.5).floor() as i32
}

#[inline]
fn target_layout_width(
    soft_wrap: bool,
    ellipsis: bool,
    max_intrinsic_width: f32,
    min_width: i32,
    max_width: i32,
) -> i32 {
    let bounded = max_width != i32::MAX;
    let available = if (soft_wrap || ellipsis) && bounded { max_width } else { i32::MAX };
    if min_width == available {
        available
    } else {
        (max_intrinsic_width.ceil() as i32).clamp(min_width, available)
    }
}

fn output_from_layout(layout: &ComputedLayout, width: i32) -> NativeLayoutOutput {
    let mut laid_out = layout.laid_out;
    laid_out.metrics.size.width = width;
    NativeLayoutOutput {
        laid_out,
        min_same_width: layout.min_same_width,
        max_same_width: layout.max_same_width,
        simple_paint_ptr: layout.simple_paint_ptr,
        simple_paint_owner: layout.simple_paint_owner.clone(),
        simple_baseline: layout.simple_baseline,
        complex_draw: layout.draw_plan.clone(),
    }
}

#[inline]
fn line_bottom(
    lines: &[LayoutLine],
    line: usize,
    visible_lines: usize,
    last_line_override: Option<FontMetricsInt>,
) -> f32 {
    if line == visible_lines - 1
        && let Some(last_fmi) = last_line_override
    {
        let prev_bot = if line == 0 { 0 } else { lines[line - 1].bottom };
        (prev_bot + last_fmi.bottom) as f32
    } else {
        lines[line].bottom as f32
    }
}

#[inline]
fn line_baseline(
    lines: &[LayoutLine],
    line: usize,
    visible_lines: usize,
    last_line_override: Option<FontMetricsInt>,
) -> f32 {
    if line == visible_lines - 1
        && let Some(last_fmi) = last_line_override
    {
        (lines[line].top - last_fmi.ascent) as f32
    } else {
        (lines[line].bottom - lines[line].descent) as f32
    }
}

#[inline]
fn next_metric_transition(runs: &[DecodedRun], start: usize, limit: usize) -> usize {
    let mut best = limit;
    for r in runs {
        if r.start > start && r.start < best {
            best = r.start;
        }
        if r.end > start && r.end < best {
            best = r.end;
        }
    }
    best
}

fn effective_style_at<'a>(
    base: &'a DecodedStyle,
    runs: &'a [DecodedRun],
    seg_start: usize,
    seg_end: usize,
) -> (PaintStyleKey, u8, u32) {
    let mut key = base.paint.clone();
    let mut dec = base.decoration;
    let mut bg = base.background;
    for r in runs {
        if r.start >= seg_end || r.end <= seg_start {
            continue;
        }
        let saved_color = key.color;
        let saved_dec = dec;
        key = r.style.paint.clone();
        dec = if r.inherits_decoration { saved_dec } else { r.style.decoration };
        if r.inherits_color {
            key.color = saved_color;
        }
        if r.style.background != 0 {
            bg = r.style.background;
        }
    }
    (key, dec, bg)
}

fn resolve_span_paint<S: TextShaper>(
    shaper: &mut S,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    seg_start: usize,
    seg_end: usize,
) -> i64 {
    if runs.is_empty() {
        return shaper.resolve_paint(&base.paint);
    }
    let (key, _, _) = effective_style_at(base, runs, seg_start, seg_end);
    shaper.resolve_paint(&key)
}

#[allow(clippy::too_many_arguments)]
fn build_style_runs_for_range<S: TextShaper>(
    shaper: &mut S,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    abs_start: usize,
    abs_end: usize,
    para_base: usize,
    bidi_levels: &[u8],
    default_rtl: bool,
    out: &mut Vec<StyleRunSpec>,
) {
    let mut pos = abs_start;
    while pos < abs_end {
        let style_end = next_metric_transition(runs, pos, abs_end);
        let paint_ptr = resolve_span_paint(shaper, base, runs, pos, style_end);
        if bidi_levels.is_empty() {
            out.push(StyleRunSpec {
                paint_ptr,
                start: pos - para_base,
                end: style_end - para_base,
                is_rtl: default_rtl,
            });
        } else {
            let mut sub = pos;
            while sub < style_end {
                let lvl = bidi_levels[sub - para_base];
                let mut sub_end = sub + 1;
                while sub_end < style_end && bidi_levels[sub_end - para_base] == lvl {
                    sub_end += 1;
                }
                out.push(StyleRunSpec {
                    paint_ptr,
                    start: sub - para_base,
                    end: sub_end - para_base,
                    is_rtl: (lvl & 1) != 0,
                });
                sub = sub_end;
            }
        }
        pos = style_end;
    }
}

#[inline]
fn line_visible_end(
    chars: &[u16],
    line_idx: usize,
    total_lines: usize,
    start: usize,
    mut end: usize,
    trailing_space_at_last_line_visible: bool,
) -> usize {
    if trailing_space_at_last_line_visible && line_idx + 1 == total_lines {
        return end;
    }
    while end > start {
        let ch = chars[end - 1];
        if ch == 0x000A {
            return end - 1;
        }
        if !is_line_end_space(ch) {
            break;
        }
        end -= 1;
    }
    end
}

#[inline]
fn is_justification_required(intr: &Intrinsics, chars: &[u16], line_end: usize) -> bool {
    intr.align == 3 && line_end < chars.len() && line_end > 0 && chars[line_end - 1] != 0x000A
}

fn compute_justify_word_spacing<S: TextShaper>(
    shaper: &mut S,
    intr: &Intrinsics,
    display_chars: &[u16],
    line: &LayoutLine,
    justify_width: f32,
) -> f32 {
    let mut end = line.end;
    while end > line.start && is_line_end_space(display_chars[end - 1]) {
        if display_chars[end - 1] == 0x000A {
            break;
        }
        end -= 1;
    }
    let slice = &display_chars[line.start..end];
    let spaces = slice.iter().filter(|&&c| c == 0x0020).count();
    if spaces == 0 {
        return 0.0;
    }
    let natural_width = measure_line_signed_width(
        shaper,
        slice,
        line.start,
        &intr.base,
        &intr.runs,
        line.dir,
        &line.directions,
        line.has_tab,
        0.0,
    )
    .abs();
    (justify_width - natural_width) / (spaces as f32)
}

#[inline]
fn calculate_run_flag(bidi_run_index: usize, bidi_run_count: usize, line_dir: i32) -> i32 {
    if bidi_run_count <= 1 {
        return TEXT_RUN_FLAG_LEFT_EDGE | TEXT_RUN_FLAG_RIGHT_EDGE;
    }
    if bidi_run_index != 0 && bidi_run_index + 1 != bidi_run_count {
        return 0;
    }
    let mut flag = 0;
    if bidi_run_index == 0 {
        flag |= if line_dir == DIR_LEFT_TO_RIGHT {
            TEXT_RUN_FLAG_LEFT_EDGE
        } else {
            TEXT_RUN_FLAG_RIGHT_EDGE
        };
    }
    if bidi_run_index + 1 == bidi_run_count {
        flag |= if line_dir == DIR_LEFT_TO_RIGHT {
            TEXT_RUN_FLAG_RIGHT_EDGE
        } else {
            TEXT_RUN_FLAG_LEFT_EDGE
        };
    }
    flag
}

#[inline]
fn resolve_run_flag_for_subsequence(
    run_flag: i32,
    is_rtl_run: bool,
    run_start: usize,
    run_end: usize,
    span_start: usize,
    span_end: usize,
) -> i32 {
    if run_flag == 0 {
        return 0;
    }
    let mut local = run_flag;
    if (run_flag & TEXT_RUN_FLAG_LEFT_EDGE) != 0 {
        if is_rtl_run {
            if span_end != run_end {
                local &= !TEXT_RUN_FLAG_LEFT_EDGE;
            }
        } else if span_start != run_start {
            local &= !TEXT_RUN_FLAG_LEFT_EDGE;
        }
    }
    if (run_flag & TEXT_RUN_FLAG_RIGHT_EDGE) != 0 {
        if is_rtl_run {
            if span_start != run_start {
                local &= !TEXT_RUN_FLAG_RIGHT_EDGE;
            }
        } else if span_end != run_end {
            local &= !TEXT_RUN_FLAG_RIGHT_EDGE;
        }
    }
    local
}

#[inline]
fn next_tab_stop(h: f32) -> f32 {
    ((h / TAB_INCREMENT).floor() + 1.0) * TAB_INCREMENT
}

#[allow(clippy::too_many_arguments)]
fn measure_line_signed_width<S: TextShaper>(
    shaper: &mut S,
    line_chars: &[u16],
    abs_line_start: usize,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    line_dir: i32,
    directions: &[i32],
    has_tab: bool,
    word_spacing_px: f32,
) -> f32 {
    let m_len = line_chars.len();
    if m_len == 0 {
        return 0.0;
    }
    let run_count = directions.len() / 2;
    let mut h = 0.0f32;
    for run_idx in 0..run_count {
        let run_start = (directions[run_idx * 2].max(0) as usize).min(m_len);
        if run_start >= m_len {
            break;
        }
        let packed = directions[run_idx * 2 + 1];
        let run_len = (packed & RUN_LENGTH_MASK).max(0) as usize;
        let run_limit = run_start.saturating_add(run_len).min(m_len);
        let run_is_rtl = (packed & RUN_RTL_FLAG) != 0;
        let run_flag = calculate_run_flag(run_idx, run_count, line_dir);

        let mut seg_start = run_start;
        let mut j = if has_tab { run_start } else { run_limit };
        while j <= run_limit {
            if j == run_limit || line_chars[j] == 0x0009 {
                let w = measure_unidirectional_segment(
                    shaper,
                    line_chars,
                    abs_line_start,
                    base,
                    runs,
                    seg_start,
                    j,
                    j,
                    run_is_rtl,
                    run_flag,
                    word_spacing_px,
                    None,
                );
                // `measure_unidirectional_segment` returns signed width based on `run_is_rtl`
                // (`-abs` for RTL, `+abs` for LTR); convert to paragraph-direction signed step!
                let abs_w = w.abs();
                h += (line_dir as f32) * abs_w;
                if j != run_limit {
                    let unsigned = h * (line_dir as f32);
                    h = (line_dir as f32) * next_tab_stop(unsigned);
                }
                seg_start = j + 1;
            }
            j += 1;
        }
    }
    h
}

#[allow(clippy::too_many_arguments)]
fn measure_unidirectional_segment<S: TextShaper>(
    shaper: &mut S,
    line_chars: &[u16],
    abs_line_start: usize,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    start: usize,
    measure_limit: usize,
    limit: usize,
    run_is_rtl: bool,
    run_flag: i32,
    word_spacing_px: f32,
    mut out_advances: Option<&mut [f32]>,
) -> f32 {
    if start >= measure_limit {
        return 0.0;
    }
    let mut total = 0.0f32;
    let mut i = start;
    while i < measure_limit {
        let inext = next_metric_transition(runs, abs_line_start + i, abs_line_start + limit)
            - abs_line_start;
        let mlimit = inext.min(measure_limit);
        let sub_flag =
            resolve_run_flag_for_subsequence(run_flag, run_is_rtl, start, limit, i, inext);
        let paint_ptr =
            resolve_span_paint(shaper, base, runs, abs_line_start + i, abs_line_start + inext);
        let adv_slice =
            out_advances.as_deref_mut().map(|slice| &mut slice[i - start..mlimit - start]);
        let w = shaper.get_run_advance(
            paint_ptr,
            line_chars,
            i,
            inext,
            i,
            inext,
            run_is_rtl,
            mlimit,
            sub_flag,
            word_spacing_px,
            adv_slice,
        );
        total += if run_is_rtl { -w } else { w };
        i = inext;
    }
    total
}

fn compute_line_start_x<S: TextShaper>(
    shaper: &mut S,
    intr: &Intrinsics,
    layout: &ComputedLayout,
    outer_width: i32,
    line_idx: usize,
    display_chars: &[u16],
    visible_end: usize,
) -> i32 {
    let line = &layout.lines[line_idx];
    let dir = line.dir;
    let left = 0i32;
    let right = outer_width;

    // `align`: 0 normal, 1 opposite, 2 center, 3 justify (normal alignment).
    let align_code = if intr.align == 3 { 0 } else { intr.align };
    if align_code == 0 {
        return if dir == DIR_LEFT_TO_RIGHT { left } else { right };
    }

    let max = if layout.is_boring {
        intr.boring.map_or(0, |b| b.width)
    } else if is_justification_required(intr, display_chars, line.end) {
        dir * outer_width
    } else {
        let ext_end = line_visible_end(
            display_chars,
            line_idx,
            layout.lines.len(),
            line.start,
            line.end,
            true,
        );
        let _ = visible_end;
        measure_line_signed_width(
            shaper,
            &display_chars[line.start..ext_end],
            line.start,
            &intr.base,
            &intr.runs,
            dir,
            &line.directions,
            line.has_tab,
            0.0,
        ) as i32
    };

    if align_code == 1 {
        if dir == DIR_LEFT_TO_RIGHT { right - max } else { left - max }
    } else {
        let max_even = max & !1;
        (right + left - max_even) >> 1
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_line_runs<S: TextShaper>(
    shaper: &mut S,
    intr: &Intrinsics,
    display_chars: &[u16],
    line_start: usize,
    visible_end: usize,
    buffer_index: u32,
    buffer_base_offset: usize,
    mt_ptr: i64,
    line_dir: i32,
    directions: &[i32],
    has_tab: bool,
    word_spacing_px: f32,
    x_origin: f32,
    ltop: f32,
    lbaseline: f32,
    lbottom: f32,
    plan: &mut ComplexParagraphDraw,
) {
    let m_len = visible_end - line_start;
    if m_len == 0 {
        return;
    }
    let line_chars = &display_chars[line_start..visible_end];
    let run_count = directions.len() / 2;
    let mut h = 0.0f32;

    for run_idx in 0..run_count {
        let run_start = (directions[run_idx * 2].max(0) as usize).min(m_len);
        if run_start >= m_len {
            break;
        }
        let packed = directions[run_idx * 2 + 1];
        let run_len = (packed & RUN_LENGTH_MASK).max(0) as usize;
        let run_limit = run_start.saturating_add(run_len).min(m_len);
        let run_is_rtl = (packed & RUN_RTL_FLAG) != 0;
        let run_flag = calculate_run_flag(run_idx, run_count, line_dir);

        let mut seg_start = run_start;
        let mut j = if has_tab { run_start } else { run_limit };
        while j <= run_limit {
            if j == run_limit || line_chars[j] == 0x0009 {
                if seg_start < j {
                    let opposite_dir = (line_dir == DIR_LEFT_TO_RIGHT) == run_is_rtl;
                    let pre_shift = if opposite_dir {
                        -measure_unidirectional_segment(
                            shaper,
                            line_chars,
                            line_start,
                            &intr.base,
                            &intr.runs,
                            seg_start,
                            j,
                            j,
                            run_is_rtl,
                            run_flag,
                            word_spacing_px,
                            None,
                        )
                    } else {
                        0.0
                    };
                    let seg_origin_x = x_origin + h + pre_shift;
                    let need_advance_after =
                        !opposite_dir && ((j != run_limit) || (run_idx + 1 < run_count));
                    let signed_seg_w = emit_unidirectional_segment_cmds(
                        shaper,
                        intr,
                        line_chars,
                        line_start,
                        buffer_index,
                        buffer_base_offset,
                        mt_ptr,
                        seg_start,
                        j,
                        run_is_rtl,
                        run_flag,
                        word_spacing_px,
                        need_advance_after,
                        seg_origin_x,
                        ltop,
                        lbaseline,
                        lbottom,
                        plan,
                    );
                    h += if opposite_dir { pre_shift } else { signed_seg_w };
                }
                if j != run_limit {
                    let unsigned = h * (line_dir as f32);
                    h = (line_dir as f32) * next_tab_stop(unsigned);
                }
                seg_start = j + 1;
            }
            j += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_unidirectional_segment_cmds<S: TextShaper>(
    shaper: &mut S,
    intr: &Intrinsics,
    line_chars: &[u16],
    abs_line_start: usize,
    buffer_index: u32,
    buffer_base_offset: usize,
    mt_ptr: i64,
    start: usize,
    limit: usize,
    run_is_rtl: bool,
    run_flag: i32,
    word_spacing_px: f32,
    need_advance_after: bool,
    mut x: f32,
    ltop: f32,
    lbaseline: f32,
    lbottom: f32,
    plan: &mut ComplexParagraphDraw,
) -> f32 {
    let original_x = x;
    let mut i = start;
    while i < limit {
        let inext = next_metric_transition(&intr.runs, abs_line_start + i, abs_line_start + limit)
            - abs_line_start;
        let sub_flag =
            resolve_run_flag_for_subsequence(run_flag, run_is_rtl, start, limit, i, inext);
        let (style_key, dec_mask, bg_color) =
            effective_style_at(&intr.base, &intr.runs, abs_line_start + i, abs_line_start + inext);
        let paint_ptr = shaper.resolve_paint(&style_key);
        if let Some(owner) = shaper.paint_owner(paint_ptr)
            && !plan.owners.iter().any(|existing| Rc::ptr_eq(existing, &owner))
        {
            plan.owners.push(owner);
        }
        let has_dec = dec_mask != 0;
        let need_width =
            run_is_rtl || bg_color != 0 || has_dec || inext < limit || need_advance_after;
        let total_w = if need_width {
            shaper.get_run_advance(
                paint_ptr,
                line_chars,
                i,
                inext,
                i,
                inext,
                run_is_rtl,
                inext,
                sub_flag,
                word_spacing_px,
                None,
            )
        } else {
            0.0
        };
        let (left_x, right_x) = if run_is_rtl { (x - total_w, x) } else { (x, x + total_w) };
        let background = (bg_color != 0).then_some(DrawRectCmd {
            left: left_x,
            top: ltop,
            right: right_x,
            bottom: lbottom,
            color: bg_color,
        });
        let dec_start = plan.decorations.len() as u32;
        if has_dec {
            let sm = shaper.stroke_metrics(paint_ptr);
            if dec_mask & 1 != 0 {
                let top = lbaseline + sm.underline_position;
                let thick = sm.underline_thickness.max(1.0);
                plan.decorations.push(DrawRectCmd {
                    left: left_x,
                    top,
                    right: right_x,
                    bottom: top + thick,
                    color: style_key.color,
                });
            }
            if dec_mask & 2 != 0 {
                let top = lbaseline + sm.strikethru_position;
                let thick = sm.strikethru_thickness.max(1.0);
                plan.decorations.push(DrawRectCmd {
                    left: left_x,
                    top,
                    right: right_x,
                    bottom: top + thick,
                    color: style_key.color,
                });
            }
        }
        let dec_end = plan.decorations.len() as u32;

        let buf_offset = (abs_line_start + i - buffer_base_offset) as i32;
        let count = (inext - i) as i32;
        let effective_mt = if word_spacing_px != 0.0 { 0 } else { mt_ptr };
        plan.runs.push(DrawRunCmd {
            background,
            buffer_index,
            mt_ptr: effective_mt,
            start: buf_offset,
            count,
            context_start: buf_offset,
            context_count: count,
            x: left_x,
            baseline_y: lbaseline,
            is_rtl: run_is_rtl,
            paint_ptr,
            run_flags: sub_flag,
            word_spacing_px,
            decorations_range: dec_start..dec_end,
        });

        x += if run_is_rtl { -total_w } else { total_w };
        i = inext;
    }
    x - original_x
}

// ---- Hit-testing & Cursor Positioning (`Layout.java` + `TextLine.java`) ---------------------

fn line_for_vertical(lines: &[LayoutLine], vertical: i32) -> usize {
    if lines.is_empty() || vertical <= lines[0].top {
        return 0;
    }
    let mut high = lines.len();
    let mut low = 0usize;
    while high - low > 1 {
        let guess = (high + low) / 2;
        if lines[guess].top > vertical {
            high = guess;
        } else {
            low = guess;
        }
    }
    low
}

fn line_for_offset(lines: &[LayoutLine], offset: usize) -> usize {
    if lines.is_empty() {
        return 0;
    }
    let mut high = lines.len();
    let mut low = 0usize;
    while high - low > 1 {
        let guess = (high + low) / 2;
        if lines[guess].start > offset {
            high = guess;
        } else {
            low = guess;
        }
    }
    low
}

fn is_rtl_char_at(line: &LayoutLine, offset: usize) -> bool {
    let dirs = &line.directions;
    let mut i = 0usize;
    while i + 1 < dirs.len() {
        let start = line.start + dirs[i].max(0) as usize;
        let limit = start + ((dirs[i + 1] & RUN_LENGTH_MASK).max(0) as usize);
        if offset >= start && offset < limit {
            return (dirs[i + 1] & RUN_RTL_FLAG) != 0;
        }
        i += 2;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn get_horizontal<S: TextShaper>(
    shaper: &mut S,
    intr: &Intrinsics,
    layout: &ComputedLayout,
    outer_width: i32,
    offset: usize,
    trailing: bool,
    line_idx: usize,
) -> f32 {
    let line = &layout.lines[line_idx];
    let display_chars: &[u16] = layout.ellipsized_chars.as_deref().unwrap_or(&intr.chars);
    let line_end = if line.end > line.start && display_chars[line.end - 1] == 0x000A {
        line.end - 1
    } else {
        line.end
    };
    let word_spacing_px = if is_justification_required(intr, display_chars, line.end) {
        compute_justify_word_spacing(shaper, intr, display_chars, line, outer_width as f32)
    } else {
        0.0
    };
    let rel_offset = offset.saturating_sub(line.start).min(line_end - line.start);
    let wid = measure_offset_on_line(
        shaper,
        &display_chars[line.start..line_end],
        line.start,
        &intr.base,
        &intr.runs,
        line.dir,
        &line.directions,
        line.has_tab,
        word_spacing_px,
        rel_offset,
        trailing,
    );
    let left_pos =
        compute_line_start_x(shaper, intr, layout, outer_width, line_idx, display_chars, line_end);
    (left_pos as f32) + wid
}

/// Port of `TextLine.measure(offset, trailing, ...)` (`TextLine.java:530–650`).
#[allow(clippy::too_many_arguments)]
fn measure_offset_on_line<S: TextShaper>(
    shaper: &mut S,
    line_chars: &[u16],
    abs_line_start: usize,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    line_dir: i32,
    directions: &[i32],
    has_tab: bool,
    word_spacing_px: f32,
    offset: usize,
    trailing: bool,
) -> f32 {
    let m_len = line_chars.len();
    if m_len == 0 {
        return 0.0;
    }
    let target = if trailing { offset.saturating_sub(1) } else { offset };
    if trailing && offset == 0 {
        return 0.0;
    }

    let run_count = directions.len() / 2;
    let mut h = 0.0f32;
    for run_idx in 0..run_count {
        let run_start = (directions[run_idx * 2].max(0) as usize).min(m_len);
        if run_start > m_len {
            break;
        }
        let packed = directions[run_idx * 2 + 1];
        let run_len = (packed & RUN_LENGTH_MASK).max(0) as usize;
        let run_limit = run_start.saturating_add(run_len).min(m_len);
        let run_is_rtl = (packed & RUN_RTL_FLAG) != 0;
        let run_flag = calculate_run_flag(run_idx, run_count, line_dir);

        let mut seg_start = run_start;
        let mut j = if has_tab { run_start } else { run_limit };
        while j <= run_limit {
            if j == run_limit || line_chars[j] == 0x0009 {
                let target_is_in_this_seg = target >= seg_start && target < j;
                let same_dir = (line_dir == DIR_RIGHT_TO_LEFT) == run_is_rtl;
                if target_is_in_this_seg && same_dir {
                    let partial = measure_unidirectional_segment(
                        shaper,
                        line_chars,
                        abs_line_start,
                        base,
                        runs,
                        seg_start,
                        offset,
                        j,
                        run_is_rtl,
                        run_flag,
                        word_spacing_px,
                        None,
                    );
                    return h + partial;
                }

                let full_seg = measure_unidirectional_segment(
                    shaper,
                    line_chars,
                    abs_line_start,
                    base,
                    runs,
                    seg_start,
                    j,
                    j,
                    run_is_rtl,
                    run_flag,
                    word_spacing_px,
                    None,
                );
                let signed_advance = (line_dir as f32) * full_seg.abs();

                if target_is_in_this_seg {
                    let partial = measure_unidirectional_segment(
                        shaper,
                        line_chars,
                        abs_line_start,
                        base,
                        runs,
                        seg_start,
                        offset,
                        j,
                        run_is_rtl,
                        run_flag,
                        word_spacing_px,
                        None,
                    );
                    return h + signed_advance + partial;
                }

                h += signed_advance;
                if j != run_limit {
                    if offset == j {
                        return h;
                    }
                    let unsigned = h * (line_dir as f32);
                    h = (line_dir as f32) * next_tab_stop(unsigned);
                    if target == j {
                        return h;
                    }
                }
                seg_start = j + 1;
            }
            j += 1;
        }
    }
    h
}

/// Port of `Layout.primaryIsTrailingPrevious` (`Layout.java:1584–1631`).
fn primary_is_trailing_previous(line: &LayoutLine, offset: usize) -> bool {
    let line_start = line.start;
    let line_end = line.end;
    let runs = &line.directions;
    let para_level = if line.dir == DIR_LEFT_TO_RIGHT { 0 } else { 1 };

    let mut level_at = -1i32;
    let mut i = 0usize;
    while i + 1 < runs.len() {
        let start = line_start + runs[i].max(0) as usize;
        let limit = (start + ((runs[i + 1] & RUN_LENGTH_MASK).max(0) as usize)).min(line_end);
        if offset >= start && offset < limit {
            if offset > start {
                return false;
            }
            level_at = (runs[i + 1] >> RUN_LEVEL_SHIFT) & RUN_LEVEL_MASK;
            break;
        }
        i += 2;
    }
    if level_at == -1 {
        level_at = para_level;
    }

    let mut level_before = -1i32;
    if offset <= line_start {
        level_before = para_level;
    } else {
        let prev = offset - 1;
        let mut j = 0usize;
        while j + 1 < runs.len() {
            let start = line_start + runs[j].max(0) as usize;
            let limit = (start + ((runs[j + 1] & RUN_LENGTH_MASK).max(0) as usize)).min(line_end);
            if prev >= start && prev < limit {
                level_before = (runs[j + 1] >> RUN_LEVEL_SHIFT) & RUN_LEVEL_MASK;
                break;
            }
            j += 2;
        }
    }
    level_before < level_at
}

/// Port of `Layout.primaryIsTrailingPreviousAllLineOffsets` (`Layout.java:1641–1671`).
fn primary_is_trailing_previous_all_line_offsets(
    line_dir: i32,
    directions: &[i32],
    m_len: usize,
) -> Vec<bool> {
    let mut trailing = vec![false; m_len + 1];
    let mut level = vec![0u8; m_len + 1];
    let para_level = if line_dir == DIR_LEFT_TO_RIGHT { 0u8 } else { 1u8 };

    let mut i = 0usize;
    while i + 1 < directions.len() {
        let start = (directions[i].max(0) as usize).min(m_len);
        let limit =
            start.saturating_add((directions[i + 1] & RUN_LENGTH_MASK).max(0) as usize).min(m_len);
        if limit > start {
            level[limit - 1] = ((directions[i + 1] >> RUN_LEVEL_SHIFT) & RUN_LEVEL_MASK) as u8;
        }
        i += 2;
    }

    let mut j = 0usize;
    while j + 1 < directions.len() {
        let start = (directions[j].max(0) as usize).min(m_len);
        let cur_lvl = ((directions[j + 1] >> RUN_LEVEL_SHIFT) & RUN_LEVEL_MASK) as u8;
        let prev_lvl = if start == 0 { para_level } else { level[start - 1] };
        trailing[start] = cur_lvl > prev_lvl;
        j += 2;
    }
    trailing
}

/// Port of `TextLine.measureAllOffsets` (`TextLine.java:749–820`).
#[allow(clippy::too_many_arguments)]
fn measure_all_offsets_on_line<S: TextShaper>(
    shaper: &mut S,
    line_chars: &[u16],
    abs_line_start: usize,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    line_dir: i32,
    directions: &[i32],
    has_tab: bool,
    word_spacing_px: f32,
    primary: bool,
) -> Vec<f32> {
    let m_len = line_chars.len();
    let mut measurement = vec![0.0f32; m_len + 1];
    if m_len == 0 {
        return measurement;
    }
    let mut trailing = primary_is_trailing_previous_all_line_offsets(line_dir, directions, m_len);
    if !primary {
        for flag in &mut trailing {
            *flag = !*flag;
        }
    }

    let mut advances = vec![0.0f32; m_len];
    let run_count = directions.len() / 2;
    let mut horizontal = 0.0f32;

    for run_idx in 0..run_count {
        let run_start = (directions[run_idx * 2].max(0) as usize).min(m_len);
        if run_start > m_len {
            break;
        }
        let packed = directions[run_idx * 2 + 1];
        let run_len = (packed & RUN_LENGTH_MASK).max(0) as usize;
        let run_limit = run_start.saturating_add(run_len).min(m_len);
        let run_is_rtl = (packed & RUN_RTL_FLAG) != 0;
        let run_flag = calculate_run_flag(run_idx, run_count, line_dir);

        let mut seg_start = run_start;
        let mut j = if has_tab { run_start } else { run_limit };
        while j <= run_limit {
            if j == run_limit || line_chars[j] == 0x0009 {
                let old_h = horizontal;
                let same_dir = (line_dir == DIR_RIGHT_TO_LEFT) == run_is_rtl;
                let prev_seg_end_h = measurement[seg_start];
                let seg_len = j - seg_start;
                let w = measure_unidirectional_segment(
                    shaper,
                    line_chars,
                    abs_line_start,
                    base,
                    runs,
                    seg_start,
                    j,
                    j,
                    run_is_rtl,
                    run_flag,
                    word_spacing_px,
                    Some(&mut advances[..seg_len]),
                );
                horizontal += if same_dir { w } else { -w };

                let mut curr_h = if same_dir { old_h } else { horizontal };
                let seg_limit = j.min(m_len);
                for offset in seg_start..=seg_limit {
                    let advance = if offset < seg_limit {
                        let a = advances[offset - seg_start];
                        if run_is_rtl { -a } else { a }
                    } else {
                        0.0
                    };
                    if offset == seg_start && trailing[offset] {
                        measurement[offset] = prev_seg_end_h;
                    } else if offset != seg_limit || trailing[offset] {
                        measurement[offset] = curr_h;
                    }
                    curr_h += advance;
                }

                if j != run_limit {
                    if !trailing[j] {
                        measurement[j] = horizontal;
                    }
                    let unsigned = horizontal * (line_dir as f32);
                    horizontal = (line_dir as f32) * next_tab_stop(unsigned);
                    if trailing[j + 1] {
                        measurement[j + 1] = horizontal;
                    }
                }
                seg_start = j + 1;
            }
            j += 1;
        }
    }
    if !trailing[m_len] {
        measurement[m_len] = horizontal;
    }
    measurement
}

/// Port of `Layout.getOffsetForHorizontal` (`Layout.java:2263–2335`).
fn offset_for_horizontal<S: TextShaper>(
    shaper: &mut S,
    intr: &Intrinsics,
    layout: &ComputedLayout,
    outer_width: i32,
    line_idx: usize,
    horiz: f32,
) -> usize {
    let line = &layout.lines[line_idx];
    let line_start = line.start;
    let line_end = line.end;
    if line_start == line_end {
        return line_start;
    }
    let display_chars: &[u16] = layout.ellipsized_chars.as_deref().unwrap_or(&intr.chars);
    let measure_end = if line_end > line_start && display_chars[line_end - 1] == 0x000A {
        line_end - 1
    } else {
        line_end
    };
    let word_spacing_px = if is_justification_required(intr, display_chars, line_end) {
        compute_justify_word_spacing(shaper, intr, display_chars, line, outer_width as f32)
    } else {
        0.0
    };
    let left_pos = compute_line_start_x(
        shaper,
        intr,
        layout,
        outer_width,
        line_idx,
        display_chars,
        measure_end,
    ) as f32;

    let rel_offsets = measure_all_offsets_on_line(
        shaper,
        &display_chars[line_start..measure_end],
        line_start,
        &intr.base,
        &intr.runs,
        line.dir,
        &line.directions,
        line.has_tab,
        word_spacing_px,
        true,
    );
    let horiz_at = |abs_off: usize| -> f32 {
        let rel = abs_off.saturating_sub(line_start).min(rel_offsets.len() - 1);
        left_pos + rel_offsets[rel]
    };

    let max = if line_idx + 1 == layout.lines.len() {
        line_end
    } else {
        let to_left = !is_rtl_char_at(line, line_end - 1);
        let walked = get_offset_to_left_right_of(
            shaper,
            &display_chars[line_start..line_end],
            line_start,
            &intr.base,
            &intr.runs,
            line.dir,
            &line.directions,
            word_spacing_px,
            line_end - line_start,
            to_left,
        );
        (line_start as isize + walked).clamp(line_start as isize, line_end as isize) as usize
    };

    let mut best = line_start;
    let mut best_dist = (horiz_at(line_start) - horiz).abs();

    let dirs = &line.directions;
    let mut i = 0usize;
    while i + 1 < dirs.len() {
        let here = line_start + (dirs[i].max(0) as usize);
        let mut there = here + ((dirs[i + 1] & RUN_LENGTH_MASK).max(0) as usize);
        let is_rtl = (dirs[i + 1] & RUN_RTL_FLAG) != 0;
        let swap = if is_rtl { -1.0f32 } else { 1.0f32 };

        if there > max {
            there = max;
        }
        if here < there {
            let mut high = there as isize;
            let mut low = here as isize;
            while high - low > 1 {
                let guess = (high + low) / 2;
                let adguess = get_offset_at_start_of(
                    shaper,
                    &display_chars[line_start..line_end],
                    line_start,
                    &intr.base,
                    &intr.runs,
                    &line.directions,
                    word_spacing_px,
                    guess as usize,
                );
                if horiz_at(adguess) * swap >= horiz * swap {
                    high = guess;
                } else {
                    low = guess;
                }
            }
            if low < (here + 1) as isize {
                low = (here + 1) as isize;
            }
            if (low as usize) < there {
                let aft_rel = get_offset_to_left_right_of(
                    shaper,
                    &display_chars[line_start..line_end],
                    line_start,
                    &intr.base,
                    &intr.runs,
                    line.dir,
                    &line.directions,
                    word_spacing_px,
                    low as usize - line_start,
                    is_rtl,
                );
                let aft =
                    (line_start as isize + aft_rel).clamp(0, display_chars.len() as isize) as usize;
                let low_start_rel = get_offset_to_left_right_of(
                    shaper,
                    &display_chars[line_start..line_end],
                    line_start,
                    &intr.base,
                    &intr.runs,
                    line.dir,
                    &line.directions,
                    word_spacing_px,
                    aft.saturating_sub(line_start),
                    !is_rtl,
                );
                let mut low_cand = (line_start as isize + low_start_rel)
                    .clamp(0, display_chars.len() as isize)
                    as usize;
                if low_cand >= here && low_cand < there {
                    let mut dist = (horiz_at(low_cand) - horiz).abs();
                    if aft < there {
                        let other = (horiz_at(aft) - horiz).abs();
                        if other < dist {
                            dist = other;
                            low_cand = aft;
                        }
                    }
                    if dist < best_dist {
                        best_dist = dist;
                        best = low_cand;
                    }
                }
            }
            let dist = (horiz_at(here) - horiz).abs();
            if dist < best_dist {
                best_dist = dist;
                best = here;
            }
        }
        i += 2;
    }

    let dist = (horiz_at(max) - horiz).abs();
    if dist <= best_dist {
        best = max;
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn get_offset_at_start_of<S: TextShaper>(
    shaper: &mut S,
    line_chars: &[u16],
    abs_line_start: usize,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    directions: &[i32],
    word_spacing_px: f32,
    abs_offset: usize,
) -> usize {
    if abs_offset == 0 {
        return 0;
    }
    let rel = abs_offset.saturating_sub(abs_line_start);
    if rel > 0 && rel < line_chars.len() {
        let c = line_chars[rel];
        if (0xDC00..=0xDFFF).contains(&c) && (0xD800..=0xDBFF).contains(&line_chars[rel - 1]) {
            return abs_offset - 1;
        }
    }
    if rel < line_chars.len() {
        let mut i = 0usize;
        while i + 1 < directions.len() {
            let rs = directions[i].max(0) as usize;
            let rl = (directions[i + 1] & RUN_LENGTH_MASK).max(0) as usize;
            let rlim = (rs + rl).min(line_chars.len());
            if rel > rs && rel < rlim {
                let run_rtl = (directions[i + 1] & RUN_RTL_FLAG) != 0;
                let stepped = get_offset_before_after_in_run(
                    shaper,
                    line_chars,
                    abs_line_start,
                    base,
                    runs,
                    rs,
                    rlim,
                    run_rtl,
                    rel,
                    true,
                    word_spacing_px,
                );
                let back = get_offset_before_after_in_run(
                    shaper,
                    line_chars,
                    abs_line_start,
                    base,
                    runs,
                    rs,
                    rlim,
                    run_rtl,
                    stepped,
                    false,
                    word_spacing_px,
                );
                return abs_line_start + back;
            }
            i += 2;
        }
    }
    abs_offset
}

/// Port of `TextLine.getOffsetToLeftRightOf` (`TextLine.java:932–1092`).
#[allow(clippy::too_many_arguments)]
fn get_offset_to_left_right_of<S: TextShaper>(
    shaper: &mut S,
    line_chars: &[u16],
    abs_line_start: usize,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    line_dir: i32,
    directions: &[i32],
    word_spacing_px: f32,
    cursor: usize,
    to_left: bool,
) -> isize {
    let line_end = line_chars.len();
    let para_is_rtl = line_dir == DIR_RIGHT_TO_LEFT;

    let mut run_index: isize;
    let mut run_level = 0i32;
    let mut run_start = 0usize;
    let mut run_limit = line_end;
    let mut new_caret: isize = -1;
    let mut trailing = false;

    if cursor == 0 {
        run_index = -2;
    } else if cursor == line_end {
        run_index = directions.len() as isize;
    } else {
        run_index = 0;
        while (run_index as usize) + 1 < directions.len() {
            let idx = run_index as usize;
            run_start = directions[idx].max(0) as usize;
            if cursor >= run_start {
                run_limit = (run_start + ((directions[idx + 1] & RUN_LENGTH_MASK).max(0) as usize))
                    .min(line_end);
                if cursor < run_limit {
                    run_level = (directions[idx + 1] >> RUN_LEVEL_SHIFT) & RUN_LEVEL_MASK;
                    if cursor == run_start {
                        let pos = cursor - 1;
                        let mut prev_idx = 0usize;
                        while prev_idx + 1 < directions.len() {
                            let ps = directions[prev_idx].max(0) as usize;
                            if pos >= ps {
                                let plim = (ps
                                    + ((directions[prev_idx + 1] & RUN_LENGTH_MASK).max(0)
                                        as usize))
                                    .min(line_end);
                                if pos < plim {
                                    let plvl = (directions[prev_idx + 1] >> RUN_LEVEL_SHIFT)
                                        & RUN_LEVEL_MASK;
                                    if plvl < run_level {
                                        run_index = prev_idx as isize;
                                        run_level = plvl;
                                        run_start = ps;
                                        run_limit = plim;
                                        trailing = true;
                                        break;
                                    }
                                }
                            }
                            prev_idx += 2;
                        }
                    }
                    break;
                }
            }
            run_index += 2;
        }

        if (run_index as usize) < directions.len() {
            let run_is_rtl = (run_level & 1) != 0;
            let advance = to_left == run_is_rtl;
            let bound = if advance { run_limit } else { run_start };
            if cursor != bound || advance != trailing {
                let stepped = get_offset_before_after_in_run(
                    shaper,
                    line_chars,
                    abs_line_start,
                    base,
                    runs,
                    run_start,
                    run_limit,
                    run_is_rtl,
                    cursor,
                    advance,
                    word_spacing_px,
                );
                new_caret = stepped as isize;
                if stepped != bound {
                    return new_caret;
                }
            }
        }
    }

    loop {
        let advance_para = to_left == para_is_rtl;
        let other_run_index = run_index + if advance_para { 2 } else { -2 };
        if other_run_index >= 0 && (other_run_index as usize) + 1 < directions.len() {
            let oidx = other_run_index as usize;
            let other_start = directions[oidx].max(0) as usize;
            let other_limit = (other_start
                + ((directions[oidx + 1] & RUN_LENGTH_MASK).max(0) as usize))
                .min(line_end);
            let other_level = (directions[oidx + 1] >> RUN_LEVEL_SHIFT) & RUN_LEVEL_MASK;
            let other_rtl = (other_level & 1) != 0;
            let advance = to_left == other_rtl;
            if new_caret == -1 {
                let start_off = if advance { other_start } else { other_limit };
                let stepped = get_offset_before_after_in_run(
                    shaper,
                    line_chars,
                    abs_line_start,
                    base,
                    runs,
                    other_start,
                    other_limit,
                    other_rtl,
                    start_off,
                    advance,
                    word_spacing_px,
                );
                new_caret = stepped as isize;
                let end_bound = if advance { other_limit } else { other_start };
                if stepped == end_bound {
                    run_index = other_run_index;
                    run_level = other_level;
                    continue;
                }
                break;
            }
            if other_level < run_level {
                new_caret = (if advance { other_start } else { other_limit }) as isize;
            }
            break;
        }

        if new_caret == -1 {
            new_caret = if advance_para { line_end as isize + 1 } else { -1 };
            break;
        }
        if new_caret <= line_end as isize {
            new_caret = if advance_para { line_end as isize } else { 0 };
        }
        break;
    }

    new_caret
}

#[allow(clippy::too_many_arguments)]
fn get_offset_before_after_in_run<S: TextShaper>(
    shaper: &mut S,
    line_chars: &[u16],
    abs_line_start: usize,
    base: &DecodedStyle,
    runs: &[DecodedRun],
    run_start: usize,
    run_limit: usize,
    run_is_rtl: bool,
    offset: usize,
    after: bool,
    word_spacing_px: f32,
) -> usize {
    let m_len = line_chars.len();
    if offset == if after { m_len } else { 0 } {
        return step_utf16_codepoint(line_chars, offset, after);
    }
    if run_start >= run_limit {
        return offset;
    }
    let target = if after { offset + 1 } else { offset };
    let mut span_start = run_start;
    let mut span_limit = run_limit;
    if !runs.is_empty() {
        loop {
            span_limit = next_metric_transition(
                runs,
                abs_line_start + span_start,
                abs_line_start + run_limit,
            ) - abs_line_start;
            if span_limit >= target || span_limit >= run_limit {
                break;
            }
            span_start = span_limit;
        }
    }
    let paint_ptr = resolve_span_paint(
        shaper,
        base,
        runs,
        abs_line_start + span_start,
        abs_line_start + span_limit,
    );
    shaper.get_text_run_cursor(
        paint_ptr,
        line_chars,
        span_start,
        span_limit,
        run_is_rtl,
        offset,
        after,
        word_spacing_px,
    )
}

#[inline]
fn step_utf16_codepoint(chars: &[u16], offset: usize, after: bool) -> usize {
    if after {
        if offset >= chars.len() {
            return chars.len() + 1;
        }
        let c = chars[offset];
        if (0xD800..=0xDBFF).contains(&c)
            && offset + 1 < chars.len()
            && (0xDC00..=0xDFFF).contains(&chars[offset + 1])
        {
            offset + 2
        } else {
            offset + 1
        }
    } else {
        if offset == 0 {
            return 0;
        }
        let c = chars[offset - 1];
        if (0xDC00..=0xDFFF).contains(&c)
            && offset >= 2
            && (0xD800..=0xDBFF).contains(&chars[offset - 2])
        {
            offset - 2
        } else {
            offset - 1
        }
    }
}
