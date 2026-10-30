//! `TextField` and `OutlinedTextField`, as Compose's Material 3: a `BasicTextField` decorated
//! with a container (filled) or an outline, a label that rests in the field and floats up
//! (animated) when it's focused or has text, and a placeholder.

use std::rc::Rc;

use compose_foundation::layout::Box;
use compose_foundation::{BasicTextField, KeyboardActions, KeyboardOptions, VisualTransformation};
use compose_runtime::{Callback, CompositionLocalProvider, composable, mutable_state_of, remember};
use compose_ui::{
    Alignment, Color, Constraints, Dp, Measurable, MeasurePolicy, MeasureResult, MeasureScope,
    Modifier, Paint, Path, Shape, SizeSpec, TextStyle, UnitExt,
};

use crate::components::{TextFieldColors, TextFieldDefaults};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// As Compose's `OutlinedTextFieldDefaults`.
pub struct OutlinedTextFieldDefaults;

impl OutlinedTextFieldDefaults {
    pub const MIN_WIDTH: Dp = Dp(280.0);
    pub const MIN_HEIGHT: Dp = Dp(56.0);
    /// The outline's width, as `UnfocusedBorderThickness` and `FocusedBorderThickness`.
    pub const UNFOCUSED_BORDER_THICKNESS: Dp = Dp(1.0);
    pub const FOCUSED_BORDER_THICKNESS: Dp = Dp(2.0);

    /// From the theme, as `OutlinedTextFieldDefaults.colors()`: no container, the indicator
    /// colors for the outline.
    pub fn colors() -> TextFieldColors {
        let scheme = MaterialTheme::color_scheme();
        TextFieldColors {
            container_color: Color::TRANSPARENT,
            focused_container_color: Color::TRANSPARENT,
            error_container_color: Color::TRANSPARENT,
            disabled_container_color: Color::TRANSPARENT,
            unfocused_indicator_color: scheme.outline,
            disabled_indicator_color: scheme.on_surface.with_alpha(0.12),
            ..TextFieldDefaults::colors()
        }
    }

    /// The outline's shape, as `OutlinedTextFieldDefaults.shape` (extra small).
    pub fn shape() -> Shape {
        MaterialTheme::shapes().extra_small.clone()
    }
}

/// The label, its style and color read here, so a frame of them composes this alone, as
/// `DecoratedLabel`: from the body-large text's style to the small one's as it floats.
#[composable]
#[allow(non_snake_case)]
fn FieldLabel(progress: compose_runtime::State<f32>, color: Color, focused: bool, label: Callback) {
    let motion = MaterialTheme::motion_scheme();
    let typography = MaterialTheme::typography();
    let (large, small) = (&typography.body_large, &typography.body_small);
    let color = compose_animation::animate_color_as_state(
        color,
        motion.fast_effects_spec(),
        "LabelContentColor",
        None,
    )
    .get();
    let mut style = TextStyle::lerp(large, small, progress.get());
    // One of the two with a color of its own: that color, as `labelTextStyleColor`, focused the
    // small one's, else the large one's, a missing one the label's.
    if large.color.is_specified() != small.color.is_specified() {
        let target = if focused { small.color } else { large.color };
        style.color = compose_animation::animate_color_as_state(
            target.or(color),
            motion.fast_effects_spec(),
            "LabelTextStyleColor",
            None,
        )
        .get();
    }
    Box!({
        CompositionLocalProvider(
            (LOCAL_CONTENT_COLOR.provides(color), LOCAL_TEXT_STYLE.provides(style.clone())),
            &*label,
        );
    });
}

/// A filled field's indicator line: 1 dp, 2 dp focused, its animations read here.
#[composable]
#[allow(non_snake_case)]
fn IndicatorLine(
    color: Option<compose_runtime::State<Color>>,
    plain_color: Color,
    thickness: Option<compose_runtime::State<Dp>>,
    plain_thickness: Dp,
) {
    let color = color.map_or(plain_color, |state| state.get());
    let thickness = thickness.map_or(plain_thickness, |state| state.get());
    compose_foundation::layout::Spacer(Modifier.height(thickness).background(color));
}

/// What a text field's animations follow, as `InputPhase`.
#[derive(Clone, Copy, PartialEq)]
enum InputPhase {
    Focused,
    UnfocusedEmpty,
    UnfocusedNotEmpty,
}

