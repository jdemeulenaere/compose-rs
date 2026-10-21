//! `BasicTextField`, as Compose's value-and-callback `BasicTextField`: an editable paragraph
//! with a cursor, focused by a tap, edited by keys and committed text.
//!
//! The node keeps its own copy of the text, so edits arriving before the next recomposition
//! build on each other; a new `value` from composition replaces it, and an edit the application
//! didn't take (its `value` unchanged by the next frame: rejected, or filtered back to the old
//! value) is undone, as Compose's value-based text field shows its `value`. The cursor moves by
//! grapheme clusters, and starts at the start.

use std::rc::Rc;

use compose_runtime::{Callback, composable};
use compose_ui::{
    Color, Constraints, DetachContext, DrawModifierNode, DrawScope, EventContext,
    FocusTargetModifierNode, IntSize, Key, KeyEvent, KeyEventType, KeyInputModifierNode, Layout,
    LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, Offset, Paint, ParagraphHandle, PointerEvent, PointerEventPass,
    PointerEventType, PointerInputModifierNode, Rect, SemanticsConfiguration,
    SemanticsModifierNode, TextLayoutOptions, TextStyle, empty_measure_policy,
};
use unicode_segmentation::UnicodeSegmentation;

/// The selection's highlight, as Compose's default `TextSelectionColors` (40% of #4286F4).
const SELECTION_COLOR: Color = Color::from_argb(0x664286F4);

/// The cursor's width, as Compose's `DefaultCursorThickness`.
const CURSOR_WIDTH: compose_ui::Dp = compose_ui::Dp(2.0);

/// An editable text, as Compose's `BasicTextField(value, onValueChange, …)`.
///
/// `decoration_box` lays out what surrounds the text (a container, a label, a placeholder) and
/// calls `inner_text_field` where the text goes, as Compose's `decorationBox`. The decoration
/// is part of the field: `modifier` applies around it, the field is one semantics node with
/// it (`mergeDescendants`, as `CoreTextField`), and a tap anywhere in it focuses the field and
/// puts the cursor at the offset nearest to the tap.
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
    #[default(crate::inner_text_field_only)] decoration_box: impl Fn(Callback) + 'static,
) {
    let element: Rc<dyn ModifierNodeElement> = Rc::new(TextFieldElement {
        value,
        on_value_change,
        enabled,
        read_only,
        style: text_style,
        single_line,
        cursor_color,
    });
    let inner_text_field = Callback::new(move || {
        Layout(
            Modifier.then_element(compose_ui::Element::Custom(element.clone())),
            empty_measure_policy(),
            compose_runtime::callback(|| {}),
        );
    });
    crate::layout::Box(
        modifier.semantics(true, |_| {}).then_custom(FocusOnTap),
        crate::Alignment::TOP_START,
        // The decoration's size is the field's: a field given a size passes it on.
        true,
        compose_runtime::callback(move |_| decoration_box(inner_text_field.clone())),
    );
}

