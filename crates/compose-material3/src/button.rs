//! The buttons, as Compose's Material 3: `Button`, `FilledTonalButton`, `ElevatedButton`,
//! `OutlinedButton` and `TextButton`, with `ButtonDefaults`, `ButtonColors` and
//! `ButtonElevation`.

use compose_foundation::BorderStroke;
use compose_foundation::layout::{Arrangement, PaddingValues, Row, RowScope};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{Alignment, Color, Dp, Modifier, MutableInteractionSource, Role, Shape, SizeSpec};

use crate::components::{Elevations, SurfaceClick, remembered_interaction_source, surface};
use crate::theme::{LOCAL_TEXT_STYLE, MaterialTheme};

/// A button's colors, enabled and disabled, as Compose's `ButtonColors`. Change some of the
/// defaults with Rust's struct update syntax (`ButtonColors { container_color, ..ButtonDefaults::
/// button_colors() }`), where Compose's `buttonColors(containerColor = …)` names them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ButtonColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
}

impl ButtonColors {
    /// The container's color, as `containerColor(enabled)`.
    pub fn container_color(&self, enabled: bool) -> Color {
        if enabled { self.container_color } else { self.disabled_container_color }
    }

    /// The content's color, as `contentColor(enabled)`.
    pub fn content_color(&self, enabled: bool) -> Color {
        if enabled { self.content_color } else { self.disabled_content_color }
    }
}

impl compose_runtime::ComposeArg for ButtonColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A button's elevation in each of its states, as Compose's `ButtonElevation`: the shadow
/// follows the press, focus and hover (snapping to the state's value, where Compose animates
/// over 90 to 150 ms), the tonal elevation is the resting one.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ButtonElevation {
    pub default_elevation: Dp,
    pub pressed_elevation: Dp,
    pub focused_elevation: Dp,
    pub hovered_elevation: Dp,
    pub disabled_elevation: Dp,
}

impl ButtonElevation {
    /// The tonal elevation, as `tonalElevation(enabled)`.
    pub fn tonal_elevation(&self, enabled: bool) -> Dp {
        if enabled { self.default_elevation } else { self.disabled_elevation }
    }

    /// The shadow's elevation in the state `interaction_source` reports (state reads: pressed,
    /// then hovered, then focused), as `shadowElevation(enabled, interactionSource)`.
    pub fn shadow_elevation(
        &self,
        enabled: bool,
        interaction_source: &MutableInteractionSource,
    ) -> Dp {
        interaction_elevation(
            enabled,
            self.default_elevation,
            self.pressed_elevation,
            self.focused_elevation,
            self.hovered_elevation,
            self.disabled_elevation,
            interaction_source,
        )
    }
}

impl compose_runtime::ComposeArg for ButtonElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The elevation for the interaction going on. The states are read only when they make a
/// difference, so a component whose elevation doesn't change isn't composed again on a hover.
pub(crate) fn interaction_elevation(
    enabled: bool,
    default: Dp,
    pressed: Dp,
    focused: Dp,
    hovered: Dp,
    disabled: Dp,
    interaction_source: &MutableInteractionSource,
) -> Dp {
    if !enabled {
        return disabled;
    }
    if pressed != default && interaction_source.is_pressed() {
        pressed
    } else if hovered != default && interaction_source.is_hovered() {
        hovered
    } else if focused != default && interaction_source.is_focused() {
        focused
    } else {
        default
    }
}

/// As Compose's `ButtonDefaults`.
pub struct ButtonDefaults {}

