//! Compose's public `TextMeasurer` (`androidx.compose.ui.text.TextMeasurer`): measures text from
//! anywhere, keeps the last layouts in a cache, and `DrawScope::draw_text` draws what it measured.
//! It lays out on the platform's text measurer (the trait [`platform::TextMeasurer`], D21), which
//! it reaches through its [`FontFamilyResolver`] (D1115).
//!
//! [`platform::TextMeasurer`]: crate::text::TextMeasurer

use std::cell::{OnceCell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;

use compose_runtime::{ComposeArg, composable};

use crate::alignment::LayoutDirection;
use crate::annotated::AnnotatedString;
use crate::constraints::Constraints;
use crate::draw::DrawScope;
use crate::font::{FontFamilyResolver, LOCAL_FONT_FAMILY_RESOLVER, LOCAL_TREE_TEXT_MEASURER};
use crate::graphics::{Color, DrawStyle, Shadow};
use crate::intl::LocaleList;
use crate::paragraph::{Paragraph, TextLayoutInput};
use crate::path::Path;
use crate::text::{
    ParagraphHandle, ParagraphKeeper, ParagraphMetrics, SharedTextMeasurer, TextLayoutOptions,
    TextLayoutResult,
};
use crate::text_attributes::{
    ResolvedTextDirection, TextDecoration, TextForegroundStyle, TextOverflow,
};
use crate::text_range::TextRange;
use crate::text_style::TextStyle;
use crate::unit::{Density, IntSize, Offset, Rect, Size};

/// How many layouts a measurer keeps unless told otherwise, as Compose's `DefaultCacheSize`: a
/// few labels drawn again, a few states of a text toggled.
pub const DEFAULT_TEXT_MEASURER_CACHE_SIZE: usize = 8;

/// What a measurement may leave out, as the default parameters of Compose's
/// `TextMeasurer.measure`: `None` is the measurer's own default.
#[derive(Clone, Debug)]
pub struct TextMeasureOptions {
    /// The style of the whole text.
    pub style: TextStyle,
    /// How what doesn't fit shows.
    pub overflow: TextOverflow,
    /// Whether to break lines at soft breaks, to fit the width.
    pub soft_wrap: bool,
    /// The most lines, at least 1.
    pub max_lines: u32,
    /// How wide and tall it may be: the maximum width is the paragraph's, the minimum the least
    /// the result measures.
    pub constraints: Constraints,
    pub layout_direction: Option<LayoutDirection>,
    pub density: Option<Density>,
    pub font_family_resolver: Option<FontFamilyResolver>,
    /// Lays the text out anew, whatever the cache holds (the new layout is kept, as Compose's).
    pub skip_cache: bool,
    /// The locales of a style naming none.
    pub default_locale_list: Option<LocaleList>,
}

impl Default for TextMeasureOptions {
    fn default() -> Self {
        TextMeasureOptions {
            style: TextStyle::DEFAULT,
            overflow: TextOverflow::Clip,
            soft_wrap: true,
            max_lines: u32::MAX,
            constraints: Constraints::unbounded(),
            layout_direction: None,
            density: None,
            font_family_resolver: None,
            skip_cache: false,
            default_locale_list: None,
        }
    }
}

/// Measures text to draw it, as Compose's `TextMeasurer`: from a composition
/// ([`remember_text_measurer`]), a layout, a draw scope or an event. It keeps the last layouts:
/// measuring again what differs only in what's drawn (the color, a brush, the shadow, the
/// decoration, the draw style) gives the same layout. Cloning shares it.
#[derive(Clone)]
pub struct TextMeasurer(Rc<MeasurerInner>);

struct MeasurerInner {
    default_font_family_resolver: FontFamilyResolver,
    default_locale_list: LocaleList,
    default_density: Density,
    default_layout_direction: LayoutDirection,
    cache: Option<RefCell<TextLayoutCache>>,
}

impl TextMeasurer {
    /// A measurer with these defaults, keeping `cache_size` layouts (none with 0), as Compose's
    /// constructor. The cache is searched, not hashed: a hit costs about 0.18 µs at 8 layouts and
    /// 2.4 µs at 1024, so a large one is for layouts dearer than that.
    pub fn new(
        default_font_family_resolver: FontFamilyResolver,
        default_locale_list: LocaleList,
        default_density: Density,
        default_layout_direction: LayoutDirection,
        cache_size: usize,
    ) -> TextMeasurer {
        TextMeasurer(Rc::new(MeasurerInner {
            default_font_family_resolver,
            default_locale_list,
            default_density,
            default_layout_direction,
            cache: (cache_size > 0).then(|| RefCell::new(TextLayoutCache::new(cache_size))),
        }))
    }

    /// `text`'s layout, as Compose's `measure`: from the cache if a layout there has the same
    /// inputs but for what's drawn, with `options`' own input and style.
    pub fn measure(
        &self,
        text: impl Into<AnnotatedString>,
        options: TextMeasureOptions,
    ) -> TextLayoutResult {
        self.measure_annotated(text.into(), options)
    }

    fn measure_annotated(
        &self,
        text: AnnotatedString,
        options: TextMeasureOptions,
    ) -> TextLayoutResult {
        let inner = &*self.0;
        let input = TextLayoutInput {
            text,
            style: options.style,
            max_lines: options.max_lines,
            soft_wrap: options.soft_wrap,
            overflow: options.overflow,
            density: options.density.unwrap_or(inner.default_density),
            layout_direction: options.layout_direction.unwrap_or(inner.default_layout_direction),
            font_family_resolver: options
                .font_family_resolver
                .unwrap_or_else(|| inner.default_font_family_resolver.clone()),
            default_locale_list: options
                .default_locale_list
                .unwrap_or_else(|| inner.default_locale_list.clone()),
            constraints: options.constraints,
        };
        let cache = inner.cache.as_ref();
        if !options.skip_cache
            && let Some(cached) = cache.and_then(|cache| cache.borrow_mut().get(&input))
        {
            let size = input.constraints.constrain(cached.laid_out_size());
            return cached.copy(input, size);
        }
        let result = lay_out(input);
        if let Some(cache) = cache {
            cache.borrow_mut().put(&result.layout_input, result.clone());
        }
        result
    }
}

impl PartialEq for TextMeasurer {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ComposeArg for TextMeasurer {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl fmt::Debug for TextMeasurer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "TextMeasurer({:p})", Rc::as_ptr(&self.0))
    }
}

