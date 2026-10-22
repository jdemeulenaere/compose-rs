//! `BasicTextField`, as Compose's value-and-callback `BasicTextField`: an editable paragraph
//! with a cursor, focused by a tap, edited by keys and committed text.
//!
//! The node keeps its own copy of the text, so edits arriving before the next recomposition
//! build on each other; a new `value` from composition replaces it, and an edit the application
//! didn't take (its `value` unchanged by the next frame: rejected, or filtered back to the old
//! value) is undone, as Compose's value-based text field shows its `value`. The cursor moves by
//! grapheme clusters, and starts at the start.

use std::rc::Rc;

use compose_runtime::{Callback, ComposeArg, composable};

use crate::text_input::{
    ImeAction, KeyboardActions, KeyboardOptions, OffsetMapping, VisualTransformation,
};
use compose_ui::{
    Color, Constraints, DetachContext, DrawModifierNode, DrawScope, EventContext,
    FocusTargetModifierNode, IntSize, Key, KeyEvent, KeyEventType, KeyInputModifierNode, Layout,
    LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, Offset, Paint, PointerEvent, PointerEventPass, PointerEventType,
    PointerInputModifierNode, Rect, SemanticsConfiguration, SemanticsModifierNode, TextStyle,
    empty_measure_policy,
    platform::{ParagraphHandle, TextLayoutOptions},
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
///
/// A multi-line field is between `min_lines` and `max_lines` lines high, as Compose's
/// `heightInLines` (a single line is one line high); its text scrolls within. The keyboard's
/// action key (Enter in a single line, or the input method's) runs `keyboard_actions`' action
/// for `keyboard_options.ime_action`, or by default moves the focus (`Next`, `Previous`) or
/// hides the on-screen keyboard (`Done`).
#[composable]
pub fn BasicTextField(
    value: String,
    on_value_change: impl Fn(String) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(false)] read_only: bool,
    #[default(crate::TextStyle::DEFAULT)] text_style: TextStyle,
    #[default(crate::KeyboardOptions::DEFAULT)] keyboard_options: KeyboardOptions,
    #[default(crate::KeyboardActions::new())] keyboard_actions: KeyboardActions,
    #[default(false)] single_line: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
    #[default(crate::VisualTransformation::NONE)] visual_transformation: impl Into<VisualTransformation>,
    #[default(crate::Color::BLACK)] cursor_color: Color,
    #[default(crate::inner_text_field_only)] decoration_box: impl Fn(Callback) + 'static,
) {
    if single_line {
        crate::text::validate_min_max_lines(1, 1);
    } else {
        crate::text::validate_min_max_lines(min_lines, max_lines);
    }
    let element: Rc<dyn ModifierNodeElement> = Rc::new(TextFieldElement {
        value,
        on_value_change,
        enabled,
        read_only,
        style: text_style,
        single_line,
        max_lines,
        min_lines,
        cursor_color,
        keyboard_options,
        keyboard_actions,
        visual_transformation,
    });
    let inner_text_field = Callback::new(move || {
        Layout(
            compose_ui::platform::shared_custom_modifier(element.clone()),
            empty_measure_policy(),
            compose_runtime::callback(|| {}),
        );
    });
    crate::layout::Box(
        // An I-beam over the field, as Compose Desktop's text fields.
        modifier
            .semantics(true, |_| {})
            .then_custom(FocusOnTap)
            .pointer_hover_icon(compose_ui::PointerIcon::Text),
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

pub(crate) struct TextFieldElement {
    pub value: String,
    pub on_value_change: Callback<dyn Fn(String)>,
    pub enabled: bool,
    pub read_only: bool,
    pub style: TextStyle,
    pub single_line: bool,
    /// The most lines' height a multi-line field takes (its text scrolls past them).
    pub max_lines: u32,
    /// The fewest lines' height a multi-line field takes.
    pub min_lines: u32,
    pub cursor_color: Color,
    pub keyboard_options: KeyboardOptions,
    pub keyboard_actions: KeyboardActions,
    pub visual_transformation: VisualTransformation,
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
            max_lines: self.max_lines,
            min_lines: self.min_lines,
            line_bounds: None,
            cursor_color: self.cursor_color,
            focused: false,
            press: None,
            last_tap: None,
            paragraph: None,
            constraints: None,
            stale: Vec::new(),
            density: None,
            direction: compose_ui::LayoutDirection::Ltr,
            cursor_rect: None,
            pending_tap: None,
            scroll: Offset::ZERO,
            overflows: false,
            preferred_x: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_typing: None,
            composition: None,
            composition_rects: Vec::new(),
            keyboard_options: self.keyboard_options,
            keyboard_actions: self.keyboard_actions.clone(),
            visual_transformation: self.visual_transformation.clone(),
            mapping: None,
            shown_text: None,
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
            || !node.visual_transformation.same(&self.visual_transformation)
        {
            if node.text != self.value {
                // The application changed the text: the history no longer applies to it.
                node.text = self.value.clone();
                node.cursor = floor_char_boundary(&node.text, node.cursor);
                node.anchor = floor_char_boundary(&node.text, node.anchor);
                node.undo.clear();
                node.redo.clear();
                node.last_typing = None;
            }
            if node.style != self.style {
                node.line_bounds = None;
            }
            node.style = self.style;
            node.single_line = self.single_line;
            node.visual_transformation = self.visual_transformation.clone();
            node.invalidate_paragraph();
        }
        if node.max_lines != self.max_lines || node.min_lines != self.min_lines {
            node.max_lines = self.max_lines;
            node.min_lines = self.min_lines;
            node.line_bounds = None;
            node.constraints = None;
        }
        node.on_value_change = self.on_value_change.clone();
        node.enabled = self.enabled;
        node.read_only = self.read_only;
        node.cursor_color = self.cursor_color;
        node.keyboard_options = self.keyboard_options;
        node.keyboard_actions = self.keyboard_actions.clone();
    }

    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<TextFieldElement>().is_some_and(|other| {
            other.value == self.value
                && other.on_value_change.ptr_eq(&self.on_value_change)
                && other.enabled == self.enabled
                && other.read_only == self.read_only
                && other.style == self.style
                && other.single_line == self.single_line
                && other.max_lines == self.max_lines
                && other.min_lines == self.min_lines
                && other.cursor_color == self.cursor_color
                && other.keyboard_options == self.keyboard_options
                && other.keyboard_actions.same(&self.keyboard_actions)
                && other.visual_transformation.same(&self.visual_transformation)
        })
    }

    fn name(&self) -> &'static str {
        "TextField"
    }
}

