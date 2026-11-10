//! The toggle buttons, as Compose's Material 3: `ToggleButton`, `ElevatedToggleButton`,
//! `FilledTonalToggleButton` and `OutlinedToggleButton`, buttons with a checked state whose shape
//! moves from round to square, with `ToggleButtonDefaults` and its siblings, `ToggleButtonColors`,
//! `ToggleButtonShapes`, `ToggleButtonElevation` and `ToggleButtonSize`.

use compose_foundation::BorderStroke;
use compose_foundation::layout::{Arrangement, Box, PaddingValues, Row, RowScope, Spacer};
use compose_runtime::composable;
use compose_ui::{Alignment, Brush, Color, Dp, Modifier, MutableInteractionSource, Role, Shape};

use crate::button::ButtonDefaults;
use crate::components::{Elevations, SurfaceClick, remembered_interaction_source, surface};
use crate::theme::{MaterialTheme, provide_content_color_text_style};

/// A toggle button's height, as Compose's `ToggleButtonSize`: one of the five expressive sizes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ToggleButtonSize {
    height: Dp,
}

impl ToggleButtonSize {
    pub const EXTRA_SMALL: ToggleButtonSize =
        ToggleButtonSize { height: ButtonDefaults::EXTRA_SMALL_CONTAINER_HEIGHT };
    pub const SMALL: ToggleButtonSize = ToggleButtonSize { height: ButtonDefaults::MIN_HEIGHT };
    pub const MEDIUM: ToggleButtonSize =
        ToggleButtonSize { height: ButtonDefaults::MEDIUM_CONTAINER_HEIGHT };
    pub const LARGE: ToggleButtonSize =
        ToggleButtonSize { height: ButtonDefaults::LARGE_CONTAINER_HEIGHT };
    pub const EXTRA_LARGE: ToggleButtonSize =
        ToggleButtonSize { height: ButtonDefaults::EXTRA_LARGE_CONTAINER_HEIGHT };

    /// Its height, as `height`.
    pub const fn height(self) -> Dp {
        self.height
    }
}

/// A size is its height wherever a height is asked for: the defaults' `…_for` functions take
/// either, as Compose's overloads.
impl From<ToggleButtonSize> for Dp {
    fn from(size: ToggleButtonSize) -> Dp {
        size.height
    }
}

impl compose_runtime::ComposeArg for ToggleButtonSize {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A toggle button's colors, unchecked, checked and disabled, as Compose's `ToggleButtonColors`.
/// Change some of the defaults with struct update syntax.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ToggleButtonColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
    pub checked_container_color: Color,
    pub checked_content_color: Color,
}

impl ToggleButtonColors {
    /// The container's color, as `containerColor(enabled, checked)`.
    pub fn container_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (false, _) => self.disabled_container_color,
            (true, true) => self.checked_container_color,
            (true, false) => self.container_color,
        }
    }

    /// The content's color, as `contentColor(enabled, checked)`.
    pub fn content_color(&self, enabled: bool, checked: bool) -> Color {
        match (enabled, checked) {
            (false, _) => self.disabled_content_color,
            (true, true) => self.checked_content_color,
            (true, false) => self.content_color,
        }
    }
}

impl compose_runtime::ComposeArg for ToggleButtonColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A toggle button's shapes at rest, pressed and checked, as Compose's `ToggleButtonShapes`: its
/// corners move between them if all three have corners, else it takes each as it comes.
#[derive(Clone, PartialEq, Debug)]
pub struct ToggleButtonShapes {
    pub shape: Shape,
    pub pressed_shape: Shape,
    pub checked_shape: Shape,
}

impl compose_runtime::ComposeArg for ToggleButtonShapes {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::hash::Hash for ToggleButtonShapes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        crate::animated_shape::hash_shapes(
            [&self.shape, &self.pressed_shape, &self.checked_shape],
            state,
        );
    }
}

impl ToggleButtonShapes {
    /// As `shapeByInteraction`: pressed, then checked, moving on the fast spatial spec.
    pub(crate) fn for_state(&self, pressed: bool, checked: bool) -> Shape {
        let target = if pressed {
            self.pressed_shape.clone()
        } else if checked {
            self.checked_shape.clone()
        } else {
            self.shape.clone()
        };
        let corner_based = self.shape.corners().is_some()
            && self.pressed_shape.corners().is_some()
            && self.checked_shape.corners().is_some();
        compose_runtime::key(self.clone(), || {
            if corner_based {
                crate::animated_shape::remember_animated_shape(
                    target,
                    MaterialTheme::motion_scheme().fast_spatial_spec(),
                )
            } else {
                target
            }
        })
    }
}