/// `input` laid out, as Compose's `TextMeasurer.layout`. The measurer chooses the width
/// (Compose's `finalMaxWidth`, D104) and the lines of an ellipsis without wrapping.
fn lay_out(input: TextLayoutInput) -> TextLayoutResult {
    assert!(input.max_lines >= 1, "max_lines should be greater than 0");
    let platform = input.font_family_resolver.platform().clone();
    let measured = LaidOutParagraph::lay_out(&platform, &input, &input.style);
    let size = input.constraints.constrain(measured.0.metrics.size);
    let queries: Rc<dyn Paragraph> =
        Rc::new(MeasuredParagraph { laid_out: measured.clone(), queries: OnceCell::new() });
    let drawable = Rc::new(DrawableParagraphs {
        measured: Some(measured.clone()),
        elsewhere: RefCell::default(),
    });
    TextLayoutResult::of_measured(input, &measured, queries, drawable, size)
}

/// What a draw scope draws of a [`TextLayoutResult`].
#[derive(Default)]
pub(crate) struct DrawableParagraphs {
    /// The paragraph a [`TextMeasurer`] laid out.
    pub(crate) measured: Option<LaidOutParagraph>,
    /// One laid out from the input when first drawn, where there's no such paragraph on the
    /// text stack of the host drawing it: a text's own result (`on_text_layout`), which doesn't
    /// keep the text's paragraph, or a measurer's on another host's stack.
    elsewhere: RefCell<Option<LaidOutParagraph>>,
}

impl TextLayoutResult {
    /// The result of `measured`, as `TextLayoutResult::new` from a paragraph, with what the
    /// layout told: no query is asked of the platform.
    pub(crate) fn of_measured(
        layout_input: TextLayoutInput,
        measured: &LaidOutParagraph,
        multi_paragraph: Rc<dyn Paragraph>,
        drawable: Rc<DrawableParagraphs>,
        size: IntSize,
    ) -> TextLayoutResult {
        let (metrics, did_exceed_max_lines) = (measured.0.metrics, measured.0.did_exceed_max_lines);
        TextLayoutResult {
            line_count: metrics.line_count,
            first_baseline: metrics.first_baseline,
            last_baseline: metrics.last_baseline,
            // The measured size is the paragraph's rounded up: smaller than it is smaller than
            // the paragraph.
            did_overflow_width: size.width < metrics.size.width,
            did_overflow_height: did_exceed_max_lines || size.height < metrics.size.height,
            layout_input,
            multi_paragraph,
            size,
            drawable,
        }
    }

