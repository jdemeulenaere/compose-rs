//! `BasicText`, as Compose's: a layout node whose modifier element lays out, measures and draws
//! a paragraph (Compose's `TextStringSimpleElement`), keeping it across passes until the text,
//! style or constraints change.

use compose_runtime::{Callback, composable};
use compose_ui::{
    AnnotatedString, Constraints, DrawModifierNode, DrawScope, IntrinsicMeasureScope, Layout,
    LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, Offset, ReleaseContext, SemanticsConfiguration, SemanticsModifierNode,
    SharedString, TextLayoutResult, TextStyle, empty_measure_policy,
    platform::{ParagraphHandle, TextLayoutOptions},
};

/// Inside a `SelectionContainer` and not under a `DisableSelection` below it: the texts there
/// are selectable, as Compose's `LocalSelectionRegistrar` is set there (and `null` under
/// `DisableSelection` or outside any container).
static LOCAL_SELECTION_REGISTRAR: compose_runtime::CompositionLocal<bool> =
    compose_runtime::CompositionLocal::new("LocalSelectionRegistrar", || false);

/// The colors of a text selection, as Compose's `TextSelectionColors`: the handles' color and
/// the highlight behind the selected text (40 % alpha by convention, to keep the text legible).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextSelectionColors {
    pub handle_color: compose_ui::Color,
    pub background_color: compose_ui::Color,
}

impl compose_runtime::ComposeArg for TextSelectionColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Compose's `DefaultTextSelectionColors`: #4286F4, its highlight at 40 %.
const DEFAULT_TEXT_SELECTION_COLORS: TextSelectionColors = TextSelectionColors {
    handle_color: compose_ui::Color::from_argb(0xFF4286F4),
    background_color: compose_ui::Color::from_argb(0x664286F4),
};

/// The selection colors of the texts and text fields below, as Compose's
/// `LocalTextSelectionColors` (Material's theme provides its primary color's).
pub static LOCAL_TEXT_SELECTION_COLORS: compose_runtime::CompositionLocal<TextSelectionColors> =
    compose_runtime::CompositionLocal::new("LocalTextSelectionColors", || {
        DEFAULT_TEXT_SELECTION_COLORS
    });

/// Makes the texts in `content` selectable, as Compose's `SelectionContainer`: a press and drag
/// selects from where it went down to the pointer, across texts in order; a double-click
/// selects a word, a triple-click a text; Cmd+A (Ctrl+A) selects all and Cmd+C (Ctrl+C) copies,
/// the texts joined by new lines. A click elsewhere in it, or its losing focus, clears it.
#[composable]
#[allow(non_snake_case)]
pub fn SelectionContainer(
    #[default(crate::Modifier)] modifier: Modifier,
    content: impl Fn() + 'static,
) {
    // Selectable again below a `DisableSelection`, as Compose's.
    compose_runtime::CompositionLocalProvider(LOCAL_SELECTION_REGISTRAR.provides(true), || {
        let content = content.clone();
        crate::layout::Box(
            modifier.clone().selection_container(),
            crate::Alignment::TOP_START,
            false,
            compose_runtime::callback(move |_| content()),
        );
    });
}

/// Leaves the texts in `content` out of the `SelectionContainer` around it, as Compose's
/// `DisableSelection`.
#[composable]
#[allow(non_snake_case)]
pub fn DisableSelection(content: impl Fn() + 'static) {
    compose_runtime::CompositionLocalProvider(LOCAL_SELECTION_REGISTRAR.provides(false), &*content);
}

/// Text, as Compose's `BasicText`: no theme, the style as given; what doesn't fit is cut,
/// ends with "…" or is drawn past its bounds (`overflow`). It's at least `min_lines` lines
/// high; `on_text_layout` hears each new layout (its line count, size and overflow).
#[composable]
pub fn BasicText(
    #[format] text: SharedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TextStyle::DEFAULT)] style: TextStyle,
    #[default(None)] on_text_layout: Option<impl Fn(TextLayoutResult) + 'static>,
    #[default(crate::TextOverflow::Clip)] overflow: compose_ui::TextOverflow,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
) {
    emit_text(
        AnnotatedString::new(text),
        modifier,
        style,
        on_text_layout,
        TextLayoutOptions { soft_wrap, max_lines, overflow, ..Default::default() },
        min_lines,
    );
}