/// A toggle button's shadow in each of its states, as Compose's `ToggleButtonElevation`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ToggleButtonElevation {
    pub default_elevation: Dp,
    pub pressed_elevation: Dp,
    pub focused_elevation: Dp,
    pub hovered_elevation: Dp,
    pub disabled_elevation: Dp,
}

impl compose_runtime::ComposeArg for ToggleButtonElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ToggleButtonElevation {
    fn elevations(self) -> Elevations {
        Elevations {
            default: self.default_elevation,
            pressed: self.pressed_elevation,
            focused: self.focused_elevation,
            hovered: self.hovered_elevation,
            dragged: self.default_elevation,
            disabled: self.disabled_elevation,
        }
    }
}

/// As Compose's `ToggleButtonDefaults`: what `ToggleButton` takes by default.
pub struct ToggleButtonDefaults {}

impl ToggleButtonDefaults {
    /// As `size`.
    pub const SIZE: ToggleButtonSize = ToggleButtonSize::SMALL;
    pub const MIN_HEIGHT: Dp = Dp(40.0);
    pub const ICON_SPACING: Dp = Dp(8.0);
    pub const ICON_SIZE: Dp = Dp(20.0);

    /// The padding of a toggle button `button_height` high (or of a `ToggleButtonSize`), as
    /// `contentPaddingFor`: the buttons' (`ButtonDefaults::content_padding_for`), without a
    /// precision pointer an icon at either end changing nothing.
    pub fn content_padding_for(
        button_height: impl Into<Dp>,
        has_start_icon: bool,
        has_end_icon: bool,
    ) -> PaddingValues {
        ButtonDefaults::content_padding_for(button_height.into(), has_start_icon, has_end_icon)
    }

    /// A pill, as `shape`.
    pub fn shape() -> Shape {
        compose_foundation::CIRCLE_SHAPE
    }

    /// As `pressedShape`: the theme's small corners.
    pub fn pressed_shape() -> Shape {
        MaterialTheme::shapes().small.clone()
    }

    /// As `checkedShape`: the theme's medium corners.
    pub fn checked_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// The shapes of a toggle button `button_height` high (or of a `ToggleButtonSize`), as
    /// `shapesFor`: a pill, its corners coming in further when pressed than when checked, the
    /// larger sizes' less round.
    pub fn shapes_for(button_height: impl Into<Dp>) -> ToggleButtonShapes {
        let height = button_height.into().0;
        let between = |a: Dp, b: Dp| (a.0 + b.0) / 2.0;
        let shapes = MaterialTheme::shapes();
        let (pressed_shape, checked_shape) = if height
            <= between(ButtonDefaults::EXTRA_SMALL_CONTAINER_HEIGHT, ButtonDefaults::MIN_HEIGHT)
            || height
                <= between(ButtonDefaults::MIN_HEIGHT, ButtonDefaults::MEDIUM_CONTAINER_HEIGHT)
        {
            (shapes.small.clone(), shapes.medium.clone())
        } else if height
            <= between(
                ButtonDefaults::MEDIUM_CONTAINER_HEIGHT,
                ButtonDefaults::LARGE_CONTAINER_HEIGHT,
            )
        {
            (shapes.medium.clone(), shapes.large.clone())
        } else {
            (shapes.large.clone(), shapes.extra_large.clone())
        };
        ToggleButtonShapes { shape: Self::shape(), pressed_shape, checked_shape }
    }

    /// The surface container, its text on-surface-variant, the primary color checked, as
    /// `colors()` (`FilledButtonTokens`).
    pub fn colors() -> ToggleButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ToggleButtonColors {
            container_color: scheme.surface_container,
            content_color: scheme.on_surface_variant,
            disabled_container_color: scheme.on_surface.with_alpha(0.1),
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
            checked_container_color: scheme.primary,
            checked_content_color: scheme.on_primary,
        }
    }

    /// Flat, 1 dp hovered, as `elevation()`.
    pub fn elevation() -> ToggleButtonElevation {
        ToggleButtonElevation {
            default_elevation: Dp(0.0),
            pressed_elevation: Dp(0.0),
            focused_elevation: Dp(0.0),
            hovered_elevation: Dp(1.0),
            disabled_elevation: Dp(0.0),
        }
    }
}