/// The decoration of a field without one: the text alone, as Compose's default `decorationBox`.
pub fn inner_text_field_only(inner_text_field: Callback) {
    inner_text_field();
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
            // At the start, as Compose's `TextFieldValue` with no selection given.
            cursor: 0,
            anchor: 0,
            selection_rects: Vec::new(),
            text: self.value.clone(),
            value: self.value.clone(),
            awaiting_value: false,
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
            density: None,
            cursor_rect: None,
            pending_tap: None,
            scroll_x: 0.0,
            overflows: false,
        })
    }

    fn update(&self, node: &mut dyn ModifierNode) {
        let Some(node) = node.downcast_mut::<TextFieldNode>() else { return };
        if node.value != self.value {
            node.value = self.value.clone();
            node.awaiting_value = false;
        }
        if node.text != self.value
            || node.style != self.style
            || node.single_line != self.single_line
        {
            if node.text != self.value {
                node.text = self.value.clone();
                node.cursor = floor_char_boundary(&node.text, node.cursor);
                node.anchor = floor_char_boundary(&node.text, node.anchor);
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
    /// The text shown: `value`, with the edits made since the last frame.
    text: String,
    /// The application's value, as composition last gave it.
    value: String,
    /// An edit was made: unless the application changes `value` by the next measurement, the
    /// edit is undone.
    awaiting_value: bool,
    /// The cursor, as a byte offset at a character boundary.
    cursor: usize,
    /// The selection's other end: the selection is between it and the cursor (none when equal).
    anchor: usize,
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
    density: Option<compose_ui::Density>,
    stale: Vec<ParagraphHandle>,
    /// Where the cursor is drawn, from the measurer, refreshed at measurement: moving the
    /// cursor asks for one (the paragraph is kept).
    cursor_rect: Option<Rect>,
    /// The selection's highlight, from the measurer, refreshed at measurement.
    selection_rects: Vec<Rect>,
    /// A tap to put the cursor at, resolved at the next measurement by the measurer's hit test.
    pending_tap: Option<Offset>,
    /// How far a single-line field's text is scrolled to keep the cursor in view, in pixels.
    scroll_x: f32,
    /// The paragraph is wider or taller than the field: drawn clipped.
    overflows: bool,
}

impl TextFieldNode {
    /// Moves the cursor, extending the selection or collapsing it.
    fn move_cursor(&mut self, cursor: usize, extend: bool, context: &mut EventContext<'_>) {
        let anchor = if extend { self.anchor } else { cursor };
        if cursor != self.cursor || anchor != self.anchor {
            self.cursor = cursor;
            self.anchor = anchor;
            context.invalidate_measure();
        }
    }

    /// The selected bytes (empty: none).
    fn selection(&self) -> std::ops::Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    /// Replaces the selection (or inserts at the cursor) with `with`.
    fn replace_selection(&mut self, with: &str, context: &mut EventContext<'_>) {
        let range = self.selection();
        let mut text = self.text.clone();
        text.replace_range(range.clone(), with);
        self.edit(text, range.start + with.len(), context);
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
        self.anchor = cursor;
        self.awaiting_value = true;
        self.invalidate_paragraph();
        context.invalidate_measure();
        let callback = self.on_value_change.clone();
        let value = self.text.clone();
        context.run(move || callback(value));
    }

    /// The start of the grapheme cluster before the cursor.
    fn previous_boundary(&self) -> usize {
        self.text[..self.cursor]
            .graphemes(true)
            .next_back()
            .map_or(0, |cluster| self.cursor - cluster.len())
    }

    /// The end of the grapheme cluster after the cursor.
    fn next_boundary(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map_or(self.cursor, |cluster| self.cursor + cluster.len())
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
        self.replace_selection(&inserted, context);
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
        // An edit the application didn't take is undone.
        if std::mem::take(&mut self.awaiting_value) && self.text != self.value {
            self.text = self.value.clone();
            self.cursor = floor_char_boundary(&self.text, self.cursor);
            self.anchor = floor_char_boundary(&self.text, self.anchor);
            self.invalidate_paragraph();
        }
        for paragraph in self.stale.drain(..) {
            scope.text_measurer().release(paragraph);
        }
        let cursor_width = scope.density().round_to_px(CURSOR_WIDTH);
        if self.constraints != Some(constraints)
            || self.paragraph.is_none()
            || self.density != Some(scope.density())
        {
            self.density = Some(scope.density());
            if let Some(old) = self.paragraph.take() {
                scope.text_measurer().release(old);
            }
            let density = scope.density();
            let options = TextLayoutOptions {
                soft_wrap: !self.single_line,
                max_lines: if self.single_line { 1 } else { u32::MAX },
            };
            let text_constraints = constraints.offset(-cursor_width, 0).loosen();
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
                scope
                    .text_measurer()
                    .hit_test(paragraph, Offset::new(tap.x + self.scroll_x, tap.y))
                    .offset,
            );
        }
        self.cursor_rect = Some(scope.text_measurer().cursor_rect(paragraph, self.cursor));
        self.selection_rects.clear();
        let selection = self.selection();
        if !selection.is_empty() {
            self.selection_rects = scope.text_measurer().selection_rects(paragraph, selection);
        }
        let metrics = scope.text_measurer().metrics(paragraph);
        let size = constraints
            .constrain(IntSize::new(metrics.size.width + cursor_width, metrics.size.height));
        self.overflows =
            metrics.size.width + cursor_width > size.width || metrics.size.height > size.height;
        // A single line scrolls sideways to keep the cursor in view, as Compose's.
        if self.single_line {
            let visible = (size.width - cursor_width).max(0) as f32;
            let x = self.cursor_rect.map_or(0.0, |rect| rect.left);
            if x - self.scroll_x > visible {
                self.scroll_x = x - visible;
            }
            if x < self.scroll_x {
                self.scroll_x = x;
            }
            let max = (metrics.size.width as f32 - visible).max(0.0);
            self.scroll_x = self.scroll_x.clamp(0.0, max);
        } else {
            self.scroll_x = 0.0;
        }
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }
}

impl DrawModifierNode for TextFieldNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let Some(paragraph) = self.paragraph else { return };
        let clipped = self.overflows || self.scroll_x != 0.0;
        if clipped {
            scope.save();
            scope.clip_outline(compose_ui::Outline::Rectangle(scope.bounds()));
        }
        let shift =
            |rect: Rect, by: f32| Rect::new(rect.left - by, rect.top, rect.right - by, rect.bottom);
        if self.focused {
            for rect in &self.selection_rects {
                scope.draw_rect(shift(*rect, self.scroll_x), Paint::fill(SELECTION_COLOR));
            }
        }
        scope.draw_paragraph(paragraph, Offset::new(-self.scroll_x, 0.0));
        if self.focused && !self.read_only {
            // The cursor: drawn where the measurer that laid the paragraph out puts it.
            let width = scope.density().to_px(CURSOR_WIDTH);
            let rect =
                shift(self.cursor_rect.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)), self.scroll_x);
            scope.draw_rect(
                Rect::new(rect.left, rect.top, rect.left + width, rect.bottom),
                Paint::fill(self.cursor_color),
            );
        }
        if clipped {
            scope.restore();
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
                // The cursor where the tap landed, now, so text typed before the next frame
                // goes there.
                match self.paragraph {
                    Some(paragraph) => {
                        let position =
                            Offset::new(event.position.x + self.scroll_x, event.position.y);
                        let offset = context.text_measurer().hit_test(paragraph, position).offset;
                        self.cursor = floor_char_boundary(&self.text, offset);
                        self.anchor = self.cursor;
                    }
                    None => self.pending_tap = Some(event.position),
                }
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

    /// A tap on a decoration around the field (a Material container's padding) puts the cursor
    /// nearest it.
    fn on_focus_tap(&mut self, position: Offset, context: &mut EventContext<'_>) {
        if let Some(paragraph) = self.paragraph {
            let position = Offset::new(position.x + self.scroll_x, position.y);
            let offset = context.text_measurer().hit_test(paragraph, position).offset;
            self.cursor = floor_char_boundary(&self.text, offset);
            self.anchor = self.cursor;
            context.invalidate_measure();
        }
    }

    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        self.focused = focused;
        context.invalidate_draw();
    }
}

