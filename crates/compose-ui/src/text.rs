//! Text: shared strings, styles, and the `TextMeasurer` interface each platform implements with
//! its own text stack (docs/07-platforms.md). `FixedTextMeasurer` lays text out with a fixed
//! advance per character, for layout tests that shouldn't depend on fonts.

use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use compose_runtime::ComposeArg;

use crate::constraints::Constraints;
use crate::paragraph::{Paragraph, TextLayoutInput};
use crate::path::Path;
use crate::text_attributes::{ResolvedTextDirection, TextOverflow};
use crate::text_range::TextRange;
use crate::text_style::TextStyle;
use crate::unit::{Density, IntSize, Offset, Rect, TextUnit};

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

/// What a text's layout came to, as Compose's `TextLayoutResult`: what it was laid out from,
/// its paragraph with Compose's queries, and its size (D1105). What `on_text_layout` hears after
/// each new layout; it keeps the paragraph alive as long as it's kept.
#[derive(Clone)]
pub struct TextLayoutResult {
    pub layout_input: TextLayoutInput,
    /// The text laid out: one paragraph for the whole text (the measurers lay a text's
    /// paragraphs out together), as Compose's `MultiParagraph`.
    pub multi_paragraph: Rc<dyn Paragraph>,
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
    /// What a draw scope draws of it, shared by its clones and copies (D1116).
    pub(crate) drawable: Rc<crate::text_measurer::DrawableParagraphs>,
}

impl TextLayoutResult {
    /// The result of laying `layout_input` out as `multi_paragraph` at `size`, as Compose's
    /// constructor: the baselines and line count are the paragraph's, and it overflows where
    /// the paragraph is larger than `size` or had more lines than its maximum.
    pub fn new(
        layout_input: TextLayoutInput,
        multi_paragraph: Rc<dyn Paragraph>,
        size: IntSize,
    ) -> TextLayoutResult {
        TextLayoutResult {
            line_count: multi_paragraph.line_count() as u32,
            first_baseline: multi_paragraph.first_baseline(),
            last_baseline: multi_paragraph.last_baseline(),
            did_overflow_width: (size.width as f32) < multi_paragraph.width(),
            did_overflow_height: multi_paragraph.did_exceed_max_lines()
                || (size.height as f32) < multi_paragraph.height(),
            layout_input,
            multi_paragraph,
            size,
            drawable: Rc::default(),
        }
    }

    /// The text doesn't fit: it's clipped, ellipsized or drawn past its bounds, as Compose's
    /// `hasVisualOverflow`.
    pub fn has_visual_overflow(&self) -> bool {
        self.did_overflow_width || self.did_overflow_height
    }

    /// The same layout, from another input at another size, as Compose's `copy`.
    pub fn copy(&self, layout_input: TextLayoutInput, size: IntSize) -> TextLayoutResult {
        match &self.drawable.measured {
            Some(measured) => TextLayoutResult::of_measured(
                layout_input,
                measured,
                self.multi_paragraph.clone(),
                self.drawable.clone(),
                size,
            ),
            None => TextLayoutResult {
                drawable: self.drawable.clone(),
                ..TextLayoutResult::new(layout_input, self.multi_paragraph.clone(), size)
            },
        }
    }

    pub fn get_line_start(&self, line: usize) -> usize {
        self.multi_paragraph.get_line_start(line)
    }

    pub fn get_line_end(&self, line: usize, visible_end: bool) -> usize {
        self.multi_paragraph.get_line_end(line, visible_end)
    }

    pub fn is_line_ellipsized(&self, line: usize) -> bool {
        self.multi_paragraph.is_line_ellipsized(line)
    }

    pub fn get_line_top(&self, line: usize) -> f32 {
        self.multi_paragraph.get_line_top(line)
    }

    pub fn get_line_baseline(&self, line: usize) -> f32 {
        self.multi_paragraph.get_line_baseline(line)
    }

    pub fn get_line_bottom(&self, line: usize) -> f32 {
        self.multi_paragraph.get_line_bottom(line)
    }

    pub fn get_line_left(&self, line: usize) -> f32 {
        self.multi_paragraph.get_line_left(line)
    }

    pub fn get_line_right(&self, line: usize) -> f32 {
        self.multi_paragraph.get_line_right(line)
    }

    pub fn get_line_for_offset(&self, offset: usize) -> usize {
        self.multi_paragraph.get_line_for_offset(offset)
    }

    pub fn get_line_for_vertical_position(&self, vertical: f32) -> usize {
        self.multi_paragraph.get_line_for_vertical_position(vertical)
    }

    pub fn get_horizontal_position(&self, offset: usize, use_primary_direction: bool) -> f32 {
        self.multi_paragraph.get_horizontal_position(offset, use_primary_direction)
    }

    pub fn get_paragraph_direction(&self, offset: usize) -> ResolvedTextDirection {
        self.multi_paragraph.get_paragraph_direction(offset)
    }