/// A filled text field, as Compose's `TextField`: a container with an indicator line (2 dp
/// focused), a label resting in the field that floats above the text when focused or filled,
/// a placeholder while focused and empty, optional leading and trailing icons (in 48 dp
/// boxes), a prefix and a suffix around the text (shown with the floated label), and
/// supporting text below; `is_error` shows it all in the error color. A multi-line field is
/// between `min_lines` and `max_lines` lines high.
#[composable]
#[allow(non_snake_case)]
pub fn TextField(
    value: String,
    on_value_change: impl Fn(String) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(false)] read_only: bool,
    #[default(crate::LOCAL_TEXT_STYLE.current())] text_style: TextStyle,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(None)] placeholder: Option<impl Fn() + 'static>,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(None)] prefix: Option<impl Fn() + 'static>,
    #[default(None)] suffix: Option<impl Fn() + 'static>,
    #[default(None)] supporting_text: Option<impl Fn() + 'static>,
    #[default(false)] is_error: bool,
    #[default(crate::VisualTransformation::NONE)] visual_transformation: impl Into<VisualTransformation>,
    #[default(crate::KeyboardOptions::DEFAULT)] keyboard_options: KeyboardOptions,
    #[default(crate::KeyboardActions::new())] keyboard_actions: KeyboardActions,
    #[default(false)] single_line: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
    #[default(crate::TextFieldDefaults::colors())] colors: TextFieldColors,
) {
    decorated_field(
        Variant::Filled,
        value,
        compose_runtime::callback(on_value_change),
        modifier,
        enabled,
        read_only,
        text_style,
        Slots {
            label: label.map(compose_runtime::callback),
            placeholder: placeholder.map(compose_runtime::callback),
            leading_icon: leading_icon.map(compose_runtime::callback),
            trailing_icon: trailing_icon.map(compose_runtime::callback),
            prefix: prefix.map(compose_runtime::callback),
            suffix: suffix.map(compose_runtime::callback),
            supporting_text: supporting_text.map(compose_runtime::callback),
        },
        is_error,
        visual_transformation,
        keyboard_options,
        keyboard_actions,
        single_line,
        (max_lines, min_lines),
        colors,
    );
}

