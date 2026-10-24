//! Material 3 components: `Text`, `Surface` (and its clickable and selectable overloads),
//! `Checkbox`, the text fields' colors, as Compose's, with their `…Defaults`.

use compose_foundation::layout::Box;
use compose_foundation::{BasicText, BorderStroke};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::ClickableOptions;
use compose_ui::DEFAULT_SHADOW_COLOR;
use compose_ui::{
    Alignment, Color, Dp, DrawModifierNode, DrawScope, FontWeight, Modifier, ModifierNode,
    ModifierNodeElement, MutableInteractionSource, Offset, Paint, Rect, Role, Shape, SharedString,
    Sp, TextStyle, UnitExt,
};

use crate::theme::{LOCAL_CONTENT_COLOR, MaterialTheme};

// ---- Text ------------------------------------------------------------------------------------

/// Text in the theme, as Compose's material3 `Text`: unspecified attributes come from `style`,
/// which defaults to `LocalTextStyle`, and the color from `LocalContentColor`. It's at least
/// `min_lines` lines high; `on_text_layout` hears each new layout (its line count, size and
/// overflow).
#[composable]
pub fn Text(
    #[format] text: SharedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Color::UNSPECIFIED)] color: Color,
    #[default(crate::Sp::UNSPECIFIED)] font_size: Sp,
    #[default(None)] font_style: Option<compose_ui::FontStyle>,
    #[default(None)] font_weight: Option<FontWeight>,
    #[default(None)] font_family: Option<compose_ui::FontFamily>,
    #[default(crate::Sp::UNSPECIFIED)] letter_spacing: Sp,
    #[default(None)] text_decoration: Option<compose_ui::TextDecoration>,
    #[default(None)] text_align: Option<compose_ui::TextAlign>,
    #[default(crate::Sp::UNSPECIFIED)] line_height: Sp,
    #[default(crate::TextOverflow::Clip)] overflow: compose_ui::TextOverflow,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
    #[default(None)] on_text_layout: Option<impl Fn(compose_ui::TextLayoutResult) + 'static>,
    #[default(crate::LOCAL_TEXT_STYLE.current())] style: TextStyle,
) {
    // The parameters over the style, as Compose's `Text`; the color by default the content
    // color.
    let color = color.or(style.color).or(LOCAL_CONTENT_COLOR.current());
    let style = style.merge(&TextStyle {
        color,
        font_size,
        font_weight,
        font_style,
        font_family,
        letter_spacing,
        text_decoration,
        text_align,
        line_height,
    });
    BasicText(text, modifier, style, on_text_layout, overflow, soft_wrap, max_lines, min_lines);
}

/// Styled text, as Compose's `Text(AnnotatedString, …)`: in the theme's text style and content
/// color, each span in its style over them.
#[composable]
pub fn AnnotatedText(
    text: compose_ui::AnnotatedString,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::Color::UNSPECIFIED)] color: Color,
    #[default(crate::TextOverflow::Clip)] overflow: compose_ui::TextOverflow,
    #[default(true)] soft_wrap: bool,
    #[default(u32::MAX)] max_lines: u32,
    #[default(1)] min_lines: u32,
    #[default(None)] on_text_layout: Option<impl Fn(compose_ui::TextLayoutResult) + 'static>,
    #[default(crate::LOCAL_TEXT_STYLE.current())] style: TextStyle,
) {
    let color = color.or(style.color).or(LOCAL_CONTENT_COLOR.current());
    let style = TextStyle { color, ..style };
    compose_foundation::BasicAnnotatedText(
        text,
        modifier,
        style,
        on_text_layout,
        overflow,
        soft_wrap,
        max_lines,
        min_lines,
    );
}

// ---- Surface ---------------------------------------------------------------------------------

/// The modifiers of a surface, in Compose's order (material3's `Modifier.surface`): a layer
/// casting its shadow if it has an elevation (not clipping), its border, its color, then a
/// layer clipping its content to its shape (D526).
pub(crate) fn surface_modifier(
    modifier: Modifier,
    shape: Shape,
    color: Color,
    border: Option<BorderStroke>,
    shadow_elevation: Dp,
) -> Modifier {
    let modifier = modifier.shadow_with(
        shadow_elevation,
        shape,
        false,
        DEFAULT_SHADOW_COLOR,
        DEFAULT_SHADOW_COLOR,
    );
    let modifier = match border {
        Some(border) => modifier.border_with(border.width, border.color, shape),
        None => modifier,
    };
    modifier.background_with(color, shape).clip(shape)
}