    pub fn get_bidi_run_direction(&self, offset: usize) -> ResolvedTextDirection {
        self.multi_paragraph.get_bidi_run_direction(offset)
    }

    pub fn get_offset_for_position(&self, position: Offset) -> usize {
        self.multi_paragraph.get_offset_for_position(position)
    }

    pub fn get_bounding_box(&self, offset: usize) -> Rect {
        self.multi_paragraph.get_bounding_box(offset)
    }

    pub fn get_bounding_boxes_for_range(&self, start: usize, end: usize) -> Vec<Rect> {
        self.multi_paragraph.get_bounding_boxes_for_range(start, end)
    }

    pub fn get_word_boundary(&self, offset: usize) -> TextRange {
        self.multi_paragraph.get_word_boundary(offset)
    }

    pub fn get_cursor_rect(&self, offset: usize) -> Rect {
        self.multi_paragraph.get_cursor_rect(offset)
    }

    pub fn get_path_for_range(&self, start: usize, end: usize) -> Path {
        self.multi_paragraph.get_path_for_range(start, end)
    }
}

impl PartialEq for TextLayoutResult {
    /// As Compose's: the same input, the same paragraph object, the same size and baselines.
    fn eq(&self, other: &Self) -> bool {
        self.layout_input == other.layout_input
            && Rc::ptr_eq(&self.multi_paragraph, &other.multi_paragraph)
            && self.size == other.size
            && self.first_baseline == other.first_baseline
            && self.last_baseline == other.last_baseline
    }
}

impl fmt::Debug for TextLayoutResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextLayoutResult")
            .field("layout_input", &self.layout_input)
            .field("multi_paragraph", &self.multi_paragraph)
            .field("size", &self.size)
            .field("first_baseline", &self.first_baseline)
            .field("last_baseline", &self.last_baseline)
            .finish()
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
    /// The fonts this measurer lays out in, for a resolver over it. `None` (the default): it knows
    /// no fonts, and every family is its default.
    fn fonts(&self) -> Option<Rc<crate::font_resolver::PlatformFonts>> {
        None
    }
    /// Whether a font `paragraph` asked for arrived since it was laid out, as Compose's
    /// `hasStaleResolvedFonts`: it's to be laid out anew.
    fn has_stale_resolved_fonts(&self, _paragraph: ParagraphHandle) -> bool {
        false
    }
    /// A new paragraph with `paragraph`'s layout in `style`'s paint, as [`restyle`](Self::restyle)
    /// makes, `paragraph` staying as it is: what Compose's `Paragraph.paint(color, …)` draws of a
    /// layout kept (a result drawn in another color, D1116). `None` (the default) when the
    /// measurer can't: the caller lays the text out anew in that paint.
    fn painted(
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
    /// The paragraph's queries, as Compose's `Paragraph` (D1105): an object of its own, valid
    /// after `paragraph` is released. The default samples the measurer's cursor at each
    /// character boundary ([`crate::paragraph::sampled_paragraph`]); a measurer that knows its
    /// lines gives its own.
    fn paragraph(&self, paragraph: ParagraphHandle) -> Rc<dyn Paragraph> {
        Rc::new(crate::paragraph::sampled_paragraph(self, paragraph))
    }
    /// The paragraph's text, for dumps and semantics.
    fn text(&self, paragraph: ParagraphHandle) -> &str;
    fn release(&mut self, paragraph: ParagraphHandle);
}

/// The platform's text measurer, shared by the tree that lays its nodes' texts out and draws them
/// and by the [`TextMeasurer`](crate::TextMeasurer)s made over it, which measure from anywhere
/// (composition, layout, a draw scope, an event), as Compose's text stack is reached from
/// anywhere. Cloning shares it. Each use borrows it: the toolkit gives what it borrowed back
/// before another node, a callback or a measurement can run (D1115). A node holding
/// `scope.text_measurer()`'s guard while it calls [`TextMeasurer::measure`](crate::TextMeasurer)
/// or a result's queries panics, as a `RefCell` borrowed twice: ask, let the guard go, then
/// measure.
#[derive(Clone)]
pub struct SharedTextMeasurer(Rc<SharedMeasurer>);

struct SharedMeasurer {
    measurer: RefCell<Box<dyn TextMeasurer>>,
    /// Paragraphs nothing keeps any more: a [`ParagraphKeeper`] can be dropped while the measurer
    /// is borrowed, or by a recording being replaced.
    dropped: Arc<DroppedParagraphs>,
}

/// The paragraphs let go of, and a flag for the draw that has none to give back (nearly all of
/// them): a lock each draw shows in a frame's instructions.
#[derive(Default)]
struct DroppedParagraphs {
    any: AtomicBool,
    handles: Mutex<Vec<ParagraphHandle>>,
}

/// Keeps a paragraph of the platform's measurer: the last one dropped gives it back, at the next
/// [`SharedTextMeasurer::release_dropped`]. What a recording holds of what `draw_text` drew: like
/// the rest of a display list, it can be sent to another thread (D270), which never touches the
/// measurer.
#[derive(Clone)]
pub(crate) struct ParagraphKeeper(Arc<KeeperInner>);