/// An outlined text field, as Compose's `OutlinedTextField`: an outline (2 dp focused) whose
/// top edge the floated label sits on, cut around it, 8 dp above the field for it; with the
/// filled field's icons, prefix and suffix, supporting text, error state and line bounds.
#[composable]
#[allow(non_snake_case)]
pub fn OutlinedTextField(
    value: String,
    on_value_change: impl Fn(String) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(false)] read_only: bool,
    #[default(crate::LOCAL_TEXT_STYLE.current())] text_style: TextStyle,
    #[default(None)] label: Option<impl Fn() + 'static>,
    #[default(None)] placeholder: Option<impl Fn() + 'static>,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(None)] prefix: Option<impl Fn() + 'static>,
    #[default(None)] suffix: Option<impl Fn() + 'static>,
    #[default(None)] supporting_text: Option<impl Fn() + 'static>,
    #[default(false)] is_error: bool,
    #[default(crate::VisualTransformation::NONE)] visual_transformation: impl Into<VisualTransformation>,
    #[default(crate::KeyboardOptions::DEFAULT)] keyboard_options: KeyboardOptions,
    #[default(crate::KeyboardActions::new())] keyboard_actions: KeyboardActions,
    #[default(false)] single_line: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
    #[default(crate::OutlinedTextFieldDefaults::colors())] colors: TextFieldColors,
) {
    decorated_field(
        Variant::Outlined,
        value,
        compose_runtime::callback(on_value_change),
        modifier,
        enabled,
        read_only,
        text_style,
        Slots {
            label: label.map(compose_runtime::callback),
            placeholder: placeholder.map(compose_runtime::callback),
            leading_icon: leading_icon.map(compose_runtime::callback),
            trailing_icon: trailing_icon.map(compose_runtime::callback),
            prefix: prefix.map(compose_runtime::callback),
            suffix: suffix.map(compose_runtime::callback),
            supporting_text: supporting_text.map(compose_runtime::callback),
        },
        is_error,
        visual_transformation,
        keyboard_options,
        keyboard_actions,
        single_line,
        (max_lines, min_lines),
        colors,
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    Filled,
    Outlined,
}

/// A decorated field's optional content.
#[derive(Clone)]
struct Slots {
    label: Option<Callback>,
    placeholder: Option<Callback>,
    leading_icon: Option<Callback>,
    trailing_icon: Option<Callback>,
    prefix: Option<Callback>,
    suffix: Option<Callback>,
    supporting_text: Option<Callback>,
}

/// The color for the field's state: disabled, in error, focused, or neither.
fn pick(enabled: bool, error: bool, focused: bool, colors: [Color; 4]) -> Color {
    let [disabled, error_color, focused_color, unfocused] = colors;
    match (enabled, error, focused) {
        (false, _, _) => disabled,
        (true, true, _) => error_color,
        (true, false, true) => focused_color,
        (true, false, false) => unfocused,
    }
}

#[allow(clippy::too_many_arguments)]
fn decorated_field(
    variant: Variant,
    value: String,
    on_value_change: Callback<dyn Fn(String)>,
    modifier: Modifier,
    enabled: bool,
    read_only: bool,
    text_style: TextStyle,
    slots: Slots,
    is_error: bool,
    visual_transformation: VisualTransformation,
    keyboard_options: KeyboardOptions,
    keyboard_actions: KeyboardActions,
    single_line: bool,
    (max_lines, min_lines): (u32, u32),
    colors: TextFieldColors,
) {
    let focused = remember(|| mutable_state_of(false));
    let is_focused = focused.get();
    // Empty as it shows, as Compose's `visualText`.
    let empty = visual_transformation
        .filter(&value)
        .map_or(value.is_empty(), |transformed| transformed.text.is_empty());
    let has_label = slots.label.is_some();
    let phase = if is_focused {
        InputPhase::Focused
    } else if empty {
        InputPhase::UnfocusedEmpty
    } else {
        InputPhase::UnfocusedNotEmpty
    };
    let motion = MaterialTheme::motion_scheme();
    let transition = compose_animation::update_transition(phase, "TextFieldInputState");
    // Each part's animation only if it's there, as Compose's, each read where it's used: a frame of
    // them composes neither this nor the rest of the field.
    // 0 resting in the field, 1 floated, as `labelProgress`.
    let progress = has_label.then(|| {
        let spec = motion.fast_spatial_spec();
        transition.animate_float_with(
            move |_| spec.clone(),
            "LabelProgress",
            |phase| if *phase == InputPhase::UnfocusedEmpty { 0.0 } else { 1.0 },
        )
    });
    // A placeholder fades in slowly as the field empties or is focused, as `placeholderOpacity`.
    let placeholder_alpha = slots.placeholder.is_some().then(|| {
        let (fast, slow) = (motion.fast_effects_spec(), motion.slow_effects_spec());
        transition.animate_float_with(
            move |segment| match (&segment.initial_state, &segment.target_state) {
                (InputPhase::Focused, InputPhase::UnfocusedEmpty) => fast.clone(),
                (InputPhase::UnfocusedEmpty, InputPhase::Focused)
                | (InputPhase::UnfocusedNotEmpty, InputPhase::UnfocusedEmpty) => slow.clone(),
                _ => fast.clone(),
            },
            "PlaceholderOpacity",
            move |phase| match phase {
                InputPhase::Focused => 1.0,
                InputPhase::UnfocusedEmpty => {
                    if has_label {
                        0.0
                    } else {
                        1.0
                    }
                }
                InputPhase::UnfocusedNotEmpty => 0.0,
            },
        )
    });
    // The prefix and suffix, shown as the label floats (or always, without one), as `affixOpacity`.
    let affix_alpha = (slots.prefix.is_some() || slots.suffix.is_some()).then(|| {
        let spec = motion.fast_effects_spec();
        transition.animate_float_with(
            move |_| spec.clone(),
            "PrefixSuffixOpacity",
            move |phase| {
                if *phase == InputPhase::UnfocusedEmpty && has_label { 0.0 } else { 1.0 }
            },
        )
    });
    // Transparent components interfere with screen readers: at alpha 0 they're not there at all, as
    // Compose's `showPlaceholder` and `showAffix`, crossing 0 composing this again.
    let shown = |alpha: Option<compose_runtime::State<f32>>| {
        compose_runtime::remember_keyed(alpha.is_some(), move || {
            Rc::new(compose_runtime::derived_state_of(move || {
                alpha.is_some_and(|alpha| alpha.get() > 0.0)
            }))
        })
        .get()
    };
    let show_placeholder_alpha = shown(placeholder_alpha);
    let show_affix = shown(affix_alpha);
    let indicator = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_indicator_color,
            colors.error_indicator_color,
            colors.focused_indicator_color,
            colors.unfocused_indicator_color,
        ],
    );
    // As `animateBorderStrokeAsState`: disabled, it snaps.
    let indicator_state = enabled.then(|| {
        compose_animation::animate_color_as_state(
            indicator,
            motion.fast_effects_spec(),
            "IndicatorColor",
            None,
        )
    });
    let (focused_thickness, unfocused_thickness) = match variant {
        Variant::Filled => (2.dp(), 1.dp()),
        Variant::Outlined => (
            OutlinedTextFieldDefaults::FOCUSED_BORDER_THICKNESS,
            OutlinedTextFieldDefaults::UNFOCUSED_BORDER_THICKNESS,
        ),
    };
    let thickness_state = enabled.then(|| {
        compose_animation::animate_dp_as_state(
            if is_focused { focused_thickness } else { unfocused_thickness },
            motion.fast_spatial_spec(),
            "IndicatorThickness",
            None,
        )
    });
    let indicator_color = move || indicator_state.map_or(indicator, |state| state.get());
    let indicator_thickness =
        move || thickness_state.map_or(unfocused_thickness, |state| state.get());
    let text_color = if enabled { colors.text_color } else { colors.disabled_text_color };
    let style = TextStyle { color: text_style.color.or(text_color), ..text_style.clone() };
    let typography = MaterialTheme::typography();
    let label_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_label_color,
            colors.error_label_color,
            colors.focused_label_color,
            colors.label_color,
        ],
    );
    let leading_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_leading_icon_color,
            colors.error_leading_icon_color,
            colors.leading_icon_color,
            colors.leading_icon_color,
        ],
    );
    let trailing_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_trailing_icon_color,
            colors.error_trailing_icon_color,
            colors.trailing_icon_color,
            colors.trailing_icon_color,
        ],
    );
    let supporting_color = pick(
        enabled,
        is_error,
        is_focused,
        [
            colors.disabled_supporting_text_color,
            colors.error_supporting_text_color,
            colors.supporting_text_color,
            colors.supporting_text_color,
        ],
    );
    let cursor_color = if is_error { colors.error_cursor_color } else { colors.cursor_color };
    // The prefix and suffix in the text's style.
    let prefix_style = TextStyle {
        color: if enabled { colors.prefix_color } else { colors.disabled_prefix_color },
        ..text_style.clone()
    };
    let suffix_style = TextStyle {
        color: if enabled { colors.suffix_color } else { colors.disabled_suffix_color },
        ..text_style.clone()
    };
    // As `TextFieldDefaults.Container`: the color read as it draws (`textFieldBackground`).
    let container_color = compose_animation::animate_color_as_state(
        pick(
            enabled,
            is_error,
            is_focused,
            [
                colors.disabled_container_color,
                colors.error_container_color,
                colors.focused_container_color,
                colors.container_color,
            ],
        ),
        motion.fast_effects_spec(),
        "ContainerColor",
        None,
    );
    let placeholder_color =
        if enabled { colors.placeholder_color } else { colors.disabled_placeholder_color };
    // Shown while the field is empty and it's not transparent, as Compose's.
    let show_placeholder = empty && slots.placeholder.is_some() && show_placeholder_alpha;
    let (has_prefix, has_suffix) =
        (slots.prefix.is_some() && show_affix, slots.suffix.is_some() && show_affix);
    let floated_label_height = typography.body_small.line_height;
    let supporting_style = typography.body_small.clone();
    // Remembered: the layout (remembered too) writes it, the outline reads it. Not state: the
    // outline draws in the node the layout measures, so measuring it draws it again (a reader
    // in another repaint boundary would draw a stale cut, docs/06).
    let label_width = remember(|| Rc::new(std::cell::Cell::new(0.0f32)));
    let label_start = remember(|| Rc::new(std::cell::Cell::new(0.0f32)));
    // The outline's path, reset each time it's drawn, as Compose's drawing caches keep one
    // (`CheckDrawingCache`): an animated cut redefines it in place on Android (D532).
    let outline = remember(Path::new);
    let decoration = {
        let (label_width, label_start) = (label_width.clone(), label_start.clone());
        let style = style.clone();
        move |inner_text_field: Callback| {
            let mut container = Modifier
                .size_in(SizeSpec::bounds(
                    TextFieldDefaults::MIN_WIDTH,
                    TextFieldDefaults::MIN_HEIGHT,
                    Dp::UNSPECIFIED,
                    Dp::UNSPECIFIED,
                ))
                .on_focus_changed(move |now: compose_ui::FocusState| focused.set(now.is_focused()));
            container = match variant {
                Variant::Filled => {
                    let shape = TextFieldDefaults::shape();
                    container.draw_behind(move |scope| {
                        scope.draw_shape(&shape, container_color.get());
                    })
                }
                Variant::Outlined => {
                    let shape = OutlinedTextFieldDefaults::shape();
                    let (label_width, label_start) = (label_width.clone(), label_start.clone());
                    let outline = outline.clone();
                    container.draw_behind(move |scope| {
                        scope.draw_shape(&shape, container_color.get());
                        draw_notched_outline(
                            scope,
                            &outline,
                            &shape,
                            indicator_thickness(),
                            indicator_color(),
                            label_start.get(),
                            label_width.get() * progress.map_or(0.0, |progress| progress.get()),
                        )
                    })
                }
            };
            let layout = DecorationLayout {
                variant,
                has_leading: slots.leading_icon.is_some(),
                has_trailing: slots.trailing_icon.is_some(),
                has_label,
                has_placeholder: show_placeholder,
                has_prefix,
                has_suffix,
                single_line,
                progress,
                floated_label_height,
                label_width: label_width.clone(),
                label_start: label_start.clone(),
            };
            let policy = compose_runtime::remember_keyed(LayoutKey(layout.key()), || {
                Rc::new(layout) as Rc<dyn MeasurePolicy>
            });
            let slots = slots.clone();
            let supporting_text = slots.supporting_text.clone();
            let below = remember(|| Rc::new(SupportingLayout) as Rc<dyn MeasurePolicy>);
            let style = style.clone();
            let (prefix_style, suffix_style) = (prefix_style.clone(), suffix_style.clone());
            let content = compose_runtime::callback(move || {
                // The text first: what focuses the field finds the first focus target below it,
                // and Tab goes from the text to its icons, as Compose's, whose field's target
                // is above them (D1298).
                inner_text_field();
                // Each icon in a 48 dp box, centered, as Compose's `IconDefaultSizeModifier`.
                for (icon, color) in
                    [(&slots.leading_icon, leading_color), (&slots.trailing_icon, trailing_color)]
                {
                    if let Some(icon) = icon.clone() {
                        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
                            let icon = icon.clone();
                            Box(
                                Modifier.size_in(SizeSpec::bounds(
                                    48.dp(),
                                    48.dp(),
                                    Dp::UNSPECIFIED,
                                    Dp::UNSPECIFIED,
                                )),
                                Alignment::CENTER,
                                false,
                                compose_runtime::callback(move |_| icon()),
                            );
                        });
                    }
                }
                if let (Some(label), Some(progress)) = (slots.label.clone(), progress) {
                    FieldLabel(progress, label_color, is_focused, label);
                }
                if show_placeholder && let Some(placeholder) = slots.placeholder.clone() {
                    let style = style.clone();
                    let placeholder_modifier = Modifier.graphics_layer(move |layer| {
                        layer.alpha = placeholder_alpha.map_or(0.0, |alpha| alpha.get());
                    });
                    Box!(modifier = placeholder_modifier, {
                        CompositionLocalProvider(
                            (
                                LOCAL_CONTENT_COLOR.provides(placeholder_color),
                                LOCAL_TEXT_STYLE.provides(style.clone()),
                            ),
                            &*placeholder,
                        );
                    });
                }
                // As Compose's `PrefixSuffixTextPadding`: 2 dp between them and the text.
                for (content, shows, style, padding) in [
                    (&slots.prefix, has_prefix, prefix_style.clone(), (0.dp(), 2.dp())),
                    (&slots.suffix, has_suffix, suffix_style.clone(), (2.dp(), 0.dp())),
                ] {
                    if let Some(content) = content.clone().filter(|_| shows) {
                        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), || {
                            let content = content.clone();
                            Box(
                                Modifier
                                    .graphics_layer(move |layer| {
                                        layer.alpha = affix_alpha.map_or(0.0, |alpha| alpha.get());
                                    })
                                    .padding((padding.0, 0.dp(), padding.1, 0.dp())),
                                Alignment::TOP_START,
                                false,
                                compose_runtime::callback(move |_| content()),
                            );
                        });
                    }
                }
                if variant == Variant::Filled {
                    IndicatorLine(indicator_state, indicator, thickness_state, unfocused_thickness);
                }
            });
            match supporting_text {
                // The supporting text below the container, 16 dp in, 4 dp under it.
                Some(supporting_text) => {
                    let (container, policy, content) = (container.clone(), policy.clone(), content);
                    let supporting_style = supporting_style.clone();
                    compose_ui::Layout(
                        Modifier,
                        below,
                        compose_runtime::callback(move || {
                            compose_ui::Layout(container.clone(), policy.clone(), content.clone());
                            let supporting_text = supporting_text.clone();
                            let supporting_style = supporting_style.clone();
                            CompositionLocalProvider(
                                LOCAL_CONTENT_COLOR.provides(supporting_color),
                                || {
                                    let supporting_text = supporting_text.clone();
                                    CompositionLocalProvider(
                                        LOCAL_TEXT_STYLE.provides(supporting_style.clone()),
                                        || {
                                            let supporting_text = supporting_text.clone();
                                            Box(
                                                Modifier.padding((
                                                    16.dp(),
                                                    4.dp(),
                                                    16.dp(),
                                                    0.dp(),
                                                )),
                                                Alignment::TOP_START,
                                                false,
                                                compose_runtime::callback(move |_| {
                                                    supporting_text()
                                                }),
                                            );
                                        },
                                    );
                                },
                            );
                        }),
                    );
                }
                None => compose_ui::Layout(container, policy, content),
            }
        }
    };
    // Room above an outlined field for its floated label, as `OutlinedTextFieldTopPadding`.
    let modifier = match variant {
        Variant::Outlined if has_label => modifier.padding((0.dp(), 8.dp(), 0.dp(), 0.dp())),
        _ => modifier,
    };
    // In error, the field says so, as Compose's `semantics { error(…) }`.
    let modifier = if is_error {
        modifier.semantics(|config| config.error("Invalid input"))
    } else {
        modifier
    };
    // Its selection in its colors' selection colors, as Compose's fields provide
    // `colors.textSelectionColors`.
    compose_runtime::CompositionLocalProvider(
        compose_foundation::LOCAL_TEXT_SELECTION_COLORS.provides(colors.text_selection_colors),
        || {
            BasicTextField(
                value,
                on_value_change,
                modifier,
                enabled,
                read_only,
                style,
                keyboard_options,
                keyboard_actions,
                single_line,
                max_lines,
                min_lines,
                visual_transformation,
                cursor_color,
                compose_runtime::callback(decoration),
            );
        },
    );
}