/// A themed container, as Compose's `Surface`: its shape and color (the surface color tinted
/// with the primary color by its absolute `tonal_elevation`, as `surfaceColorAtElevation`), its
/// shadow (`shadow_elevation`), and its content's color (by default the theme's content color
/// for `color`).
#[composable]
pub fn Surface(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::RECTANGLE_SHAPE)] shape: Shape,
    #[default(crate::MaterialTheme::color_scheme().surface)] color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::Dp(0.0))] shadow_elevation: Dp,
    #[default(None)] border: Option<BorderStroke>,
    content: impl Fn() + 'static,
) {
    surface(
        modifier,
        shape,
        color,
        content_color,
        tonal_elevation,
        shadow_elevation,
        border,
        None,
        content,
    );
}

/// A clickable surface, as Compose's `Surface(onClick, …)` overload (named apart, Rust having
/// no overloads): at least 48 dp for touch, a state layer in its content color while pressed,
/// focused or hovered, reported to `interaction_source`.
#[composable]
pub fn ClickableSurface(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::RECTANGLE_SHAPE)] shape: Shape,
    #[default(crate::MaterialTheme::color_scheme().surface)] color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::Dp(0.0))] shadow_elevation: Dp,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let interaction_source = remembered_interaction_source(interaction_source);
    let click = SurfaceClick {
        on_click,
        enabled,
        role: None,
        interaction_source,
        selected: None,
        elevations: None,
    };
    surface(
        modifier,
        shape,
        color,
        content_color,
        tonal_elevation,
        shadow_elevation,
        border,
        Some(click),
        content,
    );
}

/// A selectable surface, as Compose's `Surface(selected, onClick, …)` overload: a clickable
/// surface whose semantics say whether it's `selected`.
#[composable]
pub fn SelectableSurface(
    selected: bool,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::RECTANGLE_SHAPE)] shape: Shape,
    #[default(crate::MaterialTheme::color_scheme().surface)] color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::Dp(0.0))] tonal_elevation: Dp,
    #[default(crate::Dp(0.0))] shadow_elevation: Dp,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let interaction_source = remembered_interaction_source(interaction_source);
    let click = SurfaceClick {
        on_click,
        enabled,
        role: None,
        interaction_source,
        selected: Some(selected),
        elevations: None,
    };
    surface(
        modifier,
        shape,
        color,
        content_color,
        tonal_elevation,
        shadow_elevation,
        border,
        Some(click),
        content,
    );
}

/// The given interaction source, or one remembered here (as Compose's `interactionSource ?:
/// remember { MutableInteractionSource() }`; remembered either way, so the slots don't shift).
pub(crate) fn remembered_interaction_source(
    given: Option<MutableInteractionSource>,
) -> MutableInteractionSource {
    let own = compose_runtime::remember(MutableInteractionSource::new);
    given.unwrap_or(own)
}

/// What makes a surface clickable.
pub(crate) struct SurfaceClick {
    pub on_click: compose_runtime::Callback,
    pub enabled: bool,
    pub role: Option<Role>,
    pub interaction_source: MutableInteractionSource,
    /// Whether it's selected, for a selectable surface.
    pub selected: Option<bool>,
    /// The shadow's elevation in each state (in place of the surface's shadow elevation),
    /// following `interaction_source`.
    pub elevations: Option<Elevations>,
}

/// A component's elevation in each interaction state: what Compose's `ButtonElevation`,
/// `CardElevation` and `FloatingActionButtonElevation` have in common.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Elevations {
    pub default: Dp,
    pub pressed: Dp,
    pub focused: Dp,
    pub hovered: Dp,
    pub disabled: Dp,
}

