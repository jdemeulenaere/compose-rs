//! `BasicText`, as Compose's: a layout node whose modifier element lays out, measures and draws
//! a paragraph (Compose's `TextStringSimpleElement`), keeping it across passes until the text,
//! style or constraints change.

use compose_runtime::composable;
use compose_ui::{
    AnnotatedString, Constraints, DetachContext, DrawModifierNode, DrawScope, Layout,
    LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, Offset, ParagraphHandle, SemanticsConfiguration, SemanticsModifierNode,
    SharedString, TextLayoutOptions, TextStyle, empty_measure_policy,
};

/// Under `DisableSelection`: texts a `SelectionContainer` leaves out.
pub static LOCAL_SELECTION_DISABLED: compose_runtime::CompositionLocal<bool> =
    compose_runtime::CompositionLocal::new("LocalSelectionDisabled", || false);

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
    compose_runtime::CompositionLocalProvider(LOCAL_SELECTION_DISABLED.provides(false), || {
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
    compose_runtime::CompositionLocalProvider(LOCAL_SELECTION_DISABLED.provides(true), &*content);
}

/// Text, as Compose's `BasicText`: no theme, the style as given; what doesn't fit is cut,
/// ends with "…" or is drawn past its bounds (`overflow`).
#[composable]
pub fn BasicText(
    #[format] text: SharedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TextStyle::DEFAULT)] style: TextStyle,
    #[default(crate::TextOverflow::Clip)] overflow: compose_ui::TextOverflow,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
) {
    emit_text(AnnotatedString::new(text), modifier, style, overflow, soft_wrap, max_lines);
}

/// Styled text, as Compose's `BasicText(AnnotatedString, …)`: each span in its style over
/// `style`.
#[composable]
pub fn BasicAnnotatedText(
    text: AnnotatedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TextStyle::DEFAULT)] style: TextStyle,
    #[default(crate::TextOverflow::Clip)] overflow: compose_ui::TextOverflow,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
) {
    emit_text(text, modifier, style, overflow, soft_wrap, max_lines);
}

fn emit_text(
    text: AnnotatedString,
    modifier: Modifier,
    style: TextStyle,
    overflow: compose_ui::TextOverflow,
    soft_wrap: bool,
    max_lines: u32,
) {
    let element = TextElement {
        text,
        style,
        options: TextLayoutOptions { soft_wrap, max_lines, overflow, ..Default::default() },
        selectable: !LOCAL_SELECTION_DISABLED.current(),
    };
    Layout(modifier.then_custom(element), empty_measure_policy(), compose_runtime::callback(|| {}));
}

/// The modifier element of a text: its string, style and line options.
pub struct TextElement {
    pub text: AnnotatedString,
    pub style: TextStyle,
    pub options: TextLayoutOptions,
    /// A `SelectionContainer` can select it (not under `DisableSelection`).
    pub selectable: bool,
}

impl ModifierNodeElement for TextElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(TextNode {
            text: self.text.clone(),
            style: self.style,
            options: self.options,
            paragraph: None,
            constraints: None,
            density: None,
            direction: compose_ui::LayoutDirection::Ltr,
            stale: Vec::new(),
            overflows: false,
            selection: None,
            selection_rects: Vec::new(),
            selectable: self.selectable,
            version: 0,
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<TextNode>()
            && (node.text != self.text || node.style != self.style || node.options != self.options)
        {
            if node.text != self.text {
                // A new text: a selection in it is dropped, as Compose's.
                node.version += 1;
                node.selection = None;
                node.selection_rects.clear();
            }
            node.text = self.text.clone();
            node.style = self.style;
            node.options = self.options;
            if let Some(paragraph) = node.paragraph.take() {
                node.stale.push(paragraph);
            }
            node.constraints = None;
        }
        if let Some(node) = node.downcast_mut::<TextNode>() {
            node.selectable = self.selectable;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<TextElement>().is_some_and(|other| {
            other.text == self.text
                && other.style == self.style
                && other.options == self.options
                && other.selectable == self.selectable
        })
    }

    fn name(&self) -> &'static str {
        "Text"
    }
}