impl KeyInputModifierNode for TextFieldNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        let editable = !self.read_only;
        let shortcut = event.modifiers.meta || event.modifiers.ctrl;
        // Typed keys: the text arrives as committed text (`on_text_input`); the field takes the
        // key, down and up, so its ancestors' shortcuts and clicks don't.
        if editable && !shortcut && matches!(event.key, Key::Character(_) | Key::Space) {
            return true;
        }
        if event.kind != KeyEventType::KeyDown {
            return false;
        }
        let extend = event.modifiers.shift;
        // Shortcuts: Cmd on macOS, Ctrl elsewhere (either is taken).
        if (event.modifiers.meta || event.modifiers.ctrl)
            && let Key::Character(c) = event.key
        {
            let selected = self.text[self.selection()].to_string();
            match c.to_ascii_lowercase() {
                'a' => {
                    self.anchor = 0;
                    self.cursor = self.text.len();
                    context.invalidate_measure();
                }
                'c' if !selected.is_empty() => context.clipboard().set_text(&selected),
                'x' if !selected.is_empty() && editable => {
                    context.clipboard().set_text(&selected);
                    self.replace_selection("", context);
                }
                'v' if editable => {
                    if let Some(pasted) = context.clipboard().text() {
                        self.insert(&pasted, context);
                    }
                }
                _ => return false,
            }
            return true;
        }
        match event.key {
            // In a multi-line field, Tab types a tab, as Compose's.
            Key::Tab if editable && !self.single_line => {
                self.insert("\t", context);
                true
            }
            Key::Backspace | Key::Delete if editable && !self.selection().is_empty() => {
                self.replace_selection("", context);
                true
            }
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
            // Without Shift, an arrow collapses a selection to its side.
            Key::DirectionLeft if !extend && !self.selection().is_empty() => {
                let start = self.selection().start;
                self.move_cursor(start, false, context);
                true
            }
            Key::DirectionRight if !extend && !self.selection().is_empty() => {
                let end = self.selection().end;
                self.move_cursor(end, false, context);
                true
            }
            Key::DirectionLeft => {
                let cursor = self.previous_boundary();
                self.move_cursor(cursor, extend, context);
                true
            }
            Key::DirectionRight => {
                let cursor = self.next_boundary();
                self.move_cursor(cursor, extend, context);
                true
            }
            // The start and end of the visual (wrapped) line, from the measurer.
            Key::MoveHome | Key::MoveEnd => {
                let end = event.key == Key::MoveEnd;
                let offset = match (self.paragraph, self.cursor_rect) {
                    (Some(paragraph), Some(rect)) => {
                        let y = (rect.top + rect.bottom) / 2.0;
                        let x = if end { f32::MAX / 4.0 } else { -1.0 };
                        let found =
                            context.text_measurer().hit_test(paragraph, Offset::new(x, y)).offset;
                        let found = floor_char_boundary(&self.text, found);
                        // At a soft wrap the line ends before the space that breaks it.
                        if end && found < self.text.len() && self.text[..found].ends_with(' ') {
                            found - 1
                        } else {
                            found
                        }
                    }
                    _ if end => self.text[self.cursor..]
                        .find('\n')
                        .map_or(self.text.len(), |index| self.cursor + index),
                    _ => self.text[..self.cursor].rfind('\n').map_or(0, |index| index + 1),
                };
                self.move_cursor(offset, extend, context);
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
        // The field's node is its decoration's, as Compose's `CoreTextField`.
        config.part_of_parent = true;
        config.editable_text = Some(self.text.clone().into());
        config.editable = true;
        config.focusable |= self.enabled;
        config.focused |= self.focused;
        config.disabled |= !self.enabled;
    }
}