/// Styled text, as Compose's `BasicText(AnnotatedString, …)`: each span in its style over
/// `style`.
#[composable]
pub fn BasicAnnotatedText(
    text: AnnotatedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TextStyle::DEFAULT)] style: TextStyle,
    #[default(None)] on_text_layout: Option<impl Fn(TextLayoutResult) + 'static>,
    #[default(crate::TextOverflow::Clip)] overflow: compose_ui::TextOverflow,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
) {
    emit_text(
        text,
        modifier,
        style,
        on_text_layout,
        TextLayoutOptions { soft_wrap, max_lines, overflow, ..Default::default() },
        min_lines,
    );
}

fn emit_text(
    text: AnnotatedString,
    modifier: Modifier,
    style: TextStyle,
    on_text_layout: Option<Callback<dyn Fn(TextLayoutResult)>>,
    options: TextLayoutOptions,
    min_lines: u32,
) {
    validate_min_max_lines(min_lines, options.max_lines);
    // The selection's highlight is read only where the text can be selected, as Compose's
    // `BasicText` reads it only with a selection registrar.
    let selectable = LOCAL_SELECTION_REGISTRAR.current();
    let selection_color = if selectable {
        LOCAL_TEXT_SELECTION_COLORS.current().background_color
    } else {
        compose_ui::Color::UNSPECIFIED
    };
    let element = TextElement {
        text,
        style,
        options,
        min_lines,
        on_text_layout,
        selectable,
        selection_color,
    };
    Layout(modifier.then_custom(element), empty_measure_policy(), compose_runtime::callback(|| {}));
}

/// The modifier element of a text: its string, style and line options.
pub(crate) struct TextElement {
    pub text: AnnotatedString,
    pub style: TextStyle,
    pub options: TextLayoutOptions,
    /// The fewest lines' height it takes, as `minLines`.
    pub min_lines: u32,
    /// Hears each new layout, as `onTextLayout`.
    pub on_text_layout: Option<Callback<dyn Fn(TextLayoutResult)>>,
    /// A `SelectionContainer` can select it (in one, and not under `DisableSelection`).
    pub selectable: bool,
    /// The selection's highlight (`LOCAL_TEXT_SELECTION_COLORS`' background), when selectable.
    pub selection_color: compose_ui::Color,
}

/// The height of `lines` lines of text in `style` (at least one), as Compose's
/// `heightInLines`: the first line's height and each further line's, measured from one and two
/// lines of "H" laid out in the style.
pub(crate) fn height_of_lines(
    scope: &mut IntrinsicMeasureScope<'_>,
    style: &TextStyle,
    lines: u32,
) -> i32 {
    let density = scope.density();
    let options =
        TextLayoutOptions { direction: scope.layout_direction(), ..TextLayoutOptions::default() };
    let mut measurer = scope.text_measurer();
    let mut height = |text: &str| {
        let paragraph = measurer.layout(text, style, options, Constraints::unbounded(), &density);
        let height = measurer.metrics(paragraph).size.height;
        measurer.release(paragraph);
        height
    };
    let first = height("H");
    if lines <= 1 {
        return first;
    }
    let next = height("H\nH") - first;
    first.saturating_add(next.saturating_mul(lines.min(i32::MAX as u32) as i32 - 1))
}

/// Whether the host keeps a text's paragraph at the same layout width (D551).
fn reuses_at_same_width(scope: &IntrinsicMeasureScope<'_>) -> bool {
    scope
        .optimizations()
        .contains(compose_ui::platform::Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH)
}

