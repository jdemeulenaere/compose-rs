//! `TextStyle`'s layout-affecting attributes, defaults and parts, adapted from Compose's
//! `TextStyleLayoutAttributesTest`, `TextStyleResolveDefaultsTest` and
//! `TextSpanParagraphStyleTest`.

use compose_ui::{
    BaselineShift, Brush, Color, DrawStyle, FontFamily, FontStyle, FontSynthesis, FontWeight,
    Hyphens, LayoutDirection, LineBreak, LineHeightAlignment, LineHeightStyle, LineHeightTrim,
    LocaleList, ParagraphStyle, PlatformTextStyle, Shadow, SpanStyle, TextAlign, TextDecoration,
    TextDirection, TextGeometricTransform, TextIndent, TextMotion, TextStyle, TextUnit, UnitExt,
};

fn empty() -> TextStyle {
    TextStyle::DEFAULT
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_the_same_instance
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_the_equal_instance
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_color_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_color_to_brush_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_brush_to_color_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_brush_solid_color_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_brush_shader_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_brush_alpha_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_shadow_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_true_for_textDecoration_change
#[test]
fn what_only_paints_keeps_the_layout() {
    let style = TextStyle { line_height: 1.em(), ..empty() };
    assert!(style.has_same_layout_affecting_attributes(&style));
    assert!(style.has_same_layout_affecting_attributes(&style.clone()));
    let red = TextStyle { color: Color::RED, ..empty() };
    let green = TextStyle { color: Color::GREEN, ..empty() };
    let solid = |color| TextStyle { brush: Some(Brush::solid(color)), ..empty() };
    let gradient =
        |colors: [Color; 2]| TextStyle { brush: Some(Brush::linear_gradient(colors)), ..empty() };
    let pairs = [
        (red.clone(), green),
        (red.clone(), solid(Color::GREEN)),
        (solid(Color::GREEN), red),
        (solid(Color::RED), solid(Color::GREEN)),
        (gradient([Color::BLACK, Color::WHITE]), gradient([Color::RED, Color::BLUE])),
        (
            TextStyle { alpha: 0.5, ..gradient([Color::BLACK, Color::WHITE]) },
            TextStyle { alpha: 0.7, ..gradient([Color::BLACK, Color::WHITE]) },
        ),
        (
            TextStyle { shadow: Some(Shadow { color: Color::RED, ..Shadow::NONE }), ..empty() },
            TextStyle { shadow: Some(Shadow { color: Color::GREEN, ..Shadow::NONE }), ..empty() },
        ),
        (
            TextStyle { text_decoration: Some(TextDecoration::LINE_THROUGH), ..empty() },
            TextStyle { text_decoration: Some(TextDecoration::UNDERLINE), ..empty() },
        ),
    ];
    for (a, b) in pairs {
        assert!(a.has_same_layout_affecting_attributes(&b), "{a:?} and {b:?}");
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_background_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_fontSize_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_fontStyle_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_fontSynthesis_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_fontFamily_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_fontFeatureSettings_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_letterSpacing_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_baselineShift_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_textGeometricTransform_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_localeList_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_textAlign_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_textDirection_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_lineHeight_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_textIndent_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_platformStyle_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_color_and_textAlign_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_lineHeightStyle_change
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, returns_false_for_lineBreak_change
#[test]
fn what_lays_out_changes_the_layout() {
    let transform = |scale_x| TextGeometricTransform { scale_x, ..TextGeometricTransform::NONE };
    let indent = |first_line| TextIndent { first_line, ..TextIndent::NONE };
    let pairs = [
        (
            TextStyle { background: Color::RED, ..empty() },
            TextStyle { background: Color::GREEN, ..empty() },
        ),
        (TextStyle { font_size: 10.sp(), ..empty() }, TextStyle { font_size: 11.sp(), ..empty() }),
        (
            TextStyle { font_style: Some(FontStyle::Italic), ..empty() },
            TextStyle { font_style: Some(FontStyle::Normal), ..empty() },
        ),
        (
            TextStyle { font_synthesis: Some(FontSynthesis::Style), ..empty() },
            TextStyle { font_synthesis: Some(FontSynthesis::Weight), ..empty() },
        ),
        (
            TextStyle { font_family: Some(FontFamily::SansSerif), ..empty() },
            TextStyle { font_family: Some(FontFamily::Serif), ..empty() },
        ),
        (
            TextStyle { font_feature_settings: Some("abc".into()), ..empty() },
            TextStyle { font_feature_settings: Some("def".into()), ..empty() },
        ),
        (
            TextStyle { letter_spacing: 0.2.sp(), ..empty() },
            TextStyle { letter_spacing: 0.3.sp(), ..empty() },
        ),
        (
            TextStyle { baseline_shift: Some(BaselineShift::SUPERSCRIPT), ..empty() },
            TextStyle { baseline_shift: Some(BaselineShift::SUBSCRIPT), ..empty() },
        ),
        (
            TextStyle { text_geometric_transform: Some(transform(1.0)), ..empty() },
            TextStyle { text_geometric_transform: Some(transform(2.0)), ..empty() },
        ),
        (
            TextStyle { locale_list: Some(LocaleList::from_language_tags("en-US")), ..empty() },
            TextStyle { locale_list: Some(LocaleList::from_language_tags("en-CA")), ..empty() },
        ),
        (
            TextStyle { text_align: Some(TextAlign::Start), ..empty() },
            TextStyle { text_align: Some(TextAlign::End), ..empty() },
        ),
        (
            TextStyle { text_direction: Some(TextDirection::Ltr), ..empty() },
            TextStyle { text_direction: Some(TextDirection::Rtl), ..empty() },
        ),
        (
            TextStyle { line_height: 1.em(), ..empty() },
            TextStyle { line_height: 1.1.em(), ..empty() },
        ),
        (
            TextStyle { text_indent: Some(indent(0.sp())), ..empty() },
            TextStyle { text_indent: Some(indent(1.sp())), ..empty() },
        ),
        (
            TextStyle {
                platform_style: Some(PlatformTextStyle::with_include_font_padding(false)),
                ..empty()
            },
            TextStyle {
                platform_style: Some(PlatformTextStyle::with_include_font_padding(true)),
                ..empty()
            },
        ),
        (
            TextStyle { color: Color::RED, text_align: Some(TextAlign::Start), ..empty() },
            TextStyle { color: Color::BLUE, text_align: Some(TextAlign::End), ..empty() },
        ),
        (
            TextStyle {
                line_height_style: Some(LineHeightStyle::new(
                    LineHeightAlignment::CENTER,
                    LineHeightTrim::None,
                )),
                ..empty()
            },
            TextStyle {
                line_height_style: Some(LineHeightStyle::new(
                    LineHeightAlignment::BOTTOM,
                    LineHeightTrim::Both,
                )),
                ..empty()
            },
        ),
        (
            TextStyle { line_break: Some(LineBreak::HEADING), ..empty() },
            TextStyle { line_break: Some(LineBreak::PARAGRAPH), ..empty() },
        ),
    ];
    for (a, b) in pairs {
        assert!(!a.has_same_layout_affecting_attributes(&b), "{a:?} and {b:?}");
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleLayoutAttributesTest.kt, should_be_updated_when_a_new_attribute_is_added_to_TextStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextSpanParagraphStyleTest.kt, spanStyle_constructor_is_covered_by_TextStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextSpanParagraphStyleTest.kt, spanStyle_properties_is_covered_by_TextStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextSpanParagraphStyleTest.kt, paragraphStyle_is_covered_by_TextStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextSpanParagraphStyleTest.kt, paragraphStyle_properties_is_covered_by_TextStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextSpanParagraphStyleTest.kt, textStyle_covered_by_ParagraphStyle_and_SpanStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextSpanParagraphStyleTest.kt, testStyle_properties_is_covered_by_ParagraphStyle_and_SpanStyle
#[test]
fn a_text_style_is_exactly_a_span_style_and_a_paragraph_style() {
    // Compose checks the classes' properties by reflection; here the patterns below name every
    // field (no `..`), so a field added to any of the three styles fails to compile until it's
    // mapped (and `has_same_layout_affecting_attributes` reviewed, as Compose's test asks).
    let span = SpanStyle { color: Color::RED, font_size: 3.sp(), ..SpanStyle::DEFAULT };
    let paragraph = ParagraphStyle { text_align: Some(TextAlign::End), ..ParagraphStyle::DEFAULT };
    let style = TextStyle::from_styles(span.clone(), paragraph);
    let TextStyle {
        color,
        brush,
        alpha,
        font_size,
        font_weight,
        font_style,
        font_synthesis,
        font_family,
        font_feature_settings,
        letter_spacing,
        baseline_shift,
        text_geometric_transform,
        locale_list,
        background,
        text_decoration,
        shadow,
        draw_style,
        text_align,
        text_direction,
        line_height,
        text_indent,
        platform_style,
        line_height_style,
        line_break,
        hyphens,
        text_motion,
    } = style.clone();
    let rebuilt_span = SpanStyle {
        color,
        brush,
        alpha,
        font_size,
        font_weight,
        font_style,
        font_synthesis,
        font_family,
        font_feature_settings,
        letter_spacing,
        baseline_shift,
        text_geometric_transform,
        locale_list,
        background,
        text_decoration,
        shadow,
        platform_style: platform_style.and_then(|style| style.span_style),
        draw_style,
    };
    let rebuilt_paragraph = ParagraphStyle {
        text_align,
        text_direction,
        line_height,
        text_indent,
        platform_style: platform_style.and_then(|style| style.paragraph_style),
        line_height_style,
        line_break,
        hyphens,
        text_motion,
    };
    assert_eq!(rebuilt_span, span);
    assert_eq!(rebuilt_paragraph, paragraph);
    assert_eq!(style.to_span_style(), span);
    assert_eq!(style.to_paragraph_style(), paragraph);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_default_values
#[test]
fn resolving_defaults_fills_every_unset_field() {
    let resolved = TextStyle::resolve_defaults(&empty(), LayoutDirection::Ltr);
    assert_eq!(resolved.brush, None);
    assert_eq!(resolved.color, Color::BLACK);
    assert_eq!(resolved.font_size, 14.sp());
    assert_eq!(resolved.font_weight, Some(FontWeight::NORMAL));
    assert_eq!(resolved.font_style, Some(FontStyle::Normal));
    assert_eq!(resolved.font_synthesis, Some(FontSynthesis::All));
    // Compose's `FontFamily.Default` is no family here.
    assert_eq!(resolved.font_family, None);
    assert_eq!(resolved.font_feature_settings.as_deref(), Some(""));
    assert_eq!(resolved.letter_spacing, 0.sp());
    assert_eq!(resolved.baseline_shift, Some(BaselineShift::NONE));
    assert_eq!(resolved.text_geometric_transform, Some(TextGeometricTransform::NONE));
    assert_eq!(resolved.locale_list, None);
    assert_eq!(resolved.background, Color::TRANSPARENT);
    assert_eq!(resolved.text_decoration, Some(TextDecoration::NONE));
    assert_eq!(resolved.shadow, Some(Shadow::NONE));
    assert_eq!(resolved.text_align, Some(TextAlign::Start));
    assert_eq!(resolved.text_direction, Some(TextDirection::Ltr));
    assert_eq!(resolved.line_height, TextUnit::UNSPECIFIED);
    assert_eq!(resolved.text_indent, Some(TextIndent::NONE));
    assert_eq!(resolved.platform_style, None);
    assert_eq!(resolved.hyphens, Some(Hyphens::None));
    assert_eq!(resolved.line_break, Some(LineBreak::SIMPLE));
    assert_eq!(resolved.draw_style, Some(DrawStyle::Fill));
    assert_eq!(resolved.text_motion, Some(TextMotion::Static));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_shader_brush_color_unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_fontSize
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_fontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_fontStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_fontFamily
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_fontFeatureSettings
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_letterSpacing
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_baselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_textGeometricTransform
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_localeList
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_background
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_shadow
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_textAlign
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_lineHeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_textIndent
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_lineBreak
#[test]
fn resolving_defaults_keeps_what_is_set() {
    let resolve = |style: TextStyle| TextStyle::resolve_defaults(&style, LayoutDirection::Ltr);
    let gradient = Brush::linear_gradient([Color::WHITE, Color::BLACK]);
    let resolved = resolve(TextStyle { brush: Some(gradient.clone()), ..empty() });
    assert_eq!((resolved.brush, resolved.color), (Some(gradient), Color::UNSPECIFIED));
    assert_eq!(
        resolve(TextStyle { hyphens: Some(Hyphens::Auto), ..empty() }).hyphens,
        Some(Hyphens::Auto)
    );
    assert_eq!(resolve(TextStyle { color: Color::RED, ..empty() }).color, Color::RED);
    assert_eq!(resolve(TextStyle { font_size: 28.sp(), ..empty() }).font_size, 28.sp());
    assert_eq!(
        resolve(TextStyle { font_weight: Some(FontWeight::W900), ..empty() }).font_weight,
        Some(FontWeight::W900)
    );
    assert_eq!(
        resolve(TextStyle { font_style: Some(FontStyle::Italic), ..empty() }).font_style,
        Some(FontStyle::Italic)
    );
    assert_eq!(
        resolve(TextStyle { font_synthesis: Some(FontSynthesis::Weight), ..empty() })
            .font_synthesis,
        Some(FontSynthesis::Weight)
    );
    assert_eq!(
        resolve(TextStyle { font_family: Some(FontFamily::Cursive), ..empty() }).font_family,
        Some(FontFamily::Cursive)
    );
    let features =
        resolve(TextStyle { font_feature_settings: Some("'liga': off".into()), ..empty() });
    assert_eq!(features.font_feature_settings.as_deref(), Some("'liga': off"));
    assert_eq!(resolve(TextStyle { letter_spacing: 1.2.em(), ..empty() }).letter_spacing, 1.2.em());
    assert_eq!(
        resolve(TextStyle { baseline_shift: Some(BaselineShift::SUPERSCRIPT), ..empty() })
            .baseline_shift,
        Some(BaselineShift::SUPERSCRIPT)
    );
    let transform = TextGeometricTransform { scale_x: 10.0, ..TextGeometricTransform::NONE };
    assert_eq!(
        resolve(TextStyle { text_geometric_transform: Some(transform), ..empty() })
            .text_geometric_transform,
        Some(transform)
    );
    let french = LocaleList::from_language_tags("fr-FR");
    assert_eq!(
        resolve(TextStyle { locale_list: Some(french.clone()), ..empty() }).locale_list,
        Some(french)
    );
    assert_eq!(resolve(TextStyle { background: Color::BLUE, ..empty() }).background, Color::BLUE);
    assert_eq!(
        resolve(TextStyle { text_decoration: Some(TextDecoration::LINE_THROUGH), ..empty() })
            .text_decoration,
        Some(TextDecoration::LINE_THROUGH)
    );
    let shadow = Shadow { color: Color::YELLOW, ..Shadow::NONE };
    assert_eq!(resolve(TextStyle { shadow: Some(shadow), ..empty() }).shadow, Some(shadow));
    assert_eq!(
        resolve(TextStyle { text_align: Some(TextAlign::Right), ..empty() }).text_align,
        Some(TextAlign::Right)
    );
    assert_eq!(resolve(TextStyle { line_height: 12.sp(), ..empty() }).line_height, 12.sp());
    let indent = TextIndent { first_line: 12.sp(), rest_line: 13.sp() };
    assert_eq!(
        resolve(TextStyle { text_indent: Some(indent), ..empty() }).text_indent,
        Some(indent)
    );
    assert_eq!(
        resolve(TextStyle { line_break: Some(LineBreak::HEADING), ..empty() }).line_break,
        Some(LineBreak::HEADING)
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_textDirection_with_LTR_layoutDirection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_use_provided_values_textDirection_with_RTL_layoutDirection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleResolveDefaultsTest.kt, test_default_direction_algorithm_with_provided_layoutDirection
#[test]
fn resolving_defaults_resolves_the_direction() {
    let resolve = |direction: Option<TextDirection>, layout| {
        TextStyle::resolve_defaults(&TextStyle { text_direction: direction, ..empty() }, layout)
            .text_direction
            .unwrap()
    };
    use LayoutDirection::{Ltr, Rtl};
    use TextDirection::{Content, ContentOrLtr, ContentOrRtl};
    assert_eq!(resolve(Some(Content), Ltr), ContentOrLtr);
    assert_eq!(resolve(Some(Content), Rtl), ContentOrRtl);
    for layout in [Ltr, Rtl] {
        for fixed in [TextDirection::Ltr, TextDirection::Rtl, ContentOrLtr, ContentOrRtl] {
            assert_eq!(resolve(Some(fixed), layout), fixed);
        }
    }
    assert_eq!(resolve(None, Ltr), TextDirection::Ltr);
    assert_eq!(resolve(None, Rtl), TextDirection::Rtl);
}
