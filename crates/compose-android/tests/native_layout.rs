//! Integration tests for the native Rust `BoringLayout` + `StaticLayout` engine
//! (`compose_android::text_layout::NativeLayoutEngine`) wired through `AndroidTextMeasurer`.

#![forbid(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_android::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_android::text_layout::{
    ComplexParagraphDraw, DIR_LEFT_TO_RIGHT, DIR_RIGHT_TO_LEFT, FontMetricsInt, LineBreakItem,
    LineDirections, MeasuredTextHandle, NativeLayoutEngine, PaintStyleKey, ParagraphBidi,
    RUN_LENGTH_MASK, RUN_LEVEL_MASK, RUN_LEVEL_SHIFT, RUN_RTL_FLAG, StrokeMetrics, StyleRunSpec,
    TextShaper,
};
use compose_ui::{
    AnnotatedString, Color, Constraints, Density, LineHeightAlignment, LineHeightMode,
    LineHeightStyle, LineHeightTrim, Offset, Rect, SpanRange, SpanStyle, TextAlign, TextDecoration,
    TextOverflow, TextStyle, TextUnit,
    platform::{TextLayoutOptions, TextMeasurer, TextPosition},
};

#[derive(Clone)]
struct MockMt {
    widths: Vec<f32>,
    ascents: Vec<i32>,
    descents: Vec<i32>,
}

#[derive(Clone, Default)]
struct ShaperCounters {
    boring_measures: Rc<Cell<usize>>,
    mt_builds: Rc<Cell<usize>>,
    line_breaks: Rc<Cell<usize>>,
}

struct MockShaper {
    next_paint: i64,
    paints_by_key: HashMap<PaintStyleKey, i64>,
    keys_by_paint: HashMap<i64, PaintStyleKey>,
    next_mt: i64,
    mts: HashMap<i64, MockMt>,
    counters: ShaperCounters,
}

impl MockShaper {
    fn new(counters: ShaperCounters) -> Self {
        Self {
            next_paint: 1,
            paints_by_key: HashMap::new(),
            keys_by_paint: HashMap::new(),
            next_mt: 1,
            mts: HashMap::new(),
            counters,
        }
    }

    fn key(&self, paint_ptr: i64) -> Option<&PaintStyleKey> {
        self.keys_by_paint.get(&paint_ptr)
    }

    fn char_advance(&self, paint_ptr: i64, ch: u16, word_spacing_px: f32) -> f32 {
        // Low surrogates carry zero width; the high surrogate carries the cluster's width.
        if (0xDC00..=0xDFFF).contains(&ch) {
            return 0.0;
        }
        // Control/zero-width marks and newline carry zero advance.
        if matches!(ch, 0x000A | 0x200B | 0x200E | 0x200F | 0xFEFF) {
            return 0.0;
        }
        let Some(k) = self.keys_by_paint.get(&paint_ptr) else {
            return 8.0;
        };
        let base = k.font_px() * 0.5 + k.letter_spacing_px();
        if ch == u16::from(b' ') { base + word_spacing_px } else { base }
    }

    fn metrics_for_font(font_px: f32) -> FontMetricsInt {
        FontMetricsInt {
            top: -(font_px * 0.9).round() as i32,
            ascent: -(font_px * 0.8).round() as i32,
            descent: (font_px * 0.2).round() as i32,
            bottom: (font_px * 0.3).round() as i32,
            leading: 0,
        }
    }
}

fn is_rtl_code_unit(ch: u16) -> Option<bool> {
    if (0x0590..=0x08FF).contains(&ch) || ch == 0x200F {
        Some(true)
    } else if ch == 0x200E || (ch <= 0x7F && (ch as u8).is_ascii_alphanumeric()) {
        Some(false)
    } else {
        None
    }
}

impl TextShaper for MockShaper {
    fn resolve_paint(&mut self, style: &PaintStyleKey) -> i64 {
        if let Some(&id) = self.paints_by_key.get(style) {
            return id;
        }
        let id = self.next_paint;
        self.next_paint += 1;
        self.paints_by_key.insert(style.clone(), id);
        self.keys_by_paint.insert(id, style.clone());
        id
    }

    fn font_metrics_int(&mut self, paint_ptr: i64) -> FontMetricsInt {
        let font_px = self.keys_by_paint.get(&paint_ptr).map_or(16.0, PaintStyleKey::font_px);
        Self::metrics_for_font(font_px)
    }

    fn stroke_metrics(&mut self, paint_ptr: i64) -> StrokeMetrics {
        let font_px = self.keys_by_paint.get(&paint_ptr).map_or(16.0, PaintStyleKey::font_px);
        StrokeMetrics {
            underline_position: 2.0,
            underline_thickness: 1.0,
            strikethru_position: -font_px * 0.3,
            strikethru_thickness: 1.0,
        }
    }