/// Whether `paragraph`, laid out for this text at constraints `old` (the last ones it was
/// measured with), must be laid out again for `new`, as Compose's `ParagraphLayoutCache` and
/// `MultiParagraphLayoutCache` decide (`newLayoutWillBeDifferent`): not for the same
/// constraints; always for another minimum or maximum width; for another height only when the
/// paragraph is taller than the new maximum height or had more lines than its maximum ("vertical
/// clip changes"). With `reuse_at_same_width` (the host's
/// `Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH`, on by default, D551, D610), another width
/// bound keeps it when the width it's laid out at stays the same ([`same_layout_width`]).
pub(crate) fn new_layout_will_be_different(
    reuse_at_same_width: bool,
    measurer: &mut dyn compose_ui::platform::TextMeasurer,
    paragraph: ParagraphHandle,
    options: TextLayoutOptions,
    old: Constraints,
    new: Constraints,
) -> bool {
    if new == old {
        return false;
    }
    let same_width_bounds = new.min_width == old.min_width && new.max_width == old.max_width;
    if !same_width_bounds
        && !(reuse_at_same_width && same_layout_width(&*measurer, paragraph, options, old, new))
    {
        return true;
    }
    new.max_height < measurer.metrics(paragraph).size.height
        || measurer.did_exceed_max_lines(paragraph)
}

/// The widest a paragraph's lines may be at `constraints`: the maximum width when wrapping or
/// ellipsizing, else unbounded, as in Compose's `finalMaxWidth`.
fn line_width_limit(constraints: Constraints, options: TextLayoutOptions) -> i32 {
    let width_matters = options.soft_wrap || options.overflow.is_ellipsis();
    if width_matters && constraints.has_bounded_width() {
        constraints.max_width
    } else {
        Constraints::INFINITY
    }
}

/// The width a paragraph is laid out at for `constraints`, as Compose's `finalMaxWidth`: the
/// line width limit ([`line_width_limit`]) if the minimum width is the same, else the text's own
/// width (`max_intrinsic_width`, rounded up) kept between the minimum width and that limit.
pub(crate) fn final_max_width(
    constraints: Constraints,
    options: TextLayoutOptions,
    max_intrinsic_width: f32,
) -> i32 {
    let limit = line_width_limit(constraints, options);
    if constraints.min_width == limit {
        return limit;
    }
    (max_intrinsic_width.ceil() as i32).clamp(constraints.min_width, limit)
}

/// Whether `paragraph`, laid out at `old`, would be laid out at the same width for `new`
/// ([`final_max_width`]): then its layout is the same, since a measurer lays a text out at that
/// width alone (its line breaks, alignment, ellipsis and direction follow from it). The text's
/// own width is trusted only when the paragraph fit (no soft line break, no line past its
/// maximum), so that a fresh layout would measure the same own width.
pub(crate) fn same_layout_width(
    measurer: &dyn compose_ui::platform::TextMeasurer,
    paragraph: ParagraphHandle,
    options: TextLayoutOptions,
    old: Constraints,
    new: Constraints,
) -> bool {
    let Some(own) = measurer.max_intrinsic_width(paragraph) else { return false };
    let fit = !measurer.did_exceed_max_lines(paragraph)
        && own.ceil() as i32 <= line_width_limit(old, options);
    fit && final_max_width(old, options, own) == final_max_width(new, options, own)
}

/// Panics unless `0 < min_lines <= max_lines`, as Compose's `validateMinMaxLines`.
pub(crate) fn validate_min_max_lines(min_lines: u32, max_lines: u32) {
    assert!(
        min_lines > 0 && max_lines > 0,
        "both min_lines {min_lines} and max_lines {max_lines} must be greater than zero"
    );
    assert!(
        min_lines <= max_lines,
        "min_lines {min_lines} must be less than or equal to max_lines {max_lines}"
    );
}

pub(crate) fn same_callback<F: ?Sized>(
    first: &Option<Callback<F>>,
    second: &Option<Callback<F>>,
) -> bool {
    match (first, second) {
        (Some(first), Some(second)) => first.ptr_eq(second),
        (None, None) => true,
        _ => false,
    }
}

