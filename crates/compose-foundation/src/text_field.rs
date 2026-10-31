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
    AnyModifierNodeElement, Color, CommitTextCommand, Constraints, DeleteSurroundingTextCommand,
    DrawModifierNode, DrawScope, EditCommand, EditProcessor, EventContext,
    FinishComposingTextCommand, FocusTargetModifierNode, IntSize, IntrinsicMeasureScope, Key,
    KeyEvent, KeyEventType, KeyInputModifierNode, Layout, LayoutModifierNode, Measurable,
    MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Offset, Paint,
    PointerEvent, PointerEventPass, PointerEventType, PointerInputModifierNode, Rect,
    ReleaseContext, SemanticsConfiguration, SemanticsModifierNode, SetComposingTextCommand,
    SetSelectionCommand, TextFieldValue, TextRange, TextStyle, empty_measure_policy,
    platform::{ParagraphHandle, TextLayoutOptions},
};
use unicode_segmentation::UnicodeSegmentation;

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
    #[default(None)] interaction_source: Option<compose_ui::MutableInteractionSource>,
    #[default(crate::inner_text_field_only)] decoration_box: impl Fn(Callback) + 'static,
) {
    if single_line {
        crate::text::validate_min_max_lines(1, 1);
    } else {
        crate::text::validate_min_max_lines(min_lines, max_lines);
    }
    let interaction_source_for_press = if enabled { interaction_source.clone() } else { None };
    let element: Rc<dyn AnyModifierNodeElement> = Rc::new(TextFieldElement {
        value,
        on_value_change,
        enabled,
        read_only,
        style: text_style,
        single_line,
        max_lines,
        min_lines,
        cursor_color,
        // The highlight of a selection in it, as Compose's `CoreTextField` reads
        // `LocalTextSelectionColors`.
        selection_color: crate::LOCAL_TEXT_SELECTION_COLORS.current().background_color,
        keyboard_options,
        keyboard_actions,
        visual_transformation,
        interaction_source,
    });
    let press_source = interaction_source_for_press;
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
            .semantics_with(true, |_| {})
            .then_custom(FocusOnTap { interaction_source: press_source })
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
    pub selection_color: Color,
    pub keyboard_options: KeyboardOptions,
    pub keyboard_actions: KeyboardActions,
    pub visual_transformation: VisualTransformation,
    /// Told when the field gains and loses the focus, as `interactionSource`.
    pub interaction_source: Option<compose_ui::MutableInteractionSource>,
}

impl ModifierNodeElement for TextFieldElement {
    type Node = TextFieldNode;
    fn create(&self) -> TextFieldNode {
        TextFieldNode {
            window: None,
            pin: Default::default(),
            is_focus_target: self.enabled,
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
            style: self.style.clone(),
            single_line: self.single_line,
            max_lines: self.max_lines,
            min_lines: self.min_lines,
            line_bounds: None,
            cursor_color: self.cursor_color,
            selection_color: self.selection_color,
            focused: false,
            press: None,
            last_tap: None,
            paragraph: None,
            constraints: None,
            stale: Vec::new(),
            intrinsics: None,
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
            preedit: false,
            preedit_interrupted: false,
            composing_over_text: false,
            restart_input: false,
            size: IntSize::new(0, 0),
            composition_rects: Vec::new(),
            keyboard_options: self.keyboard_options,
            keyboard_actions: self.keyboard_actions.clone(),
            visual_transformation: self.visual_transformation.clone(),
            interaction_source: self.interaction_source.clone(),
            focus_interaction: None,
            mapping: None,
            shown_text: None,
            before_edit: None,
        }
    }

    fn update(&self, node: &mut TextFieldNode) {
        if node.value != self.value {
            node.value = self.value.clone();
            node.awaiting_value = false;
            node.before_edit = None;
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
                node.text_replaced();
            }
            if node.style != self.style {
                node.line_bounds = None;
            }
            node.style = self.style.clone();
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
        // No more written to: the input ends with what it composed, as Compose's `endInputSession`.
        if !node.enabled || node.read_only {
            node.end_composition();
        }
        node.cursor_color = self.cursor_color;
        node.selection_color = self.selection_color;
        node.keyboard_options = self.keyboard_options;
        node.keyboard_actions = self.keyboard_actions.clone();
        if node.interaction_source != self.interaction_source {
            // The focus told to the old source ends there, as `focusable`'s `update`.
            node.tell_focus(false);
            node.interaction_source = self.interaction_source.clone();
        }
    }

    fn same(&self, other: &Self) -> bool {
        other.value == self.value
            && other.on_value_change.ptr_eq(&self.on_value_change)
            && other.enabled == self.enabled
            && other.read_only == self.read_only
            && other.style == self.style
            && other.single_line == self.single_line
            && other.max_lines == self.max_lines
            && other.min_lines == self.min_lines
            && other.cursor_color == self.cursor_color
            && other.selection_color == self.selection_color
            && other.keyboard_options == self.keyboard_options
            && other.keyboard_actions.same(&self.keyboard_actions)
            && other.visual_transformation.same(&self.visual_transformation)
            && other.interaction_source == self.interaction_source
    }

    fn name(&self) -> &'static str {
        "TextField"
    }
}

/// The words of `text`, by its word breaks: those with a letter or digit, as `start..end`.
fn words_in(text: &str) -> impl DoubleEndedIterator<Item = std::ops::Range<usize>> {
    let breaks = compose_ui::platform::word_breaks(text);
    let mut words = Vec::with_capacity(breaks.len());
    for pair in breaks.windows(2) {
        if text[pair[0]..pair[1]].chars().any(char::is_alphanumeric) {
            words.push(pair[0]..pair[1]);
        }
    }
    words.into_iter()
}

