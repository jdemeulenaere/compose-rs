//! Material 3 components: `Text`, `Surface`, the buttons, `Checkbox`, `TextField`, as
//! Compose's, with their `…Defaults`.

use compose_foundation::layout::{Arrangement, Box, PaddingValues, Row, RowScope};
use compose_foundation::{BasicText, BasicTextField, BorderStroke};
use compose_runtime::{Callback, CompositionLocalProvider, composable, mutable_state_of, remember};
use compose_ui::{
    Alignment, Color, Dp, DrawModifierNode, DrawScope, FontWeight, Modifier, ModifierNode,
    ModifierNodeElement, Offset, Paint, Rect, Role, Shape, SharedString, SizeSpec, Sp, TextStyle,
    UnitExt,
};

use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

// ---- Text ------------------------------------------------------------------------------------

/// Text in the theme, as Compose's material3 `Text`: unspecified attributes come from `style`,
/// which defaults to `LocalTextStyle`, and the color from `LocalContentColor`.
#[composable]
pub fn Text(
    #[format] text: SharedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Color::UNSPECIFIED)] color: Color,
    #[default(crate::Sp::UNSPECIFIED)] font_size: Sp,
    #[default(None)] font_weight: Option<FontWeight>,
    #[default(crate::Sp::UNSPECIFIED)] line_height: Sp,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(crate::LOCAL_TEXT_STYLE.current())] style: TextStyle,
) {
    let color = color.or(style.color).or(LOCAL_CONTENT_COLOR.current());
    let style = TextStyle {
        color,
        font_size: if font_size.is_specified() { font_size } else { style.font_size },
        font_weight: font_weight.unwrap_or(style.font_weight),
        line_height: if line_height.is_specified() { line_height } else { style.line_height },
    };
    BasicText(text, modifier, style, soft_wrap, max_lines);
}

// ---- Surface ---------------------------------------------------------------------------------

/// The modifiers of a surface: clipped to its shape, filled with its color, bordered.
fn surface_modifier(
    modifier: Modifier,
    shape: Shape,
    color: Color,
    border: Option<BorderStroke>,
) -> Modifier {
    let modifier = modifier.background_shape(color, shape);
    let modifier = match border {
        Some(border) => modifier.border(border.width, border.color, shape),
        None => modifier,
    };
    modifier.clip(shape)
}

/// A themed container, as Compose's `Surface`: its shape and color, and its content's color
/// (by default the theme's content color for `color`).
#[composable]
pub fn Surface(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::RECTANGLE_SHAPE)] shape: Shape,
    #[default(crate::MaterialTheme::color_scheme().surface)] color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(None)] border: Option<BorderStroke>,
    content: impl Fn() + 'static,
) {
    let content_color = content_color
        .or(MaterialTheme::color_scheme().content_color_for(color))
        .or(LOCAL_CONTENT_COLOR.current());
    // A surface takes the pointer: what's beneath it isn't hit, as Compose's (an empty
    // `pointerInput`).
    let modifier = surface_modifier(modifier, shape, color, border).then_custom(BlockPointer);
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
        Box(modifier, Alignment::TOP_START, true, compose_runtime::callback(move |_| content()));
    });
}

// ---- Pointer helpers --------------------------------------------------------------------------

/// A pointer input node that does nothing: its node is hit, so siblings beneath aren't.
struct BlockPointer;

struct BlockPointerNode;

impl ModifierNodeElement for BlockPointer {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(BlockPointerNode)
    }
    fn update(&self, _: &mut dyn ModifierNode) {}
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<BlockPointer>().is_some()
    }
    fn name(&self) -> &'static str {
        "block_pointer"
    }
}

impl ModifierNode for BlockPointerNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn compose_ui::PointerInputModifierNode> {
        Some(self)
    }
}

