//! The buttons, as Compose's Material 3: `Button`, `FilledTonalButton`, `ElevatedButton`,
//! `OutlinedButton` and `TextButton`, with `ButtonDefaults`, `ButtonColors` and
//! `ButtonElevation`.

use compose_foundation::BorderStroke;
use compose_foundation::layout::{Arrangement, PaddingValues, Row, RowScope};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{Alignment, Color, Dp, Modifier, MutableInteractionSource, Role, Shape};

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
/// follows the press, focus and hover (animated, `surface`), the tonal elevation is the resting
/// one.
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

    /// The shadow's elevation for the interaction `interaction_source` last began, a state read, as
    /// `shadowElevation(enabled, interactionSource)`.
    pub fn shadow_elevation(
        &self,
        enabled: bool,
        interaction_source: &MutableInteractionSource,
    ) -> Dp {
        let elevations = Elevations {
            default: self.default_elevation,
            pressed: self.pressed_elevation,
            focused: self.focused_elevation,
            hovered: self.hovered_elevation,
            dragged: self.default_elevation,
            disabled: self.disabled_elevation,
        };
        crate::components::elevation_and_interaction(elevations, enabled, interaction_source).0
    }
}

impl compose_runtime::ComposeArg for ButtonElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A button's shape, resting and pressed, as Compose's `ButtonShapes`: its corners move between them
/// (on default effects, so as not to bounce), if they have corners.
#[derive(Clone, PartialEq, Debug)]
pub struct ButtonShapes {
    pub shape: Shape,
    pub pressed_shape: Shape,
}

impl compose_runtime::ComposeArg for ButtonShapes {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::hash::Hash for ButtonShapes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        crate::animated_shape::hash_shapes([&self.shape, &self.pressed_shape], state);
    }
}

