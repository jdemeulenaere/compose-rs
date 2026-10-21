//! `BasicTextField`, as Compose's value-and-callback `BasicTextField`: an editable paragraph
//! with a cursor, focused by a tap, edited by keys and committed text.
//!
//! The node keeps its own copy of the text, so edits arriving before the next recomposition
//! build on each other; a new `value` from composition replaces it.

use compose_runtime::{Callback, composable};
use compose_ui::{
    Color, Constraints, DetachContext, DrawModifierNode, DrawScope, EventContext,
    FocusTargetModifierNode, IntSize, Key, KeyEvent, KeyEventType, KeyInputModifierNode, Layout,
    LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, Offset, Paint, ParagraphHandle, PointerEvent, PointerEventPass,
    PointerEventType, PointerInputModifierNode, Rect, SemanticsConfiguration,
    SemanticsModifierNode, TextLayoutOptions, TextStyle, empty_measure_policy,
};

/// The cursor's width, as Compose's `DefaultCursorThickness`.
const CURSOR_WIDTH: compose_ui::Dp = compose_ui::Dp(2.0);

/// An editable text, as Compose's `BasicTextField(value, onValueChange, …)`.
#[composable]
pub fn BasicTextField(
    value: String,
    on_value_change: impl Fn(String) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(false)] read_only: bool,
    #[default(crate::TextStyle::DEFAULT)] text_style: TextStyle,
    #[default(false)] single_line: bool,
    #[default(crate::Color::BLACK)] cursor_color: Color,
) {
    let element = TextFieldElement {
        value,
        on_value_change,
        enabled,
        read_only,
        style: text_style,
        single_line,
        cursor_color,
    };
    Layout(modifier.then_custom(element), empty_measure_policy(), compose_runtime::callback(|| {}));
}

pub struct TextFieldElement {
    pub value: String,
    pub on_value_change: Callback<dyn Fn(String)>,
    pub enabled: bool,
    pub read_only: bool,
    pub style: TextStyle,
    pub single_line: bool,
    pub cursor_color: Color,
}

impl ModifierNodeElement for TextFieldElement {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(TextFieldNode {
            cursor: self.value.len(),
            text: self.value.clone(),
            on_value_change: self.on_value_change.clone(),
            enabled: self.enabled,
            read_only: self.read_only,
            style: self.style,
            single_line: self.single_line,
            cursor_color: self.cursor_color,
            focused: false,
            pressed: false,
            paragraph: None,
            constraints: None,
            stale: Vec::new(),
            cursor_rect: None,
            pending_tap: None,
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        let Some(node) = node.downcast_mut::<TextFieldNode>() else { return };
        if node.text != self.value
            || node.style != self.style
            || node.single_line != self.single_line
        {
            if node.text != self.value {
                node.text = self.value.clone();
                node.cursor = floor_char_boundary(&node.text, node.cursor);
            }
            node.style = self.style;
            node.single_line = self.single_line;
            node.invalidate_paragraph();
        }
        node.on_value_change = self.on_value_change.clone();
        node.enabled = self.enabled;
        node.read_only = self.read_only;
        node.cursor_color = self.cursor_color;
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<TextFieldElement>().is_some_and(|other| {
            other.value == self.value
                && other.on_value_change.ptr_eq(&self.on_value_change)
                && other.enabled == self.enabled
                && other.read_only == self.read_only
                && other.style == self.style
                && other.single_line == self.single_line
                && other.cursor_color == self.cursor_color
        })
    }

    fn name(&self) -> &'static str {
        "TextField"
    }
}

fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub struct TextFieldNode {
    text: String,
    /// The cursor, as a byte offset at a character boundary.
    cursor: usize,
    on_value_change: Callback<dyn Fn(String)>,
    enabled: bool,
    read_only: bool,
    style: TextStyle,
    single_line: bool,
    cursor_color: Color,
    focused: bool,
    pressed: bool,
    paragraph: Option<ParagraphHandle>,
    constraints: Option<Constraints>,
    stale: Vec<ParagraphHandle>,
    /// Where the cursor is drawn, from the measurer, refreshed at measurement: moving the
    /// cursor asks for one (the paragraph is kept).
    cursor_rect: Option<Rect>,
    /// A tap to put the cursor at, resolved at the next measurement by the measurer's hit test.
    pending_tap: Option<Offset>,
}

impl TextFieldNode {
    fn move_cursor(&mut self, cursor: usize, context: &mut EventContext<'_>) {
        if cursor != self.cursor {
            self.cursor = cursor;
            context.invalidate_measure();
        }
    }

    fn invalidate_paragraph(&mut self) {
        if let Some(paragraph) = self.paragraph.take() {
            self.stale.push(paragraph);
        }
        self.constraints = None;
    }

    /// Replaces the text and moves the cursor, then tells the application.
    fn edit(&mut self, text: String, cursor: usize, context: &mut EventContext<'_>) {
        self.text = text;
        self.cursor = cursor;
        self.invalidate_paragraph();
        context.invalidate_measure();
        let callback = self.on_value_change.clone();
        let value = self.text.clone();
        context.run(move || callback(value));
    }

    fn previous_boundary(&self) -> usize {
        self.text[..self.cursor].char_indices().next_back().map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self) -> usize {
        self.text[self.cursor..].chars().next().map_or(self.cursor, |c| self.cursor + c.len_utf8())
    }

