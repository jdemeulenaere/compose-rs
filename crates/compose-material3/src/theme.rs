//! `MaterialTheme`: the color scheme, typography and shapes, provided to a subtree through
//! composition locals, as Compose's (Material 3 baseline values).

use std::rc::Rc;

use compose_foundation::{LOCAL_TEXT_SELECTION_COLORS, TextSelectionColors};
use compose_runtime::{ComposeArg, CompositionLocal, CompositionLocalProvider, composable};
use compose_ui::{
    Color, CornerSize, FontWeight, LineHeightAlignment, LineHeightStyle, LineHeightTrim, Shape,
    TextStyle, TextUnit, UnitExt,
};

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
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub inverse_primary: Color,
    pub surface_tint: Color,
    pub scrim: Color,
    pub surface_bright: Color,
    pub surface_dim: Color,
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
        tertiary_container: argb(0xFFFFD8E4),
        on_tertiary_container: argb(0xFF31111D),
        error_container: argb(0xFFF9DEDC),
        on_error_container: argb(0xFF410E0B),
        inverse_surface: argb(0xFF322F35),
        inverse_on_surface: argb(0xFFF5EFF7),
        inverse_primary: argb(0xFFD0BCFF),
        surface_tint: argb(0xFF6750A4),
        scrim: argb(0xFF000000),
        surface_bright: argb(0xFFFEF7FF),
        surface_dim: argb(0xFFDED8E1),
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
        tertiary_container: argb(0xFF633B48),
        on_tertiary_container: argb(0xFFFFD8E4),
        error_container: argb(0xFF8C1D18),
        on_error_container: argb(0xFFF9DEDC),
        inverse_surface: argb(0xFFE6E0E9),
        inverse_on_surface: argb(0xFF322F35),
        inverse_primary: argb(0xFF6750A4),
        surface_tint: argb(0xFFD0BCFF),
        scrim: argb(0xFF000000),
        surface_bright: argb(0xFF3B383E),
        surface_dim: argb(0xFF141218),
    }
}

impl ColorScheme {
    /// The surface color at `elevation`, as `surfaceColorAtElevation`: tinted with the surface
    /// tint, more as it rises.
    pub fn surface_color_at_elevation(&self, elevation: compose_ui::Dp) -> Color {
        if elevation.0 == 0.0 {
            return self.surface;
        }
        let alpha = (4.5 * (elevation.0 + 1.0).ln() + 2.0) / 100.0;
        self.surface_tint.with_alpha(alpha).composite_over(self.surface)
    }

