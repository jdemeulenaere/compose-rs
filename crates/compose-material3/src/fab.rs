//! Floating action buttons, as Compose's Material 3: `FloatingActionButton`,
//! `SmallFloatingActionButton`, `LargeFloatingActionButton` and `ExtendedFloatingActionButton`,
//! with `FloatingActionButtonDefaults` and `FloatingActionButtonElevation`.

use compose_animation::{
    AnimatedVisibility, expand_horizontally, fade_in, fade_out, shrink_horizontally,
};
use compose_foundation::layout::{Arrangement, Box, Row};
use compose_runtime::{CompositionLocalProvider, composable};
use compose_ui::{
    Alignment, Color, Dp, Modifier, MutableInteractionSource, Role, Shape, SizeSpec, UnitExt,
};

use crate::components::{Elevations, SurfaceClick, remembered_interaction_source, surface};
use crate::theme::{LOCAL_TEXT_STYLE, MaterialTheme};

/// A floating action button's elevation in each of its states, as Compose's
/// `FloatingActionButtonElevation` (the shadow snapping to the state's, where Compose animates).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatingActionButtonElevation {
    pub default_elevation: Dp,
    pub pressed_elevation: Dp,
    pub focused_elevation: Dp,
    pub hovered_elevation: Dp,
}

impl FloatingActionButtonElevation {
    /// The shadow's elevation in the state `interaction_source` reports, as
    /// `shadowElevation(interactionSource)`.
    pub fn shadow_elevation(&self, interaction_source: &MutableInteractionSource) -> Dp {
        crate::button::interaction_elevation(
            true,
            self.default_elevation,
            self.pressed_elevation,
            self.focused_elevation,
            self.hovered_elevation,
            self.default_elevation,
            interaction_source,
        )
    }

    /// The tonal elevation: the resting one, as `tonalElevation()`.
    pub fn tonal_elevation(&self) -> Dp {
        self.default_elevation
    }
}

impl compose_runtime::ComposeArg for FloatingActionButtonElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `FloatingActionButtonDefaults`.
pub struct FloatingActionButtonDefaults;

impl FloatingActionButtonDefaults {
    /// A large FAB's icon, as `LargeIconSize`.
    pub const LARGE_ICON_SIZE: Dp = Dp(36.0);

    /// The large shape (16 dp corners), as `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().large
    }

    /// The medium shape (12 dp), as `smallShape`.
    pub fn small_shape() -> Shape {
        MaterialTheme::shapes().medium
    }

    /// The extra-large shape (28 dp), as `largeShape`.
    pub fn large_shape() -> Shape {
        MaterialTheme::shapes().extra_large
    }

    /// As `extendedFabShape`.
    pub fn extended_fab_shape() -> Shape {
        MaterialTheme::shapes().large
    }

    /// The primary container, as `containerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().primary_container
    }

    /// 6 dp, 8 dp hovered, as `elevation()`.
    pub fn elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation {
            default_elevation: Dp(6.0),
            pressed_elevation: Dp(6.0),
            focused_elevation: Dp(6.0),
            hovered_elevation: Dp(8.0),
        }
    }

    /// 3 dp, 4 dp hovered, as `loweredElevation()`.
    pub fn lowered_elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation {
            default_elevation: Dp(3.0),
            pressed_elevation: Dp(3.0),
            focused_elevation: Dp(3.0),
            hovered_elevation: Dp(4.0),
        }
    }

    /// Flat, for a FAB in a bottom app bar, as `bottomAppBarFabElevation()`.
    pub fn bottom_app_bar_fab_elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation {
            default_elevation: Dp(0.0),
            pressed_elevation: Dp(0.0),
            focused_elevation: Dp(0.0),
            hovered_elevation: Dp(0.0),
        }
    }
}

