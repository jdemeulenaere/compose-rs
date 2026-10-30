//! Material 3 components: `Text`, `Surface` (and its clickable and selectable overloads),
//! The text fields' colors, as Compose's, with their `…Defaults`.

use compose_foundation::layout::Box;
use compose_foundation::{BasicText, BorderStroke};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::ClickableOptions;
use compose_ui::DEFAULT_SHADOW_COLOR;
use compose_ui::{
    Alignment, Color, Dp, FontWeight, Modifier, ModifierNode, ModifierNodeElement,
    MutableInteractionSource, Role, Shape, SharedString, TextStyle, TextUnit,
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
    #[default(crate::TextUnit::UNSPECIFIED)] font_size: TextUnit,
    #[default(None)] font_style: Option<compose_ui::FontStyle>,
    #[default(None)] font_weight: Option<FontWeight>,
    #[default(None)] font_family: Option<compose_ui::FontFamily>,
    #[default(crate::TextUnit::UNSPECIFIED)] letter_spacing: TextUnit,
    #[default(None)] text_decoration: Option<compose_ui::TextDecoration>,
    #[default(None)] text_align: Option<compose_ui::TextAlign>,
    #[default(crate::TextUnit::UNSPECIFIED)] line_height: TextUnit,
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
        ..TextStyle::DEFAULT
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
        shape.clone(),
        false,
        DEFAULT_SHADOW_COLOR,
        DEFAULT_SHADOW_COLOR,
    );
    let modifier = match border {
        Some(border) => modifier.border_stroke(border, shape.clone()),
        None => modifier,
    };
    modifier.background_with(color, shape.clone()).clip(shape)
}

/// A themed container, as Compose's `Surface`: its shape and color (the surface color tinted
/// with the surface tint by its absolute `tonal_elevation`, as `surfaceColorAtElevation`), its
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
        checked: None,
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
        checked: None,
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

/// A toggleable surface, as Compose's `Surface(checked, onCheckedChange, …)` overload: a
/// clickable surface toggling `checked` (`on_checked_change(!checked)`), whose semantics say
/// whether it's on.
#[composable]
pub fn ToggleableSurface(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
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
        on_click: compose_runtime::callback(move || on_checked_change(!checked)),
        enabled,
        role: None,
        interaction_source,
        selected: None,
        checked: Some(checked),
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
    /// Whether it's on, for a toggleable surface.
    pub checked: Option<bool>,
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
    /// A chip's and a card's: the others' have none, this being their default.
    pub dragged: Dp,
    pub disabled: Dp,
}

/// The interaction an elevation follows, as material3's `animateElevation` tells them apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ElevationInteraction {
    Press,
    Hover,
    Focus,
    Drag,
}

/// The elevation of a component in its interaction state, and the interaction going on (`None` at
/// rest or disabled), as `animateElevation`: the one that started last wins
/// (`interactions.lastOrNull()`). Only the kinds whose elevation isn't the default are read, so what
/// doesn't change it isn't composed again: a button's press, where only a hover raises it.
pub(crate) fn elevation_and_interaction(
    elevations: Elevations,
    enabled: bool,
    source: &compose_ui::MutableInteractionSource,
) -> (Dp, Option<ElevationInteraction>) {
    use compose_ui::LatestInteraction as Latest;
    if !enabled {
        return (elevations.disabled, None);
    }
    let default = elevations.default;
    let is = |elevation: Dp, kind| elevation != default && source.is_latest_ongoing(kind);
    let target = if is(elevations.pressed, Latest::Press) {
        elevations.pressed
    } else if is(elevations.hovered, Latest::Hover) {
        elevations.hovered
    } else if is(elevations.focused, Latest::Focus) {
        elevations.focused
    } else if is(elevations.dragged, Latest::Drag) {
        elevations.dragged
    } else {
        default
    };
    let interaction = source.latest_ongoing_now().map(|latest| match latest {
        Latest::Press => ElevationInteraction::Press,
        Latest::Hover => ElevationInteraction::Hover,
        Latest::Focus => ElevationInteraction::Focus,
        Latest::Drag => ElevationInteraction::Drag,
    });
    (target, interaction)
}