/// As Compose's `ElevatedToggleButtonDefaults`: `ToggleButtonDefaults`' sizes and shapes, raised.
pub struct ElevatedToggleButtonDefaults {}

impl ElevatedToggleButtonDefaults {
    pub const SIZE: ToggleButtonSize = ToggleButtonDefaults::SIZE;
    pub const MIN_HEIGHT: Dp = ToggleButtonDefaults::MIN_HEIGHT;
    pub const ICON_SPACING: Dp = ToggleButtonDefaults::ICON_SPACING;
    pub const ICON_SIZE: Dp = ToggleButtonDefaults::ICON_SIZE;

    /// As `contentPaddingFor`.
    pub fn content_padding_for(
        button_height: impl Into<Dp>,
        has_start_icon: bool,
        has_end_icon: bool,
    ) -> PaddingValues {
        ToggleButtonDefaults::content_padding_for(button_height, has_start_icon, has_end_icon)
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        ToggleButtonDefaults::shape()
    }

    /// As `pressedShape`.
    pub fn pressed_shape() -> Shape {
        ToggleButtonDefaults::pressed_shape()
    }

    /// As `checkedShape`.
    pub fn checked_shape() -> Shape {
        ToggleButtonDefaults::checked_shape()
    }

    /// As `shapesFor`.
    pub fn shapes_for(button_height: impl Into<Dp>) -> ToggleButtonShapes {
        ToggleButtonDefaults::shapes_for(button_height)
    }

    /// The low surface container, its text primary, the primary color checked, as `colors()`
    /// (`ElevatedButtonTokens`).
    pub fn colors() -> ToggleButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ToggleButtonColors {
            container_color: scheme.surface_container_low,
            content_color: scheme.primary,
            disabled_container_color: scheme.on_surface.with_alpha(0.1),
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
            checked_container_color: scheme.primary,
            checked_content_color: scheme.on_primary,
        }
    }

    /// 1 dp, 3 dp hovered, flat disabled, as `elevation()`.
    pub fn elevation() -> ToggleButtonElevation {
        ToggleButtonElevation {
            default_elevation: Dp(1.0),
            pressed_elevation: Dp(1.0),
            focused_elevation: Dp(1.0),
            hovered_elevation: Dp(3.0),
            disabled_elevation: Dp(0.0),
        }
    }
}

/// As Compose's `FilledTonalToggleButtonDefaults`: `ToggleButtonDefaults`' sizes and shapes, in
/// the secondary colors.
pub struct FilledTonalToggleButtonDefaults {}

impl FilledTonalToggleButtonDefaults {
    pub const SIZE: ToggleButtonSize = ToggleButtonDefaults::SIZE;
    pub const MIN_HEIGHT: Dp = ToggleButtonDefaults::MIN_HEIGHT;
    pub const ICON_SPACING: Dp = ToggleButtonDefaults::ICON_SPACING;
    pub const ICON_SIZE: Dp = ToggleButtonDefaults::ICON_SIZE;

    /// As `contentPaddingFor`.
    pub fn content_padding_for(
        button_height: impl Into<Dp>,
        has_start_icon: bool,
        has_end_icon: bool,
    ) -> PaddingValues {
        ToggleButtonDefaults::content_padding_for(button_height, has_start_icon, has_end_icon)
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        ToggleButtonDefaults::shape()
    }

    /// As `pressedShape`.
    pub fn pressed_shape() -> Shape {
        ToggleButtonDefaults::pressed_shape()
    }

    /// As `checkedShape`.
    pub fn checked_shape() -> Shape {
        ToggleButtonDefaults::checked_shape()
    }

    /// As `shapesFor`.
    pub fn shapes_for(button_height: impl Into<Dp>) -> ToggleButtonShapes {
        ToggleButtonDefaults::shapes_for(button_height)
    }

    /// The secondary container, the secondary color checked, as `colors()` (`TonalButtonTokens`).
    pub fn colors() -> ToggleButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ToggleButtonColors {
            container_color: scheme.secondary_container,
            content_color: scheme.on_secondary_container,
            disabled_container_color: scheme.on_surface.with_alpha(0.1),
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
            checked_container_color: scheme.secondary,
            checked_content_color: scheme.on_secondary,
        }
    }

    /// Flat, 1 dp hovered, as `elevation()` (`FilledTonalButtonTokens`).
    pub fn elevation() -> ToggleButtonElevation {
        ToggleButtonDefaults::elevation()
    }
}