impl ModifierNodeElement for TextElement {
    type Node = TextNode;
    fn create(&self) -> TextNode {
        TextNode {
            text: self.text.clone(),
            style: self.style.clone(),
            options: self.options,
            min_lines: self.min_lines,
            min_height: None,
            on_text_layout: self.on_text_layout.clone(),
            last_layout: None,
            paragraph: None,
            constraints: None,
            density: None,
            direction: compose_ui::LayoutDirection::Ltr,
            stale: Vec::new(),
            restyle: false,
            probe: None,
            overflows: false,
            selection: None,
            selection_rects: Vec::new(),
            selectable: self.selectable,
            selection_color: self.selection_color,
            version: 0,
        }
    }

    fn update(&self, node: &mut TextNode) {
        if node.text != self.text
            || !node.style.has_same_layout_affecting_attributes(&self.style)
            || node.options != self.options
            || node.min_lines != self.min_lines
        {
            if node.text != self.text {
                // A new text: a selection in it is dropped, as Compose's.
                node.version += 1;
                node.selection = None;
                node.selection_rects.clear();
            }
            node.text = self.text.clone();
            node.style = self.style.clone();
            node.options = self.options;
            node.min_lines = self.min_lines;
            node.min_height = None;
            node.restyle = false;
            let probe = node.probe.take().map(|probe| probe.paragraph);
            node.stale.extend(node.paragraph.take().into_iter().chain(probe));
            node.constraints = None;
        } else if node.style != self.style {
            // Only its paint changed (its color, its decoration): the layout is kept and drawn
            // again in the new paint, as Compose's `updateDraw` (`hasSameDrawAffectingAttributes`).
            node.style = self.style.clone();
            node.restyle = true;
        }
        node.selectable = self.selectable;
        node.selection_color = self.selection_color;
        // A new listener hears the next new layout, as Compose swaps its lambda: the paragraph
        // is kept.
        node.on_text_layout = self.on_text_layout.clone();
    }

    fn same(&self, other: &Self) -> bool {
        other.text == self.text
            && other.style == self.style
            && other.options == self.options
            && other.min_lines == self.min_lines
            && same_callback(&other.on_text_layout, &self.on_text_layout)
            && other.selectable == self.selectable
            && other.selection_color == self.selection_color
    }

    fn name(&self) -> &'static str {
        "Text"
    }
}

/// What `on_text_layout` last heard of a layout: a measurement with the same paragraph and
/// these is no new layout.
#[derive(Clone, Copy, PartialEq)]
struct HeardLayout {
    size: compose_ui::IntSize,
    metrics: compose_ui::platform::ParagraphMetrics,
    did_exceed_max_lines: bool,
}

/// A text's node: its laid-out paragraph, kept while the text, style and constraints hold.
pub(crate) struct TextNode {
    text: AnnotatedString,
    style: TextStyle,
    options: TextLayoutOptions,
    min_lines: u32,
    /// `min_lines`' height, at the density it was measured at.
    min_height: Option<(compose_ui::Density, i32)>,
    on_text_layout: Option<Callback<dyn Fn(TextLayoutResult)>>,
    /// What `on_text_layout` last heard: a measurement with the same paragraph and result is
    /// no new layout.
    last_layout: Option<HeardLayout>,
    paragraph: Option<ParagraphHandle>,
    constraints: Option<Constraints>,
    /// The density it was laid out at: a new one lays it out again.
    density: Option<compose_ui::Density>,
    /// The layout direction it was laid out in (its fallback paragraph direction).
    direction: compose_ui::LayoutDirection,
    /// Paragraphs replaced by an update, released at the next measurement (an update can't
    /// reach the measurer).
    stale: Vec<ParagraphHandle>,
    /// The style changed in paint only since the paragraph was laid out: the next measurement
    /// gives the paragraph the new paint (`TextMeasurer::restyle`).
    restyle: bool,
    /// The paragraph the intrinsics read, when their constraints would lay the text out
    /// differently: kept apart, so intrinsics don't replace the measured paragraph, as Compose's
    /// intrinsics have caches of their own.
    probe: Option<Probe>,
    /// The paragraph is larger than the node: drawn clipped.
    overflows: bool,
    /// The part a `SelectionContainer` selected, and its highlight (from the measurer).
    selection: Option<std::ops::Range<usize>>,
    selection_rects: Vec<compose_ui::Rect>,
    selectable: bool,
    selection_color: compose_ui::Color,
    /// Bumped when the text changes.
    version: u64,
}