/// The FABs' body, as Compose's `FloatingActionButton`: a clickable surface around its content,
/// centered in at least `minimum` square (unless the modifier sets a minimum), in the label
/// style.
#[allow(clippy::too_many_arguments)]
fn fab(
    on_click: compose_runtime::Callback,
    modifier: Modifier,
    shape: Shape,
    container_color: Color,
    content_color: Color,
    elevation: FloatingActionButtonElevation,
    interaction_source: Option<MutableInteractionSource>,
    minimum: Dp,
    content: compose_runtime::Callback,
) {
    let interaction_source = remembered_interaction_source(interaction_source);
    let click = SurfaceClick {
        on_click,
        enabled: true,
        role: Some(Role::Button),
        interaction_source,
        selected: None,
        // The shadow follows the press and hover while drawing.
        elevations: Some(Elevations {
            default: elevation.default_elevation,
            pressed: elevation.pressed_elevation,
            focused: elevation.focused_elevation,
            hovered: elevation.hovered_elevation,
            disabled: elevation.default_elevation,
        }),
    };
    let label = MaterialTheme::typography().label_large;
    surface(
        modifier.size_in(SizeSpec::bounds(minimum, minimum, Dp::UNSPECIFIED, Dp::UNSPECIFIED)),
        shape,
        container_color,
        content_color,
        elevation.tonal_elevation(),
        Dp(0.0),
        None,
        Some(click),
        compose_runtime::callback(move || {
            let content = content.clone();
            CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(label), || {
                let content = content.clone();
                Box(
                    Modifier,
                    Alignment::CENTER,
                    false,
                    compose_runtime::callback(move |_| content()),
                );
            });
        }),
    );
}

/// A floating action button, as Compose's `FloatingActionButton`: 56 dp, the large shape, in the
/// primary container's colors, on a 6 dp shadow.
#[composable]
pub fn FloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    fab(
        on_click,
        modifier,
        shape,
        container_color,
        content_color,
        elevation,
        interaction_source,
        56.dp(),
        content,
    );
}

/// A small floating action button, as Compose's `SmallFloatingActionButton`: 40 dp, the medium
/// shape.
#[composable]
pub fn SmallFloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::small_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    fab(
        on_click,
        modifier,
        shape,
        container_color,
        content_color,
        elevation,
        interaction_source,
        40.dp(),
        content,
    );
}

/// A large floating action button, as Compose's `LargeFloatingActionButton`: 96 dp, the
/// extra-large shape (its icon `LARGE_ICON_SIZE`, 36 dp).
#[composable]
pub fn LargeFloatingActionButton(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::FloatingActionButtonDefaults::large_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    fab(
        on_click,
        modifier,
        shape,
        container_color,
        content_color,
        elevation,
        interaction_source,
        96.dp(),
        content,
    );
}

/// An extended floating action button, as Compose's `ExtendedFloatingActionButton(text, icon,
/// …, expanded)`: 56 dp high, the icon 16 dp in, then (while `expanded`) the text 12 dp after
/// it and 20 dp from the end, at least 80 dp wide; collapsed, the icon alone in a 56 dp square.
/// The text slides and fades in and out; it doesn't add to the button's semantics (as
/// Compose's, which clears it: give the icon a description).
#[composable]
pub fn ExtendedFloatingActionButton(
    text: impl Fn() + 'static,
    icon: impl Fn() + 'static,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] expanded: bool,
    #[default(crate::FloatingActionButtonDefaults::extended_fab_shape())] shape: Shape,
    #[default(crate::FloatingActionButtonDefaults::container_color())] container_color: Color,
    #[default(crate::Color::UNSPECIFIED)] content_color: Color,
    #[default(crate::FloatingActionButtonDefaults::elevation())]
    elevation: FloatingActionButtonElevation,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let (start, end) = if expanded { (16.dp(), 20.dp()) } else { (Dp(0.0), Dp(0.0)) };
    let minimum_width = if expanded { 80.dp() } else { 56.dp() };
    fab(
        on_click,
        modifier,
        shape,
        container_color,
        content_color,
        elevation,
        interaction_source,
        56.dp(),
        compose_runtime::callback(move || {
            let (text, icon) = (text.clone(), icon.clone());
            Row(
                Modifier
                    .size_in(SizeSpec::bounds(
                        minimum_width,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                        Dp::UNSPECIFIED,
                    ))
                    .padding_sides(start, Dp(0.0), end, Dp(0.0)),
                if expanded { Arrangement::Start } else { Arrangement::Center },
                Alignment::CENTER_VERTICALLY,
                compose_runtime::callback(move |_| {
                    icon();
                    // The text 12 dp after the icon (the gap shrinking with it), not read.
                    let text = text.clone();
                    AnimatedVisibility(
                        expanded,
                        Modifier.clear_and_set_semantics(|_| {}),
                        fade_in() + expand_horizontally(),
                        fade_out() + shrink_horizontally(),
                        "ExtendedFloatingActionButton",
                        compose_runtime::callback(move || {
                            let text = text.clone();
                            Box(
                                Modifier.padding_sides(12.dp(), Dp(0.0), Dp(0.0), Dp(0.0)),
                                Alignment::CENTER_START,
                                false,
                                compose_runtime::callback(move |_| text()),
                            );
                        }),
                    );
                }),
            );
        }),
    );
}
