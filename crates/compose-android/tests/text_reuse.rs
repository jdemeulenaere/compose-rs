//! Android's text layout across constraint changes (D550, D551), over a model of
//! `TextMeasurer.kt`: it reads the request as Kotlin does, chooses the width as its `layoutAt`
//! does (Compose's `finalMaxWidth` from the text's own width), breaks lines greedily at that
//! width as a `StaticLayout`, aligns them in it, and keeps what it measured of each text
//! ([`Kept`], as Kotlin's `Intrinsics`) so that `relayout` measures nothing.
//!
//! - A relayout lays the text out exactly as a fresh layout at the same constraints, measuring it
//!   once in all, as Compose's `ParagraphIntrinsics`.
//! - With D551's optimization, a host draws each frame as a host without it while the root's width
//!   changes every frame, and lays out less.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_android::text::{AndroidTextMeasurer, LaidOut, TextBackend};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::composable;
use compose_test::*;
use compose_ui::{
    Constraints, Density, IntSize, Offset, Rect, TextAlign, TextOverflow, TextStyle,
    platform::{ParagraphMetrics, TextLayoutOptions, TextMeasurer, UiHost},
};

// ---- A model of TextMeasurer.kt ----------------------------------------------------------------

/// What Kotlin keeps of a text (its `Intrinsics`): the text, its line options, its paint (color
/// and decoration), its advance per UTF-16 unit and line height, and its own width.
#[derive(Clone)]
struct Kept {
    paint: (u32, u8),
    units: Vec<Vec<u16>>,
    soft_wrap: bool,
    max_lines: usize,
    ellipsis: bool,
    align: u8,
    right_to_left: bool,
    advance: f32,
    line_height: f32,
    max_intrinsic_width: f32,
}

/// A laid-out paragraph: its width and its lines (UTF-16 ranges and widths).
struct Laid {
    kept: Kept,
    width: i32,
    lines: Vec<(usize, usize, f32)>,
}

#[derive(Clone, Default)]
struct Model {
    paragraphs: Rc<RefCell<HashMap<u32, Laid>>>,
    /// Texts measured (Kotlin's `isBoring` and `getDesiredWidth`): once per fresh layout.
    measures: Rc<Cell<usize>>,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, count: usize) -> &[u8] {
        let (head, rest) = self.0.split_at(count);
        self.0 = rest;
        head
    }
    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }
    fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    /// A style: its paint (color, decoration), font size and letter spacing (px) and line
    /// height (px, 0 unset).
    fn style(&mut self) -> ((u32, u8), f32, f32, f32) {
        let color = self.i32() as u32;
        let font_px = self.f32();
        let _weight = self.i32();
        let _italic = self.u8();
        if self.u8() == 4 {
            let length = self.i32() as usize;
            self.take(length);
        }
        let spacing = self.f32();
        let decoration = self.u8();
        let line_height = self.f32();
        let _background = self.i32();
        ((color, decoration), font_px, spacing, line_height)
    }
}

impl Model {
    /// `TextMeasurer.layoutAt`: the width, then the lines broken at it.
    fn lay_out(&self, handle: u32, kept: Kept, min_width: i32, max_width: i32) -> LaidOut {
        let bounded = max_width != i32::MAX;
        let available =
            if (kept.soft_wrap || kept.ellipsis) && bounded { max_width } else { i32::MAX };
        let width = if min_width == available {
            available
        } else {
            (kept.max_intrinsic_width.ceil() as i32).clamp(min_width, available)
        };
        let mut lines = Vec::new();
        let mut start = 0;
        for hard in &kept.units {
            let mut from = 0;
            loop {
                // As many units as fit, back to the last space when cut.
                let fit = ((width as f32 / kept.advance).floor() as usize).max(1);
                let mut end = (from + fit).min(hard.len());
                if end < hard.len()
                    && let Some(space) = (from..end).rev().find(|&at| hard[at] == u16::from(b' '))
                    && space > from
                {
                    end = space;
                }
                lines.push((start + from, start + end, (end - from) as f32 * kept.advance));
                from = end;
                while from < hard.len() && hard[from] == u16::from(b' ') {
                    from += 1;
                }
                if from >= hard.len() {
                    break;
                }
            }
            start += hard.len() + 1;
        }
        let exceeded = lines.len() > kept.max_lines;
        lines.truncate(kept.max_lines.max(1));
        let count = lines.len();
        let metrics = ParagraphMetrics {
            size: IntSize::new(width, (count as f32 * kept.line_height).ceil() as i32),
            first_baseline: kept.line_height * 0.8,
            last_baseline: kept.line_height * (count as f32 - 0.2),
            line_count: count as u32,
        };
        let max_intrinsic_width = Some(kept.max_intrinsic_width);
        self.paragraphs.borrow_mut().insert(handle, Laid { kept, width, lines });
        LaidOut { metrics, did_exceed_max_lines: exceeded, max_intrinsic_width }
    }