/// A component's elevation animated as material3's `animateElevation` does (read from its
/// `ElevationDefaults`): to a new interaction's in 120 ms (`FastOutSlowInEasing`), back from a
/// hover in 120 ms and from a press, a drag or a focus in 150 ms (`CubicBezierEasing(0.4, 0,
/// 0.6, 1)`). At once when disabled, whatever it was doing ("No transition when moving to a disabled
/// state"), and with neither an interaction before nor one now.
fn animated_elevation(
    elevations: Elevations,
    enabled: bool,
    source: &compose_ui::MutableInteractionSource,
) -> Dp {
    use compose_animation::{AnimationSpec, Easing, FAST_OUT_SLOW_IN_EASING, TweenSpec};
    let (target, interaction) = elevation_and_interaction(elevations, enabled, source);
    let last = compose_runtime::remember(|| std::rc::Rc::new(std::cell::Cell::new(None)));
    let previous = last.replace(interaction);
    let tween = |duration_millis, easing| {
        AnimationSpec::Tween(TweenSpec { duration_millis, delay_millis: 0, easing })
    };
    let outgoing = Easing::cubic_bezier(0.4, 0.0, 0.6, 1.0);
    let spec = match (enabled, interaction, previous) {
        (false, _, _) | (true, None, None) => compose_animation::snap(0).into(),
        (true, Some(_), _) => tween(120, FAST_OUT_SLOW_IN_EASING),
        (true, None, Some(ElevationInteraction::Hover)) => tween(120, outgoing),
        (true, None, Some(_)) => tween(150, outgoing),
    };
    crate::lazy_animation::animate_lazily(target, spec, |start: Dp, stop: Dp, fraction| {
        Dp(start.0 + (stop.0 - start.0) * fraction)
    })
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
    let color = scheme.apply_tonal_elevation(color, elevation);
    let modifier = match click {
        // At least the touch target, the ripple in the content color, as Compose's clickable
        // `Surface` (`ripple()` over `LocalContentColor`).
        Some(click) => {
            let modifier = minimum_interactive_component_size(modifier);
            // The shadow follows the press, hover and focus, animated and read here as Compose's
            // components read their elevation's state (`shadowElevation(enabled,
            // interactionSource)`, `animateElevation`): a change composes the surface again
            // each frame of its animation, its layer there only while it has an elevation.
            let shadow_elevation = match click.elevations {
                Some(elevations) => {
                    animated_elevation(elevations, click.enabled, &click.interaction_source)
                }
                None => shadow_elevation,
            };
            let ripple = crate::ripple::shaped_ripple(shape.clone());
            let modifier = surface_modifier(modifier, shape, color, border, shadow_elevation)
                .clickable_with(
                    ClickableOptions {
                        enabled: click.enabled,
                        role: click.role,
                        indication: ripple,
                        interaction_source: Some(click.interaction_source),
                        ..ClickableOptions::DEFAULT
                    },
                    click.on_click,
                )
                .then_custom(crate::parent_semantics::ChildSemantics);
            let modifier = match click.selected {
                Some(selected) => modifier.semantics(move |config| config.set_selected(selected)),
                None => modifier,
            };
            match click.checked {
                Some(checked) => {
                    modifier.semantics(move |config| config.set_toggleable_state(checked.into()))
                }
                None => modifier,
            }
        }
        // A surface takes the pointer: what's beneath it isn't hit, as Compose's (an empty
        // `pointerInput`).
        None => surface_modifier(modifier, shape, color, border, shadow_elevation)
            .then_custom(BlockPointer),
    };
    CompositionLocalProvider(
        (
            LOCAL_CONTENT_COLOR.provides(content_color),
            LOCAL_ABSOLUTE_TONAL_ELEVATION.provides(elevation),
        ),
        || {
            let content = content.clone();
            Box(
                modifier.clone(),
                Alignment::TOP_START,
                true,
                compose_runtime::callback(move |_| content()),
            );
        },
    );
}

/// The elevation surfaces around add up to, as Compose's `LocalAbsoluteTonalElevation`.
pub static LOCAL_ABSOLUTE_TONAL_ELEVATION: compose_runtime::CompositionLocal<Dp> =
    compose_runtime::CompositionLocal::new("LocalAbsoluteTonalElevation", || Dp(0.0));

// ---- Pointer helpers --------------------------------------------------------------------------

/// A pointer input node that does nothing: its node is hit, so siblings beneath aren't.
struct BlockPointer;

struct BlockPointerNode;