// ---- The decoration's taps ----------------------------------------------------------------

/// Focuses the focus target below it on a tap (a release after a press, inside).
struct FocusOnTap;

#[derive(Default)]
struct FocusOnTapNode {
    pressed: bool,
}

impl ModifierNodeElement for FocusOnTap {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(FocusOnTapNode::default())
    }
    fn update(&self, _: &mut dyn ModifierNode) {}
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<FocusOnTap>().is_some()
    }
    fn name(&self) -> &'static str {
        "focus_on_tap"
    }
}

impl ModifierNode for FocusOnTapNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn compose_ui::PointerInputModifierNode> {
        Some(self)
    }
}

impl compose_ui::PointerInputModifierNode for FocusOnTapNode {
    fn on_pointer_event(
        &mut self,
        event: &mut compose_ui::PointerEvent,
        pass: compose_ui::PointerEventPass,
        bounds: compose_ui::IntSize,
        context: &mut compose_ui::EventContext<'_>,
    ) {
        use compose_ui::PointerEventType;
        // After the field's own handling (the Main pass goes up from the target): a tap the
        // field took is consumed.
        if pass != compose_ui::PointerEventPass::Main {
            return;
        }
        match event.kind {
            PointerEventType::Press if !event.consumed => self.pressed = true,
            PointerEventType::Release if std::mem::take(&mut self.pressed) && !event.consumed => {
                let inside = event.position.x >= 0.0
                    && event.position.y >= 0.0
                    && event.position.x < bounds.width as f32
                    && event.position.y < bounds.height as f32;
                if inside {
                    event.consume();
                    context.request_focus_at_pointer();
                }
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.pressed = false;
    }
}