    fn font_metrics_int_for_text(
        &mut self,
        paint_ptr: i64,
        _chars: &[u16],
        _start: usize,
        _end: usize,
        _ctx_start: usize,
        _ctx_end: usize,
        _is_rtl: bool,
    ) -> FontMetricsInt {
        self.font_metrics_int(paint_ptr)
    }

    fn measure_boring_run(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        run_flags: i32,
    ) -> (f32, FontMetricsInt) {
        self.counters.boring_measures.set(self.counters.boring_measures.get() + 1);
        let fmi = self.font_metrics_int(paint_ptr);
        let w = self.get_run_advance(
            paint_ptr, chars, start, end, start, end, false, end, run_flags, 0.0, None,
        );
        (w, fmi)
    }

    fn analyze_bidi(&mut self, chars: &[u16], rtl: bool) -> ParagraphBidi {
        let base_level = u8::from(rtl);
        let paragraph_dir = if rtl { DIR_RIGHT_TO_LEFT } else { DIR_LEFT_TO_RIGHT };
        if chars.is_empty() {
            return ParagraphBidi { paragraph_dir, levels: Vec::new(), rtl_request: rtl };
        }
        let mut levels = vec![base_level; chars.len()];
        for (i, &ch) in chars.iter().enumerate() {
            match is_rtl_code_unit(ch) {
                Some(true) => levels[i] = 1,
                Some(false) => levels[i] = if rtl { 2 } else { 0 },
                None => {
                    let prev = (0..i).rev().find_map(|p| is_rtl_code_unit(chars[p]));
                    let next = ((i + 1)..chars.len()).find_map(|n| is_rtl_code_unit(chars[n]));
                    levels[i] = match (prev, next) {
                        (Some(true), Some(true)) => 1,
                        (Some(false), Some(false)) => {
                            if rtl {
                                2
                            } else {
                                0
                            }
                        }
                        _ => base_level,
                    };
                }
            }
        }
        ParagraphBidi { paragraph_dir, levels, rtl_request: rtl }
    }

    fn line_directions(
        &mut self,
        bidi: &ParagraphBidi,
        _chars: &[u16],
        start: usize,
        end: usize,
    ) -> LineDirections {
        if start >= end || bidi.levels.is_empty() {
            let flag = if bidi.paragraph_dir == DIR_RIGHT_TO_LEFT { RUN_RTL_FLAG } else { 0 };
            return LineDirections::Single([
                0,
                ((end.saturating_sub(start)) as i32 & RUN_LENGTH_MASK) | flag,
            ]);
        }
        let slice = &bidi.levels[start..end];
        let mut runs: Vec<(i32, i32, u8)> = Vec::new();
        let mut run_start = 0usize;
        let mut cur_lvl = slice[0];
        for (idx, &lvl) in slice.iter().enumerate().skip(1) {
            if lvl != cur_lvl {
                runs.push((run_start as i32, (idx - run_start) as i32, cur_lvl));
                run_start = idx;
                cur_lvl = lvl;
            }
        }
        runs.push((run_start as i32, (slice.len() - run_start) as i32, cur_lvl));

        if runs.len() == 1 {
            let (_, len, lvl) = runs[0];
            let packed =
                ((i32::from(lvl) & RUN_LEVEL_MASK) << RUN_LEVEL_SHIFT) | (len & RUN_LENGTH_MASK);
            return LineDirections::Single([0, packed]);
        }

        let max_lvl = runs.iter().map(|r| r.2).max().unwrap_or(0);
        let min_odd = runs.iter().map(|r| r.2).filter(|l| l % 2 == 1).min().unwrap_or(1);
        let mut order: Vec<usize> = (0..runs.len()).collect();
        for lvl in (min_odd..=max_lvl).rev() {
            let mut i = 0;
            while i < order.len() {
                if runs[order[i]].2 >= lvl {
                    let mut j = i + 1;
                    while j < order.len() && runs[order[j]].2 >= lvl {
                        j += 1;
                    }
                    order[i..j].reverse();
                    i = j;
                } else {
                    i += 1;
                }
            }
        }

        let mut ld = Vec::with_capacity(order.len() * 2);
        for idx in order {
            let (s, len, lvl) = runs[idx];
            ld.push(s);
            ld.push(
                ((i32::from(lvl) & RUN_LEVEL_MASK) << RUN_LEVEL_SHIFT) | (len & RUN_LENGTH_MASK),
            );
        }
        LineDirections::Multi(ld.into_boxed_slice())
    }

