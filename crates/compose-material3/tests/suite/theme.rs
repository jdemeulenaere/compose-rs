//! The theme's API against Compose's tests: `TypographyTest.kt` (the font family
//! constructor), `ColorSchemeTest.kt` (switching schemes, the baseline schemes' contrast),
//! `ColorSchemeScreenshotTest.kt`'s fixed roles and `ShapesScreenshotTest.kt` (each theme shape
//! on a card, checked as the card's corners), and `MaterialExpressiveTheme`'s defaults.

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, CornerSize, FontFamily, FontWeight, Modifier, Shape, TextStyle, UnitExt};

// ---- Typography -------------------------------------------------------------------------------

/// Every style of a typography, the emphasized ones too.
fn styles(typography: &Typography) -> [TextStyle; 30] {
    let t = typography;
    [
        t.display_large,
        t.display_medium,
        t.display_small,
        t.headline_large,
        t.headline_medium,
        t.headline_small,
        t.title_large,
        t.title_medium,
        t.title_small,
        t.body_large,
        t.body_medium,
        t.body_small,
        t.label_large,
        t.label_medium,
        t.label_small,
        t.display_large_emphasized,
        t.display_medium_emphasized,
        t.display_small_emphasized,
        t.headline_large_emphasized,
        t.headline_medium_emphasized,
        t.headline_small_emphasized,
        t.title_large_emphasized,
        t.title_medium_emphasized,
        t.title_small_emphasized,
        t.body_large_emphasized,
        t.body_medium_emphasized,
        t.body_small_emphasized,
        t.label_large_emphasized,
        t.label_medium_emphasized,
        t.label_small_emphasized,
    ]
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TypographyTest.kt, typography_defaultFontFamily
#[test]
fn the_default_typography_is_in_the_tokens_sans_serif() {
    for style in styles(&Typography::default()) {
        assert_eq!(style.font_family, Some(FontFamily::SansSerif), "{style:?}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TypographyTest.kt, typography_customFontFamily
#[test]
fn a_typography_from_a_font_family_has_it_in_every_style() {
    for style in styles(&Typography::from_font_family(FontFamily::Cursive)) {
        assert_eq!(style.font_family, Some(FontFamily::Cursive), "{style:?}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TypographyTest.kt, typography_customFontFamily_overrideDefault
#[test]
fn a_style_given_with_its_own_font_family_keeps_it() {
    let typography = Typography {
        label_medium: TextStyle { font_family: Some(FontFamily::SansSerif), ..TextStyle::DEFAULT },
        ..Typography::from_font_family(FontFamily::Cursive)
    }
    .with_default_font_family(FontFamily::Cursive);
    assert_eq!(typography.label_medium.font_family, Some(FontFamily::SansSerif));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TypographyTest.kt, typography_nullValuesWithCustomFont_fallbackToCustomFontTokens
#[test]
fn a_style_not_given_is_the_tokens_in_the_font_family() {
    let typography = Typography::from_font_family(FontFamily::Cursive);
    assert_eq!(typography.display_large.font_family, Some(FontFamily::Cursive));
    assert_eq!(typography.display_large.font_size, 57.sp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TypographyTest.kt, typography_customFontFamily_mergesWithNullFontFamilyStyles
#[test]
fn a_style_given_without_a_font_family_takes_the_typographys() {
    let typography = Typography {
        display_large: TextStyle { font_weight: Some(FontWeight::BOLD), ..TextStyle::DEFAULT },
        ..Typography::from_font_family(FontFamily::Cursive)
    }
    .with_default_font_family(FontFamily::Cursive);
    assert_eq!(typography.display_large.font_family, Some(FontFamily::Cursive));
    assert_eq!(typography.display_large.font_weight, Some(FontWeight::BOLD));
}

// ---- Color schemes ----------------------------------------------------------------------------

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ColorSchemeTest.kt, switchingBetweenColors
#[test]
fn switching_schemes_gives_the_new_one_and_leaves_the_baselines_as_they_were() {
    thread_local! { static READ: Cell<Option<ColorScheme>> = const { Cell::new(None) }; }
    let light = light_color_scheme();
    let dark = dark_color_scheme();
    let scheme = mutable_state_of(light);
    let handle = scheme.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!(color_scheme = handle.get(), {
            READ.with(|read| read.set(Some(MaterialTheme::color_scheme())));
        });
    });
    let read = || READ.with(Cell::get).expect("composed");
    for (next, expected) in [(dark, light), (light, dark), (light, light)] {
        assert_eq!(read(), expected);
        assert_eq!((light, dark), (light_color_scheme(), dark_color_scheme()));
        scheme.set(next);
        ui.wait_for_idle();
    }
}

/// A color's relative luminance (sRGB), as Compose's `Color.luminance()`.
fn luminance(color: Color) -> f32 {
    let linear = |channel: u8| {
        let value = channel as f32 / 255.0;
        if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(color.red()) + 0.7152 * linear(color.green()) + 0.0722 * linear(color.blue())
}

/// As the tests' `calculateContrastRatio`.
fn contrast(foreground: Color, background: Color) -> f32 {
    let (a, b) = (luminance(foreground) + 0.05, luminance(background) + 0.05);
    a.max(b) / a.min(b)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ColorSchemeTest.kt, baselineContentContrast
#[test]
fn the_baseline_schemes_content_contrasts_at_least_3_to_1() {
    for s in [light_color_scheme(), dark_color_scheme(), expressive_light_color_scheme()] {
        let pairs = [
            (s.on_primary, s.primary),
            (s.on_secondary, s.secondary),
            (s.on_tertiary, s.tertiary),
            (s.on_primary_container, s.primary_container),
            (s.on_secondary_container, s.secondary_container),
            (s.on_tertiary_container, s.tertiary_container),
            (s.on_error, s.error),
            (s.on_error_container, s.error_container),
            (s.on_surface, s.surface),
            (s.on_surface, s.surface_container),
            (s.on_surface, s.surface_container_high),
            (s.on_surface, s.surface_container_highest),
            (s.on_surface, s.surface_container_low),
            (s.on_surface, s.surface_container_lowest),
            (s.on_surface, s.surface_dim),
            (s.on_surface, s.surface_bright),
            (s.inverse_on_surface, s.inverse_surface),
            (s.on_primary_fixed, s.primary_fixed),
            (s.on_primary_fixed_variant, s.primary_fixed),
            (s.on_primary_fixed, s.primary_fixed_dim),
            (s.on_primary_fixed_variant, s.primary_fixed_dim),
            (s.on_secondary_fixed, s.secondary_fixed),
            (s.on_secondary_fixed_variant, s.secondary_fixed),
            (s.on_secondary_fixed, s.secondary_fixed_dim),
            (s.on_secondary_fixed_variant, s.secondary_fixed_dim),
            (s.on_tertiary_fixed, s.tertiary_fixed),
            (s.on_tertiary_fixed_variant, s.tertiary_fixed),
            (s.on_tertiary_fixed, s.tertiary_fixed_dim),
            (s.on_tertiary_fixed_variant, s.tertiary_fixed_dim),
        ];
        for (index, (on, background)) in pairs.into_iter().enumerate() {
            assert!(contrast(on, background) >= 3.0, "pair {index}: {on:?} on {background:?}");
        }
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ColorSchemeScreenshotTest.kt, fixedColors
#[test]
fn the_fixed_roles_are_the_same_tones_light_and_dark() {
    // The palettes' tones 90 and 80 for the fixed roles, 10 and 30 for their content, in every
    // baseline scheme (`ColorLightTokens`, `ColorDarkTokens`).
    let argb = Color::from_argb;
    let expected = [
        argb(0xFFEADDFF),
        argb(0xFFD0BCFF),
        argb(0xFF21005D),
        argb(0xFF4F378B),
        argb(0xFFE8DEF8),
        argb(0xFFCCC2DC),
        argb(0xFF1D192B),
        argb(0xFF4A4458),
        argb(0xFFFFD8E4),
        argb(0xFFEFB8C8),
        argb(0xFF31111D),
        argb(0xFF633B48),
    ];
    for scheme in [light_color_scheme(), expressive_light_color_scheme(), dark_color_scheme()] {
        thread_local! { static READ: Cell<Option<ColorScheme>> = const { Cell::new(None) }; }
        let _ui = compose_test(move || {
            MaterialTheme!(color_scheme = scheme, {
                READ.with(|read| read.set(Some(MaterialTheme::color_scheme())));
            });
        });
        let s = READ.with(Cell::get).expect("composed");
        let fixed = [
            s.primary_fixed,
            s.primary_fixed_dim,
            s.on_primary_fixed,
            s.on_primary_fixed_variant,
            s.secondary_fixed,
            s.secondary_fixed_dim,
            s.on_secondary_fixed,
            s.on_secondary_fixed_variant,
            s.tertiary_fixed,
            s.tertiary_fixed_dim,
            s.on_tertiary_fixed,
            s.on_tertiary_fixed_variant,
        ];
        assert_eq!(fixed, expected);
    }
}

#[test]
fn the_fixed_roles_have_their_content_colors() {
    let s = light_color_scheme();
    assert_eq!(s.content_color_for(s.primary_fixed_dim), s.on_primary_fixed);
    assert_eq!(s.content_color_for(s.tertiary_fixed), s.on_tertiary_fixed);
    assert_eq!(s.content_color_for(s.surface_dim), s.on_surface);
    assert_eq!(s.content_color_for(Color::from_argb(0xFF123456)), Color::UNSPECIFIED);
}

// ---- Shapes -----------------------------------------------------------------------------------

/// The corners of a 200 dp card in `shape`, as `ShapesScreenshotTest`'s `TestCard`.
fn card_corners(shape: fn(&Shapes) -> Shape) -> [f32; 4] {
    let mut ui = ComposeTestRule::sized(compose_ui::IntSize::new(300, 300), move || {
        MaterialTheme!({
            Card!(shape = shape(&MaterialTheme::shapes()), |_: ColumnScope| {
                Box!(modifier = Modifier.size(200.dp()));
            });
        });
    });
    let list = ui.host().display_list();
    list.ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawRoundRect(rect, radii, _) if rect.width() == 200.0 => Some(*radii),
            DrawOp::DrawRect(rect, _) if rect.width() == 200.0 => Some([0.0; 4]),
            _ => None,
        })
        .expect("the card's container")
}

fn rounded(radius: f32) -> Shape {
    Shape::Rounded([CornerSize::Dp(radius.dp()); 4])
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, extraSmallShape
#[test]
fn an_extra_small_card_has_4_dp_corners() {
    assert_eq!(card_corners(|shapes| shapes.extra_small), [4.0; 4]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, smallShape
#[test]
fn a_small_card_has_8_dp_corners() {
    assert_eq!(card_corners(|shapes| shapes.small), [8.0; 4]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, mediumShape
#[test]
fn the_medium_golden_card_is_small() {
    // Compose's test draws `shapes.small` for its medium golden.
    assert_eq!(card_corners(|shapes| shapes.small), [8.0; 4]);
    assert_eq!(Shapes::default().medium, rounded(12.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, largeShape
#[test]
fn a_large_card_has_16_dp_corners() {
    assert_eq!(card_corners(|shapes| shapes.large), [16.0; 4]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, largeIncreasedShape
#[test]
fn a_large_increased_card_has_20_dp_corners() {
    assert_eq!(card_corners(|shapes| shapes.large_increased), [20.0; 4]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, extraLargeShape
#[test]
fn an_extra_large_card_has_28_dp_corners() {
    assert_eq!(card_corners(|shapes| shapes.extra_large), [28.0; 4]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, extraLargeIncreasedShape
#[test]
fn an_extra_large_increased_card_has_32_dp_corners() {
    assert_eq!(card_corners(|shapes| shapes.extra_large_increased), [32.0; 4]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ShapesScreenshotTest.kt, extraExtraLargeShape
#[test]
fn an_extra_extra_large_card_has_48_dp_corners() {
    assert_eq!(card_corners(|shapes| shapes.extra_extra_large), [48.0; 4]);
    assert_eq!(ShapeDefaults::EXTRA_EXTRA_LARGE, rounded(48.0));
}

// ---- The expressive theme ---------------------------------------------------------------------

/// `MaterialExpressiveTheme` outside another one takes the expressive light scheme; inside one,
/// the enclosing theme's (`MaterialTheme.kt`'s `LocalUsingExpressiveTheme`).
#[test]
fn an_expressive_theme_defaults_to_the_expressive_scheme_and_then_inherits() {
    thread_local! {
        static OUTER: Cell<Option<ColorScheme>> = const { Cell::new(None) };
        static INNER: Cell<Option<ColorScheme>> = const { Cell::new(None) };
    }
    let _ui = compose_test(|| {
        MaterialExpressiveTheme!({
            OUTER.with(|read| read.set(Some(MaterialTheme::color_scheme())));
            MaterialTheme!(color_scheme = dark_color_scheme(), {
                MaterialExpressiveTheme!({
                    INNER.with(|read| read.set(Some(MaterialTheme::color_scheme())));
                });
            });
        });
    });
    assert_eq!(OUTER.with(Cell::get), Some(expressive_light_color_scheme()));
    assert_eq!(INNER.with(Cell::get), Some(dark_color_scheme()));
}