#[derive(Clone, Copy, PartialEq)]
struct LayoutKey((Variant, [bool; 7]));

impl compose_runtime::ComposeArg for LayoutKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The decoration's layout, as Compose's `TextFieldMeasurePolicy` and
/// `OutlinedTextFieldMeasurePolicy`: the leading and trailing icons (if any), the label (if
/// any), the placeholder (if shown), the prefix and suffix (if any), the text field, then
/// (filled) the indicator line. The prefix is before the text, the suffix at the end. 16 dp at
/// the sides (4 dp beside an icon, as `HorizontalIconPadding` takes 12 of them); a filled field
/// 8 dp above and below with a label (16 dp without), its text below where the floated label
/// goes; an outlined field 16 dp above and below, its floated label centered on the top edge,
/// 16 dp in. The icons are centered vertically. The label rests where a single line of text
/// would be (or at the top padding) and moves to its floated place with `progress`.
struct DecorationLayout {
    variant: Variant,
    has_leading: bool,
    has_trailing: bool,
    has_label: bool,
    has_placeholder: bool,
    has_prefix: bool,
    has_suffix: bool,
    single_line: bool,
    /// The label's: read while measuring, each frame of it laying the field out.
    progress: Option<compose_runtime::State<f32>>,
    floated_label_height: compose_ui::TextUnit,
    /// The label's width, for the outline's cut.
    label_width: Rc<std::cell::Cell<f32>>,
    /// The floated label's start, for the outline's cut.
    label_start: Rc<std::cell::Cell<f32>>,
}

