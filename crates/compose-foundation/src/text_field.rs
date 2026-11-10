//! `BasicTextField`, as Compose's value-and-callback `BasicTextField`: an editable paragraph
//! with a cursor, focused by a tap, edited by keys and committed text.
//!
//! The node keeps its own copy of the text, so edits arriving before the next recomposition
//! build on each other; a new `value` from composition replaces it, and an edit the application
//! didn't take (its `value` unchanged by the next frame: rejected, or filtered back to the old
//! value) is undone, as Compose's value-based text field shows its `value`. The cursor moves by
//! grapheme clusters, and starts at the start.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::snapshots::Snapshot as RuntimeSnapshot;
use compose_runtime::{Callback, ComposeArg, composable};

use crate::key_mapping::{KeyCommand, KeyMapping};
use crate::output_transformation::{
    IndexTransformationType, OffsetMappingCalculator, SelectionWedgeAffinity, ShownToInputMethod,
    WedgeAffinity, index_transformation_type, map_from_transformed, map_to_transformed,
    next_cursor_position, shows_as, transformed_text,
};
use crate::selection_handles::TouchHandles;

mod toolbar;
pub use toolbar::ClipboardPasteState;
mod touch;
use crate::text_field_buffer::TextFieldCharSequence;
use crate::text_undo::TextFieldEditUndoBehavior;
use crate::{
    InputTransformation, KeyboardActionHandler, OutputTransformation, TextFieldLineLimits,
    TextFieldState,
};
pub(crate) use toolbar::FieldToolbar;
use touch::Touch;

use crate::text_input::{
    ImeAction, KeyboardActions, KeyboardOptions, OffsetMapping, VisualTransformation,
};
use compose_ui::{
    AnyModifierNodeElement, Color, CommitTextCommand, Constraints, ContentDataType, ContentType,
    DeleteSurroundingTextCommand, DrawModifierNode, DrawScope, EditCommand, EditProcessor,
    EventContext, FillableData, FinishComposingTextCommand, FocusTargetModifierNode, IntSize,
    IntrinsicMeasureScope, Key, KeyEvent, KeyEventType, KeyInputModifierNode, Layout,
    LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode,
    ModifierNodeElement, Offset, Paint, PointerEvent, PointerEventPass, PointerEventType,
    PointerInputModifierNode, Rect, ReleaseContext, SemanticsConfiguration, SemanticsModifierNode,
    SetComposingTextCommand, SetSelectionCommand, TextFieldValue, TextRange, TextStyle,
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
    // What the node tells its handles, composed in the inner field (D2008).
    let handles = compose_runtime::remember(|| Rc::new(TouchHandles::new()));
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
        handles: Some(handles.clone()),
    });
    let press_source = interaction_source_for_press;
    let inner_text_field = inner_field(element, handles);
    let content_type = keyboard_content_type(keyboard_options);
    crate::layout::Box(
        // An I-beam over the field, as Compose Desktop's text fields.
        modifier
            .semantics_with(true, |config| host_semantics(config, content_type))
            .then_custom(FocusOnTap { interaction_source: press_source })
            .pointer_hover_icon(compose_ui::PointerIcon::TEXT),
        crate::Alignment::TOP_START,
        // The decoration's size is the field's: a field given a size passes it on.
        true,
        compose_runtime::callback(move |_| decoration_box(inner_text_field.clone())),
    );
}

/// The inner field: the field's node, and its handles' popups inside, as Compose composes them in
/// the inner field (a decoration that never calls it shows none, D2008).
fn inner_field(element: Rc<dyn AnyModifierNodeElement>, handles: Rc<TouchHandles>) -> Callback {
    // Its magnifier, on the inner field's coordinates as Compose's is on its core node's: where
    // the platform has one, shown while a drag says (D2073).
    let magnifier = compose_runtime::remember(|| {
        if compose_ui::is_platform_magnifier_supported() != Some(true) {
            return None;
        }
        handles.enable_magnifier();
        let source: Rc<dyn crate::selection_magnifier::SelectionMagnifierSource> = handles.clone();
        Some(crate::selection_magnifier::selection_magnifier(source))
    });
    // The same content while the field is composed: a decoration composed again (a label's
    // animation) doesn't compose the handles again.
    let content = compose_runtime::remember(move || {
        compose_runtime::callback(move || {
            crate::selection_handles::TextFieldHandles(handles.clone())
        })
    });
    Callback::new(move || {
        let modifier = compose_ui::platform::shared_custom_modifier(element.clone());
        let modifier = match &magnifier {
            Some(magnifier) => modifier.then(magnifier.clone()),
            None => modifier,
        };
        Layout(modifier, handles_host_policy(), content.clone());
    })
}

/// The inner field's own layout: as large as its node makes it, its children (the handles'
/// popup hosts, which have no size) placed at its start.
struct HandlesHost;

impl compose_ui::MeasurePolicy for HandlesHost {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        for &child in measurables {
            let placeable = scope.measure(child, constraints);
            scope.place(&placeable, 0, 0);
        }
        scope.layout(constraints.min_width, constraints.min_height)
    }
}

thread_local! {
    static HANDLES_HOST: Rc<dyn compose_ui::MeasurePolicy> = Rc::new(HandlesHost);
}

fn handles_host_policy() -> Rc<dyn compose_ui::MeasurePolicy> {
    HANDLES_HOST.with(Rc::clone)
}

/// The decoration of a field without one: the text alone, as Compose's default `decorationBox`.
pub fn inner_text_field_only(inner_text_field: Callback) {
    inner_text_field();
}

/// An editable text over a [`TextFieldState`], as Compose's `BasicTextField(state, …)`: what's
/// typed, deleted, selected or composed goes into the state at once, and an edit of the state
/// (`state.edit(…)`) shows. Rust has no overloads, so it has a name of its own (D1130).
///
/// `line_limits` makes it a single line, scrolling sideways, or a multi-line field between its
/// fewest and most lines high, scrolling down past them, as Compose's `TextFieldLineLimits`.
/// `cursor_brush` paints the cursor. `decorator` lays out what surrounds the text and calls
/// `inner_text_field` where the text goes, as Compose's `TextFieldDecorator`: the decoration is
/// part of the field, as `BasicTextField`'s `decoration_box` (D102). The keyboard's action key and
/// a single line's Enter do Compose's default actions: `Next` and `Previous` move the focus,
/// `Done` hides the on-screen keyboard.
///
/// `input_transformation` changes or rejects each of the user's edits before it reaches the state,
/// its keyboard options filling what `keyboard_options` doesn't say; `on_keyboard_action` hears the
/// action key with the default action, to call or not (D1133). Undo and redo are the state's
/// history (D1132).
///
/// `output_transformation` changes what's shown of the state, not the state: the cursor never
/// goes inside what it inserted or replaced, and a single line shows a new line as a space
/// (D1134); the input method reads what's shown (D1135). `on_text_layout` hears each new layout of
/// what's shown (D1136). It's at least ten "H" wide (D1137). Given a `scroll_state`, its scroll is
/// that state's, and drags and the wheel scroll it (D1138).
#[composable]
pub fn BasicTextFieldOfState(
    state: TextFieldState,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(false)] read_only: bool,
    #[default(None)] input_transformation: Option<InputTransformation>,
    #[default(crate::TextStyle::DEFAULT)] text_style: TextStyle,
    #[default(crate::KeyboardOptions::DEFAULT)] keyboard_options: KeyboardOptions,
    #[default(None)] on_keyboard_action: Option<KeyboardActionHandler>,
    #[default(crate::TextFieldLineLimits::DEFAULT)] line_limits: TextFieldLineLimits,
    #[default(None)] interaction_source: Option<compose_ui::MutableInteractionSource>,
    #[default(crate::Brush::solid(crate::Color::BLACK))] cursor_brush: compose_ui::Brush,
    #[default(None)] output_transformation: Option<OutputTransformation>,
    #[default(crate::inner_text_field_only)] decorator: impl Fn(Callback) + 'static,
    #[default(None)] on_text_layout: Option<impl Fn(compose_ui::TextLayoutResult) + 'static>,
    #[default(None)] scroll_state: Option<crate::ScrollState>,
    #[default(None)] password: Option<bool>,
) {
    line_limits.validate();
    // Compose's default, `rememberScrollState()` (D2017).
    let remembered_scroll = crate::remember_scroll_state(0);
    let scroll_state = Some(scroll_state.unwrap_or(remembered_scroll));
    // What the field's options don't say, its transformation's say, as Compose's
    // `fillUnspecifiedValuesWith`, read in composition.
    let transformation_options = input_transformation.as_ref().and_then(|it| it.keyboard_options());
    let keyboard_options =
        keyboard_options.fill_unspecified_values_with(transformation_options.as_ref());
    let text_style = phone_direction(text_style, keyboard_options);
    let interaction_source_for_press = if enabled { interaction_source.clone() } else { None };
    // What the node tells its handles, composed in the inner field (D2008).
    let handles = compose_runtime::remember(|| Rc::new(TouchHandles::new()));
    let element: Rc<dyn AnyModifierNodeElement> = Rc::new(StateTextFieldElement {
        state,
        enabled,
        read_only,
        style: text_style,
        line_limits,
        cursor_brush,
        selection_color: crate::LOCAL_TEXT_SELECTION_COLORS.current().background_color,
        keyboard_options,
        interaction_source,
        input_transformation,
        output_transformation,
        on_text_layout: on_text_layout.map(compose_runtime::callback),
        scroll_state,
        password,
        // What its default action uses, read where it's composed, as Compose's node's
        // `currentValueOf`: only for a handler, which calls it outside the event.
        default_action: on_keyboard_action.as_ref().map(|_| {
            (
                compose_ui::LOCAL_FOCUS_MANAGER.current(),
                compose_ui::LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current(),
            )
        }),
        on_keyboard_action,
        handles: Some(handles.clone()),
    });
    let press_source = interaction_source_for_press;
    let inner_text_field = inner_field(element, handles);
    // Dragged and wheeled along its lines, the content following the finger, as Compose's
    // `scrollable(scrollState, …, reverseDirection)` (D1138).
    let modifier = match scroll_state {
        Some(scroll_state) => {
            let horizontal = line_limits == TextFieldLineLimits::SingleLine;
            let orientation = if horizontal {
                crate::Orientation::Horizontal
            } else {
                crate::Orientation::Vertical
            };
            let right_to_left =
                compose_ui::LOCAL_LAYOUT_DIRECTION.current() == compose_ui::LayoutDirection::Rtl;
            let reverse = !(horizontal && right_to_left);
            let source = press_source.clone();
            crate::gestures::field_scrollable(
                modifier,
                scroll_state,
                orientation,
                enabled,
                reverse,
                source,
            )
        }
        None => modifier,
    };
    let content_type = keyboard_content_type(keyboard_options);
    crate::layout::Box(
        modifier
            .semantics_with(true, |config| host_semantics(config, content_type))
            .then_custom(FocusOnTap { interaction_source: press_source })
            .pointer_hover_icon(compose_ui::PointerIcon::TEXT),
        crate::Alignment::TOP_START,
        true,
        compose_runtime::callback(move |_| decorator(inner_text_field.clone())),
    );
}

/// The decorated field's own semantics: its parts' host (D2003), and the content type its
/// keyboard says, as Compose's decorator sets it (the application's, outside, wins).
fn host_semantics(config: &mut SemanticsConfiguration, content_type: Option<ContentType>) {
    config.parts_host();
    if let Some(content_type) = content_type {
        config.set_content_type(content_type);
    }
}

/// What a keyboard type says autofill fills, as Compose's text fields' semantics: an email's
/// address, a password, a phone number (D1684).
fn keyboard_content_type(options: KeyboardOptions) -> Option<ContentType> {
    match options.keyboard_type {
        compose_ui::KeyboardType::Email => Some(ContentType::EMAIL_ADDRESS),
        compose_ui::KeyboardType::Password | compose_ui::KeyboardType::NumberPassword => {
            Some(ContentType::PASSWORD)
        }
        compose_ui::KeyboardType::Phone => Some(ContentType::PHONE_NUMBER),
        _ => None,
    }
}

/// A phone number's direction is its digits', whatever the language's, as Compose's
/// `resolveTextDirectionForKeyboardTypePhone`: left to right, but for the locales whose digits are
/// written right to left (Adlam's, N'Ko's), unless the style says (D1141). The locale is the
/// style's first, else the composition's.
fn phone_direction(style: TextStyle, options: KeyboardOptions) -> TextStyle {
    if options.keyboard_type != crate::KeyboardType::Phone || style.text_direction.is_some() {
        return style;
    }
    let locale = match style.locale_list.as_ref().and_then(|list| list.get(0)) {
        Some(locale) => locale.clone(),
        None => match compose_ui::LOCAL_LOCALE_LIST.current().get(0) {
            Some(locale) => locale.clone(),
            None => return style,
        },
    };
    let right_to_left_digits =
        matches!(locale.script().as_str(), "Adlm" | "Nkoo") || locale.language() == "nqo";
    let direction = if right_to_left_digits {
        compose_ui::TextDirection::Rtl
    } else {
        compose_ui::TextDirection::Ltr
    };
    TextStyle { text_direction: Some(direction), ..style }
}

/// The state-based field's element: the value-based field's node, tied to `state` (D1130).
pub(crate) struct StateTextFieldElement {
    pub state: TextFieldState,
    pub enabled: bool,
    pub read_only: bool,
    pub style: TextStyle,
    pub line_limits: TextFieldLineLimits,
    pub cursor_brush: compose_ui::Brush,
    pub selection_color: Color,
    pub keyboard_options: KeyboardOptions,
    pub interaction_source: Option<compose_ui::MutableInteractionSource>,
    pub input_transformation: Option<InputTransformation>,
    pub output_transformation: Option<OutputTransformation>,
    pub on_text_layout: Option<Callback<dyn Fn(compose_ui::TextLayoutResult)>>,
    pub scroll_state: Option<crate::ScrollState>,
    pub on_keyboard_action: Option<KeyboardActionHandler>,
    pub default_action: Option<DefaultActionTargets>,
    /// A password's (`BasicSecureTextField`): whether it's shown obfuscated (D1142).
    pub password: Option<bool>,
    /// What it tells its handles (D2008).
    pub handles: Option<Rc<TouchHandles>>,
}

/// What a keyboard action handler's default action moves the focus with and hides the keyboard
/// with.
pub(crate) type DefaultActionTargets =
    (compose_ui::FocusManager, Option<compose_ui::SoftwareKeyboardController>);