/// A text's node: its laid-out paragraph, kept while the text, style and constraints hold.
pub struct TextNode {
    text: AnnotatedString,
    style: TextStyle,
    options: TextLayoutOptions,
    paragraph: Option<ParagraphHandle>,
    constraints: Option<Constraints>,
    /// The density it was laid out at: a new one lays it out again.
    density: Option<compose_ui::Density>,
    /// The layout direction it was laid out in (its fallback paragraph direction).
    direction: compose_ui::LayoutDirection,
    /// Paragraphs replaced by an update, released at the next measurement (an update can't
    /// reach the measurer).
    stale: Vec<ParagraphHandle>,
    /// The paragraph is larger than the node: drawn clipped.
    overflows: bool,
    /// The part a `SelectionContainer` selected, and its highlight (from the measurer).
    selection: Option<std::ops::Range<usize>>,
    selection_rects: Vec<compose_ui::Rect>,
    selectable: bool,
    /// Bumped when the text changes.
    version: u64,
}

impl ModifierNode for TextNode {
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
    fn on_detach(&mut self, context: &mut DetachContext<'_>) {
        for paragraph in self.paragraph.take().into_iter().chain(self.stale.drain(..)) {
            context.text_measurer.release(paragraph);
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
        // Only the width bounds the layout; a new height alone keeps the paragraph.
        let width_changed = self.constraints.is_none_or(|old| {
            old.min_width != constraints.min_width || old.max_width != constraints.max_width
        });
        let density = scope.density();
        let direction = scope.layout_direction();
        if width_changed
            || self.paragraph.is_none()
            || self.density != Some(density)
            || self.direction != direction
        {
            self.density = Some(density);
            self.direction = direction;
            if let Some(old) = self.paragraph.take() {
                scope.text_measurer().release(old);
            }
            let density = scope.density();
            let paragraph = scope.text_measurer().layout_annotated(
                &self.text,
                &self.style,
                TextLayoutOptions { direction, ..self.options },
                constraints,
                &density,
            );
            self.paragraph = Some(paragraph);
            // The highlight follows the new layout.
            self.selection_rects = match &self.selection {
                Some(range) => scope.text_measurer().selection_rects(paragraph, range.clone()),
                None => Vec::new(),
            };
        }
        self.constraints = Some(constraints);
        let natural = scope.text_measurer().metrics(self.paragraph.expect("laid out")).size;
        let size = constraints.constrain(natural);
        // Text larger than its bounds is clipped, as Compose's `TextOverflow.Clip`.
        self.overflows = natural.width > size.width || natural.height > size.height;
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
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
            scope.draw_rect(*rect, compose_ui::Paint::fill(crate::text::SELECTION_COLOR));
        }
        scope.draw_paragraph(paragraph, Offset::ZERO);
        if clipped {
            scope.restore();
        }
    }
}

impl SemanticsModifierNode for TextNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.text.push(self.text.text.clone());
    }
}

/// The selection's highlight, as Compose's default `TextSelectionColors` (40% of #4286F4).
pub(crate) const SELECTION_COLOR: compose_ui::Color = compose_ui::Color::from_argb(0x664286F4);

impl compose_ui::Selectable for TextNode {
    fn text(&self) -> SharedString {
        self.text.text.clone()
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn offset_at(
        &self,
        position: Offset,
        measurer: &mut dyn compose_ui::TextMeasurer,
    ) -> Option<usize> {
        let paragraph = self.paragraph?;
        let text = self.text.text.as_str();
        let mut offset = measurer.hit_test(paragraph, position).offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        Some(offset)
    }

    fn word_at(&self, offset: usize) -> std::ops::Range<usize> {
        use unicode_segmentation::UnicodeSegmentation;
        let text = self.text.text.as_str();
        text.split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|range| range.contains(&offset) || range.end == offset && offset == text.len())
            .unwrap_or(offset..offset)
    }

    fn set_selection(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        measurer: &mut dyn compose_ui::TextMeasurer,
    ) -> bool {
        let range = range.filter(|range| !range.is_empty());
        if range == self.selection {
            return false;
        }
        self.selection_rects = match (&range, self.paragraph) {
            (Some(range), Some(paragraph)) => measurer.selection_rects(paragraph, range.clone()),
            _ => Vec::new(),
        };
        self.selection = range;
        true
    }
}