/// A decoration's slots, as its children come.
struct DecorationSlots {
    leading: Option<Measurable>,
    trailing: Option<Measurable>,
    label: Option<Measurable>,
    placeholder: Option<Measurable>,
    prefix: Option<Measurable>,
    suffix: Option<Measurable>,
    text: Option<Measurable>,
}

impl DecorationLayout {
    fn slots(&self, measurables: &[Measurable]) -> DecorationSlots {
        let mut children = measurables.iter().copied();
        let mut next = |present: bool| if present { children.next() } else { None };
        DecorationSlots {
            text: next(true),
            leading: next(self.has_leading),
            trailing: next(self.has_trailing),
            label: next(self.has_label),
            placeholder: next(self.has_placeholder),
            prefix: next(self.has_prefix),
            suffix: next(self.has_suffix),
        }
    }

    /// The paddings `measure` puts around the content: (start, end, above and below, the
    /// filled field's floated label's room), given which icons there are.
    fn paddings(
        &self,
        density: compose_ui::Density,
        slots: &DecorationSlots,
    ) -> (i32, i32, i32, i32) {
        let padding = density.round_to_px(16.dp());
        let beside_icon = density.round_to_px(4.dp());
        let outlined = self.variant == Variant::Outlined;
        let vertical =
            density.round_to_px(if self.has_label && !outlined { 8.dp() } else { 16.dp() });
        let start = if slots.leading.is_some() { beside_icon } else { padding };
        let end = if slots.trailing.is_some() { beside_icon } else { padding };
        let label_room = if self.has_label && !outlined {
            density.sp_to_px(self.floated_label_height).round() as i32
        } else {
            0
        };
        (start, end, vertical, label_room)
    }