impl StateTextFieldElement {
    /// What's shown of the state: the output transformation, then a single line's, as Compose's
    /// `SingleLineOutputTransformation.wrap`.
    fn shown_transformation(&self) -> Option<OutputTransformation> {
        match self.line_limits {
            TextFieldLineLimits::SingleLine => {
                Some(OutputTransformation::single_line(self.output_transformation.as_ref()))
            }
            TextFieldLineLimits::MultiLine { .. } => self.output_transformation.clone(),
        }
    }

    /// The cursor's color, and its brush when it isn't one color.
    fn cursor(&self) -> (Color, Option<compose_ui::Brush>) {
        match &self.cursor_brush {
            compose_ui::Brush::Solid(color) => (*color, None),
            brush => (Color::BLACK, Some(brush.clone())),
        }
    }
}

impl ModifierNodeElement for StateTextFieldElement {
    type Node = TextFieldNode;
    fn create(&self) -> TextFieldNode {
        let (single_line, min_lines, max_lines) = self.line_limits.lines();
        let (cursor_color, cursor_brush) = self.cursor();
        let mut node = TextFieldElement {
            value: String::new(),
            on_value_change: Callback::new(|_: String| {}),
            enabled: self.enabled,
            read_only: self.read_only,
            style: self.style.clone(),
            single_line,
            max_lines,
            min_lines,
            cursor_color,
            selection_color: self.selection_color,
            keyboard_options: self.keyboard_options,
            keyboard_actions: KeyboardActions::new(),
            visual_transformation: VisualTransformation::NONE,
            interaction_source: self.interaction_source.clone(),
            handles: self.handles.clone(),
        }
        .create();
        let mut link = StateLink::new(self.state, cursor_brush);
        link.input_transformation = self.input_transformation.clone();
        link.on_keyboard_action = self.on_keyboard_action.clone();
        link.default_action = self.default_action.clone();
        link.shown_transformation = self.shown_transformation();
        link.on_text_layout = self.on_text_layout.clone();
        link.scroll_state = self.scroll_state;
        link.password = self.password;
        node.state = Some(Box::new(link));
        // What the state holds, read without observing it: composition isn't to recompose on it.
        RuntimeSnapshot::without_read_observation(|| node.adopt_state());
        node
    }

    fn update(&self, node: &mut TextFieldNode) {
        let (single_line, min_lines, max_lines) = self.line_limits.lines();
        let (cursor_color, cursor_brush) = self.cursor();
        let link = node.state.as_mut().expect("a state-based field's node");
        if link.state != self.state {
            // Another state: its value is taken at the next measurement, which this update asks.
            link.state.remove_notify_ime_listener(link.listener);
            link.state = self.state;
            link.listener = StateLink::listen(self.state, &link.restart);
            link.seen = None;
            // The input method reads the new state from the start, as Compose's session
            // restarts for a new state (D1141).
            node.restart_input = true;
            // Its gestures start again, as Compose's selection state, made anew for a new state,
            // restarts its pointer input: no multi-tap goes on across them (D2032).
            node.last_tap = None;
        }
        link.cursor_brush = cursor_brush;
        link.input_transformation = self.input_transformation.clone();
        link.on_keyboard_action = self.on_keyboard_action.clone();
        link.default_action = self.default_action.clone();
        link.on_text_layout = self.on_text_layout.clone();
        // A new scroll state keeps its value: what's followed is the field's, as Compose's
        // node keeps its `previousSelection` (D1140).
        link.scroll_state = self.scroll_state;
        link.password = self.password;
        let shown_transformation = self.shown_transformation();
        if link.shown_transformation != shown_transformation {
            link.shown_transformation = shown_transformation;
            node.invalidate_paragraph();
            // The input method reads what's shown: it starts over with the new text, as
            // Compose's session restarts for a new output transformation (D1135).
            node.restart_input = true;
        }
        let same_layout = node.style.has_same_layout_affecting_attributes(&self.style);
        if !same_layout || node.single_line != single_line {
            if !same_layout {
                node.line_bounds = None;
            }
            node.style = self.style.clone();
            node.single_line = single_line;
            node.invalidate_paragraph();
        } else if node.style != self.style {
            // Only its paint changed (a color, a decoration): the layout is kept and drawn again
            // in the new paint, as Compose's `hasSameDrawAffectingAttributes` (D1141).
            node.style = self.style.clone();
            if let Some(link) = node.state.as_mut() {
                link.restyle = true;
            }
        }
        if node.max_lines != max_lines || node.min_lines != min_lines {
            node.max_lines = max_lines;
            node.min_lines = min_lines;
            node.line_bounds = None;
            node.constraints = None;
        }
        node.enabled = self.enabled;
        node.read_only = self.read_only;
        // No more written to: the input ends with what it composed, as Compose's `endInputSession`,
        // and the state hears of it now.
        if !node.enabled || node.read_only {
            node.end_composition();
            node.publish_state(EditSource::InputMethod);
        }
        node.cursor_color = cursor_color;
        node.selection_color = self.selection_color;
        // New options (a new transformation's among them) are told the input method from the
        // start, as Compose's session restarts for them (D1141).
        if node.keyboard_options != self.keyboard_options {
            node.keyboard_options = self.keyboard_options;
            node.restart_input = true;
        }
        if node.interaction_source != self.interaction_source {
            node.tell_focus(false);
            node.interaction_source = self.interaction_source.clone();
        }
        node.touch.update(&self.handles);
    }

    fn same(&self, other: &Self) -> bool {
        same_handles(&other.handles, &self.handles)
            && other.state == self.state
            && other.enabled == self.enabled
            && other.read_only == self.read_only
            && other.style == self.style
            && other.line_limits == self.line_limits
            && other.cursor_brush == self.cursor_brush
            && other.selection_color == self.selection_color
            && other.keyboard_options == self.keyboard_options
            && other.interaction_source == self.interaction_source
            && other.input_transformation == self.input_transformation
            && other.output_transformation == self.output_transformation
            && same_layout_callback(&other.on_text_layout, &self.on_text_layout)
            && other.scroll_state == self.scroll_state
            && other.password == self.password
            && match (&other.on_keyboard_action, &self.on_keyboard_action) {
                (Some(a), Some(b)) => a.same(b),
                (None, None) => true,
                _ => false,
            }
            && match (&other.default_action, &self.default_action) {
                (Some((a, x)), Some((b, y))) => {
                    a.same(b)
                        && match (x, y) {
                            (Some(x), Some(y)) => x.same(y),
                            (None, None) => true,
                            _ => false,
                        }
                }
                (None, None) => true,
                _ => false,
            }
    }

    fn name(&self) -> &'static str {
        "TextField"
    }
}

/// How what a field shows maps to its text: a value-based field's visual transformation (D147),
/// or a state-based field's output transformation, whose edits map with Compose's
/// `TransformedTextFieldState` rules (D1134).
#[derive(Clone)]
enum ShownMapping {
    Visual(Rc<dyn OffsetMapping>),
    Output(Rc<OffsetMappingCalculator>),
}

/// The same text layout callback, or none in both.
fn same_layout_callback(
    a: &Option<Callback<dyn Fn(compose_ui::TextLayoutResult)>>,
    b: &Option<Callback<dyn Fn(compose_ui::TextLayoutResult)>>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.same(b),
        (None, None) => true,
        _ => false,
    }
}

/// How a key moves the cursor over what an output transformation shows.
#[derive(Clone, Copy)]
enum Motion {
    Cluster,
    /// Backward by a code point, or an emoji whole: Backspace's.
    CodePointOrEmoji,
    Word,
    LineEdge,
    TextEdge,
}

/// What made an edit of a state-based field, for its state: whether the input method restarts
/// and whether the change is a hardware keyboard's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditSource {
    /// The input method's own commands: it doesn't restart.
    InputMethod,
    /// An on-screen keyboard's key: its own edit, so it doesn't restart either.
    SoftKey,
    /// A hardware keyboard's key.
    HardwareKey,
    /// The pointer, semantics, a focus change, a measurement.
    Other,
}

/// What a state-based field's scroll last followed, as Compose's `TextFieldCoreModifierNode`'s
/// `previousSelection`, `previousCursorRect`, `previousContainerSize` and
/// `previousTextLayoutSize`: nothing followed yet at first, so a first focus follows.
#[derive(Default)]
struct Followed {
    /// The selection shown, anchor then cursor.
    selection: Option<(usize, usize)>,
    /// The followed offset's cursor, in the scroller's coordinates.
    cursor: Option<Rect>,
    container: i32,
    content: i32,
}

/// What ties a state-based field's node to its [`TextFieldState`] (D1130): the node edits a copy
/// of the state's value, as the value-based field does, takes in the state's value when it
/// changed elsewhere (`adopt_state`) and writes its own edits back after each event
/// (`publish_state`).
pub(crate) struct StateLink {
    state: TextFieldState,
    /// The state's value as the node last took it in or wrote it: a value that differs was
    /// written elsewhere.
    seen: Option<TextFieldCharSequence>,
    /// What the node replaced in its text since it last wrote it, in order, each range in the
    /// text as it was then: the edits' own ranges, which the state's changes and history are of
    /// (D1132).
    edits: Vec<(std::ops::Range<usize>, String)>,
    /// The ranges the input method's commands just replaced, for the text change they make.
    command_edits: Option<Vec<(std::ops::Range<usize>, String)>>,
    /// What sent the last key down: the text it types is that key's edit (Compose's
    /// `isFromSoftKeyboard`).
    typed_by: Option<EditSource>,
    /// The commands being applied are the input method's: what they replace is kept as what they
    /// changed, as Compose's `imeReplace` (D1132).
    input_method_edit: bool,
    /// What the input method was shown, for the commands being applied (D1135).
    input_method_shown: Option<Rc<dyn compose_ui::ShownText>>,
    /// Hears each new layout (D1136).
    on_text_layout: Option<Callback<dyn Fn(compose_ui::TextLayoutResult)>>,
    /// The paragraph it last heard of.
    heard_layout: Option<ParagraphHandle>,
    /// Ten "H"'s width, at the density and in the style it was measured (D1137).
    default_width: Option<(compose_ui::Density, TextStyle, i32)>,
    /// The application's scroll of the field (D1138).
    scroll_state: Option<crate::ScrollState>,
    /// What the scroll last followed, as Compose's `updateScrollState`: it follows again when the
    /// selection, the cursor, the viewport or the content changes (D1140).
    followed: Followed,
    /// The node, to bring its cursor into view after a layout; taken at its first focus.
    handle: Option<compose_ui::NodeHandle>,
    /// Only the style's paint changed: the paragraph is given it at the next measurement.
    restyle: bool,
    /// A password's: whether it's shown obfuscated (D1142).
    password: Option<bool>,
    /// The cursor last brought into view, in the text, and the text's size then: shown again
    /// in the containers around once either changes while the cursor shows (D1141).
    revealed: Option<(Rect, IntSize)>,
    /// The edit being made is one of its own in the state's history (a cut, a paste, a selection
    /// deleted), as Compose's `NeverMerge`.
    never_merge: bool,
    /// Set by the state's listener: the input method must start over (Compose's `restartIme`).
    restart: Rc<Cell<bool>>,
    listener: u64,
    /// The cursor's brush, when it isn't one color.
    cursor_brush: Option<compose_ui::Brush>,
    /// What the user's edits go through before the state (D1133).
    input_transformation: Option<InputTransformation>,
    /// What the keyboard's action key does (D1133).
    on_keyboard_action: Option<KeyboardActionHandler>,
    /// What the handler's default action uses.
    default_action: Option<DefaultActionTargets>,
    /// What's shown of the state: the field's output transformation, a single line's after it
    /// (D1134).
    shown_transformation: Option<OutputTransformation>,
    /// Which side of an insertion or replacement the selection's ends sit on, as Compose's
    /// `selectionWedgeAffinity`.
    wedge_affinity: SelectionWedgeAffinity,
}

impl StateLink {
    fn new(state: TextFieldState, cursor_brush: Option<compose_ui::Brush>) -> StateLink {
        let restart = Rc::new(Cell::new(false));
        let listener = StateLink::listen(state, &restart);
        StateLink {
            state,
            seen: None,
            edits: Vec::new(),
            command_edits: None,
            typed_by: None,
            input_method_edit: false,
            input_method_shown: None,
            on_text_layout: None,
            heard_layout: None,
            default_width: None,
            scroll_state: None,
            followed: Followed::default(),
            handle: None,
            restyle: false,
            password: None,
            revealed: None,
            never_merge: false,
            restart,
            listener,
            cursor_brush,
            input_transformation: None,
            on_keyboard_action: None,
            default_action: None,
            shown_transformation: None,
            wedge_affinity: SelectionWedgeAffinity::default(),
        }
    }

    /// `value` as the input method reads it: through the output transformation, its selection
    /// and composition mapped (by the wedge affinity), as Compose's `visualText` (D1135).
    fn shown_to_input_method(&self, value: &TextFieldCharSequence) -> TextFieldValue {
        if let Some(transformation) = &self.shown_transformation
            && let Some((shown, mapping)) = transformed_text(value, transformation)
        {
            let affinity = Some(self.wedge_affinity);
            let selection = map_to_transformed(value.selection, &mapping, affinity);
            let composition =
                value.composition.map(|range| map_to_transformed(range, &mapping, affinity));
            return TextFieldValue::new(shown, selection, composition);
        }
        TextFieldValue::new(&*value.text, value.selection, value.composition)
    }

    /// Listens to `state` for its rule of restarting the input method, as the input session's
    /// `NotifyImeListener`.
    fn listen(state: TextFieldState, restart: &Rc<Cell<bool>>) -> u64 {
        let restart = restart.clone();
        state
            .add_notify_ime_listener(move |_, _, restart_ime| {
                if restart_ime {
                    restart.set(true);
                }
            })
            .unwrap_or(u64::MAX)
    }
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
    /// What it tells its handles, which its composable composes (D2008).
    pub handles: Option<Rc<TouchHandles>>,
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
            text_width: 0.0,
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
            state: None,
            touch: Touch::new(self.handles.clone()),
            autofill_highlight: None,
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
        node.touch.update(&self.handles);
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
            && same_handles(&other.handles, &self.handles)
    }

    fn name(&self) -> &'static str {
        "TextField"
    }
}