    fn live(&self) -> usize {
        self.paragraphs.borrow().len()
    }
}

impl TextBackend for Model {
    fn layout(&mut self, handle: u32, text: &str, request: &[u8]) -> LaidOut {
        let mut request = Reader(request);
        let soft_wrap = request.u8() != 0;
        let max_lines = request.i32() as usize;
        let ellipsis = request.u8() != 0;
        let align = request.u8();
        let right_to_left = request.u8() != 0;
        let min_width = request.i32();
        let max_width = request.i32();
        let (paint, font_px, spacing, line_height) = request.style();
        let advance = font_px * 0.5 + spacing;
        let units: Vec<Vec<u16>> =
            text.split('\n').map(|line| line.encode_utf16().collect()).collect();
        self.measures.set(self.measures.get() + 1);
        let own = units.iter().map(Vec::len).max().unwrap_or(0) as f32 * advance;
        let kept = Kept {
            paint,
            units,
            soft_wrap,
            max_lines,
            ellipsis,
            align,
            right_to_left,
            advance,
            line_height: if line_height > 0.0 { line_height } else { font_px * 1.25 },
            // Half a pixel more with letter spacing, as `intrinsicWidth`.
            max_intrinsic_width: if spacing != 0.0 && own != 0.0 { own + 0.5 } else { own },
        };
        self.lay_out(handle, kept, min_width, max_width)
    }

    fn relayout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
    ) -> Option<LaidOut> {
        let kept = self.paragraphs.borrow_mut().remove(&previous)?.kept;
        Some(self.lay_out(handle, kept, min_width, max_width))
    }

    /// As `relayout`, keeping `previous`: Kotlin's `probeLayout`.
    fn probe_layout(
        &mut self,
        previous: u32,
        handle: u32,
        min_width: i32,
        max_width: i32,
    ) -> Option<LaidOut> {
        let kept = self.paragraphs.borrow().get(&previous)?.kept.clone();
        Some(self.lay_out(handle, kept, min_width, max_width))
    }

    /// The same lines in a new paint, as Kotlin keeps the `Layout`: nothing measured.
    fn restyle(&mut self, previous: u32, handle: u32, color: u32, decoration: u8) -> bool {
        let mut paragraphs = self.paragraphs.borrow_mut();
        let Some(mut laid) = paragraphs.remove(&previous) else { return false };
        laid.kept.paint = (color, decoration);
        paragraphs.insert(handle, laid);
        true
    }

    fn hit_test(&self, handle: u32, position: Offset) -> usize {
        let paragraphs = self.paragraphs.borrow();
        let laid = &paragraphs[&handle];
        let line = ((position.y / laid.kept.line_height) as usize).min(laid.lines.len() - 1);
        let (from, to, _) = laid.lines[line];
        (from + (position.x / laid.kept.advance).round().max(0.0) as usize).min(to)
    }

    /// Each line placed in the width as Kotlin's alignment places it (opposite and right to left
    /// from the right edge; justified lines as normal ones).
    fn cursor(&self, handle: u32, offset: usize) -> Rect {
        let paragraphs = self.paragraphs.borrow();
        let laid = &paragraphs[&handle];
        let index =
            laid.lines.iter().position(|&(_, to, _)| offset <= to).unwrap_or(laid.lines.len() - 1);
        let (from, _, line_width) = laid.lines[index];
        let free = laid.width as f32 - line_width;
        let from_right = (laid.kept.align == 1) != laid.kept.right_to_left;
        let start = match laid.kept.align {
            2 => free / 2.0,
            _ if from_right => free,
            _ => 0.0,
        };
        let column = offset.saturating_sub(from) as f32 * laid.kept.advance;
        let x = if laid.kept.right_to_left { start + line_width - column } else { start + column };
        let top = index as f32 * laid.kept.line_height;
        Rect::new(x, top, x, top + laid.kept.line_height)
    }

    fn release(&mut self, handle: u32) {
        assert!(self.paragraphs.borrow_mut().remove(&handle).is_some(), "{handle} released twice");
    }
}

