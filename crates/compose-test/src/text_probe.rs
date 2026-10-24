//! Probes of text layout for tests: a measurer counting the layouts of each text, and a dump of
//! what a frame draws with each paragraph's layout in place of its handle, so that two hosts'
//! frames can be compared whatever handles their measurers gave.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write;
use std::rc::Rc;

use compose_ui::{
    AnnotatedString, Constraints, Density, Offset, Rect, TextStyle,
    platform::{
        DrawOp, ParagraphHandle, ParagraphMetrics, TextLayoutOptions, TextMeasurer, TextPosition,
        UiHost,
    },
};

/// How many times a text was laid out: anew (`layout`, `layout_annotated`), or again from what
/// the measurer kept of it (`relayout`); and how many times its layout was kept in a new paint
/// (`restyle`, no layout).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutCounts {
    pub fresh: usize,
    pub relaid: usize,
    pub restyled: usize,
    /// Laid out for intrinsic probes (`layout_probe`), a layout each.
    pub probes: usize,
}

impl LayoutCounts {
    /// Every layout, any way (restyles aren't layouts).
    pub fn total(&self) -> usize {
        self.fresh + self.relaid + self.probes
    }
}

/// The counts of a [`CountingTextMeasurer`], by text: shared, so a test reads them while the
/// measurer is its host's.
#[derive(Clone, Default)]
pub struct TextLayoutCounter(Rc<RefCell<HashMap<String, LayoutCounts>>>);

impl TextLayoutCounter {
    /// `text`'s counts since the last [`clear`](Self::clear).
    pub fn of(&self, text: &str) -> LayoutCounts {
        self.0.borrow().get(text).copied().unwrap_or_default()
    }

    /// Every text's layouts since the last [`clear`](Self::clear).
    pub fn total(&self) -> usize {
        self.0.borrow().values().map(LayoutCounts::total).sum()
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    fn count(&self, text: &str, update: impl FnOnce(&mut LayoutCounts)) {
        update(self.0.borrow_mut().entry(text.to_string()).or_default());
    }
}

/// `inner`, counting the layouts of each text in a [`TextLayoutCounter`].
pub struct CountingTextMeasurer<M> {
    inner: M,
    counter: TextLayoutCounter,
}

impl<M: TextMeasurer> CountingTextMeasurer<M> {
    /// The measurer, and the counter that reads its counts.
    pub fn new(inner: M) -> (CountingTextMeasurer<M>, TextLayoutCounter) {
        let counter = TextLayoutCounter::default();
        (CountingTextMeasurer { inner, counter: counter.clone() }, counter)
    }

    pub fn inner(&self) -> &M {
        &self.inner
    }
}

impl<M: TextMeasurer> TextMeasurer for CountingTextMeasurer<M> {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.counter.count(text, |counts| counts.fresh += 1);
        self.inner.layout(text, style, options, constraints, density)
    }

    fn layout_annotated(
        &mut self,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.counter.count(text.as_str(), |counts| counts.fresh += 1);
        self.inner.layout_annotated(text, style, options, constraints, density)
    }

    fn layout_probe(
        &mut self,
        measured: Option<ParagraphHandle>,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.counter.count(text.as_str(), |counts| counts.probes += 1);
        self.inner.layout_probe(measured, text, style, options, constraints, density)
    }

    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
        self.inner.metrics(paragraph)
    }

    fn did_exceed_max_lines(&self, paragraph: ParagraphHandle) -> bool {
        self.inner.did_exceed_max_lines(paragraph)
    }

    fn max_intrinsic_width(&self, paragraph: ParagraphHandle) -> Option<f32> {
        self.inner.max_intrinsic_width(paragraph)
    }

    fn relayout(
        &mut self,
        paragraph: ParagraphHandle,
        constraints: Constraints,
    ) -> Option<ParagraphHandle> {
        let text = self.inner.text(paragraph).to_string();
        let relaid = self.inner.relayout(paragraph, constraints)?;
        self.counter.count(&text, |counts| counts.relaid += 1);
        Some(relaid)
    }

    fn restyle(
        &mut self,
        paragraph: ParagraphHandle,
        style: &TextStyle,
    ) -> Option<ParagraphHandle> {
        let text = self.inner.text(paragraph).to_string();
        let restyled = self.inner.restyle(paragraph, style)?;
        self.counter.count(&text, |counts| counts.restyled += 1);
        Some(restyled)
    }

    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
        self.inner.hit_test(paragraph, position)
    }

    fn is_right_to_left(&self, paragraph: ParagraphHandle) -> bool {
        self.inner.is_right_to_left(paragraph)
    }

    fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> Rect {
        self.inner.cursor_rect(paragraph, offset)
    }

    fn selection_rects(
        &self,
        paragraph: ParagraphHandle,
        range: std::ops::Range<usize>,
    ) -> Vec<Rect> {
        self.inner.selection_rects(paragraph, range)
    }

    fn text(&self, paragraph: ParagraphHandle) -> &str {
        self.inner.text(paragraph)
    }

    fn release(&mut self, paragraph: ParagraphHandle) {
        self.inner.release(paragraph)
    }
}

/// What `host`'s last frame draws, one operation per line ([`FlatDisplayList::dump`]), each
/// paragraph shown by its layout rather than its handle: its text, size, line count, baselines,
/// direction, whether it cut lines, and the cursor's rectangle before each character and at the
/// end (where its lines break and each character is). Two hosts drawing the same frame give the
/// same snapshot.
///
/// [`FlatDisplayList::dump`]: compose_ui::platform::FlatDisplayList::dump
pub fn draw_snapshot(host: &UiHost) -> String {
    let list = host.display_list();
    let paragraphs: Vec<ParagraphHandle> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawParagraph(handle, _) => Some(*handle),
            _ => None,
        })
        .collect();
    host.with_tree(|tree| {
        let measurer = tree.text_measurer_ref();
        let mut paragraphs = paragraphs.into_iter();
        let mut out = String::new();
        for line in list.dump().lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("paragraph ") else {
                let _ = writeln!(out, "{line}");
                continue;
            };
            let handle = paragraphs.next().expect("a paragraph per line");
            let at = rest.split_once(" at ").map_or("", |(_, at)| at);
            let indent = &line[..line.len() - trimmed.len()];
            let text = measurer.text(handle);
            let metrics = measurer.metrics(handle);
            let _ = writeln!(
                out,
                "{indent}paragraph {text:?} at {at}: {metrics:?}, right to left {}, cut {}",
                measurer.is_right_to_left(handle),
                measurer.did_exceed_max_lines(handle),
            );
            let offsets = text.char_indices().map(|(offset, _)| offset).chain([text.len()]);
            for offset in offsets {
                let cursor = measurer.cursor_rect(handle, offset);
                let _ = writeln!(
                    out,
                    "{indent}  {offset}: {} {} {}",
                    cursor.left, cursor.top, cursor.bottom
                );
            }
        }
        out
    })
}
