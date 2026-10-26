//! `SpanStyle`, adapted from Compose's `SpanStyleTest`.

use compose_ui::{
    BaselineShift, Brush, Color, DrawStyle, FontFamily, FontStyle, FontSynthesis, FontWeight,
    LocaleList, Offset, Shadow, SpanStyle, Stroke, StrokeCap, TextDecoration,
    TextGeometricTransform, TextUnit, UnitExt,
};

fn gradient() -> Brush {
    Brush::linear_gradient([Color::BLUE, Color::RED])
}

fn empty() -> SpanStyle {
    SpanStyle::DEFAULT
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with default values
#[test]
fn a_default_span_style_sets_nothing() {
    let style = SpanStyle::default();
    assert_eq!(style.foreground().brush(), None);
    assert_eq!(style.foreground().color(), Color::UNSPECIFIED);
    assert!(style.font_size.is_unspecified());
    assert_eq!((style.font_weight, style.font_style), (None, None));
    assert!(style.letter_spacing.is_unspecified());
    assert_eq!(style.locale_list, None);
    assert_eq!(style.background, Color::UNSPECIFIED);
    assert_eq!(style.text_decoration, None);
    assert_eq!(style.font_family, None);
    assert_eq!(style.draw_style, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized brush and alpha
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with gradient brush has unspecified color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with SolidColor converts to regular color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with half-transparent color
#[test]
fn the_paint_is_the_brush_if_any_else_the_color() {
    let style = SpanStyle { brush: Some(gradient()), ..empty() };
    assert_eq!(style.foreground().brush(), Some(&gradient()));
    assert_eq!(style.foreground().color(), Color::UNSPECIFIED);
    let style = SpanStyle { brush: Some(gradient()), alpha: 0.3, ..empty() };
    assert_eq!(style.foreground().alpha(), 0.3);
    let style = SpanStyle { brush: Some(Brush::solid(Color::RED)), ..empty() };
    assert_eq!(style.foreground().color(), Color::RED);
    assert_eq!(style.foreground().brush(), None);
    let style = SpanStyle { color: Color::RED, ..empty() };
    assert_eq!(style.foreground().color(), Color::RED);
    let half = Color::RED.with_alpha(0.5);
    let style = SpanStyle { color: half, ..empty() };
    assert_eq!(style.foreground().color(), half);
    assert!((style.foreground().alpha() - 0.5).abs() < 1.0 / 256.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized fontSize
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized fontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized fontStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized letterSpacing
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized baselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized locale
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized background
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized fontFamily
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, constructor with customized drawStyle
#[test]
fn a_span_style_keeps_each_field_it_is_given() {
    let style = SpanStyle {
        font_size: 18.sp(),
        font_weight: Some(FontWeight::W500),
        font_style: Some(FontStyle::Italic),
        letter_spacing: 1.em(),
        baseline_shift: Some(BaselineShift::SUPERSCRIPT),
        locale_list: Some(LocaleList::from_language_tags("en-US")),
        background: Color::RED,
        text_decoration: Some(TextDecoration::UNDERLINE),
        font_family: Some(FontFamily::SansSerif),
        draw_style: Some(DrawStyle::Stroke(Stroke::new(4.0))),
        ..empty()
    };
    assert_eq!(style.font_size, 18.sp());
    assert_eq!(style.font_weight, Some(FontWeight::W500));
    assert_eq!(style.font_style, Some(FontStyle::Italic));
    assert_eq!(style.letter_spacing, 1.em());
    assert_eq!(style.baseline_shift, Some(BaselineShift::SUPERSCRIPT));
    assert_eq!(style.locale_list, Some(LocaleList::from_language_tags("en-US")));
    assert_eq!(style.background, Color::RED);
    assert_eq!(style.text_decoration, Some(TextDecoration::UNDERLINE));
    assert_eq!(style.font_family, Some(FontFamily::SansSerif));
    assert_eq!(style.draw_style, Some(DrawStyle::Stroke(Stroke::new(4.0))));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with empty other should return this
#[test]
fn merging_nothing_changes_nothing() {
    assert_eq!(empty().merge(&empty()), empty());
    let red = SpanStyle { color: Color::RED, font_size: 3.sp(), ..empty() };
    assert_eq!(red.merge(&empty()), red);
}

/// Merges `this` and `other` styles made by `with`: the merged field is `other`'s if set,
/// else `this`'s.
fn check_merge<T: Clone + PartialEq + std::fmt::Debug>(
    with: impl Fn(Option<T>) -> SpanStyle,
    get: impl Fn(&SpanStyle) -> T,
    this: T,
    other: T,
    unset_other: Option<T>,
) {
    let merged = with(Some(this.clone())).merge(&with(unset_other));
    assert_eq!(get(&merged), this, "an unset field keeps this one's");
    let merged = with(Some(this)).merge(&with(Some(other.clone())));
    assert_eq!(get(&merged), other, "a set field wins");
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's color is null should use this color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's color is set should use other's color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontFamily is unspecified should use this' fontFamily
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontFamily is set should use other's fontFamily
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontSize is null should use this' fontSize
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontSize is set should use other's fontSize
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontWeight is null should use this' fontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontWeight is set should use other's fontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontStyle is null should use this' fontStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontStyle is set should use other's fontStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontSynthesis is null should use this' fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontSynthesis is set should use other's fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontFeature is null should use this' fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's fontFeature is set should use other's fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's letterSpacing is unspecified should use this' letterSpacing
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's letterSpacing is set should use other's letterSpacing
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's baselineShift is null should use this' baselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's baselineShift is set should use other's baselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's background is null should use this' background
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's background is set should use other's background
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's textDecoration is null should use this' textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's textDecoration is set should use other's textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's locale is null should use this' locale
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's locale is set should use other's locale
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's drawStyle is null should use this' drawStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with other's drawStyle is set should use other's drawStyle
#[test]
fn merging_takes_each_field_from_other_when_set() {
    check_merge(
        |value| SpanStyle { color: value.unwrap_or(Color::UNSPECIFIED), ..empty() },
        |style| style.color,
        Color::RED,
        Color::GREEN,
        None,
    );
    check_merge(
        |value| SpanStyle { font_family: value, ..empty() },
        |style| style.font_family.unwrap(),
        FontFamily::SansSerif,
        FontFamily::Serif,
        None,
    );
    check_merge(
        |value| SpanStyle { font_size: value.unwrap_or(TextUnit::UNSPECIFIED), ..empty() },
        |style| style.font_size,
        3.5.sp(),
        8.7.sp(),
        None,
    );
    check_merge(
        |value| SpanStyle { font_weight: value, ..empty() },
        |style| style.font_weight.unwrap(),
        FontWeight::W300,
        FontWeight::W500,
        None,
    );
    check_merge(
        |value| SpanStyle { font_style: value, ..empty() },
        |style| style.font_style.unwrap(),
        FontStyle::Italic,
        FontStyle::Normal,
        None,
    );
    check_merge(
        |value| SpanStyle { font_synthesis: value, ..empty() },
        |style| style.font_synthesis.unwrap(),
        FontSynthesis::Style,
        FontSynthesis::Weight,
        None,
    );
    let features =
        |value: &'static str| SpanStyle { font_feature_settings: Some(value.into()), ..empty() };
    let merged = features("\"kern\" 0").merge(&empty());
    assert_eq!(merged.font_feature_settings.as_deref(), Some("\"kern\" 0"));
    let merged = features("\"kern\" 0").merge(&features("\"kern\" 1"));
    assert_eq!(merged.font_feature_settings.as_deref(), Some("\"kern\" 1"));
    check_merge(
        |value| SpanStyle { letter_spacing: value.unwrap_or(TextUnit::UNSPECIFIED), ..empty() },
        |style| style.letter_spacing,
        1.2.em(),
        1.5.em(),
        None,
    );
    check_merge(
        |value| SpanStyle { baseline_shift: value, ..empty() },
        |style| style.baseline_shift.unwrap(),
        BaselineShift::SUPERSCRIPT,
        BaselineShift::SUBSCRIPT,
        None,
    );
    check_merge(
        |value| SpanStyle { background: value.unwrap_or(Color::UNSPECIFIED), ..empty() },
        |style| style.background,
        Color::RED,
        Color::GREEN,
        None,
    );
    check_merge(
        |value| SpanStyle { text_decoration: value, ..empty() },
        |style| style.text_decoration.unwrap(),
        TextDecoration::LINE_THROUGH,
        TextDecoration::UNDERLINE,
        None,
    );
    check_merge(
        |value| SpanStyle { locale_list: value, ..empty() },
        |style| style.locale_list.clone().unwrap(),
        LocaleList::from_language_tags("en-US"),
        LocaleList::from_language_tags("ja-JP"),
        None,
    );
    check_merge(
        |value| SpanStyle { draw_style: value, ..empty() },
        |style| style.draw_style.unwrap(),
        DrawStyle::Stroke(Stroke { cap: StrokeCap::Butt, ..Stroke::default() }),
        DrawStyle::Fill,
        None,
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with null platformStyles has null platformStyle
#[test]
fn merging_two_styles_without_a_platform_style_has_none() {
    assert_eq!(empty().merge(&empty()).platform_style, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with brush has other brush and no color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge with unspecified brush has original brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge brush with brush uses other's alpha
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, merge brush with brush uses current alpha if other's is NaN
#[test]
fn merging_a_brush_wins_over_a_color_and_keeps_an_alpha() {
    let red = SpanStyle { color: Color::RED, ..empty() };
    let brushed = SpanStyle { brush: Some(gradient()), ..empty() };
    let merged = red.merge(&brushed);
    assert_eq!(merged.color, Color::UNSPECIFIED);
    assert_eq!(merged.brush, Some(gradient()));
    let merged = brushed.merge(&empty());
    assert_eq!((merged.color, merged.brush), (Color::UNSPECIFIED, Some(gradient())));
    let faint = SpanStyle { alpha: 0.3, ..brushed.clone() };
    let merged = faint.merge(&SpanStyle { alpha: 0.6, ..brushed.clone() });
    assert_eq!((merged.brush.clone(), merged.alpha), (Some(gradient()), 0.6));
    let merged = faint.merge(&brushed);
    assert_eq!((merged.brush, merged.alpha), (Some(gradient()), 0.3));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, plus operator merges
#[test]
fn plus_merges() {
    let style = SpanStyle { color: Color::RED, font_weight: Some(FontWeight::BOLD), ..empty() }
        + SpanStyle { color: Color::GREEN, font_family: Some(FontFamily::Cursive), ..empty() };
    assert_eq!(
        style,
        SpanStyle {
            color: Color::GREEN,
            font_weight: Some(FontWeight::BOLD),
            font_family: Some(FontFamily::Cursive),
            ..SpanStyle::DEFAULT
        }
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp color with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp color with a and b are unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, when lerp from Specified to Unspecified color, uses Color lerp logic
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp background with a and b are Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp background with a is Null and b is not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp background with a is Not Null and b is Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp background with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp background with a and b are not Null and t is larger than half
#[test]
fn lerping_colors_lerps_them_as_colors() {
    let lerped = |a: Color, b: Color, t: f32, background: bool| {
        let make = |color| {
            if background {
                SpanStyle { background: color, ..empty() }
            } else {
                SpanStyle { color, ..empty() }
            }
        };
        let style = SpanStyle::lerp(&make(a), &make(b), t);
        if background { style.background } else { style.foreground().color() }
    };
    for background in [false, true] {
        assert_eq!(
            lerped(Color::RED, Color::GREEN, 0.3, background),
            Color::lerp(Color::RED, Color::GREEN, 0.3)
        );
        assert_eq!(
            lerped(Color::UNSPECIFIED, Color::UNSPECIFIED, 0.3, background),
            Color::UNSPECIFIED
        );
        assert_eq!(
            lerped(Color::RED, Color::UNSPECIFIED, 0.3, background),
            Color::lerp(Color::RED, Color::UNSPECIFIED, 0.3)
        );
        assert_eq!(
            lerped(Color::UNSPECIFIED, Color::RED, 0.1, background),
            Color::lerp(Color::UNSPECIFIED, Color::RED, 0.1)
        );
        assert_eq!(
            lerped(Color::RED, Color::GREEN, 0.8, background),
            Color::lerp(Color::RED, Color::GREEN, 0.8)
        );
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontFamily with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontFamily with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontStyle with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontStyle with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontSynthesis with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontSynthesis with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontFeatureSettings with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontFeatureSettings with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp locale with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp locale with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp textDecoration with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp textDecoration with a and b are not Null and t is larger than half
#[test]
fn lerping_discrete_fields_switches_half_way() {
    let a = SpanStyle {
        font_family: Some(FontFamily::SansSerif),
        font_style: Some(FontStyle::Italic),
        font_synthesis: Some(FontSynthesis::Style),
        font_feature_settings: Some("\"kern\" 0".into()),
        locale_list: Some(LocaleList::from_language_tags("en-US")),
        text_decoration: Some(TextDecoration::LINE_THROUGH),
        ..empty()
    };
    let b = SpanStyle {
        font_family: Some(FontFamily::Serif),
        font_style: Some(FontStyle::Normal),
        font_synthesis: Some(FontSynthesis::Weight),
        font_feature_settings: Some("\"kern\" 1".into()),
        locale_list: Some(LocaleList::from_language_tags("ja-JP")),
        text_decoration: Some(TextDecoration::UNDERLINE),
        ..empty()
    };
    for (t, expected) in [(0.3, &a), (0.8, &b)] {
        let lerped = SpanStyle::lerp(&a, &b, t);
        assert_eq!(lerped.font_family, expected.font_family);
        assert_eq!(lerped.font_style, expected.font_style);
        assert_eq!(lerped.font_synthesis, expected.font_synthesis);
        assert_eq!(lerped.font_feature_settings, expected.font_feature_settings);
        assert_eq!(lerped.locale_list, expected.locale_list);
        assert_eq!(lerped.text_decoration, expected.text_decoration);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontSize with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp fontWeight with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp baselineShift with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp textGeometricTransform with a and b are not Null
#[test]
fn lerping_numeric_fields_interpolates_them() {
    let lerped = SpanStyle::lerp(
        &SpanStyle { font_size: 8.sp(), ..empty() },
        &SpanStyle { font_size: 16.sp(), ..empty() },
        0.8,
    );
    assert_eq!(lerped.font_size, 14.4.sp());
    let lerped = SpanStyle::lerp(
        &SpanStyle { font_weight: Some(FontWeight::W200), ..empty() },
        &SpanStyle { font_weight: Some(FontWeight::W500), ..empty() },
        0.8,
    );
    assert_eq!(lerped.font_weight, Some(FontWeight::lerp(FontWeight::W200, FontWeight::W500, 0.8)));
    let (shift_a, shift_b) = (BaselineShift(1.0), BaselineShift(2.0));
    let lerped = SpanStyle::lerp(
        &SpanStyle { baseline_shift: Some(shift_a), ..empty() },
        &SpanStyle { baseline_shift: Some(shift_b), ..empty() },
        0.3,
    );
    assert_eq!(lerped.baseline_shift, Some(BaselineShift::lerp(shift_a, shift_b, 0.3)));
    let transform_a = TextGeometricTransform { scale_x: 1.5, skew_x: 0.1 };
    let transform_b = TextGeometricTransform { scale_x: 1.0, skew_x: 0.3 };
    let lerped = SpanStyle::lerp(
        &SpanStyle { text_geometric_transform: Some(transform_a), ..empty() },
        &SpanStyle { text_geometric_transform: Some(transform_b), ..empty() },
        0.3,
    );
    assert_eq!(
        lerped.text_geometric_transform,
        Some(TextGeometricTransform::lerp(transform_a, transform_b, 0.3))
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp with null platformStyles have null platformStyle
#[test]
fn lerping_two_styles_without_a_platform_style_has_none() {
    assert_eq!(SpanStyle::lerp(&empty(), &empty(), 0.5).platform_style, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp brush with a specified, b specified and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp brush with a specified, b specified and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp brush with a specified, b not specified and t is larger than half
#[test]
fn lerping_a_brush_and_a_color_switches_half_way() {
    let brushed = SpanStyle { brush: Some(gradient()), ..empty() };
    let red = SpanStyle { color: Color::RED, ..empty() };
    let lerped = SpanStyle::lerp(&brushed, &red, 0.4);
    assert_eq!((lerped.brush, lerped.color), (Some(gradient()), Color::UNSPECIFIED));
    let lerped = SpanStyle::lerp(&brushed, &red, 0.6);
    assert_eq!((lerped.brush, lerped.color), (None, Color::RED));
    let lerped = SpanStyle::lerp(&brushed, &empty(), 0.6);
    assert_eq!((lerped.brush, lerped.color), (None, Color::UNSPECIFIED));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp_shadow_start_null_interpolates
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp_shadow_null_stop_interpolates
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/SpanStyleTest.kt, lerp_shadow_start_stop_interpolates
#[test]
fn lerping_a_missing_shadow_fades_it() {
    let start = Shadow { color: Color::RED, offset: Offset::new(10.0, 10.0), blur_radius: 4.0 };
    let stop = Shadow { color: Color::BLUE, offset: Offset::new(20.0, 20.0), blur_radius: 8.0 };
    let transparent = |shadow: Shadow| Shadow { color: shadow.color.with_alpha(0.0), ..shadow };
    let with = |shadow| SpanStyle { shadow, ..empty() };
    assert_eq!(
        SpanStyle::lerp(&with(Some(start)), &with(None), 0.5).shadow,
        Some(Shadow::lerp(start, transparent(start), 0.5))
    );
    assert_eq!(
        SpanStyle::lerp(&with(None), &with(Some(stop)), 0.5).shadow,
        Some(Shadow::lerp(transparent(stop), stop, 0.5))
    );
    assert_eq!(
        SpanStyle::lerp(&with(Some(start)), &with(Some(stop)), 0.5).shadow,
        Some(Shadow::lerp(start, stop, 0.5))
    );
}