/// A surface's body, as Compose's `Surface` overloads share: the container (clickable with
/// `click`, else taking the pointer from what's beneath), its content color and absolute tonal
/// elevation provided, its content in a box passing its minimum size on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn surface(
    modifier: Modifier,
    shape: Shape,
    color: Color,
    content_color: Color,
    tonal_elevation: Dp,
    shadow_elevation: Dp,
    border: Option<BorderStroke>,
    click: Option<SurfaceClick>,
    content: compose_runtime::Callback,
) {
    let scheme = MaterialTheme::color_scheme();
    let content_color =
        content_color.or(scheme.content_color_for(color)).or(LOCAL_CONTENT_COLOR.current());
    let elevation = Dp(LOCAL_ABSOLUTE_TONAL_ELEVATION.current().0 + tonal_elevation.0);
    let color =
        if color == scheme.surface { scheme.surface_color_at_elevation(elevation) } else { color };
    let modifier = match click {
        // At least the touch target, the state layer in the content color, as Compose's
        // clickable `Surface` (`ripple()` over `LocalContentColor`).
        Some(click) => {
            let modifier = minimum_interactive_component_size(modifier);
            // The shadow follows the press, hover and focus, read here as Compose's components
            // read their elevation's state (`shadowElevation(enabled, interactionSource)`): a
            // change composes the surface again, its layer there only while it has an
            // elevation.
            let shadow_elevation = match click.elevations {
                Some(elevations) => crate::button::interaction_elevation(
                    click.enabled,
                    elevations.default,
                    elevations.pressed,
                    elevations.focused,
                    elevations.hovered,
                    elevations.disabled,
                    &click.interaction_source,
                ),
                None => shadow_elevation,
            };
            let modifier = surface_modifier(modifier, shape, color, border, shadow_elevation)
                .clickable_with(
                    ClickableOptions {
                        enabled: click.enabled,
                        role: click.role,
                        indication: content_color.into(),
                        interaction_source: Some(click.interaction_source),
                    },
                    click.on_click,
                );
            match click.selected {
                Some(selected) => {
                    modifier.semantics(move |config| config.selected = Some(selected))
                }
                None => modifier,
            }
        }
        // A surface takes the pointer: what's beneath it isn't hit, as Compose's (an empty
        // `pointerInput`).
        None => surface_modifier(modifier, shape, color, border, shadow_elevation)
            .then_custom(BlockPointer),
    };
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
        CompositionLocalProvider(LOCAL_ABSOLUTE_TONAL_ELEVATION.provides(elevation), || {
            let content = content.clone();
            Box(
                modifier.clone(),
                Alignment::TOP_START,
                true,
                compose_runtime::callback(move |_| content()),
            );
        });
    });
}

/// The elevation surfaces around add up to, as Compose's `LocalAbsoluteTonalElevation`.
pub static LOCAL_ABSOLUTE_TONAL_ELEVATION: compose_runtime::CompositionLocal<Dp> =
    compose_runtime::CompositionLocal::new("LocalAbsoluteTonalElevation", || Dp(0.0));

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
            // A circle around the box, as Compose's `ripple(bounded = false, 20.dp)`.
            let indication = compose_ui::Indication::unbounded(
                if checked { colors.checked_box_color } else { colors.unchecked_border_color },
                20.dp(),
            );
            let options = ClickableOptions {
                enabled,
                role: Some(Role::Checkbox),
                indication,
                ..ClickableOptions::DEFAULT
            };
            minimum_interactive_component_size(modifier.toggleable_with(
                checked,
                options,
                on_checked_change,
            ))
        }
        None => modifier,
    };
    let modifier = modifier
        .padding(2.dp())
        .required_size((20.dp(), 20.dp()))
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
            // Square ends, as Compose's check mark.
            let check = Paint::stroke_with(
                colors.checked_check_color,
                stroke,
                compose_ui::StrokeCap::Square,
            );
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
    pub error_cursor_color: Color,
    pub error_indicator_color: Color,
    pub error_label_color: Color,
    pub leading_icon_color: Color,
    pub disabled_leading_icon_color: Color,
    pub error_leading_icon_color: Color,
    pub trailing_icon_color: Color,
    pub disabled_trailing_icon_color: Color,
    pub error_trailing_icon_color: Color,
    pub supporting_text_color: Color,
    pub disabled_supporting_text_color: Color,
    pub error_supporting_text_color: Color,
    pub prefix_color: Color,
    pub disabled_prefix_color: Color,
    pub suffix_color: Color,
    pub disabled_suffix_color: Color,
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
            error_cursor_color: scheme.error,
            error_indicator_color: scheme.error,
            error_label_color: scheme.error,
            leading_icon_color: scheme.on_surface_variant,
            disabled_leading_icon_color: scheme.on_surface.with_alpha(0.38),
            error_leading_icon_color: scheme.on_surface_variant,
            trailing_icon_color: scheme.on_surface_variant,
            disabled_trailing_icon_color: scheme.on_surface.with_alpha(0.38),
            error_trailing_icon_color: scheme.error,
            supporting_text_color: scheme.on_surface_variant,
            disabled_supporting_text_color: scheme.on_surface.with_alpha(0.38),
            error_supporting_text_color: scheme.error,
            prefix_color: scheme.on_surface_variant,
            disabled_prefix_color: scheme.on_surface.with_alpha(0.38),
            suffix_color: scheme.on_surface_variant,
            disabled_suffix_color: scheme.on_surface.with_alpha(0.38),
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
