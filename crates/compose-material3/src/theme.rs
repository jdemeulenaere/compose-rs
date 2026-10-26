//! `MaterialTheme`: the color scheme, typography and shapes, provided to a subtree through
//! composition locals, as Compose's (Material 3 baseline values).

use std::rc::Rc;

use compose_foundation::{LOCAL_TEXT_SELECTION_COLORS, TextSelectionColors};
use compose_runtime::{ComposeArg, CompositionLocal, CompositionLocalProvider, composable};
use compose_ui::{
    Color, CornerSize, Dp, FontFamily, FontWeight, LineHeightAlignment, LineHeightStyle,
    LineHeightTrim, Shape, TextStyle, TextUnit, UnitExt,
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
    /// The fixed roles, as `primaryFixed` and the rest: tones that stay the same in the light
    /// and dark schemes.
    pub primary_fixed: Color,
    pub primary_fixed_dim: Color,
    pub on_primary_fixed: Color,
    pub on_primary_fixed_variant: Color,
    pub secondary_fixed: Color,
    pub secondary_fixed_dim: Color,
    pub on_secondary_fixed: Color,
    pub on_secondary_fixed_variant: Color,
    pub tertiary_fixed: Color,
    pub tertiary_fixed_dim: Color,
    pub on_tertiary_fixed: Color,
    pub on_tertiary_fixed_variant: Color,
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
        primary_fixed: argb(0xFFEADDFF),
        primary_fixed_dim: argb(0xFFD0BCFF),
        on_primary_fixed: argb(0xFF21005D),
        on_primary_fixed_variant: argb(0xFF4F378B),
        secondary_fixed: argb(0xFFE8DEF8),
        secondary_fixed_dim: argb(0xFFCCC2DC),
        on_secondary_fixed: argb(0xFF1D192B),
        on_secondary_fixed_variant: argb(0xFF4A4458),
        tertiary_fixed: argb(0xFFFFD8E4),
        tertiary_fixed_dim: argb(0xFFEFB8C8),
        on_tertiary_fixed: argb(0xFF31111D),
        on_tertiary_fixed_variant: argb(0xFF633B48),
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
        primary_fixed: argb(0xFFEADDFF),
        primary_fixed_dim: argb(0xFFD0BCFF),
        on_primary_fixed: argb(0xFF21005D),
        on_primary_fixed_variant: argb(0xFF4F378B),
        secondary_fixed: argb(0xFFE8DEF8),
        secondary_fixed_dim: argb(0xFFCCC2DC),
        on_secondary_fixed: argb(0xFF1D192B),
        on_secondary_fixed_variant: argb(0xFF4A4458),
        tertiary_fixed: argb(0xFFFFD8E4),
        tertiary_fixed_dim: argb(0xFFEFB8C8),
        on_tertiary_fixed: argb(0xFF31111D),
        on_tertiary_fixed_variant: argb(0xFF633B48),
    }
}