    /// As Compose's `intrinsicWidth`: the icons beside the widest of the text and the
    /// placeholder with the affixes, and the label.
    fn intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[Measurable],
        height: i32,
        max: bool,
    ) -> i32 {
        let slots = self.slots(measurables);
        let Some(text) = slots.text else { return 0 };
        let (start, end, _, _) = self.paddings(scope.density(), &slots);
        let mut ask = |slot: Option<Measurable>| {
            slot.map_or(0, |slot| {
                if max {
                    scope.max_intrinsic_width(slot, height)
                } else {
                    scope.min_intrinsic_width(slot, height)
                }
            })
        };
        let (leading, trailing) = (ask(slots.leading), ask(slots.trailing));
        let affixes = ask(slots.prefix) + ask(slots.suffix);
        let middle =
            (ask(Some(text)) + affixes).max(ask(slots.placeholder) + affixes).max(ask(slots.label));
        leading + start + middle + end + trailing
    }

    /// As Compose's `intrinsicHeight`: the content at the width left by the icons and the
    /// affixes, between the paddings and the floated label's room, and the icons.
    fn intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[Measurable],
        width: i32,
        max: bool,
    ) -> i32 {
        let slots = self.slots(measurables);
        let Some(text) = slots.text else { return 0 };
        let (start, end, vertical, label_room) = self.paddings(scope.density(), &slots);
        let less = |width: i32, by: i32| {
            if width == Constraints::INFINITY { width } else { (width - by).max(0) }
        };
        let ask = |scope: &mut compose_ui::IntrinsicMeasureScope<'_>, slot, width| {
            if max {
                scope.max_intrinsic_height(slot, width)
            } else {
                scope.min_intrinsic_height(slot, width)
            }
        };
        let mut remaining = less(width, start + end);
        let mut icons = 0;
        for icon in [slots.leading, slots.trailing].into_iter().flatten() {
            icons = icons.max(ask(scope, icon, width));
            let icon_width = scope.max_intrinsic_width(icon, Constraints::INFINITY);
            remaining = less(remaining, icon_width);
        }
        let label = slots.label.map_or(0, |label| ask(scope, label, remaining));
        let mut content = 0;
        for affix in [slots.prefix, slots.suffix].into_iter().flatten() {
            content = content.max(ask(scope, affix, remaining));
            let affix_width = scope.max_intrinsic_width(affix, Constraints::INFINITY);
            remaining = less(remaining, affix_width);
        }
        content = content.max(ask(scope, text, remaining));
        if let Some(placeholder) = slots.placeholder {
            content = content.max(ask(scope, placeholder, remaining));
        }
        // The label floats in its room (filled) or on the edge (outlined), as `measure` has
        // it; resting, it's no higher than the text.
        let _ = label;
        (vertical + label_room + content + vertical).max(icons)
    }

    fn key(&self) -> (Variant, [bool; 7]) {
        (
            self.variant,
            [
                self.has_leading,
                self.has_trailing,
                self.has_label,
                self.has_placeholder,
                self.has_prefix,
                self.has_suffix,
                self.single_line,
            ],
        )
    }
}