    fn insert(&mut self, inserted: &str, context: &mut EventContext<'_>) {
        let inserted: String = if self.single_line {
            inserted.chars().filter(|c| *c != '\n').collect()
        } else {
            inserted.to_string()
        };
        if inserted.is_empty() {
            return;
        }
        let mut text = self.text.clone();
        text.insert_str(self.cursor, &inserted);
        let cursor = self.cursor + inserted.len();
        self.edit(text, cursor, context);
    }
}

impl ModifierNode for TextFieldNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn on_detach(&mut self, context: &mut DetachContext<'_>) {
        for paragraph in self.paragraph.take().into_iter().chain(self.stale.drain(..)) {
            context.text_measurer.release(paragraph);
        }
    }
}

impl LayoutModifierNode for TextFieldNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        for paragraph in self.stale.drain(..) {
            scope.text_measurer().release(paragraph);
        }
        let cursor_width = scope.density().round_to_px(CURSOR_WIDTH);
        if self.constraints != Some(constraints) || self.paragraph.is_none() {
            if let Some(old) = self.paragraph.take() {
                scope.text_measurer().release(old);
            }
            let density = scope.density();
            let options = TextLayoutOptions {
                soft_wrap: !self.single_line,
                max_lines: if self.single_line { 1 } else { u32::MAX },
            };
            let text_constraints = constraints.offset(cursor_width, 0).loosen();
            let paragraph = scope.text_measurer().layout(
                &self.text,
                &self.style,
                options,
                text_constraints,
                &density,
            );
            self.paragraph = Some(paragraph);
            self.constraints = Some(constraints);
        }
        let paragraph = self.paragraph.expect("laid out");
        if let Some(tap) = self.pending_tap.take() {
            self.cursor = floor_char_boundary(
                &self.text,
                scope.text_measurer().hit_test(paragraph, tap).offset,
            );
        }
        self.cursor_rect = Some(scope.text_measurer().cursor_rect(paragraph, self.cursor));
        let metrics = scope.text_measurer().metrics(paragraph);
        let size = constraints
            .constrain(IntSize::new(metrics.size.width + cursor_width, metrics.size.height));
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }
}

impl DrawModifierNode for TextFieldNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let Some(paragraph) = self.paragraph else { return };
        scope.draw_paragraph(paragraph, Offset::ZERO);
        if self.focused && !self.read_only {
            // The cursor: drawn where the measurer that laid the paragraph out puts it.
            let width = scope.density().to_px(CURSOR_WIDTH);
            let rect = self.cursor_rect.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
            scope.draw_rect(
                Rect::new(rect.left, rect.top, rect.left + width, rect.bottom),
                Paint::fill(self.cursor_color),
            );
        }
    }
}

impl PointerInputModifierNode for TextFieldNode {
    /// A tap focuses the field and puts the cursor where it landed.
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass != PointerEventPass::Main || !self.enabled {
            return;
        }
        let inside = event.position.x >= 0.0
            && event.position.y >= 0.0
            && event.position.x < bounds.width as f32
            && event.position.y < bounds.height as f32;
        match event.kind {
            PointerEventType::Press if !event.consumed => {
                self.pressed = true;
                event.consume();
            }
            PointerEventType::Release
                if std::mem::take(&mut self.pressed) && inside && !event.consumed =>
            {
                event.consume();
                context.request_focus();
                self.pending_tap = Some(event.position);
                context.invalidate_measure();
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.pressed = false;
    }
}

impl FocusTargetModifierNode for TextFieldNode {
    fn can_focus(&self) -> bool {
        self.enabled
    }

    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        self.focused = focused;
        context.invalidate_draw();
    }
}

impl KeyInputModifierNode for TextFieldNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if event.kind != KeyEventType::KeyDown || !self.enabled {
            return false;
        }
        let editable = !self.read_only;
        match event.key {
            Key::Backspace if editable => {
                if self.cursor > 0 {
                    let start = self.previous_boundary();
                    let mut text = self.text.clone();
                    text.replace_range(start..self.cursor, "");
                    self.edit(text, start, context);
                }
                true
            }
            Key::Delete if editable => {
                if self.cursor < self.text.len() {
                    let end = self.next_boundary();
                    let mut text = self.text.clone();
                    text.replace_range(self.cursor..end, "");
                    let cursor = self.cursor;
                    self.edit(text, cursor, context);
                }
                true
            }
            Key::DirectionLeft => {
                let cursor = self.previous_boundary();
                self.move_cursor(cursor, context);
                true
            }
            Key::DirectionRight => {
                let cursor = self.next_boundary();
                self.move_cursor(cursor, context);
                true
            }
            Key::MoveHome => {
                let start = self.text[..self.cursor].rfind('\n').map_or(0, |index| index + 1);
                self.move_cursor(start, context);
                true
            }
            Key::MoveEnd => {
                let end = self.text[self.cursor..]
                    .find('\n')
                    .map_or(self.text.len(), |index| self.cursor + index);
                self.move_cursor(end, context);
                true
            }
            Key::Enter | Key::NumPadEnter if editable && !self.single_line => {
                self.insert("\n", context);
                true
            }
            _ => false,
        }
    }

    fn on_text_input(&mut self, text: &str, context: &mut EventContext<'_>) -> bool {
        if !self.enabled || self.read_only {
            return false;
        }
        self.insert(text, context);
        true
    }
}

impl SemanticsModifierNode for TextFieldNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.editable_text = Some(self.text.clone().into());
        config.editable = true;
        config.focusable |= self.enabled;
        config.focused |= self.focused;
        config.disabled |= !self.enabled;
    }
}