// ---- The measurer ---------------------------------------------------------------------------

fn snapshot(
    measurer: &dyn TextMeasurer,
    paragraph: compose_ui::platform::ParagraphHandle,
) -> String {
    let text = measurer.text(paragraph);
    let mut out = format!(
        "{:?} {} {:?}\n",
        measurer.metrics(paragraph),
        measurer.did_exceed_max_lines(paragraph),
        measurer.max_intrinsic_width(paragraph)
    );
    for (offset, _) in text.char_indices().chain([(text.len(), ' ')]) {
        out.push_str(&format!("{:?}\n", measurer.cursor_rect(paragraph, offset)));
    }
    out
}

#[test]
fn a_relayout_lays_the_text_out_as_a_fresh_layout_without_measuring_it_again() {
    let model = Model::default();
    let mut measurer = AndroidTextMeasurer::new(model.clone());
    let density = Density::new(2.0);
    let wide = Constraints::new(0, 1000, 0, 1000);
    let cases: [(&str, TextAlign, TextLayoutOptions); 5] = [
        ("Short", TextAlign::Start, TextLayoutOptions::default()),
        ("Wraps at most widths given", TextAlign::Center, TextLayoutOptions::default()),
        (
            "Ellipsized at narrow widths",
            TextAlign::End,
            TextLayoutOptions {
                max_lines: 1,
                overflow: TextOverflow::Ellipsis,
                ..Default::default()
            },
        ),
        ("שלום עולם", TextAlign::Start, TextLayoutOptions::default()),
        (
            "No wrap\nTwo lines",
            TextAlign::Justify,
            TextLayoutOptions { soft_wrap: false, ..Default::default() },
        ),
    ];
    for (text, align, options) in cases {
        let style = TextStyle { text_align: Some(align), ..TextStyle::DEFAULT };
        let mut paragraph = measurer.layout(text, &style, options, wide, &density);
        for constraints in [
            Constraints::new(0, 300, 0, 1000),
            Constraints::fixed(250, 40),
            Constraints::new(120, 400, 0, 1000),
            Constraints::new(0, 40, 0, 1000),
            Constraints::new(0, Constraints::INFINITY, 0, Constraints::INFINITY),
        ] {
            let measured = model.measures.get();
            paragraph = measurer.relayout(paragraph, constraints).expect("Kotlin keeps the text");
            assert_eq!(model.measures.get(), measured, "{text:?}: measured once");
            let fresh = measurer.layout(text, &style, options, constraints, &density);
            assert_eq!(
                snapshot(&measurer, paragraph),
                snapshot(&measurer, fresh),
                "{text:?} at {constraints:?}"
            );
            measurer.release(fresh);
        }
        measurer.release(paragraph);
    }
    assert_eq!(measurer.live(), 0);
    assert_eq!(model.live(), 0, "a relayout releases the paragraph it replaces");
}

#[test]
fn a_backend_keeping_nothing_leaves_the_paragraph_to_a_fresh_layout() {
    struct Forgetful;
    impl TextBackend for Forgetful {
        fn layout(&mut self, _handle: u32, _text: &str, _request: &[u8]) -> LaidOut {
            let metrics = ParagraphMetrics {
                size: IntSize::new(10, 10),
                first_baseline: 8.0,
                last_baseline: 8.0,
                line_count: 1,
            };
            LaidOut { metrics, did_exceed_max_lines: false, max_intrinsic_width: None }
        }
        fn hit_test(&self, _handle: u32, _position: Offset) -> usize {
            0
        }
        fn cursor(&self, _handle: u32, _offset: usize) -> Rect {
            Rect::new(0.0, 0.0, 0.0, 10.0)
        }
        fn release(&mut self, _handle: u32) {}
    }
    let mut measurer = AndroidTextMeasurer::new(Forgetful);
    let paragraph = measurer.layout(
        "kept",
        &TextStyle::DEFAULT,
        TextLayoutOptions::default(),
        Constraints::new(0, 100, 0, 100),
        &Density::new(1.0),
    );
    assert_eq!(measurer.relayout(paragraph, Constraints::new(0, 50, 0, 100)), None);
    assert_eq!(measurer.text(paragraph), "kept", "still live");
    measurer.release(paragraph);
    assert_eq!(measurer.relayout(paragraph, Constraints::new(0, 50, 0, 100)), None, "released");
}

