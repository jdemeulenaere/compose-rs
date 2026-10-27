//! Test support: stand-ins for the foundation layouts (phase E) and a text element, written on
//! compose-ui's public API as foundation will be.

#![allow(dead_code, non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;

thread_local! {
    pub static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    pub static MEASURES: Cell<u32> = const { Cell::new(0) };
}

pub fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

pub fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// The number of policy measurements since the last call.
pub fn take_measures() -> u32 {
    MEASURES.with(|measures| measures.replace(0))
}

/// Children stacked vertically, each at its own width: Column without arrangement.
pub struct ColumnPolicy;

impl MeasurePolicy for ColumnPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        MEASURES.with(|measures| measures.set(measures.get() + 1));
        let loose = Constraints::new(0, constraints.max_width, 0, Constraints::INFINITY);
        let (mut width, mut y) = (0, 0);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, loose);
            scope.place(&placeable, 0, y);
            y += placeable.height;
            width = width.max(placeable.width);
        }
        scope.layout(constraints.constrain_width(width), constraints.constrain_height(y))
    }
}

/// Children side by side.
pub struct RowPolicy;

impl MeasurePolicy for RowPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        MEASURES.with(|measures| measures.set(measures.get() + 1));
        let loose = Constraints::new(0, Constraints::INFINITY, 0, constraints.max_height);
        let (mut x, mut height) = (0, 0);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, loose);
            scope.place(&placeable, x, 0);
            x += placeable.width;
            height = height.max(placeable.height);
        }
        scope.layout(constraints.constrain_width(x), constraints.constrain_height(height))
    }
}

thread_local! {
    static COLUMN: Rc<dyn MeasurePolicy> = Rc::new(ColumnPolicy);
    static ROW: Rc<dyn MeasurePolicy> = Rc::new(RowPolicy);
    static STACK: Rc<dyn MeasurePolicy> = Rc::new(StackMeasurePolicy);
}

#[composable]
pub fn Column(#[default(Modifier)] modifier: Modifier, content: impl Fn() + 'static) {
    Layout(modifier, COLUMN.with(Rc::clone), content);
}

#[composable]
pub fn Row(#[default(Modifier)] modifier: Modifier, content: impl Fn() + 'static) {
    Layout(modifier, ROW.with(Rc::clone), content);
}

#[composable]
pub fn Stack(#[default(Modifier)] modifier: Modifier, content: impl Fn() + 'static) {
    Layout(modifier, STACK.with(Rc::clone), content);
}

/// A leaf of the size its modifiers give it.
#[composable]
pub fn Leaf(modifier: Modifier) {
    Layout(modifier, empty_measure_policy(), callback(|| {}));
}

/// A text leaf: a custom element measuring and drawing a paragraph, as BasicText will.
#[composable]
pub fn Text(#[format] text: SharedString, #[default(Modifier)] modifier: Modifier) {
    Layout(modifier.then_custom(TextElement { text }), empty_measure_policy(), callback(|| {}));
}

pub struct TextElement {
    pub text: SharedString,
}

impl ModifierNodeElement for TextElement {
    type Node = TextNode;
    fn create(&self) -> TextNode {
        TextNode { text: self.text.clone(), paragraph: None, constraints: None, stale: None }
    }

    fn update(&self, node: &mut TextNode) {
        if node.text != self.text {
            node.text = self.text.clone();
            node.stale = node.paragraph.take().or(node.stale.take());
            node.constraints = None;
        }
    }

    fn same(&self, other: &Self) -> bool {
        other.text == self.text
    }
}

pub struct TextNode {
    text: SharedString,
    paragraph: Option<ParagraphHandle>,
    constraints: Option<Constraints>,
    /// A paragraph replaced by an update, released at the next measurement.
    stale: Option<ParagraphHandle>,
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
    fn on_release(&mut self, context: &mut ReleaseContext<'_>) {
        for paragraph in [self.paragraph.take(), self.stale.take()].into_iter().flatten() {
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
        if let Some(stale) = self.stale.take() {
            scope.text_measurer().release(stale);
        }
        if self.constraints != Some(constraints) || self.paragraph.is_none() {
            if let Some(old) = self.paragraph.take() {
                scope.text_measurer().release(old);
            }
            let density = scope.density();
            let paragraph = scope.text_measurer().layout(
                &self.text,
                &TextStyle::DEFAULT,
                TextLayoutOptions::default(),
                constraints,
                &density,
            );
            self.paragraph = Some(paragraph);
            self.constraints = Some(constraints);
        }
        let size = scope.text_measurer().metrics(self.paragraph.expect("laid out")).size;
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }
}

impl DrawModifierNode for TextNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        if let Some(paragraph) = self.paragraph {
            scope.draw_paragraph(paragraph, Offset::ZERO);
        }
    }
}

impl SemanticsModifierNode for TextNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_text(self.text.clone());
    }
}

pub fn host(content: impl Fn() + 'static) -> UiHost {
    take_log();
    let mut host = UiHost::headless(200, 100);
    host.set_content(content);
    host.frame();
    host
}

/// Presses and releases at `x`, `y`, then runs a frame.
pub fn click(host: &mut UiHost, x: f32, y: f32) {
    host.send_pointer(PointerEvent::press(Offset::new(x, y)));
    host.send_pointer(PointerEvent::release(Offset::new(x, y)));
    host.frame();
}

/// The measurer's live paragraphs, through the host's tree.
pub fn live_paragraphs(host: &UiHost) -> usize {
    host.with_tree(|tree| {
        let measurer: &mut dyn TextMeasurer = tree.text_measurer();
        // The headless host's measurer is the fixed one.
        let any = measurer as &mut dyn std::any::Any;
        any.downcast_mut::<FixedTextMeasurer>().map_or(0, |measurer| measurer.live())
    })
}
