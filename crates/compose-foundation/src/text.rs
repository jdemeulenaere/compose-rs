//! `BasicText`, as Compose's: a layout node whose modifier element lays out, measures and draws
//! a paragraph (Compose's `TextStringSimpleElement`), keeping it across passes until the text,
//! style or constraints change.

use compose_runtime::composable;
use compose_ui::{
    Constraints, DetachContext, DrawModifierNode, DrawScope, Layout, LayoutModifierNode,
    Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Offset,
    ParagraphHandle, SemanticsConfiguration, SemanticsModifierNode, SharedString,
    TextLayoutOptions, TextStyle, empty_measure_policy,
};

/// Text, as Compose's `BasicText`: no theme, the style as given.
#[composable]
pub fn BasicText(
    #[format] text: SharedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::TextStyle::DEFAULT)] style: TextStyle,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
) {
    let element = TextElement {
        text,
        style,
        options: TextLayoutOptions { soft_wrap, max_lines, ..Default::default() },
    };
    Layout(modifier.then_custom(element), empty_measure_policy(), compose_runtime::callback(|| {}));
}

/// The modifier element of a text: its string, style and line options.
pub struct TextElement {
    pub text: SharedString,
    pub style: TextStyle,
    pub options: TextLayoutOptions,
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
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<TextNode>()
            && (node.text != self.text || node.style != self.style || node.options != self.options)
        {
            node.text = self.text.clone();
            node.style = self.style;
            node.options = self.options;
            if let Some(paragraph) = node.paragraph.take() {
                node.stale.push(paragraph);
            }
            node.constraints = None;
        }
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<TextElement>().is_some_and(|other| {
            other.text == self.text && other.style == self.style && other.options == self.options
        })
    }

    fn name(&self) -> &'static str {
        "Text"
    }
}

/// A text's node: its laid-out paragraph, kept while the text, style and constraints hold.
pub struct TextNode {
    text: SharedString,
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
            let paragraph = scope.text_measurer().layout(
                &self.text,
                &self.style,
                TextLayoutOptions { direction, ..self.options },
                constraints,
                &density,
            );
            self.paragraph = Some(paragraph);
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
        if let Some(paragraph) = self.paragraph {
            if self.overflows {
                scope.save();
                scope.clip_outline(compose_ui::Outline::Rectangle(scope.bounds()));
                scope.draw_paragraph(paragraph, Offset::ZERO);
                scope.restore();
            } else {
                scope.draw_paragraph(paragraph, Offset::ZERO);
            }
        }
    }
}

impl SemanticsModifierNode for TextNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.text.push(self.text.clone());
    }
}