impl ModifierNodeElement for BlockPointer {
    type Node = BlockPointerNode;
    fn create(&self) -> BlockPointerNode {
        BlockPointerNode
    }
    fn update(&self, _: &mut Self::Node) {}
    fn same(&self, _other: &Self) -> bool {
        true
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

/// The minimum size of an interactive element by default (48 dp), as Compose's
/// `LocalMinimumInteractiveComponentSize`'s default.
pub const MINIMUM_INTERACTIVE_SIZE: Dp = Dp(48.0);

/// The minimum size of the interactive components below, as Compose's
/// `LocalMinimumInteractiveComponentSize`: 48 dp by default; 0 dp (or `Dp::UNSPECIFIED`) turns
/// the minimum off.
pub static LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE: compose_runtime::CompositionLocal<Dp> =
    compose_runtime::CompositionLocal::new("LocalMinimumInteractiveComponentSize", || {
        MINIMUM_INTERACTIVE_SIZE
    });

/// How far a slider's or a linear indicator's semantics node reaches past its layout on each
/// side, as Compose's `HorizontalSemanticsBoundsPadding` and `VerticalSemanticsBoundsPadding`.
pub(crate) const SEMANTICS_BOUNDS_PADDING: Dp = Dp(10.0);

/// The semantics node `SEMANTICS_BOUNDS_PADDING` wider (`horizontal`) or taller on each side
/// than the layout, which stays as it was, as Compose's `IncreaseHorizontalSemanticsBounds` and
/// `IncreaseVerticalSemanticsBounds`: a "negative padding" layout, a merging semantics node
/// (whose bounds are the node's semantic bounds) and the padding back. Thin controls reach
/// screen readers' minimum box this way.
pub(crate) fn increase_semantics_bounds(modifier: Modifier, horizontal: bool) -> Modifier {
    let padding = if horizontal {
        compose_ui::PaddingValues::symmetric(SEMANTICS_BOUNDS_PADDING, Dp(0.0))
    } else {
        compose_ui::PaddingValues::symmetric(Dp(0.0), SEMANTICS_BOUNDS_PADDING)
    };
    modifier
        .then_custom(NegativePadding { horizontal })
        .semantics_with(true, |_| {})
        .padding(padding)
}

/// The layout of `increase_semantics_bounds`: measures what's inside `SEMANTICS_BOUNDS_PADDING`
/// larger on each side and takes the size without it, placing it that much before.
#[derive(Clone, Copy, PartialEq)]
struct NegativePadding {
    horizontal: bool,
}

impl ModifierNodeElement for NegativePadding {
    type Node = NegativePadding;
    fn create(&self) -> NegativePadding {
        *self
    }
    fn update(&self, node: &mut NegativePadding) {
        *node = *self;
    }
    fn same(&self, other: &Self) -> bool {
        other == self
    }
    fn name(&self) -> &'static str {
        "increaseSemanticsBounds"
    }
}

impl ModifierNode for NegativePadding {
    fn as_layout(&mut self) -> Option<&mut dyn compose_ui::LayoutModifierNode> {
        Some(self)
    }
}

impl compose_ui::LayoutModifierNode for NegativePadding {
    fn measure(
        &mut self,
        scope: &mut compose_ui::MeasureScope<'_>,
        content: compose_ui::Measurable,
        constraints: compose_ui::Constraints,
    ) -> compose_ui::MeasureResult {
        let padding = scope.density().round_to_px(SEMANTICS_BOUNDS_PADDING);
        let (dx, dy) = if self.horizontal { (padding, 0) } else { (0, padding) };
        let placeable = scope.measure(content, constraints.offset(dx * 2, dy * 2));
        scope.place(&placeable, -dx, -dy);
        scope.layout(placeable.width - dx * 2, placeable.height - dy * 2)
    }
}

/// Lays its content out at least `LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE` in both
/// directions, centered, so small controls are easy to hit, as Compose's
/// `minimumInteractiveComponentSize()` (nothing where the local is 0 or unspecified). Reads the
/// local where it's called: call it in composition.
pub fn minimum_interactive_component_size(modifier: Modifier) -> Modifier {
    let size = LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.current();
    if size.is_specified() && size.0 > 0.0 { modifier.minimum_touch_target(size) } else { modifier }
}

// ---- TextField -------------------------------------------------------------------------------

/// A text field's colors, as Compose's `TextFieldColors` (the roles used).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextFieldColors {
    /// Unfocused, as `unfocusedContainerColor`.
    pub container_color: Color,
    pub focused_container_color: Color,
    pub error_container_color: Color,
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
    /// The selection's colors in the field, as `textSelectionColors` (by default the theme's,
    /// `LOCAL_TEXT_SELECTION_COLORS`).
    pub text_selection_colors: compose_foundation::TextSelectionColors,
}