impl ButtonShapes {
    /// As `shapeByInteraction`.
    fn for_state(&self, pressed: bool) -> Shape {
        let target = if pressed { self.pressed_shape.clone() } else { self.shape.clone() };
        compose_runtime::key(self.clone(), || {
            if self.shape.corners().is_some() && self.pressed_shape.corners().is_some() {
                crate::animated_shape::remember_animated_shape(
                    target,
                    MaterialTheme::motion_scheme().default_effects_spec(),
                )
            } else {
                target
            }
        })
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

    /// The expressive sizes' heights: a small one's is `MIN_HEIGHT`, as `ExtraSmallContainerHeight`
    /// and the others.
    pub const EXTRA_SMALL_CONTAINER_HEIGHT: Dp = Dp(32.0);
    pub const MEDIUM_CONTAINER_HEIGHT: Dp = Dp(56.0);
    pub const LARGE_CONTAINER_HEIGHT: Dp = Dp(96.0);
    pub const EXTRA_LARGE_CONTAINER_HEIGHT: Dp = Dp(136.0);
    /// Their icons, as `ExtraSmallIconSize` and the others.
    pub const EXTRA_SMALL_ICON_SIZE: Dp = Dp(20.0);
    pub const SMALL_ICON_SIZE: Dp = Dp(20.0);
    pub const MEDIUM_ICON_SIZE: Dp = Dp(24.0);
    pub const LARGE_ICON_SIZE: Dp = Dp(32.0);
    pub const EXTRA_LARGE_ICON_SIZE: Dp = Dp(40.0);
    /// Between their icons and text, as `ExtraSmallIconSpacing` and the others.
    pub const EXTRA_SMALL_ICON_SPACING: Dp = Dp(4.0);
    pub const MEDIUM_ICON_SPACING: Dp = Dp(8.0);
    pub const LARGE_ICON_SPACING: Dp = Dp(12.0);
    pub const EXTRA_LARGE_ICON_SPACING: Dp = Dp(16.0);
    /// Their padding, as `ExtraSmallContentPadding` and the others (10 dp down a small one's, Compose's
    /// without a precision pointer).
    pub const EXTRA_SMALL_CONTENT_PADDING: PaddingValues =
        PaddingValues::symmetric(Dp(12.0), Dp(6.0));
    pub const SMALL_CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(16.0), Dp(10.0));
    pub const MEDIUM_CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(24.0), Dp(16.0));
    pub const LARGE_CONTENT_PADDING: PaddingValues = PaddingValues::symmetric(Dp(48.0), Dp(32.0));
    pub const EXTRA_LARGE_CONTENT_PADDING: PaddingValues =
        PaddingValues::symmetric(Dp(64.0), Dp(48.0));

    /// As `squareShape`.
    pub fn square_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// As `pressedShape`.
    pub fn pressed_shape() -> Shape {
        MaterialTheme::shapes().small.clone()
    }

    /// As `extraSmallPressedShape`.
    pub fn extra_small_pressed_shape() -> Shape {
        MaterialTheme::shapes().small.clone()
    }

    /// As `mediumPressedShape`.
    pub fn medium_pressed_shape() -> Shape {
        MaterialTheme::shapes().medium.clone()
    }

    /// As `largePressedShape`.
    pub fn large_pressed_shape() -> Shape {
        MaterialTheme::shapes().large.clone()
    }

    /// As `extraLargePressedShape`.
    pub fn extra_large_pressed_shape() -> Shape {
        MaterialTheme::shapes().large.clone()
    }

    /// As `shapes()`: a pill, its corners coming in when pressed. Change one with struct update.
    pub fn shapes() -> ButtonShapes {
        ButtonShapes {
            shape: ButtonDefaults::shape(),
            pressed_shape: ButtonDefaults::pressed_shape(),
        }
    }

    /// The shapes for a button of `button_height`, as `shapesFor`.
    pub fn shapes_for(button_height: Dp) -> ButtonShapes {
        let between = |a: Dp, b: Dp| Dp((a.0 + b.0) / 2.0);
        let pressed_shape = if button_height.0
            <= between(Self::EXTRA_SMALL_CONTAINER_HEIGHT, Self::MIN_HEIGHT).0
        {
            Self::extra_small_pressed_shape()
        } else if button_height.0 <= between(Self::MIN_HEIGHT, Self::MEDIUM_CONTAINER_HEIGHT).0 {
            Self::pressed_shape()
        } else if button_height.0
            <= between(Self::MEDIUM_CONTAINER_HEIGHT, Self::LARGE_CONTAINER_HEIGHT).0
        {
            Self::medium_pressed_shape()
        } else if button_height.0
            <= between(Self::LARGE_CONTAINER_HEIGHT, Self::EXTRA_LARGE_CONTAINER_HEIGHT).0
        {
            Self::large_pressed_shape()
        } else {
            Self::extra_large_pressed_shape()
        };
        ButtonShapes { shape: Self::shape(), pressed_shape }
    }

    /// Which of the sizes `button_height` is: 0 extra small to 4 extra large, as the `when`s below
    /// tell them apart.
    fn size_of(button_height: Dp) -> usize {
        [
            Self::MIN_HEIGHT,
            Self::MEDIUM_CONTAINER_HEIGHT,
            Self::LARGE_CONTAINER_HEIGHT,
            Self::EXTRA_LARGE_CONTAINER_HEIGHT,
        ]
        .iter()
        .take_while(|height| button_height.0 >= height.0)
        .count()
    }

    /// The padding for a button of `button_height`, as `contentPaddingFor`. An icon at either end
    /// changes nothing without a precision pointer, these sizes' spaces being the same.
    pub fn content_padding_for(
        button_height: Dp,
        _has_start_icon: bool,
        _has_end_icon: bool,
    ) -> PaddingValues {
        [
            Self::EXTRA_SMALL_CONTENT_PADDING,
            Self::SMALL_CONTENT_PADDING,
            Self::MEDIUM_CONTENT_PADDING,
            Self::LARGE_CONTENT_PADDING,
            Self::EXTRA_LARGE_CONTENT_PADDING,
        ][Self::size_of(button_height)]
    }

    /// The icon's size in a button of `button_height`, as `iconSizeFor`.
    pub fn icon_size_for(button_height: Dp) -> Dp {
        [
            Self::EXTRA_SMALL_ICON_SIZE,
            Self::SMALL_ICON_SIZE,
            Self::MEDIUM_ICON_SIZE,
            Self::LARGE_ICON_SIZE,
            Self::EXTRA_LARGE_ICON_SIZE,
        ][Self::size_of(button_height)]
    }

    /// Between the icon and the text in a button of `button_height`, as `iconSpacingFor`.
    pub fn icon_spacing_for(button_height: Dp) -> Dp {
        [
            Self::EXTRA_SMALL_ICON_SPACING,
            Self::ICON_SPACING,
            Self::MEDIUM_ICON_SPACING,
            Self::LARGE_ICON_SPACING,
            Self::EXTRA_LARGE_ICON_SPACING,
        ][Self::size_of(button_height)]
    }

    /// The text's style in a button of `button_height`, as `textStyleFor`.
    pub fn text_style_for(button_height: Dp) -> compose_ui::TextStyle {
        let typography = MaterialTheme::typography();
        match Self::size_of(button_height) {
            0 | 1 => typography.label_large.clone(),
            2 => typography.title_medium.clone(),
            3 => typography.headline_small.clone(),
            _ => typography.headline_large.clone(),
        }
    }

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

    /// The primary color, as `buttonColors()` (`FilledButtonTokens`): disabled, the on-surface
    /// color at 10 % under the on-surface-variant one at 38 %.
    pub fn button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: scheme.primary,
            content_color: scheme.on_primary,
            disabled_container_color: scheme.on_surface.with_alpha(0.1),
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
        }
    }

    /// The secondary container, as `filledTonalButtonColors()` (`FilledTonalButtonTokens`):
    /// disabled, the on-surface color at 12 % under itself at 38 %.
    pub fn filled_tonal_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: scheme.secondary_container,
            content_color: scheme.on_secondary_container,
            disabled_container_color: scheme.on_surface.with_alpha(0.12),
            disabled_content_color: scheme.on_surface.with_alpha(0.38),
        }
    }

    /// The low surface container, primary text, as `elevatedButtonColors()`
    /// (`ElevatedButtonTokens`): disabled as `button_colors()`.
    pub fn elevated_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: scheme.surface_container_low,
            content_color: scheme.primary,
            disabled_container_color: scheme.on_surface.with_alpha(0.1),
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
        }
    }

    /// Transparent, on-surface-variant text, as `outlinedButtonColors()`
    /// (`OutlinedButtonTokens.LabelTextColor`, Material 3 1.4; primary before).
    pub fn outlined_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: scheme.on_surface_variant,
            disabled_container_color: Color::TRANSPARENT,
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
        }
    }

    /// Transparent, primary text, as `textButtonColors()` (`TextButtonTokens`).
    pub fn text_button_colors() -> ButtonColors {
        let scheme = MaterialTheme::color_scheme();
        ButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: scheme.primary,
            disabled_container_color: Color::TRANSPARENT,
            disabled_content_color: scheme.on_surface_variant.with_alpha(0.38),
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

    /// The outline, as `outlinedButtonBorder(enabled)`: 1 dp of the outline-variant color
    /// (`OutlinedButtonTokens.OutlineColor`, Material 3 1.4; the outline color before), at 10 %
    /// disabled (`DisabledContainerOpacity`).
    pub fn outlined_button_border(enabled: bool) -> BorderStroke {
        let outline = MaterialTheme::color_scheme().outline_variant;
        BorderStroke::new(Dp(1.0), if enabled { outline } else { outline.with_alpha(0.1) })
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
    #[default(None)] shapes: impl Into<Option<ButtonShapes>>,
    #[default(crate::ButtonDefaults::button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::button_elevation()))] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let interaction_source = remembered_interaction_source(interaction_source);
    let shapes: Option<ButtonShapes> = shapes;
    let content_padding: Option<PaddingValues> = content_padding;
    let content_padding = content_padding.unwrap_or_else(|| {
        if shapes.is_some() {
            ButtonDefaults::content_padding_for(ButtonDefaults::MIN_HEIGHT, false, false)
        } else {
            ButtonDefaults::CONTENT_PADDING
        }
    });
    let shape = match shapes {
        Some(shapes) => shapes.for_state(interaction_source.is_pressed()),
        None => shape,
    };
    let tonal = elevation.map_or(Dp(0.0), |elevation| elevation.tonal_elevation(enabled));
    let label = MaterialTheme::typography().label_large.clone();
    let click = SurfaceClick {
        on_click,
        enabled,
        role: Some(Role::Button),
        interaction_source,
        selected: None,
        checked: None,
        // The shadow follows the press, hover and focus, animated (`surface`).
        elevations: elevation.map(|elevation| Elevations {
            default: elevation.default_elevation,
            pressed: elevation.pressed_elevation,
            focused: elevation.focused_elevation,
            hovered: elevation.hovered_elevation,
            dragged: elevation.default_elevation,
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
                        .default_min_size(ButtonDefaults::MIN_WIDTH, ButtonDefaults::MIN_HEIGHT)
                        .padding(content_padding),
                    Arrangement::CENTER.into(),
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
    #[default(None)] shapes: impl Into<Option<ButtonShapes>>,
    #[default(crate::ButtonDefaults::filled_tonal_button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::filled_tonal_button_elevation()))] elevation: Option<
        ButtonElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    Button(
        on_click,
        modifier,
        enabled,
        shape,
        shapes,
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
    #[default(None)] shapes: impl Into<Option<ButtonShapes>>,
    #[default(crate::ButtonDefaults::elevated_button_colors())] colors: ButtonColors,
    #[default(Some(crate::ButtonDefaults::elevated_button_elevation()))] elevation: Option<
        ButtonElevation,
    >,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    Button(
        on_click,
        modifier,
        enabled,
        shape,
        shapes,
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
    #[default(None)] shapes: impl Into<Option<ButtonShapes>>,
    #[default(crate::ButtonDefaults::outlined_button_colors())] colors: ButtonColors,
    #[default(None)] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
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
        shapes,
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
    #[default(None)] shapes: impl Into<Option<ButtonShapes>>,
    #[default(crate::ButtonDefaults::text_button_colors())] colors: ButtonColors,
    #[default(None)] elevation: Option<ButtonElevation>,
    #[default(None)] border: Option<BorderStroke>,
    #[default(None)] content_padding: impl Into<Option<PaddingValues>>,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn(RowScope) + 'static,
) {
    let shapes: Option<ButtonShapes> = shapes;
    let content_padding: Option<PaddingValues> = content_padding;
    let content_padding = content_padding
        .or_else(|| shapes.is_none().then_some(ButtonDefaults::TEXT_BUTTON_CONTENT_PADDING));
    Button(
        on_click,
        modifier,
        enabled,
        shape,
        shapes,
        colors,
        elevation,
        border,
        content_padding,
        interaction_source,
        content,
    );
}