    /// The paragraph's size rounded up, unconstrained: what a cached layout measures again.
    fn laid_out_size(&self) -> IntSize {
        match &self.drawable.measured {
            Some(measured) => measured.0.metrics.size,
            None => IntSize::new(
                self.multi_paragraph.width().ceil() as i32,
                self.multi_paragraph.height().ceil() as i32,
            ),
        }
    }
}

/// A paragraph of the platform's measurer that results and recordings share: given back when the
/// last of them lets go of it (D1116).
#[derive(Clone)]
pub(crate) struct LaidOutParagraph(Rc<LaidOutInner>);

struct LaidOutInner {
    platform: SharedTextMeasurer,
    keeper: ParagraphKeeper,
    metrics: ParagraphMetrics,
    did_exceed_max_lines: bool,
    /// The paint it's drawn in.
    paint: TextPaint,
    /// The same layout in the last other paints it was asked to draw in, the most recent last.
    repainted: RefCell<Vec<LaidOutParagraph>>,
}

/// The other paints a layout keeps a copy for: a text drawn in a few colors, its own each frame,
/// asks nothing new; an animated color replaces them one by one.
const REPAINTED_COPIES: usize = 4;

impl fmt::Debug for LaidOutParagraph {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "LaidOutParagraph({:?})", self.0.keeper)
    }
}

impl LaidOutParagraph {
    /// `input`'s text laid out on `platform` in `style` (its own style, or that style in another
    /// paint).
    fn lay_out(
        platform: &SharedTextMeasurer,
        input: &TextLayoutInput,
        style: &TextStyle,
    ) -> LaidOutParagraph {
        // Nothing draws what was dropped, and no measurement needs it: this keeps a measurer
        // that isn't a host's (or that draws between frames) from growing.
        platform.release_dropped();
        let options = TextLayoutOptions {
            soft_wrap: input.soft_wrap,
            max_lines: input.max_lines,
            direction: input.layout_direction,
            overflow: input.overflow,
        };
        let paint = TextPaint::of(style);
        // As Compose's: a direction left unspecified is the layout's.
        let style = TextStyle::resolve_defaults(style, input.layout_direction);
        let mut measurer = platform.borrow_mut();
        let handle = measurer.layout_annotated(
            &input.text,
            &style,
            options,
            input.constraints,
            &input.density,
        );
        LaidOutParagraph::of_handle(platform, &mut *measurer, handle, paint)
    }

    fn of_handle(
        platform: &SharedTextMeasurer,
        measurer: &mut dyn crate::text::TextMeasurer,
        handle: ParagraphHandle,
        paint: TextPaint,
    ) -> LaidOutParagraph {
        LaidOutParagraph(Rc::new(LaidOutInner {
            platform: platform.clone(),
            keeper: platform.keeper(handle),
            metrics: measurer.metrics(handle),
            did_exceed_max_lines: measurer.did_exceed_max_lines(handle),
            paint,
            repainted: RefCell::default(),
        }))
    }

    /// This layout in `paint`: itself, the last one asked for if that's the paint, else made
    /// (from the layout if the platform can, as Compose paints a kept layout in another color,
    /// else laid out anew).
    fn in_paint(&self, paint: &TextPaint, input: &TextLayoutInput) -> LaidOutParagraph {
        if self.0.paint == *paint {
            return self.clone();
        }
        {
            let mut repainted = self.0.repainted.borrow_mut();
            if let Some(found) = repainted.iter().position(|copy| copy.0.paint == *paint) {
                let copy = repainted.remove(found);
                repainted.push(copy.clone());
                return copy;
            }
        }
        let platform = &self.0.platform;
        let style = paint.over(&input.style);
        let painted = {
            let mut measurer = platform.borrow_mut();
            measurer.painted(self.0.keeper.handle(), &style).map(|handle| {
                LaidOutParagraph::of_handle(platform, &mut *measurer, handle, paint.clone())
            })
        };
        let copy = painted.unwrap_or_else(|| LaidOutParagraph::lay_out(platform, input, &style));
        let mut repainted = self.0.repainted.borrow_mut();
        if repainted.len() == REPAINTED_COPIES {
            repainted.remove(0);
        }
        repainted.push(copy.clone());
        copy
    }
}