impl ButtonDefaults {
    pub const CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(24.0), Dp(8.0));
    /// With a leading icon, as `ButtonWithIconContentPadding`.
    pub const BUTTON_WITH_ICON_CONTENT_PADDING: PaddingValues =
        PaddingValues::new(Dp(16.0), Dp(8.0), Dp(24.0), Dp(8.0));
    pub const TEXT_BUTTON_CONTENT_PADDING: PaddingValues =
        PaddingValues::symmetric(Dp(12.0), Dp(8.0));
    /// A text button with an icon, as `TextButtonWithIconContentPadding`.
    pub const TEXT_BUTTON_WITH_ICON_CONTENT_PADDING: PaddingValues =
        PaddingValues::new(Dp(12.0), Dp(8.0), Dp(16.0), Dp(8.0));
    pub const MIN_WIDTH: Dp = Dp(58.0);
    pub const MIN_HEIGHT: Dp = Dp(40.0);
    /// An icon in a button, as `IconSize`.
    pub const ICON_SIZE: Dp = Dp(18.0);
    /// Between an icon and the text, as `IconSpacing`.
    pub const ICON_SPACING: Dp = Dp(8.0);

    /// A fully rounded pill, as `ButtonDefaults.shape`.
    pub fn shape() -> Shape {
        compose_foundation::CIRCLE_SHAPE
    }

    /// As `elevatedShape`.
    pub fn elevated_shape() -> Shape {
        compose_foundation::CIRCLE_SHAPE
    }

    /// As `filledTonalShape`.
    pub fn filled_tonal_shape() -> Shape {
        compose_foundation::CIRCLE_SHAPE
    }

    /// As `outlinedShape`.
    pub fn outlined_shape() -> Shape {
        compose_foundation::CIRCLE_SHAPE
    }

    /// As `textShape`.
    pub fn text_shape() -> Shape {
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

    /// The secondary container, as `filledTonalButtonColors()`.
    pub fn filled_tonal_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: scheme.secondary_container,
            content_color: scheme.on_secondary_container,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// The low surface container, primary text, as `elevatedButtonColors()`.
    pub fn elevated_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: scheme.surface_container_low,
            content_color: scheme.primary,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// Transparent, primary text, as `outlinedButtonColors()`.
    pub fn outlined_button_colors() -> ButtonColors {
        ButtonDefaults::text_button_colors()
    }

    /// Transparent, primary text, as `textButtonColors()`.
    pub fn text_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: scheme.primary,
            disabled_container_color: Color::TRANSPARENT,
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// Flat, 1 dp hovered, as `buttonElevation()`.
    pub fn button_elevation() -> ButtonElevation {
        ButtonElevation {
            default_elevation: Dp(0.0),
            pressed_elevation: Dp(0.0),
            focused_elevation: Dp(0.0),
            hovered_elevation: Dp(1.0),
            disabled_elevation: Dp(0.0),
        }
    }

    /// As `filledTonalButtonElevation()`.
    pub fn filled_tonal_button_elevation() -> ButtonElevation {
        ButtonDefaults::button_elevation()
    }

    /// 1 dp, 3 dp hovered, flat disabled, as `elevatedButtonElevation()`.
    pub fn elevated_button_elevation() -> ButtonElevation {
        ButtonElevation {
            default_elevation: Dp(1.0),
            pressed_elevation: Dp(1.0),
            focused_elevation: Dp(1.0),
            hovered_elevation: Dp(3.0),
            disabled_elevation: Dp(0.0),
        }
    }

    /// The outline, as `outlinedButtonBorder(enabled)`.
    pub fn outlined_button_border(enabled: bool) -> BorderStroke {
        let scheme = MaterialTheme::color_scheme();
        BorderStroke::new(
            Dp(1.0),
            if enabled { scheme.outline } else { scheme.on_surface.with_alpha(0.12) },
        )
    }
}

/// A filled button, as Compose's `Button`: a clickable surface (at least 58 by 40 dp, in a 48 dp
/// touch target) around a centered row of content in the label style; its shadow follows its
/// press, focus and hover (`elevation`, none if `None`).
#[composable]
pub fn Button(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::ButtonDefaults::shape())] shape: Shape,
    #[default(crate::ButtonDefaults::button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::button_elevation()))] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::ButtonDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let interaction_source = remembered_interaction_source(interaction_source);
    let tonal = elevation.map_or(Dp(0.0), |elevation| elevation.tonal_elevation(enabled));
    let label = MaterialTheme::typography().label_large;
    let click = SurfaceClick {
        on_click,
        enabled,
        role: Some(Role::Button),
        interaction_source,
        selected: None,
        // The shadow follows the press and hover while drawing.
        elevations: elevation.map(|elevation| Elevations {
            default: elevation.default_elevation,
            pressed: elevation.pressed_elevation,
            focused: elevation.focused_elevation,
            hovered: elevation.hovered_elevation,
            disabled: elevation.disabled_elevation,
        }),
    };
    surface(
        modifier,
        shape,
        colors.container_color(enabled),
        colors.content_color(enabled),
        tonal,
        Dp(0.0),
        border,
        Some(click),
        compose_runtime::callback(move || {
            let content = content.clone();
            CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(label), || {
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
            });
        }),
    );
}

/// A button in the secondary container, as Compose's `FilledTonalButton`.
#[composable]
pub fn FilledTonalButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::ButtonDefaults::filled_tonal_shape())] shape: Shape,
    #[default(crate::ButtonDefaults::filled_tonal_button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::filled_tonal_button_elevation()))] elevation: Option<
        ButtonElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::ButtonDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    Button(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}

/// A button raised on a 1 dp shadow, as Compose's `ElevatedButton`.
#[composable]
pub fn ElevatedButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::ButtonDefaults::elevated_shape())] shape: Shape,
    #[default(crate::ButtonDefaults::elevated_button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::elevated_button_elevation()))] elevation: Option<
        ButtonElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::ButtonDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    Button(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}

/// A button with an outline and no fill, as Compose's `OutlinedButton`.
#[composable]
pub fn OutlinedButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::ButtonDefaults::outlined_shape())] shape: Shape,
    #[default(crate::ButtonDefaults::outlined_button_colors())] colors: ButtonColors,
    #[default(None)] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::ButtonDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    // `None` is the default outline, `outlinedButtonBorder(enabled)` (a default can't read
    // `enabled`).
    let border = border.or_else(|| Some(ButtonDefaults::outlined_button_border(enabled)));
    Button(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}

/// A button of text alone, as Compose's `TextButton`.
#[composable]
pub fn TextButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(crate::ButtonDefaults::text_shape())] shape: Shape,
    #[default(crate::ButtonDefaults::text_button_colors())] colors: ButtonColors,
    #[default(None)] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(crate::ButtonDefaults::TEXT_BUTTON_CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    Button(
        on_click,
        modifier,
        enabled,
        shape,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}
