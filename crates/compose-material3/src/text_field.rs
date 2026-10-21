//! `TextField` and `OutlinedTextField`, as Compose's Material 3: a `BasicTextField` decorated
//! with a container (filled) or an outline, a label that rests in the field and floats up
//! (animated) when it's focused or has text, and a placeholder.

use std::rc::Rc;

use compose_animation::{animate_float_as_state, tween};
use compose_foundation::BasicTextField;
use compose_foundation::layout::Box;
use compose_runtime::{Callback, CompositionLocalProvider, composable, mutable_state_of, remember};
use compose_ui::{
    Color, Constraints, Dp, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier,
    Paint, Path, Shape, SizeSpec, TextStyle, UnitExt,
};

use crate::components::{TextFieldColors, TextFieldDefaults};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// As Compose's `TextFieldAnimationDuration`: the label's float.
const LABEL_ANIMATION_MILLIS: u32 = 150;

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
            disabled_container_color: Color::TRANSPARENT,
            unfocused_indicator_color: scheme.outline,
            disabled_indicator_color: scheme.on_surface.with_alpha(0.12),
            ..TextFieldDefaults::colors()
        }
    }

    /// The outline's shape, as `OutlinedTextFieldDefaults.shape` (extra small).
    pub fn shape() -> Shape {
        MaterialTheme::shapes().extra_small
    }
}

/// A filled text field, as Compose's `TextField`: a container with an indicator line (2 dp
/// focused), a label resting in the field that floats above the text when focused or filled,
/// and a placeholder while focused and empty.
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
    #[default(false)] single_line: bool,
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
        label.map(compose_runtime::callback),
        placeholder.map(compose_runtime::callback),
        single_line,
        colors,
    );
}