struct KeeperInner {
    handle: ParagraphHandle,
    dropped: Arc<DroppedParagraphs>,
}

impl ParagraphKeeper {
    pub(crate) fn handle(&self) -> ParagraphHandle {
        self.0.handle
    }
}

impl Drop for KeeperInner {
    fn drop(&mut self) {
        self.dropped.handles.lock().unwrap_or_else(PoisonError::into_inner).push(self.handle);
        self.dropped.any.store(true, Ordering::Release);
    }
}

impl fmt::Debug for ParagraphKeeper {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ParagraphKeeper({:?})", self.0.handle)
    }
}

impl SharedTextMeasurer {
    pub fn new(measurer: Box<dyn TextMeasurer>) -> SharedTextMeasurer {
        SharedTextMeasurer(Rc::new(SharedMeasurer {
            measurer: RefCell::new(measurer),
            dropped: Arc::default(),
        }))
    }

    /// Panics if it's borrowed mutably, as a `RefCell`.
    pub fn borrow(&self) -> Ref<'_, dyn TextMeasurer> {
        Ref::map(self.0.measurer.borrow(), |measurer| &**measurer)
    }

    /// Panics if it's borrowed, as a `RefCell`.
    pub fn borrow_mut(&self) -> RefMut<'_, dyn TextMeasurer> {
        RefMut::map(self.0.measurer.borrow_mut(), |measurer| &mut **measurer)
    }

    /// Gives `paragraph` back to the measurer, as [`TextMeasurer::release`].
    pub fn release(&self, paragraph: ParagraphHandle) {
        self.borrow_mut().release(paragraph);
    }

    /// Whether `other` is this very measurer.
    pub fn ptr_eq(&self, other: &SharedTextMeasurer) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Keeps `paragraph`, until the keeper's last clone is dropped.
    pub(crate) fn keeper(&self, paragraph: ParagraphHandle) -> ParagraphKeeper {
        ParagraphKeeper(Arc::new(KeeperInner {
            handle: paragraph,
            dropped: self.0.dropped.clone(),
        }))
    }

    /// Gives back the paragraphs nothing keeps any more, at the end of each draw pass and before
    /// a measurer's layout. They're out of every recording, so a frame drawn after this names none
    /// of them (D1116).
    pub fn release_dropped(&self) {
        if !self.0.dropped.any.swap(false, Ordering::Acquire) {
            return;
        }
        let dropped = std::mem::take(
            &mut *self.0.dropped.handles.lock().unwrap_or_else(PoisonError::into_inner),
        );
        let mut measurer = self.borrow_mut();
        for paragraph in dropped {
            measurer.release(paragraph);
        }
    }
}

impl ComposeArg for SharedTextMeasurer {
    fn same(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}

/// The default font size when a style leaves it unspecified, as Compose's (`DefaultFontSize`).
pub const DEFAULT_FONT_SIZE: TextUnit = TextUnit::sp(14.0);

/// A style's font size in pixels: 14 sp when unset, and an `em` size a multiple of 14 sp (a
/// text's own style has no enclosing size; a span's `em` size is resolved over its text's by
/// [`crate::AnnotatedString::runs`]).
pub fn resolve_font_size_px(style: &TextStyle, density: &Density) -> f32 {
    let size = style.font_size;
    if size.is_sp() {
        density.sp_to_px(size)
    } else if size.is_em() {
        density.sp_to_px(DEFAULT_FONT_SIZE) * size.value()
    } else {
        density.sp_to_px(DEFAULT_FONT_SIZE)
    }
}

/// The color a style's text is painted in by a measurer without text shaders: its color, a
/// brush's color or gradient average at its alpha, black when unset (Compose's default).
pub fn resolve_text_color(style: &TextStyle) -> crate::graphics::Color {
    match style.foreground() {
        crate::text_attributes::TextForegroundStyle::Unspecified => crate::graphics::Color::BLACK,
        crate::text_attributes::TextForegroundStyle::Color(color) => color,
        crate::text_attributes::TextForegroundStyle::Brush { brush, alpha } => {
            brush.color(if alpha.is_nan() { 1.0 } else { alpha })
        }
    }
}

/// The pixels of `unit` in text `font_px` pixels high: `sp` through the density, `em` a
/// multiple of the font size; `None` when unspecified. For letter spacing and line heights.
pub fn resolve_text_unit_px(unit: TextUnit, font_px: f32, density: &Density) -> Option<f32> {
    if unit.is_sp() {
        Some(density.sp_to_px(unit))
    } else if unit.is_em() {
        Some(unit.value() * font_px)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_strings_compare_by_value() {
        let a: SharedString = String::from("abc").into();
        let b: SharedString = "abc".into();
        assert!(a.same(&b) && a == b);
        assert!(a.same(&a.clone()));
    }
}