impl compose_ui::PointerInputModifierNode for BlockPointerNode {
    fn on_pointer_event(
        &mut self,
        _: &mut compose_ui::PointerEvent,
        _: compose_ui::PointerEventPass,
        _: compose_ui::IntSize,
        _: &mut compose_ui::EventContext<'_>,
    ) {
    }
}

// ---- Minimum touch target --------------------------------------------------------------------

/// The minimum size of an interactive element, as Compose's `minimumInteractiveComponentSize`.
pub const MINIMUM_INTERACTIVE_SIZE: Dp = Dp(48.0);

/// Lays its content out at least `MINIMUM_INTERACTIVE_SIZE` in both directions, centered, so
/// small controls are easy to hit, as Compose's `minimumInteractiveComponentSize()`.
pub fn minimum_interactive_component_size(modifier: Modifier) -> Modifier {
    modifier.minimum_touch_target(MINIMUM_INTERACTIVE_SIZE)
}

// ---- Buttons ---------------------------------------------------------------------------------

/// A button's colors, enabled and disabled, as Compose's `ButtonColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ButtonColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
}

impl compose_runtime::ComposeArg for ButtonColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `ButtonDefaults`.
pub struct ButtonDefaults {}

impl ButtonDefaults {
    pub const CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(24.0), Dp(8.0));
    pub const TEXT_BUTTON_CONTENT_PADDING: PaddingValues =
        PaddingValues::symmetric(Dp(12.0), Dp(8.0));
    pub const MIN_WIDTH: Dp = Dp(58.0);
    pub const MIN_HEIGHT: Dp = Dp(40.0);

    /// A fully rounded pill, as `ButtonDefaults.shape`.
    pub fn shape() -> Shape {
        compose_foundation::CIRCLE_SHAPE
    }

    pub fn button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: scheme.primary,
            content_color: scheme.on_primary,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// Transparent, primary text: `outlinedButtonColors` and `textButtonColors`.
    pub fn text_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: scheme.primary,
            disabled_container_color: Color::TRANSPARENT,
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    pub fn outlined_button_border(enabled: bool) -> BorderStroke {
        let scheme = MaterialTheme::color_scheme();
        BorderStroke::new(
            Dp(1.0),
            if enabled { scheme.outline } else { scheme.on_surface.with_alpha(0.12) },
        )
    }
}

/// A filled button, as Compose's `Button`: a clickable surface around a row of content in the
/// label style.
#[composable]
pub fn Button(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::ButtonDefaults::shape())] shape: Shape,
    #[default(crate::ButtonDefaults::button_colors())] colors: ButtonColors,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::ButtonDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    content: impl Fn(RowScope) + 'static,
) {
    let (container, content_color) = if enabled {
        (colors.container_color, colors.content_color)
    } else {
        (colors.disabled_container_color, colors.disabled_content_color)
    };
    let surface =
        surface_modifier(minimum_interactive_component_size(modifier), shape, container, border)
            .clickable_indicated(enabled, Some(Role::Button), content_color, on_click);
    let label = MaterialTheme::typography().label_large;
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(label), || {
            Box(
                surface,
                Alignment::TOP_START,
                true,
                compose_runtime::callback(move |_| {
                    Row(
                        Modifier
                            .size_in(SizeSpec::bounds(
                                ButtonDefaults::MIN_WIDTH,
                                ButtonDefaults::MIN_HEIGHT,
                                Dp::UNSPECIFIED,
                                Dp::UNSPECIFIED,
                            ))
                            .padding_values(content_padding),
                        Arrangement::Center,
                        Alignment::CENTER_VERTICALLY,
                        content.clone(),
                    );
                }),
            );
        });
    });
}

/// A button with an outline and no fill, as Compose's `OutlinedButton`.
#[composable]
pub fn OutlinedButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    content: impl Fn(RowScope) + 'static,
) {
    let border = ButtonDefaults::outlined_button_border(enabled);
    Button(
        on_click,
        modifier,
        enabled,
        ButtonDefaults::shape(),
        ButtonDefaults::text_button_colors(),
        Some(border),
        ButtonDefaults::CONTENT_PADDING,
        content,
    );
}