/// The light scheme `MaterialExpressiveTheme` takes by default, as
/// `expressiveLightColorScheme()`: the baseline light scheme with darker content on the
/// containers (the palettes' tone 30).
pub const fn expressive_light_color_scheme() -> ColorScheme {
    ColorScheme {
        on_primary_container: argb(0xFF4F378B),
        on_secondary_container: argb(0xFF4A4458),
        on_tertiary_container: argb(0xFF633B48),
        on_error_container: argb(0xFF8C1D18),
        ..light_color_scheme()
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

    /// The content color for `background`, as `contentColorFor`: the matching `on` role (the
    /// first match, in Compose's order), else `Color::UNSPECIFIED`.
    pub fn content_color_for(&self, background: Color) -> Color {
        let pairs = [
            (self.primary, self.on_primary),
            (self.secondary, self.on_secondary),
            (self.tertiary, self.on_tertiary),
            (self.background, self.on_background),
            (self.error, self.on_error),
            (self.primary_container, self.on_primary_container),
            (self.secondary_container, self.on_secondary_container),
            (self.tertiary_container, self.on_tertiary_container),
            (self.error_container, self.on_error_container),
            (self.inverse_surface, self.inverse_on_surface),
            (self.surface, self.on_surface),
            (self.surface_variant, self.on_surface_variant),
            (self.surface_bright, self.on_surface),
            (self.surface_container, self.on_surface),
            (self.surface_container_high, self.on_surface),
            (self.surface_container_highest, self.on_surface),
            (self.surface_container_low, self.on_surface),
            (self.surface_container_lowest, self.on_surface),
            (self.surface_dim, self.on_surface),
            (self.primary_fixed, self.on_primary_fixed),
            (self.primary_fixed_dim, self.on_primary_fixed),
            (self.secondary_fixed, self.on_secondary_fixed),
            (self.secondary_fixed_dim, self.on_secondary_fixed),
            (self.tertiary_fixed, self.on_tertiary_fixed),
            (self.tertiary_fixed_dim, self.on_tertiary_fixed),
        ];
        pairs
            .iter()
            .find(|(color, _)| *color == background)
            .map_or(Color::UNSPECIFIED, |(_, on)| *on)
    }

    /// `background` at `elevation`, as Compose's `applyTonalElevation`: the surface color tinted
    /// by its elevation while `LOCAL_TONAL_ELEVATION_ENABLED`, any other color as it is.
    pub(crate) fn apply_tonal_elevation(&self, background: Color, elevation: Dp) -> Color {
        if background == self.surface && LOCAL_TONAL_ELEVATION_ENABLED.current() {
            self.surface_color_at_elevation(elevation)
        } else {
            background
        }
    }
}

/// The content color for `background` in the current theme, as the composable
/// `contentColorFor`: its scheme's matching role, else `LOCAL_CONTENT_COLOR`.
pub fn content_color_for(background: Color) -> Color {
    MaterialTheme::color_scheme().content_color_for(background).or(LOCAL_CONTENT_COLOR.current())
}

/// Whether surfaces below tint their surface color by their tonal elevation, as Compose's
/// `LocalTonalElevationEnabled` (on by default).
pub static LOCAL_TONAL_ELEVATION_ENABLED: CompositionLocal<bool> =
    CompositionLocal::new("LocalTonalElevationEnabled", || true);

/// The theme's text styles, as Compose's `Typography` (Material 3's type scale,
/// `TypeScaleTokens`), and their emphasized versions. `Typography::default()` is Compose's
/// `Typography()`, [`from_font_family`](Typography::from_font_family) its
/// `Typography(fontFamily)`.
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
    pub display_large_emphasized: TextStyle,
    pub display_medium_emphasized: TextStyle,
    pub display_small_emphasized: TextStyle,
    pub headline_large_emphasized: TextStyle,
    pub headline_medium_emphasized: TextStyle,
    pub headline_small_emphasized: TextStyle,
    pub title_large_emphasized: TextStyle,
    pub title_medium_emphasized: TextStyle,
    pub title_small_emphasized: TextStyle,
    pub body_large_emphasized: TextStyle,
    pub body_medium_emphasized: TextStyle,
    pub body_small_emphasized: TextStyle,
    pub label_large_emphasized: TextStyle,
    pub label_medium_emphasized: TextStyle,
    pub label_small_emphasized: TextStyle,
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
    pub const DISPLAY_LARGE_EMPHASIZED: TypeScaleToken = token(57.0, 64.0, 0.0, FontWeight::MEDIUM);
    pub const DISPLAY_MEDIUM_EMPHASIZED: TypeScaleToken =
        token(45.0, 52.0, 0.0, FontWeight::MEDIUM);
    pub const DISPLAY_SMALL_EMPHASIZED: TypeScaleToken = token(36.0, 44.0, 0.0, FontWeight::MEDIUM);
    pub const HEADLINE_LARGE_EMPHASIZED: TypeScaleToken =
        token(32.0, 40.0, 0.0, FontWeight::MEDIUM);
    pub const HEADLINE_MEDIUM_EMPHASIZED: TypeScaleToken =
        token(28.0, 36.0, 0.0, FontWeight::MEDIUM);
    pub const HEADLINE_SMALL_EMPHASIZED: TypeScaleToken =
        token(24.0, 32.0, 0.0, FontWeight::MEDIUM);
    pub const TITLE_LARGE_EMPHASIZED: TypeScaleToken = token(22.0, 28.0, 0.0, FontWeight::MEDIUM);
    pub const TITLE_MEDIUM_EMPHASIZED: TypeScaleToken = token(16.0, 24.0, 0.15, FontWeight::BOLD);
    pub const TITLE_SMALL_EMPHASIZED: TypeScaleToken = token(14.0, 20.0, 0.1, FontWeight::BOLD);
    pub const BODY_LARGE_EMPHASIZED: TypeScaleToken = token(16.0, 24.0, 0.15, FontWeight::MEDIUM);
    pub const BODY_MEDIUM_EMPHASIZED: TypeScaleToken = token(14.0, 20.0, 0.25, FontWeight::MEDIUM);
    pub const BODY_SMALL_EMPHASIZED: TypeScaleToken = token(12.0, 16.0, 0.4, FontWeight::MEDIUM);
    pub const LABEL_LARGE_EMPHASIZED: TypeScaleToken = token(14.0, 20.0, 0.1, FontWeight::BOLD);
    pub const LABEL_MEDIUM_EMPHASIZED: TypeScaleToken = token(12.0, 16.0, 0.5, FontWeight::BOLD);
    pub const LABEL_SMALL_EMPHASIZED: TypeScaleToken = token(11.0, 16.0, 0.5, FontWeight::BOLD);
}