impl MeasurePolicy for DecorationLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let padding = density.round_to_px(16.dp());
        let beside_icon = density.round_to_px(4.dp());
        let outlined = self.variant == Variant::Outlined;
        let vertical =
            density.round_to_px(if self.has_label && !outlined { 8.dp() } else { 16.dp() });
        let mut children = measurables.iter().copied();
        let Some(text) = children.next() else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let leading = if self.has_leading { children.next() } else { None };
        let trailing = if self.has_trailing { children.next() } else { None };
        let label = if self.has_label { children.next() } else { None };
        let placeholder = if self.has_placeholder { children.next() } else { None };
        let prefix = if self.has_prefix { children.next() } else { None };
        let suffix = if self.has_suffix { children.next() } else { None };
        let indicator = children.next();
        let icon_constraints = constraints.loosen();
        let leading = leading.map(|icon| scope.measure(icon, icon_constraints));
        let trailing = trailing.map(|icon| scope.measure(icon, icon_constraints));
        let leading_width = leading.map_or(0, |icon| icon.width);
        let trailing_width = trailing.map_or(0, |icon| icon.width);
        let start = leading_width + if leading.is_some() { beside_icon } else { padding };
        let end = trailing_width + if trailing.is_some() { beside_icon } else { padding };
        let loose = constraints.loosen().offset(-start - end, -2 * vertical);
        let label = label.map(|label| scope.measure(label, loose));
        self.label_width.set(label.map_or(0, |label| label.width) as f32);
        // A filled field's text sits below the floated label (16 sp lines); an outlined one's
        // label is on the edge, taking no room.
        let label_room = if self.has_label && !outlined {
            density.sp_to_px(self.floated_label_height).round() as i32
        } else {
            0
        };
        let prefix = prefix.map(|prefix| scope.measure(prefix, loose.offset(0, -label_room)));
        let suffix = suffix.map(|suffix| scope.measure(suffix, loose.offset(0, -label_room)));
        let prefix_width = prefix.map_or(0, |prefix| prefix.width);
        let suffix_width = suffix.map_or(0, |suffix| suffix.width);
        let mut text_constraints = constraints
            .offset(-start - end - prefix_width - suffix_width, -2 * vertical - label_room);
        text_constraints.min_height = 0;
        let text = scope.measure(text, text_constraints);
        let placeholder = placeholder.map(|placeholder| {
            scope.measure(placeholder, loose.offset(-prefix_width - suffix_width, -label_room))
        });
        let content_width = (prefix_width
            + text.width.max(placeholder.map_or(0, |placeholder| placeholder.width))
            + suffix_width)
            .max(label.map_or(0, |label| label.width));
        let content_height = text
            .height
            .max(placeholder.map_or(0, |placeholder| placeholder.height))
            .max(prefix.map_or(0, |prefix| prefix.height))
            .max(suffix.map_or(0, |suffix| suffix.height));
        let icons_height =
            leading.map_or(0, |icon| icon.height).max(trailing.map_or(0, |icon| icon.height));
        let width = constraints.constrain_width(start + content_width + end);
        let height = constraints.constrain_height(
            (vertical + label_room + content_height + vertical).max(icons_height),
        );
        let text_y = if self.has_label && !outlined {
            vertical + label_room
        } else if self.single_line {
            (height - text.height) / 2
        } else {
            vertical
        };
        if let Some(icon) = &leading {
            scope.place_relative(icon, 0, (height - icon.height) / 2);
        }
        if let Some(icon) = &trailing {
            scope.place_relative(icon, width - icon.width, (height - icon.height) / 2);
        }
        if let Some(label) = &label {
            // A single line centered; else at the field's 16 dp padding, as Compose's.
            let resting = if self.single_line {
                (height - label.height) / 2
            } else {
                density.round_to_px(16.dp())
            };
            let floated = if outlined { -label.height / 2 } else { vertical };
            let progress = self.progress.map_or(1.0, |progress| progress.get());
            let y = resting + ((floated - resting) as f32 * progress).round() as i32;
            // An outlined field's floated label is 16 dp in, over a leading icon.
            let x = if outlined {
                start + ((padding - start) as f32 * progress).round() as i32
            } else {
                start
            };
            self.label_start.set(x as f32);
            scope.place_relative(label, x, y);
        }
        // Centered on a single line without a filled field's label, else where the text is.
        let centered = !(self.has_label && !outlined) && self.single_line;
        let y_of = |item_height: i32| if centered { (height - item_height) / 2 } else { text_y };
        if let Some(placeholder) = &placeholder {
            scope.place_relative(placeholder, start + prefix_width, y_of(placeholder.height));
        }
        if let Some(prefix) = &prefix {
            scope.place_relative(prefix, start, y_of(prefix.height));
        }
        if let Some(suffix) = &suffix {
            scope.place_relative(suffix, width - end - suffix.width, y_of(suffix.height));
        }
        scope.place_relative(&text, start + prefix_width, text_y);
        if let Some(indicator) = indicator {
            let indicator = scope.measure(indicator, Constraints::new(width, width, 0, height));
            scope.place_relative(&indicator, 0, height - indicator.height);
        }
        scope.layout(width, height)
    }

    // As Compose's `TextFieldMeasurePolicy` and `OutlinedTextFieldMeasurePolicy`: the slots'
    // intrinsics in the layout's arrangement, without measuring (its measurement keeps the
    // label's place for the outline) (D1207).
    fn min_intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        self.intrinsic_width(scope, measurables, height, false)
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        self.intrinsic_width(scope, measurables, height, true)
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        self.intrinsic_height(scope, measurables, width, false)
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut compose_ui::IntrinsicMeasureScope<'_>,
        measurables: &[compose_ui::IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        self.intrinsic_height(scope, measurables, width, true)
    }

    fn debug_name(&self) -> &'static str {
        "TextFieldDecoration"
    }
}