/// The start of the word before `offset` in `text`.
fn previous_word_in(text: &str, offset: usize) -> usize {
    words_in(text).rev().find(|word| word.start < offset).map_or(0, |word| word.start)
}

/// The end of the word after `offset` in `text`.
fn next_word_in(text: &str, offset: usize) -> usize {
    words_in(text).find(|word| word.end > offset).map_or(text.len(), |word| word.end)
}

/// The word at `offset` in `text`, as Compose's `getWordBoundary` (D1108).
fn word_at_in(text: &str, offset: usize) -> std::ops::Range<usize> {
    compose_ui::platform::word_boundary(text, offset).range()
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
    /// Its window, read as it's focused: no cursor while another window takes the keys.
    window: Option<compose_ui::WindowInfo>,
    /// The container around it, pinned while it has the focus.
    pin: compose_ui::FocusPin,
    /// Disabled, it isn't a focus target, as `undelegate(textFieldFocusableNode)` (D1274).
    is_focus_target: bool,
    /// The text shown: `value`, with the edits made since the last frame.
    text: String,
    /// The application's value, as composition last gave it.
    value: String,
    /// The selection before the edits `awaiting_value` waits on: an edit undone restores it.
    before_edit: Option<(usize, usize)>,
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
    selection_color: Color,
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
    /// The paragraph its intrinsics read, kept for the next ones with the same text, style,
    /// density, direction and constraints, as a text's (`TextNode::probe`, D1208).
    intrinsics: Option<IntrinsicParagraph>,
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
    /// The range of the text an input method is composing, as Compose's: part of the text and of
    /// the application's value (D1663).
    composition: Option<std::ops::Range<usize>>,
    /// The composition is a platform's preedit (the desktop's input methods, which own the keys
    /// and start over after a click: D124), not the edit commands of one told of the text.
    preedit: bool,
    /// A preedit was ended other than by the platform, which is still composing until it's told
    /// (`take_preedit_interrupted`); the keys stay its own until then (D1668).
    preedit_interrupted: bool,
    /// A composition began on the text as it was (a region): its next change of the text is a step
    /// of undo of its own, what it began on being what an undo should bring back (D1670).
    composing_over_text: bool,
    /// The input method must start over: `take_input_method_restart`.
    restart_input: bool,
    /// Its size at the last measurement.
    size: IntSize,
    /// The composition's underline, from the measurer, refreshed at measurement.
    composition_rects: Vec<Rect>,
    keyboard_options: KeyboardOptions,
    keyboard_actions: KeyboardActions,
    visual_transformation: VisualTransformation,
    interaction_source: Option<compose_ui::MutableInteractionSource>,
    /// The focus told to `interaction_source`, to tell its end.
    focus_interaction: Option<compose_ui::Focus>,
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

    /// Replaces the selection (or inserts at the cursor) with `with`, as Compose's key input
    /// commits typed text (`CommitTextCommand(text, 1)`, a composition finished first).
    fn replace_selection(&mut self, with: &str, typing: bool, context: &mut EventContext<'_>) {
        let commit = CommitTextCommand::new(with, 1);
        self.apply_commands(&[&FinishComposingTextCommand, &commit], typing, context);
    }

    /// Deletes `range` (empty: nothing), leaving the cursor at its start: the text around a
    /// collapsed selection as Compose's key input deletes it (`DeleteSurroundingTextCommand`).
    fn delete(&mut self, range: std::ops::Range<usize>, context: &mut EventContext<'_>) {
        if range.is_empty() {
            return;
        }
        let collapsed = self.anchor == self.cursor;
        if collapsed && range.end == self.cursor {
            let command = DeleteSurroundingTextCommand::new(range.len(), 0);
            self.apply_commands(&[&FinishComposingTextCommand, &command], false, context);
        } else if collapsed && range.start == self.cursor {
            let command = DeleteSurroundingTextCommand::new(0, range.len());
            self.apply_commands(&[&FinishComposingTextCommand, &command], false, context);
        } else {
            let select = SetSelectionCommand::new(range.start as isize, range.end as isize);
            let commit = CommitTextCommand::new("", 0);
            self.apply_commands(&[&FinishComposingTextCommand, &select, &commit], false, context);
        }
    }

    /// Applies `commands` to the text, selection and composition through an `EditProcessor`, as
    /// Compose's text field does, then records the edit: one that leaves the text as it was is
    /// none, for the application or undo.
    fn apply_commands(
        &mut self,
        commands: &[&dyn EditCommand],
        typing: bool,
        context: &mut EventContext<'_>,
    ) {
        let selection = TextRange::new(self.anchor, self.cursor);
        // Away from what's composed, or over more of the text, an edit is a step of its own.
        let within_composition = self.composition.as_ref().is_some_and(|range| {
            range.start <= self.anchor.min(self.cursor) && self.anchor.max(self.cursor) <= range.end
        });
        let composition =
            self.composition.clone().map(|range| TextRange::new(range.start, range.end));
        let mut processor = EditProcessor::from_value(TextFieldValue::new(
            self.text.as_str(),
            selection,
            composition,
        ));
        let value = processor.apply(commands);
        let selection = value.selection();
        let was_composing = self.composition.is_some();
        self.composition = value.composition().map(TextRange::range);
        self.preedit &= self.composition.is_some();
        if value.text() == self.text {
            if !was_composing && self.composition.is_some() {
                self.composing_over_text = true;
                // Nor does typing before it join what's composed over it.
                self.last_typing = None;
            }
            self.composing_over_text &= self.composition.is_some();
            self.cursor = selection.end;
            self.anchor = selection.start;
            context.invalidate_measure();
            return;
        }
        // A composition's steps, shorter ones too, are one step of undo with what began it: no undo
        // shows half of what was composed (Compose's undo, by the clock, can; D1668).
        let over_text = std::mem::take(&mut self.composing_over_text);
        let composing_step = typing && within_composition && !over_text;
        // A deletion or a new line ends a typing step.
        let typing = typing
            && value.text().len() >= self.text.len()
            && value.text().matches('\n').count() <= self.text.matches('\n').count();
        if composing_step {
            self.redo.clear();
            self.last_typing = Some((compose_runtime::platform::now_nanos(), selection.end));
            self.set_text(value.text().to_string(), selection.end, selection.start, context);
            return;
        }
        self.edit(value.text().to_string(), selection.end, selection.start, typing, context);
    }

    /// The text changed other than by the input method's edits or typing: the composition is over
    /// and the input method starts over, as Compose's `updateState`.
    fn text_replaced(&mut self) {
        self.end_composition();
        self.restart_input = true;
    }

    /// Nothing's composed from now on; a platform's preedit is told when it asks.
    fn end_composition(&mut self) {
        self.composition = None;
        self.composing_over_text = false;
        self.preedit_interrupted |= std::mem::take(&mut self.preedit);
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

    /// Replaces the text and moves the cursor and the selection's anchor, then tells the
    /// application.
    fn edit(
        &mut self,
        text: String,
        cursor: usize,
        anchor: usize,
        typing: bool,
        context: &mut EventContext<'_>,
    ) {
        self.record_undo(typing);
        if let Some((at, _)) = self.last_typing {
            self.last_typing = Some((at, cursor));
        }
        self.set_text(text, cursor, anchor, context);
    }

    fn set_text(
        &mut self,
        text: String,
        cursor: usize,
        anchor: usize,
        context: &mut EventContext<'_>,
    ) {
        if !self.awaiting_value {
            self.before_edit = Some((self.anchor, self.cursor));
        }
        self.text = text;
        self.cursor = cursor;
        self.anchor = anchor;
        self.awaiting_value = true;
        context.invalidate_semantics();
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
        self.text_replaced();
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

    /// The transformed text words are found in, when it's shown.
    fn words_shown(&self) -> Option<&str> {
        self.shown_text.as_deref()
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

    /// The offset a page up or down, the field's height away, as `jumpByPagesOffset`: on the line
    /// most of the cursor would be on there.
    fn by_page(&mut self, down: bool, context: &mut EventContext<'_>) -> usize {
        let Some(paragraph) = self.paragraph else {
            return if down { self.text.len() } else { 0 };
        };
        let rect = context.text_measurer().cursor_rect(paragraph, self.shown(self.cursor));
        let height = context.text_measurer().metrics(paragraph).size.height as f32;
        let page = context.measured_size().height as f32;
        let middle = (rect.top + rect.bottom) / 2.0 + if down { page } else { -page };
        // Past the text, its first or last line, where the cursor is across.
        let y = middle.clamp(0.0, (height - 0.5).max(0.0));
        self.offset_at(Offset::new(rect.left, y), context).unwrap_or(self.cursor)
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

    /// What's composed stays as it is, no more a composition, and the input method starts over: what
    /// a service's edit does, as Compose's `FinishComposingTextCommand` first, and its selection, as
    /// Compose's `onValueChange` with no composition.
    fn commit_composition(&mut self, context: &mut EventContext<'_>) {
        if self.composition.is_some() {
            // The input method is told here: nothing's left to tell a platform's preedit.
            self.preedit = false;
            self.end_composition();
            context.reset_input_method();
            context.invalidate_measure();
        }
    }

    /// Ends a platform's preedit where a click or a focus change interrupts it: its text stays,
    /// and the platform's input method starts over (D124). An input method told of the text hears
    /// the new selection and finishes its own composition, as Compose's keeps it.
    fn commit_preedit(&mut self, context: &mut EventContext<'_>) {
        if self.preedit {
            self.preedit = false;
            self.composition = None;
            self.composing_over_text = false;
            context.invalidate_measure();
            context.reset_input_method();
        }
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
    fn on_detach(&mut self, _: &mut compose_ui::NodeContext<'_>) {
        self.tell_focus(false);
    }
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
        if self.is_focus_target { Some(self) } else { None }
    }
    fn focus_target_wanted(&self) -> Option<bool> {
        Some(self.enabled)
    }
    fn set_is_focus_target(&mut self, is_target: bool) {
        self.is_focus_target = is_target;
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, context: &mut ReleaseContext<'_>) {
        let intrinsics = self.intrinsics.take().map(|intrinsics| intrinsics.paragraph);
        for paragraph in
            self.paragraph.take().into_iter().chain(self.stale.drain(..)).chain(intrinsics)
        {
            context.release_paragraph(paragraph);
        }
    }
    /// Reused for another field: its editing (cursor, selection, history, composing text,
    /// gestures, scroll) goes, as a new field's, and the new element's value comes in.
    fn on_reset(&mut self, context: &mut compose_ui::NodeContext<'_>) {
        // As a new field's: the text is the value (an edit not taken yet is dropped).
        if self.text != self.value {
            self.text = self.value.clone();
            self.invalidate_paragraph();
            // Another text to lay out: reuse alone doesn't measure the node again.
            context.invalidate_measurement();
        }
        self.awaiting_value = false;
        self.cursor = 0;
        self.anchor = 0;
        self.focused = false;
        self.press = None;
        self.last_tap = None;
        self.pending_tap = None;
        self.scroll = Offset::ZERO;
        self.preferred_x = None;
        self.undo.clear();
        self.redo.clear();
        self.last_typing = None;
        self.composition = None;
        self.preedit = false;
        self.preedit_interrupted = false;
        self.composing_over_text = false;
        self.restart_input = false;
    }
}

impl TextFieldNode {
    /// The fewest and most pixels high the field's text may be: from `min_lines` and
    /// `max_lines` in a multi-line field, any height in a single line.
    fn line_bounds(&mut self, scope: &mut IntrinsicMeasureScope<'_>) -> (i32, i32) {
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
            let (anchor, cursor) = self.before_edit.unwrap_or((self.anchor, self.cursor));
            self.cursor = floor_char_boundary(&self.text, cursor);
            self.anchor = floor_char_boundary(&self.text, anchor);
            self.text_replaced();
            self.invalidate_paragraph();
        }
        self.before_edit = None;
        for paragraph in self.stale.drain(..) {
            scope.text_measurer().release(paragraph);
        }
        let cursor_width = scope.density().round_to_px(CURSOR_WIDTH);
        let paragraph = {
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
                let text = self.text.clone();
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
            paragraph
        };
        self.cursor_rect =
            Some(scope.text_measurer().cursor_rect(paragraph, self.shown(self.cursor)));
        self.selection_rects.clear();
        self.composition_rects.clear();
        let selection = self.selection();
        if !selection.is_empty() {
            let range = self.shown(selection.start)..self.shown(selection.end);
            self.selection_rects = scope.text_measurer().selection_rects(paragraph, range);
        }
        if let Some(composition) = self.composition.clone() {
            let range = self.shown(composition.start)..self.shown(composition.end);
            self.composition_rects = scope.text_measurer().selection_rects(paragraph, range);
        }
        let metrics = scope.text_measurer().metrics(paragraph);
        // A multi-line field between `min_lines` and `max_lines` lines high, as Compose's
        // `heightInLines`.
        let (min_height, max_height) = self.line_bounds(scope);
        let size = constraints.constrain(IntSize::new(
            metrics.size.width + cursor_width,
            metrics.size.height.max(min_height).min(max_height),
        ));
        self.size = size;
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

    /// The size it measures at an unbounded width, as Compose's text field's default.
    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        height: i32,
    ) -> i32 {
        self.intrinsic_size(scope, Constraints::new(0, Constraints::INFINITY, 0, height)).width
    }

    /// The size it measures at `width`, as Compose's text field's default.
    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        width: i32,
    ) -> i32 {
        self.intrinsic_size(scope, Constraints::new(0, width, 0, Constraints::INFINITY)).height
    }

    /// Its text's maximum intrinsic width, as Compose's text field (`TextDelegate`'s), and
    /// the cursor's width, which `measure` adds beside the text (D1206): the field doesn't wrap
    /// at its maximum intrinsic width.
    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        _height: i32,
    ) -> i32 {
        let cursor_width = scope.density().round_to_px(CURSOR_WIDTH);
        let unbounded = Constraints::unbounded();
        let text = self.with_intrinsic_paragraph(scope, unbounded, |measurer, paragraph| {
            match measurer.max_intrinsic_width(paragraph) {
                Some(width) => width.ceil() as i32,
                None => measurer.metrics(paragraph).size.width,
            }
        });
        text + cursor_width
    }

    /// As `min_intrinsic_height`.
    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        _content: Measurable,
        width: i32,
    ) -> i32 {
        self.intrinsic_size(scope, Constraints::new(0, width, 0, Constraints::INFINITY)).height
    }
}