// ---- Through the node -------------------------------------------------------------------------

const SHORT: &str = "Short label";
const CENTERED_WIDE: &str = "Centered, whole width";
const WRAPPING: &str = "A sentence long enough to wrap onto a second line at every width here.";
const ELLIPSIZED: &str = "A single line long enough to be ellipsized at every width it's given.";
const HEBREW: &str = "שלום עולם";
const HEBREW_END: &str = "שלום, בסוף";
const SPACED: &str = "Letter spaced";

#[composable]
fn Cases() {
    let align = |align| TextStyle { text_align: Some(align), ..TextStyle::DEFAULT };
    Column!(modifier = Modifier.fill_max_size(), {
        BasicText!(SHORT);
        BasicText!(
            CENTERED_WIDE,
            modifier = Modifier.fill_max_width(),
            style = align(TextAlign::Center)
        );
        BasicText!(WRAPPING);
        BasicText!(ELLIPSIZED, max_lines = 1, overflow = TextOverflow::Ellipsis);
        BasicText!(HEBREW);
        BasicText!(HEBREW_END, style = align(TextAlign::End));
        BasicText!(
            SPACED,
            style = TextStyle { letter_spacing: compose_ui::Sp(1.0), ..TextStyle::DEFAULT }
        );
    });
}


/// Turns compose-rs's `REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH` on or off on `host` (D551).
fn reuse_at_same_width(host: &mut compose_ui::platform::UiHost, on: bool) {
    let optimizations = host
        .optimizations()
        .with(compose_ui::platform::Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH, on);
    host.set_optimizations(optimizations);
}

fn counted(width: i32) -> (ComposeTestRule, TextLayoutCounter, Model) {
    let model = Model::default();
    let (measurer, counter) = CountingTextMeasurer::new(AndroidTextMeasurer::new(model.clone()));
    let host = UiHost::new(IntSize::new(width, 2000), Density::new(1.0), Box::new(measurer));
    (ComposeTestRule::with_host(host, Cases), counter, model)
}

fn resize(ui: &mut ComposeTestRule, width: i32, reuse: bool) {
    reuse_at_same_width(ui.host(), reuse);
    ui.host().resize(IntSize::new(width, 2000));
    ui.wait_for_idle();
    reuse_at_same_width(ui.host(), false);
}

#[test]
fn a_resized_host_relays_texts_out_from_what_kotlin_kept_and_with_the_optimization_draws_the_same()
{
    let widths = [400, 390, 420, 300, 200, 150, 100, 70, 60, 60, 40, 400, 395];
    let (mut parity, parity_counts, parity_model) = counted(widths[0]);
    let (mut reusing, reusing_counts, _) = counted(widths[0]);
    let measured = parity_model.measures.get();
    parity_counts.clear();
    reusing_counts.clear();
    for width in &widths[1..] {
        resize(&mut parity, *width, false);
        resize(&mut reusing, *width, true);
        assert_eq!(draw_snapshot(reusing.host()), draw_snapshot(parity.host()), "at width {width}");
    }
    // Compose's rule: each new width lays every text out again, from what Kotlin kept.
    let changes = widths.windows(2).filter(|pair| pair[0] != pair[1]).count();
    for text in [SHORT, CENTERED_WIDE, WRAPPING, ELLIPSIZED, HEBREW, HEBREW_END, SPACED] {
        assert_eq!(
            parity_counts.of(text),
            LayoutCounts { fresh: 0, relaid: changes, restyled: 0, probes: 0 },
            "{text:?}"
        );
    }
    assert_eq!(parity_model.measures.get(), measured, "no text measured again");
    // With the optimization, the texts that fit (77, 63, 70 px, and 104.5 letter spaced) are
    // laid out again only at the widths below their own (each, since they wrap there) and back.
    for (text, layouts) in [(SHORT, 4), (HEBREW, 3), (HEBREW_END, 3), (SPACED, 5)] {
        assert_eq!(reusing_counts.of(text).total(), layouts, "{text:?}");
    }
    for text in [CENTERED_WIDE, WRAPPING, ELLIPSIZED] {
        assert_eq!(reusing_counts.of(text), parity_counts.of(text), "{text:?}");
    }
}

// ---- A new paint --------------------------------------------------------------------------------

impl Model {
    /// The paint paragraph `handle` draws in.
    fn paint(&self, handle: compose_ui::platform::ParagraphHandle) -> (u32, u8) {
        self.paragraphs.borrow()[&(handle.0 as u32)].kept.paint
    }
}