    fn build_measured_text(&mut self, chars: &[u16], runs: &[StyleRunSpec]) -> MeasuredTextHandle {
        self.counters.mt_builds.set(self.counters.mt_builds.get() + 1);
        let mut widths = vec![0.0f32; chars.len()];
        let mut ascents = vec![-12i32; chars.len()];
        let mut descents = vec![4i32; chars.len()];
        for r in runs {
            let fmi = self.font_metrics_int(r.paint_ptr);
            for idx in r.start..r.end.min(chars.len()) {
                widths[idx] = self.char_advance(r.paint_ptr, chars[idx], 0.0);
                ascents[idx] = fmi.ascent;
                descents[idx] = fmi.descent;
            }
        }
        let id = self.next_mt;
        self.next_mt += 1;
        self.mts.insert(id, MockMt { widths, ascents, descents });
        MeasuredTextHandle { ptr: id, owner: None }
    }

    fn mt_width(&self, mt: &MeasuredTextHandle, start: usize, end: usize) -> f32 {
        let Some(m) = self.mts.get(&mt.ptr) else { return 0.0 };
        m.widths[start..end.min(m.widths.len())].iter().sum()
    }

    fn mt_char_width_at(&self, mt: &MeasuredTextHandle, offset: usize) -> f32 {
        self.mts.get(&mt.ptr).and_then(|m| m.widths.get(offset).copied()).unwrap_or(0.0)
    }

    fn compute_line_breaks(
        &mut self,
        mt: &MeasuredTextHandle,
        chars: &[u16],
        width: f32,
        _justified: bool,
        out: &mut Vec<LineBreakItem>,
    ) {
        self.counters.line_breaks.set(self.counters.line_breaks.get() + 1);
        out.clear();
        let Some(m) = self.mts.get(&mt.ptr) else { return };
        if chars.is_empty() {
            return;
        }
        let max_w = width.max(0.0);
        let mut pos = 0usize;
        while pos < chars.len() {
            let mut cur_w = 0.0f32;
            let mut last_space: Option<usize> = None;
            let mut end = pos;
            while end < chars.len() {
                let cw = m.widths[end];
                if chars[end] == u16::from(b' ') {
                    last_space = Some(end + 1);
                }
                if end > pos && chars[end] != u16::from(b' ') && cur_w + cw > max_w {
                    break;
                }
                cur_w += cw;
                end += 1;
            }
            let brk = if end < chars.len() {
                if let Some(sp) = last_space.filter(|&sp| sp > pos) { sp } else { end.max(pos + 1) }
            } else {
                chars.len()
            };
            let mut vis_end = brk;
            while vis_end > pos && chars[vis_end - 1] == u16::from(b' ') {
                vis_end -= 1;
            }
            let line_w: f32 = m.widths[pos..vis_end].iter().sum();
            let asc = m.ascents[pos..brk].iter().copied().min().unwrap_or(-12) as f32;
            let desc = m.descents[pos..brk].iter().copied().max().unwrap_or(4) as f32;
            let has_tab = chars[pos..brk].contains(&u16::from(b'\t'));
            out.push(LineBreakItem {
                break_offset: brk,
                width: line_w,
                ascent: asc,
                descent: desc,
                flags: if has_tab { 0x2000_0000 } else { 0 },
            });
            pos = brk;
        }
    }

    fn get_run_advance(
        &mut self,
        paint_ptr: i64,
        chars: &[u16],
        start: usize,
        end: usize,
        _ctx_start: usize,
        _ctx_end: usize,
        _is_rtl: bool,
        offset: usize,
        _run_flags: i32,
        word_spacing_px: f32,
        out_advances: Option<&mut [f32]>,
    ) -> f32 {
        let clamped_end = end.min(chars.len());
        let clamped_off = offset.clamp(start, clamped_end);
        if let Some(advs) = out_advances {
            for (dst, &ch) in advs.iter_mut().zip(&chars[start..clamped_end]) {
                *dst = self.char_advance(paint_ptr, ch, word_spacing_px);
            }
        }
        chars[start..clamped_off]
            .iter()
            .map(|&ch| self.char_advance(paint_ptr, ch, word_spacing_px))
            .sum()
    }

    fn get_text_run_cursor(
        &mut self,
        _paint_ptr: i64,
        chars: &[u16],
        ctx_start: usize,
        ctx_end: usize,
        _is_rtl: bool,
        offset: usize,
        after: bool,
        _word_spacing_px: f32,
    ) -> usize {
        if after {
            if offset >= ctx_end {
                return ctx_end;
            }
            let step = if offset + 1 < ctx_end
                && (0xD800..=0xDBFF).contains(&chars[offset])
                && (0xDC00..=0xDFFF).contains(&chars[offset + 1])
            {
                2
            } else {
                1
            };
            (offset + step).min(ctx_end)
        } else {
            if offset <= ctx_start {
                return ctx_start;
            }
            let step = if offset >= ctx_start + 2
                && (0xD800..=0xDBFF).contains(&chars[offset - 2])
                && (0xDC00..=0xDFFF).contains(&chars[offset - 1])
            {
                2
            } else {
                1
            };
            offset.saturating_sub(step).max(ctx_start)
        }
    }
}