/// An outlined text field, as Compose's `OutlinedTextField`: an outline (2 dp focused) whose
/// top edge the floated label sits on, cut around it, 8 dp above the field for it.
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
    #[default(false)] single_line: bool,
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
        label.map(compose_runtime::callback),
        placeholder.map(compose_runtime::callback),
        single_line,
        colors,
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    Filled,
    Outlined,
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
    label: Option<Callback>,
    placeholder: Option<Callback>,
    single_line: bool,
    colors: TextFieldColors,
) {
    let focused = remember(|| mutable_state_of(false));
    let is_focused = focused.get();
    let empty = value.is_empty();
    // 0 resting in the field, 1 floated, as Compose's label animation.
    let floated = is_focused || !empty;
    let progress = animate_float_as_state(
        if floated { 1.0 } else { 0.0 },
        tween(LABEL_ANIMATION_MILLIS),
        0.01,
        "LabelProgress",
        None,
    )
    .get();
    let indicator = match (enabled, is_focused) {
        (false, _) => colors.disabled_indicator_color,
        (true, true) => colors.focused_indicator_color,
        (true, false) => colors.unfocused_indicator_color,
    };
    let text_color = if enabled { colors.text_color } else { colors.disabled_text_color };
    let style = TextStyle { color: text_style.color.or(text_color), ..text_style };
    let typography = MaterialTheme::typography();
    let label_color = match (enabled, is_focused) {
        (false, _) => colors.disabled_label_color,
        (true, true) => colors.focused_label_color,
        (true, false) => colors.label_color,
    };
    // The label's style, from the body-large text's to the small one's as it floats.
    let label_style = lerp_style(&typography.body_large, &typography.body_small, progress);
    let container_color =
        if enabled { colors.container_color } else { colors.disabled_container_color };
    let placeholder_color =
        if enabled { colors.placeholder_color } else { colors.disabled_placeholder_color };
    let has_label = label.is_some();
    // Shown while the field is empty and its label floated (or it has none), as Compose's.
    let show_placeholder = empty && placeholder.is_some() && (!has_label || is_focused);
    let floated_label_height = typography.body_small.line_height;
    // Remembered: the layout (remembered too) writes it, the outline reads it.
    let label_width = remember(|| Rc::new(std::cell::Cell::new(0.0f32)));
    let decoration = {
        let label_width = label_width.clone();
        move |inner_text_field: Callback| {
            let mut container = Modifier
                .size_in(SizeSpec::bounds(
                    TextFieldDefaults::MIN_WIDTH,
                    TextFieldDefaults::MIN_HEIGHT,
                    Dp::UNSPECIFIED,
                    Dp::UNSPECIFIED,
                ))
                .on_focus_changed(move |now| focused.set(now));
            container = match variant {
                Variant::Filled => {
                    container.background_shape(container_color, TextFieldDefaults::shape())
                }
                Variant::Outlined => {
                    let thickness = if is_focused {
                        OutlinedTextFieldDefaults::FOCUSED_BORDER_THICKNESS
                    } else {
                        OutlinedTextFieldDefaults::UNFOCUSED_BORDER_THICKNESS
                    };
                    let shape = OutlinedTextFieldDefaults::shape();
                    let label_width = label_width.clone();
                    container.background_shape(container_color, shape).draw_behind(move |scope| {
                        draw_notched_outline(
                            scope,
                            shape,
                            thickness,
                            indicator,
                            if has_label { label_width.get() * progress } else { 0.0 },
                        )
                    })
                }
            };
            let policy = compose_runtime::remember_keyed(
                LayoutKey(variant, has_label, show_placeholder, single_line, progress.to_bits()),
                || {
                    Rc::new(DecorationLayout {
                        variant,
                        has_label,
                        has_placeholder: show_placeholder,
                        single_line,
                        progress,
                        floated_label_height,
                        label_width: label_width.clone(),
                    }) as Rc<dyn MeasurePolicy>
                },
            );
            let (label, placeholder) = (label.clone(), placeholder.clone());
            let content = compose_runtime::callback(move || {
                if let Some(label) = label.clone() {
                    Box!({
                        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(label_color), || {
                            CompositionLocalProvider(
                                LOCAL_TEXT_STYLE.provides(label_style),
                                &*label,
                            );
                        });
                    });
                }
                if show_placeholder && let Some(placeholder) = placeholder.clone() {
                    Box!({
                        CompositionLocalProvider(
                            LOCAL_CONTENT_COLOR.provides(placeholder_color),
                            || {
                                CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), || {
                                    placeholder()
                                });
                            },
                        );
                    });
                }
                inner_text_field();
                if variant == Variant::Filled {
                    // The indicator line: 1 dp, 2 dp focused.
                    let thickness = if is_focused { 2.dp() } else { 1.dp() };
                    compose_foundation::layout::Spacer(
                        Modifier.height(thickness).background(indicator),
                    );
                }
            });
            compose_ui::Layout(container, policy, content);
        }
    };
    // Room above an outlined field for its floated label, as `OutlinedTextFieldTopPadding`.
    let modifier = match variant {
        Variant::Outlined if has_label => modifier.padding_sides(0.dp(), 8.dp(), 0.dp(), 0.dp()),
        _ => modifier,
    };
    BasicTextField(
        value,
        on_value_change,
        modifier,
        enabled,
        read_only,
        style,
        single_line,
        colors.cursor_color,
        compose_runtime::callback(decoration),
    );
}

/// `from`'s size and line height going to `to`'s by `fraction`, the rest `to`'s.
fn lerp_style(from: &TextStyle, to: &TextStyle, fraction: f32) -> TextStyle {
    let lerp = |a: f32, b: f32| a + (b - a) * fraction;
    TextStyle {
        font_size: compose_ui::Sp(lerp(from.font_size.0, to.font_size.0)),
        line_height: compose_ui::Sp(lerp(from.line_height.0, to.line_height.0)),
        letter_spacing: compose_ui::Sp(lerp(from.letter_spacing.0, to.letter_spacing.0)),
        ..*to
    }
}

#[derive(Clone, Copy, PartialEq)]
struct LayoutKey(Variant, bool, bool, bool, u32);

impl compose_runtime::ComposeArg for LayoutKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The decoration's layout, as Compose's `TextFieldMeasurePolicy` and
/// `OutlinedTextFieldMeasurePolicy`: the label (if any), the placeholder (if shown), the text
/// field, then (filled) the indicator line. 16 dp at the sides; a filled field 8 dp above and
/// below with a label (16 dp without), its text below where the floated label goes; an
/// outlined field 16 dp above and below, its floated label centered on the top edge. The label
/// rests where a single line of text would be (or at the top padding) and moves to its floated
/// place with `progress`.
struct DecorationLayout {
    variant: Variant,
    has_label: bool,
    has_placeholder: bool,
    single_line: bool,
    progress: f32,
    floated_label_height: compose_ui::Sp,
    /// The label's width, for the outline's cut.
    label_width: Rc<std::cell::Cell<f32>>,
}