    /// The content color for `background`, as `contentColorFor`: the matching `on` role.
    pub fn content_color_for(&self, background: Color) -> Color {
        let pairs = [
            (self.primary, self.on_primary),
            (self.primary_container, self.on_primary_container),
            (self.secondary, self.on_secondary),
            (self.secondary_container, self.on_secondary_container),
            (self.tertiary, self.on_tertiary),
            (self.tertiary_container, self.on_tertiary_container),
            (self.error, self.on_error),
            (self.error_container, self.on_error_container),
            (self.inverse_surface, self.inverse_on_surface),
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

/// The theme's text styles, as Compose's `Typography` (Material 3's type scale,
/// `TypeScaleTokens`).
#[derive(Clone, PartialEq, Debug)]
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

/// How Material 3's text styles spread their line height, as `DefaultLineHeightStyle`
/// (`TypographyTokens.kt`): centered on the text, neither end trimmed.
pub const DEFAULT_LINE_HEIGHT_STYLE: LineHeightStyle =
    LineHeightStyle::new(LineHeightAlignment::CENTER, LineHeightTrim::None);

/// A type scale entry: its size, line height, tracking (letter spacing, sp) and weight, as
/// `TypeScaleTokens`' `…Size`, `…LineHeight`, `…Tracking` and `…Weight`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TypeScaleToken {
    pub size: f32,
    pub line_height: f32,
    pub tracking: f32,
    pub weight: FontWeight,
}

/// Material 3 1.4.0's type scale (`TypeScaleTokens`, read from its classes): sizes, line
/// heights, tracking and weights.
pub struct TypeScaleTokens {}

const fn token(size: f32, line_height: f32, tracking: f32, weight: FontWeight) -> TypeScaleToken {
    TypeScaleToken { size, line_height, tracking, weight }
}

impl TypeScaleTokens {
    pub const DISPLAY_LARGE: TypeScaleToken = token(57.0, 64.0, -0.2, FontWeight::NORMAL);
    pub const DISPLAY_MEDIUM: TypeScaleToken = token(45.0, 52.0, 0.0, FontWeight::NORMAL);
    pub const DISPLAY_SMALL: TypeScaleToken = token(36.0, 44.0, 0.0, FontWeight::NORMAL);
    pub const HEADLINE_LARGE: TypeScaleToken = token(32.0, 40.0, 0.0, FontWeight::NORMAL);
    pub const HEADLINE_MEDIUM: TypeScaleToken = token(28.0, 36.0, 0.0, FontWeight::NORMAL);
    pub const HEADLINE_SMALL: TypeScaleToken = token(24.0, 32.0, 0.0, FontWeight::NORMAL);
    pub const TITLE_LARGE: TypeScaleToken = token(22.0, 28.0, 0.0, FontWeight::NORMAL);
    pub const TITLE_MEDIUM: TypeScaleToken = token(16.0, 24.0, 0.2, FontWeight::MEDIUM);
    pub const TITLE_SMALL: TypeScaleToken = token(14.0, 20.0, 0.1, FontWeight::MEDIUM);
    pub const BODY_LARGE: TypeScaleToken = token(16.0, 24.0, 0.5, FontWeight::NORMAL);
    pub const BODY_MEDIUM: TypeScaleToken = token(14.0, 20.0, 0.2, FontWeight::NORMAL);
    pub const BODY_SMALL: TypeScaleToken = token(12.0, 16.0, 0.4, FontWeight::NORMAL);
    pub const LABEL_LARGE: TypeScaleToken = token(14.0, 20.0, 0.1, FontWeight::MEDIUM);
    pub const LABEL_MEDIUM: TypeScaleToken = token(12.0, 16.0, 0.5, FontWeight::MEDIUM);
    pub const LABEL_SMALL: TypeScaleToken = token(11.0, 16.0, 0.5, FontWeight::MEDIUM);
}

/// A type scale entry as a text style, as `TypographyTokens`: Material's default text style
/// (`DefaultTextStyle`: its line height style) with the token's values.
const fn style(token: TypeScaleToken) -> TextStyle {
    TextStyle {
        font_size: TextUnit::sp(token.size),
        font_weight: Some(token.weight),
        line_height: TextUnit::sp(token.line_height),
        letter_spacing: TextUnit::sp(token.tracking),
        ..DEFAULT_TEXT_STYLE
    }
}

impl Default for Typography {
    fn default() -> Self {
        Typography {
            display_large: style(TypeScaleTokens::DISPLAY_LARGE),
            display_medium: style(TypeScaleTokens::DISPLAY_MEDIUM),
            display_small: style(TypeScaleTokens::DISPLAY_SMALL),
            headline_large: style(TypeScaleTokens::HEADLINE_LARGE),
            headline_medium: style(TypeScaleTokens::HEADLINE_MEDIUM),
            headline_small: style(TypeScaleTokens::HEADLINE_SMALL),
            title_large: style(TypeScaleTokens::TITLE_LARGE),
            title_medium: style(TypeScaleTokens::TITLE_MEDIUM),
            title_small: style(TypeScaleTokens::TITLE_SMALL),
            body_large: style(TypeScaleTokens::BODY_LARGE),
            body_medium: style(TypeScaleTokens::BODY_MEDIUM),
            body_small: style(TypeScaleTokens::BODY_SMALL),
            label_large: style(TypeScaleTokens::LABEL_LARGE),
            label_medium: style(TypeScaleTokens::LABEL_MEDIUM),
            label_small: style(TypeScaleTokens::LABEL_SMALL),
        }
    }
}

/// The theme's corner shapes, as Compose's `Shapes`.
#[derive(Clone, PartialEq, Debug)]
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
/// The theme's text styles, shared: a read hands out the same styles (an `Rc`), as Compose's
/// reads hand out the same object, so it copies none of them (D1106).
pub static LOCAL_TYPOGRAPHY: CompositionLocal<Rc<Typography>> = CompositionLocal::with_policy(
    "LocalTypography",
    compose_runtime::SharedValueEqualityPolicy,
    default_typography,
);

/// Material's default type scale, made once per thread.
fn default_typography() -> Rc<Typography> {
    thread_local! {
        static DEFAULT: Rc<Typography> = Rc::new(Typography::default());
    }
    DEFAULT.with(Rc::clone)
}
pub static LOCAL_SHAPES: CompositionLocal<Shapes> =
    CompositionLocal::new("LocalShapes", Shapes::default);
/// The color content draws with, as Compose's `LocalContentColor`.
pub static LOCAL_CONTENT_COLOR: CompositionLocal<Color> =
    CompositionLocal::new("LocalContentColor", || Color::BLACK);
/// The style `Text` uses by default, as Compose's `LocalTextStyle`: Material's default text
/// style (`DefaultTextStyle`, with its `DefaultLineHeightStyle`).
pub static LOCAL_TEXT_STYLE: CompositionLocal<TextStyle> =
    CompositionLocal::new("LocalTextStyle", || DEFAULT_TEXT_STYLE);

/// Material's default text style, as `DefaultTextStyle` (`TypographyTokens.kt`, read from
/// material3 1.4.0): `TextStyle.Default` with `DefaultLineHeightStyle` (and, in Compose, no font
/// padding, which compose-rs never adds).
pub const DEFAULT_TEXT_STYLE: TextStyle =
    TextStyle { line_height_style: Some(DEFAULT_LINE_HEIGHT_STYLE), ..TextStyle::DEFAULT };

/// The theme of `content`, as Compose's `MaterialTheme`; what isn't given stays the enclosing
/// theme's. Also a type: `MaterialTheme::color_scheme()`
/// reads the current theme, as Kotlin's `MaterialTheme.colorScheme`.
#[composable]
pub fn MaterialTheme(
    #[default(crate::MaterialTheme::color_scheme())] color_scheme: ColorScheme,
    #[default(crate::MaterialTheme::shapes())] shapes: Shapes,
    #[default(Typography::clone(&crate::MaterialTheme::typography()))] typography: Typography,
    content: impl Fn() + 'static,
) {
    // Compared by value, as Compose's: a typography made anew but equal skips the theme, and
    // the local's policy recomposes no reader for it (D1107).
    let selection_colors = text_selection_colors(&color_scheme);
    let body_large = typography.body_large.clone();
    let typography = Rc::new(typography);
    CompositionLocalProvider(
        (
            LOCAL_COLOR_SCHEME.provides(color_scheme),
            LOCAL_SHAPES.provides(shapes),
            LOCAL_TYPOGRAPHY.provides(typography),
            LOCAL_TEXT_SELECTION_COLORS.provides(selection_colors),
            // The body text style, as Compose's (and no content color: surfaces give it).
            LOCAL_TEXT_STYLE.provides(body_large),
        ),
        || content(),
    );
}

/// The selection colors of a scheme, as Compose's `rememberTextSelectionColors`: the primary
/// color's handles, its highlight at 40 % (`TextSelectionBackgroundOpacity`).
pub(crate) fn text_selection_colors(scheme: &ColorScheme) -> TextSelectionColors {
    TextSelectionColors {
        handle_color: scheme.primary,
        background_color: scheme.primary.with_alpha(TEXT_SELECTION_BACKGROUND_OPACITY),
    }
}

/// The alpha of a selection's highlight, as `TextSelectionBackgroundOpacity`.
const TEXT_SELECTION_BACKGROUND_OPACITY: f32 = 0.4;

/// The current theme, as Kotlin's `MaterialTheme` object (a type only, so the composable keeps
/// the name in the value namespace).
pub struct MaterialTheme {}

impl MaterialTheme {
    pub fn color_scheme() -> ColorScheme {
        LOCAL_COLOR_SCHEME.current()
    }

    /// The theme's text styles, shared (D1106): `MaterialTheme::typography().body_large`
    /// reads one without copying the others.
    pub fn typography() -> Rc<Typography> {
        LOCAL_TYPOGRAPHY.current()
    }

    pub fn shapes() -> Shapes {
        LOCAL_SHAPES.current()
    }
}