/// As Compose's `OutlinedToggleButtonDefaults`: `ToggleButtonDefaults`' sizes and shapes, an
/// outline until checked.
pub struct OutlinedToggleButtonDefaults {}

impl OutlinedToggleButtonDefaults {
    pub const SIZE: ToggleButtonSize = ToggleButtonDefaults::SIZE;
    pub const MIN_HEIGHT: Dp = ToggleButtonDefaults::MIN_HEIGHT;
    pub const ICON_SPACING: Dp = ToggleButtonDefaults::ICON_SPACING;
    pub const ICON_SIZE: Dp = ToggleButtonDefaults::ICON_SIZE;

    /// As `contentPaddingFor`.
    pub fn content_padding_for(
        button_height: impl Into<Dp>,
        has_start_icon: bool,
        has_end_icon: bool,
    ) -> PaddingValues {
        ToggleButtonDefaults::content_padding_for(button_height, has_start_icon, has_end_icon)
    }

    /// As `shape`.
    pub fn shape() -> Shape {
        ToggleButtonDefaults::shape()
    }

    /// As `pressedShape`.
    pub fn pressed_shape() -> Shape {
        ToggleButtonDefaults::pressed_shape()
    }

    /// As `checkedShape`.
    pub fn checked_shape() -> Shape {
        ToggleButtonDefaults::checked_shape()
    }

    /// As `shapesFor`.
    pub fn shapes_for(button_height: impl Into<Dp>) -> ToggleButtonShapes {
        ToggleButtonDefaults::shapes_for(button_height)
    }

    /// Transparent, its text on-surface-variant, the inverse surface checked, as `colors()`
    /// (`OutlinedButtonTokens`).
    pub fn colors() -> ToggleButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ToggleButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: scheme.on_surface_variant,
            disabled_container_color: scheme.outline_variant.with_alpha(0.1),
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
            checked_container_color: scheme.inverse_surface,
            checked_content_color: scheme.inverse_on_surface,
        }
    }

    /// The outlined button's outline, none when checked, as `border(enabled, checked)`.
    pub fn border(enabled: bool, checked: bool) -> Option<BorderStroke> {
        (!checked).then(|| ButtonDefaults::outlined_button_border(enabled))
    }
}

/// A button with a checked state, as Compose's `ToggleButton`: a toggleable surface (a checkbox to
/// accessibility, in a 48 dp touch target) at least `button_size` high around a centered row of
/// its icon (`icon`, in a box `ButtonDefaults::icon_size_for` the height) and content, in the
/// size's text style. Its shape moves between `shapes`' as it's pressed and checked; its border
/// grows, shrinks and changes color as it's given another.
///
/// `shapes` and `content_padding` are `None` for the size's (`ToggleButtonDefaults::shapes_for`,
/// `content_padding_for`), as Compose's defaults reading `button_size` and `icon`.
#[composable]
pub fn ToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ToggleButtonDefaults::SIZE)] button_size: ToggleButtonSize,
    #[default(true)] enabled: bool,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(None)] shapes: impl Into<Option<ToggleButtonShapes>>,
    #[default(crate::ToggleButtonDefaults::colors())] colors: ToggleButtonColors,
    #[default(Some(crate::ToggleButtonDefaults::elevation()))] elevation: Option<
        ToggleButtonElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let interaction_source = remembered_interaction_source(interaction_source);
    let shapes: Option<ToggleButtonShapes> = shapes;
    let shapes = shapes.unwrap_or_else(|| ToggleButtonDefaults::shapes_for(button_size));
    let content_padding: Option<PaddingValues> = content_padding;
    let content_padding = content_padding.unwrap_or_else(|| {
        ToggleButtonDefaults::content_padding_for(button_size, icon.is_some(), false)
    });
    let shape = shapes.for_state(interaction_source.is_pressed(), checked);
    let border = animated_border(border);
    let height = button_size.height();
    let (icon_size, icon_spacing) =
        (ButtonDefaults::icon_size_for(height), ButtonDefaults::icon_spacing_for(height));
    let text_style = ButtonDefaults::text_style_for(height);
    let content_color = colors.content_color(enabled, checked);
    let click = SurfaceClick {
        on_click: compose_runtime::callback(move || on_checked_change(!checked)),
        enabled,
        role: Some(Role::Checkbox),
        interaction_source,
        selected: None,
        checked: Some(checked),
        elevations: elevation.map(ToggleButtonElevation::elevations),
    };
    surface(
        modifier,
        shape,
        colors.container_color(enabled, checked),
        content_color,
        Dp(0.0),
        Dp(0.0),
        border,
        Some(click),
        compose_runtime::callback(move || {
            let (icon, content, text_style) = (icon.clone(), content.clone(), text_style.clone());
            provide_content_color_text_style(content_color, text_style, || {
                Row(
                    Modifier.default_min_size(Dp::UNSPECIFIED, height).padding(content_padding),
                    Arrangement::CENTER,
                    Alignment::CENTER_VERTICALLY,
                    compose_runtime::callback(move |scope: RowScope| {
                        if let Some(icon) = icon.clone() {
                            Box(
                                Modifier.size(icon_size),
                                Alignment::CENTER,
                                true,
                                compose_runtime::callback(move |_| icon()),
                            );
                            Spacer(Modifier.width(icon_spacing));
                        }
                        content(scope);
                    }),
                );
            });
        }),
    );
}