/// A paragraph laid out for the intrinsics, at `constraints`, in `density` and `direction`.
struct Probe {
    paragraph: ParagraphHandle,
    constraints: Constraints,
    density: compose_ui::Density,
    direction: compose_ui::LayoutDirection,
}

impl ModifierNode for TextNode {
    /// Reused for another text: a selection in it goes, as with a new text.
    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.selection = None;
        self.selection_rects.clear();
        // Another item's text, even if it reads the same: a selection doesn't move to it.
        self.version += 1;
    }
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_selectable(&mut self) -> Option<&mut dyn compose_ui::Selectable> {
        if self.selectable { Some(self) } else { None }
    }
    fn on_release(&mut self, context: &mut ReleaseContext<'_>) {
        let probe = self.probe.take().map(|probe| probe.paragraph);
        let paragraphs = self.paragraph.take().into_iter().chain(self.stale.drain(..));
        for paragraph in paragraphs.chain(probe) {
            context.release_paragraph(paragraph);
        }
    }
}

impl TextNode {
    /// Lays the text out for `constraints` if Compose's rule says the paragraph would differ
    /// ([`new_layout_will_be_different`]); returns whether it did.
    fn lay_out(
        &mut self,
        scope: &mut MeasureScope<'_>,
        constraints: Constraints,
        density: compose_ui::Density,
        direction: compose_ui::LayoutDirection,
    ) -> bool {
        // The paragraph holds this text in this style, density and direction: only the
        // constraints can differ.
        let current =
            self.paragraph.filter(|_| self.density == Some(density) && self.direction == direction);
        // A font arrived, whose state its layout read: nothing kept of it is good.
        let current =
            current.filter(|&paragraph| !self.is_stale(&*scope.text_measurer(), paragraph));
        let relaid = match (current, self.constraints) {
            (Some(paragraph), Some(old)) => new_layout_will_be_different(
                reuses_at_same_width(scope),
                &mut *scope.text_measurer(),
                paragraph,
                self.options,
                old,
                constraints,
            ),
            _ => true,
        };
        if !relaid {
            return false;
        }
        // What the lines of "H" measured was in the old font.
        self.min_height = None;
        self.density = Some(density);
        self.direction = direction;
        let mut measurer = scope.text_measurer();
        // As Compose's new `Paragraph` from the `ParagraphIntrinsics` it keeps, where the
        // measurer keeps them too; otherwise laid out anew.
        let paragraph = match current.and_then(|old| measurer.relayout(old, constraints)) {
            Some(paragraph) => paragraph,
            None => {
                if let Some(old) = self.paragraph.take() {
                    measurer.release(old);
                }
                measurer.layout_annotated(
                    &self.text,
                    &self.style,
                    TextLayoutOptions { direction, ..self.options },
                    constraints,
                    &density,
                )
            }
        };
        drop(measurer);
        self.paragraph = Some(paragraph);
        // The highlight follows the new layout.
        self.selection_rects = match &self.selection {
            Some(range) => scope.text_measurer().selection_rects(paragraph, range.clone()),
            None => Vec::new(),
        };
        true
    }

    /// Whether a font `paragraph` asked for arrived since, as Compose's `hasStaleResolvedFonts`:
    /// nothing kept of it is good. The read of its state: the node measuring, or the parent
    /// asking its intrinsics, is told when one does. Asked only of a text with fonts of its own,
    /// for a frame's many others: the measure of a screen of text shows it.
    fn is_stale(
        &self,
        measurer: &dyn compose_ui::platform::TextMeasurer,
        paragraph: ParagraphHandle,
    ) -> bool {
        let fonts_may_arrive = self.text.has_span_styles()
            || matches!(self.style.font_family, Some(compose_ui::FontFamily::FontList(_)));
        fonts_may_arrive && measurer.has_stale_resolved_fonts(paragraph)
    }