/// What a paragraph is drawn in, of a text's style: all the draw affecting attributes but the
/// blend mode, which is the drawing's.
#[derive(Clone, PartialEq, Debug)]
struct TextPaint {
    foreground: TextForegroundStyle,
    shadow: Option<Shadow>,
    decoration: Option<TextDecoration>,
    draw_style: Option<DrawStyle>,
}

impl TextPaint {
    fn of(style: &TextStyle) -> TextPaint {
        TextPaint {
            foreground: style.foreground(),
            shadow: style.shadow,
            decoration: style.text_decoration,
            draw_style: style.draw_style,
        }
    }

    /// `style` in this paint.
    fn over(&self, style: &TextStyle) -> TextStyle {
        let style = TextStyle {
            shadow: self.shadow,
            text_decoration: self.decoration,
            draw_style: self.draw_style,
            ..style.clone()
        };
        match &self.foreground {
            TextForegroundStyle::Brush { brush, alpha } => {
                style.with_brush(Some(brush.clone()), *alpha)
            }
            foreground => style.with_color(foreground.color()),
        }
    }
}

/// A [`TextLayoutResult`]'s paragraph: what a measurement laid out, its queries asked of the
/// platform when first needed (on Android, each text's whole layout across JNI).
struct MeasuredParagraph {
    laid_out: LaidOutParagraph,
    queries: OnceCell<Rc<dyn Paragraph>>,
}

impl MeasuredParagraph {
    fn queries(&self) -> &dyn Paragraph {
        &**self.queries.get_or_init(|| {
            let inner = &self.laid_out.0;
            inner.platform.borrow().paragraph(inner.keeper.handle())
        })
    }
}

impl Paragraph for MeasuredParagraph {
    fn text(&self) -> &str {
        self.queries().text()
    }
    fn width(&self) -> f32 {
        self.queries().width()
    }
    fn height(&self) -> f32 {
        self.queries().height()
    }
    fn min_intrinsic_width(&self) -> f32 {
        self.queries().min_intrinsic_width()
    }
    fn max_intrinsic_width(&self) -> f32 {
        self.queries().max_intrinsic_width()
    }
    fn first_baseline(&self) -> f32 {
        self.laid_out.0.metrics.first_baseline
    }
    fn last_baseline(&self) -> f32 {
        self.laid_out.0.metrics.last_baseline
    }
    fn did_exceed_max_lines(&self) -> bool {
        self.laid_out.0.did_exceed_max_lines
    }
    fn line_count(&self) -> usize {
        self.queries().line_count()
    }
    fn get_line_left(&self, line: usize) -> f32 {
        self.queries().get_line_left(line)
    }
    fn get_line_right(&self, line: usize) -> f32 {
        self.queries().get_line_right(line)
    }
    fn get_line_top(&self, line: usize) -> f32 {
        self.queries().get_line_top(line)
    }
    fn get_line_bottom(&self, line: usize) -> f32 {
        self.queries().get_line_bottom(line)
    }
    fn get_line_baseline(&self, line: usize) -> f32 {
        self.queries().get_line_baseline(line)
    }
    fn get_line_height(&self, line: usize) -> f32 {
        self.queries().get_line_height(line)
    }
    fn get_line_width(&self, line: usize) -> f32 {
        self.queries().get_line_width(line)
    }
    fn get_line_start(&self, line: usize) -> usize {
        self.queries().get_line_start(line)
    }
    fn get_line_end(&self, line: usize, visible_end: bool) -> usize {
        self.queries().get_line_end(line, visible_end)
    }
    fn is_line_ellipsized(&self, line: usize) -> bool {
        self.queries().is_line_ellipsized(line)
    }
    fn get_line_for_offset(&self, offset: usize) -> usize {
        self.queries().get_line_for_offset(offset)
    }
    fn get_line_for_vertical_position(&self, vertical: f32) -> usize {
        self.queries().get_line_for_vertical_position(vertical)
    }
    fn get_horizontal_position(&self, offset: usize, use_primary_direction: bool) -> f32 {
        self.queries().get_horizontal_position(offset, use_primary_direction)
    }
    fn get_paragraph_direction(&self, offset: usize) -> ResolvedTextDirection {
        self.queries().get_paragraph_direction(offset)
    }
    fn get_bidi_run_direction(&self, offset: usize) -> ResolvedTextDirection {
        self.queries().get_bidi_run_direction(offset)
    }
    fn get_offset_for_position(&self, position: Offset) -> usize {
        self.queries().get_offset_for_position(position)
    }
    fn get_bounding_box(&self, offset: usize) -> Rect {
        self.queries().get_bounding_box(offset)
    }
    fn get_bounding_boxes_for_range(&self, start: usize, end: usize) -> Vec<Rect> {
        self.queries().get_bounding_boxes_for_range(start, end)
    }
    fn get_cursor_rect(&self, offset: usize) -> Rect {
        self.queries().get_cursor_rect(offset)
    }
    fn get_word_boundary(&self, offset: usize) -> TextRange {
        self.queries().get_word_boundary(offset)
    }
    fn get_path_for_range(&self, start: usize, end: usize) -> Path {
        self.queries().get_path_for_range(start, end)
    }
}