/// The start of the word before `offset` in `text`.
fn previous_word_in(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .rev()
        .find(|(start, word)| *start < offset && word.chars().any(char::is_alphanumeric))
        .map_or(0, |(start, _)| start)
}

/// The end of the word after `offset` in `text`.
fn next_word_in(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .map(|(start, word)| (start + word.len(), word))
        .find(|(end, word)| *end > offset && word.chars().any(char::is_alphanumeric))
        .map_or(text.len(), |(end, _)| end)
}

/// The word (or the run between words) at `offset` in `text`.
fn word_at_in(text: &str, offset: usize) -> std::ops::Range<usize> {
    text.split_word_bound_indices()
        .map(|(start, word)| start..start + word.len())
        .find(|range| range.contains(&offset) || range.end == offset && offset == text.len())
        .unwrap_or(offset..offset)
}

fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The text, cursor and selection an undo goes back to.
#[derive(Clone)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: usize,
}

/// Typing within this long of the last typed text joins its undo step, as Compose's
/// `UndoManager` (`SNAPSHOTS_INTERVAL_MILLIS`).
const TYPING_STEP_NANOS: u64 = 5_000_000_000;

/// The most undo steps kept, as Compose's `UndoManager`'s default.
const UNDO_LIMIT: usize = 100;

/// A second press within this long of a tap, and this close to it, selects a word, as Compose's
/// double-click.
const DOUBLE_CLICK_NANOS: u64 = 500_000_000;
const DOUBLE_CLICK_DISTANCE: f32 = 8.0;

/// What a press selects, and a drag after it extends by, as Compose's `SelectionAdjustment`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Unit {
    Character,
    /// A double-click: words.
    Word,
    /// A triple-click: lines (between new lines).
    Line,
}

/// A press on the field, while held.
#[derive(Clone)]
struct Press {
    /// Where it went down, in the text's coordinates (scroll included).
    at: Offset,
    /// The pointer moved with the button held: a drag selecting from `at`.
    dragging: bool,
    unit: Unit,
    /// The word or line a double- or triple-click selected.
    initial: std::ops::Range<usize>,
}

pub(crate) struct TextFieldNode {
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
    max_lines: u32,
    min_lines: u32,
    /// The heights of `min_lines` and `max_lines` lines, at the density they were measured at.
    line_bounds: Option<(compose_ui::Density, i32, i32)>,
    cursor_color: Color,
    focused: bool,
    press: Option<Press>,
    /// When and where the last press was, and how many came in a row: a press soon after and
    /// near it is a double- or triple-click.
    last_tap: Option<(u64, Offset, u32)>,
    paragraph: Option<ParagraphHandle>,
    constraints: Option<Constraints>,
    density: Option<compose_ui::Density>,
    /// The layout direction it was laid out in (an empty field's cursor side).
    direction: compose_ui::LayoutDirection,
    stale: Vec<ParagraphHandle>,
    /// Where the cursor is drawn, from the measurer, refreshed at measurement: moving the
    /// cursor asks for one (the paragraph is kept).
    cursor_rect: Option<Rect>,
    /// The selection's highlight, from the measurer, refreshed at measurement.
    selection_rects: Vec<Rect>,
    /// A tap to put the cursor at, resolved at the next measurement by the measurer's hit test.
    pending_tap: Option<Offset>,
    /// How far the text is scrolled to keep the cursor in view: sideways on a single line,
    /// down in a multi-line field, as Compose's `TextFieldScrollerPosition`.
    scroll: Offset,
    /// The paragraph is wider or taller than the field: drawn clipped.
    overflows: bool,
    /// The horizontal position Up and Down keep while moving through lines.
    preferred_x: Option<f32>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// When the last typed text was inserted, and where the cursor was after it: typing on from
    /// there joins its undo step.
    last_typing: Option<(u64, usize)>,
    /// The text an input method is composing, shown in place of the selection until committed,
    /// with its cursor's byte range.
    composition: Option<(String, Option<(usize, usize)>)>,
    /// The composition's underline, from the measurer, refreshed at measurement.
    composition_rects: Vec<Rect>,
    keyboard_options: KeyboardOptions,
    keyboard_actions: KeyboardActions,
    visual_transformation: VisualTransformation,
    /// How the paragraph's offsets map to the text's, when it's transformed.
    mapping: Option<Rc<dyn OffsetMapping>>,
    /// The transformed text shown, when it's transformed.
    shown_text: Option<String>,
}

