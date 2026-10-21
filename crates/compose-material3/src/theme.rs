//! `MaterialTheme`: the color scheme, typography and shapes, provided to a subtree through
//! composition locals, as Compose's (Material 3 baseline values).

use compose_runtime::{ComposeArg, CompositionLocal, CompositionLocalProvider, composable};
use compose_ui::{Color, CornerSize, FontWeight, Shape, Sp, TextStyle, UnitExt};

/// The theme's colors, as Compose's `ColorScheme` (the roles components use).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ColorScheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub error: Color,
    pub on_error: Color,
    pub background: Color,
    pub on_background: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub surface_variant: Color,
    pub on_surface_variant: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub outline: Color,
    pub outline_variant: Color,
}

const fn argb(value: u32) -> Color {
    Color::from_argb(value)
}

/// The Material 3 baseline light scheme, as `lightColorScheme()`.
pub const fn light_color_scheme() -> ColorScheme {
    ColorScheme {
        primary: argb(0xFF6750A4),
        on_primary: argb(0xFFFFFFFF),
        primary_container: argb(0xFFEADDFF),
        on_primary_container: argb(0xFF21005D),
        secondary: argb(0xFF625B71),
        on_secondary: argb(0xFFFFFFFF),
        secondary_container: argb(0xFFE8DEF8),
        on_secondary_container: argb(0xFF1D192B),
        tertiary: argb(0xFF7D5260),
        on_tertiary: argb(0xFFFFFFFF),
        error: argb(0xFFB3261E),
        on_error: argb(0xFFFFFFFF),
        background: argb(0xFFFEF7FF),
        on_background: argb(0xFF1D1B20),
        surface: argb(0xFFFEF7FF),
        on_surface: argb(0xFF1D1B20),
        surface_variant: argb(0xFFE7E0EC),
        on_surface_variant: argb(0xFF49454F),
        surface_container_lowest: argb(0xFFFFFFFF),
        surface_container_low: argb(0xFFF7F2FA),
        surface_container: argb(0xFFF3EDF7),
        surface_container_high: argb(0xFFECE6F0),
        surface_container_highest: argb(0xFFE6E0E9),
        outline: argb(0xFF79747E),
        outline_variant: argb(0xFFCAC4D0),
    }
}

/// The Material 3 baseline dark scheme, as `darkColorScheme()`.
pub const fn dark_color_scheme() -> ColorScheme {
    ColorScheme {
        primary: argb(0xFFD0BCFF),
        on_primary: argb(0xFF381E72),
        primary_container: argb(0xFF4F378B),
        on_primary_container: argb(0xFFEADDFF),
        secondary: argb(0xFFCCC2DC),
        on_secondary: argb(0xFF332D41),
        secondary_container: argb(0xFF4A4458),
        on_secondary_container: argb(0xFFE8DEF8),
        tertiary: argb(0xFFEFB8C8),
        on_tertiary: argb(0xFF492532),
        error: argb(0xFFF2B8B5),
        on_error: argb(0xFF601410),
        background: argb(0xFF141218),
        on_background: argb(0xFFE6E0E9),
        surface: argb(0xFF141218),
        on_surface: argb(0xFFE6E0E9),
        surface_variant: argb(0xFF49454F),
        on_surface_variant: argb(0xFFCAC4D0),
        surface_container_lowest: argb(0xFF0F0D13),
        surface_container_low: argb(0xFF1D1B20),
        surface_container: argb(0xFF211F26),
        surface_container_high: argb(0xFF2B2930),
        surface_container_highest: argb(0xFF36343B),
        outline: argb(0xFF938F99),
        outline_variant: argb(0xFF49454F),
    }
}

impl ColorScheme {
    /// The surface color at `elevation`, as `surfaceColorAtElevation`: tinted with the primary
    /// color, more as it rises.
    pub fn surface_color_at_elevation(&self, elevation: compose_ui::Dp) -> Color {
        if elevation.0 <= 0.0 {
            return self.surface;
        }
        let alpha = (4.5 * (elevation.0 + 1.0).ln() + 2.0) / 100.0;
        self.primary.with_alpha(alpha).composite_over(self.surface)
    }

    /// The content color for `background`, as `contentColorFor`: the matching `on` role.
    pub fn content_color_for(&self, background: Color) -> Color {
        let pairs = [
            (self.primary, self.on_primary),
            (self.primary_container, self.on_primary_container),
            (self.secondary, self.on_secondary),
            (self.secondary_container, self.on_secondary_container),
            (self.tertiary, self.on_tertiary),
            (self.error, self.on_error),
            (self.background, self.on_background),
            (self.surface, self.on_surface),
            (self.surface_variant, self.on_surface_variant),
            (self.surface_container_lowest, self.on_surface),
            (self.surface_container_low, self.on_surface),
            (self.surface_container, self.on_surface),
            (self.surface_container_high, self.on_surface),
            (self.surface_container_highest, self.on_surface),
        ];
        pairs
            .iter()
            .find(|(color, _)| *color == background)
            .map_or(Color::UNSPECIFIED, |(_, on)| *on)
    }
}