impl MeasurePolicy for DecorationLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let horizontal = density.round_to_px(16.dp());
        let outlined = self.variant == Variant::Outlined;
        let vertical =
            density.round_to_px(if self.has_label && !outlined { 8.dp() } else { 16.dp() });
        let mut children = measurables.iter().copied();
        let label = if self.has_label { children.next() } else { None };
        let placeholder = if self.has_placeholder { children.next() } else { None };
        let Some(text) = children.next() else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let indicator = children.next();
        let loose = constraints.loosen().offset(-2 * horizontal, -2 * vertical);
        let label = label.map(|label| scope.measure(label, loose));
        self.label_width.set(label.map_or(0, |label| label.width) as f32);
        // A filled field's text sits below the floated label (16 sp lines); an outlined one's
        // label is on the edge, taking no room.
        let label_room = if self.has_label && !outlined {
            density.sp_to_px(self.floated_label_height).round() as i32
        } else {
            0
        };
        let mut text_constraints = constraints.offset(-2 * horizontal, -2 * vertical - label_room);
        text_constraints.min_height = 0;
        let text = scope.measure(text, text_constraints);
        let placeholder =
            placeholder.map(|placeholder| scope.measure(placeholder, loose.offset(0, -label_room)));
        let content_width = text
            .width
            .max(label.map_or(0, |label| label.width))
            .max(placeholder.map_or(0, |placeholder| placeholder.width));
        let content_height =
            text.height.max(placeholder.map_or(0, |placeholder| placeholder.height));
        let width = constraints.constrain_width(content_width + 2 * horizontal);
        let height =
            constraints.constrain_height(vertical + label_room + content_height + vertical);
        let text_y = if self.has_label && !outlined {
            vertical + label_room
        } else if self.single_line {
            (height - text.height) / 2
        } else {
            vertical
        };
        if let Some(label) = &label {
            // A single line centered; else at the field's 16 dp padding, as Compose's.
            let resting = if self.single_line {
                (height - label.height) / 2
            } else {
                density.round_to_px(16.dp())
            };
            let floated = if outlined { -label.height / 2 } else { vertical };
            let y = resting + ((floated - resting) as f32 * self.progress).round() as i32;
            scope.place_relative(label, horizontal, y);
        }
        if let Some(placeholder) = &placeholder {
            let y = if (self.has_label && !outlined) || !self.single_line {
                text_y
            } else {
                (height - placeholder.height) / 2
            };
            scope.place_relative(placeholder, horizontal, y);
        }
        scope.place_relative(&text, horizontal, text_y);
        if let Some(indicator) = indicator {
            let indicator = scope.measure(indicator, Constraints::new(width, width, 0, height));
            scope.place_relative(&indicator, 0, height - indicator.height);
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "TextFieldDecoration"
    }
}

/// The outline of `shape` around the node, `thickness` wide, its top edge cut for a label
/// `label_width` wide (4 dp each side of it, from 16 dp in), as Compose's `outlineCutout`.
fn draw_notched_outline(
    scope: &mut compose_ui::DrawScope<'_>,
    shape: Shape,
    thickness: Dp,
    color: Color,
    label_width: f32,
) {
    let density = scope.density();
    let size = scope.size();
    let stroke = density.to_px(thickness);
    let half = stroke / 2.0;
    let radius = match shape.outline(size, &density) {
        compose_ui::Outline::Rounded(_, radii) => radii[0],
        compose_ui::Outline::Rectangle(_) => 0.0,
    };
    let radius = (radius - half).max(0.0);
    let (left, top, right, bottom) = (half, half, size.width - half, size.height - half);
    let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
    let padding = density.to_px(4.dp());
    let start = density.to_px(16.dp()) - padding;
    let cut = if label_width > 0.0 { label_width + 2.0 * padding } else { 0.0 };
    // The cut in left-to-right x, mirrored in a right-to-left layout.
    let (cut_left, cut_right) =
        if rtl { (size.width - start - cut, size.width - start) } else { (start, start + cut) };
    // From the cut's right end, clockwise round to its left end; corners as quarter circles.
    const K: f32 = 0.552_284_8;
    let mut path = Path::new();
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
    scope.draw_path(&std::sync::Arc::new(path), Paint::stroke(color, stroke));
}