impl TextFieldNode {
    /// `offset` in the text, in the paragraph shown.
    fn shown(&self, offset: usize) -> usize {
        self.mapping.as_ref().map_or(offset, |mapping| mapping.original_to_transformed(offset))
    }

    /// `offset` in the paragraph shown, in the text.
    fn original_of(&self, offset: usize) -> usize {
        self.mapping.as_ref().map_or(offset, |mapping| mapping.transformed_to_original(offset))
    }

    /// The text laid out and shown: the text, with the composition in place of the selection.
    fn display_text(&self) -> std::borrow::Cow<'_, str> {
        match &self.composition {
            Some((composing, _)) => {
                let range = self.selection();
                let mut text = String::with_capacity(self.text.len() + composing.len());
                text.push_str(&self.text[..range.start]);
                text.push_str(composing);
                text.push_str(&self.text[range.end..]);
                std::borrow::Cow::Owned(text)
            }
            None => std::borrow::Cow::Borrowed(&self.text),
        }
    }

    /// The cursor in the displayed text: in the composition while there is one.
    fn display_cursor(&self) -> usize {
        match &self.composition {
            Some((composing, cursor)) => {
                let inside =
                    cursor.map_or(composing.len(), |(start, _)| start.min(composing.len()));
                self.selection().start + inside
            }
            None => self.cursor,
        }
    }

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
    fn replace_selection(&mut self, with: &str, typing: bool, context: &mut EventContext<'_>) {
        let range = self.selection();
        let mut text = self.text.clone();
        text.replace_range(range.clone(), with);
        self.edit(text, range.start + with.len(), typing, context);
    }

    /// Deletes `range` (empty: nothing), leaving the cursor at its start.
    fn delete(&mut self, range: std::ops::Range<usize>, context: &mut EventContext<'_>) {
        if range.is_empty() {
            return;
        }
        let mut text = self.text.clone();
        text.replace_range(range.clone(), "");
        self.edit(text, range.start, false, context);
    }

    fn invalidate_paragraph(&mut self) {
        if let Some(paragraph) = self.paragraph.take() {
            self.stale.push(paragraph);
        }
        self.constraints = None;
    }

    /// Records the state before an edit for undo: typing on from the last typed text joins its
    /// step.
    fn record_undo(&mut self, typing: bool) {
        let now = compose_runtime::platform::now_nanos();
        let joins = typing
            && self.last_typing.is_some_and(|(at, cursor)| {
                cursor == self.cursor
                    && self.anchor == self.cursor
                    && now.saturating_sub(at) < TYPING_STEP_NANOS
            });
        if !joins {
            self.undo.push(Snapshot {
                text: self.text.clone(),
                cursor: self.cursor,
                anchor: self.anchor,
            });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_typing = None;
        if typing {
            self.last_typing = Some((now, usize::MAX));
        }
    }

    /// Replaces the text and moves the cursor, then tells the application.
    fn edit(&mut self, text: String, cursor: usize, typing: bool, context: &mut EventContext<'_>) {
        self.record_undo(typing);
        if let Some((at, _)) = self.last_typing {
            self.last_typing = Some((at, cursor));
        }
        self.set_text(text, cursor, cursor, context);
    }

    fn set_text(
        &mut self,
        text: String,
        cursor: usize,
        anchor: usize,
        context: &mut EventContext<'_>,
    ) {
        self.text = text;
        self.cursor = cursor;
        self.anchor = anchor;
        self.awaiting_value = true;
        self.invalidate_paragraph();
        context.invalidate_measure();
        let callback = self.on_value_change.clone();
        let value = self.text.clone();
        context.run(move || callback(value));
    }

    /// Goes back to the state before the last edit (`redo`: forward again).
    fn undo(&mut self, redo: bool, context: &mut EventContext<'_>) {
        let (from, to) =
            if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(snapshot) = from.pop() else { return };
        to.push(Snapshot { text: self.text.clone(), cursor: self.cursor, anchor: self.anchor });
        self.last_typing = None;
        self.set_text(snapshot.text, snapshot.cursor, snapshot.anchor, context);
    }

    /// The start of the grapheme cluster before `offset`.
    fn previous_boundary(&self, offset: usize) -> usize {
        self.text[..offset].graphemes(true).next_back().map_or(0, |cluster| offset - cluster.len())
    }

    /// The end of the grapheme cluster after `offset`.
    fn next_boundary(&self, offset: usize) -> usize {
        self.text[offset..].graphemes(true).next().map_or(offset, |cluster| offset + cluster.len())
    }

    /// The start of the word before `offset`: words as shown (a transformed text's), as
    /// Compose finds them in the layout.
    fn previous_word(&self, offset: usize) -> usize {
        self.by_words(offset, previous_word_in)
    }

    /// The end of the word after `offset`.
    fn next_word(&self, offset: usize) -> usize {
        self.by_words(offset, next_word_in)
    }

    /// The word (or the run between words) at `offset`: what a double-click selects.
    fn word_at(&self, offset: usize) -> std::ops::Range<usize> {
        match self.words_shown() {
            Some(shown) => {
                let range = word_at_in(shown, self.shown(offset));
                self.original_of(range.start)..self.original_of(range.end)
            }
            None => word_at_in(&self.text, offset),
        }
    }

    /// The transformed text words are found in, when it's shown (not while composing).
    fn words_shown(&self) -> Option<&str> {
        self.shown_text.as_deref().filter(|_| self.composition.is_none())
    }

    fn by_words(&self, offset: usize, find: impl Fn(&str, usize) -> usize) -> usize {
        match self.words_shown() {
            Some(shown) => self.original_of(find(shown, self.shown(offset))),
            None => find(&self.text, offset),
        }
    }

    /// The offset nearest to `position` (in the text's coordinates), from the measurer.
    fn offset_at(&self, position: Offset, context: &mut EventContext<'_>) -> Option<usize> {
        let paragraph = self.paragraph?;
        let offset = context.text_measurer().hit_test(paragraph, position).offset;
        Some(floor_char_boundary(&self.text, self.original_of(offset)))
    }

    /// The start or end of the visual (wrapped) line the cursor is on, from the measurer.
    fn line_edge(&self, end: bool, context: &mut EventContext<'_>) -> usize {
        if let Some(paragraph) = self.paragraph {
            let rect = context.text_measurer().cursor_rect(paragraph, self.shown(self.cursor));
            let y = (rect.top + rect.bottom) / 2.0;
            let x = if end { f32::MAX / 4.0 } else { -1.0 };
            let found = context.text_measurer().hit_test(paragraph, Offset::new(x, y)).offset;
            let found = floor_char_boundary(&self.text, self.original_of(found));
            // At a soft wrap the line ends before the space that breaks it (not at a new line,
            // where a space ending the line is the line's).
            return if end
                && found < self.text.len()
                && !self.text[found..].starts_with('\n')
                && self.text[..found].ends_with(' ')
            {
                found - 1
            } else {
                found
            };
        }
        if end {
            self.text[self.cursor..].find('\n').map_or(self.text.len(), |index| self.cursor + index)
        } else {
            self.text[..self.cursor].rfind('\n').map_or(0, |index| index + 1)
        }
    }

    /// The offset a line up or down, at the horizontal position Up and Down keep: before the
    /// first line, the start; past the last, the end.
    fn vertical(&mut self, down: bool, context: &mut EventContext<'_>) -> usize {
        let Some(paragraph) = self.paragraph else {
            return if down { self.text.len() } else { 0 };
        };
        let rect = context.text_measurer().cursor_rect(paragraph, self.shown(self.cursor));
        let x = *self.preferred_x.get_or_insert(rect.left);
        let height = context.text_measurer().metrics(paragraph).size.height as f32;
        let y = if down { rect.bottom + 1.0 } else { rect.top - 1.0 };
        if y < 0.0 {
            return 0;
        }
        if y >= height {
            return self.text.len();
        }
        self.offset_at(Offset::new(x, y), context).unwrap_or(self.cursor)
    }

    fn insert(&mut self, inserted: &str, typing: bool, context: &mut EventContext<'_>) {
        let inserted: String = if self.single_line {
            inserted.chars().filter(|c| *c != '\n').collect()
        } else {
            inserted.to_string()
        };
        if inserted.is_empty() {
            return;
        }
        // A new line ends a typing step.
        let typing = typing && !inserted.contains('\n');
        self.replace_selection(&inserted, typing, context);
    }

    /// Ends the composition, if any (committed text follows, or it was cancelled).
    fn end_composition(&mut self) {
        if self.composition.take().is_some() {
            self.invalidate_paragraph();
        }
    }

    /// Commits the composition in progress, if any, as Compose does when a click or a focus
    /// change interrupts it, and has the platform's input method start over.
    fn commit_composition(&mut self, context: &mut EventContext<'_>) {
        let Some((composing, _)) = self.composition.take() else { return };
        self.invalidate_paragraph();
        context.invalidate_measure();
        context.reset_input_method();
        self.insert(&composing, true, context);
    }

    /// The hard line (between new lines) at `offset`: what a triple-click selects.
    fn line_at(&self, offset: usize) -> std::ops::Range<usize> {
        let start = self.text[..offset].rfind('\n').map_or(0, |index| index + 1);
        let end = self.text[offset..].find('\n').map_or(self.text.len(), |index| offset + index);
        start..end
    }

    /// The `unit` (word, line) at `offset`.
    fn unit_at(&self, unit: Unit, offset: usize) -> std::ops::Range<usize> {
        match unit {
            Unit::Character => offset..offset,
            Unit::Word => self.word_at(offset),
            Unit::Line => self.line_at(offset),
        }
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

impl TextFieldNode {
    /// The fewest and most pixels high the field's text may be: from `min_lines` and
    /// `max_lines` in a multi-line field, any height in a single line.
    fn line_bounds(&mut self, scope: &mut MeasureScope<'_>) -> (i32, i32) {
        if self.single_line || (self.min_lines <= 1 && self.max_lines == u32::MAX) {
            return (0, i32::MAX);
        }
        let density = scope.density();
        if let Some((at, min, max)) = self.line_bounds
            && at == density
        {
            return (min, max);
        }
        let min = if self.min_lines > 1 {
            crate::text::height_of_lines(scope, &self.style, self.min_lines)
        } else {
            0
        };
        let max = if self.max_lines == u32::MAX {
            i32::MAX
        } else {
            crate::text::height_of_lines(scope, &self.style, self.max_lines.max(self.min_lines))
        };
        self.line_bounds = Some((density, min, max));
        (min, max)
    }

    /// Performs `action`, as Compose's `onImeActionPerformed`: the field's action for it, or
    /// Compose's default (`Next` and `Previous` move the focus, `Done` hides the on-screen
    /// keyboard). Returns whether something was done.
    fn perform_ime_action(&mut self, action: ImeAction, context: &mut EventContext<'_>) -> bool {
        match (self.keyboard_actions.action(action), action) {
            (Some(callback), _) => context.call(callback),
            (None, ImeAction::Next) => context.move_focus(true),
            (None, ImeAction::Previous) => context.move_focus(false),
            (None, ImeAction::Done) => context.hide_software_keyboard(),
            _ => return false,
        }
        true
    }
}

/// Moves `scroll` (along one axis) so `[start, end)` is inside `[0, visible)`, within
/// `[0, max]`.
fn scroll_into_view(scroll: f32, start: f32, end: f32, visible: f32, max: f32) -> f32 {
    let mut scroll = scroll;
    if end - scroll > visible {
        scroll = end - visible;
    }
    if start < scroll {
        scroll = start;
    }
    scroll.clamp(0.0, max.max(0.0))
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
        // A tap before this measurement is placed in the text without a composition (offsets
        // in it are the text's); the text is laid out again with the composition at the tap.
        let mut bare = self.pending_tap.is_some() && self.composition.is_some();
        if bare {
            self.invalidate_paragraph();
            for paragraph in self.stale.drain(..) {
                scope.text_measurer().release(paragraph);
            }
        }
        let paragraph = loop {
            if self.constraints != Some(constraints)
                || self.paragraph.is_none()
                || self.density != Some(scope.density())
                || self.direction != scope.layout_direction()
            {
                self.density = Some(scope.density());
                self.direction = scope.layout_direction();
                if let Some(old) = self.paragraph.take() {
                    scope.text_measurer().release(old);
                }
                let density = scope.density();
                let options = TextLayoutOptions {
                    soft_wrap: !self.single_line,
                    max_lines: if self.single_line { 1 } else { u32::MAX },
                    direction: scope.layout_direction(),
                    overflow: compose_ui::TextOverflow::Clip,
                };
                // The field's minimum width too, as Compose's text delegate: right-to-left text
                // starts at the field's right edge.
                let mut text_constraints = constraints.offset(-cursor_width, 0);
                text_constraints.min_height = 0;
                let text = if bare { self.text.clone() } else { self.display_text().into_owned() };
                // Shown transformed (a password's mask), its offsets mapped.
                let text = match self.visual_transformation.filter(&text) {
                    Some(transformed) => {
                        self.mapping = Some(transformed.offset_mapping);
                        self.shown_text = Some(transformed.text.clone());
                        transformed.text
                    }
                    None => {
                        self.mapping = None;
                        self.shown_text = None;
                        text
                    }
                };
                let paragraph = scope.text_measurer().layout(
                    &text,
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
                let offset = scope.text_measurer().hit_test(paragraph, tap + self.scroll).offset;
                self.cursor = floor_char_boundary(&self.text, self.original_of(offset));
                self.anchor = self.cursor;
            }
            if bare {
                bare = false;
                if let Some(old) = self.paragraph.take() {
                    scope.text_measurer().release(old);
                }
                continue;
            }
            break paragraph;
        };
        self.cursor_rect =
            Some(scope.text_measurer().cursor_rect(paragraph, self.shown(self.display_cursor())));
        self.selection_rects.clear();
        self.composition_rects.clear();
        match &self.composition {
            Some((composing, _)) => {
                let start = self.selection().start;
                if !composing.is_empty() {
                    let range = self.shown(start)..self.shown(start + composing.len());
                    self.composition_rects =
                        scope.text_measurer().selection_rects(paragraph, range);
                }
            }
            None => {
                let selection = self.selection();
                if !selection.is_empty() {
                    let range = self.shown(selection.start)..self.shown(selection.end);
                    self.selection_rects = scope.text_measurer().selection_rects(paragraph, range);
                }
            }
        }
        let metrics = scope.text_measurer().metrics(paragraph);
        // A multi-line field between `min_lines` and `max_lines` lines high, as Compose's
        // `heightInLines`.
        let (min_height, max_height) = self.line_bounds(scope);
        let size = constraints.constrain(IntSize::new(
            metrics.size.width + cursor_width,
            metrics.size.height.max(min_height).min(max_height),
        ));
        self.overflows =
            metrics.size.width + cursor_width > size.width || metrics.size.height > size.height;
        // The text scrolls to keep the cursor in view, as Compose's: sideways on a single line,
        // down in a multi-line field.
        let rect = self.cursor_rect.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        if self.single_line {
            let visible = (size.width - cursor_width).max(0) as f32;
            let max = metrics.size.width as f32 - visible;
            self.scroll = Offset::new(
                scroll_into_view(self.scroll.x, rect.left, rect.left, visible, max),
                0.0,
            );
        } else {
            let visible = size.height as f32;
            let max = metrics.size.height as f32 - visible;
            self.scroll = Offset::new(
                0.0,
                scroll_into_view(self.scroll.y, rect.top, rect.bottom, visible, max),
            );
        }
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        scope.layout(size.width, size.height)
    }
}

impl DrawModifierNode for TextFieldNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let Some(paragraph) = self.paragraph else { return };
        let scroll = self.scroll;
        let clipped = self.overflows || scroll != Offset::ZERO;
        if clipped {
            scope.save();
            scope.clip_outline(compose_ui::Outline::Rectangle(scope.bounds()));
        }
        let shift = |rect: Rect| {
            Rect::new(
                rect.left - scroll.x,
                rect.top - scroll.y,
                rect.right - scroll.x,
                rect.bottom - scroll.y,
            )
        };
        if self.focused {
            for rect in &self.selection_rects {
                scope.draw_rect(shift(*rect), Paint::fill(SELECTION_COLOR));
            }
        }
        scope.draw_paragraph(paragraph, Offset::new(-scroll.x, -scroll.y));
        // The composition, underlined.
        let underline = scope.density().to_px(compose_ui::Dp(1.0));
        let color = self.style.color.or(Color::BLACK);
        for rect in &self.composition_rects {
            let rect = shift(*rect);
            scope.draw_rect(
                Rect::new(rect.left, rect.bottom - underline, rect.right, rect.bottom),
                Paint::fill(color),
            );
        }
        if self.focused && !self.read_only {
            // The cursor: drawn where the measurer that laid the paragraph out puts it.
            let width = scope.density().to_px(CURSOR_WIDTH);
            let rect = shift(self.cursor_rect.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)));
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
    /// A tap focuses the field and puts the cursor where it landed; a drag selects from the
    /// press to the pointer; a double-click selects a word.
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
        let at = event.position + self.scroll;
        match event.kind {
            PointerEventType::Press if !event.consumed => {
                event.consume();
                // A click ends a composition, which commits.
                self.commit_composition(context);
                let now = compose_runtime::platform::now_nanos();
                let clicks = match self.last_tap {
                    Some((time, tapped, clicks))
                        if now.saturating_sub(time) < DOUBLE_CLICK_NANOS
                            && (tapped - event.position).x.abs() <= DOUBLE_CLICK_DISTANCE
                            && (tapped - event.position).y.abs() <= DOUBLE_CLICK_DISTANCE =>
                    {
                        clicks % 3 + 1
                    }
                    _ => 1,
                };
                self.last_tap = Some((now, event.position, clicks));
                let unit = match clicks {
                    2 => Unit::Word,
                    3 => Unit::Line,
                    _ => Unit::Character,
                };
                let mut initial = 0..0;
                if unit != Unit::Character
                    && let Some(offset) = self.offset_at(at, context)
                {
                    initial = self.unit_at(unit, offset);
                    self.anchor = initial.start;
                    self.cursor = initial.end;
                    context.invalidate_measure();
                }
                self.press = Some(Press { at, dragging: false, unit, initial });
            }
            PointerEventType::Move if event.pressed => {
                let Some(press) = &mut self.press else { return };
                if !press.dragging && (at - press.at).x.abs() < 1.0 && (at - press.at).y.abs() < 1.0
                {
                    return;
                }
                press.dragging = true;
                let press = press.clone();
                event.consume();
                if !self.focused {
                    context.request_focus();
                }
                let Some(offset) = self.offset_at(at, context) else { return };
                match press.unit {
                    Unit::Character => {
                        if let Some(anchor) = self.offset_at(press.at, context) {
                            self.anchor = anchor;
                        }
                        self.cursor = offset;
                    }
                    // By words or lines from the ones the click selected, as Compose's.
                    unit => {
                        let reached = self.unit_at(unit, offset);
                        if offset >= press.initial.start {
                            self.anchor = press.initial.start;
                            self.cursor = reached.end.max(press.initial.end);
                        } else {
                            self.anchor = press.initial.end;
                            self.cursor = reached.start;
                        }
                    }
                }
                self.preferred_x = None;
                context.invalidate_measure();
            }
            PointerEventType::Release => {
                let Some(press) = self.press.take() else { return };
                if event.consumed || (!inside && !press.dragging) {
                    return;
                }
                event.consume();
                // A tap on the field already focused asks for the on-screen keyboard (hidden by
                // `Done`, say), as Compose's `tapToFocus`; focusing it shows it anyway.
                if self.focused && !press.dragging && !self.read_only {
                    context.show_software_keyboard();
                }
                context.request_focus();
                self.preferred_x = None;
                if press.dragging || press.unit != Unit::Character {
                    return;
                }
                // The cursor where the tap landed, now, so text typed before the next frame
                // goes there.
                match self.offset_at(at, context) {
                    Some(offset) => {
                        self.cursor = offset;
                        self.anchor = offset;
                    }
                    None => self.pending_tap = Some(event.position),
                }
                context.invalidate_measure();
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.press = None;
    }
}

impl FocusTargetModifierNode for TextFieldNode {
    fn can_focus(&self) -> bool {
        self.enabled
    }

    /// A tap on a decoration around the field (a Material container's padding) puts the cursor
    /// nearest it, and asks for the on-screen keyboard as a tap on the text does (it may have
    /// been focused already, its keyboard hidden).
    fn on_focus_tap(&mut self, position: Offset, context: &mut EventContext<'_>) {
        self.commit_composition(context);
        if !self.read_only {
            context.show_software_keyboard();
        }
        match self.offset_at(position + self.scroll, context) {
            Some(offset) => {
                self.cursor = offset;
                self.anchor = offset;
            }
            // Laid out again at the next measurement (an edit before the tap): resolved then.
            None => self.pending_tap = Some(position),
        }
        context.invalidate_measure();
    }

    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        self.focused = focused;
        if !focused {
            // As Compose's `onBlur`: a composition in progress is committed, and the
            // selection collapses to its end (`deselect`).
            self.commit_composition(context);
            let end = self.selection().end;
            self.cursor = end;
            self.anchor = end;
            self.preferred_x = None;
            context.invalidate_measure();
        }
        context.invalidate_draw();
    }
}