/// The cache's key, as Compose's `CacheTextLayoutInput`: an input, compared by what affects its
/// layout.
#[derive(Clone, Debug)]
pub struct CacheTextLayoutInput {
    input: TextLayoutInput,
}

impl CacheTextLayoutInput {
    pub fn new(input: TextLayoutInput) -> CacheTextLayoutInput {
        CacheTextLayoutInput { input }
    }
}

impl PartialEq for CacheTextLayoutInput {
    fn eq(&self, other: &Self) -> bool {
        same_layout(&self.input, &other.input)
    }
}

/// Whether the inputs lay out the same, as Compose's `CacheTextLayoutInput.equals`: the text
/// first, what tells most of them apart at once.
fn same_layout(first: &TextLayoutInput, second: &TextLayoutInput) -> bool {
    first.text.as_str() == second.text.as_str()
        && first.text.has_same_layout_affecting_annotations(&second.text)
        && first.style.has_same_layout_affecting_attributes(&second.style)
        && first.max_lines == second.max_lines
        && first.soft_wrap == second.soft_wrap
        && first.overflow == second.overflow
        && first.density == second.density
        && first.layout_direction == second.layout_direction
        && first.font_family_resolver == second.font_family_resolver
        && first.default_locale_list == second.default_locale_list
        && first.constraints == second.constraints
}

/// The layouts kept, as Compose's `TextLayoutCache`: the last `capacity` used. A cache is small
/// (8 by default): it's searched from the most recent, without a hash of each style and text. A
/// large one pays for that: a hit costs about 0.18 µs at 8 and 2.4 µs at 1024.
pub struct TextLayoutCache {
    capacity: usize,
    /// The least recently used first.
    entries: VecDeque<(CacheTextLayoutInput, TextLayoutResult)>,
}

impl TextLayoutCache {
    /// Panics without a capacity, as Compose's throws.
    pub fn new(capacity: usize) -> TextLayoutCache {
        assert!(capacity > 0, "capacity must be greater than 0");
        TextLayoutCache { capacity, entries: VecDeque::new() }
    }

    /// The result of what lays out as `key`, now the most recently used. The paragraph it holds
    /// is another input's if they differ in what's drawn.
    pub fn get(&mut self, key: &TextLayoutInput) -> Option<TextLayoutResult> {
        let found = self.position_of(key)?;
        let entry = self.entries.remove(found)?;
        let result = entry.1.clone();
        self.entries.push_back(entry);
        Some(result)
    }

    fn position_of(&self, key: &TextLayoutInput) -> Option<usize> {
        self.entries.iter().rposition(|(input, _)| same_layout(&input.input, key))
    }