/// A button of text alone, as Compose's `TextButton`.
#[composable]
pub fn TextButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    content: impl Fn(RowScope) + 'static,
) {
    Button(
        on_click,
        modifier,
        enabled,
        ButtonDefaults::shape(),
        ButtonDefaults::text_button_colors(),
        None,
        ButtonDefaults::TEXT_BUTTON_CONTENT_PADDING,
        content,
    );
}

// ---- Checkbox --------------------------------------------------------------------------------

/// A checkbox's colors, as Compose's `CheckboxColors` (the enabled ones).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CheckboxColors {
    pub checked_box_color: Color,
    pub checked_check_color: Color,
    pub unchecked_border_color: Color,
    pub disabled_color: Color,
}

impl compose_runtime::ComposeArg for CheckboxColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

pub struct CheckboxDefaults {}

impl CheckboxDefaults {
    pub fn colors() -> CheckboxColors {
        let scheme = MaterialTheme::color_scheme();
        CheckboxColors {
            checked_box_color: scheme.primary,
            checked_check_color: scheme.on_primary,
            unchecked_border_color: scheme.on_surface_variant,
            disabled_color: scheme.on_surface.with_alpha(0.38),
        }
    }
}

/// A checkbox, as Compose's: a 20 dp box in a 48 dp touch target, toggled by a click when
/// `on_checked_change` is given.
#[composable]
pub fn Checkbox(
    checked: bool,
    on_checked_change: Option<impl Fn(bool) + 'static>,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::CheckboxDefaults::colors())] colors: CheckboxColors,
) {
    // As Compose's: the touch target only with a callback, then a 20 dp box whatever the size.
    let modifier = match on_checked_change {
        Some(on_checked_change) => {
            minimum_interactive_component_size(modifier.toggleable_indicated(
                checked,
                enabled,
                Some(Role::Checkbox),
                if checked { colors.checked_box_color } else { colors.unchecked_border_color },
                on_checked_change,
            ))
        }
        None => modifier,
    };
    let modifier = modifier
        .padding(2.dp())
        .required_size(20.dp(), 20.dp())
        .then_custom(CheckboxElement { checked, enabled, colors });
    compose_foundation::layout::Spacer(modifier);
}

struct CheckboxElement {
    checked: bool,
    enabled: bool,
    colors: CheckboxColors,
}

impl ModifierNodeElement for CheckboxElement {
    fn create(&self) -> std::boxed::Box<dyn ModifierNode> {
        std::boxed::Box::new(CheckboxNode {
            checked: self.checked,
            enabled: self.enabled,
            colors: self.colors,
        })
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<CheckboxNode>() {
            *node =
                CheckboxNode { checked: self.checked, enabled: self.enabled, colors: self.colors };
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<CheckboxElement>().is_some_and(|other| {
            other.checked == self.checked
                && other.enabled == self.enabled
                && other.colors == self.colors
        })
    }
    fn name(&self) -> &'static str {
        "checkbox"
    }
}

struct CheckboxNode {
    checked: bool,
    enabled: bool,
    colors: CheckboxColors,
}

impl ModifierNode for CheckboxNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for CheckboxNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let density = scope.density();
        let stroke = density.to_px(2.dp());
        let radius = density.to_px(2.dp());
        let colors = self.colors;
        let box_rect = Rect::new(0.0, 0.0, size.width, size.height);
        if self.checked {
            let fill = if self.enabled { colors.checked_box_color } else { colors.disabled_color };
            scope.draw_round_rect(box_rect, [radius; 4], Paint::fill(fill));
            let check = Paint::stroke(colors.checked_check_color, stroke);
            let point = |x: f32, y: f32| Offset::new(size.width * x, size.height * y);
            scope.draw_line(point(0.2, 0.5), point(0.4, 0.7), check);
            scope.draw_line(point(0.4, 0.7), point(0.8, 0.3), check);
        } else {
            let border =
                if self.enabled { colors.unchecked_border_color } else { colors.disabled_color };
            let half = stroke / 2.0;
            let inset = Rect::new(half, half, size.width - half, size.height - half);
            scope.draw_round_rect(
                inset,
                [(radius - half).max(0.0); 4],
                Paint::stroke(border, stroke),
            );
        }
    }
}