#[derive(Clone, Default)]
struct RecordedPlans {
    simple: Rc<RefCell<HashMap<u32, (i64, f32)>>>,
    complex: Rc<RefCell<HashMap<u32, Rc<ComplexParagraphDraw>>>>,
}

struct MockNativeBackend {
    engine: Rc<RefCell<NativeLayoutEngine<MockShaper>>>,
    plans: RecordedPlans,
}

impl MockNativeBackend {
    fn new() -> (Self, ShaperCounters, RecordedPlans, Rc<RefCell<NativeLayoutEngine<MockShaper>>>) {
        let counters = ShaperCounters::default();
        let plans = RecordedPlans::default();
        let engine =
            Rc::new(RefCell::new(NativeLayoutEngine::new(MockShaper::new(counters.clone()))));
        (Self { engine: engine.clone(), plans: plans.clone() }, counters, plans, engine)
    }

    fn store_draw(
        &self,
        handle: u32,
        simple_ptr: i64,
        baseline: f32,
        complex: Option<Rc<ComplexParagraphDraw>>,
    ) {
        self.plans.simple.borrow_mut().remove(&handle);
        self.plans.complex.borrow_mut().remove(&handle);
        if simple_ptr != 0 {
            self.plans.simple.borrow_mut().insert(handle, (simple_ptr, baseline));
        } else if let Some(plan) = complex {
            self.plans.complex.borrow_mut().insert(handle, plan);
        }
    }
}

impl TextBackend for MockNativeBackend {
    fn caches_released(&self) -> bool {
        true
    }

    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut {
        let is_probe = request.first().is_some_and(|&f| (f & 2) != 0);
        let Some(out) = self.engine.borrow_mut().layout(handle, text, request) else {
            panic!("decode_request must succeed");
        };
        if !is_probe {
            self.store_draw(handle, out.simple_paint_ptr, out.simple_baseline, out.complex_draw);
        }
        out.laid_out
    }