    /// Keeps `value` for what lays out as `key`, as the most recently used, evicting the least.
    pub fn put(&mut self, key: &TextLayoutInput, value: TextLayoutResult) {
        if let Some(found) = self.position_of(key) {
            self.entries.remove(found);
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((CacheTextLayoutInput::new(key.clone()), value));
    }
}

/// A [`TextMeasurer`] with this place's resolver, locales, density and layout direction, kept
/// while they stay, as Compose's `rememberTextMeasurer`: by default it keeps
/// [`DEFAULT_TEXT_MEASURER_CACHE_SIZE`] layouts.
#[composable(non_restartable)]
pub fn remember_text_measurer(#[default(8)] cache_size: usize) -> TextMeasurer {
    let font_family_resolver = LOCAL_FONT_FAMILY_RESOLVER.current();
    let locale_list = crate::intl::LOCAL_LOCALE_LIST.current();
    let density = crate::host::LOCAL_DENSITY.current();
    let layout_direction = crate::alignment::LOCAL_LAYOUT_DIRECTION.current();
    compose_runtime::remember_keyed(
        (font_family_resolver.clone(), locale_list.clone(), density, layout_direction, cache_size),
        || {
            TextMeasurer::new(
                font_family_resolver,
                locale_list,
                density,
                layout_direction,
                cache_size,
            )
        },
    )
}

/// What `draw_text` may leave out, as the default parameters of Compose's
/// `DrawScope.drawText(textMeasurer, text, …)`.
#[derive(Clone, Debug)]
pub struct DrawTextOptions {
    /// Where its top left goes, in the drawing's coordinates.
    pub top_left: Offset,
    pub style: TextStyle,
    pub overflow: TextOverflow,
    pub soft_wrap: bool,
    pub max_lines: u32,
    /// How wide and tall it is; a side unspecified (NaN) is what's left of the drawing after
    /// `top_left`, as `Size.Unspecified`.
    pub size: Size,
}

impl Default for DrawTextOptions {
    fn default() -> Self {
        DrawTextOptions {
            top_left: Offset::ZERO,
            style: TextStyle::DEFAULT,
            overflow: TextOverflow::Clip,
            soft_wrap: true,
            max_lines: u32::MAX,
            size: Size::UNSPECIFIED,
        }
    }
}

/// What drawing a layout may leave out, as the default parameters of Compose's
/// `DrawScope.drawText(textLayoutResult, …)`: what's unspecified is the layout's own style's.
#[derive(Clone, Debug)]
pub struct DrawTextLayoutOptions {
    pub top_left: Offset,
    /// The opacity put over the color or brush, unspecified (NaN) by default.
    pub alpha: f32,
    pub shadow: Option<Shadow>,
    pub text_decoration: Option<TextDecoration>,
    pub draw_style: Option<DrawStyle>,
}

impl Default for DrawTextLayoutOptions {
    fn default() -> Self {
        DrawTextLayoutOptions {
            top_left: Offset::ZERO,
            alpha: f32::NAN,
            shadow: None,
            text_decoration: None,
            draw_style: None,
        }
    }
}

impl DrawTextLayoutOptions {
    /// `foreground`, and what's given here over `style`.
    fn paint_over(&self, style: &TextStyle, foreground: TextForegroundStyle) -> TextPaint {
        TextPaint {
            foreground,
            shadow: self.shadow.or(style.shadow),
            decoration: self.text_decoration.or(style.text_decoration),
            draw_style: self.draw_style.or(style.draw_style),
        }
    }
}

impl DrawScope<'_> {
    /// Measures and draws `text`, as Compose's `DrawScope.drawText(textMeasurer, text, …)`, in
    /// the density and layout direction of this drawing, to fit it unless `options` give a size.
    pub fn draw_text(
        &mut self,
        text_measurer: &TextMeasurer,
        text: impl Into<AnnotatedString>,
        options: DrawTextOptions,
    ) {
        let constraints = self.text_layout_constraints(options.size, options.top_left);
        let result = text_measurer.measure(
            text,
            TextMeasureOptions {
                style: options.style,
                overflow: options.overflow,
                soft_wrap: options.soft_wrap,
                max_lines: options.max_lines,
                constraints,
                layout_direction: Some(self.layout_direction()),
                density: Some(self.density()),
                ..TextMeasureOptions::default()
            },
        );
        let paint = TextPaint::of(&result.layout_input.style);
        self.draw_laid_out(&result, &paint, options.top_left);
    }

    /// Draws a layout, as Compose's `DrawScope.drawText(textLayoutResult, color, …)`: in `color`
    /// if it's specified, else in its style's color or brush.
    pub fn draw_text_layout(
        &mut self,
        result: &TextLayoutResult,
        color: Color,
        options: DrawTextLayoutOptions,
    ) {
        let style = &result.layout_input.style;
        let foreground = match &style.brush {
            // A layout made with a brush, no color given: as the brush's overload.
            Some(brush) if !color.is_specified() => {
                let alpha = if options.alpha.is_nan() { style.alpha } else { options.alpha };
                TextForegroundStyle::from_brush(Some(brush), alpha)
            }
            _ => {
                let color = if color.is_specified() { color } else { style.color };
                // Unspecified stays so, as Compose's: its alpha is 0 whatever it's modulated by.
                let color =
                    if color.is_specified() { color.modulate(options.alpha) } else { color };
                TextForegroundStyle::from_color(color)
            }
        };
        let paint = options.paint_over(style, foreground);
        self.draw_laid_out(result, &paint, options.top_left);
    }