// ---- TextField -------------------------------------------------------------------------------

/// A text field's colors, as Compose's `TextFieldColors` (the roles used).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextFieldColors {
    pub container_color: Color,
    pub text_color: Color,
    pub cursor_color: Color,
    pub focused_indicator_color: Color,
    pub unfocused_indicator_color: Color,
    pub label_color: Color,
    pub focused_label_color: Color,
    pub placeholder_color: Color,
    pub disabled_text_color: Color,
    pub disabled_container_color: Color,
    pub disabled_indicator_color: Color,
    pub disabled_label_color: Color,
    pub disabled_placeholder_color: Color,
}

impl compose_runtime::ComposeArg for TextFieldColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

pub struct TextFieldDefaults {}

impl TextFieldDefaults {
    pub const MIN_WIDTH: Dp = Dp(280.0);
    pub const MIN_HEIGHT: Dp = Dp(56.0);

    pub fn colors() -> TextFieldColors {
        let scheme = MaterialTheme::color_scheme();
        TextFieldColors {
            container_color: scheme.surface_container_highest,
            text_color: scheme.on_surface,
            cursor_color: scheme.primary,
            focused_indicator_color: scheme.primary,
            unfocused_indicator_color: scheme.on_surface_variant,
            label_color: scheme.on_surface_variant,
            focused_label_color: scheme.primary,
            placeholder_color: scheme.on_surface_variant,
            disabled_text_color: scheme.on_surface.with_alpha(0.38),
            disabled_container_color: scheme.on_surface.with_alpha(0.04),
            disabled_indicator_color: scheme.on_surface.with_alpha(0.38),
            disabled_label_color: scheme.on_surface.with_alpha(0.38),
            disabled_placeholder_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// The filled field's shape: extra small, rounded at the top only.
    pub fn shape() -> Shape {
        let radius = match MaterialTheme::shapes().extra_small {
            Shape::Rounded(corners) => corners[0],
            Shape::Rectangle => compose_ui::CornerSize::Dp(Dp(0.0)),
        };
        Shape::Rounded([
            radius,
            radius,
            compose_ui::CornerSize::Dp(Dp(0.0)),
            compose_ui::CornerSize::Dp(Dp(0.0)),
        ])
    }
}

/// The layout of a filled text field's decoration, as Compose's `TextFieldMeasurePolicy`: the
/// label (if any), the placeholder (if shown) and the text field, then the indicator line, as
/// children in that order. 16 dp at the sides; 8 dp above and below with a label, 16 dp
/// without. The width is the widest of them and the minimum (280 dp, the container's); the label
/// sits at the top padding with the text below it; without one a single line is centered and a
/// multi-line text starts at the top padding.
struct TextFieldLayout {
    has_label: bool,
    has_placeholder: bool,
    single_line: bool,
}

impl compose_ui::MeasurePolicy for TextFieldLayout {
    fn measure(
        &self,
        scope: &mut compose_ui::MeasureScope<'_>,
        measurables: &[compose_ui::Measurable],
        constraints: compose_ui::Constraints,
    ) -> compose_ui::MeasureResult {
        let density = scope.density();
        let horizontal = density.round_to_px(16.dp());
        let vertical = density.round_to_px(if self.has_label { 8.dp() } else { 16.dp() });
        let mut children = measurables.iter().copied();
        let label = if self.has_label { children.next() } else { None };
        let placeholder = if self.has_placeholder { children.next() } else { None };
        let (Some(text), Some(indicator)) = (children.next(), children.next()) else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let loose = constraints.loosen().offset(-2 * horizontal, -2 * vertical);
        let label = label.map(|label| scope.measure(label, loose));
        let label_height = label.map_or(0, |label| label.height);
        let mut text_constraints =
            constraints.offset(-2 * horizontal, -2 * vertical - label_height);
        text_constraints.min_height = 0;
        let text = scope.measure(text, text_constraints);
        let placeholder = placeholder
            .map(|placeholder| scope.measure(placeholder, loose.offset(0, -label_height)));
        let content_width = text
            .width
            .max(label.map_or(0, |label| label.width))
            .max(placeholder.map_or(0, |placeholder| placeholder.width));
        let content_height =
            text.height.max(placeholder.map_or(0, |placeholder| placeholder.height));
        let width = constraints.constrain_width(content_width + 2 * horizontal);
        let height =
            constraints.constrain_height(vertical + label_height + content_height + vertical);
        let indicator =
            scope.measure(indicator, compose_ui::Constraints::new(width, width, 0, height));
        let text_y = if self.has_label {
            vertical + label_height
        } else if self.single_line {
            (height - text.height) / 2
        } else {
            vertical
        };
        if let Some(label) = &label {
            scope.place(label, horizontal, vertical);
        }
        if let Some(placeholder) = &placeholder {
            let y = if self.has_label || !self.single_line {
                text_y
            } else {
                (height - placeholder.height) / 2
            };
            scope.place(placeholder, horizontal, y);
        }
        scope.place(&text, horizontal, text_y);
        scope.place(&indicator, 0, height - indicator.height);
        scope.layout(width, height)
    }
}

/// A filled text field, as Compose's `TextField`: a container with an indicator line, an
/// optional label above the text (always floated, without the animation) and a placeholder
/// while empty.
#[composable]
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
    let focused = remember(|| mutable_state_of(false));
    let is_focused = focused.get();
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
    let container_color =
        if enabled { colors.container_color } else { colors.disabled_container_color };
    let placeholder_color =
        if enabled { colors.placeholder_color } else { colors.disabled_placeholder_color };
    let empty = value.is_empty();
    let has_label = label.is_some();
    // The container, label and placeholder are the field's decoration, inside the text field
    // (its semantics node, its taps), as Compose's `TextField` decoration box.
    let decoration = move |inner_text_field: Callback| {
        let container = Modifier
            .size_in(SizeSpec::bounds(
                TextFieldDefaults::MIN_WIDTH,
                TextFieldDefaults::MIN_HEIGHT,
                Dp::UNSPECIFIED,
                Dp::UNSPECIFIED,
            ))
            .background_shape(container_color, TextFieldDefaults::shape())
            .on_focus_changed(move |now| focused.set(now));
        let showing_placeholder = empty && placeholder.is_some();
        let policy =
            compose_runtime::remember_keyed((has_label, showing_placeholder, single_line), || {
                std::rc::Rc::new(TextFieldLayout {
                    has_label,
                    has_placeholder: showing_placeholder,
                    single_line,
                }) as std::rc::Rc<dyn compose_ui::MeasurePolicy>
            });
        let content = compose_runtime::callback(move || {
            if let Some(label) = label.clone() {
                Box!({
                    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(label_color), || {
                        CompositionLocalProvider(
                            LOCAL_TEXT_STYLE.provides(typography.body_small),
                            || label(),
                        );
                    });
                });
            }
            if empty && let Some(placeholder) = placeholder.clone() {
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
            // The indicator line: 1 dp, 2 dp focused.
            let thickness = if is_focused { 2.dp() } else { 1.dp() };
            compose_foundation::layout::Spacer(Modifier.height(thickness).background(indicator));
        });
        compose_ui::Layout(container, policy, content);
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