fn same_handles(a: &Option<Rc<TouchHandles>>, b: &Option<Rc<TouchHandles>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
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

/// Whether a later press of a multi-tap at `at` passed the touch slop: once it has, the slop
/// along where it passed is what its drag leaves out (D2018).
#[inline(never)]
fn past_slop(press: &mut Press, at: Offset, context: &mut EventContext<'_>) -> bool {
    let moved = at - press.at;
    let distance = moved.x.hypot(moved.y);
    let slop = context.view_configuration().touch_slop;
    if distance <= slop {
        return false;
    }
    press.slop_pending = false;
    press.slop = moved * (slop / distance);
    true
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
    /// A finger's first press, not moved past the slop: a long press may come (D2010).
    long_possible: bool,
    /// The long press came: the moves drag the selection by words until it's lifted.
    long: bool,
    /// A finger's later press of a multi-tap, not moved past the touch slop yet: its drag starts
    /// past it (Compose's `touchSelectionSubsequentPress`, D2018).
    slop_pending: bool,
    /// What the drag leaves out of the finger's movement: the touch slop, along where it first
    /// passed it (Compose's over-slop).
    slop: Offset,
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
    /// The text's width as laid out, at measurement: what the cursor's focus rect stays in.
    text_width: f32,
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
    mapping: Option<ShownMapping>,
    /// The transformed text shown, when it's transformed.
    shown_text: Option<String>,
    /// A state-based field's state (`BasicTextFieldOfState`); `None` for a value-based field.
    state: Option<Box<StateLink>>,
    /// Touch selection: its handles and what decides them (D2008).
    touch: Touch,
    /// Autofilled: the highlight's brush and the text filled, drawn while the text is that
    /// (D1685).
    autofill_highlight: Option<Box<(compose_ui::Brush, String)>>,
}

impl TextFieldNode {
    /// What the field laid out, as its text layout result: what's shown, at the text's
    /// constraints.
    fn layout_result_with(
        &self,
        measurer: &mut dyn compose_ui::platform::TextMeasurer,
        font_family_resolver: compose_ui::FontFamilyResolver,
        default_locale_list: compose_ui::LocaleList,
    ) -> Option<compose_ui::TextLayoutResult> {
        let (paragraph, constraints) = (self.paragraph?, self.constraints?);
        let text = self.shown_text.clone().unwrap_or_else(|| self.text.clone());
        // The text's constraints under the field's scroll, as Compose's: a single line unbounded
        // in width, lines in height (`TextFieldCoreModifier`'s `measure*Scroll`).
        let input_constraints = if self.single_line {
            Constraints { max_width: Constraints::INFINITY, ..constraints }
        } else {
            Constraints { max_height: Constraints::INFINITY, ..constraints }
        };
        let input = compose_ui::TextLayoutInput {
            text: text.into(),
            style: self.style.clone(),
            max_lines: if self.single_line { 1 } else { u32::MAX },
            soft_wrap: !self.single_line,
            overflow: compose_ui::TextOverflow::Clip,
            density: self.density?,
            layout_direction: self.direction,
            font_family_resolver,
            default_locale_list,
            constraints: input_constraints,
            placeholders: None,
        };
        // Laid out at the field's least height, as Compose's text under `heightInLines`, and
        // with no most height but a single line's: a multi-line field scrolls.
        let text = measurer.metrics(paragraph).size;
        let least = self.least_lines_height().clamp(constraints.min_height, constraints.max_height);
        let height = text.height.max(least);
        let height = if self.single_line { height.min(constraints.max_height) } else { height };
        // A single line is laid out as wide as its text, as Compose's under its horizontal scroll
        // (no most width), what scrolls in the field (D2017).
        let width = if self.single_line {
            text.width.max(constraints.min_width)
        } else {
            constraints.constrain(text).width
        };
        let size = IntSize::new(width, height);
        // Painted, it lays its input out again (D2053), the result's input shared (D2054).
        let input = std::rc::Rc::new(input);
        let multi_paragraph = compose_ui::MultiParagraph::of_paragraph_of(
            &input,
            measurer.paragraph(paragraph),
            input.max_lines,
        );
        Some(compose_ui::TextLayoutResult::new(input, std::rc::Rc::new(multi_paragraph), size))
    }

    /// A finger's later press of a multi-tap at `at`: a long press's start (D2018).
    #[inline(never)]
    fn subsequent_press(&mut self, at: Offset, unit: Unit, context: &mut EventContext<'_>) {
        self.cancel_long_press();
        self.press = Some(Press {
            at,
            dragging: false,
            unit,
            initial: 0..0,
            long_possible: false,
            long: true,
            slop_pending: true,
            slop: Offset::ZERO,
        });
        // Held past the long-press timeout, it drags without the slop (D2021).
        self.start_long_press(context);
        self.touch_subsequent_press(at, unit);
        context.invalidate_measure();
    }

    /// The fewest lines' height, as last measured (`line_bounds`).
    fn least_lines_height(&self) -> i32 {
        if self.single_line || self.min_lines <= 1 {
            return 0;
        }
        self.line_bounds.map_or(0, |(_, min, _)| min)
    }

    /// A state-based field's `on_text_layout` hears each new layout (a paragraph laid out
    /// again), after the layout pass, as Compose's `onTextLayout` at each new measurement
    /// (D1136).
    fn tell_text_layout(&mut self, scope: &mut MeasureScope<'_>, paragraph: ParagraphHandle) {
        let Some(link) = self.state.as_ref() else { return };
        let Some(callback) = link.on_text_layout.clone() else { return };
        if link.heard_layout == Some(paragraph) {
            return;
        }
        let resolver = scope.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER);
        let locales = scope.current_value_of(&compose_ui::LOCAL_LOCALE_LIST);
        let result = {
            let mut measurer = scope.text_measurer();
            self.layout_result_with(&mut *measurer, resolver, locales)
        };
        if let Some(link) = self.state.as_mut() {
            link.heard_layout = Some(paragraph);
        }
        if let Some(result) = result {
            scope.run_after_layout(move || callback(result));
        }
    }

    /// A state-based field's: takes in the state's value if it was changed elsewhere since the
    /// node last saw it (a programmatic `edit`, another field over the state). Returns whether it
    /// did. The history is the state's (D1132); what the state no longer composes isn't
    /// composed.
    #[inline]
    fn adopt_state(&mut self) -> bool {
        self.state.is_some() && self.adopt_state_now()
    }

    /// A state-based field's scroll with a `scroll_state` (D1138): the state's value, its bounds
    /// set from the layout, following the selection as Compose's `updateScrollState` (D1140):
    /// only while the cursor shows (focused and written to), when the selection's end, its start,
    /// or the viewport's or content's size changed since it last followed, to the offset that
    /// changed (the selection's least for a size). A right-to-left single line scrolls from its
    /// right, as Compose's `placeRelative`.
    #[inline(never)]
    fn scroll_by_state(
        &mut self,
        scope: &mut MeasureScope<'_>,
        scroll_state: crate::ScrollState,
        paragraph: ParagraphHandle,
        text: IntSize,
        size: IntSize,
        cursor_width: i32,
    ) -> bool {
        let mut short = false;
        let mirrored = self.single_line && self.direction == compose_ui::LayoutDirection::Rtl;
        // The whole field is the viewport, the text the content, as Compose's: the cursor's room
        // past the text's end is no content (`getCursorRectInScroller` keeps the cursor in the
        // text, D2017).
        let (visible, length) =
            if self.single_line { (size.width, text.width) } else { (size.height, text.height) };
        let max = (length - visible).max(0);
        compose_runtime::snapshots::Snapshot::without_read_observation(|| {
            scroll_state.set_layout(max, visible, length);
        });
        let mut value = scroll_state.value().clamp(0, max) as f32;
        let selection = (self.shown(self.anchor), self.shown(self.cursor));
        let show_cursor = self.focused && self.enabled && !self.read_only;
        let followed = &self.state.as_ref().expect("a state-based field").followed;
        let resized = length != followed.content || visible != followed.container;
        let to_follow = match followed.selection {
            Some((anchor, cursor)) if cursor == selection.1 => {
                if anchor == selection.0 {
                    resized.then_some(selection.0.min(selection.1))
                } else {
                    Some(selection.0)
                }
            }
            _ => Some(selection.1),
        };
        if let Some(offset) = to_follow.filter(|_| show_cursor) {
            let raw = scope.text_measurer().cursor_rect(paragraph, offset);
            // From the scroller's start: a right-to-left line's right.
            let left = if mirrored { length as f32 - raw.right } else { raw.left };
            let right = (left + cursor_width as f32).min(length as f32);
            let cursor = Rect::new(left, raw.top, right, raw.bottom);
            let moved = followed
                .cursor
                .is_none_or(|previous| previous.left != cursor.left || previous.top != cursor.top)
                || length != followed.content;
            if moved || visible != followed.container {
                let (start, end) = if self.single_line {
                    (cursor.left, cursor.right)
                } else {
                    (cursor.top, cursor.bottom)
                };
                let difference = offset_to_follow(value, start, end, visible as f32);
                let to = (value + round_to_next(difference)).clamp(0.0, max as f32);
                short = (round_to_next(difference) - (to - value)).abs() >= 1.0;
                if to != value {
                    scroll_state.set_value(to as i32);
                    value = to;
                }
                if let Some(link) = self.state.as_mut() {
                    link.followed = Followed {
                        selection: Some(selection),
                        cursor: Some(cursor),
                        container: visible,
                        content: length,
                    };
                }
            }
        }
        self.scroll = match (self.single_line, mirrored) {
            (true, true) => Offset::new(max as f32 - value, 0.0),
            (true, false) => Offset::new(value, 0.0),
            (false, _) => Offset::new(0.0, value),
        };
        short
    }

    /// Its text is a password's: never copied nor cut.
    fn is_password(&self) -> bool {
        self.visual_transformation.is_password()
            || self.state.as_ref().is_some_and(|link| link.password.is_some())
    }

    /// A state-based field's cursor, shown by the containers around it once the layout is over
    /// when it moved or the text's size changed while it shows (focused and written to), as
    /// Compose's `updateScrollState` brings it into view (D1141): typing a new line at the bottom of
    /// a field in a scrolling column scrolls the column. Asked only while the cursor shows.
    #[inline(never)]
    fn reveal_cursor(&mut self, scope: &mut MeasureScope<'_>, text: IntSize, short: bool) {
        let Some(cursor) = self.cursor_rect else { return };
        let collapsed = self.anchor == self.cursor;
        let scroll = self.scroll;
        let Some(link) = self.state.as_mut() else { return };
        let moved = link.revealed.is_none_or(|(previous, size)| {
            previous.left != cursor.left || previous.top != cursor.top || size != text
        });
        if !moved {
            return;
        }
        link.revealed = Some((cursor, text));
        let Some(handle) = link.handle else { return };
        let cursor = cursor_to_reveal(cursor, collapsed, short);
        let rect = Rect::new(
            cursor.left - scroll.x,
            cursor.top - scroll.y,
            cursor.right - scroll.x,
            cursor.bottom - scroll.y,
        );
        scope.run_after_layout(move || {
            // What the containers animate runs on, as Compose's launched `bringIntoView`.
            if let Some(shown) = handle.bring_into_view(Some(rect)) {
                compose_runtime::platform::launch(shown);
            }
        });
    }

    /// A state-based field's least width: ten "H" of its style, as Compose's `textFieldMinSize`
    /// (`DefaultWidthCharCount`); kept while the style and density are the same. `None` for a
    /// value-based field, as wide as its text (D1137).
    #[inline(never)]
    fn default_width(&mut self, scope: &mut IntrinsicMeasureScope<'_>) -> Option<i32> {
        let link = self.state.as_ref()?;
        let density = scope.density();
        if let Some((at, style, width)) = &link.default_width
            && *at == density
            && *style == self.style
        {
            return Some(*width);
        }
        let options = TextLayoutOptions {
            direction: scope.layout_direction(),
            ..TextLayoutOptions::default()
        };
        let width = {
            let mut measurer = scope.text_measurer();
            let paragraph = measurer.layout(
                "HHHHHHHHHH",
                &self.style,
                options,
                Constraints::unbounded(),
                &density,
            );
            let width = measurer.metrics(paragraph).size.width;
            measurer.release(paragraph);
            width
        };
        if let Some(link) = self.state.as_mut() {
            link.default_width = Some((density, self.style.clone(), width));
        }
        Some(width)
    }

    /// `adopt_state` at a measurement, and a new paint given to the paragraph (D1141). Returns
    /// whether what its semantics read changed: the state's value, or what a transformation
    /// shows of it (D1689).
    #[inline(never)]
    fn adopt_state_to_measure(&mut self, scope: &mut MeasureScope<'_>) -> bool {
        let mut changed = self.adopt_state_now();
        // What's shown may read states of its own (a secure field's revealed character, its
        // mask): read here, observed, so a change measures it again, and shown anew (D1142).
        if self.paragraph.is_some()
            && let Some(link) = self.state.as_ref()
            && let Some(transformation) = &link.shown_transformation
        {
            // The text compared, not mapped (D1145); the state's own text when it's the node's,
            // not copied.
            let text = match &link.seen {
                Some(seen) if *seen.text == *self.text => seen.text.clone(),
                _ => Rc::from(self.text.as_str()),
            };
            let value = TextFieldCharSequence::new(text, TextRange::ZERO, None);
            if !shows_as(&value, transformation, self.shown_text.as_deref()) {
                // Its semantics tell what's shown, but a password's own text (D1143).
                changed |= link.password.is_none();
                self.invalidate_paragraph();
            }
        }
        let Some(link) = self.state.as_mut() else { return changed };
        if !std::mem::take(&mut link.restyle) {
            return changed;
        }
        let Some(old) = self.paragraph else { return changed };
        let mut measurer = scope.text_measurer();
        self.paragraph = measurer.restyle(old, &self.style);
        if self.paragraph.is_none() {
            measurer.release(old);
            self.constraints = None;
        }
        // Heard again, as Compose's `onTextLayout` hears a new result of the same layout.
        link.heard_layout = None;
        changed
    }

    /// `adopt_state`, out of the value-based field's way.
    #[inline(never)]
    fn adopt_state_now(&mut self) -> bool {
        let Some(link) = self.state.as_mut() else { return false };
        let Some(value) = link.state.current() else { return false };
        if link.seen.as_ref() == Some(&value) {
            return false;
        }
        link.edits.clear();
        link.never_merge = false;
        let text_changed = *value.text != *self.text;
        let composition = value.composition.map(TextRange::range);
        let selection = value.selection;
        if text_changed {
            self.text = value.text.to_string();
        }
        link.seen = Some(value);
        if text_changed {
            self.invalidate_paragraph();
        }
        if composition != self.composition {
            self.end_composition();
            self.composition = composition;
        }
        if (selection.start, selection.end) != (self.anchor, self.cursor) {
            self.anchor = selection.start;
            self.cursor = selection.end;
            self.preferred_x = None;
        }
        true
    }

    /// `adopt_state` at an event's start: what it took in is measured, drawn and told again.
    #[inline]
    fn adopt_state_in(&mut self, context: &mut EventContext<'_>) {
        if self.state.is_some() && self.adopt_state() {
            context.invalidate_measure();
            context.invalidate_semantics();
            context.invalidate_draw();
        }
    }

    /// A state-based field's: writes what the node changed (text, selection, composition) into
    /// the state, at an event's end, as Compose's field edits it as the user
    /// (`editAsUser`); an input method's own edits don't restart it.
    #[inline]
    fn publish_state(&mut self, source: EditSource) {
        if self.state.is_some() {
            self.publish_state_now(source);
        }
    }

    /// Returns whether the state took something else than the node proposed.
    #[inline(never)]
    fn publish_state_now(&mut self, source: EditSource) -> bool {
        let Some(link) = self.state.as_mut() else { return false };
        let selection = TextRange::new(self.anchor, self.cursor);
        let composition =
            self.composition.clone().map(|range| TextRange::new(range.start, range.end));
        let unchanged = link
            .seen
            .as_ref()
            .is_some_and(|seen| seen.selection == selection && seen.composition == composition);
        if link.edits.is_empty() && unchanged {
            // A cut or paste that changed nothing leaves the next edit to merge.
            link.never_merge = false;
            return false;
        }
        let edits = std::mem::take(&mut link.edits);
        // An edit leaving a cursor puts it before what's inserted there, as Compose's
        // `updateWedgeAffinity`.
        if !edits.is_empty() && selection.collapsed() {
            link.wedge_affinity = SelectionWedgeAffinity::both(WedgeAffinity::Start);
        }
        let undo_behavior = if std::mem::take(&mut link.never_merge) {
            TextFieldEditUndoBehavior::NeverMerge
        } else {
            TextFieldEditUndoBehavior::MergeIfPossible
        };
        let text = &self.text;
        let restart = matches!(source, EditSource::HardwareKey | EditSource::Other);
        let hardware = source == EditSource::HardwareKey;
        let transformation = link.input_transformation.clone();
        link.state.edit_as_user_transformed(
            transformation.as_ref(),
            restart,
            undo_behavior,
            |buffer| {
                // Each edit where it was made, as Compose's field edits its state's buffer.
                for (range, inserted) in &edits {
                    buffer.replace_part(
                        range.start,
                        range.end,
                        inserted,
                        0,
                        inserted.len(),
                        hardware,
                    );
                }
                // The state holds what the node started from, so its edits make the node's text; if
                // not (nothing known breaks it), the difference.
                if buffer.as_str() != text.as_str() {
                    debug_assert!(false, "a field's edits didn't make its text");
                    buffer.set_text_if_changed(text);
                }
                buffer.set_selection_coerced(selection);
                match composition {
                    Some(range) if !range.collapsed() => {
                        buffer.set_composition(range.min(), range.max())
                    }
                    _ => buffer.commit_composition(),
                }
            },
        );
        // As the state holds it: an equal value it kept is the one to compare with; one a
        // transformation changed is taken in.
        let value = link.state.current_untracked();
        let differs = value.as_ref().is_some_and(|value| {
            *value.text != *self.text
                || value.selection != selection
                || value.composition != composition
        });
        link.seen = value;
        if differs {
            link.seen = None;
            self.adopt_state_now();
        }
        differs
    }

    /// `publish_state` in an event: what a transformation changed is measured, drawn and told
    /// again.
    #[inline]
    fn publish_state_in(&mut self, source: EditSource, context: &mut EventContext<'_>) {
        if self.state.is_some() && self.publish_state_now(source) {
            context.invalidate_measure();
            context.invalidate_semantics();
            context.invalidate_draw();
        }
    }

    /// An edit the input method didn't make (a cut, a paste) restarts it, as Compose's
    /// value-based field (D1664); a state-based field's restarts by its state's rule, and the
    /// edit is a step of the state's history of its own (D1132).
    fn restart_after_outside_edit(&mut self) {
        match self.state.as_mut() {
            Some(link) => link.never_merge = true,
            None => self.restart_input = true,
        }
    }

    /// `offset` in the text, in the paragraph shown.
    fn shown(&self, offset: usize) -> usize {
        match &self.mapping {
            None => offset,
            Some(ShownMapping::Visual(mapping)) => mapping.original_to_transformed(offset),
            Some(ShownMapping::Output(mapping)) => {
                map_to_transformed(TextRange::at(offset), mapping, Some(self.wedge_affinity()))
                    .start
            }
        }
    }

    /// `range` of the text, in the paragraph shown: a selection over a replacement covers it.
    fn shown_range(&self, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        match &self.mapping {
            Some(ShownMapping::Output(mapping)) => {
                let range = TextRange::new(range.start, range.end);
                map_to_transformed(range, mapping, Some(self.wedge_affinity())).range()
            }
            _ => self.shown(range.start)..self.shown(range.end),
        }
    }

    /// The state-based field's selection's wedge affinity (the default for any other field).
    fn wedge_affinity(&self) -> SelectionWedgeAffinity {
        self.state.as_ref().map_or_else(SelectionWedgeAffinity::default, |link| link.wedge_affinity)
    }

    /// What's shown for `text`, and how its offsets map: the value-based field's visual
    /// transformation, or the state-based field's output transformation (a single line's turning
    /// new lines to spaces included).
    fn transform_shown(&self, text: &str) -> Option<(String, ShownMapping)> {
        match &self.state {
            None => self
                .visual_transformation
                .filter(text)
                .map(|shown| (shown.text, ShownMapping::Visual(shown.offset_mapping))),
            Some(link) => {
                let transformation = link.shown_transformation.as_ref()?;
                let value = TextFieldCharSequence::new(Rc::from(text), TextRange::ZERO, None);
                transformed_text(&value, transformation)
                    .map(|(shown, mapping)| (shown, ShownMapping::Output(Rc::new(mapping))))
            }
        }
    }

    /// `offset` in the paragraph shown, in the text.
    fn original_of(&self, offset: usize) -> usize {
        match &self.mapping {
            None => offset,
            Some(ShownMapping::Visual(mapping)) => mapping.transformed_to_original(offset),
            // As Compose's `placeCursorAtNearestOffset`: inside a replacement, its nearer edge.
            Some(ShownMapping::Output(mapping)) => {
                let (kind, untransformed, retransformed) =
                    index_transformation_type(offset, mapping);
                match kind {
                    IndexTransformationType::Replacement
                        if retransformed.max() - offset < offset - retransformed.min() =>
                    {
                        untransformed.max()
                    }
                    _ => untransformed.min(),
                }
            }
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
        if let Some(link) = self.state.as_mut() {
            processor.track_edits(link.input_method_edit);
            if let Some(shown) = link.input_method_shown.take() {
                processor.show(shown);
            }
        }
        let value = processor.apply(commands);
        if let Some(link) = self.state.as_mut() {
            link.command_edits = Some(processor.take_edits());
        }
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
            // A new selection is the semantics' too: the frame tells the platform.
            if (selection.start, selection.end) != (self.anchor, self.cursor) {
                context.invalidate_semantics();
            }
            self.cursor = selection.end;
            self.anchor = selection.start;
            if let Some(link) = self.state.as_mut() {
                link.command_edits = None;
            }
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

    /// `apply_commands` of an input method's commands, typing: a state-based field keeps what
    /// they replace as what they changed, as Compose's `imeReplace`. Their offsets are in what
    /// the input method was shown if it `read` the text (`input_method_value`): a state-based
    /// field's output transformation's, mapped back as Compose's commands do (D1135); a
    /// platform's preedit reads nothing.
    fn apply_input_method_commands(
        &mut self,
        commands: &[&dyn EditCommand],
        read: bool,
        context: &mut EventContext<'_>,
    ) {
        if let Some(link) = self.state.as_mut() {
            link.input_method_edit = true;
            if read && let Some(transformation) = &link.shown_transformation {
                let value = TextFieldCharSequence::new(
                    Rc::from(self.text.as_str()),
                    TextRange::new(self.anchor, self.cursor),
                    None,
                );
                link.input_method_shown = transformed_text(&value, transformation).map(
                    |(shown, mapping)| -> Rc<dyn compose_ui::ShownText> {
                        Rc::new(ShownToInputMethod {
                            mapping: Rc::new(mapping),
                            length: shown.len(),
                        })
                    },
                );
            }
        }
        self.apply_commands(commands, true, context);
        if let Some(link) = self.state.as_mut() {
            link.input_method_edit = false;
        }
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

    /// Whether a font the field's text asked for arrived since `paragraph` was laid out, as
    /// `BasicText`'s (Compose's `hasStaleResolvedFonts`, D2007): the read of a state, so what
    /// measures is told when one does. Asked only of a style with fonts of its own: the field
    /// lays its text out in its style alone.
    fn is_stale(
        &self,
        measurer: &dyn compose_ui::platform::TextMeasurer,
        paragraph: ParagraphHandle,
    ) -> bool {
        matches!(self.style.font_family, Some(compose_ui::FontFamily::FontList(_)))
            && measurer.has_stale_resolved_fonts(paragraph)
    }

    /// Forgets what was measured in the fonts that were: the lines of "H" its least and most
    /// lines are, and the ten "H" a state-based field is at least as wide as.
    fn forget_font_measures(&mut self) {
        self.line_bounds = None;
        if let Some(link) = self.state.as_mut() {
            link.default_width = None;
        }
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
        // A state-based field's history is its state's.
        if self.state.is_none() {
            self.record_undo(typing);
        }
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
        // A state-based field's edit is written to its state at the event's end.
        if let Some(link) = self.state.as_mut() {
            // The commands' own ranges, or the whole text replaced (a semantics action's).
            match link.command_edits.take() {
                Some(edits) => link.edits.extend(edits),
                None => link.edits.push((0..self.text.len(), text.clone())),
            }
            self.text = text;
            self.cursor = cursor;
            self.anchor = anchor;
            context.invalidate_semantics();
            self.invalidate_paragraph();
            context.invalidate_measure();
            return;
        }
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
        // A state-based field's history is its state's (D1132).
        if let Some(link) = &self.state {
            let undo = link.state.undo_state();
            if redo {
                undo.redo();
            } else {
                undo.undo();
            }
            self.adopt_state_in(context);
            return;
        }
        let (from, to) =
            if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(snapshot) = from.pop() else { return };
        to.push(Snapshot { text: self.text.clone(), cursor: self.cursor, anchor: self.anchor });
        self.last_typing = None;
        if self.state.is_some() {
            self.end_composition();
        } else {
            self.text_replaced();
        }
        self.set_text(snapshot.text, snapshot.cursor, snapshot.anchor, context);
    }

    /// A state-based field's cursor moved by `motion` in what its output transformation shows,
    /// past what it inserted or replaced, as Compose's moves in its visual text with
    /// `calculateNextCursorPositionAndWedgeAffinity`: the range of the text it lands on (a deleted
    /// range's whole, a cursor otherwise). `None` without an output transformation.
    fn move_over_output(
        &mut self,
        motion: Motion,
        forward: bool,
        context: &mut EventContext<'_>,
    ) -> Option<std::ops::Range<usize>> {
        let (_, cursor, mapping) = self.step_over_output(motion, forward, context)?;
        // As Compose's `placeCursorBeforeCharAt(cursor)`: what the shown offset maps to.
        let untransformed =
            crate::output_transformation::map_from_transformed(TextRange::at(cursor), &mapping);
        Some(untransformed.min()..untransformed.max())
    }

    /// `move_over_output`'s step in the shown text: from where the cursor's shown, to where it
    /// goes, with the mapping; the wedge affinity taken.
    fn step_over_output(
        &mut self,
        motion: Motion,
        forward: bool,
        context: &mut EventContext<'_>,
    ) -> Option<(usize, usize, Rc<OffsetMappingCalculator>)> {
        let Some(ShownMapping::Output(mapping)) = self.mapping.clone() else { return None };
        let shown = self.shown_text.clone()?;
        let from = self.shown(self.cursor).min(shown.len());
        let proposed = match motion {
            Motion::Cluster if forward => {
                shown[from..].graphemes(true).next().map_or(from, |cluster| from + cluster.len())
            }
            Motion::Cluster => shown[..from]
                .graphemes(true)
                .next_back()
                .map_or(from, |cluster| from - cluster.len()),
            // Backspace's: a code point or an emoji of what's shown, as Compose's
            // `moveCursorPrevByCodePointOrEmoji` on its visual text (D1148).
            Motion::CodePointOrEmoji => code_point_or_emoji_before(&shown, from),
            Motion::Word if forward => next_word_in(&shown, from),
            Motion::Word => previous_word_in(&shown, from),
            Motion::LineEdge => self.line_edge_in(&shown, from, forward, |offset| offset, context),
            Motion::TextEdge => {
                if forward {
                    shown.len()
                } else {
                    0
                }
            }
        };
        let (cursor, affinity) = next_cursor_position(proposed, from, &mapping);
        if let (Some(link), Some(affinity)) = (self.state.as_mut(), affinity) {
            link.wedge_affinity = SelectionWedgeAffinity::both(affinity);
        }
        Some((from, cursor, mapping))
    }

    /// Backspace or Delete by `motion` in what an output transformation shows, as Compose's
    /// `deleteMovement` in its visual text: the shown range the cursor moves over, mapped back,
    /// is deleted (nothing, over only an insertion: the cursor goes past it), and the wedge
    /// affinity goes back to the start. Returns whether there was one to delete in.
    fn delete_over_output(
        &mut self,
        motion: Motion,
        forward: bool,
        context: &mut EventContext<'_>,
    ) -> bool {
        let Some((from, to, mapping)) = self.step_over_output(motion, forward, context) else {
            return false;
        };
        let shown = TextRange::new(from.min(to), from.max(to));
        let range = map_from_transformed(shown, &mapping);
        let range = range.min()..range.max();
        if self.anchor != self.cursor {
            // A selection shown as a cursor: what's deleted is the motion's alone.
            self.anchor = self.cursor;
        }
        if range.is_empty() {
            self.move_cursor(range.start, false, context);
        } else {
            self.delete(range, context);
        }
        if let Some(link) = self.state.as_mut() {
            link.wedge_affinity = SelectionWedgeAffinity::both(WedgeAffinity::Start);
        }
        context.invalidate_measure();
        context.invalidate_semantics();
        true
    }

    /// Whether the selection, not empty, shows as a cursor (it's a range an output
    /// transformation deleted).
    fn selection_shown_as_cursor(&self) -> bool {
        matches!(self.mapping, Some(ShownMapping::Output(_)))
            && self.shown_range(self.selection()).is_empty()
    }

    /// `move_over_output` from a key: the cursor moves there (the anchor with it, unless
    /// `extend`), and what's shown is measured and told again, the wedge affinity having maybe
    /// moved the cursor shown alone. Returns whether it moved over one.
    fn move_over_output_in(
        &mut self,
        motion: Motion,
        forward: bool,
        extend: bool,
        context: &mut EventContext<'_>,
    ) -> bool {
        let Some(range) = self.move_over_output(motion, forward, context) else { return false };
        self.move_cursor(range.end, extend, context);
        if !extend {
            self.anchor = range.start;
        }
        context.invalidate_measure();
        context.invalidate_semantics();
        true
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

    /// Where a tap at `position` puts the cursor: `offset_at`, but in what an output
    /// transformation inserted or replaced, at the edge whose cursor is nearer the tap, as
    /// Compose's `placeCursorAtNearestOffset`; in an insertion, by the side the cursor's shown on.
    fn tap_offset(&mut self, position: Offset, context: &mut EventContext<'_>) -> Option<usize> {
        let paragraph = self.paragraph?;
        let (offset, moved) = {
            let mut measurer = context.text_measurer();
            self.tap_offset_in(&mut *measurer, paragraph, position)
        };
        if moved {
            context.invalidate_semantics();
        }
        Some(offset)
    }

    /// `tap_offset` with `measurer` on `paragraph`, an event's or a measurement's (a tap before
    /// the field was laid out): the offset, and whether the wedge affinity changed.
    fn tap_offset_in(
        &mut self,
        measurer: &mut dyn compose_ui::platform::TextMeasurer,
        paragraph: ParagraphHandle,
        position: Offset,
    ) -> (usize, bool) {
        let index = measurer.hit_test(paragraph, position).offset;
        let Some(ShownMapping::Output(mapping)) = self.mapping.clone() else {
            return (floor_char_boundary(&self.text, self.original_of(index)), false);
        };
        let (kind, untransformed, retransformed) = index_transformation_type(index, &mapping);
        // Whether the replacement's or insertion's start is the nearer edge (by the cursors'
        // rects, as Compose's `findClosestRect`).
        let nearer_start = kind != IndexTransformationType::Untransformed && {
            let distance = |offset| {
                let rect = measurer.cursor_rect(paragraph, offset);
                let dx = (rect.left - position.x).max(position.x - rect.right).max(0.0);
                let dy = (rect.top - position.y).max(position.y - rect.bottom).max(0.0);
                dx * dx + dy * dy
            };
            distance(retransformed.start) < distance(retransformed.end)
        };
        let mut moved = false;
        let offset = match kind {
            IndexTransformationType::Untransformed | IndexTransformationType::Deletion => {
                untransformed.start
            }
            IndexTransformationType::Insertion => {
                let affinity = if nearer_start { WedgeAffinity::Start } else { WedgeAffinity::End };
                let affinity = SelectionWedgeAffinity::both(affinity);
                if let Some(link) = self.state.as_mut()
                    && link.wedge_affinity != affinity
                {
                    link.wedge_affinity = affinity;
                    moved = true;
                }
                untransformed.start
            }
            IndexTransformationType::Replacement if nearer_start => untransformed.start,
            IndexTransformationType::Replacement => untransformed.end,
        };
        (floor_char_boundary(&self.text, offset), moved)
    }

    /// The start or end of the visual (wrapped) line the cursor is on, from the measurer.
    fn line_edge(&self, end: bool, context: &mut EventContext<'_>) -> usize {
        let text = &self.text;
        let from_shown = |offset| floor_char_boundary(text, self.original_of(offset));
        self.line_edge_in(text, self.cursor, end, from_shown, context)
    }

    /// `line_edge` in `text` (the field's, or what an output transformation shows), the cursor
    /// at `cursor` in it, a paragraph's offset in it by `from_shown`.
    fn line_edge_in(
        &self,
        text: &str,
        cursor: usize,
        end: bool,
        from_shown: impl Fn(usize) -> usize,
        context: &mut EventContext<'_>,
    ) -> usize {
        if let Some(paragraph) = self.paragraph {
            let rect = context.text_measurer().cursor_rect(paragraph, self.shown(self.cursor));
            let y = (rect.top + rect.bottom) / 2.0;
            let x = if end { f32::MAX / 4.0 } else { -1.0 };
            let found = context.text_measurer().hit_test(paragraph, Offset::new(x, y)).offset;
            let found = from_shown(found);
            // At a soft wrap the line ends before the space that breaks it (not at a new line,
            // where a space ending the line is the line's).
            return if end
                && found < text.len()
                && !text[found..].starts_with('\n')
                && text[..found].ends_with(' ')
            {
                found - 1
            } else {
                found
            };
        }
        if end {
            text[cursor..].find('\n').map_or(text.len(), |index| cursor + index)
        } else {
            text[..cursor].rfind('\n').map_or(0, |index| index + 1)
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
        // Past the first or last line, the text's end, where the next line starts afresh, as
        // Compose's cached x forgotten there (D1147).
        if y < 0.0 {
            self.preferred_x = None;
            return 0;
        }
        if y >= height {
            self.preferred_x = None;
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
        // A state-based single line keeps a new line, shown as a space (D1146), as Compose's.
        let inserted: String = if self.single_line && self.state.is_none() {
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
    fn on_detach(&mut self, context: &mut compose_ui::NodeContext<'_>) {
        self.tell_focus(false);
        // Its handles go with it (D2011).
        self.touch.reset(Some(context.handle()));
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
    fn as_focus_properties(&mut self) -> Option<&mut dyn compose_ui::FocusPropertiesModifierNode> {
        Some(self)
    }
    fn as_key_input(&mut self) -> Option<&mut dyn KeyInputModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, context: &mut ReleaseContext<'_>) {
        if let Some(link) = &self.state {
            link.state.remove_notify_ime_listener(link.listener);
        }
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
        if let Some(link) = self.state.as_mut() {
            // As a new field's: what the state holds, taken in at the next measurement.
            link.seen = None;
            link.edits.clear();
            context.invalidate_measurement();
        } else if self.text != self.value {
            // As a new field's: the text is the value (an edit not taken yet is dropped).
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
        self.touch.reset(Some(context.handle()));
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
        // A state-based field's handler, given the default action, as Compose's
        // `onImeActionPerformed`; `None` and `Default` are never its.
        if let Some(link) = &self.state
            && let (Some(handler), Some((focus_manager, keyboard))) =
                (link.on_keyboard_action.clone(), link.default_action.clone())
            && !matches!(action, ImeAction::None | ImeAction::Default | ImeAction::Unspecified)
        {
            context.run(move || {
                handler.on_keyboard_action(&|| {
                    default_keyboard_action(action, &focus_manager, keyboard.as_ref());
                })
            });
            return true;
        }
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

/// Compose's default for `action`, outside an event (a keyboard action handler's): `Next` and
/// `Previous` move the focus, `Done` hides the on-screen keyboard, as
/// `defaultKeyboardActionWithResult`.
fn default_keyboard_action(
    action: ImeAction,
    focus_manager: &compose_ui::FocusManager,
    keyboard: Option<&compose_ui::SoftwareKeyboardController>,
) {
    use compose_ui::FocusDirection;
    match action {
        ImeAction::Next => {
            focus_manager.move_focus(FocusDirection::Next);
        }
        ImeAction::Previous => {
            focus_manager.move_focus(FocusDirection::Previous);
        }
        ImeAction::Done => {
            if let Some(keyboard) = keyboard {
                keyboard.hide();
            }
        }
        _ => {}
    }
}

/// How far a scroll at `value` moves to show `[start, end]` in `visible`, as Compose's
/// `updateScrollState`: the end first, then the start if the whole fits.
fn offset_to_follow(value: f32, start: f32, end: f32, visible: f32) -> f32 {
    let end_visible = value + visible;
    if end > end_visible || (start < value && end - start > visible) {
        end - end_visible
    } else if start < value {
        start - value
    } else {
        0.0
    }
}

/// Away from zero to a whole number, as Compose's `roundToNext`.
fn round_to_next(value: f32) -> f32 {
    if value > 0.0 { value.ceil() } else { value.floor() }
}

/// Where Backspace deletes back to from `offset`: the code point before it, or the emoji before
/// it whole (a grapheme cluster that is one: pictographic, a flag, a keycap), as Compose's
/// `findCodePointOrEmojiStartBefore` with EmojiCompat loaded (D1147).
fn code_point_or_emoji_before(text: &str, offset: usize) -> usize {
    let before = &text[..offset];
    let Some(cluster) = before.graphemes(true).next_back() else { return 0 };
    let is_emoji = cluster.chars().next().is_some_and(is_pictographic)
        || cluster.chars().any(|c| c == '\u{20E3}' || c == '\u{FE0F}');
    if is_emoji {
        offset - cluster.len()
    } else {
        before.char_indices().next_back().map_or(0, |(index, _)| index)
    }
}

/// Whether `c` starts an emoji: pictographic, or a flag's regional indicator.
fn is_pictographic(c: char) -> bool {
    matches!(
        c as u32,
        0xA9 | 0xAE
            | 0x203C
            | 0x2049
            | 0x2122
            | 0x2139
            | 0x2194..=0x21AA
            | 0x231A..=0x23FF
            | 0x24C2
            | 0x25AA..=0x25FE
            | 0x2600..=0x27BF
            | 0x2934..=0x2935
            | 0x2B05..=0x2B55
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0x1F000..=0x1FAFF
    )
}

/// Where the paragraph after `offset` ends (past a new line just after it), as Compose's
/// `findParagraphEnd`.
fn paragraph_end(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    (offset + 1..bytes.len()).find(|&index| bytes[index] == b'\n').unwrap_or(text.len())
}

/// Where the paragraph before `offset` starts (before a new line just before it), as Compose's
/// `findParagraphStart`.
fn paragraph_start(text: &str, offset: usize) -> usize {
    let bytes = text.as_bytes();
    (1..offset).rev().find(|&index| bytes[index - 1] == b'\n').unwrap_or(0)
}

/// The cursor a state-based field asks its containers to show: a collapsed cursor with no width,
/// one pixel wide where the field's scroll fell short of it, as Compose's `updateScrollState`
/// (D1145).
fn cursor_to_reveal(cursor: Rect, collapsed: bool, short: bool) -> Rect {
    if collapsed && cursor.right - cursor.left <= 0.0 && short {
        Rect::new(cursor.left, cursor.top, cursor.right + 1.0, cursor.bottom)
    } else {
        cursor
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
        // A state-based field takes in its state's value, the read observed: an edit of the state
        // measures it again.
        let selection = (self.anchor, self.cursor);
        let adopted = self.state.is_some() && self.adopt_state_to_measure(scope);
        // A font arrived: laid out anew (D2007).
        if let Some(paragraph) = self.paragraph
            && self.is_stale(&*scope.text_measurer(), paragraph)
        {
            self.invalidate_paragraph();
            self.forget_font_measures();
        }
        // At least ten "H" wide, as Compose's `textFieldMinSize`.
        let constraints = match self.state.is_some().then(|| self.default_width(scope)).flatten() {
            Some(width) => Constraints {
                min_width: constraints.min_width.max(width.min(constraints.max_width)),
                ..constraints
            },
            None => constraints,
        };
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
                // Shown transformed (a password's mask, an output transformation), its offsets
                // mapped.
                let text = match self.transform_shown(&self.text) {
                    Some((shown, mapping)) => {
                        self.mapping = Some(mapping);
                        self.shown_text = Some(shown.clone());
                        shown
                    }
                    None => {
                        self.mapping = None;
                        self.shown_text = None;
                        self.text.clone()
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
                // A new layout to hear (its handle may be the last one's, released).
                if let Some(link) = self.state.as_mut() {
                    link.heard_layout = None;
                }
            }
            let paragraph = self.paragraph.expect("laid out");
            if let Some(tap) = self.pending_tap.take() {
                let position = tap + self.scroll;
                let mut measurer = scope.text_measurer();
                self.cursor = self.tap_offset_in(&mut *measurer, paragraph, position).0;
                self.anchor = self.cursor;
            }
            self.follow_long_press(&mut *scope.text_measurer(), paragraph);
            self.follow_handle_drag(&mut *scope.text_measurer(), paragraph);
            self.perform_haptics(scope);
            paragraph
        };
        // What its semantics read changed here, not in an event that said so: the state's value
        // or what it shows taken in, a tap or a drag resolved (D1689), as Compose's decorator's
        // semantics observe the state.
        if adopted || selection != (self.anchor, self.cursor) {
            scope.invalidate_semantics();
        }
        self.cursor_rect =
            Some(scope.text_measurer().cursor_rect(paragraph, self.shown(self.cursor)));
        self.selection_rects.clear();
        self.composition_rects.clear();
        let selection = self.selection();
        if !selection.is_empty() {
            let range = self.shown_range(selection);
            self.selection_rects = scope.text_measurer().selection_rects(paragraph, range);
        }
        if let Some(composition) = self.composition.clone() {
            let range = self.shown_range(composition);
            self.composition_rects = scope.text_measurer().selection_rects(paragraph, range);
        }
        let metrics = scope.text_measurer().metrics(paragraph);
        // A multi-line field between `min_lines` and `max_lines` lines high, as Compose's
        // `heightInLines`; a field of one line as high as its first line's bottom, as Compose's
        // `heightForSingleLineField` (`getLineBottom(0)`, not the cursor's, D2004): a tall script
        // isn't cut. The legacy field's for `maxLines == 1` (a single line's too), the state's
        // for a single line.
        let one_line = if self.state.is_none() {
            self.max_lines == 1 || self.single_line
        } else {
            self.single_line
        };
        let (min_height, max_height) = if one_line {
            let line = scope.text_measurer().line_bottom(paragraph, 0).ceil() as i32;
            (line, line)
        } else {
            self.line_bounds(scope)
        };
        let size = constraints.constrain(IntSize::new(
            metrics.size.width + cursor_width,
            metrics.size.height.max(min_height).min(max_height),
        ));
        self.size = size;
        self.text_width = metrics.size.width as f32;
        self.overflows =
            metrics.size.width + cursor_width > size.width || metrics.size.height > size.height;
        // The text scrolls to keep the cursor in view, as Compose's: sideways on a single line,
        // down in a multi-line field.
        let rect = self.cursor_rect.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
        // A state-based field shows its cursor after its layout while the cursor shows (D1141).
        let reveals = self.state.is_some() && self.focused && self.enabled && !self.read_only;
        // Whether the scroll fell short of showing the cursor (D1145): only asked then.
        let short;
        if let Some(scroll_state) = self.state.as_ref().and_then(|link| link.scroll_state) {
            short = self.scroll_by_state(
                scope,
                scroll_state,
                paragraph,
                metrics.size,
                size,
                cursor_width,
            );
        } else if self.single_line {
            let visible = (size.width - cursor_width).max(0) as f32;
            let max = metrics.size.width as f32 - visible;
            let scrolled = scroll_into_view(self.scroll.x, rect.left, rect.left, visible, max);
            short = reveals
                && (scroll_into_view(self.scroll.x, rect.left, rect.left, visible, f32::MAX)
                    - scrolled)
                    .abs()
                    >= 1.0;
            self.scroll = Offset::new(scrolled, 0.0);
        } else {
            let visible = size.height as f32;
            let max = metrics.size.height as f32 - visible;
            let scrolled = scroll_into_view(self.scroll.y, rect.top, rect.bottom, visible, max);
            short = reveals
                && (scroll_into_view(self.scroll.y, rect.top, rect.bottom, visible, f32::MAX)
                    - scrolled)
                    .abs()
                    >= 1.0;
            self.scroll = Offset::new(0.0, scrolled);
        }
        if reveals {
            self.reveal_cursor(scope, metrics.size, short);
        }
        self.tell_handles(&*scope.text_measurer(), paragraph);
        self.tell_magnifier(&*scope.text_measurer(), paragraph);
        self.tell_toolbar(scope, paragraph);
        self.tell_text_layout(scope, paragraph);
        let placeable = scope.measure(content, Constraints::fixed(size.width, size.height));
        scope.place(&placeable, 0, 0);
        // A tap resolved here moved the cursor.
        self.publish_state(EditSource::Other);
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
        self.adopt_state();
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
        let text = match self.transform_shown(&self.text) {
            Some((shown, _)) => shown,
            None => self.text.clone(),
        };
        let mut measurer = scope.text_measurer();
        // A font arrived: laid out anew, and what was measured in the old font forgotten
        // (D2007).
        let stale =
            self.intrinsics.as_ref().is_some_and(|kept| self.is_stale(&*measurer, kept.paragraph));
        if stale {
            self.forget_font_measures();
        }
        let kept = self.intrinsics.as_ref().filter(|kept| {
            !stale
                && kept.text == text
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
        // The cursor of a collapsed selection only, as both of Compose's fields (D2027).
        if self.focused
            && !self.read_only
            && self.anchor == self.cursor
            && self.window.as_ref().is_none_or(|window| window.is_window_focused())
        {
            // The cursor: drawn where the measurer that laid the paragraph out puts it.
            let width = scope.density().to_px(CURSOR_WIDTH);
            let mut rect = shift(self.cursor_rect.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)));
            // A state-based field's viewport has no room past the text (D2017): at its end, the
            // cursor is kept inside, as Compose's `drawCursor` keeps it in the text.
            if self.state.is_some() && self.single_line {
                let room = (self.size.width as f32 - width).max(0.0);
                rect = Rect::new(rect.left.min(room), rect.top, rect.right, rect.bottom);
            }
            let paint = match self.state.as_ref().and_then(|link| link.cursor_brush.as_ref()) {
                Some(brush) => scope.brush_paint(brush, 1.0, compose_ui::PaintStyle::Fill),
                None => Paint::fill(self.cursor_color),
            };
            scope.draw_rect(Rect::new(rect.left, rect.top, rect.left + width, rect.bottom), paint);
        }
        if clipped {
            scope.restore();
        }
        // Autofilled: highlighted over it, until its text changes, as Compose's decorator (D1685).
        if let Some(highlight) = &self.autofill_highlight {
            let (brush, filled) = &**highlight;
            if *filled == self.text {
                let paint = scope.brush_paint(brush, 1.0, compose_ui::PaintStyle::Fill);
                scope.draw_rect(scope.bounds(), paint);
            } else {
                self.autofill_highlight = None;
            }
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
            && event.kind_moving() == PointerEventType::Move
            && event.consumed
            && self.press.as_ref().is_some_and(|press| !press.dragging && !press.long)
        {
            self.press = None;
        }
        if pass != PointerEventPass::Main {
            return;
        }
        // Every event says whether a finger or a mouse uses the field, as Compose's
        // `detectDirectTouchInteraction` (a mouse entering it too, D2027).
        self.touch_pointer_event(event, context);
        let inside = event.position.x >= 0.0
            && event.position.y >= 0.0
            && event.position.x < bounds.width as f32
            && event.position.y < bounds.height as f32;
        let at = event.position + self.scroll;
        match event.kind_moving() {
            // A right click shows the menu (D2040), neither moving the cursor nor focusing, as
            // Compose's `showTextContextMenuOnSecondaryClick` takes it before the selection's
            // gestures, which take the primary button only.
            PointerEventType::Press
                if !event.consumed
                    && event.pointer_type == compose_ui::PointerType::Mouse
                    && event.buttons.is_secondary_pressed() =>
            {
                event.consume();
                self.touch.right_click = true;
                self.right_click(event.position, context);
            }
            PointerEventType::Release if self.touch.right_click => {
                self.touch.right_click = false;
                event.consume();
            }
            // The selection's gestures take a mouse's primary button alone.
            PointerEventType::Press
                if event.pointer_type == compose_ui::PointerType::Mouse
                    && event.buttons.are_any_pressed()
                    && !event.buttons.is_primary_pressed() => {}
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
                // A finger's later press of a multi-tap selects the word (the paragraph from the
                // third) under it and drags by them, as a long press (Compose's
                // `touchSelectionSubsequentPress` gives the press to the long press's drag
                // observer): no handle while it drags (D2018, D2027).
                if event.pointer_type != compose_ui::PointerType::Mouse && unit != Unit::Character {
                    self.subsequent_press(at, unit, context);
                    return;
                }
                let mut initial = 0..0;
                if unit != Unit::Character
                    && let Some(offset) = self.offset_at(at, context)
                {
                    initial = self.unit_at(unit, offset);
                    self.anchor = initial.start;
                    self.cursor = initial.end;
                    self.mouse_selected();
                    context.invalidate_measure();
                }
                // A mouse's first press puts the cursor there and focuses the field at once, as
                // Compose's `TextFieldMouseSelectionObserver.onStart` (D2027).
                if event.pointer_type == compose_ui::PointerType::Mouse
                    && unit == Unit::Character
                    && let Some(offset) = self.tap_offset(at, context)
                {
                    self.anchor = offset;
                    self.cursor = offset;
                    self.preferred_x = None;
                    context.request_focus();
                    context.invalidate_measure();
                }
                let long_possible =
                    event.pointer_type != compose_ui::PointerType::Mouse && unit == Unit::Character;
                self.press = Some(Press {
                    at,
                    dragging: false,
                    unit,
                    initial,
                    long_possible,
                    long: false,
                    slop_pending: false,
                    slop: Offset::ZERO,
                });
                if long_possible {
                    self.start_long_press(context);
                }
            }
            PointerEventType::Move if event.pressed => {
                let Some(press) = &mut self.press else { return };
                let touch = event.pointer_type != compose_ui::PointerType::Mouse;
                // After a long press, the finger drags the selection by words (D2010).
                if press.long {
                    event.consume();
                    // A later press of a multi-tap drags once past the slop, without it.
                    if press.slop_pending && !past_slop(press, at, context) {
                        return;
                    }
                    let to = at - press.slop;
                    self.long_press_dragged(to);
                    context.invalidate_measure();
                    return;
                }
                // A finger's first press doesn't drag: the moves are for what's around, as
                // `touchSelectionFirstPress`, and it's still a tap unless it leaves the field.
                if touch && press.unit == Unit::Character {
                    let slop = context.view_configuration().touch_slop;
                    if (at - press.at).x.hypot((at - press.at).y) > slop {
                        press.long_possible = false;
                    }
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
                if !touch {
                    self.mouse_selected();
                }
                self.preferred_x = None;
                context.invalidate_measure();
            }
            PointerEventType::Release => {
                let Some(press) = self.press.take() else { return };
                self.cancel_long_press();
                if press.long {
                    // The long press's selection is made: the field takes the focus now, as
                    // Compose's `onDragStop` (D2010).
                    event.consume();
                    self.long_press_lifted();
                    // A double tap on the focused field asks for the keyboard as its taps do
                    // (Compose's tap detector beside its selection gestures, D2018).
                    if press.unit != Unit::Character && self.focused && !self.read_only {
                        context.show_software_keyboard();
                    }
                    context.request_focus();
                    context.invalidate_measure();
                    return;
                }
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
                match self.tap_offset(at, context) {
                    Some(offset) => {
                        self.cursor = offset;
                        self.anchor = offset;
                    }
                    None => self.pending_tap = Some(event.position),
                }
                self.touch_tapped();
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
        self.adopt_state_in(context);
        let selection = (self.anchor, self.cursor);
        self.on_pointer_event_here(event, pass, bounds, context);
        if selection != (self.anchor, self.cursor) {
            context.invalidate_semantics();
        }
        self.publish_state_in(EditSource::Other, context);
    }

    /// A right click opens its menu (D2040): it hears the secondary button too, its own gestures
    /// the primary one alone, as Compose's selection gestures (`isPrimaryPressed`).
    fn hears_every_button(&self) -> bool {
        true
    }

    fn on_cancel_pointer_input(&mut self) {
        self.touch.right_click = false;
        self.press = None;
        self.cancel_long_press();
        self.long_press_lifted();
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
        self.adopt_state_in(context);
        self.commit_preedit(context);
        if !self.read_only {
            context.show_software_keyboard();
        }
        match self.tap_offset(position + self.scroll, context) {
            Some(offset) => {
                self.cursor = offset;
                self.anchor = offset;
            }
            // Laid out again at the next measurement (an edit before the tap): resolved then.
            None => self.pending_tap = Some(position),
        }
        self.touch_tapped();
        context.invalidate_measure();
        self.publish_state_in(EditSource::Other, context);
    }

    fn on_focus_changed(
        &mut self,
        _previous: compose_ui::FocusState,
        current: compose_ui::FocusState,
        context: &mut EventContext<'_>,
    ) {
        self.adopt_state_in(context);
        let focused = current.is_focused();
        self.pin.focus_changed(focused, context);
        if focused {
            self.window = Some(context.current_value_of(&compose_ui::LOCAL_WINDOW_INFO));
        }
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        self.touch_focus_changed(focused, context);
        // Its decoration is told, as the field's gaining the focus shows it whole (D1145).
        tell_decoration(focused, context);
        self.tell_focus(focused);
        if focused {
            // Its cursor, as Compose's `bringSelectionEndIntoView`, beside the whole field its
            // decoration's node asks for, as Compose's `FocusableNode` (D1145): a container shows
            // the whole if it fits, else where it's typed in (D1141). All of it before it's laid
            // out.
            if let Some(rect) = self.cursor_rect {
                let scroll = self.scroll;
                context.bring_rect_into_view(Rect::new(
                    rect.left - scroll.x,
                    rect.top - scroll.y,
                    rect.right - scroll.x,
                    rect.bottom - scroll.y,
                ));
            }
            // As Compose's `StartInput`, which always asks for the keyboard: going from one field
            // to another with it hidden (`Done`, back) shows it at the first tap.
            if self.enabled && !self.read_only {
                context.show_software_keyboard();
            }
            // The cursor shows: a state-based field's scroll follows it from now (D1140), and
            // the containers around show it (D1141).
            if let Some(link) = self.state.as_mut() {
                if link.handle.is_none() {
                    link.handle = Some(context.handle());
                }
                context.invalidate_measure();
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
        self.publish_state_in(EditSource::Other, context);
    }
}

impl compose_ui::FocusPropertiesModifierNode for TextFieldNode {
    /// While it's focused, the field's rect in a focus search, and the one its containers keep in
    /// view, is its cursor's or its selection's, as Compose's `TextFieldDecoratorModifier`
    /// (`getFocusRect`); unfocused, all of it (D2002).
    fn apply_focus_properties(&self, properties: &mut compose_ui::FocusProperties) {
        if let Some(rect) = self.focus_rect() {
            properties.focus_rect = rect;
        }
    }
}

impl TextFieldNode {
    /// The cursor's rect in the text, as Compose's `calculateCursorRect`: the cursor's width, kept
    /// in the text and on a whole or half pixel. `None` before it's laid out.
    fn drawn_cursor_rect(&self) -> Option<Rect> {
        let (density, cursor) = (self.density?, self.cursor_rect?);
        let width = density.to_px(CURSOR_WIDTH).floor().max(1.0);
        let half = width / 2.0;
        let center = match self.direction {
            compose_ui::LayoutDirection::Ltr => cursor.left + half,
            compose_ui::LayoutDirection::Rtl => cursor.right - half,
        };
        // The text's width and the cursor's: the room the field gives them.
        let end = self.text_width + density.round_to_px(CURSOR_WIDTH) as f32;
        let center = center.min(end - half).max(half);
        let center = if width as i32 % 2 == 1 { center.floor() + 0.5 } else { center.round() };
        Some(Rect::new(center - half, cursor.top, center + half, cursor.bottom))
    }

    /// The focused field's cursor, as Compose's `calculateCursorRect` (the cursor's width, kept
    /// in the text and on a whole or half pixel), or the selection's bounds, under its scroll.
    fn focus_rect(&self) -> Option<Rect> {
        if !self.focused {
            return None;
        }
        let rect = if self.anchor == self.cursor {
            self.drawn_cursor_rect()?
        } else {
            let mut rects = self.selection_rects.iter();
            let first = *rects.next()?;
            rects.fold(first, |all, rect| {
                Rect::new(
                    all.left.min(rect.left),
                    all.top.min(rect.top),
                    all.right.max(rect.right),
                    all.bottom.max(rect.bottom),
                )
            })
        };
        let scroll = self.scroll;
        Some(Rect::new(
            rect.left - scroll.x,
            rect.top - scroll.y,
            rect.right - scroll.x,
            rect.bottom - scroll.y,
        ))
    }
}

fn key_is_not_tab(key: Key) -> bool {
    key != Key::Tab
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
        let modifiers = event.modifiers;
        let mapping = KeyMapping::current();
        // Typed keys: the text arrives as committed text (`on_text_input`); the field takes the
        // key, down and up, so its ancestors' shortcuts and clicks don't. Shortcuts (Cmd on
        // macOS, Ctrl elsewhere) and macOS's Emacs bindings (Ctrl) are mapped.
        if editable
            && matches!(event.key, Key::Character(_) | Key::Space)
            && !mapping.shortcut(modifiers)
            && !mapping.emacs(modifiers)
        {
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
        // The platform's key mapping, as Compose's `keyMapping.map(event)` (D1148); a command
        // editing the text isn't a read-only field's.
        let Some((command, select)) = mapping.map(event.key, modifiers) else { return false };
        if command.edits_text() && !editable {
            return false;
        }
        // Up, Down and the pages keep the horizontal position, as Compose's cached x.
        if !matches!(
            command,
            KeyCommand::Up | KeyCommand::Down | KeyCommand::PageUp | KeyCommand::PageDown
        ) {
            self.preferred_x = None;
        }
        // A state-based field's arrow moving nothing (at the text's edge) isn't taken, so the
        // focus moves on, as Compose's `TextFieldKeyEventHandler` (the selection unchanged); the
        // value-based field takes it, as Compose's `TextFieldKeyInput` (D1149).
        let edge_free = self.state.is_some()
            && matches!(
                command,
                KeyCommand::Up | KeyCommand::Down | KeyCommand::LeftChar | KeyCommand::RightChar
            );
        let before = edge_free.then(|| self.shown_selection());
        let taken = self.run_key_command(command, select, context);
        taken && before.is_none_or(|before| before != self.shown_selection())
    }

    /// The selection as shown: its ends and, of a state-based field, which side of an insertion
    /// each is shown on.
    fn shown_selection(&self) -> (usize, usize, Option<SelectionWedgeAffinity>) {
        (self.anchor, self.cursor, self.state.as_ref().map(|link| link.wedge_affinity))
    }

    /// What `command` does, as Compose's `TextFieldKeyEventHandler`; `select` extends the
    /// selection. Returns whether the key is taken.
    fn run_key_command(
        &mut self,
        command: KeyCommand,
        select: bool,
        context: &mut EventContext<'_>,
    ) -> bool {
        use KeyCommand as C;
        match command {
            // A password can't be copied nor cut, as Compose's.
            C::Copy => {
                let selection = self.selection();
                if !self.is_password() && !selection.is_empty() {
                    context.clipboard().set_text(&self.text[selection]);
                }
            }
            C::Cut => {
                let selection = self.selection();
                if !self.is_password() && !selection.is_empty() {
                    context.clipboard().set_text(&self.text[selection]);
                    self.replace_selection("", false, context);
                    self.restart_after_outside_edit();
                }
            }
            C::Paste => {
                if let Some(pasted) = context.clipboard().get_text() {
                    self.insert(&pasted, false, context);
                    self.restart_after_outside_edit();
                }
            }
            C::SelectAll => {
                self.anchor = 0;
                self.cursor = self.text.len();
                context.invalidate_measure();
            }
            C::Undo => self.undo(false, context),
            C::Redo => self.undo(true, context),
            // The selection collapsed to its end, as Compose's `DESELECT` (D1147).
            C::Deselect => {
                self.anchor = self.cursor;
                context.invalidate_measure();
            }
            // In a multi-line field, Tab types a tab, as Compose's; a single line's moves focus.
            C::Tab if self.single_line => return false,
            C::Tab => self.insert("\t", false, context),
            // A single line's Enter performs its action, as Compose's `onImeActionPerformed`.
            C::NewLine if self.single_line => {
                let action = self.keyboard_options.to_ime_options(true).ime_action;
                return self.perform_ime_action(action, context);
            }
            C::NewLine => self.insert("\n", false, context),
            // The D-pad's center shows a state-based field's on-screen keyboard, as Compose's
            // `CENTER`; the value-based field's leaves it to its focus, as `TextFieldKeyInput`.
            C::Center if self.state.is_some() => context.show_software_keyboard(),
            C::Center => return false,
            // macOS's Ctrl+K: to the line's end; at it, the new line goes.
            C::KillToLineEnd => {
                let end = self.line_edge(true, context);
                let end = if end == self.cursor { self.next_boundary(end) } else { end };
                self.delete(self.cursor..end.max(self.cursor), context);
            }
            C::DeletePrevChar
            | C::DeleteNextChar
            | C::DeletePrevWord
            | C::DeleteNextWord
            | C::DeleteFromLineStart
            | C::DeleteToLineEnd => self.delete_by_key(command, context),
            C::LeftChar
            | C::RightChar
            | C::LeftWord
            | C::RightWord
            | C::LineLeft
            | C::LineRight => {
                self.move_sideways(command, select, context);
            }
            // Taken even with nothing to move to, as Compose's: not left to a scroll around.
            C::PageUp | C::PageDown => {
                let cursor = self.by_page(command == C::PageDown, context);
                self.move_cursor(cursor, select, context);
            }
            C::Up | C::Down => {
                let cursor = self.vertical(command == C::Down, context);
                self.move_cursor(cursor, select, context);
            }
            // By paragraphs, as Compose's `NEXT_PARAGRAPH` and `PREV_PARAGRAPH` (D1147).
            C::PrevParagraph | C::NextParagraph => {
                let cursor = if command == C::NextParagraph {
                    paragraph_end(&self.text, self.cursor)
                } else {
                    paragraph_start(&self.text, self.cursor)
                };
                self.move_cursor(cursor, select, context);
            }
            C::Home | C::End => {
                let end = command == C::End;
                if !self.move_over_output_in(Motion::TextEdge, end, select, context) {
                    let cursor = if end { self.text.len() } else { 0 };
                    self.move_cursor(cursor, select, context);
                }
            }
            // The start and end of the visual (wrapped) line.
            C::LineStart | C::LineEnd => {
                let end = command == C::LineEnd;
                if !self.move_over_output_in(Motion::LineEdge, end, select, context) {
                    let cursor = self.line_edge(end, context);
                    self.move_cursor(cursor, select, context);
                }
            }
        }
        true
    }

    /// A deletion's key, as Compose's `deleteMovement`: a selection goes; else what the motion
    /// moves over.
    fn delete_by_key(&mut self, command: KeyCommand, context: &mut EventContext<'_>) {
        use KeyCommand as C;
        // A selection shown as a cursor (a deleted range) deletes by the motion, as Compose's
        // `deleteMovement` from its visual selection.
        if !self.selection().is_empty() && !self.selection_shown_as_cursor() {
            if let Some(link) = self.state.as_mut() {
                // As Compose's `deleteSelectedText`: a step of its own.
                link.never_merge = true;
            }
            self.replace_selection("", false, context);
            return;
        }
        let (motion, forward) = match command {
            C::DeletePrevChar => (Motion::CodePointOrEmoji, false),
            C::DeleteNextChar => (Motion::Cluster, true),
            C::DeletePrevWord => (Motion::Word, false),
            C::DeleteNextWord => (Motion::Word, true),
            C::DeleteFromLineStart => (Motion::LineEdge, false),
            _ => (Motion::LineEdge, true),
        };
        if self.delete_over_output(motion, forward, context) {
            return;
        }
        let cursor = self.cursor;
        let range = match command {
            // A code point, or an emoji whole, as Compose's `moveCursorPrevByCodePointOrEmoji`
            // (D1147).
            C::DeletePrevChar => code_point_or_emoji_before(&self.text, cursor)..cursor,
            C::DeletePrevWord => self.previous_word(cursor).min(cursor)..cursor,
            C::DeleteFromLineStart => self.line_edge(false, context).min(cursor)..cursor,
            C::DeleteNextChar => cursor..self.next_boundary(cursor),
            C::DeleteNextWord => cursor..self.next_word(cursor).max(cursor),
            // To the line's end, as Compose's `DELETE_TO_LINE_END`.
            _ => cursor..self.line_edge(true, context).max(cursor),
        };
        self.delete(range, context);
    }

    /// Left and Right by characters, words or to the line's sides: backward in left-to-right
    /// text, forward in right-to-left text.
    fn move_sideways(&mut self, command: KeyCommand, select: bool, context: &mut EventContext<'_>) {
        use KeyCommand as C;
        let right_to_left = self
            .paragraph
            .is_some_and(|paragraph| context.text_measurer().is_right_to_left(paragraph));
        let left = matches!(command, C::LeftChar | C::LeftWord | C::LineLeft);
        let backward = left != right_to_left;
        let motion = match command {
            C::LineLeft | C::LineRight => Motion::LineEdge,
            C::LeftWord | C::RightWord => Motion::Word,
            _ => Motion::Cluster,
        };
        let selection = self.selection();
        // A selection collapses where it's shown as one (not a deleted range, shown as a cursor),
        // as Compose's `collapseLeftOr` in its visual text.
        let collapses = matches!(motion, Motion::Cluster)
            && !select
            && !selection.is_empty()
            && !self.shown_range(selection.clone()).is_empty();
        if !collapses && self.move_over_output_in(motion, !backward, select, context) {
            return;
        }
        let cursor = match motion {
            Motion::LineEdge => self.line_edge(!backward, context),
            Motion::Word if backward => self.previous_word(self.cursor),
            Motion::Word => self.next_word(self.cursor),
            // Without Shift, an arrow collapses a selection to its side.
            _ if collapses => {
                if backward {
                    selection.start
                } else {
                    selection.end
                }
            }
            _ if backward => self.previous_boundary(self.cursor),
            _ => self.next_boundary(self.cursor),
        };
        self.move_cursor(cursor, select, context);
    }
}

impl KeyInputModifierNode for TextFieldNode {
    fn on_key_event(&mut self, event: &KeyEvent, context: &mut EventContext<'_>) -> bool {
        self.adopt_state_in(context);
        let source =
            if event.from_soft_keyboard { EditSource::SoftKey } else { EditSource::HardwareKey };
        if let Some(link) = self.state.as_mut() {
            link.typed_by = (event.kind == KeyEventType::KeyDown).then_some(source);
        }
        if source == EditSource::HardwareKey && event.kind == KeyEventType::KeyDown {
            self.touch_hardware_key(context);
        }
        let selection = (self.anchor, self.cursor);
        let consumed = self.on_key_event_here(event, context);
        if selection != (self.anchor, self.cursor) {
            context.invalidate_semantics();
        }
        // An on-screen keyboard's key is its own edit: it doesn't restart it, as Compose's key
        // handler (`restartImeIfContentChanges = !isFromSoftKeyboard`).
        self.publish_state_in(source, context);
        consumed
    }

    fn on_text_input(&mut self, text: &str, context: &mut EventContext<'_>) -> bool {
        if !self.enabled || self.read_only {
            return false;
        }
        self.adopt_state_in(context);
        self.preferred_x = None;
        // What a platform's preedit shows was never typed: what's sent takes its place.
        if self.preedit {
            self.on_preedit("", None, context);
        }
        self.insert(text, true, context);
        // The text of an on-screen keyboard's key is its own edit, as the key is.
        let source = self.state.as_mut().and_then(|link| link.typed_by.take());
        // Without a key, the platform's input method committed it.
        self.publish_state_in(source.unwrap_or(EditSource::InputMethod), context);
        true
    }

    /// A window over this field's takes the keys: what's being composed stays in the text, composed
    /// no more, and the input method starts over for whichever field it's given next (D1286).
    fn on_window_lost_keys(&mut self, context: &mut EventContext<'_>) {
        self.adopt_state_in(context);
        self.commit_preedit(context);
        self.composing_over_text = false;
        if self.composition.take().is_some() {
            context.invalidate_measure();
            context.invalidate_draw();
            context.reset_input_method();
        }
        self.publish_state_in(EditSource::InputMethod, context);
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
        self.adopt_state_in(context);
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
                self.apply_input_method_commands(&[&composing, &select], false, context);
            }
            None => self.apply_input_method_commands(&[&composing], false, context),
        }
        self.preedit = self.composition.is_some();
        self.publish_state_in(EditSource::InputMethod, context);
        true
    }

    fn input_method_value(&self) -> Option<TextFieldValue> {
        if !self.enabled || self.read_only {
            return None;
        }
        // A state-based field's is its state's, which an edit elsewhere may have changed since,
        // as it's shown.
        if let Some(link) = &self.state {
            return link.state.current_untracked().map(|value| link.shown_to_input_method(&value));
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
        self.adopt_state_in(context);
        self.preferred_x = None;
        self.preedit = false;
        self.apply_input_method_commands(commands, true, context);
        self.publish_state_in(EditSource::InputMethod, context);
        true
    }

    fn input_method_value_is(&self, value: &TextFieldValue) -> Option<bool> {
        if !self.enabled || self.read_only {
            return None;
        }
        if let Some(link) = &self.state {
            return link.state.current_untracked().map(|state| {
                if link.shown_transformation.is_some() {
                    return link.shown_to_input_method(&state) == *value;
                }
                value.text() == &*state.text
                    && value.selection() == state.selection
                    && value.composition() == state.composition
            });
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
        let restart = std::mem::take(&mut self.restart_input);
        match &self.state {
            Some(link) => link.restart.take() | restart,
            None => restart,
        }
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
        // The wedge affinity in the top bits: a move across an insertion moves the cursor shown.
        let affinity = self.wedge_affinity();
        let side = |affinity: WedgeAffinity| u64::from(affinity == WedgeAffinity::End) << 63;
        Some([
            paragraph.0,
            self.cursor as u64 | side(affinity.end),
            self.anchor as u64 | side(affinity.start),
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
        self.adopt_state_in(context);
        self.commit_preedit(context);
        // Android's default action key is `Done` for a single line.
        let action = match action {
            ImeAction::Default if self.single_line => ImeAction::Done,
            action => action,
        };
        let performed = self.perform_ime_action(action, context);
        self.publish_state_in(EditSource::InputMethod, context);
        performed
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
    /// What autofill reads of `apply_semantics`, with its values (D1688): a part, text to fill
    /// through the node, its fillable data and a state-based field's input text, without the
    /// rest (its actions bound at each key).
    fn apply_autofill_semantics(&self, config: &mut SemanticsConfiguration) {
        config.part_of_parent();
        let text = &self.text;
        if let Some(link) = &self.state {
            let shown: compose_ui::SharedString = match &self.shown_text {
                Some(shown) if link.password.is_none() => shown.clone().into(),
                _ => text.clone().into(),
            };
            if let Some(data) = FillableData::create_from_text(shown) {
                config.set_fillable_data(data);
            }
            config.set_input_text(text.clone());
        } else if let Some(data) = FillableData::create_from_text(text.clone()) {
            // The field's own text, transformed or not, as `apply_semantics`.
            config.set_fillable_data(data);
        }
        config.set_content_data_type(ContentDataType::TEXT);
        config.on_fill_data_of_node();
    }

    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        // The field's node is its decoration's, as Compose's `CoreTextField`.
        config.part_of_parent();
        // What's shown, and where offsets of the text are in it: a password's mask, a
        // transformation's own mapping, as Compose's; a composition is in the text.
        let text = &self.text;
        if self.state.is_some() {
            // A state-based field's: what it measured, the selection mapped as a range; a
            // password's own text, as Compose's announces it (D1143).
            let password = self.state.as_ref().is_some_and(|link| link.password.is_some());
            let shown: compose_ui::SharedString = match &self.shown_text {
                Some(shown) if !password => shown.clone().into(),
                _ => text.clone().into(),
            };
            let range =
                if password { self.selection() } else { self.shown_range(self.selection()) };
            let range = if self.anchor > self.cursor { range.end..range.start } else { range };
            config.set_text_selection_range(compose_ui::TextRange::new(
                floor_char_boundary(shown.as_str(), range.start),
                floor_char_boundary(shown.as_str(), range.end),
            ));
            // Autofill's value: what's shown, a password's own text (Compose's decorator).
            if let Some(data) = FillableData::create_from_text(shown.clone()) {
                config.set_fillable_data(data);
            }
            config.set_editable_text(shown);
            // The state's own text, transformed or not, as Compose's `inputText`.
            config.set_input_text(text.clone());
        } else {
            let (shown, mapping): (compose_ui::SharedString, _) = match self
                .visual_transformation
                .filter(text)
            {
                Some(transformed) => (transformed.text.into(), Some(transformed.offset_mapping)),
                None => (text.clone().into(), None),
            };
            let in_shown = |offset: usize| {
                let offset =
                    mapping.as_ref().map_or(offset, |it| it.original_to_transformed(offset));
                floor_char_boundary(shown.as_str(), offset)
            };
            config.set_text_selection_range(compose_ui::TextRange::new(
                in_shown(self.anchor),
                in_shown(self.cursor),
            ));
            // Autofill's value: the field's own text (Compose's `CoreTextField`), the shown one's
            // allocation when it's not transformed.
            let own: compose_ui::SharedString =
                if mapping.is_none() { shown.clone() } else { text.clone().into() };
            if let Some(data) = FillableData::create_from_text(own) {
                config.set_fillable_data(data);
            }
            config.set_editable_text(shown);
        }
        if self.visual_transformation.is_password() {
            config.password();
        }
        // A secure field's, obfuscated or shown, as Compose's `BasicSecureTextField`.
        if let Some(obfuscated) = self.state.as_ref().and_then(|link| link.password) {
            config.password_obfuscated(obfuscated);
        }
        // As Compose's `CoreTextField`: editable, and its text set by accessibility, when
        // enabled and not read-only.
        let editable = self.enabled && !self.read_only;
        config.set_is_editable(editable);
        // A field is text to autofill, filled through its node (D1681).
        config.set_content_data_type(ContentDataType::TEXT);
        config.on_fill_data_of_node();
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
        config.on_long_click_of_node();
        config.on_ime_action_of_node(
            self.keyboard_options.to_ime_options(self.single_line).ime_action,
        );
        if self.anchor != self.cursor && !self.is_password() {
            config.copy_text_of_node();
            if editable {
                config.cut_text_of_node();
            }
        }
        if editable {
            config.paste_text_of_node();
        }
        // What a state-based field's transformation says (its longest text).
        if let Some(transformation) =
            self.state.as_ref().and_then(|link| link.input_transformation.as_ref())
        {
            transformation.apply_semantics(config);
        }
    }

    fn perform_text_action(
        &mut self,
        action: compose_ui::TextAction,
        context: &mut EventContext<'_>,
    ) -> bool {
        self.adopt_state_in(context);
        let performed = self.perform_text_action_here(action, context);
        self.publish_state_in(EditSource::Other, context);
        performed
    }

    fn text_layout_offset(&self) -> Offset {
        self.scroll
    }

    fn text_layout_result(
        &mut self,
        context: &mut EventContext<'_>,
    ) -> Option<compose_ui::TextLayoutResult> {
        let resolver = context.current_value_of(&compose_ui::LOCAL_FONT_FAMILY_RESOLVER);
        let locales = context.current_value_of(&compose_ui::LOCAL_LOCALE_LIST);
        let mut measurer = context.text_measurer();
        self.layout_result_with(&mut *measurer, resolver, locales)
    }
}

impl TextFieldNode {
    fn perform_text_action_here(
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
            // As Compose's: focused, the selection's toolbar asked for (`updateTextToolbarState`,
            // `enterSelectionMode`); a value-based field's handles in selection mode too.
            Action::LongClick => {
                if !self.focused {
                    context.request_focus();
                }
                if self.state.is_none() {
                    self.touch_enter_selection_mode();
                    context.invalidate_measure();
                }
                self.set_toolbar_state(toolbar::ToolbarState::Selection);
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
            Action::FillData(data) => {
                // A state-based field refuses it unless editable (Compose's decorator); a
                // value-based one takes it, highlighted, and its text stays then
                // (`handleTextUpdateFromSemantics`).
                if !editable && self.state.is_some() {
                    return false;
                }
                if editable && let Some(text) = data.text_value() {
                    self.end_composition();
                    context.reset_input_method();
                    let end = text.as_str().len();
                    self.edit(text.as_str().to_string(), end, end, false, context);
                }
                // Highlighted until its text changes (D1685).
                let brush = context.current_value_of(&crate::LOCAL_AUTOFILL_HIGHLIGHT_BRUSH);
                self.autofill_highlight = Some(Box::new((brush, self.text.clone())));
                context.invalidate_draw();
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
            Action::SetSelection { start, end, relative_to_original } => {
                if !self.enabled {
                    return false;
                }
                // In what's shown; what's being composed is part of the text.
                let transformed = match &self.state {
                    // What a state-based field measured, its offsets mapped by its rules.
                    Some(_) => None,
                    None => self.visual_transformation.filter(&self.text),
                };
                let shown_length = match (&self.state, &self.shown_text) {
                    (Some(_), Some(shown)) => shown.len(),
                    _ => transformed.as_ref().map_or(self.text.len(), |shown| shown.text.len()),
                };
                let within = |offset: i32| offset >= 0 && offset as usize <= shown_length;
                if !within(start) || !within(end) {
                    return false;
                }
                let at = |offset: i32| {
                    let offset = offset as usize;
                    let original = match &self.state {
                        Some(_) => self.original_of(offset),
                        None => transformed
                            .as_ref()
                            .map_or(offset, |it| it.offset_mapping.transformed_to_original(offset)),
                    };
                    floor_char_boundary(&self.text, original)
                };
                let (anchor, cursor) = match &self.mapping {
                    // A state-based field's: what the shown range touches, as Compose's
                    // `selectCharsIn` (a cursor inside a replacement selects what it replaced).
                    Some(ShownMapping::Output(mapping)) => {
                        let range = TextRange::new(start as usize, end as usize);
                        let range = map_from_transformed(range, mapping);
                        let at = |offset| floor_char_boundary(&self.text, offset);
                        (at(range.start), at(range.end))
                    }
                    _ => (at(start), at(end)),
                };
                if (anchor, cursor) == (self.anchor, self.cursor) {
                    return false;
                }
                self.commit_composition(context);
                self.anchor = anchor;
                self.cursor = cursor;
                self.preferred_x = None;
                // The selection's toolbar, but for a service's traversal or a cursor, as
                // Compose's `setSelection` (D2032).
                let state = if relative_to_original || start == end {
                    toolbar::ToolbarState::None
                } else {
                    toolbar::ToolbarState::Selection
                };
                self.set_toolbar_state(state);
                context.invalidate_measure();
                context.invalidate_semantics();
                true
            }
            Action::CopyText | Action::CutText => {
                let selected = self.selection();
                if selected.is_empty() || self.is_password() {
                    return false;
                }
                let is_cut = action == Action::CutText;
                if is_cut && !editable {
                    return false;
                }
                let end = selected.end;
                let selected = self.text[selected].to_string();
                context.clipboard().set_text(&selected);
                if is_cut {
                    self.replace_selection("", false, context);
                    self.restart_after_outside_edit();
                } else {
                    // Copied, the selection collapses to its end, as Compose's
                    // `copy(cancelSelection = true)` for both fields.
                    self.move_cursor(end, false, context);
                }
                true
            }
            Action::PasteText => {
                if !editable {
                    return false;
                }
                if let Some(pasted) = context.clipboard().get_text() {
                    self.insert(&pasted, false, context);
                    self.restart_after_outside_edit();
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
}

// ---- The decoration's taps ----------------------------------------------------------------

/// Focuses the focus target below it on a tap (a release after a press, inside), and tells
/// `interaction_source` of presses anywhere in the field, as `tapPressTextFieldModifier`.
///
/// It also asks the containers around to show the whole field when the field gains the focus, as
/// the `FocusableNode` of Compose's `TextFieldDecoratorModifier` (D1145).
struct FocusOnTap {
    interaction_source: Option<compose_ui::MutableInteractionSource>,
}

/// The key the field's node finds its decoration's `FocusOnTapNode` by, to tell it of its focus.
static FIELD_DECORATION: compose_ui::TraverseKey =
    compose_ui::TraverseKey::new("TextFieldDecoration");

/// Tells the decoration's `FocusOnTapNode` around the field whether the field has the focus
/// (D1145): found when the focus changes, so no state is shared through the composition (D1149).
fn tell_decoration(focused: bool, context: &mut compose_ui::NodeContext<'_>) {
    let Some(mut decoration) = context.find_nearest_ancestor_with_key(&FIELD_DECORATION) else {
        return;
    };
    let node: &mut dyn ModifierNode = &mut *decoration;
    if let Some(decoration) = node.downcast_mut::<FocusOnTapNode>() {
        decoration.field_focused = focused;
    }
}

#[derive(Default)]
struct FocusOnTapNode {
    pressed: bool,
    interaction_source: Option<compose_ui::MutableInteractionSource>,
    press: Option<compose_ui::Press>,
    /// Whether the field below has the focus, as its node tells it.
    field_focused: bool,
    /// The field's focus when it was last told: shown when it's gained.
    focused: bool,
}

impl FocusOnTapNode {
    /// Forgets the field's focus: detached or reused, it's another field's decoration next.
    fn forget_focus(&mut self) {
        self.field_focused = false;
        self.focused = false;
    }
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

    fn as_focus_event(&mut self) -> Option<&mut dyn compose_ui::FocusEventModifierNode> {
        Some(self)
    }

    fn on_detach(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.end_press(false);
        self.forget_focus();
    }

    fn on_reset(&mut self, _context: &mut compose_ui::NodeContext<'_>) {
        self.forget_focus();
    }

    fn as_traversable(&mut self) -> Option<&mut dyn compose_ui::TraversableNode> {
        Some(self)
    }
}

impl compose_ui::TraversableNode for FocusOnTapNode {
    fn traverse_key(&self) -> &'static compose_ui::TraverseKey {
        &FIELD_DECORATION
    }
}

impl compose_ui::FocusEventModifierNode for FocusOnTapNode {
    /// The field gaining the focus (not another target in its decoration) is shown whole, if its
    /// containers can, as Compose's `FocusableNode.onFocusStateChange` (D1145).
    fn on_focus_event(&mut self, state: compose_ui::FocusState, context: &mut EventContext<'_>) {
        // Nothing below focused, the field isn't (it went while focused, or was reused).
        if !state.has_focus() {
            self.field_focused = false;
        }
        let focused = self.field_focused;
        if focused && !self.focused {
            context.bring_into_view();
        }
        self.focused = focused;
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
            && event.kind_moving() == PointerEventType::Move
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

#[cfg(test)]
#[path = "text_field_key_mapping_tests.rs"]
mod key_mapping_tests;

#[cfg(test)]
mod tests {
    use super::*;

    // Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/text/input/internal/TextFieldCoreModifier.kt, updateScrollState's bringIntoViewRect (no test of its own)
    #[test]
    fn a_cursor_the_scroll_fell_short_of_is_widened_by_a_pixel() {
        let cursor = Rect::new(10.0, 0.0, 10.0, 20.0);
        assert_eq!(cursor_to_reveal(cursor, true, true), Rect::new(10.0, 0.0, 11.0, 20.0));
        assert_eq!(cursor_to_reveal(cursor, true, false), cursor);
        assert_eq!(cursor_to_reveal(cursor, false, true), cursor);
        let wide = Rect::new(10.0, 0.0, 12.0, 20.0);
        assert_eq!(cursor_to_reveal(wide, true, true), wide);
    }

    /// A focus target telling the decoration around it of its gains only, as the field's node
    /// does of both: what a field that never told of its loss would leave the decoration with.
    struct TellsGains;
    struct TellsGainsNode;

    impl ModifierNodeElement for TellsGains {
        type Node = TellsGainsNode;
        fn create(&self) -> TellsGainsNode {
            TellsGainsNode
        }
        fn update(&self, _node: &mut TellsGainsNode) {}
        fn same(&self, _other: &Self) -> bool {
            true
        }
    }

    impl ModifierNode for TellsGainsNode {
        fn as_focus_event(&mut self) -> Option<&mut dyn compose_ui::FocusEventModifierNode> {
            Some(self)
        }
    }

    impl compose_ui::FocusEventModifierNode for TellsGainsNode {
        fn on_focus_event(
            &mut self,
            state: compose_ui::FocusState,
            context: &mut EventContext<'_>,
        ) {
            if state.is_focused() {
                tell_decoration(true, context);
            }
        }
    }

    /// The decoration forgets the field's focus when nothing below it is focused any more, though
    /// the field didn't tell it (D2000's backstop): focused again, it's shown whole again.
    // Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/text/input/internal/TextFieldDecoratorModifier.kt, onFocusEvent (no test of its own)
    #[test]
    fn the_decoration_forgets_a_focus_lost_below_it_untold() {
        use crate::gestures::ScrollableState;
        use crate::layout::{Arrangement, Box, Column};
        use crate::{Alignment, ScrollModifiers};
        use compose_runtime::callback;
        use compose_test::*;
        use compose_ui::UnitExt;
        fn spacer(height: i32) {
            let none = callback(|_| {});
            Box(Modifier.height(height.dp()), Alignment::TOP_START, false, none);
        }

        let scroll_state =
            **std::boxed::Box::leak(std::boxed::Box::new(crate::ScrollState::new(0)));
        let mut ui = compose_test(move || {
            let column = Modifier.height(100.dp()).vertical_scroll(scroll_state);
            Column(
                column,
                Arrangement::TOP,
                Alignment::START,
                callback(move |_| {
                    spacer(70);
                    // The decoration: the target, then 30 dp more, so it's 20 dp past the view.
                    let decoration = Modifier.then_custom(FocusOnTap { interaction_source: None });
                    let content = callback(move |_| {
                        let target = Modifier
                            .height(20.dp())
                            .then_custom(TellsGains)
                            .focus_target()
                            .test_tag("target");
                        Box(target, Alignment::TOP_START, false, callback(|_| {}));
                        spacer(30);
                    });
                    Column(decoration, Arrangement::TOP, Alignment::START, content);
                    spacer(300);
                }),
            );
        });
        ui.on_node_with_tag("target").request_focus();
        ui.wait_for_idle();
        assert!(scroll_state.value() > 0, "shown whole the first time");
        // The focus goes, untold; the view back at the top.
        ui.host().clear_focus();
        ui.wait_for_idle();
        scroll_state.dispatch_raw_delta(-(scroll_state.value() as f32));
        ui.wait_for_idle();
        assert_eq!(scroll_state.value(), 0);
        ui.on_node_with_tag("target").request_focus();
        ui.wait_for_idle();
        assert!(scroll_state.value() > 0, "shown whole again");
    }
}