fn key_is_not_tab(key: Key) -> bool {
    key != Key::Tab
}

/// Cmd on macOS, Ctrl elsewhere: shortcuts and their key mappings, as Compose Desktop's.
fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

impl KeyInputModifierNode for TextFieldNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        let editable = !self.read_only;
        let mut modifiers = event.modifiers;
        // Shortcuts: Cmd on macOS, Ctrl elsewhere. On macOS, Ctrl moves and deletes as Cocoa's
        // Emacs-style bindings, as Compose Desktop's key mapping.
        let shortcut = if is_macos() { modifiers.meta } else { modifiers.ctrl };
        let emacs = is_macos() && modifiers.ctrl && !modifiers.meta;
        // Typed keys: the text arrives as committed text (`on_text_input`); the field takes the
        // key, down and up, so its ancestors' shortcuts and clicks don't.
        if editable && !shortcut && !emacs && matches!(event.key, Key::Character(_) | Key::Space) {
            return true;
        }
        if event.kind != KeyEventType::KeyDown {
            return false;
        }
        // While an input method composes, keys are its own; Tab still moves focus (which
        // commits the composition).
        if self.composition.is_some() {
            return key_is_not_tab(event.key);
        }
        let mut key = event.key;
        if emacs && let Key::Character(c) = key {
            let mapped = match c.to_ascii_lowercase() {
                'a' => Key::MoveHome,
                'e' => Key::MoveEnd,
                'b' => Key::DirectionLeft,
                'f' => Key::DirectionRight,
                'p' => Key::DirectionUp,
                'n' => Key::DirectionDown,
                'd' => Key::Delete,
                'h' => Key::Backspace,
                'k' if editable => {
                    let end = self.line_edge(true, context);
                    // At a line's end, the new line goes.
                    let end = if end == self.cursor { self.next_boundary(end) } else { end };
                    self.delete(self.cursor..end.max(self.cursor), context);
                    return true;
                }
                _ => return false,
            };
            key = mapped;
            modifiers.ctrl = false;
        }
        let extend = modifiers.shift;
        // Word motion: Option on macOS, Ctrl elsewhere. Line motion: Cmd on macOS.
        let by_word = if is_macos() { modifiers.alt } else { modifiers.ctrl };
        let by_line = is_macos() && modifiers.meta;
        if !matches!(key, Key::DirectionUp | Key::DirectionDown) {
            self.preferred_x = None;
        }
        if shortcut && let Key::Character(c) = key {
            let selected = self.text[self.selection()].to_string();
            match c.to_ascii_lowercase() {
                'a' => {
                    self.anchor = 0;
                    self.cursor = self.text.len();
                    context.invalidate_measure();
                }
                // A password can't be copied nor cut, as Compose's.
                'c' if self.visual_transformation.is_password() => {}
                'x' if self.visual_transformation.is_password() => {}
                'c' if !selected.is_empty() => context.clipboard().set_text(&selected),
                'x' if !selected.is_empty() && editable => {
                    context.clipboard().set_text(&selected);
                    self.replace_selection("", false, context);
                }
                'v' if editable => {
                    if let Some(pasted) = context.clipboard().text() {
                        self.insert(&pasted, false, context);
                    }
                }
                'z' if editable => self.undo(modifiers.shift, context),
                'y' if editable && !is_macos() => self.undo(true, context),
                _ => return false,
            }
            return true;
        }
        let right_to_left = self
            .paragraph
            .is_some_and(|paragraph| context.text_measurer().is_right_to_left(paragraph));
        match key {
            // In a multi-line field, a plain Tab types a tab, as Compose's; Shift+Tab moves focus.
            Key::Tab if editable && !self.single_line && !extend && !shortcut && !modifiers.alt => {
                self.insert("\t", false, context);
                true
            }
            Key::Backspace | Key::Delete if editable && !self.selection().is_empty() => {
                self.replace_selection("", false, context);
                true
            }
            Key::Backspace if editable => {
                let start = if by_line {
                    self.line_edge(false, context)
                } else if by_word {
                    self.previous_word(self.cursor)
                } else {
                    self.previous_boundary(self.cursor)
                };
                self.delete(start.min(self.cursor)..self.cursor, context);
                true
            }
            Key::Delete if editable => {
                let end = if by_word {
                    self.next_word(self.cursor)
                } else {
                    self.next_boundary(self.cursor)
                };
                self.delete(self.cursor..end.max(self.cursor), context);
                true
            }
            Key::DirectionLeft | Key::DirectionRight => {
                // Left is backward in left-to-right text, forward in right-to-left text.
                let backward = (key == Key::DirectionLeft) != right_to_left;
                let selection = self.selection();
                let cursor = if by_line {
                    self.line_edge(!backward, context)
                } else if by_word {
                    if backward {
                        self.previous_word(self.cursor)
                    } else {
                        self.next_word(self.cursor)
                    }
                } else if !extend && !selection.is_empty() {
                    // Without Shift, an arrow collapses a selection to its side.
                    if backward { selection.start } else { selection.end }
                } else if backward {
                    self.previous_boundary(self.cursor)
                } else {
                    self.next_boundary(self.cursor)
                };
                self.move_cursor(cursor, extend, context);
                true
            }
            Key::DirectionUp | Key::DirectionDown => {
                let down = key == Key::DirectionDown;
                let cursor = if by_line {
                    if down { self.text.len() } else { 0 }
                } else {
                    self.vertical(down, context)
                };
                self.move_cursor(cursor, extend, context);
                true
            }
            // The start and end of the visual (wrapped) line, or of the text with Ctrl.
            Key::MoveHome | Key::MoveEnd => {
                let end = key == Key::MoveEnd;
                let cursor = if modifiers.ctrl {
                    if end { self.text.len() } else { 0 }
                } else {
                    self.line_edge(end, context)
                };
                self.move_cursor(cursor, extend, context);
                true
            }
            Key::Enter | Key::NumPadEnter if editable && !self.single_line => {
                self.insert("\n", false, context);
                true
            }
            // A single line's Enter performs its action, as Compose's `onImeActionPerformed`.
            Key::Enter | Key::NumPadEnter if self.single_line => {
                self.perform_ime_action(self.keyboard_options.ime_action, context)
            }
            _ => false,
        }
    }

    fn on_text_input(&mut self, text: &str, context: &mut EventContext<'_>) -> bool {
        if !self.enabled || self.read_only {
            return false;
        }
        self.end_composition();
        self.preferred_x = None;
        self.insert(text, true, context);
        true
    }

    fn on_preedit(
        &mut self,
        text: &str,
        cursor: Option<(usize, usize)>,
        context: &mut EventContext<'_>,
    ) -> bool {
        if !self.enabled || self.read_only {
            return false;
        }
        let composition = (!text.is_empty()).then(|| (text.to_string(), cursor));
        if composition != self.composition {
            self.composition = composition;
            self.invalidate_paragraph();
            context.invalidate_measure();
        }
        true
    }

    fn ime_options(&self) -> Option<compose_ui::ImeOptions> {
        Some(self.keyboard_options.to_ime_options(self.single_line))
    }

    /// The input method's action key: the field's action for it (whichever the keyboard showed,
    /// as Compose's), committing a composition first.
    fn on_ime_action(&mut self, action: ImeAction, context: &mut EventContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        self.commit_composition(context);
        // Android's default action key is `Done` for a single line.
        let action = match action {
            ImeAction::Default if self.single_line => ImeAction::Done,
            action => action,
        };
        self.perform_ime_action(action, context)
    }

    fn text_input_area(&self) -> Option<Rect> {
        let rect = self.cursor_rect?;
        let scroll = self.scroll;
        Some(Rect::new(
            rect.left - scroll.x,
            rect.top - scroll.y,
            rect.right - scroll.x,
            rect.bottom - scroll.y,
        ))
    }
}

impl SemanticsModifierNode for TextFieldNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        // The field's node is its decoration's, as Compose's `CoreTextField`.
        config.part_of_parent = true;
        // What's shown: a password's mask, as Compose's.
        let text = self.display_text();
        config.editable_text = Some(match self.visual_transformation.filter(&text) {
            Some(transformed) => transformed.text.into(),
            None => text.into_owned().into(),
        });
        config.password |= self.visual_transformation.is_password();
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