/// The border following the one given: its width on the fast spatial spec, a solid one's color on
/// the default effects one, as `animateBorderStrokeAsState`; none while it has no width.
fn animated_border(border: Option<BorderStroke>) -> Option<BorderStroke> {
    let scheme = MaterialTheme::motion_scheme();
    let width = crate::lazy_animation::animate_lazily(
        border.as_ref().map_or(Dp(0.0), |border| border.width),
        scheme.fast_spatial_spec(),
        |start: Dp, stop: Dp, fraction| Dp(start.0 + (stop.0 - start.0) * fraction),
    );
    let solid = match border.as_ref().map(|border| &border.brush) {
        Some(Brush::Solid(color)) => *color,
        _ => Color::TRANSPARENT,
    };
    let color = crate::lazy_animation::animate_lazily(
        solid,
        scheme.default_effects_spec(),
        crate::lazy_animation::lerp_color,
    );
    if width.0 <= 0.0 {
        return None;
    }
    Some(match border.map(|border| border.brush) {
        Some(Brush::Solid(_)) | None => BorderStroke::new(width, color),
        Some(brush) => BorderStroke::new(width, brush),
    })
}

/// A toggle button on a 1 dp shadow, as Compose's `ElevatedToggleButton`.
#[composable]
pub fn ElevatedToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ElevatedToggleButtonDefaults::SIZE)] button_size: ToggleButtonSize,
    #[default(true)] enabled: bool,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(None)] shapes: impl Into<Option<ToggleButtonShapes>>,
    #[default(crate::ElevatedToggleButtonDefaults::colors())] colors: ToggleButtonColors,
    #[default(Some(crate::ElevatedToggleButtonDefaults::elevation()))] elevation: Option<
        ToggleButtonElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    ToggleButton(
        checked,
        on_checked_change,
        modifier,
        button_size,
        enabled,
        icon,
        shapes,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}

/// A toggle button in the secondary colors, as Compose's `FilledTonalToggleButton`.
#[composable]
pub fn FilledTonalToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FilledTonalToggleButtonDefaults::SIZE)] button_size: ToggleButtonSize,
    #[default(true)] enabled: bool,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(None)] shapes: impl Into<Option<ToggleButtonShapes>>,
    #[default(crate::FilledTonalToggleButtonDefaults::colors())] colors: ToggleButtonColors,
    #[default(Some(crate::FilledTonalToggleButtonDefaults::elevation()))] elevation: Option<
        ToggleButtonElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    ToggleButton(
        checked,
        on_checked_change,
        modifier,
        button_size,
        enabled,
        icon,
        shapes,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}

/// A toggle button with an outline until checked, as Compose's `OutlinedToggleButton`. `border`
/// is `None` for `OutlinedToggleButtonDefaults::border(enabled, checked)` (a default can't read
/// them).
#[composable]
pub fn OutlinedToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::OutlinedToggleButtonDefaults::SIZE)] button_size: ToggleButtonSize,
    #[default(true)] enabled: bool,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(None)] shapes: impl Into<Option<ToggleButtonShapes>>,
    #[default(crate::OutlinedToggleButtonDefaults::colors())] colors: ToggleButtonColors,
    #[default(None)] elevation: Option<ToggleButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let border = border.or_else(|| OutlinedToggleButtonDefaults::border(enabled, checked));
    ToggleButton(
        checked,
        on_checked_change,
        modifier,
        button_size,
        enabled,
        icon,
        shapes,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}
