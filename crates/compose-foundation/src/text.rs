//! `BasicText`, as Compose's: a layout node whose modifier element lays out, measures and draws
//! a paragraph (Compose's `TextStringSimpleElement`), keeping it across passes until the text,
//! style or constraints change.

use compose_runtime::{Callback, composable};
use std::rc::Rc;

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

/// The color a field is highlighted with once autofilled, as Compose's Android
/// `autofillHighlightColor()`.
pub const DEFAULT_AUTOFILL_HIGHLIGHT_COLOR: compose_ui::Color =
    compose_ui::Color::from_argb(0x4dff_eb3b);

/// What a field is highlighted with once autofilled, until its text changes, as Compose's
/// `LocalAutofillHighlightBrush` (D1685). Compose's deprecated `LocalAutofillHighlightColor`
/// isn't ported.
pub static LOCAL_AUTOFILL_HIGHLIGHT_BRUSH: compose_runtime::CompositionLocal<compose_ui::Brush> =
    compose_runtime::CompositionLocal::new("LocalAutofillHighlightBrush", || {
        compose_ui::Brush::solid(DEFAULT_AUTOFILL_HIGHLIGHT_COLOR)
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
    // What the tree holding its selection tells its handles, composed after its content.
    let handles = compose_runtime::remember(|| {
        let handles = std::rc::Rc::new(compose_ui::ContainerHandles::new());
        // Its drags show the platform's magnifier, where it has one (D2073).
        if compose_ui::is_platform_magnifier_supported() == Some(true) {
            handles.enable_magnifier();
        }
        handles
    });
    handles.set_haptic_feedback(compose_ui::LOCAL_HAPTIC_FEEDBACK.current());
    // Its toolbar and right-click menu (D2041).
    let menu = compose_runtime::remember(|| {
        std::rc::Rc::new(crate::text_context_menu::container::ContainerMenu::new(handles.clone()))
    });
    // Selectable again below a `DisableSelection`, as Compose's.
    compose_runtime::CompositionLocalProvider(LOCAL_SELECTION_REGISTRAR.provides(true), || {
        let content = content.clone();
        let shown = handles.clone();
        let menu = menu.clone();
        // Its magnifier, on the container's coordinates, as Compose's `SelectionManager.modifier`
        // (`selectionMagnifier`): where the platform has one, shown while a drag says (D2073).
        let magnifier = if handles.has_magnifier() {
            let source: std::rc::Rc<dyn crate::selection_magnifier::SelectionMagnifierSource> =
                handles.clone();
            crate::selection_magnifier::selection_magnifier(source)
        } else {
            Modifier
        };
        crate::layout::Box(
            modifier
                .clone()
                .selection_container_with_handles(handles.clone())
                .then(magnifier)
                .then(menu.modifier()),
            crate::Alignment::TOP_START,
            // Its content measured with its constraints, the least included, as Compose's
            // `SimpleLayout` (D2073).
            true,
            compose_runtime::callback(move |_| {
                content();
                crate::selection_handles::SelectionContainerHandles(shown.clone());
                crate::text_context_menu::container::ContainerDropdownMenu(menu.clone());
            }),
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
    // A selectable text shows the I-beam, inside what the caller's modifier says, as Compose's
    // `SelectionController.modifier`.
    let modifier = if selectable {
        modifier.pointer_hover_icon(compose_ui::PointerIcon::TEXT)
    } else {
        modifier
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
            baselines: None,
            selection: None,
            selection_rects: Vec::new(),
            selectable: self.selectable,
            selection_color: self.selection_color,
            version: 0,
            paragraphs: Paragraphs::of(&self.text),
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
            // Laid out anew, as paragraphs if it has paragraph styles (their platform paragraphs
            // go back to the measurer once dropped).
            node.paragraphs = Paragraphs::of(&node.text);
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

/// The constraints a text is laid out at and its result reports, as Compose's
/// `layoutWithConstraints`' final ones: with more than one minimum line, their minimum height that
/// many lines' (`MinLinesConstrainer.coerceMinLines`), at most the maximum height (D2054).
fn final_constraints(constraints: Constraints, min_lines: u32, min_height: i32) -> Constraints {
    if min_lines <= 1 {
        return constraints;
    }
    let min_height = min_height.clamp(0, constraints.max_height);
    Constraints::new(
        constraints.min_width,
        constraints.max_width,
        min_height,
        constraints.max_height,
    )
}

/// What `on_text_layout` last heard of a layout: a measurement with the same paragraph and
/// these is no new layout. New constraints are, as Compose's `layoutWithConstraints` makes a new
/// result for them even when it keeps the layout (D2054).
#[derive(Clone, Copy, PartialEq)]
struct HeardLayout {
    constraints: Constraints,
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
    /// Its first and last baselines, for `ModifierNode::baselines`.
    baselines: Option<(i32, i32)>,
    /// The part a `SelectionContainer` selected, and its highlight (from the measurer).
    selection: Option<std::ops::Range<usize>>,
    selection_rects: Vec<compose_ui::Rect>,
    selectable: bool,
    selection_color: compose_ui::Color,
    /// Bumped when the text changes.
    version: u64,
    /// A text with paragraph styles: laid out as paragraphs, each its own platform paragraph,
    /// where `paragraph` and `probe` stay `None` (D2053). A text without them has none, and lays
    /// out as one platform paragraph.
    paragraphs: Option<Box<Paragraphs>>,
}

/// A text with paragraph styles laid out as Compose's `MultiParagraphLayoutCache` lays it out
/// (D2053): split into paragraphs (`MultiParagraphIntrinsics`), each in its style, laid out one
/// below the other (`MultiParagraph`) at the width Compose's `finalMaxWidth` gives, kept while
/// Compose's `newLayoutWillBeDifferent` says so.
#[derive(Default)]
struct Paragraphs {
    intrinsics: Option<KeptIntrinsics>,
    layout: Option<ParagraphsLayout>,
    /// The last intrinsic height asked for, and its width, from `intrinsics`.
    intrinsic_height: Option<(i32, i32)>,
}

/// The text's paragraphs and their widths, and what they were made in.
#[derive(Clone)]
struct KeptIntrinsics {
    intrinsics: Rc<compose_ui::MultiParagraphIntrinsics>,
    density: compose_ui::Density,
    direction: compose_ui::LayoutDirection,
    resolver: compose_ui::FontFamilyResolver,
    locales: compose_ui::LocaleList,
}

impl KeptIntrinsics {
    /// Whether they're the text's in `scope`'s setting: its density, layout direction, font
    /// resolver and locales, and no font they resolved to arrived since.
    fn hold_in(&self, scope: &IntrinsicMeasureScope<'_>) -> bool {
        self.density == scope.density()
            && self.direction == scope.layout_direction()
            && self.resolver == scope.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER)
            && self.locales == scope.current_value_of(&compose_ui::LOCAL_LOCALE_LIST)
            && !self.intrinsics.has_stale_resolved_fonts()
    }
}

/// The text laid out at `constraints` from `from`'s paragraphs, `size` big: kept through other
/// paragraphs made since for another setting (intrinsics asked in another direction), as Compose's
/// layout cache keeps its result.
struct ParagraphsLayout {
    multi_paragraph: Rc<compose_ui::MultiParagraph>,
    from: KeptIntrinsics,
    constraints: Constraints,
    size: compose_ui::IntSize,
}

impl Paragraphs {
    /// Paragraphs to lay `text` out as, if it has paragraph styles: a text without them lays out
    /// as one platform paragraph, its node's fast path.
    fn of(text: &AnnotatedString) -> Option<Box<Paragraphs>> {
        text.has_paragraph_styles().then(Box::default)
    }
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
    fn baselines(&self) -> Option<(i32, i32)> {
        self.baselines
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
    fn reads_state(&self) -> bool {
        self.text.has_span_styles()
            || matches!(self.style.font_family, Some(compose_ui::FontFamily::FontList(_)))
    }
    fn on_release(&mut self, context: &mut ReleaseContext<'_>) {
        let probe = self.probe.take().map(|probe| probe.paragraph);
        let paragraphs = self.paragraph.take().into_iter().chain(self.stale.drain(..));
        for paragraph in paragraphs.chain(probe) {
            context.release_paragraph(paragraph);
        }
        // Its paragraphs' platform paragraphs go back once dropped (at the end of the next draw).
        if let Some(paragraphs) = &mut self.paragraphs {
            **paragraphs = Paragraphs::default();
        }
    }
}

/// The most lines `input` is laid out with, as Compose's `finalMaxLines`: one for an ellipsis
/// without wrapping.
fn final_max_lines(input: &compose_ui::TextLayoutInput) -> u32 {
    if !input.soft_wrap && input.overflow.is_ellipsis() { 1 } else { input.max_lines }
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

/// The most lines a text is laid out with, as Compose's `finalMaxLines`: one for an ellipsis
/// without wrapping.
fn final_max_lines_of(options: TextLayoutOptions) -> u32 {
    if !options.soft_wrap && options.overflow.is_ellipsis() { 1 } else { options.max_lines }
}

/// `intrinsics`' paragraphs laid out for `constraints`, as Compose's `MultiParagraphLayoutCache`
/// (`layoutText`): at Compose's `finalMaxWidth` (their widest paragraph's width kept within the
/// constraints), with Compose's `finalMaxLines`; known as drawn in `paint`'s paint when given (a
/// layout to draw), else in each paragraph's own (one only measured).
fn lay_out_paragraphs(
    intrinsics: &Rc<compose_ui::MultiParagraphIntrinsics>,
    options: TextLayoutOptions,
    constraints: Constraints,
    paint: Option<&TextStyle>,
) -> compose_ui::MultiParagraph {
    let width = final_max_width(constraints, options, intrinsics.max_intrinsic_width());
    let constraints = Constraints::fit_prioritizing_width(0, width, 0, constraints.max_height);
    let max_lines = final_max_lines_of(options);
    let intrinsics = intrinsics.clone();
    match paint {
        Some(style) => compose_ui::MultiParagraph::new_in_paint_of(
            intrinsics,
            constraints,
            max_lines,
            options.overflow,
            style,
        ),
        None => {
            compose_ui::MultiParagraph::new(intrinsics, constraints, max_lines, options.overflow)
        }
    }
}

/// Whether `layout` must be laid out again for `new`, as Compose's `MultiParagraphLayoutCache`
/// decides (`newLayoutWillBeDifferent`), as [`new_layout_will_be_different`] for one paragraph:
/// with `reuse_at_same_width` (D551), another width bound keeps it when its width
/// (`finalMaxWidth`, from the paragraphs' own widths) stays the same.
fn paragraphs_layout_will_be_different(
    reuse_at_same_width: bool,
    options: TextLayoutOptions,
    layout: &ParagraphsLayout,
    new: Constraints,
) -> bool {
    let old = layout.constraints;
    if new == old {
        return false;
    }
    if new.min_width != old.min_width || new.max_width != old.max_width {
        let own = layout.from.intrinsics.max_intrinsic_width();
        let same_width = final_max_width(old, options, own) == final_max_width(new, options, own);
        if !(reuse_at_same_width && same_width) {
            return true;
        }
    }
    let multi_paragraph = &layout.multi_paragraph;
    (new.max_height as f32) < multi_paragraph.height() || multi_paragraph.did_exceed_max_lines()
}

impl TextNode {
    /// What the text is laid out from, as Compose's `TextLayoutInput`.
    fn layout_input(
        &self,
        constraints: Constraints,
        density: compose_ui::Density,
        direction: compose_ui::LayoutDirection,
        font_family_resolver: compose_ui::FontFamilyResolver,
        default_locale_list: compose_ui::LocaleList,
    ) -> compose_ui::TextLayoutInput {
        compose_ui::TextLayoutInput {
            text: self.text.clone(),
            style: self.style.clone(),
            max_lines: self.options.max_lines,
            soft_wrap: self.options.soft_wrap,
            overflow: self.options.overflow,
            density,
            layout_direction: direction,
            font_family_resolver,
            default_locale_list,
            constraints,
            placeholders: None,
        }
    }

    /// The text's paragraphs in this scope's density, layout direction, font resolver and
    /// locales: those kept, else made anew (Compose's `setLayoutDirection`, its style's defaults
    /// resolved in the direction), and what was measured of the old ones forgotten. Made anew too
    /// once a font one of them resolved to arrives (the read of its state: the node measuring, or
    /// the parent asking its intrinsics, is told when one does).
    fn paragraph_intrinsics(&mut self, scope: &IntrinsicMeasureScope<'_>) -> KeptIntrinsics {
        let paragraphs = self.paragraphs.as_mut().expect("a text of paragraphs");
        if let Some(kept) = &paragraphs.intrinsics
            && kept.hold_in(scope)
        {
            return kept.clone();
        }
        paragraphs.intrinsic_height = None;
        // What the lines of "H" measured may have been in another font.
        self.min_height = None;
        let (density, direction) = (scope.density(), scope.layout_direction());
        let resolver = scope.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER);
        let locales = scope.current_value_of(&compose_ui::LOCAL_LOCALE_LIST);
        let style = TextStyle::resolve_defaults(&self.style, direction);
        let intrinsics = Rc::new(compose_ui::MultiParagraphIntrinsics::new(
            self.text.clone(),
            &style,
            &[],
            density,
            resolver.clone(),
            self.options.soft_wrap,
            locales.clone(),
        ));
        let kept = KeptIntrinsics { intrinsics, density, direction, resolver, locales };
        paragraphs.intrinsics = Some(kept.clone());
        kept
    }

    /// Measures a text with paragraph styles, as Compose's `TextAnnotatedStringNode` with its
    /// `MultiParagraphLayoutCache` (D2053): its paragraphs laid out again only when Compose's rule
    /// says they'd differ, its size their width (the widest's own, at most the width laid out at)
    /// and height in the constraints, at least `min_lines` high.
    fn measure_paragraphs(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let direction = scope.layout_direction();
        let min_height = self.min_lines_height(scope, density);
        // Laid out, kept and reported at Compose's final constraints (D2054).
        let incoming = constraints;
        let constraints = final_constraints(incoming, self.min_lines, min_height);
        let reuse = reuses_at_same_width(scope);
        let options = self.options;
        // A new paint only since the last measurement.
        let restyle = std::mem::take(&mut self.restyle);
        // Its layout kept as Compose's cache keeps it: in the same setting, where the new
        // constraints wouldn't change it.
        let kept = self.paragraphs_layout().is_some_and(|layout| {
            layout.from.hold_in(scope)
                && !paragraphs_layout_will_be_different(reuse, options, layout, constraints)
        });
        if !kept {
            let from = self.paragraph_intrinsics(scope);
            let multi_paragraph =
                lay_out_paragraphs(&from.intrinsics, options, constraints, Some(&self.style));
            let paragraphs = self.paragraphs.as_mut().expect("a text of paragraphs");
            paragraphs.layout = Some(ParagraphsLayout {
                multi_paragraph: Rc::new(multi_paragraph),
                from,
                constraints,
                size: compose_ui::IntSize::ZERO,
            });
        }
        let relaid = !kept;
        let paragraphs = self.paragraphs.as_mut().expect("a text of paragraphs");
        let layout = paragraphs.layout.as_mut().expect("laid out");
        if kept
            && restyle
            && let Some(painted) = layout.multi_paragraph.in_paint_of(&self.style)
        {
            // The kept layout in the new paint: its paragraphs swapped for their copies in it,
            // so the ones laid out in the old paint go (D2054), as a plain text's restyle
            // replaces its paragraph. The same layout: nothing new is heard.
            layout.multi_paragraph = Rc::new(painted);
        }
        let multi_paragraph = layout.multi_paragraph.clone();
        // As Compose's: as wide as its widest paragraph, within the width it's laid out at.
        let own_width = layout.from.intrinsics.max_intrinsic_width();
        let width = own_width.min(multi_paragraph.width());
        let natural =
            compose_ui::IntSize::new(width.ceil() as i32, multi_paragraph.height().ceil() as i32);
        // The result's size, as Compose's (in the final constraints); the node's in its own.
        let size = constraints.constrain(natural);
        let node_size = incoming.constrain(size);
        layout.constraints = constraints;
        layout.size = size;
        // Compose's `hasVisualOverflow`: clipped where it's larger than the node, or cut by its
        // maximum lines.
        self.overflows = (node_size.width as f32) < multi_paragraph.width()
            || (node_size.height as f32) < multi_paragraph.height()
            || multi_paragraph.did_exceed_max_lines();
        if relaid {
            // The highlight follows the new layout.
            self.selection_rects = match &self.selection {
                Some(range) => multi_paragraph.selection_rects(range.clone()),
                None => Vec::new(),
            };
        }
        let (first_baseline, last_baseline) =
            (multi_paragraph.first_baseline(), multi_paragraph.last_baseline());
        if let Some(callback) = &self.on_text_layout {
            let metrics = compose_ui::platform::ParagraphMetrics {
                size: natural,
                first_baseline,
                last_baseline,
                line_count: multi_paragraph.line_count() as u32,
            };
            let did_exceed_max_lines = multi_paragraph.did_exceed_max_lines();
            let heard = HeardLayout { constraints, size, metrics, did_exceed_max_lines };
            if relaid || self.last_layout != Some(heard) {
                self.last_layout = Some(heard);
                let input = self.layout_input(
                    constraints,
                    density,
                    direction,
                    scope.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER),
                    scope.current_value_of(&compose_ui::LOCAL_LOCALE_LIST),
                );
                let result = TextLayoutResult::new(input, multi_paragraph, size);
                let callback = callback.clone();
                scope.run_after_layout(move || callback(result));
            }
        }
        let placeable =
            scope.measure(content, Constraints::fixed(node_size.width, node_size.height));
        scope.place(&placeable, 0, 0);
        let baselines = (first_baseline.round() as i32, last_baseline.round() as i32);
        if self.baselines != Some(baselines) {
            self.baselines = Some(baselines);
            scope.baselines_changed();
        }
        scope.layout(node_size.width, node_size.height)
    }

    /// A text with paragraph styles' height laid out at `width`, at least `min_lines` high, as
    /// Compose's `MultiParagraphLayoutCache.intrinsicHeight`: the last width's kept; the measured
    /// layout's when the width wouldn't lay it out again, else its paragraphs laid out apart.
    fn paragraphs_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        width: i32,
    ) -> i32 {
        let density = scope.density();
        let intrinsics = self.paragraph_intrinsics(scope).intrinsics;
        let min_height = self.min_lines_height(scope, density);
        let reuse = reuses_at_same_width(scope);
        let options = self.options;
        let paragraphs = self.paragraphs.as_mut().expect("a text of paragraphs");
        if let Some((at, height)) = paragraphs.intrinsic_height
            && at == width
        {
            return height.max(min_height);
        }
        let constraints = Constraints::new(0, width, 0, Constraints::INFINITY);
        let height = match &paragraphs.layout {
            Some(layout)
                if Rc::ptr_eq(&layout.from.intrinsics, &intrinsics)
                    && !paragraphs_layout_will_be_different(
                        reuse,
                        options,
                        layout,
                        constraints,
                    ) =>
            {
                layout.multi_paragraph.height()
            }
            _ => lay_out_paragraphs(&intrinsics, options, constraints, None).height(),
        };
        let height = height.ceil() as i32;
        paragraphs.intrinsic_height = Some((width, height));
        height.max(min_height)
    }

    /// The layout of a text with paragraph styles, once measured.
    fn paragraphs_layout(&self) -> Option<&ParagraphsLayout> {
        self.paragraphs.as_ref()?.layout.as_ref()
    }

    /// `offset` (at most the text's length) kept to what its layout holds, for a query that
    /// takes an offset: past the paragraphs the maximum lines laid out, the end of the last one
    /// laid out (D2054). Compose's `MultiParagraph` throws there (`findParagraphByIndex`), as
    /// ours panics; its selection asks only where the text shows, ours for a selection kept
    /// through a text that shows fewer lines. One platform paragraph holds the whole text.
    fn laid_out_offset(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        match self.paragraphs_layout() {
            Some(layout) if offset < self.text.len() => {
                let infos = layout.multi_paragraph.paragraph_info_list();
                offset.min(infos.last().map_or(0, |last| last.end_index))
            }
            _ => offset,
        }
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
        if self.paragraphs.is_some() {
            return self.measure_paragraphs(scope, content, constraints);
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
        // The result's constraints and size, as Compose's (D2054); the node's size in its own.
        let reported = final_constraints(constraints, self.min_lines, min_height);
        let size = reported.constrain(natural);
        let node_size = constraints.constrain(size);
        // Text larger than its bounds is clipped, as Compose's `TextOverflow.Clip`.
        self.overflows = natural.width > node_size.width || natural.height > node_size.height;
        // Each new layout is heard, as Compose's `onTextLayout`: a paragraph laid out again
        // (a new text, style or width), new constraints, or a new size.
        if let Some(callback) = &self.on_text_layout {
            let heard = HeardLayout {
                constraints: reported,
                size,
                metrics,
                did_exceed_max_lines: scope.text_measurer().did_exceed_max_lines(paragraph),
            };
            if relaid || self.last_layout != Some(heard) {
                self.last_layout = Some(heard);
                // The paragraph's queries, asked for only when there's a callback to hear them.
                let input = Rc::new(self.layout_input(
                    reported,
                    density,
                    direction,
                    scope.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER),
                    scope.current_value_of(&compose_ui::LOCAL_LOCALE_LIST),
                ));
                let paragraph = scope.text_measurer().paragraph(paragraph);
                // Painted, it lays its input out again (D2053), the result's input shared.
                let max_lines = final_max_lines(&input);
                let multi_paragraph =
                    compose_ui::MultiParagraph::of_paragraph_of(&input, paragraph, max_lines);
                let result = TextLayoutResult::new(input, Rc::new(multi_paragraph), size);
                let callback = callback.clone();
                scope.run_after_layout(move || callback(result));
            }
        }
        let placeable =
            scope.measure(content, Constraints::fixed(node_size.width, node_size.height));
        scope.place(&placeable, 0, 0);
        let baselines =
            (metrics.first_baseline.round() as i32, metrics.last_baseline.round() as i32);
        if self.baselines != Some(baselines) {
            self.baselines = Some(baselines);
            scope.baselines_changed();
        }
        scope.layout(node_size.width, node_size.height)
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
        if self.paragraphs.is_some() {
            // Its widest paragraph's, as Compose's `MultiParagraphIntrinsics`.
            let intrinsics = self.paragraph_intrinsics(scope).intrinsics;
            let min = intrinsics.min_intrinsic_width().ceil() as i32;
            return (min, intrinsics.max_intrinsic_width().ceil() as i32);
        }
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
        if self.paragraphs.is_some() {
            return self.paragraphs_intrinsic_height(scope, width);
        }
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
        // One platform paragraph, or a text's paragraphs (D2053).
        let multi_paragraph = match &self.paragraphs {
            Some(paragraphs) => match &paragraphs.layout {
                Some(layout) => Some(&layout.multi_paragraph),
                None => return,
            },
            None => None,
        };
        let paragraph = self.paragraph;
        if multi_paragraph.is_none() && paragraph.is_none() {
            return;
        }
        // Clipped as the text is, its selection's highlight included.
        let clipped = self.overflows && self.options.overflow != compose_ui::TextOverflow::Visible;
        if clipped {
            scope.save();
            scope.clip_outline(compose_ui::Outline::Rectangle(scope.bounds()));
        }
        for rect in &self.selection_rects {
            scope.draw_rect(*rect, compose_ui::Paint::fill(self.selection_color));
        }
        match (multi_paragraph, paragraph) {
            // In its style's paint, as Compose's node paints its `MultiParagraph`.
            (Some(multi_paragraph), _) => scope.draw_multi_paragraph(multi_paragraph, &self.style),
            (None, Some(paragraph)) => scope.draw_paragraph(paragraph, Offset::ZERO),
            (None, None) => {}
        }
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

    /// Nothing autofill reads (D1688).
    fn apply_autofill_semantics(&self, _config: &mut SemanticsConfiguration) {}

    fn text_layout_result(
        &mut self,
        context: &mut compose_ui::EventContext<'_>,
    ) -> Option<TextLayoutResult> {
        let resolver = context.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER);
        let locales = context.current_value_of(&compose_ui::LOCAL_LOCALE_LIST);
        if let Some(paragraphs) = &self.paragraphs {
            // The text's paragraphs, as laid out: what Compose's semantics hear.
            let layout = paragraphs.layout.as_ref()?;
            let (density, direction) = (layout.from.density, layout.from.direction);
            let input =
                self.layout_input(layout.constraints, density, direction, resolver, locales);
            return Some(TextLayoutResult::new(input, layout.multi_paragraph.clone(), layout.size));
        }
        let (paragraph, constraints) = (self.paragraph?, self.reported_constraints()?);
        let input = Rc::new(self.layout_input(
            constraints,
            self.density?,
            self.direction,
            resolver,
            locales,
        ));
        let measurer = context.text_measurer();
        let size = constraints.constrain(measurer.metrics(paragraph).size);
        let max_lines = final_max_lines(&input);
        let paragraph = measurer.paragraph(paragraph);
        let multi_paragraph =
            compose_ui::MultiParagraph::of_paragraph_of(&input, paragraph, max_lines);
        Some(TextLayoutResult::new(input, Rc::new(multi_paragraph), size))
    }
}

impl TextNode {
    /// The constraints a plain text's result reports: those it was measured at, with the
    /// minimum lines' height (D2054).
    fn reported_constraints(&self) -> Option<Constraints> {
        let constraints = self.constraints?;
        let min_height = match self.min_height {
            Some((_, height)) => height,
            None => 0,
        };
        Some(final_constraints(constraints, self.min_lines, min_height))
    }
}

impl TextNode {
    /// `ask` with its paragraph's layout and its size, once laid out.
    fn laid_out<R>(
        &self,
        measurer: &compose_ui::platform::SharedTextMeasurer,
        ask: impl FnOnce(&dyn compose_ui::Paragraph, compose_ui::Size) -> R,
    ) -> Option<R> {
        if self.paragraphs.is_some() {
            // Its paragraphs' queries, over the whole text (Compose's `MultiParagraph`'s).
            let layout = self.paragraphs_layout()?;
            let size = compose_ui::Size::new(layout.size.width as f32, layout.size.height as f32);
            return Some(ask(&*layout.multi_paragraph, size));
        }
        let (paragraph, constraints) = (self.paragraph?, self.reported_constraints()?);
        let measurer = measurer.borrow();
        let size = constraints.constrain(measurer.metrics(paragraph).size);
        let layout = measurer.paragraph(paragraph);
        Some(ask(&*layout, compose_ui::Size::new(size.width as f32, size.height as f32)))
    }

    /// `ask` of the line at `offset`, as Compose's delegate's line queries: none past the lines
    /// laid out (an offset past a cut text's laid-out paragraphs is kept to them, D2054).
    fn line_query(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
        ask: impl FnOnce(&dyn compose_ui::Paragraph, usize) -> f32,
    ) -> Option<f32> {
        let offset = self.laid_out_offset(offset);
        self.laid_out(measurer, |layout, _| {
            let line = layout.get_line_for_offset(offset);
            (line < layout.line_count()).then(|| ask(layout, line))
        })?
    }
}

/// The end of what a text laid out as `layout` within `size` shows, as Compose's
/// `lastVisibleOffset`: its last line, or the last whose top is above its bottom when it's cut
/// by its height, to that line's visible end.
fn last_visible_offset(layout: &dyn compose_ui::Paragraph, size: compose_ui::Size) -> usize {
    let lines = layout.line_count();
    if lines == 0 {
        return 0;
    }
    let mut last = lines - 1;
    if layout.height() > size.height {
        while last > 0 && layout.get_line_top(last) >= size.height {
            last -= 1;
        }
    }
    layout.get_line_end(last, true)
}

/// The direction a selection's end at `offset` says, as Compose's `getTextDirectionForOffset`: on
/// an empty line, or in an empty text, its paragraph's (a run there says left to right, whatever
/// the paragraph's direction); elsewhere its run's (D2062). The lines before and after are its
/// characters' neighbours' (Compose's `offset - 1` and `offset + 1`, in bytes here); `end` is the
/// end of what's laid out, past which there's no line: an offset past it (the end of a text cut
/// by its maximum lines) is the run's, as before (D2054).
fn text_direction_for_offset(
    layout: &dyn compose_ui::Paragraph,
    text: &str,
    end: usize,
    offset: usize,
) -> compose_ui::ResolvedTextDirection {
    if offset > end {
        return layout.get_bidi_run_direction(offset);
    }
    let empty_line = text.is_empty() || {
        let (Some(before), Some(after)) = (text.get(..offset), text.get(offset..)) else {
            return layout.get_bidi_run_direction(offset);
        };
        let line = layout.get_line_for_offset(offset);
        let other = |at: Option<usize>| at.is_none_or(|at| layout.get_line_for_offset(at) != line);
        let previous = before.chars().next_back().map(|it| offset - it.len_utf8());
        let next = after.chars().next().map(|it| offset + it.len_utf8()).filter(|at| *at <= end);
        other(previous) && other(next)
    };
    if empty_line {
        layout.get_paragraph_direction(offset)
    } else {
        layout.get_bidi_run_direction(offset)
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
        let text = self.text.as_str();
        let offset = if self.paragraphs.is_some() {
            // The paragraph at its height's, as Compose's `MultiParagraph.getOffsetForPosition`.
            self.paragraphs_layout()?.multi_paragraph.get_offset_for_position(position)
        } else {
            measurer.borrow().hit_test(self.paragraph?, position).offset
        };
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        Some(offset)
    }

    /// As Compose's `getWordBoundary` (D1108).
    fn word_at(&self, offset: usize) -> std::ops::Range<usize> {
        if let Some(layout) = self.paragraphs_layout() {
            // Within its paragraph, as Compose's `MultiParagraph.getWordBoundary`.
            let offset = self.laid_out_offset(offset);
            return layout.multi_paragraph.get_word_boundary(offset).range();
        }
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
        self.selection_rects = match (&range, self.paragraph, self.paragraphs_layout()) {
            // Each paragraph's highlight, below the ones above it (D2053).
            (Some(range), _, Some(layout)) => layout.multi_paragraph.selection_rects(range.clone()),
            (Some(range), Some(paragraph), None) => {
                measurer.borrow().selection_rects(paragraph, range.clone())
            }
            _ => Vec::new(),
        };
        self.selection = range;
        true
    }

    /// As Compose's `MultiWidgetSelectionDelegate.getHandlePosition`: at the offset kept to the
    /// last visible one (`lastVisibleOffset`), where `getSelectionHandleCoordinates` says.
    fn handle_position(
        &self,
        offset: usize,
        start: bool,
        crossed: bool,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> Option<Offset> {
        self.laid_out(measurer, |layout, size| {
            let offset = offset.min(last_visible_offset(layout, size));
            crate::selection_handles::selection_handle_coordinates(
                layout,
                self.text.as_str(),
                offset,
                (start, crossed),
                size,
            )
        })?
    }

    fn bounding_box(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> compose_ui::Rect {
        let text = self.text.as_str();
        if text.is_empty() {
            return compose_ui::Rect::ZERO;
        }
        // Compose's `offset.coerceIn(0, textLength - 1)`, in bytes: the character the offset is
        // in, the last past the text.
        let mut offset = offset.min(text.len() - 1);
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        self.laid_out(measurer, |layout, _| layout.get_bounding_box(offset))
            .unwrap_or(compose_ui::Rect::ZERO)
    }

    fn cursor_rect(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> compose_ui::Rect {
        let text = self.text.as_str();
        let mut offset = self.laid_out_offset(offset);
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        self.laid_out(measurer, |layout, _| layout.get_cursor_rect(offset))
            .unwrap_or(compose_ui::Rect::ZERO)
    }

    fn range_of_line_containing(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> std::ops::Range<usize> {
        self.laid_out(measurer, |layout, size| {
            let visible = last_visible_offset(layout, size);
            if visible < 1 {
                return 0..0;
            }
            let line = layout.get_line_for_offset(offset.min(visible - 1));
            layout.get_line_start(line)..layout.get_line_end(line, true)
        })
        .unwrap_or(0..0)
    }

    fn last_visible_offset(&self, measurer: &compose_ui::platform::SharedTextMeasurer) -> usize {
        self.laid_out(measurer, last_visible_offset).unwrap_or(0)
    }

    fn line_height(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> f32 {
        if self.text.as_str().is_empty() {
            return 0.0;
        }
        let max_lines = self.options.max_lines.max(1) as usize;
        // The line of what's laid out; an offset past its end has none.
        let laid_out = self.laid_out_offset(offset);
        self.laid_out(measurer, |layout, _| {
            let lines = layout.line_count();
            if lines == 0 {
                return 0.0;
            }
            let line = layout.get_line_for_offset(laid_out).min(max_lines - 1).min(lines - 1);
            if offset > layout.get_line_end(line, false) {
                return 0.0;
            }
            layout.get_line_height(line)
        })
        .unwrap_or(0.0)
    }

    /// As Compose's `MultiWidgetSelectionDelegate.getLineLeft`: none past the lines laid out.
    fn line_left(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> Option<f32> {
        self.line_query(offset, measurer, |layout, line| layout.get_line_left(line))
    }

    fn line_right(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> Option<f32> {
        self.line_query(offset, measurer, |layout, line| layout.get_line_right(line))
    }

    /// As Compose's `getCenterYForOffset`: half way down the line.
    fn center_y_for_offset(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> Option<f32> {
        self.line_query(offset, measurer, |layout, line| {
            let (top, bottom) = (layout.get_line_top(line), layout.get_line_bottom(line));
            (bottom - top) / 2.0 + top
        })
    }

    fn direction_at(
        &self,
        offset: usize,
        measurer: &compose_ui::platform::SharedTextMeasurer,
    ) -> compose_ui::ResolvedTextDirection {
        let text = self.text.as_str();
        if let Some(layout) = self.paragraphs_layout() {
            // What's laid out ends at its last paragraph's end (D2054); the text's own end, past
            // it in a cut text, says what the cut says, as every offset between them does (D2071).
            let infos = layout.multi_paragraph.paragraph_info_list();
            let end = infos.last().map_or(0, |last| last.end_index).min(text.len());
            let offset = self.laid_out_offset(offset).min(end);
            return text_direction_for_offset(&*layout.multi_paragraph, text, end, offset);
        }
        match self.paragraph {
            Some(paragraph) => {
                let measurer = measurer.borrow();
                let layout = measurer.paragraph(paragraph);
                text_direction_for_offset(&*layout, text, text.len(), offset.min(text.len()))
            }
            None => compose_ui::ResolvedTextDirection::Ltr,
        }
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