    /// The paragraph an intrinsic height at `constraints` reads, leaving the measured one and
    /// its constraints as they are: the measured paragraph if the constraints wouldn't lay it
    /// out again, else the intrinsics' own, laid out again by the same rule (D554).
    fn probe_paragraph(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        constraints: Constraints,
        density: compose_ui::Density,
        direction: compose_ui::LayoutDirection,
    ) -> ParagraphHandle {
        let reuse = reuses_at_same_width(scope);
        let mut measurer = scope.text_measurer();
        // A font arrived: the paragraphs kept are as good as none, and nothing's reused of them.
        let measured = self
            .paragraph
            .filter(|_| self.density == Some(density) && self.direction == direction)
            .filter(|&paragraph| !self.is_stale(&*measurer, paragraph));
        let probe_is_stale =
            self.probe.as_ref().is_some_and(|probe| self.is_stale(&*measurer, probe.paragraph));
        if let (Some(paragraph), Some(old)) = (measured, self.constraints)
            && !new_layout_will_be_different(
                reuse,
                &mut *measurer,
                paragraph,
                self.options,
                old,
                constraints,
            )
        {
            return paragraph;
        }
        if let Some(probe) = &mut self.probe
            && !probe_is_stale
            && probe.density == density
            && probe.direction == direction
            && !new_layout_will_be_different(
                reuse,
                &mut *measurer,
                probe.paragraph,
                self.options,
                probe.constraints,
                constraints,
            )
        {
            probe.constraints = constraints;
            return probe.paragraph;
        }
        if let Some(old) = self.probe.take() {
            measurer.release(old.paragraph);
        }
        // From what the measurer kept of the measured paragraph, where it's in this setting.
        let paragraph = measurer.layout_probe(
            measured,
            &self.text,
            &self.style,
            TextLayoutOptions { direction, ..self.options },
            constraints,
            &density,
        );
        self.probe = Some(Probe { paragraph, constraints, density, direction });
        paragraph
    }
}

impl LayoutModifierNode for TextNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        for paragraph in self.stale.drain(..) {
            scope.text_measurer().release(paragraph);
        }
        let density = scope.density();
        let direction = scope.layout_direction();
        // A new paint only: the layout kept, in the new paint (laid out anew if the measurer
        // can't).
        if std::mem::take(&mut self.restyle)
            && let Some(old) = self.paragraph
        {
            let mut measurer = scope.text_measurer();
            self.paragraph = measurer.restyle(old, &self.style);
            if self.paragraph.is_none() {
                measurer.release(old);
                self.constraints = None;
            }
        }
        let relaid = self.lay_out(scope, constraints, density, direction);
        self.constraints = Some(constraints);
        let paragraph = self.paragraph.expect("laid out");
        let metrics = scope.text_measurer().metrics(paragraph);
        let natural = metrics.size;
        let min_height = self.min_lines_height(scope, density);
        let size = constraints
            .constrain(compose_ui::IntSize::new(natural.width, natural.height.max(min_height)));
        // Text larger than its bounds is clipped, as Compose's `TextOverflow.Clip`.
        self.overflows = natural.width > size.width || natural.height > size.height;
        // Each new layout is heard, as Compose's `onTextLayout`: a paragraph laid out again
        // (a new text, style or width), or a new size.
        if let Some(callback) = &self.on_text_layout {
            let heard = HeardLayout {
                size,
                metrics,
                did_exceed_max_lines: scope.text_measurer().did_exceed_max_lines(paragraph),
            };
            if relaid || self.last_layout != Some(heard) {
                self.last_layout = Some(heard);
                // The paragraph's queries, asked for only when there's a callback to hear them.
                let input = compose_ui::TextLayoutInput {
                    text: self.text.clone(),
                    style: self.style.clone(),
                    max_lines: self.options.max_lines,
                    soft_wrap: self.options.soft_wrap,
                    overflow: self.options.overflow,
                    density,
                    layout_direction: direction,
                    font_family_resolver: scope
                        .current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER),
                    default_locale_list: scope.current_value_of(&compose_ui::LOCAL_LOCALE_LIST),
                    constraints,
                };
                let result =
                    TextLayoutResult::new(input, scope.text_measurer().paragraph(paragraph), size);
                let callback = callback.clone();
                scope.run_after_layout(move || callback(result));
            }
        }
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }

    /// The width with every soft line break taken (its longest word), as Compose's text
    /// node (`ParagraphIntrinsics.minIntrinsicWidth`, rounded up).
    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        _height: i32,
    ) -> i32 {
        self.intrinsic_widths(scope).0
    }

    /// The height laid out at `width`, at least `min_lines` high, as Compose's text node.
    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        width: i32,
    ) -> i32 {
        self.intrinsic_height(scope, width)
    }

    /// The width without soft line breaks, as Compose's text node
    /// (`ParagraphIntrinsics.maxIntrinsicWidth`, rounded up).
    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        _height: i32,
    ) -> i32 {
        self.intrinsic_widths(scope).1
    }

    /// As `min_intrinsic_height`, as Compose's.
    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        width: i32,
    ) -> i32 {
        self.intrinsic_height(scope, width)
    }
}