/// A paragraph a text field's intrinsics laid out, and what for.
struct IntrinsicParagraph {
    paragraph: ParagraphHandle,
    text: String,
    style: TextStyle,
    options: TextLayoutOptions,
    density: compose_ui::Density,
    constraints: Constraints,
}

impl TextFieldNode {
    /// Runs `read` on its shown text laid out for `constraints` (as `measure` lays it out),
    /// in a paragraph of its own, kept for the next intrinsics asked with the same text, style,
    /// density, direction and constraints (D1208): its intrinsics leave what it measured, its
    /// scroll and its cursor as they are.
    fn with_intrinsic_paragraph<T>(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        constraints: Constraints,
        read: impl FnOnce(&mut dyn compose_ui::platform::TextMeasurer, ParagraphHandle) -> T,
    ) -> T {
        let density = scope.density();
        let options = TextLayoutOptions {
            soft_wrap: !self.single_line,
            max_lines: if self.single_line { 1 } else { u32::MAX },
            direction: scope.layout_direction(),
            overflow: compose_ui::TextOverflow::Clip,
        };
        let cursor_width = density.round_to_px(CURSOR_WIDTH);
        let mut text_constraints = constraints.offset(-cursor_width, 0);
        text_constraints.min_height = 0;
        let text = self.text.clone();
        let text = match self.visual_transformation.filter(&text) {
            Some(transformed) => transformed.text,
            None => text,
        };
        let mut measurer = scope.text_measurer();
        let kept = self.intrinsics.as_ref().filter(|kept| {
            kept.text == text
                && kept.style == self.style
                && kept.options == options
                && kept.density == density
                && kept.constraints == text_constraints
        });
        let paragraph = match kept {
            Some(kept) => kept.paragraph,
            None => {
                if let Some(old) = self.intrinsics.take() {
                    measurer.release(old.paragraph);
                }
                let paragraph = measurer.layout_probe(
                    None,
                    &compose_ui::AnnotatedString::from(text.as_str()),
                    &self.style,
                    options,
                    text_constraints,
                    &density,
                );
                self.intrinsics = Some(IntrinsicParagraph {
                    paragraph,
                    text,
                    style: self.style.clone(),
                    options,
                    density,
                    constraints: text_constraints,
                });
                paragraph
            }
        };
        read(&mut *measurer, paragraph)
    }