/// The theme's text styles, as Compose's `Typography` (Material 3 baseline sizes).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Typography {
    pub display_large: TextStyle,
    pub display_medium: TextStyle,
    pub display_small: TextStyle,
    pub headline_large: TextStyle,
    pub headline_medium: TextStyle,
    pub headline_small: TextStyle,
    pub title_large: TextStyle,
    pub title_medium: TextStyle,
    pub title_small: TextStyle,
    pub body_large: TextStyle,
    pub body_medium: TextStyle,
    pub body_small: TextStyle,
    pub label_large: TextStyle,
    pub label_medium: TextStyle,
    pub label_small: TextStyle,
}

const fn style(size: f32, line_height: f32, weight: FontWeight) -> TextStyle {
    TextStyle {
        font_size: Sp(size),
        font_weight: Some(weight),
        line_height: Sp(line_height),
        ..TextStyle::DEFAULT
    }
}

impl Default for Typography {
    fn default() -> Self {
        let (normal, medium) = (FontWeight::NORMAL, FontWeight::MEDIUM);
        Typography {
            display_large: style(57.0, 64.0, normal),
            display_medium: style(45.0, 52.0, normal),
            display_small: style(36.0, 44.0, normal),
            headline_large: style(32.0, 40.0, normal),
            headline_medium: style(28.0, 36.0, normal),
            headline_small: style(24.0, 32.0, normal),
            title_large: style(22.0, 28.0, normal),
            title_medium: style(16.0, 24.0, medium),
            title_small: style(14.0, 20.0, medium),
            body_large: style(16.0, 24.0, normal),
            body_medium: style(14.0, 20.0, normal),
            body_small: style(12.0, 16.0, normal),
            label_large: style(14.0, 20.0, medium),
            label_medium: style(12.0, 16.0, medium),
            label_small: style(11.0, 16.0, medium),
        }
    }
}

/// The theme's corner shapes, as Compose's `Shapes`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shapes {
    pub extra_small: Shape,
    pub small: Shape,
    pub medium: Shape,
    pub large: Shape,
    pub extra_large: Shape,
}

fn rounded(radius: f32) -> Shape {
    Shape::Rounded([CornerSize::Dp(radius.dp()); 4])
}

impl Default for Shapes {
    fn default() -> Self {
        Shapes {
            extra_small: rounded(4.0),
            small: rounded(8.0),
            medium: rounded(12.0),
            large: rounded(16.0),
            extra_large: rounded(28.0),
        }
    }
}

macro_rules! compose_arg_by_eq {
    ($($type:ty),*) => {
        $(impl ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

compose_arg_by_eq!(ColorScheme, Typography, Shapes);

pub static LOCAL_COLOR_SCHEME: CompositionLocal<ColorScheme> =
    CompositionLocal::new("LocalColorScheme", light_color_scheme);
pub static LOCAL_TYPOGRAPHY: CompositionLocal<Typography> =
    CompositionLocal::new("LocalTypography", Typography::default);
pub static LOCAL_SHAPES: CompositionLocal<Shapes> =
    CompositionLocal::new("LocalShapes", Shapes::default);
/// The color content draws with, as Compose's `LocalContentColor`.
pub static LOCAL_CONTENT_COLOR: CompositionLocal<Color> =
    CompositionLocal::new("LocalContentColor", || Color::BLACK);
/// The style `Text` uses by default, as Compose's `LocalTextStyle`.
pub static LOCAL_TEXT_STYLE: CompositionLocal<TextStyle> =
    CompositionLocal::new("LocalTextStyle", || TextStyle::DEFAULT);

/// The theme of `content`, as Compose's `MaterialTheme`; what isn't given stays the enclosing
/// theme's. Also a type: `MaterialTheme::color_scheme()`
/// reads the current theme, as Kotlin's `MaterialTheme.colorScheme`.
#[composable]
pub fn MaterialTheme(
    #[default(crate::MaterialTheme::color_scheme())] color_scheme: ColorScheme,
    #[default(crate::MaterialTheme::shapes())] shapes: Shapes,
    #[default(crate::MaterialTheme::typography())] typography: Typography,
    content: impl Fn() + 'static,
) {
    CompositionLocalProvider(LOCAL_COLOR_SCHEME.provides(color_scheme), || {
        CompositionLocalProvider(LOCAL_SHAPES.provides(shapes), || {
            CompositionLocalProvider(LOCAL_TYPOGRAPHY.provides(typography), || {
                // The body text style, as Compose's (and no content color: surfaces give it).
                CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(typography.body_large), || {
                    content()
                });
            });
        });
    });
}

/// The current theme, as Kotlin's `MaterialTheme` object (a type only, so the composable keeps
/// the name in the value namespace).
pub struct MaterialTheme {}

impl MaterialTheme {
    pub fn color_scheme() -> ColorScheme {
        LOCAL_COLOR_SCHEME.current()
    }

    pub fn typography() -> Typography {
        LOCAL_TYPOGRAPHY.current()
    }

    pub fn shapes() -> Shapes {
        LOCAL_SHAPES.current()
    }
}