impl TextNode {
    /// At least `min_lines` high, as Compose's `heightInLines` (0 for one line).
    fn min_lines_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        density: compose_ui::Density,
    ) -> i32 {
        if self.min_lines <= 1 {
            return 0;
        }
        match self.min_height {
            Some((at, height)) if at == density => height,
            _ => {
                let height = height_of_lines(scope, &self.style, self.min_lines);
                self.min_height = Some((density, height));
                height
            }
        }
    }

    /// The text's minimum and maximum intrinsic widths, rounded up, from its paragraph in this
    /// density and direction (the measured one, else the intrinsics' own, laid out unbounded):
    /// they don't depend on the width it's laid out at. A measurer that doesn't know its texts'
    /// minimum width gives the maximum for it; one that doesn't know either, the width laid out
    /// unbounded.
    fn intrinsic_widths(&mut self, scope: &mut IntrinsicMeasureScope<'_>) -> (i32, i32) {
        let density = scope.density();
        let direction = scope.layout_direction();
        let measured = self
            .paragraph
            .filter(|_| self.density == Some(density) && self.direction == direction)
            .filter(|&paragraph| !self.is_stale(&*scope.text_measurer(), paragraph));
        let paragraph = match measured {
            Some(paragraph) => paragraph,
            None => self.probe_paragraph(scope, Constraints::unbounded(), density, direction),
        };
        let own_width = scope.text_measurer().max_intrinsic_width(paragraph);
        let max = match own_width {
            Some(width) => width.ceil() as i32,
            None => {
                let unbounded =
                    self.probe_paragraph(scope, Constraints::unbounded(), density, direction);
                scope.text_measurer().metrics(unbounded).size.width
            }
        };
        let min = scope
            .text_measurer()
            .min_intrinsic_width(paragraph)
            .map_or(max, |width| width.ceil() as i32);
        (min, max)
    }

    /// The text's height laid out at `width`, at least `min_lines` high, as Compose's
    /// `intrinsicHeight`.
    fn intrinsic_height(&mut self, scope: &mut IntrinsicMeasureScope<'_>, width: i32) -> i32 {
        let density = scope.density();
        let direction = scope.layout_direction();
        let constraints = Constraints::new(0, width, 0, Constraints::INFINITY);
        let paragraph = self.probe_paragraph(scope, constraints, density, direction);
        let height = scope.text_measurer().metrics(paragraph).size.height;
        height.max(self.min_lines_height(scope, density))
    }
}