#[test]
fn a_new_paint_keeps_the_layout_and_later_layouts_are_in_it() {
    let model = Model::default();
    let mut measurer = AndroidTextMeasurer::new(model.clone());
    let density = Density::new(1.0);
    let red = TextStyle { color: compose_ui::Color::RED, ..TextStyle::DEFAULT };
    let paragraph = measurer.layout(
        "Painted",
        &red,
        TextLayoutOptions::default(),
        Constraints::new(0, 300, 0, 300),
        &density,
    );
    let before = snapshot(&measurer, paragraph);
    let measured = model.measures.get();
    let blue = TextStyle {
        color: compose_ui::Color::BLUE,
        text_decoration: Some(compose_ui::TextDecoration::UNDERLINE),
        ..red
    };
    let restyled = measurer.restyle(paragraph, &blue).expect("Kotlin keeps the layout");
    assert_ne!(restyled, paragraph, "a new handle draws the new paint");
    assert_eq!(snapshot(&measurer, restyled), before, "the same layout");
    assert_eq!(model.measures.get(), measured, "nothing measured");
    assert_eq!(model.paint(restyled), (compose_ui::Color::BLUE.argb(), 1));
    assert_eq!(model.live(), 1, "the old handle released");
    // Laid out again for other constraints: in the new paint.
    let relaid = measurer.relayout(restyled, Constraints::new(0, 30, 0, 300)).unwrap();
    assert_eq!(model.paint(relaid), (compose_ui::Color::BLUE.argb(), 1));
    measurer.release(relaid);
    assert_eq!(measurer.restyle(relaid, &red), None, "released");
}

#[composable]
fn Colored(color: compose_runtime::MutableState<compose_ui::Color>) {
    BasicText!(SHORT, style = TextStyle { color: color.get(), ..TextStyle::DEFAULT });
}

#[test]
fn a_new_color_draws_the_kept_layout_without_laying_it_out() {
    let owned = compose_runtime::mutable_state_of(compose_ui::Color::RED);
    let color = owned.handle();
    let model = Model::default();
    let (measurer, counter) = CountingTextMeasurer::new(AndroidTextMeasurer::new(model.clone()));
    let host = UiHost::new(IntSize::new(300, 300), Density::new(1.0), Box::new(measurer));
    let mut ui = ComposeTestRule::with_host(host, move || Colored(color));
    let measured = model.measures.get();
    counter.clear();
    color.set(compose_ui::Color::BLUE);
    ui.wait_for_idle();
    assert_eq!(counter.of(SHORT), LayoutCounts { fresh: 0, relaid: 0, restyled: 1, probes: 0 });
    assert_eq!(model.measures.get(), measured);
    let drawn = ui
        .host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            compose_ui::platform::DrawOp::DrawParagraph(handle, _) => Some(*handle),
            _ => None,
        })
        .unwrap();
    assert_eq!(model.paint(drawn).0, compose_ui::Color::BLUE.argb());
}

// ---- Intrinsic probes ---------------------------------------------------------------------------

#[test]
fn a_probe_lays_out_from_the_measured_paragraphs_intrinsics_and_keeps_it() {
    let model = Model::default();
    let mut measurer = AndroidTextMeasurer::new(model.clone());
    let density = Density::new(1.0);
    let text = compose_ui::AnnotatedString::new(compose_ui::SharedString::from("Probed text"));
    let options = TextLayoutOptions::default();
    let measured = measurer.layout_annotated(
        &text,
        &TextStyle::DEFAULT,
        options,
        Constraints::new(0, 300, 0, 300),
        &density,
    );
    let measures = model.measures.get();
    let narrow = Constraints::new(0, 40, 0, Constraints::INFINITY);
    let probe = measurer.layout_probe(
        Some(measured),
        &text,
        &TextStyle::DEFAULT,
        options,
        narrow,
        &density,
    );
    assert_eq!(model.measures.get(), measures, "measured from what Kotlin kept");
    assert_eq!(measurer.text(measured), "Probed text", "the measured paragraph kept");
    let fresh = measurer.layout_annotated(&text, &TextStyle::DEFAULT, options, narrow, &density);
    assert_eq!(snapshot(&measurer, probe), snapshot(&measurer, fresh));
    for paragraph in [measured, probe, fresh] {
        measurer.release(paragraph);
    }
    assert_eq!(model.live(), 0);
}