/// The field, then its supporting text below it, as wide as the field (wrapping within it),
/// as Compose's text field layouts measure it.
struct SupportingLayout;

impl MeasurePolicy for SupportingLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [field, supporting] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let mut field_constraints = constraints;
        field_constraints.min_height = 0;
        let field = scope.measure(*field, field_constraints);
        let supporting =
            scope.measure(*supporting, Constraints::new(0, field.width, 0, constraints.max_height));
        scope.place_relative(&field, 0, 0);
        scope.place_relative(&supporting, 0, field.height);
        let height = constraints.constrain_height(field.height + supporting.height);
        scope.layout(field.width, height)
    }

    fn debug_name(&self) -> &'static str {
        "TextFieldSupporting"
    }
}

/// The outline of `shape` around the node, `thickness` wide, its top edge cut for a label
/// `label_width` wide starting `label_start` in (4 dp each side of it), as Compose's
/// `outlineCutout`: built in `path`, reset first.
fn draw_notched_outline(
    scope: &mut compose_ui::DrawScope<'_>,
    path: &Path,
    shape: &Shape,
    thickness: Dp,
    color: Color,
    label_start: f32,
    label_width: f32,
) {
    let density = scope.density();
    let size = scope.size();
    let stroke = density.to_px(thickness);
    let half = stroke / 2.0;
    let radius = match shape.outline(size, &density) {
        compose_ui::Outline::Rounded(_, radii) => radii[0],
        compose_ui::Outline::Rectangle(_) | compose_ui::Outline::Generic(..) => 0.0,
    };
    let radius = (radius - half).max(0.0);
    let (left, top, right, bottom) = (half, half, size.width - half, size.height - half);
    let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
    let padding = density.to_px(4.dp());
    let start = label_start - padding;
    let cut = if label_width > 0.0 { label_width + 2.0 * padding } else { 0.0 };
    // The cut in left-to-right x, mirrored in a right-to-left layout.
    let (cut_left, cut_right) =
        if rtl { (size.width - start - cut, size.width - start) } else { (start, start + cut) };
    // From the cut's right end, clockwise round to its left end; corners as quarter circles.
    const K: f32 = 0.552_284_8;
    path.reset();
    path.move_to(if cut > 0.0 { cut_right } else { left + radius }, top);
    path.line_to(right - radius, top);
    path.cubic_to(
        right - radius + radius * K,
        top,
        right,
        top + radius - radius * K,
        right,
        top + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + radius * K,
        right - radius + radius * K,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(left + radius, bottom);
    path.cubic_to(
        left + radius - radius * K,
        bottom,
        left,
        bottom - radius + radius * K,
        left,
        bottom - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(
        left,
        top + radius - radius * K,
        left + radius - radius * K,
        top,
        left + radius,
        top,
    );
    if cut > 0.0 {
        path.line_to(cut_left, top);
    } else {
        path.close();
    }
    scope.draw_path(path, Paint::stroke(color, stroke));
}