/// A type scale entry as a text style in `font_family`, as `TypographyTokens`: Material's
/// default text style (`DefaultTextStyle`: its line height style) with the token's values. The
/// tokens' fonts (`TypefaceTokens.Brand` and `Plain`) are both sans serif.
const fn style(token: TypeScaleToken, font_family: FontFamily) -> TextStyle {
    TextStyle {
        font_family: Some(font_family),
        font_size: TextUnit::sp(token.size),
        font_weight: Some(token.weight),
        line_height: TextUnit::sp(token.line_height),
        letter_spacing: TextUnit::sp(token.tracking),
        ..DEFAULT_TEXT_STYLE
    }
}

impl Default for Typography {
    fn default() -> Self {
        Typography::from_font_family(FontFamily::SansSerif)
    }
}

impl Typography {
    /// The type scale in `font_family`, as `Typography(fontFamily)`. Styles given by struct
    /// update keep theirs; [`with_default_font_family`](Self::with_default_font_family) gives
    /// them this one where they have none, as Compose's constructor does.
    pub const fn from_font_family(font_family: FontFamily) -> Typography {
        Typography {
            display_large: style(TypeScaleTokens::DISPLAY_LARGE, font_family),
            display_medium: style(TypeScaleTokens::DISPLAY_MEDIUM, font_family),
            display_small: style(TypeScaleTokens::DISPLAY_SMALL, font_family),
            headline_large: style(TypeScaleTokens::HEADLINE_LARGE, font_family),
            headline_medium: style(TypeScaleTokens::HEADLINE_MEDIUM, font_family),
            headline_small: style(TypeScaleTokens::HEADLINE_SMALL, font_family),
            title_large: style(TypeScaleTokens::TITLE_LARGE, font_family),
            title_medium: style(TypeScaleTokens::TITLE_MEDIUM, font_family),
            title_small: style(TypeScaleTokens::TITLE_SMALL, font_family),
            body_large: style(TypeScaleTokens::BODY_LARGE, font_family),
            body_medium: style(TypeScaleTokens::BODY_MEDIUM, font_family),
            body_small: style(TypeScaleTokens::BODY_SMALL, font_family),
            label_large: style(TypeScaleTokens::LABEL_LARGE, font_family),
            label_medium: style(TypeScaleTokens::LABEL_MEDIUM, font_family),
            label_small: style(TypeScaleTokens::LABEL_SMALL, font_family),
            display_large_emphasized: style(TypeScaleTokens::DISPLAY_LARGE_EMPHASIZED, font_family),
            display_medium_emphasized: style(
                TypeScaleTokens::DISPLAY_MEDIUM_EMPHASIZED,
                font_family,
            ),
            display_small_emphasized: style(TypeScaleTokens::DISPLAY_SMALL_EMPHASIZED, font_family),
            headline_large_emphasized: style(
                TypeScaleTokens::HEADLINE_LARGE_EMPHASIZED,
                font_family,
            ),
            headline_medium_emphasized: style(
                TypeScaleTokens::HEADLINE_MEDIUM_EMPHASIZED,
                font_family,
            ),
            headline_small_emphasized: style(
                TypeScaleTokens::HEADLINE_SMALL_EMPHASIZED,
                font_family,
            ),
            title_large_emphasized: style(TypeScaleTokens::TITLE_LARGE_EMPHASIZED, font_family),
            title_medium_emphasized: style(TypeScaleTokens::TITLE_MEDIUM_EMPHASIZED, font_family),
            title_small_emphasized: style(TypeScaleTokens::TITLE_SMALL_EMPHASIZED, font_family),
            body_large_emphasized: style(TypeScaleTokens::BODY_LARGE_EMPHASIZED, font_family),
            body_medium_emphasized: style(TypeScaleTokens::BODY_MEDIUM_EMPHASIZED, font_family),
            body_small_emphasized: style(TypeScaleTokens::BODY_SMALL_EMPHASIZED, font_family),
            label_large_emphasized: style(TypeScaleTokens::LABEL_LARGE_EMPHASIZED, font_family),
            label_medium_emphasized: style(TypeScaleTokens::LABEL_MEDIUM_EMPHASIZED, font_family),
            label_small_emphasized: style(TypeScaleTokens::LABEL_SMALL_EMPHASIZED, font_family),
        }
    }