impl compose_runtime::ComposeArg for TextFieldColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// What `tonalColors()` have in common, filled or outlined: the text's and the other parts'.
pub(crate) fn tonal_text_colors() -> TextFieldColors {
    let scheme = MaterialTheme::color_scheme();
    let disabled = scheme.on_surface_variant.with_alpha(0.38);
    TextFieldColors {
        focused_label_color: scheme.on_surface_variant,
        error_leading_icon_color: scheme.on_surface_variant,
        disabled_label_color: disabled,
        disabled_leading_icon_color: disabled,
        disabled_trailing_icon_color: disabled,
        disabled_placeholder_color: disabled,
        disabled_prefix_color: disabled,
        disabled_suffix_color: disabled,
        supporting_text_color: scheme.on_background,
        disabled_supporting_text_color: scheme.on_background.with_alpha(0.38),
        ..TextFieldDefaults::colors()
    }
}

impl TextFieldColors {
    /// The indicator's color by the state, as `indicatorColor`.
    pub(crate) fn indicator_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_indicator_color
        } else if is_error {
            self.error_indicator_color
        } else if focused {
            self.focused_indicator_color
        } else {
            self.unfocused_indicator_color
        }
    }
}

pub struct TextFieldDefaults {}

impl TextFieldDefaults {
    pub const MIN_WIDTH: Dp = Dp(280.0);
    pub const MIN_HEIGHT: Dp = Dp(56.0);
    /// The line's width, as `UnfocusedIndicatorThickness` and `FocusedIndicatorThickness`.
    pub const UNFOCUSED_INDICATOR_THICKNESS: Dp = Dp(1.0);
    pub const FOCUSED_INDICATOR_THICKNESS: Dp = Dp(2.0);

    pub fn colors() -> TextFieldColors {
        Self::colors_of(
            &MaterialTheme::color_scheme(),
            compose_foundation::LOCAL_TEXT_SELECTION_COLORS.current(),
        )
    }

    /// `colors()` from these, for what's not composing.
    pub(crate) fn colors_of(
        scheme: &crate::ColorScheme,
        text_selection_colors: compose_foundation::TextSelectionColors,
    ) -> TextFieldColors {
        TextFieldColors {
            container_color: scheme.surface_container_highest,
            focused_container_color: scheme.surface_container_highest,
            error_container_color: scheme.surface_container_highest,
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
            text_selection_colors,
        }
    }

    /// A filled field's paddings with a label inside, as `contentPaddingWithLabel()`: 16 dp at the
    /// sides, 8 above and below. Change them with struct update.
    pub const fn content_padding_with_label() -> compose_ui::PaddingValues {
        compose_ui::PaddingValues { start: Dp(16.0), top: Dp(8.0), end: Dp(16.0), bottom: Dp(8.0) }
    }

    /// Without one inside, as `contentPaddingWithoutLabel()`: 16 dp all round.
    pub const fn content_padding_without_label() -> compose_ui::PaddingValues {
        compose_ui::PaddingValues {
            start: Dp(16.0),
            top: Dp(16.0),
            end: Dp(16.0),
            bottom: Dp(16.0),
        }
    }

    /// The expressive rounded shape, as `roundedShape`: `CornerMedium`.
    pub fn rounded_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// As `tonalColors()`: on the surface container, no indicator.
    pub fn tonal_colors() -> TextFieldColors {
        let scheme = MaterialTheme::color_scheme();
        TextFieldColors {
            container_color: scheme.surface_container,
            focused_container_color: scheme.surface_container,
            error_container_color: scheme.error_container,
            disabled_container_color: scheme.surface_container.with_alpha(0.38),
            focused_indicator_color: Color::TRANSPARENT,
            unfocused_indicator_color: Color::TRANSPARENT,
            disabled_indicator_color: Color::TRANSPARENT,
            error_indicator_color: Color::TRANSPARENT,
            ..tonal_text_colors()
        }
    }

    /// The filled field's shape: extra small, rounded at the top only.
    pub fn shape() -> Shape {
        Self::shape_of(&MaterialTheme::shapes())
    }

    /// `shape()` from these, for what's not composing.
    pub(crate) fn shape_of(shapes: &crate::Shapes) -> Shape {
        let radius = match shapes.extra_small.corners() {
            Some(corners) => corners[0],
            None => compose_ui::CornerSize::Dp(Dp(0.0)),
        };
        Shape::Rounded([
            radius,
            radius,
            compose_ui::CornerSize::Dp(Dp(0.0)),
            compose_ui::CornerSize::Dp(Dp(0.0)),
        ])
    }
}