    /// The size `measure` gives it at `constraints`, as Compose's text field's default
    /// intrinsics measure it.
    fn intrinsic_size(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        constraints: Constraints,
    ) -> IntSize {
        let cursor_width = scope.density().round_to_px(CURSOR_WIDTH);
        let text = self.with_intrinsic_paragraph(scope, constraints, |measurer, paragraph| {
            measurer.metrics(paragraph).size
        });
        let (min_height, max_height) = self.line_bounds(scope);
        constraints.constrain(IntSize::new(
            text.width + cursor_width,
            text.height.max(min_height).min(max_height),
        ))
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
                scope.draw_rect(shift(*rect), Paint::fill(self.selection_color));
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
        if self.focused
            && !self.read_only
            && self.window.as_ref().is_none_or(|window| window.is_window_focused())
        {
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

impl TextFieldNode {
    fn on_pointer_event_here(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        bounds: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if !self.enabled {
            return;
        }
        // Something around took a move of the press (a page scrolling): no tap, as
        // `detectTapAndPress`, the field having left the moves to it (below).
        if pass == PointerEventPass::Final
            && event.kind == PointerEventType::Move
            && event.consumed
            && self.press.as_ref().is_some_and(|press| !press.dragging)
        {
            self.press = None;
        }
        if pass != PointerEventPass::Main {
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
                self.commit_preedit(context);
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
                let touch = event.pointer_type != compose_ui::PointerType::Mouse;
                // A finger's first press doesn't drag: the moves are for what's around, as
                // `touchSelectionFirstPress`, and it's still a tap unless it leaves the field.
                if touch && press.unit == Unit::Character {
                    if !inside {
                        self.press = None;
                    }
                    return;
                }
                // A later press drags by words or lines past the slop, as
                // `touchSelectionSubsequentPress`.
                let slop = if touch { context.view_configuration().touch_slop } else { 1.0 };
                if !press.dragging
                    && (at - press.at).x.abs() < slop
                    && (at - press.at).y.abs() < slop
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
                // `Done`, say), as Compose's `tapToFocus`; focusing it shows it anyway (below).
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
        let selection = (self.anchor, self.cursor);
        self.on_pointer_event_here(event, pass, bounds, context);
        if selection != (self.anchor, self.cursor) {
            context.invalidate_semantics();
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.press = None;
    }
}

impl FocusTargetModifierNode for TextFieldNode {
    fn focusability(&self) -> compose_ui::Focusability {
        if self.enabled {
            compose_ui::Focusability::ALWAYS
        } else {
            compose_ui::Focusability::NEVER
        }
    }

    /// A tap on a decoration around the field (a Material container's padding) puts the cursor
    /// nearest it, and asks for the on-screen keyboard as a tap on the text does (it may have
    /// been focused already, its keyboard hidden).
    fn on_focus_tap(&mut self, position: Offset, context: &mut EventContext<'_>) {
        self.commit_preedit(context);
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

    fn on_focus_changed(
        &mut self,
        _previous: compose_ui::FocusState,
        current: compose_ui::FocusState,
        context: &mut EventContext<'_>,
    ) {
        let focused = current.is_focused();
        self.pin.focus_changed(focused, context);
        if focused {
            self.window = Some(context.current_value_of(&compose_ui::LOCAL_WINDOW_INFO));
        }
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        self.tell_focus(focused);
        if focused {
            context.bring_into_view();
            // As Compose's `StartInput`, which always asks for the keyboard: going from one field
            // to another with it hidden (`Done`, back) shows it at the first tap.
            if self.enabled && !self.read_only {
                context.show_software_keyboard();
            }
        }
        // A new field's text is for the input method to read anew, as Compose's `startInput`.
        self.restart_input |= focused;
        if !focused {
            // As Compose's `onBlur`: a composition in progress is committed, and the
            // selection collapses to its end (`deselect`).
            self.commit_preedit(context);
            self.composition = None;
            self.composing_over_text = false;
            // Interrupted and not yet told: this is the node's last chance, off the focus's path.
            if std::mem::take(&mut self.preedit_interrupted) {
                context.reset_input_method();
            }
            let end = self.selection().end;
            self.cursor = end;
            self.anchor = end;
            self.preferred_x = None;
            context.invalidate_measure();
            context.invalidate_semantics();
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

impl TextFieldNode {
    /// Tells `interaction_source` the focus came or went, as `focusable`'s.
    fn tell_focus(&mut self, focused: bool) {
        use compose_ui::{Focus, FocusInteraction, Interaction};
        let Some(source) = &self.interaction_source else { return };
        if focused {
            let focus = Focus::new();
            self.focus_interaction = Some(focus);
            source.try_emit(Interaction::Focus(FocusInteraction::Focus(focus)));
        } else if let Some(focus) = self.focus_interaction.take() {
            source.try_emit(Interaction::Focus(FocusInteraction::Unfocus(focus)));
        }
    }

    fn on_key_event_here(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
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
        // While a platform's input method composes, keys are its own (D124); Tab still moves focus
        // (which commits the composition). One told of the text sends its own: a key finishes its
        // composition first, as Compose's.
        if self.preedit || self.preedit_interrupted {
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
                    self.restart_input = true;
                }
                'v' if editable => {
                    if let Some(pasted) = context.clipboard().get_text() {
                        self.insert(&pasted, false, context);
                        self.restart_input = true;
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
            // Taken even with nothing to move to, as Compose's: not left to a scroll around.
            Key::PageUp | Key::PageDown => {
                let cursor = self.by_page(key == Key::PageDown, context);
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
}

impl KeyInputModifierNode for TextFieldNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        let selection = (self.anchor, self.cursor);
        let consumed = self.on_key_event_here(event, context);
        if selection != (self.anchor, self.cursor) {
            context.invalidate_semantics();
        }
        consumed
    }

    fn on_text_input(&mut self, text: &str, context: &mut EventContext<'_>) -> bool {
        if !self.enabled || self.read_only {
            return false;
        }
        self.preferred_x = None;
        // What a platform's preedit shows was never typed: what's sent takes its place.
        if self.preedit {
            self.on_preedit("", None, context);
        }
        self.insert(text, true, context);
        true
    }

    /// A window over this field's takes the keys: what's being composed stays in the text, composed
    /// no more, and the input method starts over for whichever field it's given next (D1286).
    fn on_window_lost_keys(&mut self, context: &mut EventContext<'_>) {
        self.commit_preedit(context);
        self.composing_over_text = false;
        if self.composition.take().is_some() {
            context.invalidate_measure();
            context.invalidate_draw();
            context.reset_input_method();
        }
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
        // A platform's cancel with nothing composed, or before each commit: nothing.
        if text.is_empty() && self.composition.is_none() {
            return true;
        }
        let start = self.composition.as_ref().map_or(self.selection().start, |range| range.start);
        let composing = SetComposingTextCommand::new(text, 1);
        match cursor.filter(|_| !text.is_empty()) {
            Some((from, to)) => {
                let select =
                    SetSelectionCommand::new((start + from) as isize, (start + to) as isize);
                self.apply_commands(&[&composing, &select], true, context);
            }
            None => self.apply_commands(&[&composing], true, context),
        }
        self.preedit = self.composition.is_some();
        true
    }

    fn input_method_value(&self) -> Option<TextFieldValue> {
        if !self.enabled || self.read_only {
            return None;
        }
        let composition =
            self.composition.clone().map(|range| TextRange::new(range.start, range.end));
        let selection = TextRange::new(self.anchor, self.cursor);
        Some(TextFieldValue::new(self.text.as_str(), selection, composition))
    }

    fn on_edit_commands(
        &mut self,
        commands: &[&dyn EditCommand],
        context: &mut EventContext<'_>,
    ) -> bool {
        if !self.enabled || self.read_only {
            return false;
        }
        self.preferred_x = None;
        self.preedit = false;
        self.apply_commands(commands, true, context);
        true
    }

    fn input_method_value_is(&self, value: &TextFieldValue) -> Option<bool> {
        if !self.enabled || self.read_only {
            return None;
        }
        let composition =
            self.composition.as_ref().map(|range| TextRange::new(range.start, range.end));
        Some(
            value.text() == self.text
                && value.selection() == TextRange::new(self.anchor, self.cursor)
                && value.composition() == composition,
        )
    }

    fn take_input_method_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart_input)
    }

    fn take_preedit_interrupted(&mut self) -> bool {
        std::mem::take(&mut self.preedit_interrupted)
    }

    fn cursor_anchor_key(&self) -> Option<compose_ui::CursorAnchorKey> {
        let paragraph = self.paragraph?;
        let (composition_start, composition_end) = self
            .composition
            .as_ref()
            .map_or((u64::MAX, u64::MAX), |range| (range.start as u64, range.end as u64));
        Some([
            paragraph.0,
            self.cursor as u64,
            self.anchor as u64,
            composition_start,
            composition_end,
            self.scroll.x.to_bits() as u64,
            self.scroll.y.to_bits() as u64,
            self.size.width as u64,
            self.size.height as u64,
        ])
    }

    fn cursor_anchor(
        &self,
        lines: bool,
        context: &mut EventContext<'_>,
    ) -> Option<compose_ui::CursorAnchor> {
        let paragraph = self.paragraph?;
        let size = (self.size.width as f32, self.size.height as f32);
        let shown =
            Rect::new(self.scroll.x, self.scroll.y, self.scroll.x + size.0, self.scroll.y + size.1);
        let holds = |x: f32, y: f32| {
            x >= shown.left && x <= shown.right && y >= shown.top && y <= shown.bottom
        };
        let measurer = context.text_measurer();
        let right_to_left = measurer.is_right_to_left(paragraph);
        let cursor = measurer.cursor_rect(paragraph, self.shown(self.selection().start));
        let layout_width = measurer.metrics(paragraph).size.width as f32;
        let x = cursor.left.clamp(0.0, layout_width.max(0.0));
        let (top_shows, bottom_shows) = (holds(x, cursor.top), holds(x, cursor.bottom));
        let insertion_marker = compose_ui::CursorAnchorBox {
            rect: Rect::new(x, cursor.top, x, cursor.bottom),
            visible: top_shows || bottom_shows,
            invisible: !top_shows || !bottom_shows,
            right_to_left,
        };
        let mut characters = Vec::new();
        if let Some(composition) = self.composition.clone() {
            for (index, c) in self.text[composition.clone()].char_indices() {
                let offset = composition.start + index;
                let range = self.shown(offset)..self.shown(offset + c.len_utf8());
                let rects = measurer.selection_rects(paragraph, range);
                let rect = rects.first().copied().unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                characters.push((
                    offset,
                    compose_ui::CursorAnchorBox {
                        rect,
                        visible: rect.left < shown.right
                            && rect.right > shown.left
                            && rect.top < shown.bottom
                            && rect.bottom > shown.top,
                        invisible: !holds(rect.left, rect.top) || !holds(rect.right, rect.bottom),
                        right_to_left,
                    },
                ));
            }
        }
        let mut visible_lines = Vec::new();
        if lines && size.0 > 0.0 && size.1 > 0.0 {
            let layout = measurer.paragraph(paragraph);
            let last = layout.line_count().saturating_sub(1);
            let first_line = layout.get_line_for_vertical_position(shown.top).min(last);
            let last_line = layout.get_line_for_vertical_position(shown.bottom).min(last);
            for line in first_line..=last_line {
                visible_lines.push(Rect::new(
                    layout.get_line_left(line),
                    layout.get_line_top(line),
                    layout.get_line_right(line),
                    layout.get_line_bottom(line),
                ));
            }
        }
        Some(compose_ui::CursorAnchor {
            origin: Offset::new(-self.scroll.x, -self.scroll.y),
            insertion_marker,
            characters,
            editor_bounds: shown,
            visible_lines,
        })
    }

    fn ime_options(&self) -> Option<compose_ui::ImeOptions> {
        if !self.enabled || self.read_only {
            return None;
        }
        Some(self.keyboard_options.to_ime_options(self.single_line))
    }

    /// The input method's action key: the field's action for it (whichever the keyboard showed,
    /// as Compose's), committing a platform's preedit first.
    fn on_ime_action(&mut self, action: ImeAction, context: &mut EventContext<'_>) -> bool {
        if !self.enabled {
            return false;
        }
        self.commit_preedit(context);
        // Android's default action key is `Done` for a single line.
        let action = match action {
            ImeAction::Default if self.single_line => ImeAction::Done,
            action => action,
        };
        self.perform_ime_action(action, context)
    }

    /// Only a field that's written to has an input, as Compose's session starting for
    /// `hasFocus && enabled && !readOnly`.
    fn text_input_area(&self) -> Option<Rect> {
        if !self.enabled || self.read_only {
            return None;
        }
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
        config.part_of_parent();
        // What's shown, and where offsets of the text are in it: a password's mask, a
        // transformation's own mapping, as Compose's; a composition is in the text.
        let text = &self.text;
        let (shown, mapping): (compose_ui::SharedString, _) =
            match self.visual_transformation.filter(text) {
                Some(transformed) => (transformed.text.into(), Some(transformed.offset_mapping)),
                None => (text.clone().into(), None),
            };
        let in_shown = |offset: usize| {
            let offset = mapping.as_ref().map_or(offset, |it| it.original_to_transformed(offset));
            floor_char_boundary(shown.as_str(), offset)
        };
        config.set_text_selection_range(compose_ui::TextRange::new(
            in_shown(self.anchor),
            in_shown(self.cursor),
        ));
        config.set_editable_text(shown.clone());
        if self.visual_transformation.is_password() {
            config.password();
        }
        // As Compose's `CoreTextField`: editable, and its text set by accessibility, when
        // enabled and not read-only.
        let editable = self.enabled && !self.read_only;
        config.set_is_editable(editable);
        if editable {
            config.set_text_of_node();
            config.insert_text_at_cursor_of_node();
        }
        if self.enabled {
            config.set_focused(self.focused);
            config.request_focus_of_node();
        } else {
            config.disabled();
        }
        // As Compose's `CoreTextFieldSemanticsModifier`.
        config.get_text_layout_result_of_node();
        config.set_selection_of_node();
        config.on_click_of_node();
        config.on_ime_action_of_node(
            self.keyboard_options.to_ime_options(self.single_line).ime_action,
        );
        if self.anchor != self.cursor && !self.visual_transformation.is_password() {
            config.copy_text_of_node();
            if editable {
                config.cut_text_of_node();
            }
        }
        if editable {
            config.paste_text_of_node();
        }
    }

    fn perform_text_action(
        &mut self,
        action: compose_ui::TextAction,
        context: &mut EventContext<'_>,
    ) -> bool {
        use compose_ui::TextAction as Action;
        let editable = self.enabled && !self.read_only;
        match action {
            Action::Click => {
                context.request_focus();
                if !self.read_only {
                    context.show_software_keyboard();
                }
                true
            }
            Action::SetText(text) => {
                if !editable {
                    return false;
                }
                // Nothing composed is left of what was there, as Compose's `DeleteAllCommand` and
                // commit; if the application refuses the text, `awaiting_value` undoes it.
                self.end_composition();
                context.reset_input_method();
                let end = text.as_str().len();
                self.edit(text.as_str().to_string(), end, end, false, context);
                true
            }
            Action::InsertText(text) => {
                if !editable {
                    return false;
                }
                self.commit_composition(context);
                self.insert(text.as_str(), false, context);
                true
            }
            Action::SetSelection { start, end } => {
                if !self.enabled {
                    return false;
                }
                // In what's shown; what's being composed is part of the text.
                let transformed = self.visual_transformation.filter(&self.text);
                let shown_length =
                    transformed.as_ref().map_or(self.text.len(), |shown| shown.text.len());
                let within = |offset: i32| offset >= 0 && offset as usize <= shown_length;
                if !within(start) || !within(end) {
                    return false;
                }
                let at = |offset: i32| {
                    let offset = offset as usize;
                    let original = transformed
                        .as_ref()
                        .map_or(offset, |it| it.offset_mapping.transformed_to_original(offset));
                    floor_char_boundary(&self.text, original)
                };
                let (anchor, cursor) = (at(start), at(end));
                if (anchor, cursor) == (self.anchor, self.cursor) {
                    return false;
                }
                self.commit_composition(context);
                self.anchor = anchor;
                self.cursor = cursor;
                self.preferred_x = None;
                context.invalidate_measure();
                context.invalidate_semantics();
                true
            }
            Action::CopyText | Action::CutText => {
                let selected = self.selection();
                if selected.is_empty() || self.visual_transformation.is_password() {
                    return false;
                }
                let is_cut = action == Action::CutText;
                if is_cut && !editable {
                    return false;
                }
                let selected = self.text[selected].to_string();
                context.clipboard().set_text(&selected);
                if is_cut {
                    self.replace_selection("", false, context);
                    self.restart_input = true;
                }
                true
            }
            Action::PasteText => {
                if !editable {
                    return false;
                }
                if let Some(pasted) = context.clipboard().get_text() {
                    self.insert(&pasted, false, context);
                    self.restart_input = true;
                }
                true
            }
            Action::ImeAction => {
                let action = self.keyboard_options.to_ime_options(self.single_line).ime_action;
                self.on_ime_action(action, context);
                true
            }
        }
    }

    fn text_layout_offset(&self) -> Offset {
        self.scroll
    }

    fn text_layout_result(
        &mut self,
        context: &mut EventContext<'_>,
    ) -> Option<compose_ui::TextLayoutResult> {
        let (paragraph, constraints) = (self.paragraph?, self.constraints?);
        let text = self.shown_text.clone().unwrap_or_else(|| self.text.clone());
        let input = compose_ui::TextLayoutInput {
            text: text.into(),
            style: self.style.clone(),
            max_lines: if self.single_line { 1 } else { u32::MAX },
            soft_wrap: !self.single_line,
            overflow: compose_ui::TextOverflow::Clip,
            density: self.density?,
            layout_direction: self.direction,
            font_family_resolver: context.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER),
            default_locale_list: context.current_value_of(&compose_ui::LOCAL_LOCALE_LIST),
            constraints,
            placeholders: None,
        };
        let measurer = context.text_measurer();
        let size = constraints.constrain(measurer.metrics(paragraph).size);
        Some(compose_ui::TextLayoutResult::new(input, measurer.paragraph(paragraph), size))
    }
}

// ---- The decoration's taps ----------------------------------------------------------------

/// Focuses the focus target below it on a tap (a release after a press, inside), and tells
/// `interaction_source` of presses anywhere in the field, as `tapPressTextFieldModifier`.
struct FocusOnTap {
    interaction_source: Option<compose_ui::MutableInteractionSource>,
}

#[derive(Default)]
struct FocusOnTapNode {
    pressed: bool,
    interaction_source: Option<compose_ui::MutableInteractionSource>,
    press: Option<compose_ui::Press>,
}

impl FocusOnTapNode {
    /// Ends the press told, if any: released, or cancelled.
    fn end_press(&mut self, released: bool) {
        use compose_ui::PressInteraction;
        if let (Some(press), Some(source)) = (self.press.take(), &self.interaction_source) {
            source.try_emit(if released {
                PressInteraction::Release(press)
            } else {
                PressInteraction::Cancel(press)
            });
        }
    }
}

impl ModifierNodeElement for FocusOnTap {
    type Node = FocusOnTapNode;
    fn create(&self) -> FocusOnTapNode {
        FocusOnTapNode { interaction_source: self.interaction_source.clone(), ..Default::default() }
    }
    fn update(&self, node: &mut Self::Node) {
        if node.interaction_source != self.interaction_source {
            node.end_press(false);
            node.interaction_source = self.interaction_source.clone();
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.interaction_source == other.interaction_source
    }
    fn name(&self) -> &'static str {
        "focus_on_tap"
    }
}

impl ModifierNode for FocusOnTapNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn compose_ui::PointerInputModifierNode> {
        Some(self)
    }

    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.end_press(false);
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
        let is_inside = |event: &compose_ui::PointerEvent| {
            event.position.x >= 0.0
                && event.position.y >= 0.0
                && event.position.x < bounds.width as f32
                && event.position.y < bounds.height as f32
        };
        // Before the field's handling: the field consumes its own taps.
        if pass == compose_ui::PointerEventPass::Initial {
            match event.kind {
                PointerEventType::Press if !event.consumed => {
                    self.end_press(false);
                    if let Some(source) = &self.interaction_source {
                        let press = compose_ui::Press::new(event.position);
                        self.press = Some(press);
                        source.try_emit(compose_ui::PressInteraction::Press(press));
                    }
                }
                PointerEventType::Release => self.end_press(!event.consumed),
                _ => {}
            }
            return;
        }
        // Taken by something around (a page scrolling), or gone off: no press any more.
        if pass == compose_ui::PointerEventPass::Final
            && event.kind == PointerEventType::Move
            && (event.consumed || context.is_out_of_touch_target(event, bounds))
        {
            self.end_press(false);
        }
        // After the field's own handling (the Main pass goes up from the target): a tap the
        // field took is consumed.
        if pass != compose_ui::PointerEventPass::Main {
            return;
        }
        match event.kind {
            PointerEventType::Press if !event.consumed => self.pressed = true,
            PointerEventType::Release
                if std::mem::take(&mut self.pressed) && !event.consumed && is_inside(event) =>
            {
                event.consume();
                context.request_focus_at_pointer();
            }
            _ => {}
        }
    }

    fn on_cancel_pointer_input(&mut self) {
        self.pressed = false;
        self.end_press(false);
    }
}