    /// Every style without a font family in `font_family`, as `Typography(fontFamily, …)` does
    /// with the styles it's given (`withDefaultFontFamily`).
    pub fn with_default_font_family(mut self, font_family: FontFamily) -> Typography {
        for style in [
            &mut self.display_large,
            &mut self.display_medium,
            &mut self.display_small,
            &mut self.headline_large,
            &mut self.headline_medium,
            &mut self.headline_small,
            &mut self.title_large,
            &mut self.title_medium,
            &mut self.title_small,
            &mut self.body_large,
            &mut self.body_medium,
            &mut self.body_small,
            &mut self.label_large,
            &mut self.label_medium,
            &mut self.label_small,
            &mut self.display_large_emphasized,
            &mut self.display_medium_emphasized,
            &mut self.display_small_emphasized,
            &mut self.headline_large_emphasized,
            &mut self.headline_medium_emphasized,
            &mut self.headline_small_emphasized,
            &mut self.title_large_emphasized,
            &mut self.title_medium_emphasized,
            &mut self.title_small_emphasized,
            &mut self.body_large_emphasized,
            &mut self.body_medium_emphasized,
            &mut self.body_small_emphasized,
            &mut self.label_large_emphasized,
            &mut self.label_medium_emphasized,
            &mut self.label_small_emphasized,
        ] {
            if style.font_family.is_none() {
                style.font_family = Some(font_family);
            }
        }
        self
    }
}

/// The theme's corner shapes, as Compose's `Shapes`; `Shapes::default()` is `Shapes()`
/// (`ShapeDefaults`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shapes {
    pub extra_small: Shape,
    pub small: Shape,
    pub medium: Shape,
    pub large: Shape,
    pub extra_large: Shape,
    pub large_increased: Shape,
    pub extra_large_increased: Shape,
    pub extra_extra_large: Shape,
}

const fn rounded(radius: f32) -> Shape {
    Shape::Rounded([CornerSize::Dp(Dp(radius)); 4])
}

