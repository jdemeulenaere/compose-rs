//! Compose's font model: `Font`, `FontFamily`, `FontVariation` and `FontMatcher`. A resource font's
//! id is its bytes here (D1119): fonts in a test differ by a byte slice of their own.

use compose_ui::platform::FontMatcher;
use compose_ui::{
    Density, DeviceFontFamilyName, Font, FontFamily, FontLoadingStrategy, FontOptions, FontStyle,
    FontVariation, FontVariationSetting, FontVariationSettings, FontWeight, Typeface, UnitExt,
};

const RESOURCE: &[u8] = b"a font";
const FALLBACK_RESOURCE: &[u8] = b"its fallback";

fn resource(weight: FontWeight, style: FontStyle) -> Font {
    Font::from_bytes(RESOURCE, FontOptions { weight, style, ..FontOptions::default() })
}

fn hash_of(font: &Font) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    font.hash(&mut hasher);
    hasher.finish()
}

// ---- FontTest --------------------------------------------------------------------------------

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontTest.kt, default values
#[test]
fn a_font_is_normal_by_default() {
    let font = Font::from_bytes(RESOURCE, FontOptions::default());
    assert_eq!(font.weight(), FontWeight::NORMAL);
    assert_eq!(font.style(), FontStyle::Normal);
    assert_eq!(font.loading_strategy(), FontLoadingStrategy::Blocking);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontTest.kt, two equal font declarations are equal
#[test]
fn fonts_declared_alike_are_equal() {
    assert_eq!(
        resource(FontWeight::W900, FontStyle::Italic),
        resource(FontWeight::W900, FontStyle::Italic)
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontTest.kt, two non equal font declarations are not equal
#[test]
fn fonts_of_another_weight_are_not_equal() {
    assert_ne!(
        resource(FontWeight::W900, FontStyle::Italic),
        resource(FontWeight::W800, FontStyle::Italic)
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontTest.kt, asFontFamilyList returns a FontFamily
#[test]
fn a_font_makes_a_family_of_itself() {
    let font = resource(FontWeight::W900, FontStyle::Italic);
    let family = font.to_font_family();
    let FontFamily::FontList(list) = &family else { panic!("a font list: {family:?}") };
    assert_eq!(list.fonts(), std::slice::from_ref(&font));
}

#[test]
fn the_same_bytes_elsewhere_are_the_same_font() {
    let copy: std::sync::Arc<[u8]> = std::sync::Arc::from(RESOURCE);
    let shared = Font::from_shared_bytes(copy, FontOptions::default());
    assert_eq!(shared, Font::from_bytes(RESOURCE, FontOptions::default()));
    assert_eq!(hash_of(&shared), hash_of(&Font::from_bytes(RESOURCE, FontOptions::default())));
    assert_ne!(shared, Font::from_bytes(FALLBACK_RESOURCE, FontOptions::default()));
}

#[test]
fn fonts_from_elsewhere_are_equal_by_where_they_come_from() {
    let file = |path: &str| Font::from_file(path, FontOptions::default());
    assert_eq!(file("/a.ttf"), file("/a.ttf"));
    assert_ne!(file("/a.ttf"), file("/b.ttf"));
    let asset = |path: &'static str| Font::from_asset(path, FontOptions::default());
    assert_eq!(asset("a.ttf"), asset("a.ttf"));
    assert_ne!(asset("/a.ttf"), file("/a.ttf"));
}

// ---- ResourceFontVariationSettingsTest --------------------------------------------------------

fn varied(grade: i32) -> Font {
    Font::from_bytes(
        RESOURCE,
        FontOptions {
            weight: FontWeight::new(100),
            style: FontStyle::Italic,
            variation_settings: Some(FontVariationSettings::new([FontVariation::grade(grade)])),
            ..FontOptions::default()
        },
    )
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/ResourceFontVariationSettingsTest.kt, resourceFont_acceptsVariationSettings
#[test]
fn a_font_takes_variation_settings() {
    assert_eq!(varied(3).variation_settings().settings(), &[FontVariation::grade(3)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/ResourceFontVariationSettingsTest.kt, resourceFont_usesVariationSettingsInEquals
#[test]
fn variation_settings_make_another_font() {
    assert_ne!(varied(3), varied(4));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/ResourceFontVariationSettingsTest.kt, resourceFont_usesVariationSettingsInHashCode
#[test]
fn variation_settings_make_another_hash() {
    assert_ne!(hash_of(&varied(3)), hash_of(&varied(4)));
}

#[test]
fn a_fonts_variation_settings_default_to_its_weight_and_style() {
    let font = resource(FontWeight::W700, FontStyle::Italic);
    assert_eq!(
        font.variation_settings(),
        &FontVariation::settings(FontWeight::W700, FontStyle::Italic, [])
    );
    // As Compose's `Font(DeviceFontFamilyName)`: none unless given.
    let device = Font::from_device_family(
        DeviceFontFamilyName::new("sans-serif"),
        FontOptions { weight: FontWeight::W700, ..FontOptions::default() },
    );
    assert!(device.variation_settings().settings().is_empty());
    assert_eq!(device.loading_strategy(), FontLoadingStrategy::OptionalLocal);
}

#[test]
#[should_panic(expected = "name may not be empty")]
fn a_device_family_needs_a_name() {
    DeviceFontFamilyName::new("");
}

// ---- FontFamilyTest, LoadedFontFamilyTest -----------------------------------------------------

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontFamilyTest.kt, cannot be instantiated with empty font list
#[test]
#[should_panic(expected = "At least one font should be passed to FontFamily")]
fn a_family_needs_a_font() {
    FontFamily::of(std::iter::empty());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontFamilyTest.kt, two equal family declarations are equal
#[test]
fn families_declared_alike_are_equal() {
    assert_eq!(
        FontFamily::of([resource(FontWeight::W900, FontStyle::Italic)]),
        FontFamily::of([resource(FontWeight::W900, FontStyle::Italic)])
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontFamilyTest.kt, two non equal family declarations are not equal
#[test]
fn families_of_other_fonts_are_not_equal() {
    assert_ne!(
        FontFamily::of([resource(FontWeight::W900, FontStyle::Italic)]),
        FontFamily::of([resource(FontWeight::W800, FontStyle::Italic)])
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontFamilyTest.kt, can add fallback font at same weight and style
#[test]
fn a_family_can_have_a_fallback_at_the_same_weight_and_style() {
    FontFamily::of([
        resource(FontWeight::W900, FontStyle::Italic),
        resource(FontWeight::W900, FontStyle::Italic),
    ]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/LoadedFontFamilyTest.kt, testCreateAndGetTypeface
#[test]
fn a_family_of_a_typeface_keeps_it() {
    let typeface = Typeface::new("a typeface");
    let family = FontFamily::from_typeface(typeface.clone());
    assert_eq!(family, FontFamily::Loaded(typeface));
}

#[test]
fn typefaces_are_equal_by_identity() {
    let typeface = Typeface::new(1);
    assert_eq!(typeface, typeface.clone());
    assert_ne!(typeface, Typeface::new(1));
    assert_eq!(typeface.downcast_ref::<i32>(), Some(&1));
    assert_eq!(typeface.downcast_ref::<u8>(), None);
}

// ---- FontVariationTest ------------------------------------------------------------------------

fn value_of(setting: &FontVariationSetting) -> f32 {
    setting.to_variation_value(None)
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, ital_setsItal
#[test]
fn italic_sets_its_axis() {
    let setting = FontVariation::italic(0.3);
    assert_eq!(setting.axis_name(), "ital");
    assert_eq!(value_of(&setting), 0.3);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, ital_throws_whenTooLow
#[test]
#[should_panic(expected = "'ital' must be in 0.0f..1.0f")]
fn italic_below_zero_panics() {
    FontVariation::italic(-0.1);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, ital_throws_whenTooHigh
#[test]
#[should_panic(expected = "'ital' must be in 0.0f..1.0f")]
fn italic_above_one_panics() {
    FontVariation::italic(1.1);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Opsz_sets
#[test]
fn optical_sizing_sets_its_axis() {
    let setting = FontVariation::optical_sizing(5.sp());
    assert_eq!(setting.axis_name(), "opsz");
    assert_eq!(setting.to_variation_value(Some(&Density::new(1.0))), 5.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Opsz_convertsWithDensity
#[test]
fn optical_sizing_scales_with_the_font_scale_only() {
    let setting = FontVariation::optical_sizing(5.sp());
    let density = Density { density: 3.0, font_scale: 2.0 };
    assert_eq!(setting.to_variation_value(Some(&density)), 10.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Opsz_throws_whenTooLow
#[test]
#[should_panic(expected = "'opsz' must be provided in sp units")]
fn optical_sizing_in_em_panics() {
    FontVariation::optical_sizing(1.0.em());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Opsz_needsDensity
#[test]
fn optical_sizing_needs_a_density() {
    assert!(FontVariation::optical_sizing(5.sp()).needs_density());
    assert!(!FontVariation::weight(100).needs_density());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Slnt_sets
#[test]
fn slant_sets_its_axis() {
    let setting = FontVariation::slant(-30.0);
    assert_eq!(setting.axis_name(), "slnt");
    assert_eq!(value_of(&setting), -30.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Slnt_throws_whenTooSmall
#[test]
#[should_panic(expected = "'slnt' must be in -90f..90f")]
fn slant_below_its_range_panics() {
    FontVariation::slant(-91.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Slnt_throws_whenTooBig
#[test]
#[should_panic(expected = "'slnt' must be in -90f..90f")]
fn slant_above_its_range_panics() {
    FontVariation::slant(91.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Wdth_sets
#[test]
fn width_sets_its_axis() {
    let setting = FontVariation::width(50.0);
    assert_eq!(setting.axis_name(), "wdth");
    assert_eq!(value_of(&setting), 50.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Wdth_sets_atMaxSize
#[test]
fn a_large_width_is_a_width() {
    assert_eq!(value_of(&FontVariation::width(1000.0)), 1000.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Wdth_throws_whenTooSmall
#[test]
#[should_panic(expected = "'wdth' must be strictly > 0.0f")]
fn no_width_panics() {
    FontVariation::width(0.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Wght_sets
#[test]
fn weight_sets_its_axis() {
    let setting = FontVariation::weight(400);
    assert_eq!(setting.axis_name(), "wght");
    assert_eq!(value_of(&setting), 400.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Wght_throws_whenTooSmall
#[test]
#[should_panic(expected = "'wght' value must be in [1, 1000]")]
fn weight_below_its_range_panics() {
    FontVariation::weight(0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, Wght_throws_whenTooBig
#[test]
#[should_panic(expected = "'wght' value must be in [1, 1000]")]
fn weight_above_its_range_panics() {
    FontVariation::weight(1001);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, grad_sets
#[test]
fn grade_sets_its_axis() {
    let setting = FontVariation::grade(500);
    assert_eq!(setting.axis_name(), "GRAD");
    assert_eq!(value_of(&setting), 500.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, grad_throws_whenTooSmall
#[test]
#[should_panic(expected = "'GRAD' must be in -1000..1000")]
fn grade_below_its_range_panics() {
    FontVariation::grade(-1001);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, grad_throws_whenTooBig
#[test]
#[should_panic(expected = "'GRAD' must be in -1000..1000")]
fn grade_above_its_range_panics() {
    FontVariation::grade(1001);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, setting_makesSetting
#[test]
fn a_setting_of_any_axis() {
    let setting = FontVariation::setting("1234", 8.9);
    assert_eq!(setting.axis_name(), "1234");
    assert_eq!(value_of(&setting), 8.9);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, setting_throws_whenBadName
#[test]
#[should_panic(expected = "Name must be exactly four characters")]
fn a_setting_of_a_long_name_panics() {
    FontVariation::setting("Weight", 500.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, canExtend
#[test]
fn an_application_can_add_its_own_axis() {
    fn fizzable(fizz: i32) -> FontVariationSetting {
        assert!((1..=11).contains(&fizz), "'fzzt' must be in 1..11");
        FontVariation::setting("fzzt", fizz as f32)
    }
    let setting = fizzable(7);
    assert_eq!(setting.axis_name(), "fzzt");
    assert_eq!(value_of(&setting), 7.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_empty_is_empty
#[test]
fn the_empty_settings_are_empty() {
    let settings = FontVariation::empty();
    assert!(settings.settings().is_empty());
    assert!(!settings.needs_density());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_with_settings
#[test]
fn settings_hold_their_settings() {
    let (first, second) = (FontVariation::weight(400), FontVariation::width(100.0));
    let settings = FontVariationSettings::new([first.clone(), second.clone()]);
    assert_eq!(settings.settings(), &[first, second]);
    assert!(!settings.needs_density());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_with_settings_needsDensity
#[test]
fn settings_with_optical_sizing_need_a_density() {
    let settings = FontVariationSettings::new([
        FontVariation::weight(400),
        FontVariation::optical_sizing(12.sp()),
    ]);
    assert!(settings.needs_density());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_throws_on_duplicate
#[test]
#[should_panic(expected = "'wght' must be unique")]
fn settings_of_one_axis_twice_panic() {
    FontVariationSettings::new([FontVariation::weight(400), FontVariation::weight(700)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_throws_on_duplicate_multiple
#[test]
#[should_panic(expected = "'wght' must be unique")]
fn settings_of_one_axis_twice_among_others_panic() {
    FontVariationSettings::new([
        FontVariation::weight(400),
        FontVariation::width(100.0),
        FontVariation::italic(0.5),
        FontVariation::grade(1),
        FontVariation::weight(700),
    ]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_merge_other_settings
#[test]
fn settings_merge_with_others_overriding_in_place() {
    let base =
        FontVariationSettings::new([FontVariation::weight(400), FontVariation::width(100.0)]);
    let overrides =
        FontVariationSettings::new([FontVariation::weight(700), FontVariation::italic(0.5)]);
    assert_eq!(
        base.merge(Some(&overrides)).settings(),
        &[FontVariation::weight(700), FontVariation::width(100.0), FontVariation::italic(0.5)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_merge_other_settings_empty
#[test]
fn merging_with_empty_settings_gives_the_others() {
    let base = FontVariationSettings::new([FontVariation::weight(400)]);
    assert!(base.merge(Some(&FontVariation::empty())).ptr_eq(&base));
    assert!(FontVariation::empty().merge(Some(&base)).ptr_eq(&base));
    assert!(base.merge(None).ptr_eq(&base));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_merge_vararg_settings
#[test]
fn settings_merge_with_a_list_overriding_in_place() {
    let base =
        FontVariationSettings::new([FontVariation::weight(400), FontVariation::width(100.0)]);
    let merged = base.merge_settings([FontVariation::weight(700), FontVariation::italic(0.5)]);
    assert_eq!(
        merged.settings(),
        &[FontVariation::weight(700), FontVariation::width(100.0), FontVariation::italic(0.5)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontVariationTest.kt, settings_merge_vararg_settings_throws_on_duplicate
#[test]
#[should_panic(expected = "'wdth' must be unique")]
fn settings_merging_a_list_of_one_axis_twice_panic() {
    FontVariationSettings::new([FontVariation::weight(400)])
        .merge_settings([FontVariation::width(100.0), FontVariation::width(200.0)]);
}

#[test]
fn settings_of_a_weight_and_a_style() {
    let settings =
        FontVariation::settings(FontWeight::W700, FontStyle::Italic, [FontVariation::grade(1)]);
    assert_eq!(
        settings.settings(),
        &[FontVariation::weight(700), FontVariation::italic(1.0), FontVariation::grade(1)]
    );
}

// ---- FontMatcherTest --------------------------------------------------------------------------

/// A font of `weight` and `style`, `index` telling a family's fallbacks apart, as Compose's
/// `FontTestData`.
fn font_of(weight: u16, style: FontStyle, fallback: bool) -> Font {
    Font::from_bytes(
        if fallback { FALLBACK_RESOURCE } else { RESOURCE },
        FontOptions { weight: FontWeight::new(weight), style, ..FontOptions::default() },
    )
}

fn regular(weight: u16) -> Font {
    font_of(weight, FontStyle::Normal, false)
}

fn regular_fallback(weight: u16) -> Font {
    font_of(weight, FontStyle::Normal, true)
}

fn italic(weight: u16) -> Font {
    font_of(weight, FontStyle::Italic, false)
}

fn italic_fallback(weight: u16) -> Font {
    font_of(weight, FontStyle::Italic, true)
}

/// What a query of `weight` and `style` matches in `fonts`.
fn matched(fonts: &[Font], weight: u16, style: FontStyle) -> Vec<Font> {
    FontMatcher::match_font(fonts, FontWeight::new(weight), style)
}

fn italics(fonts: &[Font], weight: u16) -> Vec<Font> {
    matched(fonts, weight, FontStyle::Italic)
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, family with single italic font matches
#[test]
fn a_single_italic_font_matches() {
    assert_eq!(italics(&[italic(100)], 100), [italic(100)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, family with fallback italic font matches
#[test]
fn an_italic_font_and_its_fallback_match() {
    assert_eq!(
        italics(&[italic(100), italic_fallback(100)], 100),
        [italic(100), italic_fallback(100)]
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, family with single normal font matches
#[test]
fn a_single_regular_font_matches() {
    assert_eq!(matched(&[regular(100)], 100, FontStyle::Normal), [regular(100)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic query against family with multiple fonts matches
#[test]
fn an_italic_query_finds_its_font_among_others() {
    let family = [regular(100), italic(100), regular(200), italic(200)];
    assert_eq!(italics(&family, 200), [italic(200)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic query against family with multiple fonts and fallback matches
#[test]
fn an_italic_query_finds_its_font_and_fallback_among_others() {
    let family = [regular(100), italic(100), regular(200), italic(200), italic_fallback(200)];
    assert_eq!(italics(&family, 200), [italic(200), italic_fallback(200)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, normal style query against family with multiple fonts matches
#[test]
fn a_regular_query_finds_its_font_among_others() {
    let family = [regular(100), italic(200), regular(200), regular(300)];
    assert_eq!(matched(&family, 200, FontStyle::Normal), [regular(200)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style below 400 with no exact result matches a smaller italic value
#[test]
fn below_400_a_smaller_italic_comes_first() {
    let family = [regular(100), italic(100), regular(300), regular(400), italic(400)];
    assert_eq!(italics(&family, 300), [italic(100)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style below 400 with no exact result matches a smaller italic and fallback
#[test]
fn below_400_a_smaller_italic_and_its_fallback_come_first() {
    let family =
        [regular(100), italic(100), italic_fallback(100), regular(300), regular(400), italic(400)];
    assert_eq!(italics(&family, 300), [italic(100), italic_fallback(100)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style below 400 with no smaller weight matches larger weight and style
#[test]
fn below_400_without_a_smaller_italic_a_larger_one_matches() {
    let family = [regular(100), regular(200), regular(300), regular(400), italic(400)];
    assert_eq!(italics(&family, 200), [italic(400)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style below 400 with no smaller weight matches larger weight and style fallback
#[test]
fn below_400_without_a_smaller_italic_a_larger_one_and_its_fallback_match() {
    let family =
        [regular(100), regular(200), regular(300), regular(400), italic(400), italic_fallback(400)];
    assert_eq!(italics(&family, 200), [italic(400), italic_fallback(400)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style above 500 with no match returns a larger weight
#[test]
fn above_500_a_larger_italic_comes_first() {
    let family = [
        italic(100),
        italic(300),
        italic(400),
        regular(600),
        regular(700),
        regular(800),
        italic(800),
    ];
    assert_eq!(italics(&family, 600), [italic(800)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style above 500 with no match returns a larger weight and fallback
#[test]
fn above_500_a_larger_italic_and_its_fallback_come_first() {
    let family = [
        italic(100),
        italic(300),
        italic(400),
        regular(600),
        regular(700),
        regular(800),
        italic(800),
        italic_fallback(800),
    ];
    assert_eq!(italics(&family, 600), [italic(800), italic_fallback(800)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style above 500 with no greater weight matches a smaller weight
#[test]
fn above_500_without_a_larger_italic_a_smaller_one_matches() {
    let family = [italic(300), regular(400), italic(400), regular(500), regular(600), regular(700)];
    assert_eq!(italics(&family, 600), [italic(400)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style above 500 with no greater weight matches a smaller weight and fallback
#[test]
fn above_500_without_a_larger_italic_a_smaller_one_and_its_fallback_match() {
    let family = [
        italic(300),
        regular(400),
        regular_fallback(400),
        italic(400),
        italic_fallback(400),
        regular(500),
        regular(600),
        regular(700),
    ];
    assert_eq!(italics(&family, 600), [italic(400), italic_fallback(400)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 400 first check if 500 italic is there
#[test]
fn at_400_a_500_italic_comes_first() {
    let family = [italic(300), regular(400), regular(500), italic(500), italic(600)];
    assert_eq!(italics(&family, 400), [italic(500)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 400 first check if 500 italic is there and fallback
#[test]
fn at_400_a_500_italic_and_its_fallback_come_first() {
    let family =
        [italic(300), regular(400), regular(500), italic(500), italic(600), italic_fallback(500)];
    assert_eq!(italics(&family, 400), [italic(500), italic_fallback(500)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 400 returns a smaller italic if 500 is not there
#[test]
fn at_400_without_a_500_italic_a_smaller_one_matches() {
    assert_eq!(italics(&[italic(300), regular(500), italic(600)], 400), [italic(300)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 400 returns a smaller italic if 500 is not there and fallback
#[test]
fn at_400_without_a_500_italic_a_smaller_one_and_its_fallback_match() {
    let family = [italic(300), regular(500), italic(600), italic_fallback(300)];
    assert_eq!(italics(&family, 400), [italic(300), italic_fallback(300)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 500 returns a smaller italic if exists
#[test]
fn at_500_a_smaller_italic_matches() {
    assert_eq!(italics(&[italic(300), regular(500), italic(600)], 500), [italic(300)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 500 returns a smaller italic and fallback if exists
#[test]
fn at_500_a_smaller_italic_and_its_fallback_match() {
    let family = [italic(300), regular(500), italic(600), italic_fallback(300)];
    assert_eq!(italics(&family, 500), [italic(300), italic_fallback(300)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 400 returns a larger italic if there is no 500 or smaller italic
#[test]
fn at_400_with_no_500_or_smaller_italic_a_larger_one_matches() {
    let family = [regular(300), regular(400), regular(500), italic(600)];
    assert_eq!(italics(&family, 400), [italic(600)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 400 returns a larger italic and fallback when no 500 or smaller italic
#[test]
fn at_400_with_no_500_or_smaller_italic_a_larger_one_and_its_fallback_match() {
    let family = [regular(300), regular(400), regular(500), italic(600), italic_fallback(600)];
    assert_eq!(italics(&family, 400), [italic(600), italic_fallback(600)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 500 returns a larger italic if there is no smaller italic
#[test]
fn at_500_with_no_smaller_italic_a_larger_one_matches() {
    let family = [regular(300), regular(400), regular(500), italic(600)];
    assert_eq!(italics(&family, 500), [italic(600)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 500 returns a larger italic if there is no smaller italic and fallback
#[test]
fn at_500_with_no_smaller_italic_a_larger_one_and_its_fallback_match() {
    let family = [regular(300), regular(400), regular(500), italic(600), italic_fallback(600)];
    assert_eq!(italics(&family, 500), [italic(600), italic_fallback(600)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style returns another italic if exists
#[test]
fn another_italic_comes_before_the_same_weight_regular() {
    let family = [regular(100), regular(200), regular(400), regular(500), italic(600)];
    assert_eq!(italics(&family, 100), [italic(600)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style returns another italic and fallback if exists
#[test]
fn another_italic_and_its_fallback_come_before_the_same_weight_regular() {
    let family =
        [regular(100), regular(200), regular(400), regular(500), italic(600), italic_fallback(600)];
    assert_eq!(italics(&family, 100), [italic(600), italic_fallback(600)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style returns another italic if exists even when smaller
#[test]
fn another_italic_comes_first_even_when_smaller() {
    let family = [italic(100), regular(200), regular(400), regular(500), regular(600)];
    assert_eq!(italics(&family, 600), [italic(100)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style returns another italic and fallback if exists even when smaller
#[test]
fn another_italic_and_its_fallback_come_first_even_when_smaller() {
    let family =
        [italic(100), regular(200), regular(400), regular(500), regular(600), italic_fallback(100)];
    assert_eq!(italics(&family, 600), [italic(100), italic_fallback(100)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style returns same weight regular if no other italic exists
#[test]
fn without_an_italic_the_same_weight_regular_matches() {
    assert_eq!(italics(&[regular(200), regular(300), regular(400)], 300), [regular(300)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style returns same weight regular and fallback if no other italic exists
#[test]
fn without_an_italic_the_same_weight_regular_and_its_fallback_match() {
    let family = [regular(200), regular(300), regular_fallback(300), regular(400)];
    assert_eq!(italics(&family, 300), [regular(300), regular_fallback(300)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 600 returns larger weight regular if no other italic exists
#[test]
fn at_600_without_an_italic_the_same_weight_regular_matches() {
    let family = [regular(400), regular(500), regular(600), regular(700)];
    assert_eq!(italics(&family, 600), [regular(600)]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, italic style at 600 returns larger weight regular and fallback if no other italic
#[test]
fn at_600_without_an_italic_the_same_weight_regular_and_its_fallback_match() {
    let family = [regular(400), regular(500), regular(600), regular(700), regular_fallback(600)];
    assert_eq!(italics(&family, 600), [regular(600), regular_fallback(600)]);
}

/// Two regular fonts of each weight in `weights`.
fn pairs(weights: impl IntoIterator<Item = u16>) -> Vec<Font> {
    weights.into_iter().flat_map(|weight| [regular(weight), regular_fallback(weight)]).collect()
}

/// The weights `filter_by_closest_weight` leaves of `fonts`, asking 417, each once per pair.
fn closest(fonts: &[Font], prefer_below: bool, min: Option<u16>, max: Option<u16>) -> Vec<u16> {
    let found = FontMatcher::filter_by_closest_weight(
        fonts,
        FontWeight::new(417),
        prefer_below,
        min.map(FontWeight::new),
        max.map(FontWeight::new),
    );
    found
        .chunks(2)
        .map(|pair| {
            assert_eq!(pair.len(), 2, "both fonts of a weight");
            assert_eq!(pair[0].weight(), pair[1].weight());
            pair[0].weight().weight()
        })
        .collect()
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_onlyChoosesExactWeight_whenPreferBelow
#[test]
fn the_exact_weight_is_chosen_when_preferring_below() {
    assert_eq!(closest(&pairs(400..=500), true, None, None), [417]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_onlyChoosesExactWeight_whenPreferAbove
#[test]
fn the_exact_weight_is_chosen_when_preferring_above() {
    assert_eq!(closest(&pairs(400..=500), false, None, None), [417]);
}

fn without_417() -> Vec<Font> {
    pairs((400..=416).chain(418..=500))
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_onlyChoosesClosestAbove_whenPreferAbove
#[test]
fn the_closest_above_is_chosen_when_preferring_above() {
    assert_eq!(closest(&without_417(), false, None, None), [418]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_onlyChoosesClosestBelow_whenNoneAbove_whenPreferAbove
#[test]
fn the_closest_below_is_chosen_when_preferring_above_with_none_above() {
    assert_eq!(closest(&pairs(400..=416), false, None, None), [416]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_onlyChoosesClosestBelow_whenPreferBelow
#[test]
fn the_closest_below_is_chosen_when_preferring_below() {
    assert_eq!(closest(&without_417(), true, None, None), [416]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_onlyChoosesClosestAbove_whenNoneBelow_whenPreferBelow
#[test]
fn the_closest_above_is_chosen_when_preferring_below_with_none_below() {
    assert_eq!(closest(&pairs(418..=500), true, None, None), [418]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_respectsMinSearchRange
#[test]
fn a_minimum_of_the_search_is_kept() {
    assert_eq!(closest(&pairs(400..=500), true, Some(450), None), [450]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_respectsMaxSearchRange
#[test]
fn a_maximum_of_the_search_is_kept() {
    assert_eq!(closest(&pairs(400..=500), true, None, Some(401)), [401]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_respectsMinAndMaxSearchRange_evenIfConflicting
#[test]
fn a_search_range_that_contradicts_itself_finds_nothing() {
    assert!(closest(&pairs(100..1000), true, Some(500), Some(400)).is_empty());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_respectsMinAndMaxSearchRangeAbove
#[test]
fn a_search_range_above_the_weight_finds_it() {
    assert_eq!(closest(&pairs(100..1000), true, Some(900), Some(900)), [900]);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontMatcherTest.kt, filterByClosestWeight_respectsMinAndMaxSearchRange_whenBelow
#[test]
fn a_search_range_below_the_weight_finds_it() {
    assert_eq!(closest(&pairs(100..1000), true, Some(100), Some(100)), [100]);
}