    fn relayout(
        &mut self,
        previous: u32,
        handle: u32,
        _text: &str,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<LaidOut> {
        let out = self
            .engine
            .borrow_mut()
            .relayout(previous, handle, min_width, max_width, max_height)?;
        self.plans.simple.borrow_mut().remove(&previous);
        self.plans.complex.borrow_mut().remove(&previous);
        self.store_draw(handle, out.simple_paint_ptr, out.simple_baseline, out.complex_draw);
        Some(out.laid_out)
    }

    fn relayout_in_place(
        &mut self,
        handle: u32,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<LaidOut> {
        self.engine.borrow_mut().relayout_in_place(handle, min_width, max_width, max_height)
    }

    fn probe_layout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
        max_height: i32,
    ) -> Option<LaidOut> {
        self.engine
            .borrow_mut()
            .probe_layout(previous, handle, min_width, max_width, max_height)
            .map(|o| o.laid_out)
    }

    fn restyle(
        &mut self,
        previous: u32,
        handle: u32,
        _text: &str,
        color: u32,
        decoration: u8,
        has_extras: bool,
    ) -> bool {
        if has_extras {
            return false;
        }
        let Some(out) = self.engine.borrow_mut().restyle(previous, handle, color, decoration)
        else {
            return false;
        };
        self.plans.simple.borrow_mut().remove(&previous);
        self.plans.complex.borrow_mut().remove(&previous);
        self.store_draw(handle, out.simple_paint_ptr, out.simple_baseline, out.complex_draw);
        true
    }

    fn hit_test(&self, handle: u32, position: Offset) -> usize {
        self.engine.borrow().hit_test(handle, position)
    }

    fn cursor(&self, handle: u32, offset: usize) -> Rect {
        self.engine.borrow().cursor(handle, offset)
    }

    fn release(&mut self, handle: u32) {
        self.engine.borrow_mut().release(handle);
        self.plans.simple.borrow_mut().remove(&handle);
        self.plans.complex.borrow_mut().remove(&handle);
    }
}

#[test]
fn boring_layout_and_static_layout_produce_identical_single_line_metrics_and_cursors() {
    let (backend_b, counters_b, plans_b, _) = MockNativeBackend::new();
    let mut measurer_boring = AndroidTextMeasurer::new(backend_b);

    let (backend_s, counters_s, _, _) = MockNativeBackend::new();
    let mut measurer_static = AndroidTextMeasurer::new(backend_s);

    let text = "Hello Compose";
    let static_text = "Hello Compose\u{200E}";
    let style = TextStyle { font_size: TextUnit::sp(20.0), ..TextStyle::DEFAULT };
    let density = Density::new(1.0);
    let constraints = Constraints::new(0, 300, 0, 500);
    let opts = TextLayoutOptions::default();

    // Plain single-line LTR takes the `BoringLayout` fast path (zero `MeasuredText`!).
    let boring_h = measurer_boring.layout(text, &style, opts, constraints, &density);
    assert_eq!(counters_b.boring_measures.get(), 1);
    assert_eq!(counters_b.mt_builds.get(), 0);
    assert!(plans_b.simple.borrow().contains_key(&(boring_h.0 as u32)));

    // Trailing U+200E (zero-width LTR mark) bypasses `BoringLayout` (`could_affect_rtl`) and exercises `StaticLayout`.
    let static_h = measurer_static.layout(static_text, &style, opts, constraints, &density);
    assert_eq!(counters_s.mt_builds.get(), 1);

    assert_eq!(measurer_boring.metrics(boring_h), measurer_static.metrics(static_h));
    assert!(!measurer_boring.did_exceed_max_lines(boring_h));
    assert!(!measurer_static.did_exceed_max_lines(static_h));

    for byte_off in 0..=text.len() {
        let cb = measurer_boring.cursor_rect(boring_h, byte_off);
        let cs = measurer_static.cursor_rect(static_h, byte_off);
        assert_eq!(cb, cs, "cursor mismatch at byte {byte_off}");
    }

    for step in 0..=24 {
        let x = step as f32 * 5.0;
        let hb = measurer_boring.hit_test(boring_h, Offset::new(x, 10.0));
        let hs = measurer_static.hit_test(static_h, Offset::new(x, 10.0));
        assert_eq!(hb, hs, "hit-test mismatch at x={x}");
    }
    assert_eq!(measurer_boring.hit_test(boring_h, Offset::new(135.0, 10.0)).offset, text.len());
    assert_eq!(
        measurer_static.hit_test(static_h, Offset::new(135.0, 10.0)).offset,
        static_text.len()
    );
}

#[test]
fn line_height_style_trims_and_top_ratios_adjust_baselines_and_height() {
    let (backend, _, _, _) = MockNativeBackend::new();
    let mut measurer = AndroidTextMeasurer::new(backend);
    let density = Density::new(1.0);
    let opts = TextLayoutOptions::default();

    // 20px font -> raw ascent = -16, descent = 4 (raw height = 20).
    // Line height = 30px -> extra = 10px.
    // TopRatio 0.5 -> ascent_delta = 5 -> adjusted ascent = -21, descent = 9.
    let untrimmed_style = TextStyle {
        font_size: TextUnit::sp(20.0),
        line_height: TextUnit::sp(30.0),
        line_height_style: Some(LineHeightStyle {
            alignment: LineHeightAlignment::CENTER,
            trim: LineHeightTrim::None,
            mode: LineHeightMode::Fixed,
        }),
        ..TextStyle::DEFAULT
    };
    let single_untrimmed =
        measurer.layout("Hi", &untrimmed_style, opts, Constraints::new(0, 80, 0, 500), &density);
    let m_untrimmed = measurer.metrics(single_untrimmed);
    assert_eq!(m_untrimmed.size.height, 30);
    assert_eq!(m_untrimmed.first_baseline, 21.0);

    // Both trimmed on a single line -> raw height 20, baseline 16.
    let trimmed_style = TextStyle {
        line_height_style: Some(LineHeightStyle {
            alignment: LineHeightAlignment::CENTER,
            trim: LineHeightTrim::Both,
            mode: LineHeightMode::Fixed,
        }),
        ..untrimmed_style
    };
    let single_trimmed =
        measurer.layout("Hi", &trimmed_style, opts, Constraints::new(0, 80, 0, 500), &density);
    let m_trimmed = measurer.metrics(single_trimmed);
    assert_eq!(m_trimmed.size.height, 20);
    assert_eq!(m_trimmed.first_baseline, 16.0);

    // Two wrapped lines ("aaaa bbbb" at 20px -> 10px/char -> 40px per word, wraps at 60px):
    // Line 0 trims top (ascent -16, descent 9 -> height 25, baseline 16).
    // Line 1 trims bottom (ascent -21, descent 4 -> height 25, baseline 16 + 9 + 21 = 46, total height 50).
    let two_trimmed = measurer.layout(
        "aaaa bbbb",
        &trimmed_style,
        opts,
        Constraints::new(0, 60, 0, 500),
        &density,
    );
    let m_two = measurer.metrics(two_trimmed);
    assert_eq!(m_two.line_count, 2);
    assert_eq!(m_two.first_baseline, 16.0);
    assert_eq!(m_two.last_baseline, 46.0);
    assert_eq!(m_two.size.height, 50);
}

#[test]
fn empty_and_trailing_newline_paragraphs_apply_last_line_metrics() {
    let (backend, _, _, _) = MockNativeBackend::new();
    let mut measurer = AndroidTextMeasurer::new(backend);
    let density = Density::new(1.0);
    let opts = TextLayoutOptions::default();
    let style = TextStyle {
        font_size: TextUnit::sp(20.0),
        line_height: TextUnit::sp(30.0),
        line_height_style: Some(LineHeightStyle {
            alignment: LineHeightAlignment::CENTER,
            trim: LineHeightTrim::None,
            mode: LineHeightMode::Fixed,
        }),
        ..TextStyle::DEFAULT
    };

    let empty = measurer.layout("", &style, opts, Constraints::new(0, 200, 0, 200), &density);
    let m_empty = measurer.metrics(empty);
    assert_eq!(m_empty.line_count, 1);
    assert_eq!(m_empty.size.height, 30);
    assert_eq!(m_empty.first_baseline, 21.0);

    let trailing_nl =
        measurer.layout("Hi\n", &style, opts, Constraints::new(0, 200, 0, 200), &density);
    let m_nl = measurer.metrics(trailing_nl);
    assert_eq!(m_nl.line_count, 2);
    assert_eq!(m_nl.size.height, 60);
    assert_eq!(m_nl.first_baseline, 21.0);
    assert_eq!(m_nl.last_baseline, 51.0);
}

#[test]
fn ellipsis_merges_soft_wrapped_overflow_lines_to_fill_available_width() {
    let (backend, _, plans, _) = MockNativeBackend::new();
    let mut measurer = AndroidTextMeasurer::new(backend);
    let density = Density::new(1.0);
    // 20px font -> 10px per char (including '…').
    let style = TextStyle { font_size: TextUnit::sp(20.0), ..TextStyle::DEFAULT };
    let opts = TextLayoutOptions {
        max_lines: 1,
        overflow: TextOverflow::Ellipsis,
        ..TextLayoutOptions::default()
    };

    // "alpha beta gamma":
    // Without StaticLayout overflow-line merging, LineBreaker at width=95 breaks after "alpha "
    // (50px <= 95px, since "alpha beta" = 100px > 95px), leaving line 0 as only "alpha " before
    // ellipsizing! With overflow-line merging (`StaticLayout.java:974-996`), line 0 merges all
    // remaining text and truncates at 95px -> 8 chars ("alpha be") + "…" = 90px <= 95px!
    let handle = measurer.layout(
        "alpha beta gamma",
        &style,
        opts,
        Constraints::new(0, 95, 0, 200),
        &density,
    );
    assert!(measurer.did_exceed_max_lines(handle));
    assert_eq!(measurer.metrics(handle).line_count, 1);

    let complex = plans.complex.borrow();
    let Some(plan) = complex.get(&(handle.0 as u32)) else {
        panic!("expected ComplexParagraphDraw for ellipsized line");
    };
    assert_eq!(plan.runs.len(), 1);
    let buf = &plan.buffers[plan.runs[0].buffer_index as usize];
    let prefix: String = String::from_utf16_lossy(&buf[..8]);
    assert_eq!(prefix, "alpha be");
    assert_eq!(buf[8], 0x2026);
    assert_eq!(buf[9], 0xFEFF);
}

#[test]
fn annotated_spans_and_restyle_preserve_explicit_colors_decorations_and_backgrounds() {
    let (backend, _, plans, engine) = MockNativeBackend::new();
    let mut measurer = AndroidTextMeasurer::new(backend);
    let density = Density::new(1.0);
    let opts = TextLayoutOptions::default();

    let annotated = AnnotatedString::with_styles(
        "base red bg",
        [
            // "red" [5..8): explicit RED color and underline.
            SpanRange::new(
                SpanStyle {
                    color: Color::RED,
                    text_decoration: Some(TextDecoration::UNDERLINE),
                    ..SpanStyle::DEFAULT
                },
                5,
                8,
            ),
            // "bg" [9..11): inherits color/decoration, sets BLUE background and 24px font size.
            SpanRange::new(
                SpanStyle {
                    font_size: TextUnit::sp(24.0),
                    background: Color::BLUE,
                    ..SpanStyle::DEFAULT
                },
                9,
                11,
            ),
        ],
        [],
    );

    let base_style =
        TextStyle { color: Color::BLACK, font_size: TextUnit::sp(16.0), ..TextStyle::DEFAULT };
    let initial = measurer.layout_annotated(
        &annotated,
        &base_style,
        opts,
        Constraints::new(0, 300, 0, 200),
        &density,
    );
    let initial_metrics = measurer.metrics(initial);

    // Restyle base paragraph to GREEN + LINE_THROUGH:
    // - "base " [0..5), " " [8..9), and "bg" [9..11) inherit GREEN + LINE_THROUGH!
    // - "red" [5..8) keeps explicit RED + UNDERLINE!
    // - "bg" [9..11) keeps BLUE background!
    let restyled_style = TextStyle {
        color: Color::GREEN,
        text_decoration: Some(TextDecoration::LINE_THROUGH),
        ..base_style
    };
    let Some(restyled) = measurer.restyle(initial, &restyled_style) else {
        panic!("expected restyle to succeed");
    };
    assert_eq!(measurer.metrics(restyled), initial_metrics);

    let complex = plans.complex.borrow();
    let Some(plan) = complex.get(&(restyled.0 as u32)) else {
        panic!("expected complex draw plan for multi-span paragraph");
    };
    assert_eq!(plan.runs.len(), 4);

    engine.borrow().with_shaper_mut(|shaper| {
        let Some(k0) = shaper.key(plan.runs[0].paint_ptr) else { panic!("missing run 0 paint") };
        let Some(k1) = shaper.key(plan.runs[1].paint_ptr) else { panic!("missing run 1 paint") };
        let Some(k3) = shaper.key(plan.runs[3].paint_ptr) else { panic!("missing run 3 paint") };
        assert_eq!(k0.color, Color::GREEN.to_argb());
        assert_eq!(k1.color, Color::RED.to_argb());
        assert_eq!(k3.color, Color::GREEN.to_argb());
        assert_eq!(k3.font_px(), 24.0);
    });

    assert!(plan.runs[0].background.is_none());
    let Some(bg_cmd) = plan.runs[3].background else {
        panic!("expected background rect on run 3");
    };
    assert_eq!(bg_cmd.color, Color::BLUE.to_argb());
    assert_eq!(plan.decorations.len(), 4);
    assert_eq!(plan.decorations[0].color, Color::GREEN.to_argb());
    assert_eq!(plan.decorations[1].color, Color::RED.to_argb());
}

#[test]
fn alignments_and_justify_position_runs_and_distribute_word_spacing() {
    let (backend, _, plans, _) = MockNativeBackend::new();
    let mut measurer = AndroidTextMeasurer::new(backend);
    let density = Density::new(1.0);
    let opts = TextLayoutOptions::default();

    // Center alignment of "abcd" (40px) in fixed 100px width -> run starts at x = 30.0.
    let center_style = TextStyle {
        font_size: TextUnit::sp(20.0),
        text_align: Some(TextAlign::Center),
        ..TextStyle::DEFAULT
    };
    let centered =
        measurer.layout("abcd", &center_style, opts, Constraints::fixed(100, 100), &density);
    {
        let complex = plans.complex.borrow();
        let Some(plan) = complex.get(&(centered.0 as u32)) else {
            panic!("expected complex plan for centered text");
        };
        assert_eq!(plan.runs[0].x, 30.0);
    }

    // Justify alignment of "aa bb cc dd" (80px visible on line 0 "aa bb cc ", "dd" on line 1)
    // in fixed 100px width:
    // Line 0 visible text is "aa bb cc" (80px, 2 spaces) -> extra = 20px -> word_spacing_px = 10.0!
    // Line 1 is last line ("dd") -> word_spacing_px = 0.0!
    let justify_style = TextStyle {
        font_size: TextUnit::sp(20.0),
        text_align: Some(TextAlign::Justify),
        ..TextStyle::DEFAULT
    };
    let justified = measurer.layout(
        "aa bb cc dd",
        &justify_style,
        opts,
        Constraints::fixed(100, 200),
        &density,
    );
    assert_eq!(measurer.metrics(justified).line_count, 2);
    {
        let complex = plans.complex.borrow();
        let Some(plan) = complex.get(&(justified.0 as u32)) else {
            panic!("expected complex plan for justified text");
        };
        assert_eq!(plan.runs.len(), 2);
        assert_eq!(plan.runs[0].word_spacing_px, 10.0);
        assert_eq!(plan.runs[1].word_spacing_px, 0.0);
    }
}

#[test]
fn mixed_bidi_and_surrogate_pairs_map_visual_runs_cursors_and_hit_tests() {
    let (backend, _, plans, _) = MockNativeBackend::new();
    let mut measurer = AndroidTextMeasurer::new(backend);
    let density = Density::new(1.0);
    let opts = TextLayoutOptions::default();
    let style = TextStyle { font_size: TextUnit::sp(20.0), ..TextStyle::DEFAULT }; // 10px/unit

    let bidi_text = "hi שלום ok";
    let res = measurer.layout(bidi_text, &style, opts, Constraints::new(0, 300, 0, 200), &density);
    {
        let complex = plans.complex.borrow();
        let Some(plan) = complex.get(&(res.0 as u32)) else {
            panic!("expected complex plan for mixed BiDi text");
        };
        assert_eq!(plan.runs.len(), 3);
        assert!(!plan.runs[0].is_rtl);
        assert_eq!(plan.runs[0].x, 0.0);
        assert!(plan.runs[1].is_rtl);
        assert_eq!(plan.runs[1].x, 30.0);
        assert!(!plan.runs[2].is_rtl);
        assert_eq!(plan.runs[2].x, 70.0);
    }

    // Verify Android `Layout.getPrimaryHorizontal` across all 11 logical char boundaries:
    // Logical: h(0..1) i(1..2) ' '(2..3) ש(3..5) ל(5..7) ו(7..9) ם(9..11) ' '(11..12) o(12..13) k(13..14)
    let char_byte_boundaries = [0usize, 1, 2, 3, 5, 7, 9, 11, 12, 13, 14];
    let expected_primary_x = [0.0f32, 10.0, 20.0, 30.0, 60.0, 50.0, 40.0, 70.0, 80.0, 90.0, 100.0];
    for (&byte_off, &exp_x) in char_byte_boundaries.iter().zip(&expected_primary_x) {
        assert_eq!(
            measurer.cursor_rect(res, byte_off).left,
            exp_x,
            "primary cursor mismatch at byte {byte_off}"
        );
    }

    // Hit-test inside and at boundaries of the embedded RTL run [30..70]:
    assert_eq!(measurer.hit_test(res, Offset::new(32.0, 10.0)), TextPosition { offset: 3 });
    assert_eq!(measurer.hit_test(res, Offset::new(41.0, 10.0)), TextPosition { offset: 9 });
    assert_eq!(measurer.hit_test(res, Offset::new(61.0, 10.0)), TextPosition { offset: 5 });
    assert_eq!(measurer.hit_test(res, Offset::new(68.0, 10.0)), TextPosition { offset: 11 });

    // Surrogate pair emoji ("a😀b": 'a' = 1 byte, '😀' = 4 bytes, 'b' = 1 byte):
    // Hit-test never splits the surrogate pair!
    let emoji_res =
        measurer.layout("a😀b", &style, opts, Constraints::new(0, 200, 0, 100), &density);
    let pos_before_emoji = measurer.hit_test(emoji_res, Offset::new(11.0, 10.0));
    let pos_after_emoji = measurer.hit_test(emoji_res, Offset::new(19.0, 10.0));
    assert_eq!(pos_before_emoji, TextPosition { offset: 1 });
    assert_eq!(pos_after_emoji, TextPosition { offset: 5 });
}

#[test]
fn relayout_in_place_and_probe_layout_reuse_intrinsics_without_reshaping() {
    let (backend, counters, _, _) = MockNativeBackend::new();
    let mut measurer = AndroidTextMeasurer::new(backend);
    let density = Density::new(1.0);
    let opts = TextLayoutOptions::default();
    let style = TextStyle { font_size: TextUnit::sp(20.0), ..TextStyle::DEFAULT };

    let annotated_two = AnnotatedString::new("one\ntwo");
    let first = measurer.layout_annotated(
        &annotated_two,
        &style,
        opts,
        Constraints::new(0, 200, 0, 200),
        &density,
    );
    assert_eq!(counters.mt_builds.get(), 2); // 2 hard paragraphs shaped once.

    // Probe at a narrower width builds zero new `MeasuredText` instances and keeps `first` alive.
    let probed = measurer.layout_probe(
        Some(first),
        &annotated_two,
        &style,
        opts,
        Constraints::new(0, 120, 0, 200),
        &density,
    );
    assert_eq!(counters.mt_builds.get(), 2);
    assert_eq!(measurer.metrics(probed).line_count, 2);
    measurer.release(probed);

    // Relayout at another width also reuses pre-built `MeasuredText` instances (`mt_builds` stays 2).
    let Some(relaid) = measurer.relayout(first, Constraints::new(80, 160, 0, 200)) else {
        panic!("expected relayout to succeed");
    };
    assert_eq!(counters.mt_builds.get(), 2);
    assert_eq!(measurer.metrics(relaid).size.width, 80);

    // Single-line boring paragraph updates width in-place across `[min_same_width, i32::MAX]` with
    // zero shaping or line-breaking calls!
    let single =
        measurer.layout("short", &style, opts, Constraints::new(100, 200, 0, 100), &density);
    let boring_before = counters.boring_measures.get();
    let Some(widened) = measurer.relayout(single, Constraints::new(150, 300, 0, 100)) else {
        panic!("expected in-place relayout to succeed");
    };
    assert_eq!(counters.boring_measures.get(), boring_before);
    assert_eq!(measurer.metrics(widened).size.width, 150);
}