impl Default for Shapes {
    fn default() -> Self {
        Shapes {
            extra_small: ShapeDefaults::EXTRA_SMALL,
            small: ShapeDefaults::SMALL,
            medium: ShapeDefaults::MEDIUM,
            large: ShapeDefaults::LARGE,
            extra_large: ShapeDefaults::EXTRA_LARGE,
            large_increased: ShapeDefaults::LARGE_INCREASED,
            extra_large_increased: ShapeDefaults::EXTRA_LARGE_INCREASED,
            extra_extra_large: ShapeDefaults::EXTRA_EXTRA_LARGE,
        }
    }
}

/// The shapes `Shapes` takes by default, as Compose's `ShapeDefaults` (`ShapeTokens`).
pub struct ShapeDefaults;

impl ShapeDefaults {
    pub const EXTRA_SMALL: Shape = rounded(4.0);
    pub const SMALL: Shape = rounded(8.0);
    pub const MEDIUM: Shape = rounded(12.0);
    pub const LARGE: Shape = rounded(16.0);
    pub const LARGE_INCREASED: Shape = rounded(20.0);
    pub const EXTRA_LARGE: Shape = rounded(28.0);
    pub const EXTRA_LARGE_INCREASED: Shape = rounded(32.0);
    pub const EXTRA_EXTRA_LARGE: Shape = rounded(48.0);
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
        ),
        || {
            // The body text style, as Compose's (and no content color: surfaces give it).
            ProvideTextStyle(body_large.clone(), content.clone());
        },
    );
}

/// `value` merged into the current text style for `content`, as Compose's `ProvideTextStyle`:
/// what it sets wins, the rest stays.
#[composable]
pub fn ProvideTextStyle(value: TextStyle, content: impl Fn() + 'static) {
    let merged = LOCAL_TEXT_STYLE.current().merge(&value);
    CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(merged), || content());
}

/// An expressive theme, as Compose's `MaterialExpressiveTheme` (without its motion scheme,
/// which compose-rs's theme doesn't carry yet): outside another expressive theme, what isn't
/// given is `expressive_light_color_scheme()`, `Shapes::default()` and `Typography::default()`;
/// inside one, the enclosing theme's.
#[composable]
pub fn MaterialExpressiveTheme(
    #[default(None)] color_scheme: Option<ColorScheme>,
    #[default(None)] shapes: Option<Shapes>,
    #[default(None)] typography: Option<Typography>,
    content: impl Fn() + 'static,
) {
    if LOCAL_USING_EXPRESSIVE_THEME.current() {
        MaterialTheme(
            color_scheme.unwrap_or_else(MaterialTheme::color_scheme),
            shapes.unwrap_or_else(MaterialTheme::shapes),
            typography.unwrap_or_else(MaterialTheme::typography),
            content,
        );
    } else {
        CompositionLocalProvider(LOCAL_USING_EXPRESSIVE_THEME.provides(true), || {
            MaterialTheme(
                color_scheme.unwrap_or_else(expressive_light_color_scheme),
                shapes.unwrap_or_default(),
                typography.unwrap_or_default(),
                content.clone(),
            );
        });
    }
}

/// Whether an expressive theme is around, as Compose's `LocalUsingExpressiveTheme`.
static LOCAL_USING_EXPRESSIVE_THEME: CompositionLocal<bool> =
    CompositionLocal::new("LocalUsingExpressiveTheme", || false);

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

/// The current theme's values, as Compose's `MaterialTheme.Values` (what
/// `MaterialTheme.LocalMaterialTheme` holds; compose-rs keeps the theme in a local each).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MaterialThemeValues {
    pub color_scheme: ColorScheme,
    pub typography: Typography,
    pub shapes: Shapes,
}

impl Default for MaterialThemeValues {
    fn default() -> Self {
        MaterialThemeValues {
            color_scheme: light_color_scheme(),
            typography: Typography::default(),
            shapes: Shapes::default(),
        }
    }
}

impl MaterialTheme {
    /// The current theme's values, as `MaterialTheme.LocalMaterialTheme.current`.
    pub fn values() -> MaterialThemeValues {
        MaterialThemeValues {
            color_scheme: LOCAL_COLOR_SCHEME.current(),
            typography: LOCAL_TYPOGRAPHY.current(),
            shapes: LOCAL_SHAPES.current(),
        }
    }

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