    /// Draws a layout painted with `brush`, as Compose's
    /// `DrawScope.drawText(textLayoutResult, brush, …)`.
    pub fn draw_text_layout_with_brush(
        &mut self,
        result: &TextLayoutResult,
        brush: crate::brush::Brush,
        options: DrawTextLayoutOptions,
    ) {
        let style = &result.layout_input.style;
        let alpha = if options.alpha.is_nan() { style.alpha } else { options.alpha };
        let paint = options.paint_over(style, TextForegroundStyle::from_brush(Some(&brush), alpha));
        self.draw_laid_out(result, &paint, options.top_left);
    }

    /// `result`'s layout in `paint` at `top_left`, clipped to its size if it overflows and isn't
    /// meant to show.
    fn draw_laid_out(&mut self, result: &TextLayoutResult, paint: &TextPaint, top_left: Offset) {
        let paragraph = self.drawable_of(result).in_paint(paint, &result.layout_input);
        let clipped =
            result.has_visual_overflow() && result.layout_input.overflow != TextOverflow::Visible;
        let (width, height) = (result.size.width as f32, result.size.height as f32);
        let handle = paragraph.0.keeper.handle();
        self.list.laid_out_paragraphs.push(paragraph.0.keeper.clone());
        self.translate(top_left.x, top_left.y, |scope| {
            if clipped {
                scope.clip_rect(Rect::new(0.0, 0.0, width, height), |scope| {
                    scope.draw_paragraph(handle, Offset::ZERO);
                });
            } else {
                scope.draw_paragraph(handle, Offset::ZERO);
            }
        });
    }

    /// The paragraph of `result` this drawing's renderer can draw: what the measurement laid
    /// out, on the tree's text stack; else laid out on it now, once (a text's own result, or a
    /// measurer on another stack: another host's, or a resolver the content provides).
    fn drawable_of(&self, result: &TextLayoutResult) -> LaidOutParagraph {
        let tree = self.current_value_of(&LOCAL_TREE_TEXT_MEASURER);
        let platform = &tree;
        if let Some(measured) = &result.drawable.measured
            && measured.0.platform.ptr_eq(platform)
        {
            return measured.clone();
        }
        let mut elsewhere = result.drawable.elsewhere.borrow_mut();
        if let Some(laid_out) = &*elsewhere
            && laid_out.0.platform.ptr_eq(platform)
        {
            return laid_out.clone();
        }
        let input = &result.layout_input;
        let laid_out = LaidOutParagraph::lay_out(platform, input, &input.style);
        *elsewhere = Some(laid_out.clone());
        laid_out
    }

    /// The constraints of a text drawn at `top_left` in `size`, as Compose's
    /// `textLayoutConstraints`: a side given is fixed, a side unspecified at most what's left
    /// of the drawing.
    fn text_layout_constraints(&self, size: Size, top_left: Offset) -> Constraints {
        let side = |given: f32, drawing: f32, start: f32| {
            if given.is_nan() {
                (0, ((drawing - start).ceil() as i32).max(0))
            } else {
                let fixed = given.ceil() as i32;
                (fixed, fixed)
            }
        };
        let (min_width, max_width) = side(size.width, self.size().width, top_left.x);
        let (min_height, max_height) = side(size.height, self.size().height, top_left.y);
        Constraints::new(min_width, max_width, min_height, max_height)
    }
}

/// Paints a layout where the drawing is, as Compose's `TextPainter`: `draw_text_layout` without a
/// position or overrides.
pub struct TextPainter;

impl TextPainter {
    /// Draws `result` at the origin of `scope`, clipped to its size if it overflows and isn't
    /// meant to show, as Compose's `TextPainter.paint`.
    pub fn paint(scope: &mut DrawScope<'_>, result: &TextLayoutResult) {
        let paint = TextPaint::of(&result.layout_input.style);
        scope.draw_laid_out(result, &paint, Offset::ZERO);
    }
}