impl DrawModifierNode for TextNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let Some(paragraph) = self.paragraph else { return };
        // Clipped as the text is, its selection's highlight included.
        let clipped = self.overflows && self.options.overflow != compose_ui::TextOverflow::Visible;
        if clipped {
            scope.save();
            scope.clip_outline(compose_ui::Outline::Rectangle(scope.bounds()));
        }
        for rect in &self.selection_rects {
            scope.draw_rect(*rect, compose_ui::Paint::fill(self.selection_color));
        }
        scope.draw_paragraph(paragraph, Offset::ZERO);
        if clipped {
            scope.restore();
        }
    }
}

impl SemanticsModifierNode for TextNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_text(self.text.text().clone());
        config.get_text_layout_result_of_node();
    }

    fn text_layout_result(
        &mut self,
        context: &mut compose_ui::EventContext<'_>,
    ) -> Option<TextLayoutResult> {
        let (paragraph, constraints) = (self.paragraph?, self.constraints?);
        let input = compose_ui::TextLayoutInput {
            text: self.text.clone(),
            style: self.style.clone(),
            max_lines: self.options.max_lines,
            soft_wrap: self.options.soft_wrap,
            overflow: self.options.overflow,
            density: self.density?,
            layout_direction: self.direction,
            font_family_resolver: context.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER),
            default_locale_list: context.current_value_of(&compose_ui::LOCAL_LOCALE_LIST),
            constraints,
        };
        let measurer = context.text_measurer();
        let size = constraints.constrain(measurer.metrics(paragraph).size);
        Some(TextLayoutResult::new(input, measurer.paragraph(paragraph), size))
    }
}

impl compose_ui::Selectable for TextNode {
    fn text(&self) -> SharedString {
        self.text.text().clone()
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn offset_at(
        &self,
        position: Offset,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> Option<usize> {
        let paragraph = self.paragraph?;
        let text = self.text.as_str();
        let mut offset = measurer.borrow().hit_test(paragraph, position).offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        Some(offset)
    }

    /// As Compose's `getWordBoundary` (D1108).
    fn word_at(&self, offset: usize) -> std::ops::Range<usize> {
        compose_ui::platform::word_boundary(self.text.as_str(), offset).range()
    }

    fn set_selection(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> bool {
        let range = range.filter(|range| !range.is_empty());
        if range == self.selection {
            return false;
        }
        self.selection_rects = match (&range, self.paragraph) {
            (Some(range), Some(paragraph)) => {
                measurer.borrow().selection_rects(paragraph, range.clone())
            }
            _ => Vec::new(),
        };
        self.selection = range;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use compose_ui::TextOverflow;

    #[test]
    fn the_final_width_is_composes_final_max_width() {
        let wrap = TextLayoutOptions::default();
        let no_wrap = TextLayoutOptions { soft_wrap: false, ..wrap };
        let ellipsis = TextLayoutOptions { overflow: TextOverflow::Ellipsis, ..no_wrap };
        let loose = Constraints::new(0, 300, 0, 100);
        // The text's own width, rounded up, within the bounds.
        assert_eq!(final_max_width(loose, wrap, 76.2), 77);
        assert_eq!(final_max_width(loose, wrap, 500.0), 300);
        assert_eq!(final_max_width(Constraints::new(120, 300, 0, 100), wrap, 76.2), 120);
        // A fixed width is the width, whatever the text's.
        assert_eq!(final_max_width(Constraints::fixed(200, 10), wrap, 76.2), 200);
        // Unwrapped and not ellipsized, the maximum doesn't bound it.
        assert_eq!(final_max_width(loose, no_wrap, 500.0), 500);
        assert_eq!(final_max_width(Constraints::fixed(200, 10), no_wrap, 500.0), 500);
        // Ellipsized, it does.
        assert_eq!(final_max_width(loose, ellipsis, 500.0), 300);
        // Unbounded.
        let unbounded = Constraints::new(0, Constraints::INFINITY, 0, 100);
        assert_eq!(final_max_width(unbounded, wrap, 500.0), 500);
    }
}
